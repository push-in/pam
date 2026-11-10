use std::ffi::{OsStr, OsString};
use std::fs;
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
#[cfg(target_os = "linux")]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::admin_auth::{self, ADMIN_TOKEN_ENV, ADMIN_TOKEN_FILE_ENV};
use crate::control_plane::{ClusterSnapshot, ControlPlane, SharedClusterSnapshot};
use crate::ingress::{Ingress, Route as IngressRoute};
use crate::prefork::WorkerProcess;
use crate::worker_state::{
    WORKER_STATE_PATH_ENV, WorkerLifecycle, WorkerRuntimeRecord, WorkerStartupPhases, epoch_millis,
    read,
};

const SIGTERM: i32 = 15;
const SIGKILL: i32 = 9;
const DEFAULT_MAX_REQUESTS: u64 = 10_000_000;
const MAX_RESTART_BACKOFF: Duration = Duration::from_secs(30);
/// A worker answers a request stuck past its deadline with 504 by itself and
/// asks to be recycled; the master only kills a worker that failed to do so
/// within this allowance (plus `--watchdog-grace`), e.g. a fully hung process.
const WORKER_SELF_TIMEOUT_ALLOWANCE_MILLIS: u64 = 1_000;
static NEXT_INCARNATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}

#[derive(Clone, Debug)]
pub struct StartOptions {
    pub script: PathBuf,
    pub script_args: Vec<OsString>,
    pub workers: usize,
    pub max_requests: u64,
    pub graceful_timeout: Duration,
    pub startup_timeout: Duration,
    pub restart_backoff: Duration,
    pub watchdog_grace: Duration,
    pub admin_address: Option<SocketAddr>,
    pub admin_token_digest: Option<[u8; 32]>,
    pub state_file: Option<PathBuf>,
    pub ingress_address: Option<SocketAddr>,
    pub pools: Vec<PoolSpec>,
    pub php_extensions: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoolSpec {
    pub name: String,
    pub workers: usize,
    pub prefixes: Vec<String>,
}

impl StartOptions {
    pub fn parse(arguments: impl Iterator<Item = OsString>) -> Result<Self, String> {
        let mut arguments = arguments.peekable();
        let script = match arguments.peek() {
            Some(argument) if !argument.to_string_lossy().starts_with('-') => {
                PathBuf::from(arguments.next().expect("peeked argument must exist"))
            }
            _ => PathBuf::from("index.php"),
        };
        let mut workers = std::thread::available_parallelism().map_or(1, usize::from);
        let mut max_requests = DEFAULT_MAX_REQUESTS;
        let mut graceful_timeout = Duration::from_secs(15);
        let mut startup_timeout = Duration::from_secs(30);
        let mut restart_backoff = Duration::from_millis(100);
        let mut watchdog_grace = Duration::from_millis(250);
        let mut admin_address = None;
        let mut state_file = None;
        let mut ingress_address = None;
        let mut pools = Vec::new();
        let mut php_extensions = Vec::new();
        let mut isolate_php_extensions = false;
        let mut script_args = Vec::new();

        while let Some(argument) = arguments.next() {
            let text = argument.to_string_lossy();
            match text.as_ref() {
                "--workers" => {
                    workers = parse_positive(arguments.next(), "--workers")?;
                }
                "--max-requests" => {
                    max_requests = parse_positive(arguments.next(), "--max-requests")? as u64;
                }
                "--graceful-timeout" => {
                    let milliseconds = parse_positive(arguments.next(), "--graceful-timeout")?;
                    graceful_timeout = Duration::from_millis(milliseconds as u64);
                }
                "--startup-timeout" => {
                    let milliseconds = parse_positive(arguments.next(), "--startup-timeout")?;
                    startup_timeout = Duration::from_millis(milliseconds as u64);
                }
                "--restart-backoff" => {
                    let milliseconds = parse_positive(arguments.next(), "--restart-backoff")?;
                    restart_backoff = Duration::from_millis(milliseconds as u64);
                }
                "--watchdog-grace" => {
                    let milliseconds = parse_positive(arguments.next(), "--watchdog-grace")?;
                    watchdog_grace = Duration::from_millis(milliseconds as u64);
                }
                "--admin-address" => {
                    admin_address = Some(parse_address(arguments.next(), "--admin-address")?);
                }
                "--state-file" => {
                    state_file = Some(PathBuf::from(
                        arguments
                            .next()
                            .ok_or_else(|| "--state-file requires a path".to_owned())?,
                    ));
                }
                "--ingress-address" => {
                    ingress_address = Some(parse_address(arguments.next(), "--ingress-address")?);
                }
                "--pool" => {
                    pools.push(parse_pool(arguments.next())?);
                }
                "--php-extension" => {
                    isolate_php_extensions = true;
                    php_extensions.push(parse_php_extension(arguments.next())?);
                }
                "--isolate-php-extensions" => isolate_php_extensions = true,
                "--" => {
                    script_args.extend(arguments);
                    break;
                }
                _ if text.starts_with('-') => {
                    return Err(format!("unknown start option: {text}"));
                }
                _ => script_args.push(argument),
            }
        }

        if workers > 256 {
            return Err("--workers cannot exceed 256".to_owned());
        }
        if !pools.is_empty() {
            if ingress_address.is_none() {
                return Err("--pool requires --ingress-address".to_owned());
            }
            if pools
                .iter()
                .filter(|pool| pool.prefixes.iter().any(|prefix| prefix == "*"))
                .count()
                != 1
            {
                return Err("pool configuration requires exactly one '*' fallback".to_owned());
            }
            let total: usize = pools.iter().map(|pool| pool.workers).sum();
            if total > 256 {
                return Err("total pool workers cannot exceed 256".to_owned());
            }
            workers = total;
        }
        php_extensions.sort_unstable();
        php_extensions.dedup();
        if php_extensions.len() > 64 {
            return Err("--php-extension cannot select more than 64 extensions".to_owned());
        }
        let admin_token_digest = if admin_address.is_some() {
            admin_auth::load()?.map(|credential| credential.digest())
        } else {
            None
        };
        if admin_address.is_some_and(|address| !address.ip().is_loopback())
            && admin_token_digest.is_none()
        {
            return Err(format!(
                "non-loopback --admin-address requires {ADMIN_TOKEN_ENV} or {ADMIN_TOKEN_FILE_ENV} with a 32 to 256 character ASCII token"
            ));
        }

        Ok(Self {
            script,
            script_args,
            workers,
            max_requests,
            graceful_timeout,
            startup_timeout,
            restart_backoff,
            watchdog_grace,
            admin_address,
            admin_token_digest,
            state_file,
            ingress_address,
            pools,
            php_extensions: isolate_php_extensions.then_some(php_extensions),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterState {
    pub version: u8,
    pub pid: u32,
    pub process_start: Option<u64>,
    pub workers: usize,
    pub admin_address: Option<String>,
    pub started_at_millis: u64,
    #[serde(default)]
    pub worker_spawn_spread_millis: Option<u64>,
    #[serde(default)]
    pub worker_startup_p95_millis: Option<u64>,
    #[serde(default)]
    pub worker_startup_max_millis: Option<u64>,
    #[serde(default)]
    pub worker_startup_phase_p95_millis: Option<WorkerStartupPhases>,
}

pub fn read_master_state(path: &Path) -> Result<MasterState, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("cannot read PAM state {}: {error}", path.display()))?;
    let state: MasterState = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid PAM state {}: {error}", path.display()))?;
    if state.version != 1 {
        return Err(format!("unsupported PAM state version {}", state.version));
    }
    Ok(state)
}

pub fn master_is_running(state: &MasterState) -> bool {
    let pid = match i32::try_from(state.pid) {
        Ok(pid) => pid,
        Err(_) => return false,
    };
    // SAFETY: signal 0 performs a read-only process existence check.
    if unsafe { kill(pid, 0) } != 0 {
        return false;
    }
    if linux_process_state(state.pid) == Some('Z') {
        return false;
    }
    state
        .process_start
        .is_none_or(|expected| linux_process_start(state.pid) == Some(expected))
}

pub fn signal_master(state: &MasterState, signal: i32) -> Result<(), String> {
    if !master_is_running(state) {
        return Err(format!("PAM master {} is not running", state.pid));
    }
    let pid = i32::try_from(state.pid).map_err(|_| "PAM master PID is invalid".to_owned())?;
    // SAFETY: the PID and its process-start fingerprint were verified above.
    if unsafe { kill(pid, signal) } != 0 {
        return Err(format!(
            "cannot signal PAM master {}: {}",
            state.pid,
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

pub const RELOAD_SIGNAL: i32 = 1;
pub const STOP_SIGNAL: i32 = SIGTERM;

struct Worker {
    id: usize,
    generation: u64,
    process: WorkerProcess,
    state_path: PathBuf,
    spawned_at_millis: u64,
    started: Instant,
    consecutive_failures: u32,
    pool: Option<RuntimePool>,
    retry_replacement_after: Option<Instant>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.state_path);
    }
}

/// A replacement booting next to a worker that asked to be recycled. The old
/// worker keeps serving until the replacement reports ready (make-before-break).
struct Replacement {
    replaces: u32,
    worker: Worker,
}

/// A worker that was replaced and is draining its in-flight requests.
struct Retiring {
    worker: Worker,
    deadline: Instant,
}

#[derive(Clone, Debug)]
struct RuntimePool {
    name: String,
    workers: usize,
    prefixes: Vec<String>,
    address: SocketAddr,
}

struct RuntimeDirectory(PathBuf);

impl RuntimeDirectory {
    /// Creates a fresh private directory for worker state files and hand-off
    /// sockets. The name carries 128 random bits and the directory is created
    /// with mode 0700 in a single `mkdir(2)`, so no other user can predict it,
    /// pre-create it or observe it with wider permissions. An existing entry is
    /// never reused: a collision retries with a new name.
    fn create() -> Result<Self, String> {
        Self::create_in(&std::env::temp_dir())
    }

    fn create_in(parent: &Path) -> Result<Self, String> {
        use std::os::unix::fs::DirBuilderExt;
        for _ in 0..8 {
            let path = parent.join(format!(
                "pam-master-{}-{}",
                std::process::id(),
                random_token()?
            ));
            match fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => {
                    verify_private_directory(&path)?;
                    return Ok(Self(path));
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!(
                        "cannot create runtime directory {}: {error}",
                        path.display()
                    ));
                }
            }
        }
        Err(format!(
            "cannot create a unique runtime directory in {}",
            parent.display()
        ))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

fn random_token() -> Result<String, String> {
    use std::io::Read;
    // /dev/urandom is the portable kernel CSPRNG interface on Linux and macOS.
    let mut bytes = [0_u8; 16];
    fs::File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(&mut bytes))
        .map_err(|error| format!("cannot read system randomness: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Refuses a runtime directory that is a symlink, belongs to another user or is
/// reachable by group/other (a hostile umask cannot widen `mkdir`'s mode, but
/// the check also guards against a filesystem that ignores it).
fn verify_private_directory(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "cannot inspect runtime directory {}: {error}",
            path.display()
        )
    })?;
    // SAFETY: geteuid has no preconditions.
    let owner = unsafe { libc::geteuid() };
    if !metadata.file_type().is_dir()
        || metadata.uid() != owner
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(format!(
            "runtime directory {} is not a private directory owned by this user",
            path.display()
        ));
    }
    Ok(())
}

impl Drop for RuntimeDirectory {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!(
                "pam: cannot remove runtime directory {}: {error}",
                self.0.display()
            );
        }
    }
}

pub fn run(executable: &OsStr, options: StartOptions) -> Result<u8, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("cannot create the cluster supervisor: {error}"))?;
    runtime.block_on(supervise(executable, options))
}

