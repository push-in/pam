use std::collections::{BTreeMap, HashSet, VecDeque};
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cluster::{
    MasterState, RELOAD_SIGNAL, STOP_SIGNAL, master_is_running, read_master_state, signal_master,
};

const MAX_RECORD_BYTES: u64 = 1_048_576;
const MAX_APPLICATIONS: usize = 1_024;
const DEFAULT_LOG_MAX_BYTES: u64 = 10 * 1024 * 1024;
const DEFAULT_LOG_RETAIN: usize = 5;
const MAX_LOG_RETAIN: usize = 100;
const MAX_DAEMON_MESSAGE_BYTES: usize = 16 * 1024;
const MAX_DAEMON_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_DEPLOY_HISTORY: usize = 50;
const MAX_DASHBOARD_BYTES: usize = 2 * 1024 * 1024;
const MAX_RESOURCE_HISTORY: usize = 120;
const RESOURCE_SAMPLE_INTERVAL: Duration = Duration::from_secs(60);
const SUPERVISION_INTERVAL: Duration = Duration::from_millis(250);
const DEFAULT_RESTART_DELAY_MILLIS: u64 = 250;
const DEFAULT_RESTART_BACKOFF_MAX_MILLIS: u64 = 15_000;
const DEFAULT_MAX_UNSTABLE_RESTARTS: u32 = 10;
const DEFAULT_MIN_UPTIME_MILLIS: u64 = 30_000;
const DEFAULT_HEALTH_INTERVAL_MILLIS: u64 = 5_000;
const DEFAULT_HEALTH_TIMEOUT_MILLIS: u64 = 1_000;
const DEFAULT_HEALTH_FAILURE_THRESHOLD: u32 = 3;
const MAX_HEALTH_START_PERIOD_MILLIS: u64 = 3_600_000;
const DEFAULT_SHUTDOWN_TIMEOUT_MILLIS: u64 = 20_000;
const MIN_SHUTDOWN_TIMEOUT_MILLIS: u64 = 100;
const MAX_SHUTDOWN_TIMEOUT_MILLIS: u64 = 300_000;
const FORCED_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_CONCURRENT_HEALTH_PROBES: usize = 64;
const MAX_ENVIRONMENT_FILE_BYTES: u64 = 64 * 1024;
const MAX_ENVIRONMENT_VARIABLES: usize = 256;
const MAX_DASHBOARD_REQUEST_BYTES: usize = 16 * 1024;
static LIVE_DASHBOARD_RUNNING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[repr(u8)]
enum DaemonOperation {
    Ping = 1,
    Stop = 2,
    Execute = 3,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DaemonRequest {
    schema_version: u8,
    operation_code: u8,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    arguments: Vec<String>,
    #[serde(default)]
    working_directory: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DaemonResponse {
    schema_version: u8,
    ok: bool,
    pid: u32,
    message: String,
    #[serde(default)]
    exit_code: u8,
    #[serde(default)]
    stdout: String,
    #[serde(default)]
    stderr: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[repr(u8)]
enum ApplicationKind {
    Runtime = 1,
    LaravelOctane = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum ApplicationState {
    Online = 1,
    Stopped = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum RolloutPhase {
    Stable = 1,
    Evaluating = 2,
    Promoted = 3,
    Aborted = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum RolloutDecision {
    Pending = 1,
    Promoted = 2,
    Aborted = 3,
    DeadlineAborted = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum LogStream {
    StandardOutput = 1,
    StandardError = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum ResourceAlertState {
    Healthy = 1,
    MemoryWarning = 2,
    TaskWarning = 3,
    MemoryAndTaskWarning = 4,
    Unavailable = 5,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum ResourceEnforcementState {
    Enforced = 1,
    NotRequested = 2,
    Unverified = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum RecoveryState {
    Healthy = 1,
    Backoff = 2,
    Stabilizing = 3,
    CircuitOpen = 4,
    Disabled = 5,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum HealthState {
    Disabled = 1,
    Healthy = 2,
    Failing = 3,
    Unhealthy = 4,
    Starting = 5,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApplicationRecord {
    schema_version: u8,
    name: String,
    kind_code: u8,
    working_directory: PathBuf,
    command: Vec<String>,
    master_state_file: PathBuf,
    stdout_log: PathBuf,
    stderr_log: PathBuf,
    #[serde(default = "default_log_max_bytes")]
    log_max_bytes: u64,
    #[serde(default = "default_log_retain")]
    log_retain: usize,
    #[serde(default)]
    memory_warning_bytes: Option<u64>,
    #[serde(default)]
    task_warning_count: Option<u64>,
    #[serde(default)]
    memory_max_bytes: Option<u64>,
    #[serde(default)]
    task_max_count: Option<u64>,
    #[serde(default)]
    environment_file: Option<PathBuf>,
    #[serde(default = "default_shutdown_timeout_millis")]
    shutdown_timeout_millis: u64,
    #[serde(default)]
    health_check_address: Option<SocketAddr>,
    #[serde(default)]
    health_check_path: Option<String>,
    #[serde(default = "default_health_interval_millis")]
    health_check_interval_millis: u64,
    #[serde(default = "default_health_timeout_millis")]
    health_check_timeout_millis: u64,
    #[serde(default)]
    health_check_start_period_millis: u64,
    #[serde(default = "default_health_failure_threshold")]
    health_check_failure_threshold: u32,
    #[serde(default)]
    consecutive_health_failures: u32,
    #[serde(default)]
    last_health_check_at_millis: Option<u64>,
    #[serde(default)]
    last_health_success_at_millis: Option<u64>,
    #[serde(default = "default_disabled_health_state")]
    health_state_code: u8,
    #[serde(default)]
    total_unhealthy_restart_count: u64,
    #[serde(default = "default_stopped_state")]
    desired_state_code: u8,
    #[serde(default)]
    auto_restart: bool,
    #[serde(default = "default_restart_delay_millis")]
    restart_delay_millis: u64,
    #[serde(default = "default_restart_backoff_max_millis")]
    restart_backoff_max_millis: u64,
    #[serde(default = "default_max_unstable_restarts")]
    max_unstable_restarts: u32,
    #[serde(default = "default_min_uptime_millis")]
    min_uptime_millis: u64,
    #[serde(default)]
    unstable_restart_count: u32,
    #[serde(default)]
    total_auto_restart_count: u64,
    #[serde(default)]
    next_restart_at_millis: Option<u64>,
    #[serde(default = "default_disabled_recovery_state")]
    recovery_state_code: u8,
    created_at_millis: u64,
}

#[derive(Debug)]
struct HealthProbeResult {
    name: String,
    pid: u32,
    address: SocketAddr,
    path: String,
    checked_at_millis: u64,
    success: bool,
}

#[derive(Default)]
struct MasterWatchers {
    _descriptors: Vec<OwnedFd>,
    poll_descriptors: Vec<libc::pollfd>,
}

impl MasterWatchers {
    fn exit_ready(&mut self) -> bool {
        if self.poll_descriptors.is_empty() {
            return false;
        }
        for descriptor in &mut self.poll_descriptors {
            descriptor.revents = 0;
        }
        let ready = unsafe {
            libc::poll(
                self.poll_descriptors.as_mut_ptr(),
                self.poll_descriptors.len() as _,
                0,
            )
        };
        ready > 0
            && self
                .poll_descriptors
                .iter()
                .any(|descriptor| descriptor.revents & libc::POLLIN != 0)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SavedApplication {
    name: String,
    desired_state_code: u8,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SavedProcessList {
    schema_version: u8,
    applications: Vec<SavedApplication>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeploymentHistory {
    schema_version: u8,
    name: String,
    entries: Vec<DeploymentEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeploymentEntry {
    release_directory: PathBuf,
    activated_at_millis: u64,
    event_kind_code: u8,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResourceHistory {
    schema_version: u8,
    name: String,
    entries: Vec<ResourceHistoryEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResourceHistoryEntry {
    observed_at_millis: u64,
    state_code: u8,
    workers: usize,
    rss_bytes: u64,
    tasks: u64,
    alert_state_code: u8,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveDashboardConfig {
    schema_version: u8,
    listen: SocketAddr,
    token_sha256: [u8; 32],
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveDashboardState {
    schema_version: u8,
    state_code: u8,
    pid: u32,
    process_start_ticks: u64,
    listen: SocketAddr,
    started_at_millis: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum DeploymentEventKind {
    Baseline = 1,
    Deploy = 2,
    Rollback = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum DeploymentAction {
    Activated = 1,
    RolledBack = 2,
    Unchanged = 3,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EcosystemConfig {
    schema_version: u8,
    applications: BTreeMap<String, EcosystemApplication>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EcosystemApplication {
    kind_code: u8,
    #[serde(default)]
    script: Option<PathBuf>,
    #[serde(default = "default_workers")]
    workers: usize,
    #[serde(default = "default_current_directory")]
    cwd: PathBuf,
    #[serde(default)]
    arguments: Vec<String>,
    #[serde(default = "default_true")]
    autostart: bool,
    #[serde(default)]
    env_file: Option<PathBuf>,
    #[serde(default = "default_shutdown_timeout_millis")]
    shutdown_timeout_millis: u64,
    #[serde(default)]
    health_check_url: Option<String>,
    #[serde(default = "default_health_interval_millis")]
    health_check_interval_millis: u64,
    #[serde(default = "default_health_timeout_millis")]
    health_check_timeout_millis: u64,
    #[serde(default)]
    health_check_start_period_millis: u64,
    #[serde(default = "default_health_failure_threshold")]
    health_check_failure_threshold: u32,
    #[serde(default = "default_true")]
    auto_restart: bool,
    #[serde(default = "default_restart_delay_millis")]
    restart_delay_millis: u64,
    #[serde(default = "default_restart_backoff_max_millis")]
    restart_backoff_max_millis: u64,
    #[serde(default = "default_max_unstable_restarts")]
    max_unstable_restarts: u32,
    #[serde(default = "default_min_uptime_millis")]
    min_uptime_millis: u64,
    #[serde(default)]
    memory_warning_bytes: Option<u64>,
    #[serde(default)]
    task_warning_count: Option<u64>,
    #[serde(default)]
    memory_max_bytes: Option<u64>,
    #[serde(default)]
    task_max_count: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum ReconcileAction {
    Created = 1,
    Unchanged = 2,
    Scaled = 3,
    Restarted = 4,
    Disabled = 5,
    PolicyUpdated = 6,
    ResourceLimitsUpdated = 7,
}

pub fn run(
    executable: &OsStr,
    command: &str,
    arguments: impl Iterator<Item = OsString>,
) -> Result<u8, String> {
    let arguments = arguments.collect::<Vec<_>>();
    if command == "__manager_local" {
        let mut arguments = arguments.into_iter();
        let local_command = required_utf8(arguments.next(), "__manager_local command")?;
        return run_local(executable, &local_command, arguments.collect());
    }
    let interactive = (command == "up" && arguments.iter().any(|value| value == "--attach"))
        || (command == "logs"
            && arguments
                .iter()
                .any(|value| value == "--follow" || value == "-f"));
    if daemon_managed_command(command) && !interactive {
        ensure_daemon(executable)?;
        return daemon_execute(command, arguments);
    }
    run_local(executable, command, arguments)
}

fn run_local(executable: &OsStr, command: &str, arguments: Vec<OsString>) -> Result<u8, String> {
    match command {
        "up" => up(executable, arguments),
        "ps" => list(arguments),
        "status" | "describe" => inspect(command, arguments),
        "reload" => signal(arguments, RELOAD_SIGNAL, "reloading"),
        "restart" => restart(executable, arguments),
        "scale" => scale(executable, arguments),
        "stop" => stop(arguments),
        "delete" => delete(arguments),
        "logs" => logs(arguments),
        "save" => save(arguments),
        "resurrect" => resurrect(executable, arguments),
        "startup" => startup(executable, arguments),
        "monit" => monit(arguments),
        "monit:history" => resource_history(arguments),
        "dashboard" => dashboard(arguments),
        "dashboard:start" => live_dashboard_start(executable, arguments),
        "dashboard:status" => live_dashboard_status(arguments),
        "dashboard:stop" => live_dashboard_stop(arguments),
        "apply" => apply_ecosystem(executable, arguments),
        "config:check" => check_ecosystem(arguments),
        "deploy" => deploy(executable, arguments),
        "deploy:history" => deployment_history(arguments),
        "rollback" => rollback(executable, arguments),
        "traffic:start" => traffic_start(executable, arguments),
        "traffic:set" => traffic_set(arguments),
        "traffic:promote" => traffic_promote(arguments),
        "traffic:abort" => traffic_abort(arguments),
        "traffic:status" => traffic_status(arguments),
        "traffic:evaluate" => traffic_evaluate(arguments),
        "traffic:stop" => traffic_stop(arguments),
        "__traffic_proxy" => traffic_proxy(arguments),
        "__manager_dashboard_server" => live_dashboard_server(arguments),
        "daemon" => daemon(executable, arguments),
        "__pamd" => daemon_serve(executable),
        _ => Err(format!(
            "unsupported PAM process-manager command: {command}"
        )),
    }
}

fn daemon_managed_command(command: &str) -> bool {
    matches!(
        command,
        "up" | "ps"
            | "status"
            | "describe"
            | "reload"
            | "restart"
            | "scale"
            | "stop"
            | "delete"
            | "save"
            | "resurrect"
            | "monit"
            | "monit:history"
            | "dashboard"
            | "dashboard:start"
            | "dashboard:status"
            | "dashboard:stop"
            | "deploy"
            | "deploy:history"
            | "rollback"
            | "traffic:start"
            | "traffic:set"
            | "traffic:promote"
            | "traffic:abort"
            | "traffic:status"
            | "traffic:evaluate"
            | "traffic:stop"
    )
}

fn traffic_start(executable: &OsStr, arguments: Vec<OsString>) -> Result<u8, String> {
    let mut name = None;
    let mut listen = None;
    let mut stable = None;
    let mut candidate = None;
    let mut weight = 0_u16;
    let mut deadline_seconds = 300_u64;
    let mut tls_certificate = None;
    let mut tls_private_key = None;
    let mut json = false;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--listen" => listen = Some(parse_socket(arguments.next(), "--listen")?),
            "--stable" => stable = Some(parse_socket(arguments.next(), "--stable")?),
            "--candidate" => candidate = Some(parse_socket(arguments.next(), "--candidate")?),
            "--weight-bps" => weight = parse_basis_points(arguments.next())?,
            "--deadline-seconds" => deadline_seconds = parse_rollout_deadline(arguments.next())?,
            "--tls-cert" => {
                tls_certificate = Some(PathBuf::from(required_utf8(
                    arguments.next(),
                    "--tls-cert",
                )?))
            }
            "--tls-key" => {
                tls_private_key = Some(PathBuf::from(required_utf8(arguments.next(), "--tls-key")?))
            }
            "--json" => json = true,
            option if option.starts_with('-') => {
                return Err(format!("unknown traffic:start option: {option}"));
            }
            _ if name.is_none() => name = Some(argument.to_string_lossy().into_owned()),
            _ => return Err("traffic:start accepts one name".to_owned()),
        }
    }
    let name = name.ok_or_else(|| "traffic:start requires a name".to_owned())?;
    validate_name(&name)?;
    let paths = ManagerPaths::load()?;
    let config_path = paths.traffic_config(&name);
    let state_path = paths.traffic_state(&name);
    let metrics_path = paths.traffic_metrics(&name);
    if state_path.exists()
        && read_master_state(&state_path).is_ok_and(|state| master_is_running(&state))
    {
        return Err(format!("traffic ingress {name:?} is already online"));
    }
    let config = crate::traffic::TrafficConfig {
        schema_version: 1,
        generation: 1,
        name: name.clone(),
        listen: listen.ok_or_else(|| "traffic:start requires --listen".to_owned())?,
        stable: stable.ok_or_else(|| "traffic:start requires --stable".to_owned())?,
        candidate,
        candidate_weight_basis_points: weight,
        rollout_phase_code: if candidate.is_some() && weight > 0 {
            RolloutPhase::Evaluating as u8
        } else {
            RolloutPhase::Stable as u8
        },
        rollout_deadline_millis: (candidate.is_some() && weight > 0)
            .then(|| epoch_millis().saturating_add(deadline_seconds.saturating_mul(1000))),
        last_rollout_decision_code: None,
        last_evaluated_at_millis: None,
        last_evaluated_candidate_requests: None,
        last_evaluated_candidate_errors: None,
        tls_certificate: tls_certificate
            .map(|path| {
                fs::canonicalize(&path).map_err(|error| {
                    format!("cannot resolve TLS certificate {}: {error}", path.display())
                })
            })
            .transpose()?,
        tls_private_key: tls_private_key
            .map(|path| {
                fs::canonicalize(&path).map_err(|error| {
                    format!("cannot resolve TLS private key {}: {error}", path.display())
                })
            })
            .transpose()?,
    };
    crate::traffic::validate_config(&config)?;
    write_private_json(&config_path, &config)?;
    let stdout = secure_append(&paths.logs.join(format!("traffic-{name}.out.log")))?;
    let stderr = secure_append(&paths.logs.join(format!("traffic-{name}.error.log")))?;
    let mut command = Command::new(executable);
    command
        .args(["__traffic_proxy"])
        .arg(&config_path)
        .arg(&state_path)
        .arg(&metrics_path)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr);
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        })
    };
    command
        .spawn()
        .map_err(|error| format!("cannot start traffic ingress: {error}"))?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let state = loop {
        if let Ok(state) = read_master_state(&state_path)
            && master_is_running(&state)
        {
            break state;
        }
        if Instant::now() >= deadline {
            return Err(format!("traffic ingress {name:?} did not become ready"));
        }
        thread::sleep(Duration::from_millis(50));
    };
    print_traffic(&config, Some(&state), json);
    Ok(0)
}

fn traffic_set(arguments: Vec<OsString>) -> Result<u8, String> {
    let mut name = None;
    let mut candidate = None;
    let mut weight = None;
    let mut deadline_seconds = 300_u64;
    let mut json = false;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--candidate" => candidate = Some(parse_socket(arguments.next(), "--candidate")?),
            "--weight-bps" => weight = Some(parse_basis_points(arguments.next())?),
            "--deadline-seconds" => deadline_seconds = parse_rollout_deadline(arguments.next())?,
            "--json" => json = true,
            option if option.starts_with('-') => {
                return Err(format!("unknown traffic:set option: {option}"));
            }
            _ if name.is_none() => name = Some(argument.to_string_lossy().into_owned()),
            _ => return Err("traffic:set accepts one name".to_owned()),
        }
    }
    let name = name.ok_or_else(|| "traffic:set requires a name".to_owned())?;
    validate_name(&name)?;
    let paths = ManagerPaths::load()?;
    let mut config = crate::traffic::read_config(&paths.traffic_config(&name))?;
    if let Some(candidate) = candidate {
        config.candidate = Some(candidate);
    }
    if let Some(weight) = weight {
        config.candidate_weight_basis_points = weight;
    }
    if config.candidate.is_some() && config.candidate_weight_basis_points > 0 {
        config.rollout_phase_code = RolloutPhase::Evaluating as u8;
        config.rollout_deadline_millis =
            Some(epoch_millis().saturating_add(deadline_seconds.saturating_mul(1000)));
        config.last_rollout_decision_code = None;
        config.last_evaluated_at_millis = None;
        config.last_evaluated_candidate_requests = None;
        config.last_evaluated_candidate_errors = None;
    }
    config.generation = config
        .generation
        .checked_add(1)
        .ok_or_else(|| "traffic generation exhausted".to_owned())?;
    crate::traffic::validate_config(&config)?;
    write_private_json(&paths.traffic_config(&name), &config)?;
    print_traffic(
        &config,
        read_master_state(&paths.traffic_state(&name)).ok().as_ref(),
        json,
    );
    Ok(0)
}

fn traffic_promote(arguments: Vec<OsString>) -> Result<u8, String> {
    update_traffic_terminal(arguments, true)
}

fn traffic_abort(arguments: Vec<OsString>) -> Result<u8, String> {
    update_traffic_terminal(arguments, false)
}

fn update_traffic_terminal(arguments: Vec<OsString>, promote: bool) -> Result<u8, String> {
    let (name, json) = parse_name_json(
        arguments,
        if promote {
            "traffic:promote"
        } else {
            "traffic:abort"
        },
    )?;
    let paths = ManagerPaths::load()?;
    let mut config = crate::traffic::read_config(&paths.traffic_config(&name))?;
    if promote {
        config.stable = config
            .candidate
            .ok_or_else(|| "traffic ingress has no candidate to promote".to_owned())?;
    }
    config.candidate = None;
    config.candidate_weight_basis_points = 0;
    config.rollout_phase_code = if promote {
        RolloutPhase::Promoted as u8
    } else {
        RolloutPhase::Aborted as u8
    };
    config.rollout_deadline_millis = None;
    config.last_rollout_decision_code = Some(if promote {
        RolloutDecision::Promoted as u8
    } else {
        RolloutDecision::Aborted as u8
    });
    config.generation = config
        .generation
        .checked_add(1)
        .ok_or_else(|| "traffic generation exhausted".to_owned())?;
    write_private_json(&paths.traffic_config(&name), &config)?;
    print_traffic(
        &config,
        read_master_state(&paths.traffic_state(&name)).ok().as_ref(),
        json,
    );
    Ok(0)
}

fn traffic_status(arguments: Vec<OsString>) -> Result<u8, String> {
    let (name, json) = parse_name_json(arguments, "traffic:status")?;
    let paths = ManagerPaths::load()?;
    let config = crate::traffic::read_config(&paths.traffic_config(&name))?;
    let state = read_master_state(&paths.traffic_state(&name)).ok();
    let online = state.as_ref().is_some_and(master_is_running);
    print_traffic(&config, state.as_ref(), json);
    Ok(if online { 0 } else { 1 })
}

fn traffic_evaluate(arguments: Vec<OsString>) -> Result<u8, String> {
    let mut name = None;
    let mut minimum_requests = None;
    let mut maximum_error_basis_points = None;
    let mut json = false;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--min-candidate-requests" => {
                minimum_requests = Some(required_positive_u64(
                    arguments.next(),
                    "--min-candidate-requests",
                )?)
            }
            "--max-candidate-error-bps" => {
                maximum_error_basis_points = Some(parse_basis_points(arguments.next())?)
            }
            "--json" => json = true,
            option if option.starts_with('-') => {
                return Err(format!("unknown traffic:evaluate option: {option}"));
            }
            _ if name.is_none() => name = Some(argument.to_string_lossy().into_owned()),
            _ => return Err("traffic:evaluate accepts one name".to_owned()),
        }
    }
    let name = name.ok_or_else(|| "traffic:evaluate requires a name".to_owned())?;
    validate_name(&name)?;
    let minimum_requests = minimum_requests
        .ok_or_else(|| "traffic:evaluate requires --min-candidate-requests".to_owned())?;
    let maximum_error_basis_points = maximum_error_basis_points
        .ok_or_else(|| "traffic:evaluate requires --max-candidate-error-bps".to_owned())?;
    let paths = ManagerPaths::load()?;
    let config_path = paths.traffic_config(&name);
    let mut config = crate::traffic::read_config(&config_path)?;
    if config.candidate.is_none() || config.rollout_phase_code != RolloutPhase::Evaluating as u8 {
        return Err("traffic ingress has no rollout under evaluation".to_owned());
    }
    let metrics = crate::traffic::read_metrics(&paths.traffic_metrics(&name))?;
    if metrics.generation != config.generation {
        return Err("rollout metrics have not reached the active generation".to_owned());
    }
    let expired = config
        .rollout_deadline_millis
        .is_some_and(|deadline| epoch_millis() >= deadline);
    let error_basis_points = if metrics.candidate_requests == 0 {
        0
    } else {
        let requests = u128::from(metrics.candidate_requests);
        (u128::from(metrics.candidate_errors) * 10_000).div_ceil(requests) as u16
    };
    let decision = if expired {
        RolloutDecision::DeadlineAborted
    } else if metrics.candidate_requests < minimum_requests {
        RolloutDecision::Pending
    } else if error_basis_points <= maximum_error_basis_points {
        RolloutDecision::Promoted
    } else {
        RolloutDecision::Aborted
    };
    config.last_rollout_decision_code = Some(decision as u8);
    config.last_evaluated_at_millis = Some(epoch_millis());
    config.last_evaluated_candidate_requests = Some(metrics.candidate_requests);
    config.last_evaluated_candidate_errors = Some(metrics.candidate_errors);
    if decision != RolloutDecision::Pending {
        if decision == RolloutDecision::Promoted {
            config.stable = config.candidate.expect("candidate checked above");
            config.rollout_phase_code = RolloutPhase::Promoted as u8;
        } else {
            config.rollout_phase_code = RolloutPhase::Aborted as u8;
        }
        config.candidate = None;
        config.candidate_weight_basis_points = 0;
        config.rollout_deadline_millis = None;
        config.generation = config
            .generation
            .checked_add(1)
            .ok_or_else(|| "traffic generation exhausted".to_owned())?;
    }
    write_private_json(&config_path, &config)?;
    if json {
        println!(
            "{}",
            serde_json::json!({"schemaVersion":1,"name":name,"decisionCode":decision as u8,"candidateRequests":metrics.candidate_requests,"candidateErrors":metrics.candidate_errors,"candidateErrorBasisPoints":error_basis_points,"generation":config.generation})
        );
    } else {
        println!("Rollout {name}: decision {}", decision as u8);
    }
    Ok(if decision == RolloutDecision::Pending {
        1
    } else {
        0
    })
}

fn traffic_stop(arguments: Vec<OsString>) -> Result<u8, String> {
    let (name, json) = parse_name_json(arguments, "traffic:stop")?;
    let paths = ManagerPaths::load()?;
    if let Ok(state) = read_master_state(&paths.traffic_state(&name))
        && master_is_running(&state)
    {
        signal_master(&state, STOP_SIGNAL)?;
        let deadline = Instant::now() + Duration::from_secs(20);
        while master_is_running(&state) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        if master_is_running(&state) {
            return Err(format!("traffic ingress {name:?} did not stop"));
        }
    }
    if json {
        println!(
            "{}",
            serde_json::json!({"schemaVersion":1,"name":name,"stateCode":ApplicationState::Stopped as u8})
        );
    } else {
        println!("Stopped traffic ingress {name}");
    }
    Ok(0)
}

fn traffic_proxy(arguments: Vec<OsString>) -> Result<u8, String> {
    match arguments.as_slice() {
        [config, state, metrics] => crate::traffic::run(
            PathBuf::from(config),
            PathBuf::from(state),
            PathBuf::from(metrics),
        ),
        _ => Err("__traffic_proxy requires config, state and metrics paths".to_owned()),
    }
}

fn parse_socket(value: Option<OsString>, option: &str) -> Result<std::net::SocketAddr, String> {
    required_utf8(value, option)?
        .parse()
        .map_err(|_| format!("{option} requires IP:port"))
}

fn parse_health_check_url(value: &str) -> Result<(SocketAddr, String), String> {
    let remainder = value
        .strip_prefix("http://")
        .ok_or_else(|| "health check URL must use http://".to_owned())?;
    let (authority, path) = remainder
        .split_once('/')
        .map_or((remainder, "/".to_owned()), |(authority, path)| {
            (authority, format!("/{path}"))
        });
    let address = authority
        .parse::<SocketAddr>()
        .map_err(|_| "health check URL requires an explicit IP address and port".to_owned())?;
    if !address.ip().is_loopback() {
        return Err("health check URL must target a loopback address".to_owned());
    }
    let lowercase_path = path.to_ascii_lowercase();
    if path.len() > 1024
        || !path.starts_with('/')
        || path.contains(['\0', '\r', '\n', ' '])
        || lowercase_path.contains("%0d")
        || lowercase_path.contains("%0a")
    {
        return Err("health check path is invalid or exceeds 1024 bytes".to_owned());
    }
    Ok((address, path))
}

fn validate_health_policy(
    url: Option<&str>,
    interval_millis: u64,
    timeout_millis: u64,
    failure_threshold: u32,
) -> Result<Option<(SocketAddr, String)>, String> {
    if !(250..=3_600_000).contains(&interval_millis) {
        return Err("health check interval must be 250-3600000 milliseconds".to_owned());
    }
    if !(50..=5_000).contains(&timeout_millis) || timeout_millis >= interval_millis {
        return Err(
            "health check timeout must be 50-5000 milliseconds and less than the interval"
                .to_owned(),
        );
    }
    if !(1..=100).contains(&failure_threshold) {
        return Err("health check failure threshold must be 1-100".to_owned());
    }
    url.map(parse_health_check_url).transpose()
}

fn parse_basis_points(value: Option<OsString>) -> Result<u16, String> {
    required_utf8(value, "--weight-bps")?
        .parse::<u16>()
        .ok()
        .filter(|value| *value <= 10_000)
        .ok_or_else(|| "--weight-bps requires an integer from 0 to 10000".to_owned())
}

fn parse_rollout_deadline(value: Option<OsString>) -> Result<u64, String> {
    required_utf8(value, "--deadline-seconds")?
        .parse::<u64>()
        .ok()
        .filter(|value| (1..=604_800).contains(value))
        .ok_or_else(|| "--deadline-seconds requires an integer from 1 to 604800".to_owned())
}

fn print_traffic(config: &crate::traffic::TrafficConfig, state: Option<&MasterState>, json: bool) {
    let online = state.is_some_and(master_is_running);
    if json {
        let metrics = ManagerPaths::load().ok().and_then(|paths| {
            crate::traffic::read_metrics(&paths.traffic_metrics(&config.name)).ok()
        });
        println!(
            "{}",
            serde_json::json!({"schemaVersion":1,"name":config.name,"stateCode":if online { ApplicationState::Online as u8 } else { ApplicationState::Stopped as u8 },"pid":state.map(|value| value.pid),"listen":config.listen,"stable":config.stable,"candidate":config.candidate,"candidateWeightBasisPoints":config.candidate_weight_basis_points,"generation":config.generation,"rolloutPhaseCode":config.rollout_phase_code,"rolloutDeadlineMillis":config.rollout_deadline_millis,"lastRolloutDecisionCode":config.last_rollout_decision_code,"lastEvaluatedAtMillis":config.last_evaluated_at_millis,"lastEvaluatedCandidateRequests":config.last_evaluated_candidate_requests,"lastEvaluatedCandidateErrors":config.last_evaluated_candidate_errors,"tlsEnabled":config.tls_certificate.is_some(),"metrics":metrics})
        );
    } else {
        println!(
            "{} {}: {} -> stable {}, candidate {:?} @ {} bps",
            config.name,
            if online { "online" } else { "stopped" },
            config.listen,
            config.stable,
            config.candidate,
            config.candidate_weight_basis_points
        );
    }
}

fn apply_ecosystem(executable: &OsStr, arguments: Vec<OsString>) -> Result<u8, String> {
    let (path, json) = parse_config_arguments(arguments, "apply")?;
    let (config, root) = load_ecosystem(&path)?;
    let paths = ManagerPaths::load()?;
    let mut results = Vec::new();
    for (name, application) in config.applications {
        validate_ecosystem_application(&root, &name, &application)?;
        let environment_file = application
            .env_file
            .as_deref()
            .map(|path| resolve_environment_file(&root, path))
            .transpose()?;
        let health_check = validate_health_policy(
            application.health_check_url.as_deref(),
            application.health_check_interval_millis,
            application.health_check_timeout_millis,
            application.health_check_failure_threshold,
        )?;
        validate_shutdown_policy(application.shutdown_timeout_millis)?;
        validate_health_start_period(
            application.health_check_start_period_millis,
            health_check.is_some(),
        )?;
        let action = if !application.autostart {
            let record_path = paths.application(&name);
            if record_path.exists() {
                let mut command = Command::new(executable);
                command.args(["stop", &name]);
                run_reconcile_command(command, &name)?;
            }
            ReconcileAction::Disabled
        } else {
            let record_path = paths.application(&name);
            if !record_path.exists() {
                let cwd = scoped_cwd(&root, &application.cwd)?;
                let mut command = Command::new(executable);
                command
                    .current_dir(cwd)
                    .args(["up", "--name", &name, "--workers"]);
                command.arg(application.workers.to_string());
                if let Some(path) = environment_file.as_deref() {
                    command.arg("--env-file").arg(path);
                }
                if let Some(url) = application.health_check_url.as_deref() {
                    command
                        .arg("--health-check-url")
                        .arg(url)
                        .arg("--health-check-interval-ms")
                        .arg(application.health_check_interval_millis.to_string())
                        .arg("--health-check-timeout-ms")
                        .arg(application.health_check_timeout_millis.to_string())
                        .arg("--health-check-start-period-ms")
                        .arg(application.health_check_start_period_millis.to_string())
                        .arg("--health-check-failures")
                        .arg(application.health_check_failure_threshold.to_string());
                }
                if !application.auto_restart {
                    command.arg("--no-autorestart");
                }
                command
                    .arg("--shutdown-timeout-ms")
                    .arg(application.shutdown_timeout_millis.to_string())
                    .arg("--restart-delay-ms")
                    .arg(application.restart_delay_millis.to_string())
                    .arg("--restart-backoff-max-ms")
                    .arg(application.restart_backoff_max_millis.to_string())
                    .arg("--max-unstable-restarts")
                    .arg(application.max_unstable_restarts.to_string())
                    .arg("--min-uptime-ms")
                    .arg(application.min_uptime_millis.to_string());
                if let Some(value) = application.memory_warning_bytes {
                    command.args(["--memory-warning-bytes", &value.to_string()]);
                }
                if let Some(value) = application.task_warning_count {
                    command.args(["--task-warning-count", &value.to_string()]);
                }
                if let Some(value) = application.memory_max_bytes {
                    command.args(["--memory-max-bytes", &value.to_string()]);
                }
                if let Some(value) = application.task_max_count {
                    command.args(["--task-max-count", &value.to_string()]);
                }
                if application.kind_code == ApplicationKind::Runtime as u8 {
                    command.arg(
                        application
                            .script
                            .as_deref()
                            .unwrap_or(Path::new("index.php")),
                    );
                }
                if !application.arguments.is_empty() {
                    command.arg("--").args(&application.arguments);
                }
                run_reconcile_command(command, &name)?;
                ReconcileAction::Created
            } else {
                let mut record = read_record(&record_path)?;
                let limit_updated = record.memory_max_bytes != application.memory_max_bytes
                    || record.task_max_count != application.task_max_count;
                let environment_updated = record.environment_file != environment_file;
                let health_updated = record.health_check_address
                    != health_check.as_ref().map(|(address, _)| *address)
                    || record.health_check_path
                        != health_check.as_ref().map(|(_, path)| path.clone())
                    || record.health_check_interval_millis
                        != application.health_check_interval_millis
                    || record.health_check_timeout_millis
                        != application.health_check_timeout_millis
                    || record.health_check_start_period_millis
                        != application.health_check_start_period_millis
                    || record.health_check_failure_threshold
                        != application.health_check_failure_threshold;
                let policy_updated = record.memory_warning_bytes
                    != application.memory_warning_bytes
                    || record.task_warning_count != application.task_warning_count
                    || record.auto_restart != application.auto_restart
                    || record.restart_delay_millis != application.restart_delay_millis
                    || record.restart_backoff_max_millis != application.restart_backoff_max_millis
                    || record.max_unstable_restarts != application.max_unstable_restarts
                    || record.min_uptime_millis != application.min_uptime_millis
                    || record.shutdown_timeout_millis != application.shutdown_timeout_millis;
                if policy_updated || limit_updated || environment_updated || health_updated {
                    record.memory_warning_bytes = application.memory_warning_bytes;
                    record.task_warning_count = application.task_warning_count;
                    record.memory_max_bytes = application.memory_max_bytes;
                    record.task_max_count = application.task_max_count;
                    record.auto_restart = application.auto_restart;
                    record.restart_delay_millis = application.restart_delay_millis;
                    record.restart_backoff_max_millis = application.restart_backoff_max_millis;
                    record.max_unstable_restarts = application.max_unstable_restarts;
                    record.min_uptime_millis = application.min_uptime_millis;
                    record.shutdown_timeout_millis = application.shutdown_timeout_millis;
                    record.environment_file = environment_file;
                    record.health_check_address =
                        health_check.as_ref().map(|(address, _)| *address);
                    record.health_check_path = health_check.map(|(_, path)| path);
                    record.health_check_interval_millis = application.health_check_interval_millis;
                    record.health_check_timeout_millis = application.health_check_timeout_millis;
                    record.health_check_start_period_millis =
                        application.health_check_start_period_millis;
                    record.health_check_failure_threshold =
                        application.health_check_failure_threshold;
                    record.consecutive_health_failures = 0;
                    record.last_health_check_at_millis = None;
                    record.health_state_code = initial_health_state(
                        record.health_check_address.is_some(),
                        record.health_check_start_period_millis,
                    );
                    if !record.auto_restart {
                        record.recovery_state_code = RecoveryState::Disabled as u8;
                        record.next_restart_at_millis = None;
                    }
                    write_record(&record_path, &record)?;
                }
                let state = running_state(&record);
                if (limit_updated || environment_updated || health_updated)
                    && state.as_ref().is_some_and(master_is_running)
                {
                    restart_record(executable, &record, false, false)?;
                    if limit_updated {
                        ReconcileAction::ResourceLimitsUpdated
                    } else {
                        ReconcileAction::Restarted
                    }
                } else if !state.as_ref().is_some_and(master_is_running) {
                    let mut command = Command::new(executable);
                    command.args(["restart", &name]);
                    run_reconcile_command(command, &name)?;
                    ReconcileAction::Restarted
                } else if state
                    .as_ref()
                    .is_some_and(|state| state.workers != application.workers)
                {
                    let mut command = Command::new(executable);
                    command.args(["scale", &name, &application.workers.to_string()]);
                    run_reconcile_command(command, &name)?;
                    ReconcileAction::Scaled
                } else if policy_updated {
                    ReconcileAction::PolicyUpdated
                } else {
                    ReconcileAction::Unchanged
                }
            }
        };
        results.push(serde_json::json!({"name":name,"actionCode":action as u8}));
    }
    if json {
        println!(
            "{}",
            serde_json::json!({"schemaVersion":1,"results":results})
        );
    } else {
        println!(
            "Applied {} applications from {}",
            results.len(),
            path.display()
        );
        for result in results {
            println!(
                "{}\taction {}",
                result["name"].as_str().unwrap_or("?"),
                result["actionCode"]
            );
        }
    }
    Ok(0)
}

fn check_ecosystem(arguments: Vec<OsString>) -> Result<u8, String> {
    let (path, json) = parse_config_arguments(arguments, "config:check")?;
    let (config, root) = load_ecosystem(&path)?;
    for (name, application) in &config.applications {
        validate_ecosystem_application(&root, name, application)?;
    }
    if json {
        println!(
            "{}",
            serde_json::json!({"schemaVersion":1,"valid":true,"applications":config.applications.len()})
        );
    } else {
        println!(
            "{} is valid ({} applications)",
            path.display(),
            config.applications.len()
        );
    }
    Ok(0)
}

fn parse_config_arguments(
    arguments: Vec<OsString>,
    command: &str,
) -> Result<(PathBuf, bool), String> {
    let mut path = None;
    let mut json = false;
    for argument in arguments {
        if argument == "--json" {
            json = true;
        } else if argument.to_string_lossy().starts_with('-') || path.is_some() {
            return Err(format!(
                "{command} accepts a pam.toml path and optional --json"
            ));
        } else {
            path = Some(PathBuf::from(argument));
        }
    }
    Ok((path.unwrap_or_else(|| PathBuf::from("pam.toml")), json))
}

fn load_ecosystem(path: &Path) -> Result<(EcosystemConfig, PathBuf), String> {
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(format!(
            "refusing symlink ecosystem configuration {}",
            path.display()
        ));
    }
    let path = fs::canonicalize(path)
        .map_err(|error| format!("cannot resolve {}: {error}", path.display()))?;
    let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_RECORD_BYTES {
        return Err(format!(
            "invalid ecosystem configuration {}",
            path.display()
        ));
    }
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let config: EcosystemConfig =
        toml::from_str(&text).map_err(|error| format!("invalid pam.toml: {error}"))?;
    if config.schema_version != 1
        || config.applications.is_empty()
        || config.applications.len() > MAX_APPLICATIONS
    {
        return Err("pam.toml requires schema_version = 1 and 1-1024 applications".to_owned());
    }
    let root = path
        .parent()
        .ok_or_else(|| "pam.toml has no parent directory".to_owned())?
        .to_path_buf();
    Ok((config, root))
}

fn validate_ecosystem_application(
    root: &Path,
    name: &str,
    application: &EcosystemApplication,
) -> Result<(), String> {
    validate_name(name)?;
    if !matches!(application.kind_code, 1 | 2) {
        return Err(format!("application {name:?} kind_code must be 1 or 2"));
    }
    if application.workers == 0 || application.workers > 256 {
        return Err(format!("application {name:?} workers must be 1-256"));
    }
    validate_resource_policy(
        application.memory_warning_bytes,
        application.task_warning_count,
        application.memory_max_bytes,
        application.task_max_count,
        &format!("application {name:?}"),
    )?;
    validate_recovery_policy(
        application.restart_delay_millis,
        application.restart_backoff_max_millis,
        application.max_unstable_restarts,
        application.min_uptime_millis,
    )?;
    validate_shutdown_policy(application.shutdown_timeout_millis)?;
    validate_health_start_period(
        application.health_check_start_period_millis,
        application.health_check_url.is_some(),
    )?;
    if let Some(path) = application.env_file.as_deref() {
        resolve_environment_file(root, path)?;
    }
    validate_health_policy(
        application.health_check_url.as_deref(),
        application.health_check_interval_millis,
        application.health_check_timeout_millis,
        application.health_check_failure_threshold,
    )?;
    if application.health_check_url.is_some() && !application.auto_restart {
        return Err(format!(
            "application {name:?} health checks require auto_restart = true"
        ));
    }
    if application.kind_code == ApplicationKind::LaravelOctane as u8 && application.script.is_some()
    {
        return Err(format!("Laravel application {name:?} cannot set script"));
    }
    let cwd = scoped_cwd(root, &application.cwd)?;
    if application.kind_code == ApplicationKind::LaravelOctane as u8
        && !cwd.join("artisan").is_file()
    {
        return Err(format!(
            "Laravel application {name:?} requires artisan in {}",
            cwd.display()
        ));
    }
    if application
        .arguments
        .iter()
        .any(|value| value.contains(['\0', '\n', '\r']))
    {
        return Err(format!(
            "application {name:?} contains invalid argument controls"
        ));
    }
    Ok(())
}

fn scoped_cwd(root: &Path, cwd: &Path) -> Result<PathBuf, String> {
    let cwd = fs::canonicalize(root.join(cwd))
        .map_err(|error| format!("cannot resolve service cwd: {error}"))?;
    if !cwd.starts_with(root) {
        return Err("service cwd must stay beneath the pam.toml directory".to_owned());
    }
    Ok(cwd)
}

fn resolve_environment_file(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let unresolved = root.join(path);
    if fs::symlink_metadata(&unresolved).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(format!(
            "refusing symlink environment file {}",
            unresolved.display()
        ));
    }
    let path = fs::canonicalize(&unresolved).map_err(|error| {
        format!(
            "cannot resolve environment file {}: {error}",
            path.display()
        )
    })?;
    load_environment_file(&path)?;
    Ok(path)
}

fn load_environment_file(path: &Path) -> Result<BTreeMap<String, String>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "cannot inspect environment file {}: {error}",
            path.display()
        )
    })?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_ENVIRONMENT_FILE_BYTES {
        return Err(format!(
            "environment file must be a regular file no larger than {MAX_ENVIRONMENT_FILE_BYTES} bytes"
        ));
    }
    if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
        return Err(
            "environment file must be owned by the current user and mode 0600 or stricter"
                .to_owned(),
        );
    }
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("cannot read environment file {}: {error}", path.display()))?;
    let mut environment = BTreeMap::new();
    for (index, line) in contents.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("environment file line {} requires KEY=VALUE", index + 1))?;
        let key = key.trim();
        if key.is_empty()
            || !key
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(format!(
                "environment file line {} has an invalid key",
                index + 1
            ));
        }
        if matches!(key, "PAM_MANAGER_STATE_DIR" | "PAM_MANAGER_RUNTIME_DIR") {
            return Err(format!(
                "environment file line {} overrides reserved manager state",
                index + 1
            ));
        }
        let value = value.trim();
        let value = match (value.as_bytes().first(), value.as_bytes().last()) {
            (Some(b'\''), Some(b'\'')) | (Some(b'\"'), Some(b'\"')) if value.len() >= 2 => {
                &value[1..value.len() - 1]
            }
            (Some(b'\'' | b'\"'), _) | (_, Some(b'\'' | b'\"')) => {
                return Err(format!(
                    "environment file line {} has unmatched quotes",
                    index + 1
                ));
            }
            _ => value,
        };
        if value.contains(['\0', '\n', '\r']) {
            return Err(format!(
                "environment file line {} has control characters",
                index + 1
            ));
        }
        environment.insert(key.to_owned(), value.to_owned());
        if environment.len() > MAX_ENVIRONMENT_VARIABLES {
            return Err(format!(
                "environment file cannot exceed {MAX_ENVIRONMENT_VARIABLES} variables"
            ));
        }
    }
    Ok(environment)
}

fn apply_environment_file(command: &mut Command, path: Option<&Path>) -> Result<(), String> {
    if let Some(path) = path {
        command.envs(load_environment_file(path)?);
    }
    Ok(())
}

fn run_reconcile_command(mut command: Command, name: &str) -> Result<(), String> {
    let output = command
        .output()
        .map_err(|error| format!("cannot reconcile {name:?}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cannot reconcile {name:?}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

const fn default_workers() -> usize {
    1
}
fn default_current_directory() -> PathBuf {
    PathBuf::from(".")
}
const fn default_true() -> bool {
    true
}
const fn default_stopped_state() -> u8 {
    ApplicationState::Stopped as u8
}
const fn default_restart_delay_millis() -> u64 {
    DEFAULT_RESTART_DELAY_MILLIS
}
const fn default_restart_backoff_max_millis() -> u64 {
    DEFAULT_RESTART_BACKOFF_MAX_MILLIS
}
const fn default_max_unstable_restarts() -> u32 {
    DEFAULT_MAX_UNSTABLE_RESTARTS
}
const fn default_min_uptime_millis() -> u64 {
    DEFAULT_MIN_UPTIME_MILLIS
}
const fn default_disabled_recovery_state() -> u8 {
    RecoveryState::Disabled as u8
}
const fn default_health_interval_millis() -> u64 {
    DEFAULT_HEALTH_INTERVAL_MILLIS
}
const fn default_health_timeout_millis() -> u64 {
    DEFAULT_HEALTH_TIMEOUT_MILLIS
}
const fn default_health_failure_threshold() -> u32 {
    DEFAULT_HEALTH_FAILURE_THRESHOLD
}
const fn default_shutdown_timeout_millis() -> u64 {
    DEFAULT_SHUTDOWN_TIMEOUT_MILLIS
}
const fn default_disabled_health_state() -> u8 {
    HealthState::Disabled as u8
}

fn scale(executable: &OsStr, arguments: Vec<OsString>) -> Result<u8, String> {
    let mut arguments = arguments.into_iter();
    let name = required_utf8(arguments.next(), "scale application")?;
    validate_name(&name)?;
    let workers = required_positive(arguments.next(), "scale workers")?;
    if workers > 256 {
        return Err("scale workers cannot exceed 256".to_owned());
    }
    let json = match arguments.next() {
        None => false,
        Some(value) if value == "--json" && arguments.next().is_none() => true,
        _ => return Err("scale requires NAME WORKERS and optional --json".to_owned()),
    };
    let paths = ManagerPaths::load()?;
    let path = paths.application(&name);
    let mut record = read_record(&path)?;
    set_command_option(&mut record.command, "--workers", &workers.to_string());
    write_record(&path, &record)?;
    restart_record(executable, &record, json, true)?;
    Ok(0)
}

fn save(arguments: Vec<OsString>) -> Result<u8, String> {
    let json = parse_json_only(arguments, "save")?;
    let paths = ManagerPaths::load()?;
    let records = read_all_records(&paths)?;
    let saved = SavedProcessList {
        schema_version: 1,
        applications: records
            .iter()
            .map(|record| SavedApplication {
                name: record.name.clone(),
                desired_state_code: record.desired_state_code,
            })
            .collect(),
    };
    write_private_json(&paths.base.join("dump.json"), &saved)?;
    if json {
        println!(
            "{}",
            serde_json::to_string(&saved).map_err(|error| error.to_string())?
        );
    } else {
        println!("Saved {} applications", saved.applications.len());
    }
    Ok(0)
}

fn resurrect(executable: &OsStr, arguments: Vec<OsString>) -> Result<u8, String> {
    let json = parse_json_only(arguments, "resurrect")?;
    let restarted = resurrect_saved(executable)?;
    if json {
        println!(
            "{}",
            serde_json::json!({"schemaVersion":1,"resurrected":restarted})
        );
    } else {
        println!("Resurrected {} applications", restarted.len());
    }
    Ok(0)
}

fn resurrect_saved(executable: &OsStr) -> Result<Vec<String>, String> {
    let paths = ManagerPaths::load()?;
    let dump_path = paths.base.join("dump.json");
    let dump: SavedProcessList = read_private_json(&dump_path)?;
    if dump.schema_version != 1 || dump.applications.len() > MAX_APPLICATIONS {
        return Err("unsupported or oversized saved process list".to_owned());
    }
    let mut restarted = Vec::new();
    for saved in dump.applications {
        validate_name(&saved.name)?;
        if saved.desired_state_code != ApplicationState::Online as u8 {
            continue;
        }
        let path = paths.application(&saved.name);
        let mut record = read_record(&path)?;
        reset_recovery(&mut record);
        write_record(&path, &record)?;
        if running_state(&record)
            .as_ref()
            .is_some_and(master_is_running)
        {
            continue;
        }
        restart_record(executable, &record, false, false)?;
        restarted.push(saved.name);
    }
    Ok(restarted)
}

fn reset_recovery(record: &mut ApplicationRecord) {
    record.desired_state_code = ApplicationState::Online as u8;
    record.unstable_restart_count = 0;
    record.next_restart_at_millis = None;
    record.recovery_state_code = if record.auto_restart {
        RecoveryState::Healthy as u8
    } else {
        RecoveryState::Disabled as u8
    };
    record.consecutive_health_failures = 0;
    record.health_state_code = initial_health_state(
        record.health_check_address.is_some(),
        record.health_check_start_period_millis,
    );
}

const fn initial_health_state(configured: bool, start_period_millis: u64) -> u8 {
    if !configured {
        HealthState::Disabled as u8
    } else if start_period_millis > 0 {
        HealthState::Starting as u8
    } else {
        HealthState::Healthy as u8
    }
}

fn health_probe(address: SocketAddr, path: &str, timeout: Duration) -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(&address, timeout) else {
        return false;
    };
    if stream.set_read_timeout(Some(timeout)).is_err()
        || stream.set_write_timeout(Some(timeout)).is_err()
        || write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
        )
        .is_err()
    {
        return false;
    }
    let mut bytes = Vec::new();
    if Read::by_ref(&mut stream)
        .take(8193)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() > 8192
    {
        return false;
    }
    let Some(line) = bytes.split(|byte| *byte == b'\n').next() else {
        return false;
    };
    let Ok(line) = std::str::from_utf8(line) else {
        return false;
    };
    let mut fields = line.strip_suffix('\r').unwrap_or(line).split_whitespace();
    matches!(fields.next(), Some("HTTP/1.0" | "HTTP/1.1"))
        && fields
            .next()
            .and_then(|code| code.parse::<u16>().ok())
            .is_some_and(|code| (200..300).contains(&code))
}

fn health_start_period_elapsed(state: &MasterState, start_period_millis: u64, now: u64) -> bool {
    now.saturating_sub(state.started_at_millis) >= start_period_millis
}

fn schedule_health_probes(
    paths: &ManagerPaths,
    sender: &mpsc::Sender<HealthProbeResult>,
    in_flight: &mut HashSet<String>,
) -> Result<(), String> {
    let now = epoch_millis();
    for record in read_all_records(paths)? {
        if in_flight.len() >= MAX_CONCURRENT_HEALTH_PROBES {
            break;
        }
        let (Some(address), Some(path), Some(state)) = (
            record.health_check_address,
            record.health_check_path.clone(),
            running_state(&record).filter(master_is_running),
        ) else {
            continue;
        };
        if record.desired_state_code != ApplicationState::Online as u8
            || in_flight.contains(&record.name)
            || !health_start_period_elapsed(&state, record.health_check_start_period_millis, now)
            || record.last_health_check_at_millis.is_some_and(|checked| {
                now.saturating_sub(checked) < record.health_check_interval_millis
            })
        {
            continue;
        }
        in_flight.insert(record.name.clone());
        let sender = sender.clone();
        let name = record.name;
        let timeout = Duration::from_millis(record.health_check_timeout_millis);
        thread::spawn(move || {
            let success = health_probe(address, &path, timeout);
            let _ = sender.send(HealthProbeResult {
                name,
                pid: state.pid,
                address,
                path,
                checked_at_millis: epoch_millis(),
                success,
            });
        });
    }
    Ok(())
}

fn apply_health_probe_results(
    paths: &ManagerPaths,
    receiver: &mpsc::Receiver<HealthProbeResult>,
    in_flight: &mut HashSet<String>,
) -> Result<(), String> {
    while let Ok(result) = receiver.try_recv() {
        in_flight.remove(&result.name);
        let path = paths.application(&result.name);
        let Ok(mut record) = read_record(&path) else {
            continue;
        };
        let state = running_state(&record);
        if record.desired_state_code != ApplicationState::Online as u8
            || record.health_check_address != Some(result.address)
            || record.health_check_path.as_deref() != Some(result.path.as_str())
            || state.as_ref().map(|state| state.pid) != Some(result.pid)
        {
            continue;
        }
        record.last_health_check_at_millis = Some(result.checked_at_millis);
        if result.success {
            record.consecutive_health_failures = 0;
            record.last_health_success_at_millis = Some(result.checked_at_millis);
            record.health_state_code = HealthState::Healthy as u8;
        } else {
            record.consecutive_health_failures =
                record.consecutive_health_failures.saturating_add(1);
            if record.consecutive_health_failures >= record.health_check_failure_threshold {
                record.health_state_code = HealthState::Unhealthy as u8;
                record.total_unhealthy_restart_count =
                    record.total_unhealthy_restart_count.saturating_add(1);
                write_record(&path, &record)?;
                if let Some(state) = state.filter(master_is_running) {
                    signal_master(&state, libc::SIGKILL)?;
                }
                continue;
            }
            record.health_state_code = HealthState::Failing as u8;
        }
        write_record(&path, &record)?;
    }
    Ok(())
}

fn schedule_recovery(record: &mut ApplicationRecord, now: u64) {
    record.unstable_restart_count = record.unstable_restart_count.saturating_add(1);
    if record.unstable_restart_count > record.max_unstable_restarts {
        record.recovery_state_code = RecoveryState::CircuitOpen as u8;
        record.next_restart_at_millis = None;
        return;
    }
    let exponent = record.unstable_restart_count.saturating_sub(1).min(20);
    let multiplier = 1_u64 << exponent;
    let delay = record
        .restart_delay_millis
        .saturating_mul(multiplier)
        .min(record.restart_backoff_max_millis);
    record.recovery_state_code = RecoveryState::Backoff as u8;
    record.next_restart_at_millis = Some(now.saturating_add(delay));
}

fn supervise_applications(executable: &OsStr, paths: &ManagerPaths) -> Result<Option<u64>, String> {
    let now = epoch_millis();
    let mut earliest_restart = None;
    for mut record in read_all_records(paths)? {
        if record.desired_state_code != ApplicationState::Online as u8 || !record.auto_restart {
            continue;
        }
        let path = paths.application(&record.name);
        if let Some(state) = running_state(&record).filter(master_is_running) {
            if record.recovery_state_code == RecoveryState::Stabilizing as u8
                && now.saturating_sub(state.started_at_millis) >= record.min_uptime_millis
            {
                record.unstable_restart_count = 0;
                record.next_restart_at_millis = None;
                record.recovery_state_code = RecoveryState::Healthy as u8;
                write_record(&path, &record)?;
            }
            continue;
        }
        if record.recovery_state_code == RecoveryState::CircuitOpen as u8 {
            continue;
        }
        if record.recovery_state_code != RecoveryState::Backoff as u8
            || record.next_restart_at_millis.is_none()
        {
            schedule_recovery(&mut record, now);
            earliest_restart = earliest_deadline(earliest_restart, record.next_restart_at_millis);
            write_record(&path, &record)?;
            continue;
        }
        if record
            .next_restart_at_millis
            .is_some_and(|deadline| deadline > now)
        {
            earliest_restart = earliest_deadline(earliest_restart, record.next_restart_at_millis);
            continue;
        }
        match restart_record(executable, &record, false, false) {
            Ok(_) => {
                record.total_auto_restart_count = record.total_auto_restart_count.saturating_add(1);
                record.next_restart_at_millis = None;
                record.recovery_state_code = RecoveryState::Stabilizing as u8;
            }
            Err(error) => {
                eprintln!("pamd could not recover {:?}: {error}", record.name);
                schedule_recovery(&mut record, epoch_millis());
                earliest_restart =
                    earliest_deadline(earliest_restart, record.next_restart_at_millis);
            }
        }
        write_record(&path, &record)?;
    }
    Ok(earliest_restart)
}

fn earliest_deadline(current: Option<u64>, candidate: Option<u64>) -> Option<u64> {
    match (current, candidate) {
        (Some(current), Some(candidate)) => Some(current.min(candidate)),
        (None, candidate) => candidate,
        (current, None) => current,
    }
}

fn next_supervision_delay(now_millis: u64, earliest_restart: Option<u64>) -> Duration {
    earliest_restart
        .map(|deadline| Duration::from_millis(deadline.saturating_sub(now_millis).max(1)))
        .map_or(SUPERVISION_INTERVAL, |delay| {
            delay.min(SUPERVISION_INTERVAL)
        })
}

fn startup(executable: &OsStr, arguments: Vec<OsString>) -> Result<u8, String> {
    let install = match arguments.as_slice() {
        [] => false,
        [value] if value == "--print" => false,
        [value] if value == "--install" => true,
        _ => return Err("startup accepts --print or --install".to_owned()),
    };
    let executable = fs::canonicalize(executable)
        .map_err(|error| format!("cannot resolve PAM executable: {error}"))?;
    let unit = systemd_unit(&executable)?;
    if !install {
        print!("{unit}");
        return Ok(0);
    }
    let home =
        std::env::var_os("HOME").ok_or_else(|| "HOME is required for --install".to_owned())?;
    let directory = PathBuf::from(home).join(".config/systemd/user");
    ensure_directory(&directory)?;
    let path = directory.join("pamd.service");
    write_private_bytes(&path, unit.as_bytes())?;
    println!("Installed {}", path.display());
    println!("Enable with: systemctl --user enable --now pamd.service");
    Ok(0)
}

fn monit(arguments: Vec<OsString>) -> Result<u8, String> {
    let json = parse_json_only(arguments, "monit")?;
    let records = read_all_records(&ManagerPaths::load()?)?;
    if json {
        println!(
            "{}",
            serde_json::json!({"schemaVersion":1,"applications":records.iter().map(|record| application_json(record, running_state(record).as_ref())).collect::<Vec<_>>() })
        );
    } else {
        println!("PAM MONIT\nNAME\tSTATE\tPID\tWORKERS\tRSS_BYTES\tTASKS\tALERT");
        for record in records {
            let state = running_state(&record);
            let online = state.as_ref().is_some_and(master_is_running);
            let resources = state
                .as_ref()
                .filter(|_| online)
                .map(|value| crate::resource_monitor::process_tree(value.pid))
                .unwrap_or_default();
            let alert = resource_alert_state(&record, &resources);
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                record.name,
                if online { "online" } else { "stopped" },
                state.as_ref().map_or(0, |value| value.pid),
                state.as_ref().map_or(0, |value| value.workers),
                resources.rss_bytes,
                resources.tasks,
                alert as u8,
            );
        }
    }
    Ok(0)
}

fn resource_history(arguments: Vec<OsString>) -> Result<u8, String> {
    let mut name = None;
    let mut json = false;
    let mut record = false;
    let mut limit = MAX_RESOURCE_HISTORY;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--json" => json = true,
            "--record" => record = true,
            "--limit" => {
                limit = required_positive(arguments.next(), "--limit")?;
                if limit > MAX_RESOURCE_HISTORY {
                    return Err(format!("--limit cannot exceed {MAX_RESOURCE_HISTORY}"));
                }
            }
            option if option.starts_with('-') => {
                return Err(format!("unknown monit:history option: {option}"));
            }
            _ if name.is_none() => name = Some(argument.to_string_lossy().into_owned()),
            _ => return Err("monit:history accepts at most one application name".to_owned()),
        }
    }
    if let Some(name) = name.as_deref() {
        validate_name(name)?;
    }
    let paths = ManagerPaths::load()?;
    if record {
        record_resource_history(&paths)?;
    }
    let records = read_all_records(&paths)?;
    let histories = records
        .iter()
        .filter(|application| name.as_deref().is_none_or(|name| name == application.name))
        .map(|application| {
            let mut history = read_resource_history(&paths, &application.name)?;
            if history.entries.len() > limit {
                history.entries.drain(..history.entries.len() - limit);
            }
            Ok(history)
        })
        .collect::<Result<Vec<_>, String>>()?;
    if name.is_some() && histories.is_empty() {
        return Err(format!("unknown managed application {:?}", name.unwrap()));
    }
    if json {
        println!(
            "{}",
            serde_json::json!({"schemaVersion":1,"sampleIntervalSeconds":RESOURCE_SAMPLE_INTERVAL.as_secs(),"retentionLimit":MAX_RESOURCE_HISTORY,"applications":histories})
        );
    } else {
        println!(
            "PAM RESOURCE HISTORY (latest {limit} samples)\nNAME\tOBSERVED_AT_MS\tSTATE\tWORKERS\tRSS_BYTES\tTASKS\tALERT"
        );
        for history in histories {
            for entry in history.entries {
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    history.name,
                    entry.observed_at_millis,
                    entry.state_code,
                    entry.workers,
                    entry.rss_bytes,
                    entry.tasks,
                    entry.alert_state_code
                );
            }
        }
    }
    Ok(0)
}

fn record_resource_history(paths: &ManagerPaths) -> Result<(), String> {
    let observed_at_millis = epoch_millis();
    for record in read_all_records(paths)? {
        let state = running_state(&record);
        let online = state.as_ref().is_some_and(master_is_running);
        let resources = state
            .as_ref()
            .filter(|_| online)
            .map(|value| crate::resource_monitor::process_tree(value.pid))
            .unwrap_or_default();
        let mut history = read_resource_history(paths, &record.name)?;
        append_resource_entry(
            &mut history,
            ResourceHistoryEntry {
                observed_at_millis,
                state_code: if online {
                    ApplicationState::Online as u8
                } else {
                    ApplicationState::Stopped as u8
                },
                workers: state.as_ref().map_or(0, |value| value.workers),
                rss_bytes: resources.rss_bytes,
                tasks: resources.tasks,
                alert_state_code: resource_alert_state(&record, &resources) as u8,
            },
        );
        write_private_json(&paths.resource_history(&record.name), &history)?;
    }
    Ok(())
}

fn append_resource_entry(history: &mut ResourceHistory, entry: ResourceHistoryEntry) {
    history.entries.push(entry);
    if history.entries.len() > MAX_RESOURCE_HISTORY {
        history
            .entries
            .drain(..history.entries.len() - MAX_RESOURCE_HISTORY);
    }
}

fn read_resource_history(paths: &ManagerPaths, name: &str) -> Result<ResourceHistory, String> {
    let path = paths.resource_history(name);
    if !path.exists() {
        return Ok(ResourceHistory {
            schema_version: 1,
            name: name.to_owned(),
            entries: Vec::new(),
        });
    }
    let history: ResourceHistory = read_private_json(&path)?;
    if history.schema_version != 1
        || history.name != name
        || history.entries.len() > MAX_RESOURCE_HISTORY
        || history.entries.iter().any(|entry| {
            !matches!(entry.state_code, 1 | 2)
                || !matches!(entry.alert_state_code, 1..=5)
                || entry.workers > 256
        })
        || history
            .entries
            .windows(2)
            .any(|entries| entries[0].observed_at_millis > entries[1].observed_at_millis)
    {
        return Err("invalid resource history contract".to_owned());
    }
    Ok(history)
}

fn dashboard(arguments: Vec<OsString>) -> Result<u8, String> {
    let output = match arguments.as_slice() {
        [] => PathBuf::from("pam-dashboard.html"),
        [path] if path != "--output" => PathBuf::from(path),
        [option, path] if option == "--output" => PathBuf::from(path),
        _ => return Err("dashboard accepts one output path or --output FILE.html".to_owned()),
    };
    if output.extension() != Some(OsStr::new("html")) {
        return Err("dashboard output must use the .html extension".to_owned());
    }
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create dashboard directory: {error}"))?;
    }
    let paths = ManagerPaths::load()?;
    let applications = dashboard_applications(&paths)?;
    let html = crate::manager_dashboard::render(&applications);
    if html.len() > MAX_DASHBOARD_BYTES {
        return Err("manager dashboard exceeds the 2 MiB safety limit".to_owned());
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    let mut file = options.open(&output).map_err(|error| {
        format!(
            "cannot create new manager dashboard {}: {error}",
            output.display()
        )
    })?;
    file.write_all(html.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("cannot persist manager dashboard: {error}"))?;
    println!(
        "Wrote private PAM manager dashboard to {}",
        output.display()
    );
    Ok(0)
}

fn dashboard_applications(
    paths: &ManagerPaths,
) -> Result<Vec<crate::manager_dashboard::DashboardApplication>, String> {
    read_all_records(paths)?
        .into_iter()
        .map(|record| {
            let state = running_state(&record);
            let online = state.as_ref().is_some_and(master_is_running);
            let resources = state
                .as_ref()
                .filter(|_| online)
                .map(|value| crate::resource_monitor::process_tree(value.pid))
                .unwrap_or_default();
            let alert = resource_alert_state(&record, &resources);
            let history = read_resource_history(paths, &record.name)?;
            let first_rss = history.entries.first().map(|entry| entry.rss_bytes);
            let latest_rss = history.entries.last().map(|entry| entry.rss_bytes);
            let peak_rss_bytes = history
                .entries
                .iter()
                .map(|entry| entry.rss_bytes)
                .max()
                .unwrap_or(0);
            let (alert_label, alert_class) = match alert {
                ResourceAlertState::Healthy => ("Healthy", "healthy"),
                ResourceAlertState::MemoryWarning => ("Memory warning", "warning"),
                ResourceAlertState::TaskWarning => ("Task warning", "warning"),
                ResourceAlertState::MemoryAndTaskWarning => ("Memory + task warning", "warning"),
                ResourceAlertState::Unavailable => ("Metrics unavailable", "unavailable"),
            };
            Ok(crate::manager_dashboard::DashboardApplication {
                name: record.name,
                kind_label: if record.kind_code == ApplicationKind::LaravelOctane as u8 {
                    "Laravel Octane"
                } else {
                    "PAM Runtime"
                },
                online,
                workers: state.as_ref().map_or(0, |value| value.workers),
                rss_bytes: resources.rss_bytes,
                tasks: resources.tasks,
                alert_label,
                alert_class,
                warning: matches!(
                    alert,
                    ResourceAlertState::MemoryWarning
                        | ResourceAlertState::TaskWarning
                        | ResourceAlertState::MemoryAndTaskWarning
                ),
                history_samples: history.entries.len(),
                peak_rss_bytes,
                rss_delta_bytes: (history.entries.len() >= 2).then(|| {
                    i128::from(latest_rss.unwrap_or(0)) - i128::from(first_rss.unwrap_or(0))
                }),
            })
        })
        .collect()
}

fn live_dashboard_start(executable: &OsStr, arguments: Vec<OsString>) -> Result<u8, String> {
    let mut listen = "127.0.0.1:9615".parse::<SocketAddr>().unwrap();
    let mut token_file = None;
    let mut json = false;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--listen" => listen = parse_socket(arguments.next(), "--listen")?,
            "--token-file" => {
                token_file = Some(PathBuf::from(required_utf8(
                    arguments.next(),
                    "--token-file",
                )?))
            }
            "--json" => json = true,
            option => return Err(format!("unknown dashboard:start option: {option}")),
        }
    }
    if !listen.ip().is_loopback() || listen.port() == 0 {
        return Err(
            "dashboard:start requires an explicit loopback IP and non-zero port".to_owned(),
        );
    }
    let token_file =
        token_file.ok_or_else(|| "dashboard:start requires --token-file".to_owned())?;
    let token_file = fs::canonicalize(&token_file)
        .map_err(|error| format!("cannot resolve dashboard token file: {error}"))?;
    let metadata = fs::metadata(&token_file)
        .map_err(|error| format!("cannot inspect dashboard token file: {error}"))?;
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err("dashboard token file must be owner-only (mode 0600 or stricter)".to_owned());
    }
    let credential = crate::admin_auth::read_file(&token_file)?;
    let paths = ManagerPaths::load()?;
    let state_path = paths.live_dashboard_state();
    if read_live_dashboard_state(&state_path).is_ok_and(|state| live_dashboard_running(&state)) {
        return Err("live manager dashboard is already online".to_owned());
    }
    if state_path.exists() {
        fs::remove_file(&state_path)
            .map_err(|error| format!("cannot remove stale dashboard state: {error}"))?;
    }
    let config_path = paths.live_dashboard_config();
    write_private_json(
        &config_path,
        &LiveDashboardConfig {
            schema_version: 1,
            listen,
            token_sha256: credential.digest(),
        },
    )?;
    let stdout = secure_append(&paths.logs.join("live-dashboard.out.log"))?;
    let stderr = secure_append(&paths.logs.join("live-dashboard.error.log"))?;
    let mut command = Command::new(executable);
    command
        .arg("__manager_dashboard_server")
        .arg(&config_path)
        .arg(&state_path)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr);
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        })
    };
    command
        .spawn()
        .map_err(|error| format!("cannot start live manager dashboard: {error}"))?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let state = loop {
        if let Ok(state) = read_live_dashboard_state(&state_path)
            && live_dashboard_running(&state)
        {
            break state;
        }
        if Instant::now() >= deadline {
            return Err(
                "live manager dashboard did not become ready; inspect live-dashboard.error.log"
                    .to_owned(),
            );
        }
        thread::sleep(Duration::from_millis(25));
    };
    print_live_dashboard_state(&state, json);
    Ok(0)
}

