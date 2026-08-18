use std::ffi::{OsStr, OsString};
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use crate::composer;
use crate::plugin_registry::{self, VerifiedRelease};
use crate::project::{ProjectContext, ProjectKind};
use crate::terminal::Terminal;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const REGISTRY_CONFIG: &str = "pam-registry.json";
const REGISTRY_STATE: &str = ".pam/plugin-registry-state.json";
const MAX_REGISTRY_CONFIG_BYTES: u64 = 16 * 1024;
const MAX_COMPOSER_LOCK_BYTES: u64 = 8 * 1024 * 1024;
const MAX_PLUGIN_ARTIFACT_BYTES: u64 = 512 * 1024 * 1024;
const MAX_PLUGIN_ARTIFACTS: usize = 256;
const MAX_RELEASES_PER_PLUGIN: usize = 32;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegistryConfig {
    schema_version: u8,
    root_path: PathBuf,
    root_sha256: String,
    catalog_path: PathBuf,
    #[serde(default)]
    native_protocol: Option<u32>,
    #[serde(default)]
    desktop_protocol: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegistryState {
    schema_version: u8,
    registry: String,
    root_sha256: String,
    root_generation: u32,
    catalog_sequence: u64,
}

#[derive(Clone, Copy)]
struct Package {
    alias: &'static str,
    composer: &'static str,
    requirement: &'static str,
    description: &'static str,
}

const PACKAGES: &[Package] = &[
    versioned_package(
        "native",
        "pam-native",
        "^0.6",
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
    versioned_package(
        "media",
        "pam-native-media",
        "^0.2",
        "Native media metadata and thumbnails",
    ),
    versioned_package(
        "mobile-ui",
        "pam-mobile-ui",
        "^0.4",
        "Official PAM design system",
    ),
    package("nfc", "pam-native-nfc", "NFC and NDEF sessions"),
    versioned_package(
        "nitro",
        "pam-native-nitro",
        "^0.3",
        "Offline-first typed data",
    ),
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
    versioned_package(alias, name, "^0.1", description)
}

const fn versioned_package(
    alias: &'static str,
    name: &'static str,
    requirement: &'static str,
    description: &'static str,
) -> Package {
    Package {
        alias,
        composer: name,
        requirement,
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
                    "requirement": package.requirement,
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
            "  {}  {:<22} {:<7} {}",
            ui.status(if status == "installed" { "ok" } else { "info" }, status),
            ui.heading(package.alias),
            ui.muted(package.requirement),
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

pub fn add(executable: &OsStr, context: &ProjectContext, alias: &str) -> Result<u8, String> {
    let package = resolve(alias)?;
    let project = &context.root;
    let name = format!("pushinbr/{}", package.composer);
    let authenticated = resolve_authenticated(context, &name)?;
    let selected_requirement = authenticated
        .as_ref()
        .map_or(package.requirement, |release| release.version.as_str());
    let requirement = format!("{name}:{selected_requirement}");
    let verified_artifact = authenticated
        .as_ref()
        .map(|release| verify_remote_artifact(project, release))
        .transpose()?;
    let composer_home = verified_artifact
        .as_ref()
        .and_then(|path| path.parent())
        .map(|directory| create_authenticated_composer_home(project, directory))
        .transpose()?;
    let install_result = in_project(project, || {
        if authenticated.is_none() {
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
        }
        println!("Validating dependency compatibility without changing the project...");
        composer_success_with_home(
            executable,
            &["require", &requirement, "--dry-run", "--no-interaction"],
            "Composer dependency preflight",
            composer_home.as_deref(),
        )?;
        println!("Installing {requirement}...");
        composer_success_with_home(
            executable,
            &["require", &requirement, "--no-interaction"],
            "Composer install",
            composer_home.as_deref(),
        )?;
        if let Some(release) = &authenticated {
            validate_locked_release(
                project,
                release,
                verified_artifact
                    .as_deref()
                    .expect("authenticated artifact"),
            )?;
            persist_registry_state(project, release)?;
            prune_superseded_artifacts(
                verified_artifact
                    .as_deref()
                    .expect("authenticated artifact"),
                release,
            )?;
        }
        Ok(())
    });
    let cleanup_result = composer_home
        .as_deref()
        .map(fs::remove_dir_all)
        .transpose()
        .map_err(|error| format!("cannot remove authenticated Composer home: {error}"));
    install_result?;
    cleanup_result?;
    refresh_native(executable, project)?;
    println!(
        "Installed {}. Run `pam doctor` to validate native integration.",
        package.alias
    );
    Ok(0)
}

fn resolve_authenticated(
    context: &ProjectContext,
    package: &str,
) -> Result<Option<VerifiedRelease>, String> {
    let path = context.root.join(REGISTRY_CONFIG);
    if !path.exists() {
        return Ok(None);
    }
    let config: RegistryConfig = read_bounded_json(&path, "registry configuration")?;
    if config.schema_version != 1 {
        return Err("unsupported pam-registry.json schema; expected integer 1".to_owned());
    }
    let (surface_code, native_protocol, desktop_protocol) = match context.kind {
        ProjectKind::Native => (2, config.native_protocol, None),
        ProjectKind::Desktop => (3, None, config.desktop_protocol),
        ProjectKind::Api | ProjectKind::Laravel | ProjectKind::Raw => (1, None, None),
    };
    if surface_code == 2 && native_protocol.is_none() {
        return Err("pam-registry.json requires nativeProtocol for a Native project".to_owned());
    }
    if surface_code == 3 && desktop_protocol.is_none() {
        return Err("pam-registry.json requires desktopProtocol for a Desktop project".to_owned());
    }
    let state = read_registry_state(&context.root)?;
    let root = confined_project_path(&context.root, &config.root_path, "rootPath")?;
    let catalog = confined_project_path(&context.root, &config.catalog_path, "catalogPath")?;
    let pam_version = Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|error| format!("invalid PAM package version: {error}"))?;
    let release = plugin_registry::resolve_verified(
        &root,
        &config.root_sha256,
        &catalog,
        package,
        surface_code,
        &pam_version,
        native_protocol,
        desktop_protocol,
        state.as_ref().map(|value| value.catalog_sequence),
        None,
    )?;
    if let Some(state) = state {
        if state.registry != release.registry {
            return Err(
                "signed registry identity does not match the accepted project state".to_owned(),
            );
        }
        if state.root_sha256 != release.root_sha256 {
            return Err(
                "trusted registry root changed without an authenticated rotation".to_owned(),
            );
        }
        if release.root_generation < state.root_generation {
            return Err("signed registry root generation would roll the project back".to_owned());
        }
    }
    if release.artifact_kind_code != 1 {
        return Err("pam add requires a signed Composer artifactKindCode of 1".to_owned());
    }
    Ok(Some(release))
}

fn confined_project_path(project: &Path, relative: &Path, field: &str) -> Result<PathBuf, String> {
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!(
            "pam-registry.json {field} must be a normalized project-relative path"
        ));
    }
    Ok(project.join(relative))
}

fn read_bounded_json<T: for<'de> Deserialize<'de>>(path: &Path, label: &str) -> Result<T, String> {
    read_json_with_limit(path, label, MAX_REGISTRY_CONFIG_BYTES)
}

fn read_json_with_limit<T: for<'de> Deserialize<'de>>(
    path: &Path,
    label: &str,
    maximum_bytes: u64,
) -> Result<T, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
    if !metadata.file_type().is_file() || metadata.len() > maximum_bytes {
        return Err(format!(
            "{label} must be a regular file no larger than {maximum_bytes} bytes"
        ));
    }
    let bytes =
        fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("invalid {}: {error}", path.display()))
}

