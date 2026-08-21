use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use crate::cluster::{
    self, RELOAD_SIGNAL, STOP_SIGNAL, StartOptions, master_is_running, read_master_state,
    signal_master,
};

const CLUSTER_OPTIONS: &[&str] = &[
    "--workers",
    "--max-requests",
    "--graceful-timeout",
    "--startup-timeout",
    "--restart-backoff",
    "--watchdog-grace",
    "--admin-address",
    "--ingress-address",
    "--pool",
    "--php-extension",
];
const CLUSTER_FLAGS: &[&str] = &["--isolate-php-extensions"];

pub fn start(
    executable: &OsStr,
    artisan: PathBuf,
    project_root: &Path,
    arguments: impl Iterator<Item = OsString>,
) -> Result<u8, String> {
    let (mut supervisor, server) = split_start_arguments(arguments)?;
    let state_path = state_path(project_root);
    supervisor.insert(0, artisan.into_os_string());
    if !supervisor.iter().any(|value| value == "--admin-address") {
        supervisor.extend([
            OsString::from("--admin-address"),
            OsString::from("127.0.0.1:0"),
        ]);
    }
    supervisor.extend([OsString::from("--state-file"), state_path.into_os_string()]);
    supervisor.push(OsString::from("--"));
    supervisor.push(OsString::from("pam:octane"));
    supervisor.extend(server);
    let options = StartOptions::parse(supervisor.into_iter())?;
    cluster::run(executable, options)
}

pub fn status(project_root: &Path) -> Result<u8, String> {
    let path = state_path(project_root);
    let state = read_master_state(&path)?;
    let running = master_is_running(&state);
    println!(
        "{}",
        serde_json::json!({
            "running": running,
            "pid": state.pid,
            "workers": state.workers,
            "adminAddress": state.admin_address,
            "startedAtMillis": state.started_at_millis,
            "stateFile": path,
        })
    );
    Ok(if running { 0 } else { 1 })
}

pub fn reload(project_root: &Path) -> Result<u8, String> {
    let state = read_master_state(&state_path(project_root))?;
    signal_master(&state, RELOAD_SIGNAL)?;
    eprintln!("PAM Octane master {} is reloading", state.pid);
    Ok(0)
}

pub fn stop(project_root: &Path) -> Result<u8, String> {
    let state = read_master_state(&state_path(project_root))?;
    signal_master(&state, STOP_SIGNAL)?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if !master_is_running(&state) {
            eprintln!("PAM Octane master {} stopped", state.pid);
            return Ok(0);
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err(format!(
        "PAM Octane master {} did not stop in 20 seconds",
        state.pid
    ))
}

pub fn state_path(project_root: &Path) -> PathBuf {
    project_root.join(".pam/octane.json")
}

fn split_start_arguments(
    arguments: impl Iterator<Item = OsString>,
) -> Result<(Vec<OsString>, Vec<OsString>), String> {
    let mut supervisor = Vec::new();
    let mut server = Vec::new();
    let mut arguments = arguments.peekable();
    while let Some(argument) = arguments.next() {
        let text = argument.to_string_lossy();
        if CLUSTER_FLAGS.contains(&text.as_ref()) {
            supervisor.push(argument);
        } else if let Some((name, value)) = text.split_once('=')
            && CLUSTER_OPTIONS.contains(&name)
        {
            if value.is_empty() {
                return Err(format!("{name} requires a value"));
            }
            supervisor.extend([OsString::from(name), OsString::from(value)]);
        } else if CLUSTER_OPTIONS.contains(&text.as_ref()) {
            let value = arguments
                .next()
                .ok_or_else(|| format!("{text} requires a value"))?;
            supervisor.extend([argument, value]);
        } else {
            server.push(argument);
        }
    }
    Ok((supervisor, server))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separates_supervisor_and_laravel_options() {
        let (supervisor, server) = split_start_arguments(
            [
                "--workers=8",
                "--isolate-php-extensions",
                "--host=0.0.0.0",
                "--port",
                "8080",
            ]
            .into_iter()
            .map(OsString::from),
        )
        .unwrap();
        assert_eq!(supervisor, ["--workers", "8", "--isolate-php-extensions"]);
        assert_eq!(server, ["--host=0.0.0.0", "--port", "8080"]);
    }
}
