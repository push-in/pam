use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::Path;

use crate::composer;

pub fn list(project: Option<&Path>, json: bool) -> Result<u8, String> {
    let installed = project
        .map(installed_packages)
        .transpose()?
        .unwrap_or_default();
    if json {
        let packages = installed
            .iter()
            .map(|composer| serde_json::json!({"composer": composer, "installed": true}))
            .collect::<Vec<_>>();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "schema": 1,
                "packages": packages,
            }))
            .map_err(|error| error.to_string())?
        );
        return Ok(0);
    }
    if installed.is_empty() {
        println!("No direct Composer packages are installed in this project.");
    } else {
        for package in installed {
            println!("{package}");
        }
    }
    Ok(0)
}

/// Compatibility alias for `pam composer require`.
///
/// PAM intentionally owns no package registry, package allowlist, artifact store,
/// or package-specific post-install hooks. Composer and the package own them.
pub fn add(executable: &OsStr, project: &Path, requested: &str) -> Result<u8, String> {
    let name = composer_package_name(requested)?;
    in_project(project, || {
        println!("Checking metadata for {name}...");
        composer_success(
            executable,
            &["show", name, "--all"],
            "package metadata lookup",
        )?;
        println!("Validating dependency compatibility without changing the project...");
        composer_success(
            executable,
            &["require", requested, "--dry-run", "--no-interaction"],
            "Composer dependency preflight",
        )?;
        println!("Installing {requested}...");
        composer_success(
            executable,
            &["require", requested, "--no-interaction"],
            "Composer install",
        )
    })?;
    println!("Installed {name} with Composer.");
    Ok(0)
}

/// Compatibility alias for `pam composer remove`.
pub fn remove(executable: &OsStr, project: &Path, requested: &str) -> Result<u8, String> {
    let name = composer_package_name(requested)?;
    in_project(project, || {
        composer_success(
            executable,
            &["remove", name, "--dry-run", "--no-interaction"],
            "Composer removal preflight",
        )?;
        composer_success(
            executable,
            &["remove", name, "--no-interaction"],
            "Composer removal",
        )
    })?;
    println!("Removed {name} with Composer.");
    Ok(0)
}

pub fn repair_dependencies(executable: &OsStr, project: &Path) -> Result<bool, String> {
    if !project.join("composer.json").is_file() || project.join("vendor/autoload.php").is_file() {
        return Ok(false);
    }
    in_project(project, || {
        println!("Composer dependencies are missing; validating the locked install...");
        composer_success(
            executable,
            &["install", "--dry-run", "--no-interaction"],
            "Composer install preflight",
        )?;
        composer_success(
            executable,
            &["install", "--no-interaction", "--prefer-dist"],
            "Composer install",
        )
    })?;
    Ok(true)
}

fn composer_package_name(requirement: &str) -> Result<&str, String> {
    let name = requirement
        .split_once(':')
        .map_or(requirement, |(name, _)| name);
    let mut parts = name.split('/');
    let vendor = parts.next().unwrap_or_default();
    let package = parts.next().unwrap_or_default();
    if parts.next().is_some()
        || vendor.is_empty()
        || package.is_empty()
        || !name.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'/' | b'.' | b'_' | b'-')
        })
    {
        return Err(format!(
            "{requirement:?} is not a Composer package; use vendor/package (for example pushinbr/pam-native-auth)"
        ));
    }
    Ok(name)
}

fn installed_packages(project: &Path) -> Result<Vec<String>, String> {
    let path = project.join("composer.json");
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let source =
        fs::read(&path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let manifest: serde_json::Value = serde_json::from_slice(&source)
        .map_err(|error| format!("invalid {}: {error}", path.display()))?;
    let mut names = Vec::new();
    for section in ["require", "require-dev"] {
        if let Some(packages) = manifest.get(section).and_then(serde_json::Value::as_object) {
            names.extend(packages.keys().cloned());
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

fn composer_success(executable: &OsStr, arguments: &[&str], operation: &str) -> Result<(), String> {
    let arguments = arguments.iter().map(OsString::from).collect::<Vec<_>>();
    let status = composer::run(executable, &arguments)?;
    if status == 0 {
        Ok(())
    } else {
        Err(format!("{operation} failed with status {status}"))
    }
}

fn in_project<T>(
    project: &Path,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let original = std::env::current_dir().map_err(|error| error.to_string())?;
    std::env::set_current_dir(project)
        .map_err(|error| format!("cannot enter {}: {error}", project.display()))?;
    let result = operation();
    std::env::set_current_dir(&original)
        .map_err(|error| format!("cannot restore {}: {error}", original.display()))?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composer_adapter_accepts_packages_without_an_official_allowlist() {
        assert_eq!(
            composer_package_name("acme/pam-tool").unwrap(),
            "acme/pam-tool"
        );
        assert_eq!(
            composer_package_name("pushinbr/pam-native:^0.6").unwrap(),
            "pushinbr/pam-native"
        );
        assert!(composer_package_name("auth").is_err());
        assert!(composer_package_name("Pushinbr/Pam-Native").is_err());
    }
}