fn live_dashboard_status(arguments: Vec<OsString>) -> Result<u8, String> {
    let json = parse_json_only(arguments, "dashboard:status")?;
    let path = ManagerPaths::load()?.live_dashboard_state();
    let state = read_live_dashboard_state(&path).ok();
    let online = state.as_ref().is_some_and(live_dashboard_running);
    if let Some(state) = state.filter(|_| online) {
        print_live_dashboard_state(&state, json);
    } else if json {
        println!(
            "{}",
            serde_json::json!({"schemaVersion":1,"stateCode":2,"online":false})
        );
    } else {
        println!("Live PAM manager dashboard is stopped");
    }
    Ok(if online { 0 } else { 1 })
}

fn live_dashboard_stop(arguments: Vec<OsString>) -> Result<u8, String> {
    let json = parse_json_only(arguments, "dashboard:stop")?;
    let paths = ManagerPaths::load()?;
    let state_path = paths.live_dashboard_state();
    let state = read_live_dashboard_state(&state_path)?;
    if live_dashboard_running(&state) {
        let result = unsafe { libc::kill(state.pid as i32, libc::SIGTERM) };
        if result != 0 {
            return Err(format!(
                "cannot stop live manager dashboard: {}",
                std::io::Error::last_os_error()
            ));
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while live_dashboard_running(&state) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(25));
        }
        if live_dashboard_running(&state) {
            return Err("live manager dashboard did not stop before the deadline".to_owned());
        }
    }
    for path in [state_path, paths.live_dashboard_config()] {
        if path.exists() {
            fs::remove_file(&path)
                .map_err(|error| format!("cannot remove live dashboard state: {error}"))?;
        }
    }
    if json {
        println!(
            "{}",
            serde_json::json!({"schemaVersion":1,"stateCode":2,"online":false})
        );
    } else {
        println!("Stopped live PAM manager dashboard");
    }
    Ok(0)
}