async fn supervise(executable: &OsStr, options: StartOptions) -> Result<u8, String> {
    let runtime_directory = RuntimeDirectory::create()?;
    let pools = prepare_pools(&options)?;
    let ingress = if let Some(address) = options.ingress_address {
        Some(
            Ingress::start(
                address,
                pools
                    .iter()
                    .map(|pool| IngressRoute {
                        name: pool.name.clone(),
                        address: pool.address,
                        prefixes: pool.prefixes.clone(),
                    })
                    .collect(),
            )
            .await?,
        )
    } else {
        None
    };
    // Listeners outlive the worker incarnations that bound them, so a recycle
    // or a reload never closes a socket with connections queued on it.
    #[cfg(target_os = "linux")]
    let listeners = match crate::listener_registry::Registry::start(runtime_directory.path()) {
        Ok(registry) => Some(registry),
        Err(error) => {
            eprintln!(
                "pam: workers bind their own listeners (listener registry unavailable: {error})"
            );
            None
        }
    };
    let mut generation = 1;
    let snapshot: SharedClusterSnapshot = Arc::new(RwLock::new(ClusterSnapshot {
        live: true,
        startup_complete: false,
        desired_workers: options.workers,
        generation,
        workers: Vec::new(),
    }));
    let control_plane = options
        .admin_address
        .map(|address| ControlPlane::start(address, snapshot.clone(), options.admin_token_digest))
        .transpose()?;
    if let Some(control_plane) = &control_plane {
        eprintln!(
            "Pam control plane listening on http://{}",
            control_plane.address()
        );
    }

    let mut workers = spawn_generation(
        executable,
        &options,
        &pools,
        generation,
        runtime_directory.path(),
    )?;
    if let Err(error) = wait_generation_ready(&mut workers, options.startup_timeout).await {
        terminate_workers(&mut workers, options.graceful_timeout).await;
        #[cfg(target_os = "linux")]
        if let Some(listeners) = &listeners {
            listeners.clear();
        }
        set_cluster_health(&snapshot, false, false, generation, &workers);
        return Err(error);
    }
    set_cluster_health(&snapshot, true, true, generation, &workers);
    let _state_file = options
        .state_file
        .as_deref()
        .map(|path| MasterStateFile::create(path, &options, control_plane.as_ref(), &workers))
        .transpose()?;
    let mut warming: Vec<Replacement> = Vec::new();
    let mut retiring: Vec<Retiring> = Vec::new();
    let mut checks = tokio::time::interval(Duration::from_millis(100));
    checks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    #[cfg(unix)]
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|error| format!("cannot listen for SIGTERM: {error}"))?;
    #[cfg(unix)]
    let mut reload = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
        .map_err(|error| format!("cannot listen for SIGHUP: {error}"))?;

    eprintln!(
        "Pam master {} started {} worker(s); send SIGHUP for zero-downtime reload",
        std::process::id(),
        options.workers,
    );

    loop {
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                result.map_err(|error| format!("cannot listen for Ctrl-C: {error}"))?;
                break;
            }
            _ = terminate.recv() => break,
            _ = reload.recv() => {
                let next_generation = generation + 1;
                let mut replacements = spawn_generation(
                    executable,
                    &options,
                    &pools,
                    next_generation,
                    runtime_directory.path(),
                )?;
                match wait_generation_ready(&mut replacements, options.startup_timeout).await {
                    Ok(()) => {
                        generation = next_generation;
                        set_cluster_health(&snapshot, true, true, generation, &replacements);
                        workers.extend(warming.drain(..).map(|replacement| replacement.worker));
                        workers.extend(retiring.drain(..).map(|retiring| retiring.worker));
                        terminate_workers(&mut workers, options.graceful_timeout).await;
                        workers = replacements;
                        #[cfg(target_os = "linux")]
                        if let Some(listeners) = &listeners {
                            listeners.activate(generation);
                        }
                        eprintln!("Pam master activated worker generation {generation}");
                    }
                    Err(error) => {
                        eprintln!(
                            "pam: generation {next_generation} failed readiness; keeping generation {generation}: {error}"
                        );
                        terminate_workers(&mut replacements, options.graceful_timeout).await;
                        #[cfg(target_os = "linux")]
                        if let Some(listeners) = &listeners {
                            listeners.discard(next_generation);
                        }
                        set_cluster_health(&snapshot, true, true, generation, &workers);
                    }
                }
            }
            _ = checks.tick() => {
                for index in (0..workers.len()).rev() {
                    if worker_timed_out(&workers[index], options.watchdog_grace) {
                        let request = worker_record(&workers[index])
                            .and_then(|record| record.request_id)
                            .unwrap_or_else(|| "unknown".to_owned());
                        eprintln!(
                            "pam: worker {} generation {} exceeded the hard request deadline ({request}); killing it",
                            workers[index].id, workers[index].generation,
                        );
                        workers[index].process.signal(SIGKILL);
                    }
                    let status = workers[index]
                        .process
                        .try_wait()
                        .map_err(|error| format!("cannot inspect worker: {error}"))?;
                    if let Some(status) = status {
                        let failed = workers.remove(index);
                        sweep_worker_uploads(&failed);
                        if warming
                            .iter()
                            .any(|replacement| replacement.replaces == failed.process.id())
                        {
                            // Its replacement is already booting and takes the
                            // slot as soon as it is ready.
                            eprintln!(
                                "pam: worker {} generation {} exited with {} while its replacement boots",
                                failed.id, failed.generation, status,
                            );
                            continue;
                        }
                        eprintln!(
                            "pam: worker {} generation {} exited with {}; restarting",
                            failed.id, failed.generation, status,
                        );
                        let failures = restart_failures(
                            status.success(),
                            failed.started.elapsed(),
                            failed.consecutive_failures,
                        );
                        if failures > 0 {
                            // Nobody accepts on this worker's listener during
                            // the backoff; close it so the kernel routes new
                            // connections to the live siblings meanwhile.
                            #[cfg(target_os = "linux")]
                            if let Some(listeners) = &listeners {
                                listeners.release(failed.id);
                            }
                            let multiplier = 1_u32 << failures.saturating_sub(1).min(8);
                            let backoff = options
                                .restart_backoff
                                .saturating_mul(multiplier)
                                .min(MAX_RESTART_BACKOFF);
                            tokio::time::sleep(backoff).await;
                        }
                        let zygote = live_zygote(&failed);
                        workers.push(launch_worker(
                            executable,
                            &options,
                            failed.id,
                            generation,
                            runtime_directory.path(),
                            failures,
                            failed.pool.clone(),
                            zygote.as_ref(),
                        )?);
                        continue;
                    }
                    // Make-before-break recycling: a worker that reached
                    // --max-requests (or whose PHP engine is wedged) keeps
                    // serving while its replacement boots.
                    let pid = workers[index].process.id();
                    let wants_recycle = worker_record(&workers[index])
                        .is_some_and(|record| record.recycle_requested);
                    if wants_recycle
                        && !warming.iter().any(|replacement| replacement.replaces == pid)
                        && workers[index]
                            .retry_replacement_after
                            .is_none_or(|after| Instant::now() >= after)
                    {
                        let zygote = live_zygote(&workers[index]);
                        let replacement = launch_worker(
                            executable,
                            &options,
                            workers[index].id,
                            workers[index].generation,
                            runtime_directory.path(),
                            0,
                            workers[index].pool.clone(),
                            zygote.as_ref(),
                        )?;
                        warming.push(Replacement { replaces: pid, worker: replacement });
                    }
                }

                for index in (0..warming.len()).rev() {
                    let exited = warming[index]
                        .worker
                        .process
                        .try_wait()
                        .map_err(|error| format!("cannot inspect worker: {error}"))?;
                    let ready = exited.is_none()
                        && worker_record(&warming[index].worker).is_some_and(|record| {
                            matches!(
                                record.lifecycle(),
                                Ok(WorkerLifecycle::Ready | WorkerLifecycle::Busy)
                            )
                        });
                    if ready {
                        let Replacement { replaces, worker } = warming.remove(index);
                        if let Some(position) =
                            workers.iter().position(|old| old.process.id() == replaces)
                        {
                            let mut old = workers.remove(position);
                            old.process.signal(SIGTERM);
                            retiring.push(Retiring {
                                worker: old,
                                deadline: Instant::now() + options.graceful_timeout,
                            });
                        }
                        workers.push(worker);
                        continue;
                    }
                    let timed_out = warming[index].worker.started.elapsed() >= options.startup_timeout;
                    if exited.is_some() || timed_out {
                        let mut failed = warming.remove(index);
                        eprintln!(
                            "pam: replacement for worker {} generation {} failed to start ({}); the current worker keeps serving",
                            failed.worker.id,
                            failed.worker.generation,
                            exited.map_or_else(|| "startup timeout".to_owned(), |status| status.to_string()),
                        );
                        if exited.is_none() {
                            failed.worker.process.kill();
                            let _ = failed.worker.process.wait();
                        }
                        let retry = Instant::now() + options.restart_backoff.max(Duration::from_secs(1));
                        if let Some(old) = workers
                            .iter_mut()
                            .find(|old| old.process.id() == failed.replaces)
                        {
                            old.retry_replacement_after = Some(retry);
                        } else {
                            // The old worker is gone too: start a plain replacement.
                            let zygote = live_zygote(&failed.worker);
                            workers.push(launch_worker(
                                executable,
                                &options,
                                failed.worker.id,
                                generation,
                                runtime_directory.path(),
                                1,
                                failed.worker.pool.clone(),
                                zygote.as_ref(),
                            )?);
                        }
                    }
                }

                reap_retiring(&mut retiring);
                set_cluster_health(&snapshot, true, true, generation, &workers);
            }
        }
    }

    set_cluster_health(&snapshot, true, false, generation, &workers);
    workers.extend(warming.drain(..).map(|replacement| replacement.worker));
    workers.extend(retiring.drain(..).map(|retiring| retiring.worker));
    terminate_workers(&mut workers, options.graceful_timeout).await;
    #[cfg(target_os = "linux")]
    if let Some(listeners) = &listeners {
        listeners.clear();
    }
    if let Some(ingress) = ingress {
        ingress.stop().await;
    }
    set_cluster_health(&snapshot, false, false, generation, &workers);
    Ok(0)
}

