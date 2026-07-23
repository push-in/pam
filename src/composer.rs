use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::php::PhpRuntime;

const COMPOSER_BOOTSTRAP: &str = include_str!("../runtime/composer_bootstrap.php");

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
    let composer = resolve_or_install(executable)?;
    // SAFETY: Composer execution owns the single-threaded Embed lifecycle and
    // this flag is removed before control returns to the caller.
    unsafe { env::set_var("PAM_COMPOSER_MODE", "1") };
    let result = (|| {
        let mut runtime = PhpRuntime::initialize(executable, &composer, arguments)?;
        runtime.execute_file(&composer)
    })();
    // SAFETY: See the set_var safety note above.
    unsafe { env::remove_var("PAM_COMPOSER_MODE") };
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

    let bootstrap = directory.join("pam-composer-bootstrap.php");
    fs::write(&bootstrap, COMPOSER_BOOTSTRAP)
        .map_err(|error| format!("cannot write Composer bootstrap: {error}"))?;
    fs::set_permissions(&bootstrap, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("cannot secure Composer bootstrap: {error}"))?;

    eprintln!("Pam is downloading and verifying Composer...");
    let arguments = [target.as_os_str().to_os_string()];
    // SAFETY: Bootstrap owns the single-threaded Embed lifecycle.
    unsafe { env::set_var("PAM_COMPOSER_MODE", "1") };
    let result = (|| {
        let mut runtime = PhpRuntime::initialize(executable, &bootstrap, &arguments)?;
        runtime.execute_file(&bootstrap)
    })();
    // SAFETY: See the set_var safety note above.
    unsafe { env::remove_var("PAM_COMPOSER_MODE") };
    let _ = fs::remove_file(&bootstrap);
    let _ = fs::remove_file(directory.join("composer-setup.php"));
    let status = result?;
    if status != 0 || !target.is_file() {
        return Err(format!("Composer bootstrap failed with status {status}"));
    }
    fs::set_permissions(target, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("cannot secure Composer PHAR: {error}"))?;
    eprintln!("Composer installed at {}", target.display());
    Ok(())
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
