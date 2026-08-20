use std::ffi::{OsStr, OsString};
use std::fs;
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::admin_auth::{self, ADMIN_TOKEN_ENV, ADMIN_TOKEN_FILE_ENV};
use crate::control_plane::{ClusterSnapshot, ControlPlane, SharedClusterSnapshot};
use crate::ingress::{Ingress, Route as IngressRoute};
use crate::worker_state::{
    WORKER_STATE_PATH_ENV, WorkerLifecycle, WorkerRuntimeRecord, epoch_millis, read,
};

const SIGTERM: i32 = 15;
const SIGKILL: i32 = 9;
const DEFAULT_MAX_REQUESTS: u64 = 10_000_000;
const MAX_RESTART_BACKOFF: Duration = Duration::from_secs(30);

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
    child: Child,
    state_path: PathBuf,
    started: Instant,
    consecutive_failures: u32,
    pool: Option<RuntimePool>,
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
    fn create() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!(
            "pam-master-{}-{}",
            std::process::id(),
            epoch_millis()
        ));
        fs::create_dir(&path).map_err(|error| {
            format!(
                "cannot create runtime directory {}: {error}",
                path.display()
            )
        })?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).map_err(|error| {
            format!(
                "cannot secure runtime directory {}: {error}",
                path.display()
            )
        })?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
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
        set_cluster_health(&snapshot, false, false, generation, &workers);
        return Err(error);
    }
    set_cluster_health(&snapshot, true, true, generation, &workers);
    let _state_file = options
        .state_file
        .as_deref()
        .map(|path| MasterStateFile::create(path, &options, control_plane.as_ref()))
        .transpose()?;
    let mut checks = tokio::time::interval(Duration::from_millis(200));
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
                        terminate_workers(&mut workers, options.graceful_timeout).await;
                        workers = replacements;
                        eprintln!("Pam master activated worker generation {generation}");
                    }
                    Err(error) => {
                        eprintln!(
                            "pam: generation {next_generation} failed readiness; keeping generation {generation}: {error}"
                        );
                        terminate_workers(&mut replacements, options.graceful_timeout).await;
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
                        send_signal(&workers[index].child, SIGKILL);
                    }
                    let status = workers[index]
                        .child
                        .try_wait()
                        .map_err(|error| format!("cannot inspect worker: {error}"))?;
                    if let Some(status) = status {
                        let failed = workers.remove(index);
                        eprintln!(
                            "pam: worker {} generation {} exited with {}; restarting",
                            failed.id, failed.generation, status,
                        );
                        let failures = if failed.started.elapsed() >= Duration::from_secs(60) {
                            1
                        } else {
                            failed.consecutive_failures.saturating_add(1)
                        };
                        let multiplier = 1_u32 << failures.saturating_sub(1).min(8);
                        let backoff = options
                            .restart_backoff
                            .saturating_mul(multiplier)
                            .min(MAX_RESTART_BACKOFF);
                        tokio::time::sleep(backoff).await;
                        workers.push(spawn_worker(
                            executable,
                            &options,
                            failed.id,
                            generation,
                            runtime_directory.path(),
                            failures,
                            failed.pool.clone(),
                        )?);
                    }
                }
                set_cluster_health(&snapshot, true, true, generation, &workers);
            }
        }
    }

    set_cluster_health(&snapshot, true, false, generation, &workers);
    terminate_workers(&mut workers, options.graceful_timeout).await;
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
        let state = MasterState {
            version: 1,
            pid: std::process::id(),
            process_start: linux_process_start(std::process::id()),
            workers: options.workers,
            admin_address: control_plane.map(|control| control.address().to_string()),
            started_at_millis: epoch_millis(),
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

fn linux_process_start(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_command = stat.rsplit_once(") ")?.1;
    after_command.split_whitespace().nth(19)?.parse().ok()
}

fn spawn_generation(
    executable: &OsStr,
    options: &StartOptions,
    pools: &[RuntimePool],
    generation: u64,
    runtime_directory: &Path,
) -> Result<Vec<Worker>, String> {
    if pools.is_empty() {
        return (1..=options.workers)
            .map(|id| {
                spawn_worker(
                    executable,
                    options,
                    id,
                    generation,
                    runtime_directory,
                    0,
                    None,
                )
            })
            .collect();
    }
    let mut id = 0;
    let mut workers = Vec::with_capacity(options.workers);
    for pool in pools {
        for _ in 0..pool.workers {
            id += 1;
            workers.push(spawn_worker(
                executable,
                options,
                id,
                generation,
                runtime_directory,
                0,
                Some(pool.clone()),
            )?);
        }
    }
    Ok(workers)
}

fn spawn_worker(
    executable: &OsStr,
    options: &StartOptions,
    id: usize,
    generation: u64,
    runtime_directory: &Path,
    consecutive_failures: u32,
    pool: Option<RuntimePool>,
) -> Result<Worker, String> {
    let state_path = runtime_directory.join(format!("worker-{id}-generation-{generation}.json"));
    if let Err(error) = fs::remove_file(&state_path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return Err(format!(
            "cannot reset worker state {}: {error}",
            state_path.display()
        ));
    }
    let max_requests = staggered_max_requests(options, id);
    let mut command = Command::new(executable);
    command
        .arg("__worker")
        .arg(&options.script)
        .args(&options.script_args)
        .env("PAM_WORKER_ID", id.to_string())
        .env("PAM_WORKER_GENERATION", generation.to_string())
        .env("PAM_MAX_REQUESTS", max_requests.to_string())
        .env_remove(ADMIN_TOKEN_ENV)
        .env_remove(ADMIN_TOKEN_FILE_ENV)
        .env(
            "PAM_CACHE_INVALIDATION_LOG",
            runtime_directory.join("cache-invalidations.log"),
        )
        .env(WORKER_STATE_PATH_ENV, &state_path)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if let Some(pool) = &pool {
        command
            .env("PAM_WORKER_POOL", &pool.name)
            .env("PAM_INTERNAL_LISTEN_ADDRESS", pool.address.to_string());
    }
    let child = command
        .spawn()
        .map_err(|error| format!("cannot start worker {id}: {error}"))?;
    Ok(Worker {
        id,
        generation,
        child,
        state_path,
        started: Instant::now(),
        consecutive_failures,
        pool,
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

fn staggered_max_requests(options: &StartOptions, worker_id: usize) -> u64 {
    if options.workers <= 1 || options.max_requests < 1_000 {
        return options.max_requests;
    }
    let spread = options.max_requests / 4;
    let offset = spread.saturating_mul(worker_id.saturating_sub(1) as u64)
        / options.workers.saturating_sub(1) as u64;
    options.max_requests.saturating_add(offset)
}

async fn wait_generation_ready(workers: &mut [Worker], timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        let mut ready = 0;
        for worker in workers.iter_mut() {
            if let Some(status) = worker
                .child
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
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
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
            record.pid == worker.child.id()
                && record.worker_id == worker.id
                && record.generation == worker.generation
        })
}

fn worker_timed_out(worker: &Worker, grace: Duration) -> bool {
    worker_record(worker).is_some_and(|record| {
        let now = epoch_millis();
        record.lifecycle().ok() == Some(WorkerLifecycle::Busy)
            && record
                .deadline_at_millis
                .is_some_and(|deadline| now >= deadline.saturating_add(grace.as_millis() as u64))
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
    for worker in workers.iter() {
        send_signal(&worker.child, SIGTERM);
    }

    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let mut running = false;
        for worker in workers.iter_mut() {
            if worker.child.try_wait().ok().flatten().is_none() {
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
        if worker.child.try_wait().ok().flatten().is_none() {
            let _ = worker.child.kill();
        }
        let _ = worker.child.wait();
    }
    workers.clear();
}

fn send_signal(child: &Child, signal: i32) {
    let Ok(pid) = i32::try_from(child.id()) else {
        return;
    };
    // SAFETY: kill is called with the exact PID returned by Child and a valid signal.
    unsafe {
        kill(pid, signal);
    }
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
    fn production_default_avoids_continuous_worker_recycling() {
        let options = StartOptions::parse(std::iter::empty()).unwrap();

        assert_eq!(options.max_requests, DEFAULT_MAX_REQUESTS);
    }

    #[test]
    fn large_recycle_limits_are_staggered_across_workers() {
        let mut options = StartOptions::parse(std::iter::empty()).unwrap();
        options.workers = 4;

        assert_eq!(staggered_max_requests(&options, 1), 10_000_000);
        assert_eq!(staggered_max_requests(&options, 4), 12_500_000);
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
