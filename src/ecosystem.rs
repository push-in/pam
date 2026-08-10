use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::composer;
use crate::terminal::Terminal;

#[derive(Clone, Copy)]
struct Package {
    alias: &'static str,
    composer: &'static str,
    description: &'static str,
}

const PACKAGES: &[Package] = &[
    package(
        "native",
        "pam-native",
        "PAM Native application SDK and renderer contracts",
    ),
    package(
        "auth",
        "pam-native-auth",
        "OAuth/OIDC and secure credentials",
    ),
    package(
        "background-transfer",
        "pam-native-background-transfer",
        "Resumable uploads and downloads",
    ),
    package(
        "bluetooth",
        "pam-native-bluetooth",
        "Bluetooth device access",
    ),
    package("devtools", "pam-native-devtools", "Development diagnostics"),
    package(
        "feature-flags",
        "pam-native-feature-flags",
        "Typed feature flags",
    ),
    package(
        "firebase",
        "pam-native-firebase",
        "Firebase platform services",
    ),
    package(
        "health",
        "pam-native-health",
        "HealthKit and Health Connect",
    ),
    package(
        "intents",
        "pam-native-intents",
        "App Intents and Android intents",
    ),
    package(
        "laravel-sync",
        "pam-native-laravel-sync",
        "Laravel synchronization bridge",
    ),
    package(
        "live-activities",
        "pam-native-live-activities",
        "iOS Live Activities",
    ),
    package("maps", "pam-native-maps", "Native maps and markers"),
    package(
        "media",
        "pam-native-media",
        "Native media metadata and thumbnails",
    ),
    package("mobile-ui", "pam-mobile-ui", "Official PAM design system"),
    package("nfc", "pam-native-nfc", "NFC and NDEF sessions"),
    package("nitro", "pam-native-nitro", "Offline-first typed data"),
    package(
        "observability",
        "pam-native-observability",
        "Traces, logs, and telemetry",
    ),
    package("payments", "pam-native-payments", "Native payment sheets"),
    package(
        "plugin-kit",
        "pam-native-plugin-kit",
        "Plugin authoring and code generation",
    ),
    package(
        "realtime",
        "pam-native-realtime",
        "Realtime application transport",
    ),
    package("scanner", "pam-native-scanner", "Native barcode scanner"),
    package(
        "share-extension",
        "pam-native-share-extension",
        "Incoming share extensions",
    ),
    package(
        "subscriptions",
        "pam-native-subscriptions",
        "Store subscriptions",
    ),
    package(
        "sync",
        "pam-native-sync",
        "Offline mutation synchronization",
    ),
    package("testing", "pam-native-testing", "Native test harness"),
    package("video", "pam-native-video", "Native video playback"),
    package("widgets", "pam-native-widgets", "Android and iOS widgets"),
];

const fn package(alias: &'static str, name: &'static str, description: &'static str) -> Package {
    Package {
        alias,
        composer: name,
        description,
    }
}

pub fn list(project: Option<&Path>, json: bool) -> Result<u8, String> {
    let installed = project
        .map(installed_packages)
        .transpose()?
        .unwrap_or_default();
    if json {
        let packages = PACKAGES
            .iter()
            .map(|package| {
                let composer = format!("pushinbr/{}", package.composer);
                serde_json::json!({
                    "alias": package.alias,
                    "composer": composer,
                    "description": package.description,
                    "installed": installed.contains(&composer),
                })
            })
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
    let ui = Terminal::stdout();
    println!("{}", ui.brand("PAM / ECOSYSTEM"));
    println!("{}", ui.rule());
    println!();
    for package in PACKAGES {
        let name = format!("pushinbr/{}", package.composer);
        let status = if installed.contains(&name) {
            "installed"
        } else {
            "available"
        };
        println!(
            "  {}  {:<22} {}",
            ui.status(if status == "installed" { "ok" } else { "info" }, status),
            ui.heading(package.alias),
            ui.muted(package.description),
        );
    }
    println!();
    println!(
        "{}",
        ui.muted("Install a capability with `pam add <name>`.")
    );
    Ok(0)
}

pub fn add(executable: &OsStr, project: &Path, alias: &str) -> Result<u8, String> {
    let package = resolve(alias)?;
    let requirement = format!("pushinbr/{}:^0.6", package.composer);
    in_project(project, || {
        println!("Checking metadata for {requirement}...");
        composer_success(
            executable,
            &[
                "show",
                requirement.split(':').next().unwrap_or_default(),
                "--all",
            ],
            "package metadata lookup",
        )?;
        println!("Validating dependency compatibility without changing the project...");
        composer_success(
            executable,
            &["require", &requirement, "--dry-run", "--no-interaction"],
            "Composer dependency preflight",
        )?;
        println!("Installing {requirement}...");
        composer_success(
            executable,
            &["require", &requirement, "--no-interaction"],
            "Composer install",
        )
    })?;
    refresh_native(executable, project)?;
    println!(
        "Installed {}. Run `pam doctor` to validate native integration.",
        package.alias
    );
    Ok(0)
}

pub fn remove(executable: &OsStr, project: &Path, alias: &str) -> Result<u8, String> {
    let package = resolve(alias)?;
    let name = format!("pushinbr/{}", package.composer);
    in_project(project, || {
        composer_success(
            executable,
            &["remove", &name, "--dry-run", "--no-interaction"],
            "Composer removal preflight",
        )?;
        composer_success(
            executable,
            &["remove", &name, "--no-interaction"],
            "Composer removal",
        )
    })?;
    refresh_native(executable, project)?;
    println!(
        "Removed {}. Run `pam doctor` to validate the project.",
        package.alias
    );
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

pub(crate) fn refresh_native(executable: &OsStr, project: &Path) -> Result<(), String> {
    if !project.join("pam-native.json").is_file() {
        return Ok(());
    }
    command_success(
        Command::new(executable)
            .args(["mobile", "codegen"])
            .arg(project),
        "PAM Native code generation",
    )?;
    if cfg!(target_os = "macos") {
        command_success(
            Command::new(executable)
                .args(["mobile", "ios:prepare"])
                .arg(project),
            "PAM Native iOS preparation",
        )?;
    }
    Ok(())
}

fn command_success(command: &mut Command, operation: &str) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|error| format!("cannot start {operation}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{operation} failed with {status}"))
    }
}

fn resolve(alias: &str) -> Result<Package, String> {
    PACKAGES
        .iter()
        .copied()
        .find(|package| package.alias == alias || package.composer == alias)
        .ok_or_else(|| format!("unknown PAM capability {alias:?}; run `pam packages`"))
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
    Ok(names)
}

fn composer_success(executable: &OsStr, arguments: &[&str], operation: &str) -> Result<(), String> {
    let arguments = arguments.iter().map(OsString::from).collect::<Vec<_>>();
    let status = composer::run(executable, &arguments)?;
    if status == 0 {
        Ok(())
    } else {
        Err(format!(
            "{operation} failed with status {status}; the project was not advanced"
        ))
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