struct MasterStateFile(PathBuf);

impl MasterStateFile {
    fn create(
        path: &Path,
        options: &StartOptions,
        control_plane: Option<&ControlPlane>,
        workers: &[Worker],
    ) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "cannot create PAM state directory {}: {error}",
                    parent.display()
                )
            })?;
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).map_err(|error| {
                format!(
                    "cannot secure PAM state directory {}: {error}",
                    parent.display()
                )
            })?;
        }
        if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(format!(
                "refusing symlink PAM state file {}",
                path.display()
            ));
        }
        let (worker_spawn_spread_millis, worker_startup_p95_millis, worker_startup_max_millis) =
            generation_startup_summary(workers);
        let worker_startup_phase_p95_millis = generation_startup_phase_p95(workers);
        let state = MasterState {
            version: 1,
            pid: std::process::id(),
            process_start: linux_process_start(std::process::id()),
            workers: options.workers,
            admin_address: control_plane.map(|control| control.address().to_string()),
            started_at_millis: epoch_millis(),
            worker_spawn_spread_millis,
            worker_startup_p95_millis,
            worker_startup_max_millis,
            worker_startup_phase_p95_millis,
        };
        let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
        let bytes = serde_json::to_vec_pretty(&state)
            .map_err(|error| format!("cannot serialize PAM state: {error}"))?;
        fs::write(&temporary, bytes)
            .map_err(|error| format!("cannot write PAM state {}: {error}", temporary.display()))?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("cannot secure PAM state {}: {error}", temporary.display()))?;
        fs::rename(&temporary, path)
            .map_err(|error| format!("cannot publish PAM state {}: {error}", path.display()))?;
        Ok(Self(path.to_owned()))
    }
}

