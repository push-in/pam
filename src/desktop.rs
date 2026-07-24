use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

const DESKTOP_BINARY_ENV: &str = "PAM_DESKTOP_BINARY";

pub fn run(
    pam_executable: &OsStr,
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<u8, String> {
    let pam_binary = absolute_executable(pam_executable)?;
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    let desktop_binary = desktop_executable(&pam_binary, &arguments);
    let status = Command::new(&desktop_binary)
        .args(&arguments)
        .env("PAM_BINARY", &pam_binary)
        .status()
        .map_err(|error| {
            format!(
                "cannot start {}: {error}. Run `pam composer install` in the project or set {DESKTOP_BINARY_ENV}",
                Path::new(&desktop_binary).display(),
            )
        })?;

    Ok(status
        .code()
        .and_then(|code| u8::try_from(code).ok())
        .unwrap_or(1))
}

fn absolute_executable(executable: &OsStr) -> Result<PathBuf, String> {
    std::env::current_exe()
        .or_else(|_| {
            let executable = Path::new(executable);
            if executable.is_absolute() {
                Ok(executable.to_path_buf())
            } else {
                std::env::current_dir().map(|directory| directory.join(executable))
            }
        })
        .map_err(|error| format!("cannot resolve the Pam executable: {error}"))
}

fn desktop_executable(pam_binary: &Path, arguments: &[OsString]) -> OsString {
    if let Some(configured) = std::env::var_os(DESKTOP_BINARY_ENV)
        && !configured.is_empty()
    {
        return configured;
    }

    let sibling = pam_binary.with_file_name(desktop_binary_name());
    if sibling.is_file() {
        return sibling.into_os_string();
    }

    for entry in arguments
        .iter()
        .rev()
        .map(Path::new)
        .filter(|path| path.exists())
    {
        if let Some(binary) = composer_desktop_executable(entry) {
            return binary.into_os_string();
        }
    }

    if let Ok(directory) = std::env::current_dir()
        && let Some(binary) = composer_desktop_executable(&directory)
    {
        return binary.into_os_string();
    }

    OsString::from(desktop_binary_name())
}

fn composer_desktop_executable(entry: &Path) -> Option<PathBuf> {
    crate::composer::discover(entry)
        .ok()
        .flatten()
        .map(|project| {
            project
                .vendor_directory
                .join("bin")
                .join(desktop_binary_name())
        })
        .filter(|binary| binary.is_file())
}

fn desktop_binary_name() -> &'static str {
    if cfg!(windows) {
        "pam-desktop.exe"
    } else {
        "pam-desktop"
    }
}
