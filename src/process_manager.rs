use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::Shutdown;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::cluster::{
    MasterState, RELOAD_SIGNAL, STOP_SIGNAL, master_is_running, read_master_state, signal_master,
};

const MAX_RECORD_BYTES: u64 = 1_048_576;
const MAX_APPLICATIONS: usize = 1_024;
const DEFAULT_LOG_MAX_BYTES: u64 = 10 * 1024 * 1024;
const DEFAULT_LOG_RETAIN: usize = 5;
const MAX_LOG_RETAIN: usize = 100;
const MAX_DAEMON_MESSAGE_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[repr(u8)]
enum DaemonOperation {
    Ping = 1,
    Stop = 2,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DaemonRequest {
    schema_version: u8,
    operation_code: u8,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DaemonResponse {
    schema_version: u8,
    ok: bool,
    pid: u32,
    message: String,
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
    created_at_millis: u64,
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum ReconcileAction {
    Created = 1,
    Unchanged = 2,
    Scaled = 3,
    Restarted = 4,
    Disabled = 5,
}

pub fn run(
    executable: &OsStr,
    command: &str,
    arguments: impl Iterator<Item = OsString>,
) -> Result<u8, String> {
    match command {
        "up" => up(executable, arguments.collect()),
        "ps" => list(arguments.collect()),
        "status" | "describe" => inspect(command, arguments.collect()),
        "reload" => signal(arguments.collect(), RELOAD_SIGNAL, "reloading"),
        "restart" => restart(executable, arguments.collect()),
        "scale" => scale(executable, arguments.collect()),
        "stop" => stop(arguments.collect()),
        "delete" => delete(arguments.collect()),
        "logs" => logs(arguments.collect()),
        "save" => save(arguments.collect()),
        "resurrect" => resurrect(executable, arguments.collect()),
        "startup" => startup(executable, arguments.collect()),
        "monit" => monit(arguments.collect()),
        "apply" => apply_ecosystem(executable, arguments.collect()),
        "config:check" => check_ecosystem(arguments.collect()),
        "daemon" => daemon(executable, arguments.collect()),
        "__pamd" => daemon_serve(executable),
        _ => Err(format!(
            "unsupported PAM process-manager command: {command}"
        )),
    }
}

fn apply_ecosystem(executable: &OsStr, arguments: Vec<OsString>) -> Result<u8, String> {
    let (path, json) = parse_config_arguments(arguments, "apply")?;
    let (config, root) = load_ecosystem(&path)?;
    let paths = ManagerPaths::load()?;
    let mut results = Vec::new();
    for (name, application) in config.applications {
        validate_ecosystem_application(&root, &name, &application)?;
        let action = if !application.autostart {
            let record_path = paths.application(&name);
            if record_path.exists() {
                let record = read_record(&record_path)?;
                if running_state(&record)
                    .as_ref()
                    .is_some_and(master_is_running)
                {
                    let mut command = Command::new(executable);
                    command.args(["stop", &name]);
                    run_reconcile_command(command, &name)?;
                }
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
                let record = read_record(&record_path)?;
                let state = running_state(&record);
                if !state.as_ref().is_some_and(master_is_running) {
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
                desired_state_code: if running_state(record)
                    .as_ref()
                    .is_some_and(master_is_running)
                {
                    ApplicationState::Online as u8
                } else {
                    ApplicationState::Stopped as u8
                },
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
        let record = read_record(&paths.application(&saved.name))?;
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
        println!("PAM MONIT\nNAME\tSTATE\tPID\tWORKERS");
        for record in records {
            let state = running_state(&record);
            let online = state.as_ref().is_some_and(master_is_running);
            println!(
                "{}\t{}\t{}\t{}",
                record.name,
                if online { "online" } else { "stopped" },
                state.as_ref().map_or(0, |value| value.pid),
                state.as_ref().map_or(0, |value| value.workers)
            );
        }
    }
    Ok(0)
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
                    println!("pamd is online (PID {})", response.pid);
                    return Ok(0);
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err("pamd did not become ready; inspect pamd.error.log".to_owned())
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
    let dump = ManagerPaths::load()?.base.join("dump.json");
    if dump.exists()
        && let Err(error) = resurrect_saved(executable)
    {
        eprintln!("pamd could not restore saved applications: {error}");
    }
    let own_uid = unsafe { libc::geteuid() };
    for connection in listener.incoming() {
        let mut stream = connection.map_err(|error| error.to_string())?;
        if peer_uid(&stream)? != own_uid {
            continue;
        }
        let request = read_daemon_request(&mut stream)?;
        let operation = request.operation_code;
        let stop = operation == DaemonOperation::Stop as u8;
        let valid = request.schema_version == 1
            && matches!(operation, value if value == DaemonOperation::Ping as u8 || value == DaemonOperation::Stop as u8);
        let response = DaemonResponse {
            schema_version: 1,
            ok: valid,
            pid: std::process::id(),
            message: if valid { "ok" } else { "unsupported request" }.to_owned(),
        };
        serde_json::to_writer(&mut stream, &response).map_err(|error| error.to_string())?;
        stream.write_all(b"\n").map_err(|error| error.to_string())?;
        if valid && stop {
            break;
        }
    }
    fs::remove_file(&socket).map_err(|error| format!("cannot remove daemon socket: {error}"))?;
    Ok(0)
}

fn daemon_request(operation: DaemonOperation) -> Result<DaemonResponse, String> {
    let mut stream = UnixStream::connect(daemon_socket_path()?)
        .map_err(|error| format!("cannot connect to pamd: {error}"))?;
    serde_json::to_writer(
        &mut stream,
        &DaemonRequest {
            schema_version: 1,
            operation_code: operation as u8,
        },
    )
    .map_err(|error| error.to_string())?;
    stream.write_all(b"\n").map_err(|error| error.to_string())?;
    stream
        .shutdown(Shutdown::Write)
        .map_err(|error| error.to_string())?;
    let response: DaemonResponse =
        serde_json::from_reader(stream).map_err(|error| error.to_string())?;
    if response.schema_version != 1 || !response.ok {
        return Err(response.message);
    }
    Ok(response)
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

fn up(executable: &OsStr, arguments: Vec<OsString>) -> Result<u8, String> {
    let mut name = None;
    let mut target = None;
    let mut workers = None;
    let mut attach = false;
    let mut json = false;
    let mut log_max_bytes = DEFAULT_LOG_MAX_BYTES;
    let mut log_retain = DEFAULT_LOG_RETAIN;
    let mut application_arguments = Vec::new();
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--name" => name = Some(required_utf8(arguments.next(), "--name")?),
            "--workers" => workers = Some(required_positive(arguments.next(), "--workers")?),
            "--attach" => attach = true,
            "--json" => json = true,
            "--log-max-bytes" => {
                log_max_bytes = required_positive_u64(arguments.next(), "--log-max-bytes")?
            }
            "--log-retain" => {
                log_retain = required_positive(arguments.next(), "--log-retain")?;
                if log_retain > MAX_LOG_RETAIN {
                    return Err(format!("--log-retain cannot exceed {MAX_LOG_RETAIN}"));
                }
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

    let cwd = fs::canonicalize(std::env::current_dir().map_err(|error| error.to_string())?)
        .map_err(|error| format!("cannot resolve application directory: {error}"))?;
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
    let mut command = Command::new(executable);
    command.args(&launch_arguments);
    command.current_dir(&cwd);
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
        if Instant::now() >= deadline {
            return Err(format!(
                "application {name:?} did not become ready; inspect {}",
                stderr_log.display()
            ));
        }
        thread::sleep(Duration::from_millis(50));
    };
    let record = ApplicationRecord {
        schema_version: 1,
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
    let record = record(&name)?;
    let Some(state) = running_state(&record) else {
        return Ok(0);
    };
    if master_is_running(&state) {
        signal_master(&state, STOP_SIGNAL)?;
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    while master_is_running(&state) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(50));
    }
    if master_is_running(&state) {
        return Err(format!("application {name:?} did not stop in 20 seconds"));
    }
    if json {
        println!(
            "{}",
            serde_json::json!({"schemaVersion":1,"name":name,"stateCode":ApplicationState::Stopped as u8})
        );
    } else {
        println!("Stopped {name}");
    }
    Ok(0)
}

fn restart(executable: &OsStr, arguments: Vec<OsString>) -> Result<u8, String> {
    let (name, json) = parse_name_json(arguments, "restart")?;
    let record = record(&name)?;
    restart_record(executable, &record, json, true)
}

fn restart_record(
    executable: &OsStr,
    record: &ApplicationRecord,
    json: bool,
    emit: bool,
) -> Result<u8, String> {
    let name = &record.name;
    if let Some(state) = running_state(record)
        && master_is_running(&state)
    {
        signal_master(&state, STOP_SIGNAL)?;
        let deadline = Instant::now() + Duration::from_secs(20);
        while master_is_running(&state) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        if master_is_running(&state) {
            return Err(format!("application {name:?} did not stop before restart"));
        }
    }
    if record.command.len() < 2 {
        return Err(format!("application {name:?} has no restart command"));
    }
    rotate_log(&record.stdout_log, record.log_max_bytes, record.log_retain)?;
    rotate_log(&record.stderr_log, record.log_max_bytes, record.log_retain)?;
    let stdout = secure_append(&record.stdout_log)?;
    let stderr = secure_append(&record.stderr_log)?;
    let mut command = Command::new(executable);
    command
        .args(&record.command[1..])
        .current_dir(&record.working_directory)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr);
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
    command
        .spawn()
        .map_err(|error| format!("cannot restart {name:?}: {error}"))?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let state = loop {
        if let Some(state) = running_state(record)
            && master_is_running(&state)
        {
            break state;
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
            println!("{}", application_json(record, Some(&state)));
        } else {
            println!("Restarted {name} (PID {})", state.pid);
        }
    }
    Ok(0)
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
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--lines" => lines = required_positive(arguments.next(), "--lines")?,
            "--errors" => errors = true,
            "--both" => both = true,
            "--follow" | "-f" => follow = true,
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
    let paths = if both {
        vec![record.stdout_log, record.stderr_log]
    } else if errors {
        vec![record.stderr_log]
    } else {
        vec![record.stdout_log]
    };
    for path in &paths {
        print_tail(path, lines.min(100_000))?;
    }
    if follow {
        follow_logs(&paths)?;
    }
    Ok(0)
}

fn follow_logs(paths: &[PathBuf]) -> Result<(), String> {
    let mut offsets = paths
        .iter()
        .map(|path| {
            fs::metadata(path)
                .map(|metadata| metadata.len())
                .unwrap_or(0)
        })
        .collect::<Vec<_>>();
    loop {
        for (index, path) in paths.iter().enumerate() {
            let length = fs::metadata(path)
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            if length < offsets[index] {
                offsets[index] = 0;
            }
            if length > offsets[index] {
                let mut file = File::open(path)
                    .map_err(|error| format!("cannot follow log {}: {error}", path.display()))?;
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
    let mut file =
        File::open(path).map_err(|error| format!("cannot open log {}: {error}", path.display()))?;
    let length = file.metadata().map_err(|error| error.to_string())?.len();
    let window = length.min(8 * 1024 * 1024);
    file.seek(SeekFrom::Start(length - window))
        .map_err(|error| error.to_string())?;
    let mut text = String::new();
    file.read_to_string(&mut text)
        .map_err(|error| format!("log is not valid UTF-8: {error}"))?;
    let selected = text.lines().rev().take(lines).collect::<Vec<_>>();
    for line in selected.into_iter().rev() {
        println!("{line}");
    }
    Ok(())
}

fn application_json(record: &ApplicationRecord, state: Option<&MasterState>) -> serde_json::Value {
    let online = state.is_some_and(master_is_running);
    serde_json::json!({
        "schemaVersion": 1,
        "name": record.name,
        "kindCode": record.kind_code,
        "stateCode": if online { ApplicationState::Online as u8 } else { ApplicationState::Stopped as u8 },
        "pid": state.map(|state| state.pid),
        "workers": state.map(|state| state.workers),
        "startedAtMillis": state.map(|state| state.started_at_millis),
        "workingDirectory": record.working_directory,
        "stdoutLog": record.stdout_log,
        "stderrLog": record.stderr_log,
    })
}

struct ManagerPaths {
    base: PathBuf,
    applications: PathBuf,
    runtime: PathBuf,
    logs: PathBuf,
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
        };
        for path in [&paths.applications, &paths.runtime, &paths.logs] {
            secure_directory(path)?;
        }
        Ok(paths)
    }
    fn application(&self, name: &str) -> PathBuf {
        self.applications.join(format!("{name}.json"))
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
    let record: ApplicationRecord =
        serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
            .map_err(|error| format!("invalid application record: {error}"))?;
    if record.schema_version != 1 || record.kind_code == 0 || record.kind_code > 2 {
        return Err("unsupported application record contract".to_owned());
    }
    validate_name(&record.name)?;
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
            ],
            [1, 2, 3, 4, 5]
        );
        assert_eq!(
            [
                ApplicationState::Online as u8,
                ApplicationState::Stopped as u8
            ],
            [1, 2]
        );
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
    }
}