impl Drop for MasterStateFile {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_file(&self.0)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!(
                "pam: cannot remove state file {}: {error}",
                self.0.display()
            );
        }
    }
}

pub(crate) fn linux_process_start(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_command = stat.rsplit_once(") ")?.1;
    after_command.split_whitespace().nth(19)?.parse().ok()
}

fn linux_process_state(pid: u32) -> Option<char> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.rsplit_once(") ")?.1.chars().next()
}

#[cfg(target_os = "linux")]
type SharedZygote = std::rc::Rc<std::cell::RefCell<crate::prefork::Zygote>>;
#[cfg(not(target_os = "linux"))]
type SharedZygote = std::convert::Infallible;

fn spawn_generation(
    executable: &OsStr,
    options: &StartOptions,
    pools: &[RuntimePool],
    generation: u64,
    runtime_directory: &Path,
) -> Result<Vec<Worker>, String> {
    let zygote = start_zygote(executable, options)?;
    let zygote = zygote.as_ref();
    if pools.is_empty() {
        return (1..=options.workers)
            .map(|id| {
                launch_worker(
                    executable,
                    options,
                    id,
                    generation,
                    runtime_directory,
                    0,
                    None,
                    zygote,
                )
            })
            .collect();
    }
    let mut id = 0;
    let mut workers = Vec::with_capacity(options.workers);
    for pool in pools {
        for _ in 0..pool.workers {
            id += 1;
            workers.push(launch_worker(
                executable,
                options,
                id,
                generation,
                runtime_directory,
                0,
                Some(pool.clone()),
                zygote,
            )?);
        }
    }
    Ok(workers)
}