fn print_live_dashboard_state(state: &LiveDashboardState, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::json!({"schemaVersion":1,"stateCode":1,"online":true,"pid":state.pid,"listen":state.listen,"startedAtMillis":state.started_at_millis})
        );
    } else {
        println!(
            "Live PAM manager dashboard online at http://{}",
            state.listen
        );
    }
}

fn read_live_dashboard_state(path: &Path) -> Result<LiveDashboardState, String> {
    let state: LiveDashboardState = read_private_json(path)?;
    if state.schema_version != 1
        || state.state_code != 1
        || !state.listen.ip().is_loopback()
        || state.listen.port() == 0
        || state.pid == 0
        || state.process_start_ticks == 0
    {
        return Err("invalid live dashboard state contract".to_owned());
    }
    Ok(state)
}

fn live_dashboard_running(state: &LiveDashboardState) -> bool {
    (unsafe { libc::kill(state.pid as i32, 0) == 0 })
        && linux_process_start_ticks(state.pid) == Some(state.process_start_ticks)
}

fn linux_process_start_ticks(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_name = stat.get(stat.rfind(')')? + 2..)?;
    after_name.split_whitespace().nth(19)?.parse().ok()
}

fn live_dashboard_server(arguments: Vec<OsString>) -> Result<u8, String> {
    let [config_path, state_path] = arguments.as_slice() else {
        return Err("__manager_dashboard_server requires config and state paths".to_owned());
    };
    let config_path = PathBuf::from(config_path);
    let state_path = PathBuf::from(state_path);
    let config: LiveDashboardConfig = read_private_json(&config_path)?;
    if config.schema_version != 1 || !config.listen.ip().is_loopback() || config.listen.port() == 0
    {
        return Err("invalid live dashboard config contract".to_owned());
    }
    let listener = TcpListener::bind(config.listen)
        .map_err(|error| format!("cannot bind live dashboard {}: {error}", config.listen))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    LIVE_DASHBOARD_RUNNING.store(true, std::sync::atomic::Ordering::SeqCst);
    unsafe {
        libc::signal(libc::SIGTERM, live_dashboard_signal as libc::sighandler_t);
        libc::signal(libc::SIGINT, live_dashboard_signal as libc::sighandler_t);
    }
    write_private_json(
        &state_path,
        &LiveDashboardState {
            schema_version: 1,
            state_code: 1,
            pid: std::process::id(),
            process_start_ticks: linux_process_start_ticks(std::process::id())
                .ok_or_else(|| "cannot identify live dashboard process".to_owned())?,
            listen: config.listen,
            started_at_millis: epoch_millis(),
        },
    )?;
    while LIVE_DASHBOARD_RUNNING.load(std::sync::atomic::Ordering::SeqCst) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                if let Err(error) = serve_live_dashboard_request(&mut stream, &config.token_sha256)
                {
                    eprintln!("live dashboard request rejected: {error}");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(25));
            }
            Err(error) => return Err(format!("live dashboard accept failed: {error}")),
        }
    }
    if state_path.exists() {
        fs::remove_file(state_path).map_err(|error| error.to_string())?;
    }
    Ok(0)
}