fn read_registry_state(project: &Path) -> Result<Option<RegistryState>, String> {
    let path = project.join(REGISTRY_STATE);
    if !path.exists() {
        return Ok(None);
    }
    let state: RegistryState = read_bounded_json(&path, "registry state")?;
    if state.schema_version != 1 || state.registry.is_empty() || state.catalog_sequence == 0 {
        return Err("invalid project plugin-registry state".to_owned());
    }
    Ok(Some(state))
}

fn persist_registry_state(project: &Path, release: &VerifiedRelease) -> Result<(), String> {
    let path = project.join(REGISTRY_STATE);
    let parent = path.parent().expect("state path has parent");
    if parent.exists() {
        let metadata = fs::symlink_metadata(parent)
            .map_err(|error| format!("cannot inspect {}: {error}", parent.display()))?;
        if !metadata.file_type().is_dir() {
            return Err(format!(
                "registry state parent is not a real directory: {}",
                parent.display()
            ));
        }
    } else {
        fs::create_dir(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let temporary = parent.join(format!(".plugin-registry-state-{}.tmp", std::process::id()));
    let bytes = serde_json::to_vec_pretty(&RegistryState {
        schema_version: 1,
        registry: release.registry.clone(),
        root_sha256: release.root_sha256.clone(),
        root_generation: release.root_generation,
        catalog_sequence: release.catalog_sequence,
    })
    .map_err(|error| format!("cannot encode registry state: {error}"))?;
    fs::write(&temporary, bytes)
        .map_err(|error| format!("cannot write {}: {error}", temporary.display()))?;
    fs::rename(&temporary, &path)
        .map_err(|error| format!("cannot replace {}: {error}", path.display()))
}

fn validate_locked_release(
    project: &Path,
    release: &VerifiedRelease,
    artifact: &Path,
) -> Result<(), String> {
    let path = project.join("composer.lock");
    let lock: serde_json::Value =
        read_json_with_limit(&path, "Composer lock", MAX_COMPOSER_LOCK_BYTES)?;
    let locked = ["packages", "packages-dev"]
        .into_iter()
        .filter_map(|section| lock.get(section).and_then(serde_json::Value::as_array))
        .flatten()
        .find(|item| item.get("name").and_then(serde_json::Value::as_str) == Some(&release.package))
        .ok_or_else(|| {
            format!(
                "composer.lock does not contain signed package {}",
                release.package
            )
        })?;
    let version = locked.get("version").and_then(serde_json::Value::as_str);
    let dist = locked.get("dist").and_then(serde_json::Value::as_object);
    let url = dist
        .and_then(|value| value.get("url"))
        .and_then(serde_json::Value::as_str);
    if version != Some(release.version.as_str())
        || !url.is_some_and(|value| locked_artifact_matches(value, artifact))
    {
        return Err(format!(
            "composer.lock does not match the verified local artifact for {} {}; expected exact version and artifact path",
            release.package, release.version
        ));
    }
    Ok(())
}

fn locked_artifact_matches(url: &str, artifact: &Path) -> bool {
    let candidate = url.strip_prefix("file://").unwrap_or(url);
    Path::new(candidate).is_absolute()
        && fs::canonicalize(candidate).ok().as_deref() == fs::canonicalize(artifact).ok().as_deref()
}

fn verify_remote_artifact(project: &Path, release: &VerifiedRelease) -> Result<PathBuf, String> {
    let store = plugin_artifact_directory(project)?;
    let package_directory = store.join(release.package.replace('/', "--"));
    ensure_real_directory(&package_directory)?;
    let directory = package_directory.join(&release.sha256);
    enforce_directory_limit(
        &package_directory,
        if directory.exists() {
            MAX_RELEASES_PER_PLUGIN
        } else {
            MAX_RELEASES_PER_PLUGIN - 1
        },
        "plugin release",
    )?;
    ensure_real_directory(&directory)?;
    let artifact = directory.join("package.zip");
    if artifact.exists() {
        verify_artifact_file(&artifact, &release.sha256)?;
        return Ok(artifact);
    }
    let temporary = directory.join(format!(".package-{}.tmp", std::process::id()));
    let temporary_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("cannot allocate plugin artifact: {error}"))?;
    drop(temporary_file);
    let result = (|| {
        let status = Command::new("curl")
            .args([
                "--proto",
                "=https",
                "--tlsv1.2",
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--max-filesize",
                &MAX_PLUGIN_ARTIFACT_BYTES.to_string(),
                "--output",
            ])
            .arg(&temporary)
            .arg(&release.artifact_url)
            .status()
            .map_err(|error| format!("cannot download signed plugin artifact: {error}"))?;
        if !status.success() {
            return Err(format!(
                "signed plugin artifact download failed with {status}"
            ));
        }
        verify_artifact_file(&temporary, &release.sha256)?;
        fs::rename(&temporary, &artifact)
            .map_err(|error| format!("cannot publish verified plugin artifact: {error}"))?;
        Ok(artifact.clone())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn plugin_artifact_directory(project: &Path) -> Result<PathBuf, String> {
    let state = project.join(".pam");
    ensure_real_directory(&state)?;
    let artifacts = state.join("plugin-artifacts");
    ensure_real_directory(&artifacts)?;
    enforce_directory_limit(&artifacts, MAX_PLUGIN_ARTIFACTS, "plugin artifact")?;
    Ok(artifacts)
}

fn enforce_directory_limit(path: &Path, maximum: usize, label: &str) -> Result<(), String> {
    let count = fs::read_dir(path)
        .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?
        .take(maximum + 1)
        .count();
    if count > maximum {
        return Err(format!(
            "{label} store exceeds {maximum} entries; inspect {}",
            path.display()
        ));
    }
    Ok(())
}

fn create_authenticated_composer_home(
    project: &Path,
    artifact_directory: &Path,
) -> Result<PathBuf, String> {
    let state = project.join(".pam");
    ensure_real_directory(&state)?;
    let mut home = None;
    for attempt in 0..32_u8 {
        let candidate = state.join(format!(
            "composer-authenticated-{}-{attempt}",
            std::process::id()
        ));
        match fs::create_dir(&candidate) {
            Ok(()) => {
                home = Some(candidate);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!("cannot create {}: {error}", candidate.display()));
            }
        }
    }
    let home = home.ok_or_else(|| "cannot allocate authenticated Composer home".to_owned())?;
    let config = serde_json::to_vec_pretty(&serde_json::json!({
        "config": {
            "secure-http": true,
            "cache-dir": home.join("cache")
        },
        "repositories": {
            "pam-signed-artifacts": {
                "type": "artifact",
                "url": artifact_directory,
                "canonical": true
            }
        }
    }))
    .map_err(|error| format!("cannot encode authenticated Composer config: {error}"))?;
    if let Err(error) = fs::write(home.join("config.json"), config) {
        let _ = fs::remove_dir_all(&home);
        return Err(format!(
            "cannot write authenticated Composer config: {error}"
        ));
    }
    Ok(home)
}

fn ensure_real_directory(path: &Path) -> Result<(), String> {
    if path.exists() {
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
        if metadata.file_type().is_dir() {
            return Ok(());
        }
        return Err(format!("{} is not a real directory", path.display()));
    }
    fs::create_dir(path).map_err(|error| format!("cannot create {}: {error}", path.display()))
}

fn prune_superseded_artifacts(artifact: &Path, release: &VerifiedRelease) -> Result<(), String> {
    let current_release = artifact.parent().expect("artifact has release directory");
    let package_directory = current_release
        .parent()
        .expect("artifact has package directory");
    for entry in fs::read_dir(package_directory)
        .map_err(|error| format!("cannot inspect {}: {error}", package_directory.display()))?
    {
        let entry = entry.map_err(|error| format!("cannot inspect plugin artifact: {error}"))?;
        let path = entry.path();
        if path == current_release {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
        if name.len() != 64
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || !file_type.is_dir()
        {
            return Err(format!(
                "refusing unexpected entry in artifact store for {}: {}",
                release.package,
                path.display()
            ));
        }
        fs::remove_dir_all(&path)
            .map_err(|error| format!("cannot prune {}: {error}", path.display()))?;
    }
    Ok(())
}

fn verify_artifact_file(path: &Path, expected_sha256: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect plugin artifact: {error}"))?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_PLUGIN_ARTIFACT_BYTES {
        return Err("plugin artifact is not a bounded regular file".to_owned());
    }
    if metadata.len() < 4 {
        return Err("plugin artifact is not a ZIP archive".to_owned());
    }
    let mut input =
        fs::File::open(path).map_err(|error| format!("cannot read plugin artifact: {error}"))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut header = [0_u8; 4];
    input
        .read_exact(&mut header)
        .map_err(|error| format!("cannot read plugin artifact header: {error}"))?;
    if !matches!(&header, b"PK\x03\x04" | b"PK\x05\x06" | b"PK\x07\x08") {
        return Err("plugin artifact is not a ZIP archive".to_owned());
    }
    digest.update(header);
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|error| format!("cannot hash plugin artifact: {error}"))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    let actual = format!("{:x}", digest.finalize());
    if actual == expected_sha256 {
        Ok(())
    } else {
        Err(format!(
            "signed plugin artifact SHA-256 mismatch: expected {expected_sha256}, received {actual}"
        ))
    }
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
    composer_success_with_home(executable, arguments, operation, None)
}

fn composer_success_with_home(
    executable: &OsStr,
    arguments: &[&str],
    operation: &str,
    composer_home: Option<&Path>,
) -> Result<(), String> {
    let arguments = arguments.iter().map(OsString::from).collect::<Vec<_>>();
    let status = composer::run_with_home(executable, &arguments, composer_home)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_project(label: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("pam-ecosystem-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn signed_release() -> VerifiedRelease {
        VerifiedRelease {
            registry: "pam-official".to_owned(),
            root_sha256: "ab".repeat(32),
            root_generation: 2,
            catalog_sequence: 9,
            package: "pushinbr/pam-native-maps".to_owned(),
            version: "1.2.3".to_owned(),
            artifact_kind_code: 1,
            artifact_url: "https://plugins.pam.dev/maps-1.2.3.zip".to_owned(),
            sha256: "cd".repeat(32),
        }
    }

    #[test]
    fn official_capabilities_keep_their_independent_release_lines() {
        assert_eq!(resolve("native").unwrap().requirement, "^0.6");
        assert_eq!(resolve("media").unwrap().requirement, "^0.2");
        assert_eq!(resolve("mobile-ui").unwrap().requirement, "^0.4");
        assert_eq!(resolve("nitro").unwrap().requirement, "^0.3");
        assert_eq!(resolve("maps").unwrap().requirement, "^0.1");
    }

    #[test]
    fn authenticated_lock_requires_exact_version_and_url() {
        let project = temporary_project("lock");
        let release = signed_release();
        let artifact = project.join("verified.zip");
        fs::write(&artifact, b"PK\x03\x04verified").unwrap();
        fs::write(
            project.join("composer.lock"),
            serde_json::to_vec(&serde_json::json!({
                "packages": [{
                    "name": release.package,
                    "version": release.version,
                    "dist": {"url": artifact, "shasum": ""}
                }],
                "packages-dev": []
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(validate_locked_release(&project, &release, &artifact).is_ok());
        let mut lock: serde_json::Value =
            serde_json::from_slice(&fs::read(project.join("composer.lock")).unwrap()).unwrap();
        lock["packages"][0]["dist"]["url"] = serde_json::json!("https://attacker.invalid/map.zip");
        fs::write(
            project.join("composer.lock"),
            serde_json::to_vec(&lock).unwrap(),
        )
        .unwrap();
        assert!(
            validate_locked_release(&project, &release, &artifact)
                .unwrap_err()
                .contains("does not match the verified local artifact")
        );
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn downloaded_artifact_requires_the_signed_sha256() {
        let project = temporary_project("artifact");
        let artifact = project.join("plugin.zip");
        fs::write(&artifact, b"PK\x03\x04signed artifact").unwrap();
        let expected = format!("{:x}", Sha256::digest(b"PK\x03\x04signed artifact"));
        assert!(verify_artifact_file(&artifact, &expected).is_ok());
        assert!(
            verify_artifact_file(&artifact, &"00".repeat(32))
                .unwrap_err()
                .contains("SHA-256 mismatch")
        );
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn artifact_store_prunes_only_superseded_versions_of_the_same_package() {
        let project = temporary_project("prune");
        let release = signed_release();
        let store = plugin_artifact_directory(&project).unwrap();
        let package = store.join("pushinbr--pam-native-maps");
        let current_directory = package.join(&release.sha256);
        let old_directory = package.join("ef".repeat(32));
        let other_directory = store
            .join("pushinbr--pam-native-auth")
            .join("12".repeat(32));
        fs::create_dir_all(&current_directory).unwrap();
        fs::create_dir_all(&old_directory).unwrap();
        fs::create_dir_all(&other_directory).unwrap();
        let current = current_directory.join("package.zip");
        let old = old_directory.join("package.zip");
        let other = other_directory.join("package.zip");
        fs::write(&current, b"current").unwrap();
        fs::write(&old, b"old").unwrap();
        fs::write(&other, b"other").unwrap();
        prune_superseded_artifacts(&current, &release).unwrap();
        assert!(current.is_file());
        assert!(!old_directory.exists());
        assert!(other.is_file());
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn registry_state_persists_the_rollback_floor_and_root_identity() {
        let project = temporary_project("state");
        let release = signed_release();
        persist_registry_state(&project, &release).unwrap();
        let state = read_registry_state(&project).unwrap().unwrap();
        assert_eq!(state.registry, release.registry);
        assert_eq!(state.root_sha256, release.root_sha256);
        assert_eq!(state.root_generation, 2);
        assert_eq!(state.catalog_sequence, 9);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn registry_documents_must_stay_beneath_the_project() {
        let project = temporary_project("paths");
        assert!(
            confined_project_path(&project, Path::new("registry/root.json"), "rootPath").is_ok()
        );
        assert!(confined_project_path(&project, Path::new("../root.json"), "rootPath").is_err());
        assert!(confined_project_path(&project, Path::new("/root.json"), "rootPath").is_err());
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn authenticated_composer_home_exposes_the_verified_artifact_repository() {
        let project = temporary_project("composer-home");
        let artifacts = plugin_artifact_directory(&project).unwrap();
        let home = create_authenticated_composer_home(&project, &artifacts).unwrap();
        let config: serde_json::Value =
            serde_json::from_slice(&fs::read(home.join("config.json")).unwrap()).unwrap();
        assert_eq!(
            config["repositories"]["pam-signed-artifacts"]["type"],
            "artifact"
        );
        assert_eq!(
            config["repositories"]["pam-signed-artifacts"]["url"],
            serde_json::json!(artifacts)
        );
        assert_eq!(
            config["repositories"]["pam-signed-artifacts"]["canonical"],
            true
        );
        assert_eq!(config["config"]["secure-http"], true);
        fs::remove_dir_all(project).unwrap();
    }
}