/// Prefork shares one OPcache/JIT segment between the workers of a generation.
/// It needs Linux and at least two workers, and `PAM_PREFORK=0` disables it.
#[cfg(target_os = "linux")]
fn start_zygote(
    executable: &OsStr,
    options: &StartOptions,
) -> Result<Option<SharedZygote>, String> {
    if options.workers < 2 || std::env::var_os("PAM_PREFORK").is_some_and(|value| value == "0") {
        return Ok(None);
    }
    let mut command = Command::new(executable);
    command
        .arg("__zygote")
        .arg(&options.script)
        .args(&options.script_args)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    configure_runtime_command(&mut command, options);
    let zygote = crate::prefork::Zygote::start(command)?;
    Ok(Some(std::rc::Rc::new(std::cell::RefCell::new(zygote))))
}

#[cfg(not(target_os = "linux"))]
fn start_zygote(
    _executable: &OsStr,
    _options: &StartOptions,
) -> Result<Option<SharedZygote>, String> {
    Ok(None)
}

/// Settings shared by exec'd workers and the zygote that forks them.
fn configure_runtime_command(command: &mut Command, options: &StartOptions) {
    command
        .env_remove(ADMIN_TOKEN_ENV)
        .env_remove(ADMIN_TOKEN_FILE_ENV);
    if let Some(extensions) = options.php_extensions.as_deref() {
        command
            .env("PHP_INI_SCAN_DIR", "")
            .env("PAM_INI_ENTRIES", php_extension_ini_entries(extensions));
    }
    #[cfg(target_os = "linux")]
    {
        let master_pid = std::process::id();
        // SAFETY: this closure runs in the single-threaded child after fork and before exec.
        unsafe {
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() as u32 != master_pid {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "PAM master exited while spawning a worker",
                    ));
                }
                Ok(())
            });
        }
    }
}

/// Per-worker identity, applied with `exec` or after a zygote fork.
struct WorkerEnvironment {
    state_path: PathBuf,
    spawned_at_millis: u64,
    variables: Vec<(String, String)>,
}

fn worker_environment(
    options: &StartOptions,
    id: usize,
    generation: u64,
    runtime_directory: &Path,
    pool: Option<&RuntimePool>,
) -> Result<WorkerEnvironment, String> {
    // A make-before-break replacement shares the id and generation of the
    // worker it replaces while both run, so each incarnation has its own file.
    let incarnation = NEXT_INCARNATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let state_path = runtime_directory.join(format!(
        "worker-{id}-generation-{generation}-{incarnation}.json"
    ));
    if let Err(error) = fs::remove_file(&state_path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return Err(format!(
            "cannot reset worker state {}: {error}",
            state_path.display()
        ));
    }
    let path_text = |path: &Path| {
        path.to_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("runtime path is not UTF-8: {}", path.display()))
    };
    let spawned_at_millis = epoch_millis();
    let mut environment = vec![
        ("PAM_WORKER_ID".to_owned(), id.to_string()),
        ("PAM_WORKER_GENERATION".to_owned(), generation.to_string()),
        (
            "PAM_MAX_REQUESTS".to_owned(),
            staggered_max_requests(options, id).to_string(),
        ),
        (
            "PAM_WORKER_SPAWNED_AT_MILLIS".to_owned(),
            spawned_at_millis.to_string(),
        ),
        (
            "PAM_CACHE_INVALIDATION_LOG".to_owned(),
            path_text(&runtime_directory.join("cache-invalidations.log"))?,
        ),
        (WORKER_STATE_PATH_ENV.to_owned(), path_text(&state_path)?),
    ];
    #[cfg(target_os = "linux")]
    {
        let registry = runtime_directory.join("listeners.sock");
        if registry.exists() {
            environment.push((
                crate::listener_registry::REGISTRY_ENV.to_owned(),
                path_text(&registry)?,
            ));
        }
    }
    if let Some(pool) = pool {
        environment.push(("PAM_WORKER_POOL".to_owned(), pool.name.clone()));
        environment.push((
            "PAM_INTERNAL_LISTEN_ADDRESS".to_owned(),
            pool.address.to_string(),
        ));
    }
    Ok(WorkerEnvironment {
        state_path,
        spawned_at_millis,
        variables: environment,
    })
}

#[allow(clippy::too_many_arguments)]
fn launch_worker(
    executable: &OsStr,
    options: &StartOptions,
    id: usize,
    generation: u64,
    runtime_directory: &Path,
    consecutive_failures: u32,
    pool: Option<RuntimePool>,
    zygote: Option<&SharedZygote>,
) -> Result<Worker, String> {
    let WorkerEnvironment {
        state_path,
        spawned_at_millis,
        variables: environment,
    } = worker_environment(options, id, generation, runtime_directory, pool.as_ref())?;
    let process = match zygote {
        #[cfg(target_os = "linux")]
        Some(zygote) => {
            let pid = zygote
                .borrow_mut()
                .fork_worker(environment)
                .map_err(|error| format!("cannot start worker {id}: {error}"))?;
            WorkerProcess::Forked {
                pid,
                zygote: zygote.clone(),
                status: None,
            }
        }
        #[cfg(not(target_os = "linux"))]
        Some(never) => match *never {},
        None => {
            let mut command = Command::new(executable);
            command
                .arg("__worker")
                .arg(&options.script)
                .args(&options.script_args)
                .envs(environment)
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit());
            configure_runtime_command(&mut command, options);
            WorkerProcess::Spawned(
                command
                    .spawn()
                    .map_err(|error| format!("cannot start worker {id}: {error}"))?,
            )
        }
    };
    Ok(Worker {
        id,
        generation,
        process,
        state_path,
        spawned_at_millis,
        started: Instant::now(),
        consecutive_failures,
        pool,
        retry_replacement_after: None,
    })
}