extern "C" fn live_dashboard_signal(_: libc::c_int) {
    LIVE_DASHBOARD_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
}

fn serve_live_dashboard_request(
    stream: &mut TcpStream,
    expected_digest: &[u8; 32],
) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| error.to_string())?;
    let mut request = Vec::new();
    let mut chunk = [0_u8; 2048];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("request ended before headers".to_owned());
        }
        request.extend_from_slice(&chunk[..read]);
        if request.len() > MAX_DASHBOARD_REQUEST_BYTES {
            write_http_response(
                stream,
                "431 Request Header Fields Too Large",
                "text/plain; charset=utf-8",
                b"Request headers exceed 16 KiB.\n",
                &[],
            )?;
            return Ok(());
        }
    }
    let request =
        std::str::from_utf8(&request).map_err(|_| "request headers are not UTF-8".to_owned())?;
    let mut lines = request.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or_default();
    let target = request_parts.next().unwrap_or_default();
    let version = request_parts.next().unwrap_or_default();
    if request_parts.next().is_some() || version != "HTTP/1.1" {
        write_http_response(
            stream,
            "400 Bad Request",
            "text/plain; charset=utf-8",
            b"Malformed HTTP request.\n",
            &[],
        )?;
        return Ok(());
    }
    let authorized = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("authorization"))
        .and_then(|(_, value)| live_dashboard_credential(value.trim()))
        .is_some_and(|credential| {
            constant_time_digest_eq(
                expected_digest,
                &Sha256::digest(credential.as_bytes()).into(),
            )
        });
    if !authorized {
        write_http_response(
            stream,
            "401 Unauthorized",
            "text/plain; charset=utf-8",
            b"Authentication required. Use HTTP Basic user pam with the dashboard token as password, or a Bearer token.\n",
            &["WWW-Authenticate: Basic realm=\"PAM manager\", charset=\"UTF-8\""],
        )?;
        return Ok(());
    }
    if method != "GET" {
        write_http_response(
            stream,
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            b"Only GET is supported.\n",
            &["Allow: GET"],
        )?;
        return Ok(());
    }
    match target {
        "/" => {
            let paths = ManagerPaths::load()?;
            let html = crate::manager_dashboard::render_live(&dashboard_applications(&paths)?);
            if html.len() > MAX_DASHBOARD_BYTES {
                return Err("live dashboard exceeds the 2 MiB safety limit".to_owned());
            }
            write_http_response(
                stream,
                "200 OK",
                "text/html; charset=utf-8",
                html.as_bytes(),
                &[],
            )?;
        }
        "/health" => write_http_response(
            stream,
            "200 OK",
            "application/json; charset=utf-8",
            br#"{"schemaVersion":1,"stateCode":1,"healthy":true}"#,
            &[],
        )?,
        _ => write_http_response(
            stream,
            "404 Not Found",
            "text/plain; charset=utf-8",
            b"Not found.\n",
            &[],
        )?,
    }
    Ok(())
}

fn live_dashboard_credential(header: &str) -> Option<String> {
    if let Some(token) = header.strip_prefix("Bearer ") {
        return Some(token.to_owned());
    }
    let encoded = header.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    decoded.strip_prefix("pam:").map(str::to_owned)
}

