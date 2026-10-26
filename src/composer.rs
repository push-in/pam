use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::php::PhpRuntime;

const COMPOSER_BOOTSTRAP: &str = include_str!("../runtime/composer_bootstrap.php");
const MAX_COMPOSER_SIGNATURE_BYTES: usize = 1_024;
const MAX_COMPOSER_INSTALLER_BYTES: usize = 1_048_576;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComposerProject {
    pub root: PathBuf,
    pub manifest: PathBuf,
    pub vendor_directory: PathBuf,
    pub autoload: PathBuf,
}

pub fn discover(entry: &Path) -> Result<Option<ComposerProject>, String> {
    let start = if entry.is_dir() {
        entry
    } else {
        entry
            .parent()
            .ok_or_else(|| format!("{} has no parent directory", entry.display()))?
    };

    let mut nearest_manifest = None;
    for directory in start.ancestors() {
        let manifest = directory.join("composer.json");
        if !manifest.is_file() {
            continue;
        }

        let contents = fs::read_to_string(&manifest)
            .map_err(|error| format!("cannot read {}: {error}", manifest.display()))?;
        let composer: Value = serde_json::from_str(&contents)
            .map_err(|error| format!("invalid {}: {error}", manifest.display()))?;
        let configured_vendor = composer
            .get("config")
            .and_then(|config| config.get("vendor-dir"))
            .and_then(Value::as_str)
            .unwrap_or("vendor");
        let configured_vendor = PathBuf::from(configured_vendor);
        let vendor_directory = if configured_vendor.is_absolute() {
            configured_vendor
        } else {
            directory.join(configured_vendor)
        };

        let project = ComposerProject {
            root: directory.to_path_buf(),
            manifest,
            autoload: vendor_directory.join("autoload.php"),
            vendor_directory,
        };
        if project.autoload.is_file() {
            return Ok(Some(project));
        }
        nearest_manifest.get_or_insert(project);
    }

    Ok(nearest_manifest)
}

pub fn run(executable: &OsStr, arguments: &[OsString]) -> Result<u8, String> {
    run_with_home(executable, arguments, None)
}

pub fn run_with_home(
    executable: &OsStr,
    arguments: &[OsString],
    composer_home: Option<&Path>,
) -> Result<u8, String> {
    let composer = resolve_or_install(executable)?;
    // SAFETY: Composer execution owns the single-threaded Embed lifecycle and
    // this flag is removed before control returns to the caller.
    unsafe { env::set_var("PAM_COMPOSER_MODE", "1") };
    let previous_home = env::var_os("COMPOSER_HOME");
    if let Some(home) = composer_home {
        // SAFETY: Composer execution owns the single-threaded Embed lifecycle.
        unsafe { env::set_var("COMPOSER_HOME", home) };
    }
    let result = (|| {
        let mut runtime = PhpRuntime::initialize(executable, &composer, arguments)?;
        runtime.execute_file(&composer)
    })();
    // SAFETY: See the set_var safety note above.
    unsafe { env::remove_var("PAM_COMPOSER_MODE") };
    if composer_home.is_some() {
        // SAFETY: Restore the process environment before returning control.
        unsafe {
            if let Some(home) = previous_home {
                env::set_var("COMPOSER_HOME", home);
            } else {
                env::remove_var("COMPOSER_HOME");
            }
        }
    }
    result
}

fn resolve_or_install(executable: &OsStr) -> Result<PathBuf, String> {
    if let Some(override_path) = env::var_os("PAM_COMPOSER").filter(|path| !path.is_empty()) {
        let path = PathBuf::from(override_path);
        return path
            .is_file()
            .then_some(path.clone())
            .ok_or_else(|| format!("PAM_COMPOSER is not a file: {}", path.display()));
    }

    let cached = composer_cache()?.join("composer.phar");
    if cached.is_file() {
        return Ok(cached);
    }

    if let Some(path) = executable_in_path("composer") {
        return Ok(path);
    }

    install_composer(executable, &cached)?;
    Ok(cached)
}