fn prepare_pools(options: &StartOptions) -> Result<Vec<RuntimePool>, String> {
    options
        .pools
        .iter()
        .map(|pool| {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|error| {
                format!("cannot reserve address for pool {}: {error}", pool.name)
            })?;
            let address = listener.local_addr().map_err(|error| error.to_string())?;
            drop(listener);
            Ok(RuntimePool {
                name: pool.name.clone(),
                workers: pool.workers,
                prefixes: pool.prefixes.clone(),
                address,
            })
        })
        .collect()
}

/// Consecutive-failure count used for restart backoff. A worker that exits
/// successfully was recycled on purpose (`--max-requests` or a fatal-error
/// recycle) and is replaced immediately: backing off there would take
/// capacity away exactly when busy workers recycle, and the delay grows with
/// traffic because busy workers recycle sooner.
fn restart_failures(clean_exit: bool, uptime: Duration, previous_failures: u32) -> u32 {
    if clean_exit {
        0
    } else if uptime >= Duration::from_secs(60) {
        1
    } else {
        previous_failures.saturating_add(1)
    }
}

fn staggered_max_requests(options: &StartOptions, worker_id: usize) -> u64 {
    // Always stagger: with small limits (Octane's default is 500) evenly
    // balanced workers would otherwise reach the limit together and recycle at
    // the same moment, leaving no listener and refusing connections. The
    // stagger only lowers limits (by up to 25%): --max-requests is a ceiling.
    if options.workers <= 1 {
        return options.max_requests;
    }
    let spread = options.max_requests / 4;
    let offset = spread.saturating_mul(worker_id.saturating_sub(1) as u64)
        / options.workers.saturating_sub(1) as u64;
    options.max_requests.saturating_sub(offset).max(1)
}

async fn wait_generation_ready(workers: &mut [Worker], timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        let mut ready = 0;
        for worker in workers.iter_mut() {
            if let Some(status) = worker
                .process
                .try_wait()
                .map_err(|error| format!("cannot inspect worker {}: {error}", worker.id))?
            {
                return Err(format!(
                    "worker {} generation {} exited before readiness with {status}",
                    worker.id, worker.generation
                ));
            }
            if worker_record(worker).is_some_and(|record| {
                matches!(
                    record.lifecycle(),
                    Ok(WorkerLifecycle::Ready | WorkerLifecycle::Busy)
                )
            }) {
                ready += 1;
            }
        }
        if ready == workers.len() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "only {ready}/{} workers became ready before {} ms",
                workers.len(),
                timeout.as_millis()
            ));
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

fn generation_startup_summary(workers: &[Worker]) -> (Option<u64>, Option<u64>, Option<u64>) {
    let timings = workers
        .iter()
        .filter_map(|worker| {
            worker_record(worker).map(|record| (worker.spawned_at_millis, record.updated_at_millis))
        })
        .collect::<Vec<_>>();
    if timings.len() != workers.len() {
        return (None, None, None);
    }
    summarize_startup_timings(&timings)
}

fn summarize_startup_timings(timings: &[(u64, u64)]) -> (Option<u64>, Option<u64>, Option<u64>) {
    if timings.is_empty() {
        return (None, None, None);
    }
    let mut startup = timings
        .iter()
        .map(|(spawned, ready)| ready.saturating_sub(*spawned))
        .collect::<Vec<_>>();
    startup.sort_unstable();
    let first_spawn = timings.iter().map(|(spawned, _)| *spawned).min();
    let last_spawn = timings.iter().map(|(spawned, _)| *spawned).max();
    let spread = first_spawn
        .zip(last_spawn)
        .map(|(first, last)| last.saturating_sub(first));
    let p95_index = (startup.len() * 95).div_ceil(100).saturating_sub(1);
    (spread, Some(startup[p95_index]), startup.last().copied())
}

fn generation_startup_phase_p95(workers: &[Worker]) -> Option<WorkerStartupPhases> {
    let phases = workers
        .iter()
        .filter_map(|worker| worker_record(worker)?.startup_phases)
        .collect::<Vec<_>>();
    if phases.len() != workers.len() {
        return None;
    }
    summarize_startup_phases(&phases)
}

fn summarize_startup_phases(phases: &[WorkerStartupPhases]) -> Option<WorkerStartupPhases> {
    if phases.is_empty() {
        return None;
    }
    Some(WorkerStartupPhases {
        spawn_to_process_millis: p95(phases
            .iter()
            .map(|phase| phase.spawn_to_process_millis)
            .collect()),
        php_engine_millis: p95(phases.iter().map(|phase| phase.php_engine_millis).collect()),
        spawn_to_engine_millis: p95(phases
            .iter()
            .map(|phase| phase.spawn_to_engine_millis)
            .collect()),
        composer_millis: p95(phases.iter().map(|phase| phase.composer_millis).collect()),
        runtime_bootstrap_millis: p95(phases
            .iter()
            .map(|phase| phase.runtime_bootstrap_millis)
            .collect()),
        application_millis: p95(phases
            .iter()
            .map(|phase| phase.application_millis)
            .collect()),
    })
}

fn p95(mut values: Vec<u64>) -> u64 {
    values.sort_unstable();
    let index = (values.len() * 95).div_ceil(100).saturating_sub(1);
    values[index]
}

fn parse_php_extension(value: Option<OsString>) -> Result<String, String> {
    let value = value
        .ok_or_else(|| "--php-extension requires a module name".to_owned())?
        .into_string()
        .map_err(|_| "--php-extension requires valid UTF-8".to_owned())?;
    validate_php_extension(&value)?;
    Ok(value)
}

pub(crate) fn validate_php_extension(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(
            "--php-extension requires a 1-64 character module name containing only letters, digits, '_' or '-'"
                .to_owned(),
        );
    }
    Ok(())
}

fn php_extension_ini_entries(extensions: &[String]) -> String {
    let mut entries = std::env::var("PAM_INI_ENTRIES").unwrap_or_default();
    for extension in extensions {
        if extension == "opcache" {
            entries.push_str("zend_extension=opcache\n");
        } else {
            entries.push_str("extension=");
            entries.push_str(extension);
            entries.push('\n');
        }
    }
    entries
}