fn constant_time_digest_eq(expected: &[u8; 32], supplied: &[u8; 32]) -> bool {
    expected
        .iter()
        .zip(supplied)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn write_http_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
    extra_headers: &[&str],
) -> Result<(), String> {
    let mut headers = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nPragma: no-cache\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'\r\n",
        body.len()
    );
    for header in extra_headers {
        headers.push_str(header);
        headers.push_str("\r\n");
    }
    headers.push_str("\r\n");
    stream
        .write_all(headers.as_bytes())
        .and_then(|()| stream.write_all(body))
        .map_err(|error| error.to_string())
}

fn deploy(executable: &OsStr, arguments: Vec<OsString>) -> Result<u8, String> {
    let (name, release, json) = parse_deploy_arguments(arguments)?;
    let release = validate_release_directory(&release)?;
    let paths = ManagerPaths::load()?;
    let record = read_record(&paths.application(&name))?;
    if record.working_directory == release {
        print_deployment_result(&name, &release, DeploymentAction::Unchanged, json);
        return Ok(0);
    }
    let mut history = read_deployment_history(&paths, &name)?;
    if history.entries.is_empty() {
        history.entries.push(DeploymentEntry {
            release_directory: record.working_directory.clone(),
            activated_at_millis: record.created_at_millis,
            event_kind_code: DeploymentEventKind::Baseline as u8,
        });
    }
    activate_release(executable, &paths, &record, &release)?;
    append_deployment_entry(
        &mut history,
        DeploymentEntry {
            release_directory: release.clone(),
            activated_at_millis: epoch_millis(),
            event_kind_code: DeploymentEventKind::Deploy as u8,
        },
    );
    write_private_json(&paths.deployment(&name), &history)?;
    print_deployment_result(&name, &release, DeploymentAction::Activated, json);
    Ok(0)
}

fn rollback(executable: &OsStr, arguments: Vec<OsString>) -> Result<u8, String> {
    let (name, steps, json) = parse_rollback_arguments(arguments)?;
    let paths = ManagerPaths::load()?;
    let record = read_record(&paths.application(&name))?;
    let mut history = read_deployment_history(&paths, &name)?;
    let target = history
        .entries
        .iter()
        .rev()
        .filter(|entry| entry.release_directory != record.working_directory)
        .nth(steps - 1)
        .map(|entry| entry.release_directory.clone())
        .ok_or_else(|| {
            format!("application {name:?} has no rollback target for {steps} step(s)")
        })?;
    let target = validate_release_directory(&target)?;
    activate_release(executable, &paths, &record, &target)?;
    append_deployment_entry(
        &mut history,
        DeploymentEntry {
            release_directory: target.clone(),
            activated_at_millis: epoch_millis(),
            event_kind_code: DeploymentEventKind::Rollback as u8,
        },
    );
    write_private_json(&paths.deployment(&name), &history)?;
    print_deployment_result(&name, &target, DeploymentAction::RolledBack, json);
    Ok(0)
}

fn deployment_history(arguments: Vec<OsString>) -> Result<u8, String> {
    let (name, json) = parse_name_json(arguments, "deploy:history")?;
    let history = read_deployment_history(&ManagerPaths::load()?, &name)?;
    if json {
        println!(
            "{}",
            serde_json::to_string(&history).map_err(|error| error.to_string())?
        );
    } else if history.entries.is_empty() {
        println!("No deployment history for {name}.");
    } else {
        println!("RELEASE\tEVENT\tACTIVATED_AT_MS");
        for entry in history.entries {
            println!(
                "{}\t{}\t{}",
                entry.release_directory.display(),
                entry.event_kind_code,
                entry.activated_at_millis
            );
        }
    }
    Ok(0)
}

fn activate_release(
    executable: &OsStr,
    paths: &ManagerPaths,
    previous: &ApplicationRecord,
    release: &Path,
) -> Result<(), String> {
    stop_record(previous)?;
    let mut candidate = previous.clone();
    candidate.working_directory = release.to_path_buf();
    if candidate.kind_code == ApplicationKind::LaravelOctane as u8 {
        candidate.master_state_file = release.join(".pam/octane.json");
    }
    let record_path = paths.application(&candidate.name);
    write_record(&record_path, &candidate)?;
    if let Err(error) = restart_record(executable, &candidate, false, false) {
        write_record(&record_path, previous)?;
        let recovery = restart_record(executable, previous, false, false);
        return match recovery {
            Ok(_) => Err(format!(
                "release failed readiness and previous release was restored: {error}"
            )),
            Err(recovery_error) => Err(format!(
                "release failed ({error}); previous release recovery also failed ({recovery_error})"
            )),
        };
    }
    Ok(())
}

fn stop_record(record: &ApplicationRecord) -> Result<(), String> {
    let Some(state) = running_state(record) else {
        return Ok(());
    };
    terminate_master(&state, record.shutdown_timeout_millis)?;
    Ok(())
}

fn read_deployment_history(paths: &ManagerPaths, name: &str) -> Result<DeploymentHistory, String> {
    validate_name(name)?;
    let path = paths.deployment(name);
    if !path.exists() {
        return Ok(DeploymentHistory {
            schema_version: 1,
            name: name.to_owned(),
            entries: Vec::new(),
        });
    }
    let history: DeploymentHistory = read_private_json(&path)?;
    if history.schema_version != 1
        || history.name != name
        || history.entries.len() > MAX_DEPLOY_HISTORY
    {
        return Err("invalid deployment history contract".to_owned());
    }
    if history
        .entries
        .iter()
        .any(|entry| !matches!(entry.event_kind_code, 1..=3))
    {
        return Err("invalid deployment event kind".to_owned());
    }
    Ok(history)
}

fn append_deployment_entry(history: &mut DeploymentHistory, entry: DeploymentEntry) {
    history.entries.push(entry);
    if history.entries.len() > MAX_DEPLOY_HISTORY {
        let excess = history.entries.len() - MAX_DEPLOY_HISTORY;
        history.entries.drain(..excess);
    }
}

fn validate_release_directory(path: &Path) -> Result<PathBuf, String> {
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(format!(
            "refusing symlink release directory {}",
            path.display()
        ));
    }
    let path = fs::canonicalize(path)
        .map_err(|error| format!("cannot resolve release directory: {error}"))?;
    if !path.is_dir() {
        return Err("release target must be a directory".to_owned());
    }
    Ok(path)
}

fn parse_deploy_arguments(arguments: Vec<OsString>) -> Result<(String, PathBuf, bool), String> {
    let mut values = Vec::new();
    let mut json = false;
    for argument in arguments {
        if argument == "--json" {
            json = true;
        } else {
            values.push(argument);
        }
    }
    if values.len() != 2 {
        return Err("deploy requires NAME RELEASE_DIRECTORY and optional --json".to_owned());
    }
    let name = required_utf8(Some(values.remove(0)), "deploy name")?;
    validate_name(&name)?;
    Ok((name, PathBuf::from(values.remove(0)), json))
}

fn parse_rollback_arguments(arguments: Vec<OsString>) -> Result<(String, usize, bool), String> {
    let mut name = None;
    let mut steps = 1;
    let mut json = false;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--steps" => steps = required_positive(arguments.next(), "--steps")?,
            "--json" => json = true,
            option if option.starts_with('-') => {
                return Err(format!("unknown rollback option: {option}"));
            }
            _ if name.is_none() => name = Some(argument.to_string_lossy().into_owned()),
            _ => return Err("rollback accepts one application name".to_owned()),
        }
    }
    let name = name.ok_or_else(|| "rollback requires an application name".to_owned())?;
    validate_name(&name)?;
    if steps > MAX_DEPLOY_HISTORY {
        return Err(format!("--steps cannot exceed {MAX_DEPLOY_HISTORY}"));
    }
    Ok((name, steps, json))
}

fn print_deployment_result(name: &str, release: &Path, action: DeploymentAction, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::json!({"schemaVersion":1,"name":name,"actionCode":action as u8,"releaseDirectory":release})
        );
    } else {
        println!(
            "{} {} -> {}",
            match action {
                DeploymentAction::Activated => "Deployed",
                DeploymentAction::RolledBack => "Rolled back",
                DeploymentAction::Unchanged => "Unchanged",
            },
            name,
            release.display()
        );
    }
}

fn daemon(executable: &OsStr, arguments: Vec<OsString>) -> Result<u8, String> {
    let action = match arguments.as_slice() {
        [action] => action.to_string_lossy(),
        _ => return Err("daemon requires one of: start, status, stop".to_owned()),
    };
    match action.as_ref() {
        "start" => {
            if daemon_request(DaemonOperation::Ping).is_ok() {
                println!("pamd is already online");
                return Ok(0);
            }
            let response = start_daemon(executable)?;
            println!("pamd is online (PID {})", response.pid);
            Ok(0)
        }
        "status" => match daemon_request(DaemonOperation::Ping) {
            Ok(response) => {
                println!("pamd is online (PID {})", response.pid);
                Ok(0)
            }
            Err(_) => {
                println!("pamd is stopped");
                Ok(1)
            }
        },
        "stop" => {
            let response = daemon_request(DaemonOperation::Stop)?;
            println!("pamd stopped (PID {})", response.pid);
            Ok(0)
        }
        _ => Err("daemon requires one of: start, status, stop".to_owned()),
    }
}

fn ensure_daemon(executable: &OsStr) -> Result<(), String> {
    if daemon_request(DaemonOperation::Ping).is_ok() {
        return Ok(());
    }
    start_daemon(executable).map(|_| ())
}

fn start_daemon(executable: &OsStr) -> Result<DaemonResponse, String> {
    let paths = ManagerPaths::load()?;
    let stderr = secure_append(&paths.logs.join("pamd.error.log"))?;
    let stdout = secure_append(&paths.logs.join("pamd.out.log"))?;
    let mut command = Command::new(executable);
    command
        .arg("__pamd")
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr);
    // SAFETY: the child is single-threaded between fork and exec.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        })
    };
    command
        .spawn()
        .map_err(|error| format!("cannot start pamd: {error}"))?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Ok(response) = daemon_request(DaemonOperation::Ping) {
            return Ok(response);
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err("pamd did not become ready; inspect pamd.error.log".to_owned())
}

fn daemon_serve(executable: &OsStr) -> Result<u8, String> {
    let socket = daemon_socket_path()?;
    if socket.exists() {
        if daemon_request(DaemonOperation::Ping).is_ok() {
            return Err("pamd is already running".to_owned());
        }
        fs::remove_file(&socket).map_err(|error| format!("cannot remove stale socket: {error}"))?;
    }
    let listener = UnixListener::bind(&socket)
        .map_err(|error| format!("cannot bind daemon socket {}: {error}", socket.display()))?;
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))
        .map_err(|error| error.to_string())?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("cannot configure daemon socket: {error}"))?;
    let paths = ManagerPaths::load()?;
    let dump = paths.base.join("dump.json");
    if dump.exists()
        && let Err(error) = resurrect_saved(executable)
    {
        eprintln!("pamd could not restore saved applications: {error}");
    }
    if let Err(error) = record_resource_history(&paths) {
        eprintln!("pamd could not record initial resource history: {error}");
    }
    let mut next_sample = Instant::now() + RESOURCE_SAMPLE_INTERVAL;
    let mut next_supervision = Instant::now();
    let mut master_watchers = MasterWatchers::default();
    let (health_sender, health_receiver) = mpsc::channel();
    let mut health_probes_in_flight = HashSet::new();
    let own_uid = unsafe { libc::geteuid() };
    loop {
        let reaped_child = reap_daemon_children();
        if let Err(error) =
            apply_health_probe_results(&paths, &health_receiver, &mut health_probes_in_flight)
        {
            eprintln!("pamd health result error: {error}");
        }
        if reaped_child || master_watchers.exit_ready() || Instant::now() >= next_supervision {
            let earliest_restart = match supervise_applications(executable, &paths) {
                Ok(deadline) => deadline,
                Err(error) => {
                    eprintln!("pamd supervision error: {error}");
                    None
                }
            };
            if let Err(error) =
                schedule_health_probes(&paths, &health_sender, &mut health_probes_in_flight)
            {
                eprintln!("pamd health scheduling error: {error}");
            }
            next_supervision =
                Instant::now() + next_supervision_delay(epoch_millis(), earliest_restart);
            master_watchers = watch_running_masters(&paths);
        }
        if Instant::now() >= next_sample {
            if let Err(error) = record_resource_history(&paths) {
                eprintln!("pamd could not record resource history: {error}");
            }
            next_sample = Instant::now() + RESOURCE_SAMPLE_INTERVAL;
        }
        let mut stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(25));
                continue;
            }
            Err(error) => {
                eprintln!("pamd accept error: {error}");
                continue;
            }
        };
        let (response, stop) = match handle_daemon_request(executable, &mut stream, own_uid) {
            Ok(result) => result,
            Err(error) => (daemon_error_response(error), false),
        };
        if let Err(error) = serde_json::to_writer(&mut stream, &response) {
            eprintln!("pamd response error: {error}");
            continue;
        }
        if let Err(error) = stream.write_all(b"\n") {
            eprintln!("pamd response error: {error}");
        }
        if stop && response.ok {
            break;
        }
    }
    fs::remove_file(&socket).map_err(|error| format!("cannot remove daemon socket: {error}"))?;
    Ok(0)
}

fn handle_daemon_request(
    executable: &OsStr,
    stream: &mut UnixStream,
    own_uid: libc::uid_t,
) -> Result<(DaemonResponse, bool), String> {
    if peer_uid(stream)? != own_uid {
        return Err("peer UID does not own this pamd".to_owned());
    }
    let request = read_daemon_request(stream)?;
    if request.schema_version != 1 {
        return Err("unsupported daemon schema".to_owned());
    }
    let operation = request.operation_code;
    if operation == DaemonOperation::Ping as u8 || operation == DaemonOperation::Stop as u8 {
        return Ok((
            daemon_success_response(),
            operation == DaemonOperation::Stop as u8,
        ));
    }
    if operation != DaemonOperation::Execute as u8 {
        return Err("unsupported daemon operation".to_owned());
    }
    let command = request
        .command
        .ok_or_else(|| "execute requires command".to_owned())?;
    if !daemon_managed_command(&command) || request.arguments.len() > 256 {
        return Err("command is not allowed through pamd".to_owned());
    }
    let cwd = request
        .working_directory
        .ok_or_else(|| "execute requires cwd".to_owned())?;
    let cwd =
        fs::canonicalize(cwd).map_err(|error| format!("cannot resolve client cwd: {error}"))?;
    if !cwd.is_dir() {
        return Err("client cwd is not a directory".to_owned());
    }
    let output = Command::new(executable)
        .arg("__manager_local")
        .arg(&command)
        .args(&request.arguments)
        .current_dir(cwd)
        .output()
        .map_err(|error| format!("cannot execute manager command: {error}"))?;
    let stdout =
        String::from_utf8(output.stdout).map_err(|_| "manager stdout is not UTF-8".to_owned())?;
    let stderr =
        String::from_utf8(output.stderr).map_err(|_| "manager stderr is not UTF-8".to_owned())?;
    if stdout.len() + stderr.len() > MAX_DAEMON_RESPONSE_BYTES as usize {
        return Err("manager response exceeds 2 MiB".to_owned());
    }
    Ok((
        DaemonResponse {
            schema_version: 1,
            ok: true,
            pid: std::process::id(),
            message: "ok".to_owned(),
            exit_code: output.status.code().unwrap_or(1).try_into().unwrap_or(1),
            stdout,
            stderr,
        },
        false,
    ))
}

fn daemon_success_response() -> DaemonResponse {
    DaemonResponse {
        schema_version: 1,
        ok: true,
        pid: std::process::id(),
        message: "ok".to_owned(),
        exit_code: 0,
        stdout: String::new(),
        stderr: String::new(),
    }
}

fn daemon_error_response(message: String) -> DaemonResponse {
    DaemonResponse {
        schema_version: 1,
        ok: false,
        pid: std::process::id(),
        message,
        exit_code: 1,
        stdout: String::new(),
        stderr: String::new(),
    }
}

fn daemon_request(operation: DaemonOperation) -> Result<DaemonResponse, String> {
    let mut stream = UnixStream::connect(daemon_socket_path()?)
        .map_err(|error| format!("cannot connect to pamd: {error}"))?;
    serde_json::to_writer(
        &mut stream,
        &DaemonRequest {
            schema_version: 1,
            operation_code: operation as u8,
            command: None,
            arguments: Vec::new(),
            working_directory: None,
        },
    )
    .map_err(|error| error.to_string())?;
    stream.write_all(b"\n").map_err(|error| error.to_string())?;
    stream
        .shutdown(Shutdown::Write)
        .map_err(|error| error.to_string())?;
    let response = read_daemon_response(stream)?;
    if response.schema_version != 1 || !response.ok {
        return Err(response.message);
    }
    Ok(response)
}

