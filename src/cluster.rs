use std::ffi::{OsStr, OsString};
use std::fs;
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use crate::control_plane::{ClusterSnapshot, ControlPlane, SharedClusterSnapshot};
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
        })
    }
}

struct Worker {
    id: usize,
    generation: u64,
    child: Child,
    state_path: PathBuf,
    started: Instant,
    consecutive_failures: u32,
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
        .map(|address| ControlPlane::start(address, snapshot.clone()))
        .transpose()?;
    if let Some(control_plane) = &control_plane {
        eprintln!(
            "Pam control plane listening on http://{}",
            control_plane.address()
        );
    }

    let mut workers = spawn_generation(executable, &options, generation, runtime_directory.path())?;
    if let Err(error) = wait_generation_ready(&mut workers, options.startup_timeout).await {
        terminate_workers(&mut workers, options.graceful_timeout).await;
        set_cluster_health(&snapshot, false, false, generation, &workers);
        return Err(error);
    }
    set_cluster_health(&snapshot, true, true, generation, &workers);
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
                        )?);
                    }
                }
                set_cluster_health(&snapshot, true, true, generation, &workers);
            }
        }
    }

    set_cluster_health(&snapshot, true, false, generation, &workers);
    terminate_workers(&mut workers, options.graceful_timeout).await;
    set_cluster_health(&snapshot, false, false, generation, &workers);
    Ok(0)
}

fn spawn_generation(
    executable: &OsStr,
    options: &StartOptions,
    generation: u64,
    runtime_directory: &Path,
) -> Result<Vec<Worker>, String> {
    (1..=options.workers)
        .map(|id| spawn_worker(executable, options, id, generation, runtime_directory, 0))
        .collect()
}

fn spawn_worker(
    executable: &OsStr,
    options: &StartOptions,
    id: usize,
    generation: u64,
    runtime_directory: &Path,
    consecutive_failures: u32,
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
    let child = Command::new(executable)
        .arg("__worker")
        .arg(&options.script)
        .args(&options.script_args)
        .env("PAM_WORKER_ID", id.to_string())
        .env("PAM_WORKER_GENERATION", generation.to_string())
        .env("PAM_MAX_REQUESTS", max_requests.to_string())
        .env(WORKER_STATE_PATH_ENV, &state_path)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("cannot start worker {id}: {error}"))?;
    Ok(Worker {
        id,
        generation,
        child,
        state_path,
        started: Instant::now(),
        consecutive_failures,
    })
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
}