fn worker_record(worker: &Worker) -> Option<WorkerRuntimeRecord> {
    read(&worker.state_path)
        .inspect_err(|error| {
            if worker.state_path.exists() {
                eprintln!("pam: {error}");
            }
        })
        .ok()
        .filter(|record| {
            record.pid == worker.process.id()
                && record.worker_id == worker.id
                && record.generation == worker.generation
        })
}

#[cfg(target_os = "linux")]
fn live_zygote(worker: &Worker) -> Option<SharedZygote> {
    worker
        .process
        .zygote()
        .filter(|zygote| zygote.borrow_mut().is_running())
}

#[cfg(not(target_os = "linux"))]
fn live_zygote(_worker: &Worker) -> Option<SharedZygote> {
    None
}

/// Forgets retired workers that exited and kills those that outlived the
/// graceful timeout.
fn reap_retiring(retiring: &mut Vec<Retiring>) {
    retiring.retain_mut(|entry| {
        if entry.worker.process.try_wait().ok().flatten().is_some() {
            sweep_worker_uploads(&entry.worker);
            return false;
        }
        if Instant::now() >= entry.deadline {
            eprintln!(
                "pam: retired worker {} generation {} did not drain in time; killing it",
                entry.worker.id, entry.worker.generation,
            );
            entry.worker.process.kill();
            let _ = entry.worker.process.wait();
            sweep_worker_uploads(&entry.worker);
            return false;
        }
        true
    });
}

/// Deletes the temporary uploads a reaped worker left behind (it was killed
/// or crashed with a request in flight).
fn sweep_worker_uploads(worker: &Worker) {
    let Some(directory) = worker_record(worker).and_then(|record| record.upload_directory) else {
        return;
    };
    let removed = crate::upload::sweep_process(Path::new(&directory), worker.process.id());
    if removed > 0 {
        eprintln!(
            "pam: removed {removed} temporary upload file(s) left by worker {} generation {}",
            worker.id, worker.generation,
        );
    }
}

fn worker_timed_out(worker: &Worker, grace: Duration) -> bool {
    worker_record(worker).is_some_and(|record| {
        let now = epoch_millis();
        // A worker that asked to be recycled already answered its stuck
        // request with 504 itself; it is replaced make-before-break and then
        // killed if it does not drain in time.
        !record.recycle_requested
            && record.lifecycle().ok() == Some(WorkerLifecycle::Busy)
            && record.deadline_at_millis.is_some_and(|deadline| {
                now >= deadline
                    .saturating_add(grace.as_millis() as u64)
                    .saturating_add(WORKER_SELF_TIMEOUT_ALLOWANCE_MILLIS)
            })
    })
}

fn set_cluster_health(
    snapshot: &SharedClusterSnapshot,
    live: bool,
    startup_complete: bool,
    generation: u64,
    workers: &[Worker],
) {
    let records = workers.iter().filter_map(worker_record).collect();
    let mut current = snapshot
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    current.live = live;
    current.startup_complete = startup_complete;
    current.generation = generation;
    current.workers = records;
}