fn daemon_execute(command: &str, arguments: Vec<OsString>) -> Result<u8, String> {
    let arguments = arguments
        .into_iter()
        .map(|value| {
            value
                .into_string()
                .map_err(|_| "pamd arguments must be UTF-8".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let request = DaemonRequest {
        schema_version: 1,
        operation_code: DaemonOperation::Execute as u8,
        command: Some(command.to_owned()),
        arguments,
        working_directory: Some(std::env::current_dir().map_err(|error| error.to_string())?),
    };
    let bytes = serde_json::to_vec(&request).map_err(|error| error.to_string())?;
    if bytes.len() > MAX_DAEMON_MESSAGE_BYTES {
        return Err("daemon request exceeds size limit".to_owned());
    }
    let mut stream = UnixStream::connect(daemon_socket_path()?)
        .map_err(|error| format!("cannot connect to pamd: {error}"))?;
    stream
        .write_all(&bytes)
        .map_err(|error| error.to_string())?;
    stream
        .shutdown(Shutdown::Write)
        .map_err(|error| error.to_string())?;
    let response = read_daemon_response(stream)?;
    if response.schema_version != 1 || !response.ok {
        return Err(response.message);
    }
    std::io::stdout()
        .write_all(response.stdout.as_bytes())
        .map_err(|error| error.to_string())?;
    std::io::stderr()
        .write_all(response.stderr.as_bytes())
        .map_err(|error| error.to_string())?;
    Ok(response.exit_code)
}

fn read_daemon_response(stream: UnixStream) -> Result<DaemonResponse, String> {
    let mut bytes = Vec::new();
    stream
        .take(MAX_DAEMON_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_DAEMON_RESPONSE_BYTES {
        return Err("daemon response exceeds 2 MiB".to_owned());
    }
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

fn read_daemon_request(stream: &mut UnixStream) -> Result<DaemonRequest, String> {
    let mut bytes = Vec::new();
    stream
        .take((MAX_DAEMON_MESSAGE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_DAEMON_MESSAGE_BYTES {
        return Err("daemon request exceeds size limit".to_owned());
    }
    serde_json::from_slice(&bytes).map_err(|error| format!("invalid daemon request: {error}"))
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn peer_uid(stream: &UnixStream) -> Result<libc::uid_t, String> {
    let mut credentials = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&raw mut credentials).cast(),
            &raw mut length,
        )
    };
    if result != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(credentials.uid)
}

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
))]
fn peer_uid(stream: &UnixStream) -> Result<libc::uid_t, String> {
    let mut uid: libc::uid_t = 0;
    let mut gid: libc::gid_t = 0;
    let result = unsafe { libc::getpeereid(stream.as_raw_fd(), &raw mut uid, &raw mut gid) };
    if result != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(uid)
}

fn daemon_socket_path() -> Result<PathBuf, String> {
    let base = if let Some(path) = std::env::var_os("PAM_MANAGER_RUNTIME_DIR") {
        PathBuf::from(path)
    } else if let Some(path) = std::env::var_os("XDG_RUNTIME_DIR") {
        PathBuf::from(path).join("pam")
    } else {
        ManagerPaths::load()?.runtime.join("daemon")
    };
    secure_directory(&base)?;
    Ok(base.join("pamd.sock"))
}

fn managed_launch_command(
    executable: &OsStr,
    arguments: &[OsString],
    working_directory: &Path,
    name: &str,
    memory_max_bytes: Option<u64>,
    task_max_count: Option<u64>,
) -> Result<Command, String> {
    if memory_max_bytes.is_none() && task_max_count.is_none() {
        let mut command = Command::new(executable);
        command.args(arguments).current_dir(working_directory);
        return Ok(command);
    }
    let systemd_run = Path::new("/usr/bin/systemd-run");
    if !systemd_run.is_file() {
        return Err("cgroup limits require /usr/bin/systemd-run".to_owned());
    }
    let mut command = Command::new(systemd_run);
    command
        .current_dir(working_directory)
        .args(["--user", "--scope", "--quiet", "--collect"])
        .arg(format!(
            "--unit=pam-{name}-{}-{}",
            std::process::id(),
            epoch_millis()
        ));
    if let Some(value) = memory_max_bytes {
        command.args(["--property", &format!("MemoryMax={value}")]);
    }
    if let Some(value) = task_max_count {
        command.args(["--property", &format!("TasksMax={value}")]);
    }
    command.arg("--").arg(executable).args(arguments);
    Ok(command)
}

fn up(executable: &OsStr, arguments: Vec<OsString>) -> Result<u8, String> {
    let mut name = None;
    let mut target = None;
    let mut workers = None;
    let mut attach = false;
    let mut json = false;
    let mut log_max_bytes = DEFAULT_LOG_MAX_BYTES;
    let mut log_retain = DEFAULT_LOG_RETAIN;
    let mut memory_warning_bytes = None;
    let mut task_warning_count = None;
    let mut memory_max_bytes = None;
    let mut task_max_count = None;
    let mut environment_file = None;
    let mut shutdown_timeout_millis = DEFAULT_SHUTDOWN_TIMEOUT_MILLIS;
    let mut health_check_url = None;
    let mut health_check_interval_millis = DEFAULT_HEALTH_INTERVAL_MILLIS;
    let mut health_check_timeout_millis = DEFAULT_HEALTH_TIMEOUT_MILLIS;
    let mut health_check_start_period_millis = 0;
    let mut health_check_failure_threshold = DEFAULT_HEALTH_FAILURE_THRESHOLD;
    let mut auto_restart = true;
    let mut restart_delay_millis = DEFAULT_RESTART_DELAY_MILLIS;
    let mut restart_backoff_max_millis = DEFAULT_RESTART_BACKOFF_MAX_MILLIS;
    let mut max_unstable_restarts = DEFAULT_MAX_UNSTABLE_RESTARTS;
    let mut min_uptime_millis = DEFAULT_MIN_UPTIME_MILLIS;
    let mut application_arguments = Vec::new();
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--name" => name = Some(required_utf8(arguments.next(), "--name")?),
            "--workers" => workers = Some(required_positive(arguments.next(), "--workers")?),
            "--attach" => attach = true,
            "--json" => json = true,
            "--no-autorestart" => auto_restart = false,
            "--env-file" => {
                environment_file = Some(PathBuf::from(required_utf8(
                    arguments.next(),
                    "--env-file",
                )?))
            }
            "--shutdown-timeout-ms" => {
                shutdown_timeout_millis =
                    required_positive_u64(arguments.next(), "--shutdown-timeout-ms")?
            }
            "--health-check-url" => {
                health_check_url = Some(required_utf8(arguments.next(), "--health-check-url")?)
            }
            "--health-check-interval-ms" => {
                health_check_interval_millis =
                    required_positive_u64(arguments.next(), "--health-check-interval-ms")?
            }
            "--health-check-timeout-ms" => {
                health_check_timeout_millis =
                    required_positive_u64(arguments.next(), "--health-check-timeout-ms")?
            }
            "--health-check-start-period-ms" => {
                health_check_start_period_millis =
                    required_u64(arguments.next(), "--health-check-start-period-ms")?
            }
            "--health-check-failures" => {
                health_check_failure_threshold =
                    required_positive(arguments.next(), "--health-check-failures")?
                        .try_into()
                        .map_err(|_| "--health-check-failures is too large".to_owned())?
            }
            "--restart-delay-ms" => {
                restart_delay_millis =
                    required_positive_u64(arguments.next(), "--restart-delay-ms")?
            }
            "--restart-backoff-max-ms" => {
                restart_backoff_max_millis =
                    required_positive_u64(arguments.next(), "--restart-backoff-max-ms")?
            }
            "--max-unstable-restarts" => {
                max_unstable_restarts =
                    required_positive(arguments.next(), "--max-unstable-restarts")?
                        .try_into()
                        .map_err(|_| "--max-unstable-restarts is too large".to_owned())?;
            }
            "--min-uptime-ms" => {
                min_uptime_millis = required_positive_u64(arguments.next(), "--min-uptime-ms")?
            }
            "--log-max-bytes" => {
                log_max_bytes = required_positive_u64(arguments.next(), "--log-max-bytes")?
            }
            "--log-retain" => {
                log_retain = required_positive(arguments.next(), "--log-retain")?;
                if log_retain > MAX_LOG_RETAIN {
                    return Err(format!("--log-retain cannot exceed {MAX_LOG_RETAIN}"));
                }
            }
            "--memory-warning-bytes" => {
                memory_warning_bytes = Some(required_positive_u64(
                    arguments.next(),
                    "--memory-warning-bytes",
                )?)
            }
            "--task-warning-count" => {
                task_warning_count = Some(required_positive_u64(
                    arguments.next(),
                    "--task-warning-count",
                )?)
            }
            "--memory-max-bytes" => {
                memory_max_bytes = Some(required_positive_u64(
                    arguments.next(),
                    "--memory-max-bytes",
                )?)
            }
            "--task-max-count" => {
                task_max_count = Some(required_positive_u64(arguments.next(), "--task-max-count")?)
            }
            "--" => {
                application_arguments.extend(arguments);
                break;
            }
            option if option.starts_with('-') => {
                return Err(format!("unknown up option: {option}"));
            }
            _ if target.is_none() => target = Some(PathBuf::from(argument)),
            _ => return Err("pam up accepts at most one application target before `--`".to_owned()),
        }
    }
    validate_resource_policy(
        memory_warning_bytes,
        task_warning_count,
        memory_max_bytes,
        task_max_count,
        "application",
    )?;
    validate_shutdown_policy(shutdown_timeout_millis)?;
    validate_recovery_policy(
        restart_delay_millis,
        restart_backoff_max_millis,
        max_unstable_restarts,
        min_uptime_millis,
    )?;
    let health_check = validate_health_policy(
        health_check_url.as_deref(),
        health_check_interval_millis,
        health_check_timeout_millis,
        health_check_failure_threshold,
    )?;
    validate_health_start_period(health_check_start_period_millis, health_check.is_some())?;
    if health_check.is_some() && !auto_restart {
        return Err("health checks require automatic restart".to_owned());
    }

    let cwd = fs::canonicalize(std::env::current_dir().map_err(|error| error.to_string())?)
        .map_err(|error| format!("cannot resolve application directory: {error}"))?;
    let environment_file = environment_file
        .as_deref()
        .map(|path| resolve_environment_file(&cwd, path))
        .transpose()?;
    let laravel = cwd.join("artisan").is_file() && target.is_none();
    let target = target.unwrap_or_else(|| {
        if laravel {
            PathBuf::from("artisan")
        } else {
            PathBuf::from("index.php")
        }
    });
    let inferred_name = cwd.file_name().and_then(OsStr::to_str).unwrap_or("pam-app");
    let name = name.unwrap_or_else(|| inferred_name.to_owned());
    validate_name(&name)?;
    let paths = ManagerPaths::load()?;
    let record_path = paths.application(&name);
    if record_path.exists() {
        let record = read_record(&record_path)?;
        if running_state(&record).is_some_and(|state| master_is_running(&state)) {
            return Err(format!("application {name:?} is already online"));
        }
        return Err(format!(
            "application {name:?} already exists; delete or restart it"
        ));
    }

    let master_state_file = paths.runtime.join(format!("{name}.master.json"));
    let stdout_log = paths.logs.join(format!("{name}.out.log"));
    let stderr_log = paths.logs.join(format!("{name}.error.log"));
    rotate_log(&stdout_log, log_max_bytes, log_retain)?;
    rotate_log(&stderr_log, log_max_bytes, log_retain)?;
    let mut launch_arguments = Vec::<OsString>::new();
    if laravel {
        launch_arguments.push(OsString::from("octane:start"));
    } else {
        launch_arguments.extend([OsString::from("start"), target.clone().into_os_string()]);
        launch_arguments.extend([
            OsString::from("--state-file"),
            master_state_file.clone().into_os_string(),
        ]);
    }
    if let Some(workers) = workers {
        launch_arguments.extend([
            OsString::from("--workers"),
            OsString::from(workers.to_string()),
        ]);
    }
    if !application_arguments.is_empty() {
        launch_arguments.push(OsString::from("--"));
        launch_arguments.extend(application_arguments);
    }
    let mut command = managed_launch_command(
        executable,
        &launch_arguments,
        &cwd,
        &name,
        memory_max_bytes,
        task_max_count,
    )?;
    apply_environment_file(&mut command, environment_file.as_deref())?;
    if attach {
        command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
    } else {
        command
            .stdin(Stdio::null())
            .stdout(secure_append(&stdout_log)?)
            .stderr(secure_append(&stderr_log)?);
    }
    if !attach {
        // SAFETY: this runs in the child after fork and before exec. setsid has no
        // memory allocation requirement and detaches the supervisor from the shell.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            })
        };
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot start {name:?}: {error}"))?;
    let effective_state = if laravel {
        cwd.join(".pam/octane.json")
    } else {
        master_state_file
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    let state = loop {
        if let Ok(state) = read_master_state(&effective_state)
            && master_is_running(&state)
        {
            break state;
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("cannot inspect application launch: {error}"))?
        {
            return Err(format!(
                "application {name:?} exited before readiness with {status}; inspect {}",
                stderr_log.display()
            ));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "application {name:?} did not become ready; inspect {}",
                stderr_log.display()
            ));
        }
        thread::sleep(Duration::from_millis(50));
    };
    let record = ApplicationRecord {
        schema_version: 2,
        name: name.clone(),
        kind_code: if laravel {
            ApplicationKind::LaravelOctane as u8
        } else {
            ApplicationKind::Runtime as u8
        },
        working_directory: cwd,
        command: std::iter::once(executable.to_string_lossy().into_owned())
            .chain(
                launch_arguments
                    .iter()
                    .map(|argument| argument.to_string_lossy().into_owned()),
            )
            .collect(),
        master_state_file: effective_state,
        stdout_log,
        stderr_log,
        log_max_bytes,
        log_retain,
        memory_warning_bytes,
        task_warning_count,
        memory_max_bytes,
        task_max_count,
        environment_file,
        shutdown_timeout_millis,
        health_check_address: health_check.as_ref().map(|(address, _)| *address),
        health_check_path: health_check.map(|(_, path)| path),
        health_check_interval_millis,
        health_check_timeout_millis,
        health_check_start_period_millis,
        health_check_failure_threshold,
        consecutive_health_failures: 0,
        last_health_check_at_millis: None,
        last_health_success_at_millis: None,
        health_state_code: initial_health_state(
            health_check_url.is_some(),
            health_check_start_period_millis,
        ),
        total_unhealthy_restart_count: 0,
        desired_state_code: ApplicationState::Online as u8,
        auto_restart,
        restart_delay_millis,
        restart_backoff_max_millis,
        max_unstable_restarts,
        min_uptime_millis,
        unstable_restart_count: 0,
        total_auto_restart_count: 0,
        next_restart_at_millis: None,
        recovery_state_code: if auto_restart {
            RecoveryState::Healthy as u8
        } else {
            RecoveryState::Disabled as u8
        },
        created_at_millis: epoch_millis(),
    };
    write_record(&record_path, &record)?;
    if json {
        println!("{}", application_json(&record, Some(&state)));
    } else {
        println!(
            "Started {} (PID {}, {} workers)",
            record.name, state.pid, state.workers
        );
    }
    if attach {
        let status = child.wait().map_err(|error| error.to_string())?;
        return Ok(status.code().unwrap_or(1).try_into().unwrap_or(1));
    }
    Ok(0)
}

fn list(arguments: Vec<OsString>) -> Result<u8, String> {
    let json = parse_json_only(arguments, "ps")?;
    let paths = ManagerPaths::load()?;
    let records = read_all_records(&paths)?;
    if json {
        println!(
            "{}",
            serde_json::json!({"schemaVersion": 1, "applications": records.iter().map(|record| application_json(record, running_state(record).as_ref())).collect::<Vec<_>>() })
        );
    } else if records.is_empty() {
        println!("No managed PAM applications.");
    } else {
        println!("NAME\tSTATUS\tPID\tWORKERS\tDIRECTORY");
        for record in records {
            let state = running_state(&record);
            let online = state.as_ref().is_some_and(master_is_running);
            println!(
                "{}\t{}\t{}\t{}\t{}",
                record.name,
                if online { "online" } else { "stopped" },
                state.as_ref().map_or(0, |state| state.pid),
                state.as_ref().map_or(0, |state| state.workers),
                record.working_directory.display()
            );
        }
    }
    Ok(0)
}

fn inspect(command: &str, arguments: Vec<OsString>) -> Result<u8, String> {
    let (name, json) = parse_name_json(arguments, command)?;
    let record = record(&name)?;
    let state = running_state(&record);
    let online = state.as_ref().is_some_and(master_is_running);
    if json || command == "describe" {
        println!("{}", application_json(&record, state.as_ref()));
    } else {
        println!(
            "{} is {}",
            record.name,
            if online { "online" } else { "stopped" }
        );
    }
    Ok(if online { 0 } else { 1 })
}

fn signal(arguments: Vec<OsString>, signal: i32, action: &str) -> Result<u8, String> {
    let (name, json) = parse_name_json(arguments, "reload")?;
    let record = record(&name)?;
    let state = running_state(&record)
        .ok_or_else(|| format!("application {name:?} has no master state"))?;
    signal_master(&state, signal)?;
    if json {
        println!(
            "{}",
            serde_json::json!({"schemaVersion":1,"name":name,"stateCode":ApplicationState::Online as u8,"pid":state.pid})
        );
    } else {
        println!("Application {name} is {action} (PID {})", state.pid);
    }
    Ok(0)
}

fn stop(arguments: Vec<OsString>) -> Result<u8, String> {
    let (name, json) = parse_name_json(arguments, "stop")?;
    let paths = ManagerPaths::load()?;
    let path = paths.application(&name);
    let mut record = read_record(&path)?;
    record.desired_state_code = ApplicationState::Stopped as u8;
    record.recovery_state_code = RecoveryState::Disabled as u8;
    record.unstable_restart_count = 0;
    record.next_restart_at_millis = None;
    record.consecutive_health_failures = 0;
    record.health_state_code = HealthState::Disabled as u8;
    write_record(&path, &record)?;
    let Some(state) = running_state(&record) else {
        return Ok(0);
    };
    let forced = terminate_master(&state, record.shutdown_timeout_millis)?;
    if json {
        println!(
            "{}",
            serde_json::json!({"schemaVersion":1,"name":name,"stateCode":ApplicationState::Stopped as u8,"forced":forced})
        );
    } else if forced {
        println!(
            "Stopped {name} (forced after {} ms)",
            record.shutdown_timeout_millis
        );
    } else {
        println!("Stopped {name}");
    }
    Ok(0)
}

fn restart(executable: &OsStr, arguments: Vec<OsString>) -> Result<u8, String> {
    let (name, json) = parse_name_json(arguments, "restart")?;
    let paths = ManagerPaths::load()?;
    let path = paths.application(&name);
    let mut record = read_record(&path)?;
    reset_recovery(&mut record);
    write_record(&path, &record)?;
    restart_record(executable, &record, json, true)
}

fn restart_record(
    executable: &OsStr,
    record: &ApplicationRecord,
    json: bool,
    emit: bool,
) -> Result<u8, String> {
    let name = &record.name;
    let forced = if let Some(state) = running_state(record) {
        terminate_master(&state, record.shutdown_timeout_millis)?
    } else {
        false
    };
    if record.command.len() < 2 {
        return Err(format!("application {name:?} has no restart command"));
    }
    rotate_log(&record.stdout_log, record.log_max_bytes, record.log_retain)?;
    rotate_log(&record.stderr_log, record.log_max_bytes, record.log_retain)?;
    let stdout = secure_append(&record.stdout_log)?;
    let stderr = secure_append(&record.stderr_log)?;
    let restart_arguments = record.command[1..]
        .iter()
        .map(OsString::from)
        .collect::<Vec<_>>();
    let mut command = managed_launch_command(
        executable,
        &restart_arguments,
        &record.working_directory,
        &record.name,
        record.memory_max_bytes,
        record.task_max_count,
    )?;
    apply_environment_file(&mut command, record.environment_file.as_deref())?;
    command.stdin(Stdio::null()).stdout(stdout).stderr(stderr);
    // SAFETY: see the detached launch in `up`.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        })
    };
    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot restart {name:?}: {error}"))?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let state = loop {
        if let Some(state) = running_state(record)
            && master_is_running(&state)
        {
            break state;
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("cannot inspect application restart: {error}"))?
        {
            return Err(format!(
                "application {name:?} exited before restart readiness with {status}"
            ));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "application {name:?} did not become ready after restart"
            ));
        }
        thread::sleep(Duration::from_millis(50));
    };
    if emit {
        if json {
            let mut application = application_json(record, Some(&state));
            application["shutdownForced"] = serde_json::json!(forced);
            println!("{application}");
        } else if forced {
            println!(
                "Restarted {name} (PID {}, previous master forced after {} ms)",
                state.pid, record.shutdown_timeout_millis
            );
        } else {
            println!("Restarted {name} (PID {})", state.pid);
        }
    }
    Ok(0)
}

fn terminate_master(state: &MasterState, graceful_timeout_millis: u64) -> Result<bool, String> {
    if !master_is_running(state) {
        return Ok(false);
    }
    signal_master(state, STOP_SIGNAL)?;
    let graceful_deadline = Instant::now() + Duration::from_millis(graceful_timeout_millis);
    while master_is_running(state) && Instant::now() < graceful_deadline {
        reap_daemon_children();
        thread::sleep(Duration::from_millis(50));
    }
    if !master_is_running(state) {
        return Ok(false);
    }
    signal_master(state, libc::SIGKILL)?;
    let forced_deadline = Instant::now() + FORCED_SHUTDOWN_TIMEOUT;
    while master_is_running(state) && Instant::now() < forced_deadline {
        reap_daemon_children();
        thread::sleep(Duration::from_millis(50));
    }
    if master_is_running(state) {
        return Err(format!(
            "master PID {} remained alive after SIGKILL",
            state.pid
        ));
    }
    Ok(true)
}

fn reap_daemon_children() -> bool {
    let mut reaped = false;
    loop {
        let result = unsafe { libc::waitpid(-1, std::ptr::null_mut(), libc::WNOHANG) };
        if result <= 0 {
            break;
        }
        reaped = true;
    }
    reaped
}

fn watch_running_masters(paths: &ManagerPaths) -> MasterWatchers {
    let descriptors = read_all_records(paths)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|record| running_state(&record))
        .filter(master_is_running)
        .filter_map(|state| open_pidfd(state.pid))
        .collect::<Vec<_>>();
    let poll_descriptors = descriptors
        .iter()
        .map(|descriptor| libc::pollfd {
            fd: descriptor.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        })
        .collect();
    MasterWatchers {
        _descriptors: descriptors,
        poll_descriptors,
    }
}

