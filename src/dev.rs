use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, UNIX_EPOCH};

use crate::terminal::Terminal;

const WATCH_INTERVAL: Duration = Duration::from_millis(250);
const RELOAD_DEBOUNCE: Duration = Duration::from_millis(150);
const CHILD_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const SIGINT: i32 = 2;

unsafe extern "C" {
    fn kill(process_id: i32, signal: i32) -> i32;
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FileFingerprint {
    length: u64,
    modified_nanos: Option<u128>,
}

pub fn run(script: &Path, arguments: &[OsString]) -> Result<u8, String> {
    let executable =
        env::current_exe().map_err(|error| format!("cannot locate the Pam executable: {error}"))?;
    let watch_root = script
        .parent()
        .ok_or_else(|| "the PHP script has no parent directory".to_owned())?;
    let stopped = install_ctrl_c_listener()?;
    let mut files = snapshot(watch_root)?;
    let mut child = Some(spawn_child(&executable, script, arguments)?);
    let ui = Terminal::stderr();

    eprintln!(
        "{}  {}",
        ui.brand("PAM / DEV MATRIX"),
        ui.muted("hot reload active")
    );
    eprintln!("{}", ui.rule());
    eprintln!(
        "{} {}",
        ui.status("ok", "Watching"),
        ui.command(watch_root.display())
    );
    eprintln!("{}", ui.muted("  Ctrl+C stops the development runtime."));

    while !stopped.load(Ordering::SeqCst) {
        thread::sleep(WATCH_INTERVAL);
        let current_files = snapshot(watch_root)?;

        if current_files != files {
            thread::sleep(RELOAD_DEBOUNCE);
            files = snapshot(watch_root)?;
            eprintln!(
                "\n{}",
                ui.status("warn", "Change detected · reloading runtime")
            );

            if let Some(mut running_child) = child.take() {
                stop_child(&mut running_child);
            }
            child = Some(spawn_child(&executable, script, arguments)?);
            continue;
        }

        if let Some(running_child) = child.as_mut()
            && let Some(status) = running_child
                .try_wait()
                .map_err(|error| format!("cannot inspect the Pam child process: {error}"))?
        {
            eprintln!(
                "{}",
                ui.status(
                    "warn",
                    format!("Runtime exited with {status} · waiting for a file change")
                )
            );
            child = None;
        }
    }

    if let Some(mut running_child) = child {
        stop_child(&mut running_child);
    }

    eprintln!("{}", ui.status("info", "Development runtime stopped"));
    Ok(0)
}

fn spawn_child(executable: &Path, script: &Path, arguments: &[OsString]) -> Result<Child, String> {
    Command::new(executable)
        .arg(script)
        .args(arguments)
        .env("PAM_DEV_CHILD", "1")
        .spawn()
        .map_err(|error| format!("cannot start the Pam development process: {error}"))
}

fn stop_child(child: &mut Child) {
    if child.try_wait().ok().flatten().is_some() {
        return;
    }

    // SAFETY: The process ID belongs to the child created by this supervisor.
    unsafe {
        kill(child.id() as i32, SIGINT);
    }

    let deadline = std::time::Instant::now() + CHILD_SHUTDOWN_TIMEOUT;
    while std::time::Instant::now() < deadline {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }

    let _ = child.kill();
    let _ = child.wait();
}

fn install_ctrl_c_listener() -> Result<Arc<AtomicBool>, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("cannot create the development signal listener: {error}"))?;
    let stopped = Arc::new(AtomicBool::new(false));
    let signal_stopped = Arc::clone(&stopped);

    thread::spawn(move || {
        runtime.block_on(async move {
            if let Err(error) = tokio::signal::ctrl_c().await {
                eprintln!("pam: failed to install Ctrl-C handler: {error}");
            }
            signal_stopped.store(true, Ordering::SeqCst);
        });
    });

    Ok(stopped)
}

fn snapshot(root: &Path) -> Result<BTreeMap<PathBuf, FileFingerprint>, String> {
    let mut files = BTreeMap::new();
    visit_directory(root, &mut files)?;
    Ok(files)
}

fn visit_directory(
    directory: &Path,
    files: &mut BTreeMap<PathBuf, FileFingerprint>,
) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("cannot watch {}: {error}", directory.display()))?;

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                eprintln!("pam: skipped an unreadable directory entry: {error}");
                continue;
            }
        };
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                eprintln!("pam: cannot inspect {}: {error}", path.display());
                continue;
            }
        };

        if file_type.is_dir() {
            if !is_ignored_directory(&path) {
                visit_directory(&path, files)?;
            }
        } else if file_type.is_file() && is_watched_file(&path) {
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(error) => {
                    eprintln!("pam: cannot inspect {}: {error}", path.display());
                    continue;
                }
            };
            let modified_nanos = metadata
                .modified()
                .ok()
                .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_nanos());

            files.insert(
                path,
                FileFingerprint {
                    length: metadata.len(),
                    modified_nanos,
                },
            );
        }
    }

    Ok(())
}

fn is_ignored_directory(path: &Path) -> bool {
    path.file_name().is_some_and(|name| {
        matches!(
            name.to_str(),
            Some(
                ".git"
                    | ".idea"
                    | ".pam"
                    | "dist"
                    | "node_modules"
                    | "storage"
                    | "target"
                    | "vendor"
            )
        )
    }) || (path.file_name() == Some(std::ffi::OsStr::new("cache"))
        && path.parent().and_then(Path::file_name) == Some(std::ffi::OsStr::new("bootstrap")))
}

fn is_watched_file(path: &Path) -> bool {
    if path.extension().is_some_and(|extension| extension == "php") {
        return true;
    }

    path.file_name().is_some_and(|name| {
        matches!(
            name.to_str(),
            Some(".env" | "composer.json" | "composer.lock")
        )
    })
}