async fn terminate_workers(workers: &mut Vec<Worker>, timeout: Duration) {
    for worker in workers.iter_mut() {
        worker.process.signal(SIGTERM);
    }

    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let mut running = false;
        for worker in workers.iter_mut() {
            if worker.process.try_wait().ok().flatten().is_none() {
                running = true;
            }
        }
        if !running {
            workers.clear();
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    for worker in workers.iter_mut() {
        if worker.process.try_wait().ok().flatten().is_none() {
            worker.process.kill();
        }
        let _ = worker.process.wait();
    }
    workers.clear();
}

fn parse_positive(value: Option<OsString>, option: &str) -> Result<usize, String> {
    let value = value.ok_or_else(|| format!("{option} requires a value"))?;
    let parsed = value
        .to_string_lossy()
        .parse::<usize>()
        .map_err(|_| format!("{option} requires a positive integer"))?;
    if parsed == 0 {
        return Err(format!("{option} requires a positive integer"));
    }
    Ok(parsed)
}

fn parse_address(value: Option<OsString>, option: &str) -> Result<SocketAddr, String> {
    value
        .ok_or_else(|| format!("{option} requires an IP:port value"))?
        .to_string_lossy()
        .parse()
        .map_err(|_| format!("{option} requires a valid IP:port value"))
}

fn parse_pool(value: Option<OsString>) -> Result<PoolSpec, String> {
    let value = value.ok_or_else(|| "--pool requires name=workers@prefix[,prefix]".to_owned())?;
    let value = value.to_string_lossy();
    let (name, rest) = value
        .split_once('=')
        .ok_or_else(|| "--pool requires name=workers@prefix[,prefix]".to_owned())?;
    let (workers, prefixes) = rest
        .split_once('@')
        .ok_or_else(|| "--pool requires name=workers@prefix[,prefix]".to_owned())?;
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err("pool name may only contain letters, numbers, '-' and '_'".to_owned());
    }
    let workers = workers
        .parse::<usize>()
        .map_err(|_| "pool workers must be a positive integer".to_owned())?;
    if workers == 0 {
        return Err("pool workers must be a positive integer".to_owned());
    }
    let prefixes: Vec<String> = prefixes.split(',').map(str::to_owned).collect();
    if prefixes.is_empty()
        || prefixes
            .iter()
            .any(|prefix| prefix != "*" && !prefix.starts_with('/'))
    {
        return Err("pool prefixes must be '*' or absolute paths".to_owned());
    }
    Ok(PoolSpec {
        name: name.to_owned(),
        workers,
        prefixes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_directory_is_private_random_and_never_reused() {
        use std::os::unix::fs::MetadataExt;
        let parent =
            std::env::temp_dir().join(format!("pam-rtdir-test-{}", random_token().unwrap()));
        fs::create_dir(&parent).unwrap();
        let first = RuntimeDirectory::create_in(&parent).unwrap();
        let second = RuntimeDirectory::create_in(&parent).unwrap();
        assert_ne!(first.path(), second.path());
        let metadata = fs::symlink_metadata(first.path()).unwrap();
        assert_eq!(metadata.mode() & 0o777, 0o700);
        let name = first
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert_eq!(name.rsplit('-').next().unwrap().len(), 32);

        // A pre-planted entry (here a symlink) is refused by verification.
        let planted = parent.join("planted");
        std::os::unix::fs::symlink(first.path(), &planted).unwrap();
        assert!(verify_private_directory(&planted).is_err());
        let wide = parent.join("wide");
        fs::create_dir(&wide).unwrap();
        fs::set_permissions(&wide, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(verify_private_directory(&wide).is_err());
        drop(first);
        drop(second);
        fs::remove_dir_all(&parent).unwrap();
    }

    #[test]
    fn production_default_avoids_continuous_worker_recycling() {
        let options = StartOptions::parse(std::iter::empty()).unwrap();

        assert_eq!(options.max_requests, DEFAULT_MAX_REQUESTS);
        assert!(options.php_extensions.is_none());
    }

    #[test]
    fn parses_a_bounded_deduplicated_php_extension_allowlist() {
        let options = StartOptions::parse(
            [
                "--php-extension",
                "mbstring",
                "--php-extension",
                "opcache",
                "--php-extension",
                "mbstring",
            ]
            .into_iter()
            .map(OsString::from),
        )
        .unwrap();

        assert_eq!(
            options.php_extensions,
            Some(vec!["mbstring".to_owned(), "opcache".to_owned()])
        );
        let entries = php_extension_ini_entries(options.php_extensions.as_deref().unwrap());
        assert!(entries.contains("extension=mbstring\n"));
        assert!(entries.contains("zend_extension=opcache\n"));
        assert!(
            StartOptions::parse(
                ["--php-extension", "../../unsafe"]
                    .into_iter()
                    .map(OsString::from)
            )
            .is_err()
        );
    }

    #[test]
    fn explicitly_isolates_an_empty_php_extension_profile() {
        let options =
            StartOptions::parse(["--isolate-php-extensions"].into_iter().map(OsString::from))
                .unwrap();
        assert_eq!(options.php_extensions, Some(Vec::new()));
    }

    #[test]
    fn older_master_state_defaults_additive_startup_diagnostics() {
        let state: MasterState = serde_json::from_value(serde_json::json!({
            "version": 1,
            "pid": 42,
            "processStart": null,
            "workers": 4,
            "adminAddress": null,
            "startedAtMillis": 1
        }))
        .unwrap();

        assert_eq!(state.worker_spawn_spread_millis, None);
        assert_eq!(state.worker_startup_p95_millis, None);
        assert_eq!(state.worker_startup_max_millis, None);
        assert!(state.worker_startup_phase_p95_millis.is_none());
    }

    #[test]
    fn startup_phase_summary_uses_nearest_rank_p95_for_each_phase() {
        let phases = (1..=20)
            .map(|value| WorkerStartupPhases {
                spawn_to_process_millis: value / 2,
                php_engine_millis: value - (value / 2),
                spawn_to_engine_millis: value,
                composer_millis: value * 2,
                runtime_bootstrap_millis: value * 3,
                application_millis: value * 4,
            })
            .collect::<Vec<_>>();

        let summary = summarize_startup_phases(&phases).unwrap();

        assert_eq!(summary.spawn_to_process_millis, 9);
        assert_eq!(summary.php_engine_millis, 10);
        assert_eq!(summary.spawn_to_engine_millis, 19);
        assert_eq!(summary.composer_millis, 38);
        assert_eq!(summary.runtime_bootstrap_millis, 57);
        assert_eq!(summary.application_millis, 76);
    }

    #[test]
    fn large_recycle_limits_are_staggered_across_workers() {
        let mut options = StartOptions::parse(std::iter::empty()).unwrap();
        options.workers = 4;

        assert_eq!(staggered_max_requests(&options, 1), 10_000_000);
        assert_eq!(staggered_max_requests(&options, 4), 7_500_000);
    }

    #[test]
    fn planned_recycles_restart_without_backoff_and_crashes_back_off() {
        assert_eq!(restart_failures(true, Duration::from_millis(500), 7), 0);
        assert_eq!(restart_failures(false, Duration::from_millis(500), 0), 1);
        assert_eq!(restart_failures(false, Duration::from_millis(500), 3), 4);
        assert_eq!(restart_failures(false, Duration::from_secs(61), 3), 1);
    }

    #[test]
    fn small_recycle_limits_are_staggered_so_workers_never_recycle_together() {
        let mut options = StartOptions::parse(std::iter::empty()).unwrap();
        options.workers = 4;
        options.max_requests = 500;

        let limits = (1..=4)
            .map(|worker| staggered_max_requests(&options, worker))
            .collect::<Vec<_>>();
        assert_eq!(limits, vec![500, 459, 417, 375]);
        assert!(limits.iter().all(|limit| *limit <= options.max_requests));

        options.workers = 1;
        assert_eq!(staggered_max_requests(&options, 1), 500);
    }

    #[test]
    fn startup_summary_separates_spawn_spread_from_worker_bootstrap() {
        let timings = (0_u64..20)
            .map(|index| (1_000 + index * 2, 1_100 + index * 3))
            .collect::<Vec<_>>();
        let (spread, p95, maximum) = summarize_startup_timings(&timings);

        assert_eq!(spread, Some(38));
        assert_eq!(p95, Some(118));
        assert_eq!(maximum, Some(119));
        assert_eq!(summarize_startup_timings(&[]), (None, None, None));
    }

    #[test]
    fn parses_specialized_worker_pools() {
        let options = StartOptions::parse(
            [
                "--ingress-address",
                "127.0.0.1:8080",
                "--pool",
                "api=4@/api,/graphql",
                "--pool",
                "web=2@*",
            ]
            .into_iter()
            .map(OsString::from),
        )
        .unwrap();
        assert_eq!(options.workers, 6);
        assert_eq!(
            options.pools[0],
            PoolSpec {
                name: "api".to_owned(),
                workers: 4,
                prefixes: vec!["/api".to_owned(), "/graphql".to_owned()],
            }
        );
    }

    #[test]
    fn pool_configuration_requires_one_fallback() {
        let error = StartOptions::parse(
            [
                "--ingress-address",
                "127.0.0.1:8080",
                "--pool",
                "api=2@/api",
            ]
            .into_iter()
            .map(OsString::from),
        )
        .unwrap_err();
        assert!(error.contains("exactly one '*' fallback"));
    }
}