#[cfg(target_os = "linux")]
fn open_pidfd(pid: u32) -> Option<OwnedFd> {
    let descriptor = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) as libc::c_int };
    if descriptor < 0 {
        None
    } else {
        Some(unsafe { OwnedFd::from_raw_fd(descriptor) })
    }
}

#[cfg(not(target_os = "linux"))]
fn open_pidfd(_pid: u32) -> Option<OwnedFd> {
    None
}

fn delete(arguments: Vec<OsString>) -> Result<u8, String> {
    let (name, json) = parse_name_json(arguments, "delete")?;
    let paths = ManagerPaths::load()?;
    let path = paths.application(&name);
    let record = read_record(&path)?;
    if running_state(&record)
        .as_ref()
        .is_some_and(master_is_running)
    {
        return Err(format!("application {name:?} is online; stop it first"));
    }
    fs::remove_file(&path).map_err(|error| format!("cannot delete application record: {error}"))?;
    let history = paths.resource_history(&name);
    if history.exists() {
        fs::remove_file(&history)
            .map_err(|error| format!("cannot delete application resource history: {error}"))?;
    }
    if json {
        println!(
            "{}",
            serde_json::json!({"schemaVersion":1,"name":name,"deleted":true})
        );
    } else {
        println!("Deleted {name}");
    }
    Ok(0)
}

fn logs(arguments: Vec<OsString>) -> Result<u8, String> {
    let mut name = None;
    let mut lines = 100_usize;
    let mut errors = false;
    let mut both = false;
    let mut follow = false;
    let mut json = false;
    let mut query = None::<String>;
    let mut include_rotated = false;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--lines" => lines = required_positive(arguments.next(), "--lines")?,
            "--errors" => errors = true,
            "--both" => both = true,
            "--follow" | "-f" => follow = true,
            "--json" => json = true,
            "--query" => query = Some(required_utf8(arguments.next(), "--query")?),
            "--include-rotated" => include_rotated = true,
            option if option.starts_with('-') => {
                return Err(format!("unknown logs option: {option}"));
            }
            _ if name.is_none() => name = Some(argument.to_string_lossy().into_owned()),
            _ => return Err("logs accepts one application name".to_owned()),
        }
    }
    let name = name.ok_or_else(|| "logs requires an application name".to_owned())?;
    validate_name(&name)?;
    let record = record(&name)?;
    if errors && both {
        return Err("logs accepts either --errors or --both, not both".to_owned());
    }
    if follow && (json || query.is_some() || include_rotated) {
        return Err("logs --follow cannot be combined with structured query options".to_owned());
    }
    if query.as_ref().is_some_and(|value| {
        value.is_empty() || value.len() > 256 || value.contains(['\0', '\n', '\r'])
    }) {
        return Err("logs --query requires 1-256 characters without controls".to_owned());
    }
    let streams = if both {
        vec![
            (record.stdout_log, LogStream::StandardOutput),
            (record.stderr_log, LogStream::StandardError),
        ]
    } else if errors {
        vec![(record.stderr_log, LogStream::StandardError)]
    } else {
        vec![(record.stdout_log, LogStream::StandardOutput)]
    };
    if json || query.is_some() || include_rotated {
        let limit = lines.min(10_000);
        let mut entries = VecDeque::with_capacity(limit.min(1024));
        let mut truncated = false;
        for (path, stream) in &streams {
            let mut files = if include_rotated {
                (1..=record.log_retain)
                    .rev()
                    .map(|index| (rotated_log_path(path, index), index))
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            files.push((path.clone(), 0));
            for (file, rotated_index) in files {
                if !file.exists() {
                    continue;
                }
                for line in read_log_lines(&file)? {
                    if query.as_ref().is_none_or(|query| line.contains(query)) {
                        if entries.len() == limit {
                            entries.pop_front();
                            truncated = true;
                        }
                        entries.push_back(serde_json::json!({
                            "streamCode": *stream as u8,
                            "rotatedIndex": rotated_index,
                            "line": line,
                        }));
                    }
                }
            }
        }
        if json {
            println!(
                "{}",
                serde_json::json!({"schemaVersion":1,"name":name,"query":query,"truncated":truncated,"entries":entries})
            );
        } else {
            for entry in entries {
                println!("{}", entry["line"].as_str().unwrap_or_default());
            }
        }
    } else {
        for (path, _) in &streams {
            print_tail(path, lines.min(100_000))?;
        }
    }
    if follow {
        follow_logs(
            &streams
                .iter()
                .map(|(path, _)| path.clone())
                .collect::<Vec<_>>(),
        )?;
    }
    Ok(0)
}

fn follow_logs(paths: &[PathBuf]) -> Result<(), String> {
    let mut offsets = paths
        .iter()
        .map(|path| {
            fs::symlink_metadata(path)
                .ok()
                .filter(|metadata| metadata.file_type().is_file())
                .map(|metadata| metadata.len())
                .unwrap_or(0)
        })
        .collect::<Vec<_>>();
    loop {
        for (index, path) in paths.iter().enumerate() {
            let length = fs::symlink_metadata(path)
                .ok()
                .filter(|metadata| metadata.file_type().is_file())
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            if length < offsets[index] {
                offsets[index] = 0;
            }
            if length > offsets[index] {
                let mut file = open_log_read(path)?;
                file.seek(SeekFrom::Start(offsets[index]))
                    .map_err(|error| error.to_string())?;
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes)
                    .map_err(|error| error.to_string())?;
                std::io::stdout()
                    .write_all(&bytes)
                    .map_err(|error| error.to_string())?;
                std::io::stdout()
                    .flush()
                    .map_err(|error| error.to_string())?;
                offsets[index] = length;
            }
        }
        thread::sleep(Duration::from_millis(200));
    }
}

fn rotate_log(path: &Path, max_bytes: u64, retain: usize) -> Result<(), String> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    if metadata.file_type().is_symlink() {
        return Err(format!("refusing symlink log {}", path.display()));
    }
    if metadata.len() < max_bytes {
        return Ok(());
    }
    for index in (1..=retain).rev() {
        let source = if index == 1 {
            path.to_path_buf()
        } else {
            rotated_log_path(path, index - 1)
        };
        let destination = rotated_log_path(path, index);
        if source.exists() {
            if destination.exists() {
                fs::remove_file(&destination).map_err(|error| error.to_string())?;
            }
            fs::rename(&source, &destination)
                .map_err(|error| format!("cannot rotate log {}: {error}", path.display()))?;
        }
    }
    Ok(())
}

fn rotated_log_path(path: &Path, index: usize) -> PathBuf {
    PathBuf::from(format!("{}.{}", path.display(), index))
}

const fn default_log_max_bytes() -> u64 {
    DEFAULT_LOG_MAX_BYTES
}

const fn default_log_retain() -> usize {
    DEFAULT_LOG_RETAIN
}

fn print_tail(path: &Path, lines: usize) -> Result<(), String> {
    let text = read_log_lines(path)?;
    let selected = text.iter().rev().take(lines).collect::<Vec<_>>();
    for line in selected.into_iter().rev() {
        println!("{line}");
    }
    Ok(())
}

fn read_log_lines(path: &Path) -> Result<Vec<String>, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect log {}: {error}", path.display()))?;
    if !metadata.file_type().is_file() {
        return Err(format!("refusing non-regular log {}", path.display()));
    }
    let mut file = open_log_read(path)?;
    let length = file.metadata().map_err(|error| error.to_string())?.len();
    let window = length.min(8 * 1024 * 1024);
    file.seek(SeekFrom::Start(length - window))
        .map_err(|error| error.to_string())?;
    let mut bytes = Vec::with_capacity(window as usize);
    file.read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read log {}: {error}", path.display()))?;
    Ok(String::from_utf8_lossy(&bytes)
        .lines()
        .map(str::to_owned)
        .collect())
}

fn open_log_read(path: &Path) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.read(true).custom_flags(libc::O_NOFOLLOW);
    options
        .open(path)
        .map_err(|error| format!("cannot open log {}: {error}", path.display()))
}

fn application_json(record: &ApplicationRecord, state: Option<&MasterState>) -> serde_json::Value {
    let online = state.is_some_and(master_is_running);
    let resources = state
        .filter(|_| online)
        .map(|state| crate::resource_monitor::process_tree(state.pid))
        .unwrap_or_default();
    let resource_alert_state = resource_alert_state(record, &resources);
    let resource_enforcement_state =
        if record.memory_max_bytes.is_none() && record.task_max_count.is_none() {
            ResourceEnforcementState::NotRequested
        } else if record
            .memory_max_bytes
            .is_none_or(|value| resources.cgroup_memory_max_bytes == Some(value))
            && record
                .task_max_count
                .is_none_or(|value| resources.cgroup_task_max_count == Some(value))
        {
            ResourceEnforcementState::Enforced
        } else {
            ResourceEnforcementState::Unverified
        };
    serde_json::json!({
        "schemaVersion": 1,
        "name": record.name,
        "kindCode": record.kind_code,
        "stateCode": if online { ApplicationState::Online as u8 } else { ApplicationState::Stopped as u8 },
        "desiredStateCode": record.desired_state_code,
        "pid": state.map(|state| state.pid),
        "workers": state.map(|state| state.workers),
        "startedAtMillis": state.map(|state| state.started_at_millis),
        "workingDirectory": record.working_directory,
        "stdoutLog": record.stdout_log,
        "stderrLog": record.stderr_log,
        "environmentFileConfigured": record.environment_file.is_some(),
        "shutdownPolicy": {
            "gracefulTimeoutMillis": record.shutdown_timeout_millis,
            "forcedTimeoutMillis": FORCED_SHUTDOWN_TIMEOUT.as_millis(),
        },
        "healthCheck": {
            "configured": record.health_check_address.is_some(),
            "stateCode": record.health_state_code,
            "intervalMillis": record.health_check_interval_millis,
            "timeoutMillis": record.health_check_timeout_millis,
            "startPeriodMillis": record.health_check_start_period_millis,
            "failureThreshold": record.health_check_failure_threshold,
            "consecutiveFailures": record.consecutive_health_failures,
            "lastCheckedAtMillis": record.last_health_check_at_millis,
            "lastSuccessAtMillis": record.last_health_success_at_millis,
            "totalUnhealthyRestartCount": record.total_unhealthy_restart_count,
        },
        "resources": resources,
        "resourcePolicy": {
            "memoryWarningBytes": record.memory_warning_bytes,
            "taskWarningCount": record.task_warning_count,
            "memoryMaxBytes": record.memory_max_bytes,
            "taskMaxCount": record.task_max_count,
            "enforcementCode": resource_enforcement_state as u8,
        },
        "resourceAlertStateCode": resource_alert_state as u8,
        "recovery": {
            "stateCode": record.recovery_state_code,
            "autoRestart": record.auto_restart,
            "restartDelayMillis": record.restart_delay_millis,
            "restartBackoffMaxMillis": record.restart_backoff_max_millis,
            "maxUnstableRestarts": record.max_unstable_restarts,
            "minUptimeMillis": record.min_uptime_millis,
            "unstableRestartCount": record.unstable_restart_count,
            "totalAutoRestartCount": record.total_auto_restart_count,
            "nextRestartAtMillis": record.next_restart_at_millis,
        },
    })
}

fn resource_alert_state(
    record: &ApplicationRecord,
    resources: &crate::resource_monitor::ResourceSnapshot,
) -> ResourceAlertState {
    if !resources.observed {
        return ResourceAlertState::Unavailable;
    }
    let memory = record
        .memory_warning_bytes
        .is_some_and(|limit| resources.rss_bytes >= limit);
    let tasks = record
        .task_warning_count
        .is_some_and(|limit| resources.tasks >= limit);
    match (memory, tasks) {
        (false, false) => ResourceAlertState::Healthy,
        (true, false) => ResourceAlertState::MemoryWarning,
        (false, true) => ResourceAlertState::TaskWarning,
        (true, true) => ResourceAlertState::MemoryAndTaskWarning,
    }
}

struct ManagerPaths {
    base: PathBuf,
    applications: PathBuf,
    runtime: PathBuf,
    logs: PathBuf,
    deployments: PathBuf,
    traffic: PathBuf,
    history: PathBuf,
}
impl ManagerPaths {
    fn load() -> Result<Self, String> {
        let base = if let Some(path) = std::env::var_os("PAM_MANAGER_STATE_DIR") {
            PathBuf::from(path)
        } else if let Some(path) = std::env::var_os("XDG_STATE_HOME") {
            PathBuf::from(path).join("pam")
        } else {
            PathBuf::from(
                std::env::var_os("HOME")
                    .ok_or_else(|| "cannot locate PAM state; set XDG_STATE_HOME".to_owned())?,
            )
            .join(".local/state/pam")
        };
        let paths = Self {
            base: base.clone(),
            applications: base.join("applications"),
            runtime: base.join("runtime"),
            logs: base.join("logs"),
            deployments: base.join("deployments"),
            traffic: base.join("traffic"),
            history: base.join("history"),
        };
        for path in [
            &paths.applications,
            &paths.runtime,
            &paths.logs,
            &paths.deployments,
            &paths.traffic,
            &paths.history,
        ] {
            secure_directory(path)?;
        }
        Ok(paths)
    }
    fn application(&self, name: &str) -> PathBuf {
        self.applications.join(format!("{name}.json"))
    }
    fn deployment(&self, name: &str) -> PathBuf {
        self.deployments.join(format!("{name}.json"))
    }
    fn traffic_config(&self, name: &str) -> PathBuf {
        self.traffic.join(format!("{name}.json"))
    }
    fn traffic_state(&self, name: &str) -> PathBuf {
        self.runtime.join(format!("traffic-{name}.json"))
    }
    fn traffic_metrics(&self, name: &str) -> PathBuf {
        self.traffic.join(format!("{name}.metrics.json"))
    }
    fn resource_history(&self, name: &str) -> PathBuf {
        self.history.join(format!("{name}.json"))
    }
    fn live_dashboard_config(&self) -> PathBuf {
        self.base.join("live-dashboard.json")
    }
    fn live_dashboard_state(&self) -> PathBuf {
        self.runtime.join("live-dashboard.json")
    }
}

fn set_command_option(command: &mut Vec<String>, option: &str, value: &str) {
    if let Some(index) = command.iter().position(|argument| argument == option) {
        if let Some(existing) = command.get_mut(index + 1) {
            *existing = value.to_owned();
            return;
        }
    }
    let separator = command
        .iter()
        .position(|argument| argument == "--")
        .unwrap_or(command.len());
    command.splice(separator..separator, [option.to_owned(), value.to_owned()]);
}

fn systemd_unit(executable: &Path) -> Result<String, String> {
    let executable = executable
        .to_str()
        .ok_or_else(|| "PAM executable path must be UTF-8 for systemd".to_owned())?;
    if executable.contains(['\n', '\r']) {
        return Err("PAM executable path cannot contain control characters".to_owned());
    }
    let escaped = executable.replace('%', "%%").replace(' ', "\\x20");
    Ok(format!(
        "[Unit]\nDescription=PAM per-user process manager\nAfter=network.target\n\n[Service]\nType=simple\nExecStart={escaped} __pamd\nExecStop={escaped} daemon stop\nRestart=on-failure\nRestartSec=1\nNoNewPrivileges=true\nPrivateTmp=true\nProtectSystem=strict\n\n[Install]\nWantedBy=default.target\n"
    ))
}

fn secure_directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    if fs::symlink_metadata(path)
        .map_err(|error| error.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err(format!(
            "refusing symlink manager directory {}",
            path.display()
        ));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| error.to_string())
}
fn ensure_directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    if fs::symlink_metadata(path)
        .map_err(|error| error.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err(format!("refusing symlink directory {}", path.display()));
    }
    Ok(())
}
fn secure_append(path: &Path) -> Result<File, String> {
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(format!("refusing symlink log {}", path.display()));
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| format!("cannot open log {}: {error}", path.display()))
}
fn write_record(path: &Path, record: &ApplicationRecord) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(record).map_err(|error| error.to_string())?;
    write_private_bytes(path, &bytes)
}
fn write_private_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err("manager JSON exceeds 1 MiB".to_owned());
    }
    write_private_bytes(path, &bytes)
}
fn write_private_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(format!("refusing symlink file {}", path.display()));
    }
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    let mut file = options
        .open(&temporary)
        .map_err(|error| format!("cannot create manager record: {error}"))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| error.to_string())?;
    fs::rename(&temporary, path).map_err(|error| format!("cannot publish manager record: {error}"))
}
fn read_private_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_RECORD_BYTES {
        return Err(format!("invalid manager file {}", path.display()));
    }
    serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
        .map_err(|error| format!("invalid manager JSON: {error}"))
}
fn read_record(path: &Path) -> Result<ApplicationRecord, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot read application record {}: {error}", path.display()))?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_RECORD_BYTES {
        return Err(format!("invalid application record {}", path.display()));
    }
    let mut record: ApplicationRecord =
        serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
            .map_err(|error| format!("invalid application record: {error}"))?;
    if !matches!(record.schema_version, 1 | 2) || record.kind_code == 0 || record.kind_code > 2 {
        return Err("unsupported application record contract".to_owned());
    }
    validate_name(&record.name)?;
    if record.schema_version == 1 {
        let online = running_state(&record)
            .as_ref()
            .is_some_and(master_is_running);
        record.schema_version = 2;
        record.desired_state_code = if online {
            ApplicationState::Online as u8
        } else {
            ApplicationState::Stopped as u8
        };
        record.auto_restart = online;
        record.recovery_state_code = if online {
            RecoveryState::Healthy as u8
        } else {
            RecoveryState::Disabled as u8
        };
    }
    if !matches!(record.desired_state_code, 1 | 2)
        || !(1..=5).contains(&record.recovery_state_code)
        || !(1..=5).contains(&record.health_state_code)
    {
        return Err("invalid application recovery state".to_owned());
    }
    validate_recovery_policy(
        record.restart_delay_millis,
        record.restart_backoff_max_millis,
        record.max_unstable_restarts,
        record.min_uptime_millis,
    )?;
    validate_shutdown_policy(record.shutdown_timeout_millis)?;
    if record.health_check_address.is_some() != record.health_check_path.is_some() {
        return Err(
            "application health check address and path must be configured together".to_owned(),
        );
    }
    let health_url = record.health_check_address.map(|address| {
        format!(
            "http://{}{}",
            address,
            record.health_check_path.as_deref().unwrap_or("/")
        )
    });
    validate_health_start_period(
        record.health_check_start_period_millis,
        health_url.is_some(),
    )?;
    validate_health_policy(
        health_url.as_deref(),
        record.health_check_interval_millis,
        record.health_check_timeout_millis,
        record.health_check_failure_threshold,
    )?;
    if health_url.is_some() && !record.auto_restart {
        return Err("application health checks require automatic restart".to_owned());
    }
    Ok(record)
}
fn read_all_records(paths: &ManagerPaths) -> Result<Vec<ApplicationRecord>, String> {
    let mut entries = fs::read_dir(&paths.applications)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    if entries.len() > MAX_APPLICATIONS {
        return Err(format!(
            "manager state exceeds {MAX_APPLICATIONS} applications"
        ));
    }
    entries.sort_by_key(|entry| entry.file_name());
    entries
        .into_iter()
        .filter(|entry| entry.path().extension() == Some(OsStr::new("json")))
        .map(|entry| read_record(&entry.path()))
        .collect()
}
fn record(name: &str) -> Result<ApplicationRecord, String> {
    validate_name(name)?;
    read_record(&ManagerPaths::load()?.application(name))
}
fn running_state(record: &ApplicationRecord) -> Option<MasterState> {
    read_master_state(&record.master_state_file).ok()
}
fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        || matches!(name, "." | "..")
    {
        return Err(
            "application name must contain 1-64 ASCII letters, digits, '.', '-' or '_'".to_owned(),
        );
    }
    Ok(())
}
fn required_utf8(value: Option<OsString>, option: &str) -> Result<String, String> {
    value
        .ok_or_else(|| format!("{option} requires a value"))?
        .into_string()
        .map_err(|_| format!("{option} requires UTF-8"))
}
fn required_positive(value: Option<OsString>, option: &str) -> Result<usize, String> {
    required_utf8(value, option)?
        .parse::<usize>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{option} requires a positive integer"))
}
fn required_positive_u64(value: Option<OsString>, option: &str) -> Result<u64, String> {
    required_utf8(value, option)?
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{option} requires a positive integer"))
}
fn required_u64(value: Option<OsString>, option: &str) -> Result<u64, String> {
    required_utf8(value, option)?
        .parse::<u64>()
        .map_err(|_| format!("{option} requires a non-negative integer"))
}
fn validate_resource_policy(
    memory_warning_bytes: Option<u64>,
    task_warning_count: Option<u64>,
    memory_max_bytes: Option<u64>,
    task_max_count: Option<u64>,
    label: &str,
) -> Result<(), String> {
    if memory_warning_bytes == Some(0)
        || task_warning_count == Some(0)
        || memory_max_bytes == Some(0)
        || task_max_count == Some(0)
    {
        return Err(format!("{label} resource thresholds must be positive"));
    }
    if memory_warning_bytes
        .zip(memory_max_bytes)
        .is_some_and(|(warning, maximum)| warning > maximum)
        || task_warning_count
            .zip(task_max_count)
            .is_some_and(|(warning, maximum)| warning > maximum)
    {
        return Err(format!("{label} warning cannot exceed its hard limit"));
    }
    Ok(())
}