fn install_composer(executable: &OsStr, target: &Path) -> Result<(), String> {
    let directory = target
        .parent()
        .ok_or_else(|| "Composer cache path has no parent directory".to_owned())?;
    fs::create_dir_all(directory).map_err(|error| {
        format!(
            "cannot create Composer cache {}: {error}",
            directory.display()
        )
    })?;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).map_err(|error| {
        format!(
            "cannot secure Composer cache {}: {error}",
            directory.display()
        )
    })?;

    // Per-process names: two concurrent `pam composer` bootstraps must not
    // verify one process's signature against the other's installer.
    let pid = std::process::id();
    let bootstrap = directory.join(format!("pam-composer-bootstrap-{pid}.php"));
    let signature = directory.join(format!("composer-installer-{pid}.sig"));
    let installer = directory.join(format!("composer-setup-{pid}.php"));
    fs::write(&bootstrap, COMPOSER_BOOTSTRAP)
        .map_err(|error| format!("cannot write Composer bootstrap: {error}"))?;
    fs::set_permissions(&bootstrap, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("cannot secure Composer bootstrap: {error}"))?;

    eprintln!("Pam is downloading and verifying Composer...");
    let downloads = download_composer_file(
        "https://composer.github.io/installer.sig",
        &signature,
        MAX_COMPOSER_SIGNATURE_BYTES,
    )
    .and_then(|()| {
        download_composer_file(
            "https://getcomposer.org/installer",
            &installer,
            MAX_COMPOSER_INSTALLER_BYTES,
        )
    });
    if let Err(error) = downloads {
        let _ = fs::remove_file(&bootstrap);
        let _ = fs::remove_file(&signature);
        let _ = fs::remove_file(&installer);
        return Err(error);
    }
    let arguments = [
        target.as_os_str().to_os_string(),
        signature.as_os_str().to_os_string(),
        installer.as_os_str().to_os_string(),
    ];
    // SAFETY: Bootstrap owns the single-threaded Embed lifecycle.
    unsafe { env::set_var("PAM_COMPOSER_MODE", "1") };
    let result = (|| {
        let mut runtime = PhpRuntime::initialize(executable, &bootstrap, &arguments)?;
        runtime.execute_file(&bootstrap)
    })();
    // SAFETY: See the set_var safety note above.
    unsafe { env::remove_var("PAM_COMPOSER_MODE") };
    let _ = fs::remove_file(&bootstrap);
    let _ = fs::remove_file(&signature);
    let _ = fs::remove_file(&installer);
    let status = result?;
    if status != 0 || !target.is_file() {
        return Err(format!("Composer bootstrap failed with status {status}"));
    }
    fs::set_permissions(target, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("cannot secure Composer PHAR: {error}"))?;
    eprintln!("Composer installed at {}", target.display());
    Ok(())
}

fn download_composer_file(url: &str, target: &Path, maximum_bytes: usize) -> Result<(), String> {
    let _ = fs::remove_file(target);
    let output = Command::new("curl")
        .args([
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--tlsv1.2",
            "--connect-timeout",
            "15",
            "--max-time",
            "120",
            "--max-filesize",
            &maximum_bytes.to_string(),
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--output",
        ])
        .arg(target)
        .arg(url)
        .output()
        .map_err(|error| format!("cannot download Composer bootstrap with curl: {error}"))?;
    if !output.status.success() {
        let _ = fs::remove_file(target);
        return Err(format!(
            "Composer bootstrap download failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let metadata = fs::symlink_metadata(target)
        .map_err(|error| format!("cannot inspect Composer bootstrap download: {error}"))?;
    if !metadata.file_type().is_file()
        || metadata.len() == 0
        || metadata.len() > maximum_bytes as u64
    {
        let _ = fs::remove_file(target);
        return Err("Composer bootstrap download is empty, unsafe, or oversized".to_owned());
    }
    fs::set_permissions(target, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("cannot secure Composer bootstrap download: {error}"))
}

fn composer_cache() -> Result<PathBuf, String> {
    if let Some(cache) = env::var_os("XDG_CACHE_HOME") {
        return Ok(PathBuf::from(cache).join("pam"));
    }
    env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".cache/pam"))
        .ok_or_else(|| "cannot locate the user cache; set XDG_CACHE_HOME".to_owned())
}

fn executable_in_path(name: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|path| {
        env::split_paths(&path)
            .map(|directory| directory.join(name))
            .find(|candidate| candidate.is_file())
    })
}