fn validate_shutdown_policy(timeout_millis: u64) -> Result<(), String> {
    if !(MIN_SHUTDOWN_TIMEOUT_MILLIS..=MAX_SHUTDOWN_TIMEOUT_MILLIS).contains(&timeout_millis) {
        return Err(format!(
            "shutdown timeout must be between {MIN_SHUTDOWN_TIMEOUT_MILLIS} and {MAX_SHUTDOWN_TIMEOUT_MILLIS} milliseconds"
        ));
    }
    Ok(())
}

fn validate_health_start_period(
    start_period_millis: u64,
    health_check_configured: bool,
) -> Result<(), String> {
    if start_period_millis > MAX_HEALTH_START_PERIOD_MILLIS {
        return Err(format!(
            "health check start period must be 0-{MAX_HEALTH_START_PERIOD_MILLIS} milliseconds"
        ));
    }
    if start_period_millis > 0 && !health_check_configured {
        return Err("health check start period requires a health check URL".to_owned());
    }
    Ok(())
}

fn validate_recovery_policy(
    delay_millis: u64,
    maximum_delay_millis: u64,
    maximum_restarts: u32,
    minimum_uptime_millis: u64,
) -> Result<(), String> {
    if !(10..=60_000).contains(&delay_millis) {
        return Err("restart delay must be 10-60000 milliseconds".to_owned());
    }
    if !(delay_millis..=300_000).contains(&maximum_delay_millis) {
        return Err(
            "restart backoff maximum must be at least the delay and at most 300000 milliseconds"
                .to_owned(),
        );
    }
    if !(1..=100).contains(&maximum_restarts) {
        return Err("maximum unstable restarts must be 1-100".to_owned());
    }
    if !(1_000..=3_600_000).contains(&minimum_uptime_millis) {
        return Err("minimum stable uptime must be 1000-3600000 milliseconds".to_owned());
    }
    Ok(())
}
fn parse_json_only(arguments: Vec<OsString>, command: &str) -> Result<bool, String> {
    match arguments.as_slice() {
        [] => Ok(false),
        [value] if value == "--json" => Ok(true),
        _ => Err(format!("{command} accepts only --json")),
    }
}
fn parse_name_json(arguments: Vec<OsString>, command: &str) -> Result<(String, bool), String> {
    let mut name = None;
    let mut json = false;
    for argument in arguments {
        if argument == "--json" {
            json = true;
        } else if argument.to_string_lossy().starts_with('-') || name.is_some() {
            return Err(format!(
                "{command} requires one application name and optional --json"
            ));
        } else {
            name = Some(argument.to_string_lossy().into_owned());
        }
    }
    let name = name.ok_or_else(|| format!("{command} requires an application name"))?;
    validate_name(&name)?;
    Ok((name, json))
}
fn epoch_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn application_names_are_bounded_and_path_safe() {
        for accepted in ["api", "billing-api", "tenant_1", "api.production"] {
            assert!(validate_name(accepted).is_ok());
        }
        for rejected in ["", ".", "..", "../api", "api/log", "ápi"] {
            assert!(validate_name(rejected).is_err(), "{rejected}");
        }
    }
    #[test]
    fn public_integer_codes_are_sequential() {
        assert_eq!(
            [
                DaemonOperation::Ping as u8,
                DaemonOperation::Stop as u8,
                DaemonOperation::Execute as u8,
            ],
            [1, 2, 3]
        );
        assert_eq!(
            [
                ApplicationKind::Runtime as u8,
                ApplicationKind::LaravelOctane as u8
            ],
            [1, 2]
        );
        assert_eq!(
            [
                ReconcileAction::Created as u8,
                ReconcileAction::Unchanged as u8,
                ReconcileAction::Scaled as u8,
                ReconcileAction::Restarted as u8,
                ReconcileAction::Disabled as u8,
                ReconcileAction::PolicyUpdated as u8,
                ReconcileAction::ResourceLimitsUpdated as u8,
            ],
            [1, 2, 3, 4, 5, 6, 7]
        );
        assert_eq!(
            [
                DeploymentEventKind::Baseline as u8,
                DeploymentEventKind::Deploy as u8,
                DeploymentEventKind::Rollback as u8,
            ],
            [1, 2, 3]
        );
        assert_eq!(
            [
                DeploymentAction::Activated as u8,
                DeploymentAction::RolledBack as u8,
                DeploymentAction::Unchanged as u8,
            ],
            [1, 2, 3]
        );
        assert_eq!(
            [
                ApplicationState::Online as u8,
                ApplicationState::Stopped as u8
            ],
            [1, 2]
        );
        assert_eq!(
            [
                RolloutPhase::Stable as u8,
                RolloutPhase::Evaluating as u8,
                RolloutPhase::Promoted as u8,
                RolloutPhase::Aborted as u8,
            ],
            [1, 2, 3, 4]
        );
        assert_eq!(
            [
                RolloutDecision::Pending as u8,
                RolloutDecision::Promoted as u8,
                RolloutDecision::Aborted as u8,
                RolloutDecision::DeadlineAborted as u8,
            ],
            [1, 2, 3, 4]
        );
        assert_eq!(
            [
                LogStream::StandardOutput as u8,
                LogStream::StandardError as u8,
            ],
            [1, 2]
        );
        assert_eq!(
            [
                ResourceAlertState::Healthy as u8,
                ResourceAlertState::MemoryWarning as u8,
                ResourceAlertState::TaskWarning as u8,
                ResourceAlertState::MemoryAndTaskWarning as u8,
                ResourceAlertState::Unavailable as u8,
            ],
            [1, 2, 3, 4, 5]
        );
        assert_eq!(
            [
                ResourceEnforcementState::Enforced as u8,
                ResourceEnforcementState::NotRequested as u8,
                ResourceEnforcementState::Unverified as u8,
            ],
            [1, 2, 3]
        );
        assert_eq!(
            [
                RecoveryState::Healthy as u8,
                RecoveryState::Backoff as u8,
                RecoveryState::Stabilizing as u8,
                RecoveryState::CircuitOpen as u8,
                RecoveryState::Disabled as u8,
            ],
            [1, 2, 3, 4, 5]
        );
        assert_eq!(
            [
                HealthState::Disabled as u8,
                HealthState::Healthy as u8,
                HealthState::Failing as u8,
                HealthState::Unhealthy as u8,
                HealthState::Starting as u8,
            ],
            [1, 2, 3, 4, 5]
        );
    }

    #[test]
    fn recovery_backoff_is_exponential_bounded_and_opens_the_circuit() {
        let mut record = ApplicationRecord {
            schema_version: 2,
            name: "api".to_owned(),
            kind_code: 1,
            working_directory: PathBuf::from("/srv/api"),
            command: vec!["pam".to_owned(), "start".to_owned()],
            master_state_file: PathBuf::from("state.json"),
            stdout_log: PathBuf::from("out.log"),
            stderr_log: PathBuf::from("error.log"),
            log_max_bytes: DEFAULT_LOG_MAX_BYTES,
            log_retain: DEFAULT_LOG_RETAIN,
            memory_warning_bytes: None,
            task_warning_count: None,
            memory_max_bytes: None,
            task_max_count: None,
            environment_file: None,
            shutdown_timeout_millis: DEFAULT_SHUTDOWN_TIMEOUT_MILLIS,
            health_check_address: None,
            health_check_path: None,
            health_check_interval_millis: DEFAULT_HEALTH_INTERVAL_MILLIS,
            health_check_timeout_millis: DEFAULT_HEALTH_TIMEOUT_MILLIS,
            health_check_start_period_millis: 0,
            health_check_failure_threshold: DEFAULT_HEALTH_FAILURE_THRESHOLD,
            consecutive_health_failures: 0,
            last_health_check_at_millis: None,
            last_health_success_at_millis: None,
            health_state_code: HealthState::Disabled as u8,
            total_unhealthy_restart_count: 0,
            desired_state_code: ApplicationState::Online as u8,
            auto_restart: true,
            restart_delay_millis: 100,
            restart_backoff_max_millis: 250,
            max_unstable_restarts: 3,
            min_uptime_millis: 1_000,
            unstable_restart_count: 0,
            total_auto_restart_count: 0,
            next_restart_at_millis: None,
            recovery_state_code: RecoveryState::Healthy as u8,
            created_at_millis: 1,
        };
        for (now, deadline) in [(1_000, 1_100), (2_000, 2_200), (3_000, 3_250)] {
            schedule_recovery(&mut record, now);
            assert_eq!(record.recovery_state_code, RecoveryState::Backoff as u8);
            assert_eq!(record.next_restart_at_millis, Some(deadline));
        }
        schedule_recovery(&mut record, 4_000);
        assert_eq!(record.recovery_state_code, RecoveryState::CircuitOpen as u8);
        assert_eq!(record.next_restart_at_millis, None);
    }

    #[test]
    fn supervision_wakes_for_the_earliest_backoff_without_busy_spinning() {
        assert_eq!(earliest_deadline(None, Some(1_050)), Some(1_050));
        assert_eq!(earliest_deadline(Some(1_080), Some(1_020)), Some(1_020));
        assert_eq!(earliest_deadline(Some(1_020), None), Some(1_020));
        assert_eq!(next_supervision_delay(1_000, None), SUPERVISION_INTERVAL);
        assert_eq!(
            next_supervision_delay(1_000, Some(1_010)),
            Duration::from_millis(10)
        );
        assert_eq!(
            next_supervision_delay(1_000, Some(2_000)),
            SUPERVISION_INTERVAL
        );
        assert_eq!(
            next_supervision_delay(1_000, Some(999)),
            Duration::from_millis(1)
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn pidfd_reports_the_registered_master_exit_without_pid_polling() {
        let mut child = Command::new("/bin/sleep").arg("10").spawn().unwrap();
        let descriptor = open_pidfd(child.id()).expect("Linux pidfd support");
        let mut watchers = MasterWatchers {
            poll_descriptors: vec![libc::pollfd {
                fd: descriptor.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            }],
            _descriptors: vec![descriptor],
        };
        assert!(!watchers.exit_ready());
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(watchers.exit_ready());
    }

    #[test]
    fn shutdown_escalates_when_a_master_ignores_sigterm() {
        let mut child = Command::new("/bin/sh")
            .args(["-c", "trap '' TERM; while :; do :; done"])
            .spawn()
            .expect("spawn signal-resistant master");
        thread::sleep(Duration::from_millis(50));
        let state = MasterState {
            version: 1,
            pid: child.id(),
            process_start: crate::cluster::linux_process_start(child.id()),
            workers: 1,
            admin_address: None,
            started_at_millis: epoch_millis(),
        };

        assert!(terminate_master(&state, MIN_SHUTDOWN_TIMEOUT_MILLIS).unwrap());
        let _ = child.wait();
        assert!(!master_is_running(&state));
    }

    #[test]
    fn shutdown_timeout_policy_is_bounded() {
        assert!(validate_shutdown_policy(MIN_SHUTDOWN_TIMEOUT_MILLIS).is_ok());
        assert!(validate_shutdown_policy(MAX_SHUTDOWN_TIMEOUT_MILLIS).is_ok());
        assert!(validate_shutdown_policy(MIN_SHUTDOWN_TIMEOUT_MILLIS - 1).is_err());
        assert!(validate_shutdown_policy(MAX_SHUTDOWN_TIMEOUT_MILLIS + 1).is_err());
    }

    #[test]
    fn health_start_period_suppresses_liveness_until_elapsed() {
        let state = MasterState {
            version: 1,
            pid: 1,
            process_start: None,
            workers: 1,
            admin_address: None,
            started_at_millis: 10_000,
        };
        assert!(!health_start_period_elapsed(&state, 30_000, 39_999));
        assert!(health_start_period_elapsed(&state, 30_000, 40_000));
        assert!(health_start_period_elapsed(&state, 0, 10_000));
        assert!(!health_start_period_elapsed(&state, 30_000, 9_000));
        assert!(validate_health_start_period(3_600_000, true).is_ok());
        assert!(validate_health_start_period(3_600_001, true).is_err());
        assert!(validate_health_start_period(1, false).is_err());
    }

    #[test]
    fn environment_files_are_literal_private_and_cannot_redirect_manager_state() {
        let directory = std::env::temp_dir().join(format!(
            "pam-env-contract-{}-{}",
            std::process::id(),
            epoch_millis()
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("production.env");
        fs::write(
            &path,
            "# comment\nexport APP_MODE=production\nLITERAL='$HOME'\nEMPTY=\n",
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let environment = load_environment_file(&path).unwrap();
        assert_eq!(environment["APP_MODE"], "production");
        assert_eq!(environment["LITERAL"], "$HOME");
        assert_eq!(environment["EMPTY"], "");

        fs::write(&path, "PAM_MANAGER_STATE_DIR=/tmp/redirected\n").unwrap();
        assert!(
            load_environment_file(&path)
                .unwrap_err()
                .contains("reserved")
        );
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn health_checks_are_loopback_bounded_and_do_not_accept_header_injection() {
        let (address, path) =
            parse_health_check_url("http://127.0.0.1:8080/health?full=1").unwrap();
        assert!(address.ip().is_loopback());
        assert_eq!(path, "/health?full=1");
        for rejected in [
            "https://127.0.0.1:8080/health",
            "http://example.com:8080/health",
            "http://192.0.2.1:8080/health",
            "http://127.0.0.1:8080/health%0d%0aHost:evil",
            "http://127.0.0.1/health",
        ] {
            assert!(parse_health_check_url(rejected).is_err(), "{rejected}");
        }
    }
    #[test]
    fn rotated_log_names_are_stable() {
        assert_eq!(
            rotated_log_path(Path::new("api.log"), 3),
            PathBuf::from("api.log.3")
        );
    }
    #[test]
    fn worker_option_is_replaced_or_inserted_before_application_arguments() {
        let mut existing = vec![
            "pam".to_owned(),
            "start".to_owned(),
            "--workers".to_owned(),
            "1".to_owned(),
        ];
        set_command_option(&mut existing, "--workers", "4");
        assert_eq!(existing, ["pam", "start", "--workers", "4"]);

        let mut missing = vec![
            "pam".to_owned(),
            "start".to_owned(),
            "--".to_owned(),
            "--port=1".to_owned(),
        ];
        set_command_option(&mut missing, "--workers", "2");
        assert_eq!(
            missing,
            ["pam", "start", "--workers", "2", "--", "--port=1"]
        );
    }

    #[test]
    fn systemd_unit_runs_the_daemon_in_the_foreground() {
        let unit = systemd_unit(Path::new("/opt/PAM Runtime/pam")).unwrap();
        assert!(unit.contains("Type=simple"));
        assert!(unit.contains("ExecStart=/opt/PAM\\x20Runtime/pam __pamd"));
        assert!(unit.contains("NoNewPrivileges=true"));
        assert!(!unit.contains("ProtectHome="));
    }

    #[test]
    fn hard_limits_use_a_unique_fail_closed_systemd_scope() {
        let command = managed_launch_command(
            OsStr::new("/opt/pam"),
            &[OsString::from("start"), OsString::from("index.php")],
            Path::new("/srv/api"),
            "api",
            Some(268_435_456),
            Some(64),
        )
        .unwrap();
        assert_eq!(command.get_program(), OsStr::new("/usr/bin/systemd-run"));
        let arguments = command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(arguments.iter().any(|value| value == "MemoryMax=268435456"));
        assert!(arguments.iter().any(|value| value == "TasksMax=64"));
        assert!(
            arguments
                .iter()
                .any(|value| value.starts_with("--unit=pam-api-"))
        );
        assert_eq!(arguments.iter().filter(|value| *value == "--").count(), 1);
    }

    #[test]
    fn ecosystem_contract_rejects_unknown_fields_and_string_kinds() {
        let unknown = r#"schema_version=1
[applications.api]
kind_code=1
surprise=true
"#;
        assert!(toml::from_str::<EcosystemConfig>(unknown).is_err());
        let string_kind = r#"schema_version=1
[applications.api]
kind_code="runtime"
"#;
        assert!(toml::from_str::<EcosystemConfig>(string_kind).is_err());
        let limited = r#"schema_version=1
[applications.api]
kind_code=1
memory_warning_bytes=1024
memory_max_bytes=2048
task_warning_count=8
task_max_count=16
env_file=".env.production"
health_check_url="http://127.0.0.1:8080/health"
health_check_interval_millis=5000
health_check_timeout_millis=1000
health_check_start_period_millis=30000
health_check_failure_threshold=3
"#;
        let limited = toml::from_str::<EcosystemConfig>(limited).unwrap();
        let api = &limited.applications["api"];
        assert_eq!(api.memory_max_bytes, Some(2048));
        assert_eq!(api.task_max_count, Some(16));
        assert_eq!(api.env_file, Some(PathBuf::from(".env.production")));
        assert_eq!(api.health_check_failure_threshold, 3);
        assert_eq!(api.health_check_start_period_millis, 30_000);
        assert!(validate_resource_policy(Some(3), None, Some(2), None, "api").is_err());
    }

    #[test]
    fn deployment_history_retention_is_bounded() {
        let mut history = DeploymentHistory {
            schema_version: 1,
            name: "api".to_owned(),
            entries: Vec::new(),
        };
        for index in 0..=MAX_DEPLOY_HISTORY {
            append_deployment_entry(
                &mut history,
                DeploymentEntry {
                    release_directory: PathBuf::from(index.to_string()),
                    activated_at_millis: index as u64,
                    event_kind_code: DeploymentEventKind::Deploy as u8,
                },
            );
        }
        assert_eq!(history.entries.len(), MAX_DEPLOY_HISTORY);
        assert_eq!(history.entries[0].release_directory, PathBuf::from("1"));
    }

    #[test]
    fn resource_history_retention_is_bounded() {
        let mut history = ResourceHistory {
            schema_version: 1,
            name: "api".to_owned(),
            entries: Vec::new(),
        };
        for index in 0..=MAX_RESOURCE_HISTORY {
            append_resource_entry(
                &mut history,
                ResourceHistoryEntry {
                    observed_at_millis: index as u64,
                    state_code: ApplicationState::Online as u8,
                    workers: 1,
                    rss_bytes: index as u64,
                    tasks: 1,
                    alert_state_code: ResourceAlertState::Healthy as u8,
                },
            );
        }
        assert_eq!(history.entries.len(), MAX_RESOURCE_HISTORY);
        assert_eq!(history.entries[0].observed_at_millis, 1);
    }
}
