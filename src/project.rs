use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::terminal::Terminal;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum ProjectKind {
    Api = 1,
    Native = 2,
    Laravel = 3,
    Desktop = 4,
    Raw = 5,
    Product = 6,
}

impl ProjectKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Api => "API",
            Self::Native => "PAM Native",
            Self::Laravel => "Laravel",
            Self::Desktop => "PAM Desktop",
            Self::Raw => "PAM Runtime",
            Self::Product => "PAM Product",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectContext {
    pub root: PathBuf,
    pub kind: ProjectKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisteredCommand {
    pub name: String,
    pub description: String,
    pub target: CommandTarget,
    pub arguments: Vec<String>,
    pub environment: std::collections::BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandTarget {
    PhpScript(PathBuf),
    Executable(PathBuf),
}

pub fn discover_cleanable(path: &Path) -> Option<ProjectContext> {
    discover(path).or_else(|| {
        let root = fs::canonicalize(path).ok()?;
        (root.is_dir() && root.join("Cargo.toml").is_file()).then_some(ProjectContext {
            root,
            kind: ProjectKind::Raw,
        })
    })
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct DevelopmentArtifacts {
    root: PathBuf,
    exists: bool,
    bytes: u64,
    files: u64,
    complete: bool,
}

const MAX_ARTIFACT_ENTRIES: u64 = 100_000;
pub const DEFAULT_DEV_ARTIFACT_BUDGET_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MIN_DEV_ARTIFACT_BUDGET_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Copy)]
#[repr(u8)]
enum CleanupArtifactKind {
    Cache = 1,
    Build = 2,
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum ArtifactBudgetState {
    WithinBudget = 1,
    Exceeded = 2,
    IncompleteScan = 3,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CleanupEntry {
    path: PathBuf,
    kind_code: u8,
    existed: bool,
    bytes: u64,
    files: u64,
    complete: bool,
    removed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CleanupReport<'a> {
    schema_version: u8,
    result_code: u8,
    operation_code: u8,
    project_type_code: u8,
    root: &'a Path,
    bytes: u64,
    files: u64,
    entries: &'a [CleanupEntry],
}

pub fn clean(context: &ProjectContext, all: bool, dry_run: bool, json: bool) -> Result<u8, String> {
    clean_with_output(context, all, dry_run, json, true)
}

pub fn clean_after_dev(context: &ProjectContext) -> Result<u8, String> {
    clean_with_output(context, true, false, false, false)
}

fn clean_with_output(
    context: &ProjectContext,
    all: bool,
    dry_run: bool,
    json: bool,
    emit_report: bool,
) -> Result<u8, String> {
    let root = fs::canonicalize(&context.root).map_err(|error| {
        format!(
            "cannot resolve project root {}: {error}",
            context.root.display()
        )
    })?;
    let targets = cleanup_targets(context.kind, all);
    let mut entries = Vec::with_capacity(targets.len());
    for (relative, kind) in targets {
        let path = root.join(relative);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                entries.push(CleanupEntry {
                    path: relative.to_owned(),
                    kind_code: kind as u8,
                    existed: false,
                    bytes: 0,
                    files: 0,
                    complete: true,
                    removed: false,
                });
                continue;
            }
            Err(error) => return Err(format!("cannot inspect {}: {error}", path.display())),
        };
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "refusing to clean generated artifact symlink {}",
                path.display()
            ));
        }
        if !metadata.is_dir() {
            return Err(format!(
                "refusing to clean non-directory artifact {}",
                path.display()
            ));
        }
        if !artifact_path_is_direct(&root, &path)? {
            return Err(format!(
                "refusing to clean artifact path that resolves through a symlink or outside the project: {}",
                path.display()
            ));
        }
        let mut bytes = 0;
        let mut files = 0;
        let complete = measure_directory(&path, &mut bytes, &mut files)?;
        if !complete && !dry_run {
            return Err(format!(
                "refusing to clean incompletely scanned artifact {}",
                path.display()
            ));
        }
        entries.push(CleanupEntry {
            path: relative.to_owned(),
            kind_code: kind as u8,
            existed: true,
            bytes,
            files,
            complete,
            removed: false,
        });
    }
    if !dry_run {
        for entry in entries.iter_mut().filter(|entry| entry.existed) {
            let path = root.join(&entry.path);
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| format!("cannot reinspect {}: {error}", path.display()))?;
            if metadata.file_type().is_symlink()
                || !metadata.is_dir()
                || !artifact_path_is_direct(&root, &path)?
            {
                return Err(format!(
                    "refusing to clean artifact that changed after validation: {}",
                    path.display()
                ));
            }
            fs::remove_dir_all(&path)
                .map_err(|error| format!("cannot clean {}: {error}", path.display()))?;
            entry.removed = true;
        }
    }
    let bytes = entries.iter().map(|entry| entry.bytes).sum();
    let files = entries.iter().map(|entry| entry.files).sum();
    let report = CleanupReport {
        schema_version: 1,
        result_code: 1,
        operation_code: if dry_run {
            1
        } else if all {
            3
        } else {
            2
        },
        project_type_code: context.kind as u8,
        root: &root,
        bytes,
        files,
        entries: &entries,
    };
    if !emit_report {
        return Ok(0);
    }
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .map_err(|error| format!("cannot encode cleanup report: {error}"))?
        );
    } else {
        let ui = Terminal::stdout();
        println!("{}", ui.brand("PAM / CLEAN"));
        println!("{}\n", ui.rule());
        println!("  {}  {}", ui.heading("Project"), root.display());
        println!(
            "  {}  {} across {} files",
            ui.heading(if dry_run { "Reclaimable" } else { "Removed" }),
            human_bytes(bytes),
            files
        );
        for entry in entries.iter().filter(|entry| entry.existed) {
            println!(
                "  {}  {} ({})",
                if entry.removed { "✓" } else { "·" },
                entry.path.display(),
                human_bytes(entry.bytes)
            );
        }
        if dry_run {
            println!(
                "\n  Preview only. Run `pam clean{}` to remove these artifacts.",
                if all { " --all" } else { "" }
            );
        }
    }
    Ok(0)
}

pub fn dev_artifact_budget() -> Result<u64, String> {
    let Some(value) = std::env::var_os("PAM_DEV_ARTIFACT_BUDGET_BYTES") else {
        return Ok(DEFAULT_DEV_ARTIFACT_BUDGET_BYTES);
    };
    let value = value
        .to_str()
        .ok_or_else(|| "PAM_DEV_ARTIFACT_BUDGET_BYTES must be valid UTF-8".to_owned())?
        .parse::<u64>()
        .map_err(|_| "PAM_DEV_ARTIFACT_BUDGET_BYTES must be an integer byte count".to_owned())?;
    if !(MIN_DEV_ARTIFACT_BUDGET_BYTES..=DEFAULT_DEV_ARTIFACT_BUDGET_BYTES).contains(&value) {
        return Err(format!(
            "PAM_DEV_ARTIFACT_BUDGET_BYTES must be between {MIN_DEV_ARTIFACT_BUDGET_BYTES} and {DEFAULT_DEV_ARTIFACT_BUDGET_BYTES}"
        ));
    }
    Ok(value)
}

pub fn enforce_dev_artifact_budget(
    context: &ProjectContext,
    budget_bytes: u64,
) -> Result<Option<u64>, String> {
    if !(MIN_DEV_ARTIFACT_BUDGET_BYTES..=DEFAULT_DEV_ARTIFACT_BUDGET_BYTES).contains(&budget_bytes)
    {
        return Err("development artifact budget is outside the supported bounds".to_owned());
    }
    let footprint = artifact_footprint(context)?;
    if !footprint["complete"].as_bool().unwrap_or(false) {
        return Err(format!(
            "cannot enforce the development artifact budget because {} could not be scanned completely",
            context.root.display()
        ));
    }
    let bytes = footprint["bytes"].as_u64().unwrap_or(0);
    if bytes <= budget_bytes {
        return Ok(None);
    }
    eprintln!(
        "PAM development artifacts reached {} (budget {}). Cleaning regenerable project outputs before starting dev.",
        human_bytes(bytes),
        human_bytes(budget_bytes)
    );
    clean(context, true, false, false)?;
    Ok(Some(bytes))
}

fn cleanup_targets(kind: ProjectKind, all: bool) -> Vec<(&'static Path, CleanupArtifactKind)> {
    if kind == ProjectKind::Product {
        if all {
            let mut targets = vec![
                (
                    Path::new("apps/server/.pam/cache"),
                    CleanupArtifactKind::Cache,
                ),
                (
                    Path::new("apps/server/.pam/phpunit-cache"),
                    CleanupArtifactKind::Cache,
                ),
                (
                    Path::new("apps/native/.pam/cache"),
                    CleanupArtifactKind::Cache,
                ),
                (
                    Path::new("apps/native/.pam/phpunit-cache"),
                    CleanupArtifactKind::Cache,
                ),
                (
                    Path::new("apps/native/.pam-native/android"),
                    CleanupArtifactKind::Build,
                ),
                (
                    Path::new("apps/native/.pam-native/ios"),
                    CleanupArtifactKind::Build,
                ),
                (
                    Path::new("apps/desktop/.pam/cache"),
                    CleanupArtifactKind::Cache,
                ),
                (Path::new("apps/desktop/target"), CleanupArtifactKind::Build),
            ];
            targets.extend(workspace_tooling_targets());
            return targets;
        }
        let mut targets = vec![
            (
                Path::new("apps/server/.pam/cache"),
                CleanupArtifactKind::Cache,
            ),
            (
                Path::new("apps/server/.pam/phpunit-cache"),
                CleanupArtifactKind::Cache,
            ),
            (
                Path::new("apps/native/.pam/cache"),
                CleanupArtifactKind::Cache,
            ),
            (
                Path::new("apps/native/.pam/phpunit-cache"),
                CleanupArtifactKind::Cache,
            ),
            (
                Path::new("apps/native/.pam-native/android/app/build"),
                CleanupArtifactKind::Build,
            ),
            (
                Path::new("apps/native/.pam-native/android/build"),
                CleanupArtifactKind::Build,
            ),
            (
                Path::new("apps/native/.pam-native/android/gradle-home/caches"),
                CleanupArtifactKind::Cache,
            ),
            (
                Path::new("apps/native/.pam-native/ios/App/DerivedData"),
                CleanupArtifactKind::Build,
            ),
            (
                Path::new("apps/desktop/target/debug/incremental"),
                CleanupArtifactKind::Cache,
            ),
            (
                Path::new("apps/desktop/target/release/incremental"),
                CleanupArtifactKind::Cache,
            ),
            (
                Path::new("apps/desktop/.pam/cache"),
                CleanupArtifactKind::Cache,
            ),
        ];
        targets.extend(workspace_tooling_targets());
        return targets;
    }
    if all {
        let mut targets = vec![
            (Path::new(".pam/cache"), CleanupArtifactKind::Cache),
            (Path::new(".pam/phpunit-cache"), CleanupArtifactKind::Cache),
            (Path::new(".pam-native/android"), CleanupArtifactKind::Build),
            (Path::new(".pam-native/ios"), CleanupArtifactKind::Build),
            (Path::new("target"), CleanupArtifactKind::Build),
        ];
        targets.extend(workspace_tooling_targets());
        return targets;
    }
    let mut targets = vec![
        (Path::new(".pam/cache"), CleanupArtifactKind::Cache),
        (Path::new(".pam/phpunit-cache"), CleanupArtifactKind::Cache),
        (
            Path::new(".pam-native/android/app/build"),
            CleanupArtifactKind::Build,
        ),
        (
            Path::new(".pam-native/android/build"),
            CleanupArtifactKind::Build,
        ),
        (
            Path::new(".pam-native/android/gradle-home/caches"),
            CleanupArtifactKind::Cache,
        ),
        (
            Path::new(".pam-native/android/gradle-home/daemon"),
            CleanupArtifactKind::Cache,
        ),
        (
            Path::new(".pam-native/android/gradle-home/native"),
            CleanupArtifactKind::Cache,
        ),
        (
            Path::new(".pam-native/android/gradle-home/notifications"),
            CleanupArtifactKind::Cache,
        ),
        (
            Path::new(".pam-native/ios/App/DerivedData"),
            CleanupArtifactKind::Build,
        ),
        (
            Path::new("target/debug/incremental"),
            CleanupArtifactKind::Cache,
        ),
        (
            Path::new("target/release/incremental"),
            CleanupArtifactKind::Cache,
        ),
    ];
    targets.extend(workspace_tooling_targets());
    targets
}

fn workspace_tooling_targets() -> Vec<(&'static Path, CleanupArtifactKind)> {
    vec![
        (Path::new("android/.gradle"), CleanupArtifactKind::Cache),
        (Path::new("android/.kotlin"), CleanupArtifactKind::Cache),
        (Path::new("android/build"), CleanupArtifactKind::Build),
        (Path::new("android/app/build"), CleanupArtifactKind::Build),
        (
            Path::new("android/plugin-api/build"),
            CleanupArtifactKind::Build,
        ),
        (
            Path::new("android/macrobenchmark/build"),
            CleanupArtifactKind::Build,
        ),
        (Path::new(".build"), CleanupArtifactKind::Build),
        (Path::new("ios/.build"), CleanupArtifactKind::Build),
        (Path::new("scripts/__pycache__"), CleanupArtifactKind::Cache),
        (Path::new("tests/__pycache__"), CleanupArtifactKind::Cache),
        (
            Path::new("benchmarks/__pycache__"),
            CleanupArtifactKind::Cache,
        ),
        (
            Path::new("benchmarks/package/__pycache__"),
            CleanupArtifactKind::Cache,
        ),
        (Path::new(".pytest_cache"), CleanupArtifactKind::Cache),
        (Path::new(".mypy_cache"), CleanupArtifactKind::Cache),
        (Path::new(".ruff_cache"), CleanupArtifactKind::Cache),
    ]
}

pub fn info(context: &ProjectContext, json: bool) -> Result<u8, String> {
    let composer = context.root.join("composer.json");
    let manifest = context.root.join("pam.json");
    let legacy_native = context.root.join("pam-native.json");
    let project_manifest = if manifest.is_file() {
        let source = fs::read(&manifest)
            .map_err(|error| format!("cannot read {}: {error}", manifest.display()))?;
        Some(
            serde_json::from_slice::<serde_json::Value>(&source)
                .map_err(|error| format!("invalid {}: {error}", manifest.display()))?,
        )
    } else {
        None
    };
    let development = development_artifacts(context)?;
    let artifact_footprint = artifact_footprint(context)?;
    let artifact_budget = artifact_budget_report(&artifact_footprint)?;
    let next_commands = contextual_next_commands(context.kind);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "root": context.root,
                "type": context.kind as u8,
                "typeLabel": context.kind.label(),
                "manifest": manifest.is_file(),
                "composer": composer.is_file(),
                "nativeManifest": legacy_native.is_file(),
                "pamVersion": env!("CARGO_PKG_VERSION"),
                "name": project_manifest.as_ref().and_then(|manifest| manifest.get("name")),
                "version": project_manifest.as_ref().and_then(|manifest| manifest.get("version")),
                "native": project_manifest.as_ref().and_then(|manifest| manifest.get("native")),
                "developmentArtifacts": development,
                "artifactFootprint": artifact_footprint,
                "artifactBudget": artifact_budget,
                "nextCommands": next_commands,
            }))
            .map_err(|error| error.to_string())?
        );
        return Ok(0);
    }
    let ui = Terminal::stdout();
    println!("{}", ui.brand("PAM / PROJECT"));
    println!("{}", ui.rule());
    println!();
    println!("  {}  {}", ui.heading("Type"), context.kind.label());
    println!("  {}  {}", ui.heading("Root"), context.root.display());
    println!(
        "  {}  {}",
        ui.heading("Manifest"),
        if manifest.is_file() {
            "pam.json"
        } else {
            "legacy discovery"
        }
    );
    println!("  {}  {}", ui.heading("PAM"), env!("CARGO_PKG_VERSION"));
    if let Some(name) = project_manifest
        .as_ref()
        .and_then(|manifest| manifest.get("name"))
        .and_then(serde_json::Value::as_str)
    {
        println!("  {}  {}", ui.heading("Name"), name);
    }
    if let Some(version) = project_manifest
        .as_ref()
        .and_then(|manifest| manifest.get("version"))
        .and_then(serde_json::Value::as_str)
    {
        println!("  {}  {}", ui.heading("Version"), version);
    }
    println!();
    println!("{}", ui.heading("DEVELOPMENT"));
    println!(
        "  {}  {} across {} files{}",
        ui.heading("Artifacts"),
        human_bytes(artifact_footprint["bytes"].as_u64().unwrap_or(0)),
        artifact_footprint["files"].as_u64().unwrap_or(0),
        if artifact_footprint["complete"].as_bool().unwrap_or(false) {
            ""
        } else {
            " (partial scan)"
        }
    );
    println!(
        "  {}  {} · state code {}",
        ui.heading("Dev budget"),
        human_bytes(artifact_budget["limitBytes"].as_u64().unwrap_or(0)),
        artifact_budget["stateCode"].as_u64().unwrap_or(0)
    );
    println!("  {}  {}", ui.heading("Location"), context.root.display());
    println!();
    println!("{}", ui.heading("NEXT"));
    for command in next_commands {
        println!("  {}", ui.command(command));
    }
    Ok(0)
}

pub fn diagnostic_context(context: &ProjectContext) -> Result<serde_json::Value, String> {
    let artifacts = artifact_footprint(context)?;
    Ok(serde_json::json!({
        "root": context.root,
        "typeCode": context.kind as u8,
        "typeLabel": context.kind.label(),
        "paths": {
            "manifest": context.root.join("pam.json"),
            "composerManifest": context.root.join("composer.json"),
            "nativeManifest": context.root.join("pam-native.json"),
        },
        "developmentArtifacts": artifacts,
        "nextCommands": contextual_next_commands(context.kind),
    }))
}

fn artifact_footprint(context: &ProjectContext) -> Result<serde_json::Value, String> {
    let root = fs::canonicalize(&context.root).map_err(|error| {
        format!(
            "cannot resolve project root {}: {error}",
            context.root.display()
        )
    })?;
    let mut entries = Vec::new();
    let mut total_bytes = 0_u64;
    let mut total_files = 0_u64;
    let mut complete = true;
    for (relative, kind) in cleanup_targets(context.kind, true) {
        let path = root.join(relative);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => {
                complete = false;
                entries.push(serde_json::json!({
                    "path": relative,
                    "kindCode": kind as u8,
                    "exists": true,
                    "bytes": 0,
                    "files": 0,
                    "complete": false,
                }));
                continue;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                entries.push(serde_json::json!({
                    "path": relative,
                    "kindCode": kind as u8,
                    "exists": false,
                    "bytes": 0,
                    "files": 0,
                    "complete": true,
                }));
                continue;
            }
            Err(error) => return Err(format!("cannot inspect {}: {error}", path.display())),
        }
        if !artifact_path_is_direct(&root, &path)? {
            complete = false;
            entries.push(serde_json::json!({
                "path": relative,
                "kindCode": kind as u8,
                "exists": true,
                "bytes": 0,
                "files": 0,
                "complete": false,
            }));
            continue;
        }
        let mut bytes = 0;
        let mut files = 0;
        let entry_complete = measure_directory(&path, &mut bytes, &mut files)?;
        total_bytes = total_bytes.saturating_add(bytes);
        total_files = total_files.saturating_add(files);
        complete &= entry_complete;
        entries.push(serde_json::json!({
            "path": relative,
            "kindCode": kind as u8,
            "exists": true,
            "bytes": bytes,
            "files": files,
            "complete": entry_complete,
        }));
    }
    Ok(serde_json::json!({
        "bytes": total_bytes,
        "files": total_files,
        "complete": complete,
        "entries": entries,
    }))
}

fn artifact_path_is_direct(root: &Path, path: &Path) -> Result<bool, String> {
    let resolved = fs::canonicalize(path)
        .map_err(|error| format!("cannot resolve artifact path {}: {error}", path.display()))?;
    Ok(resolved.starts_with(root) && resolved == path)
}

fn artifact_budget_report(footprint: &serde_json::Value) -> Result<serde_json::Value, String> {
    let limit_bytes = dev_artifact_budget()?;
    let bytes = footprint["bytes"].as_u64().unwrap_or(0);
    let state = if !footprint["complete"].as_bool().unwrap_or(false) {
        ArtifactBudgetState::IncompleteScan
    } else if bytes > limit_bytes {
        ArtifactBudgetState::Exceeded
    } else {
        ArtifactBudgetState::WithinBudget
    };
    Ok(serde_json::json!({
        "limitBytes": limit_bytes,
        "stateCode": state as u8,
        "cleanupCommand": "pam clean --all",
    }))
}

fn development_artifacts(context: &ProjectContext) -> Result<DevelopmentArtifacts, String> {
    let root = context.root.join(".pam-native");
    if !root.is_dir() {
        return Ok(DevelopmentArtifacts {
            root,
            exists: false,
            bytes: 0,
            files: 0,
            complete: true,
        });
    }
    let mut bytes = 0_u64;
    let mut files = 0_u64;
    let complete = measure_directory(&root, &mut bytes, &mut files)?;
    Ok(DevelopmentArtifacts {
        root,
        exists: true,
        bytes,
        files,
        complete,
    })
}

fn measure_directory(root: &Path, bytes: &mut u64, files: &mut u64) -> Result<bool, String> {
    for entry in
        fs::read_dir(root).map_err(|error| format!("cannot inspect {}: {error}", root.display()))?
    {
        if *files >= MAX_ARTIFACT_ENTRIES {
            return Ok(false);
        }
        let entry = entry.map_err(|error| error.to_string())?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("cannot inspect {}: {error}", entry.path().display()))?;
        if file_type.is_dir() {
            if !measure_directory(&entry.path(), bytes, files)? {
                return Ok(false);
            }
        } else if file_type.is_file() {
            *files = files.saturating_add(1);
            *bytes =
                bytes.saturating_add(entry.metadata().map_err(|error| error.to_string())?.len());
        } else {
            return Ok(false);
        }
    }
    Ok(true)
}

fn contextual_next_commands(kind: ProjectKind) -> &'static [&'static str] {
    match kind {
        ProjectKind::Native => &["pam doctor", "pam dev", "pam test", "pam build"],
        ProjectKind::Desktop => &["pam doctor", "pam dev", "pam test", "pam build"],
        ProjectKind::Laravel => &["pam doctor", "pam dev", "pam test", "pam benchmark <url>"],
        ProjectKind::Product => &["pam info", "pam clean --dry-run", "read README.md"],
        ProjectKind::Api | ProjectKind::Raw => &["pam doctor", "pam dev", "pam test", "pam build"],
    }
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub fn ensure_manifest(context: &ProjectContext) -> Result<bool, String> {
    let path = context.root.join("pam.json");
    if path.is_file() {
        return Ok(false);
    }
    let name = context
        .root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("pam-project");
    let source = serde_json::to_string_pretty(&serde_json::json!({
        "$schema": "https://push-in.github.io/pam-docs/schemas/pam.schema.json",
        "schema": 1,
        "type": context.kind as u8,
        "name": name,
        "version": "0.1.0",
    }))
    .map_err(|error| error.to_string())?
        + "\n";
    fs::write(&path, source)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    Ok(true)
}

pub fn validate_context(context: &ProjectContext) -> Result<(), String> {
    let manifest_path = context.root.join("pam.json");
    if manifest_path.is_file() {
        let source = fs::read(&manifest_path)
            .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?;
        let manifest: serde_json::Value = serde_json::from_slice(&source)
            .map_err(|error| format!("invalid {}: {error}", manifest_path.display()))?;
        let schema = manifest
            .get("schema")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| "pam.json schema must be the integer 1".to_owned())?;
        if schema != 1 {
            return Err(format!("unsupported pam.json schema {schema}; expected 1"));
        }
        let kind = manifest
            .get("type")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| "pam.json type must be an integer from 1 through 6".to_owned())?;
        if kind != context.kind as u64 {
            return Err(format!(
                "pam.json type {kind} does not match the discovered {} project type {}",
                context.kind.label(),
                context.kind as u8
            ));
        }
        if context.kind == ProjectKind::Product {
            let surface_codes = manifest
                .pointer("/workspace/surfaceCodes")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| {
                    "product workspace surfaceCodes must be the integer array [1, 2, 3]".to_owned()
                })?;
            if surface_codes.as_slice()
                != [
                    serde_json::Value::from(1),
                    serde_json::Value::from(2),
                    serde_json::Value::from(3),
                ]
            {
                return Err(
                    "product workspace surfaceCodes must be the integer array [1, 2, 3]".to_owned(),
                );
            }
            if manifest
                .pointer("/workspace/contractPath")
                .and_then(serde_json::Value::as_str)
                != Some("packages/contracts")
            {
                return Err("product workspace contractPath must be packages/contracts".to_owned());
            }
            let contracts = context.root.join("packages/contracts");
            let contracts_metadata = fs::symlink_metadata(&contracts).map_err(|error| {
                format!(
                    "cannot inspect product workspace contracts {}: {error}",
                    contracts.display()
                )
            })?;
            if !contracts_metadata.is_dir() || contracts_metadata.file_type().is_symlink() {
                return Err(
                    "product workspace contracts must be a real project-local directory".to_owned(),
                );
            }
        }
        let name = manifest
            .get("name")
            .and_then(serde_json::Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .ok_or_else(|| "pam.json name must be a non-empty string".to_owned())?;
        if name.chars().count() > 80 || name.contains(['\n', '\r', '\0']) {
            return Err("pam.json name must be safe and at most 80 characters".to_owned());
        }
        if let Some(version) = manifest.get("version") {
            let version = version
                .as_str()
                .filter(|version| valid_version(version))
                .ok_or_else(|| "pam.json version must use MAJOR.MINOR.PATCH SemVer".to_owned())?;
            if version.len() > 64 {
                return Err("pam.json version must be at most 64 bytes".to_owned());
            }
        }
    }
    let composer_path = context.root.join("composer.json");
    if composer_path.is_file() {
        let source = fs::read(&composer_path)
            .map_err(|error| format!("cannot read {}: {error}", composer_path.display()))?;
        serde_json::from_slice::<serde_json::Value>(&source)
            .map_err(|error| format!("invalid {}: {error}", composer_path.display()))?;
    }
    registered_commands(context)?;
    Ok(())
}

pub fn registered_commands(context: &ProjectContext) -> Result<Vec<RegisteredCommand>, String> {
    let mut commands = Vec::new();
    collect_manifest_commands(
        &context.root.join("pam.json"),
        &context.root,
        &context.root,
        &mut commands,
    )?;
    let vendor_directory = crate::composer::discover(&context.root)?
        // A project with its own composer.json must never inherit commands from
        // an installed vendor directory belonging to an ancestor monorepo.
        // Composer discovery intentionally walks upwards for PHP entrypoints,
        // so command discovery constrains that result to this project root.
        .filter(|composer| composer.root == context.root)
        .map(|composer| composer.vendor_directory)
        .unwrap_or_else(|| context.root.join("vendor"));
    let installed = vendor_directory.join("composer/installed.json");
    if installed.is_file() {
        let source = fs::read(&installed)
            .map_err(|error| format!("cannot read {}: {error}", installed.display()))?;
        let data: serde_json::Value = serde_json::from_slice(&source)
            .map_err(|error| format!("invalid {}: {error}", installed.display()))?;
        let packages = data
            .get("packages")
            .and_then(serde_json::Value::as_array)
            .or_else(|| data.as_array());
        if let Some(packages) = packages {
            for package in packages {
                let Some(extra) = package
                    .get("extra")
                    .and_then(|extra| extra.get("pam"))
                    .and_then(|pam| pam.get("commands"))
                else {
                    continue;
                };
                let install_path = package
                    .get("install-path")
                    .or_else(|| package.get("install_path"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(".");
                let package_root = vendor_directory.join("composer").join(install_path);
                collect_commands(extra, &package_root, &context.root, &mut commands)?;
            }
        }
    }
    commands.sort_by(|left, right| left.name.cmp(&right.name));
    if let Some(command) = commands.iter().find(|command| {
        crate::catalog::COMMANDS
            .iter()
            .any(|built_in| built_in.name == command.name)
            && !crate::catalog::package_can_override(&command.name)
    }) {
        return Err(format!(
            "PAM command registration {} shadows a built-in command",
            command.name
        ));
    }
    for pair in commands.windows(2) {
        if pair[0].name == pair[1].name {
            return Err(format!(
                "duplicate PAM command registration: {}",
                pair[0].name
            ));
        }
    }
    Ok(commands)
}

fn collect_manifest_commands(
    path: &Path,
    base: &Path,
    project_root: &Path,
    output: &mut Vec<RegisteredCommand>,
) -> Result<(), String> {
    if !path.is_file() {
        return Ok(());
    }
    let source =
        fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let manifest: serde_json::Value = serde_json::from_slice(&source)
        .map_err(|error| format!("invalid {}: {error}", path.display()))?;
    if let Some(commands) = manifest.get("commands") {
        collect_commands(commands, base, project_root, output)?;
    }
    Ok(())
}

fn collect_commands(
    value: &serde_json::Value,
    base: &Path,
    project_root: &Path,
    output: &mut Vec<RegisteredCommand>,
) -> Result<(), String> {
    let entries = value
        .as_object()
        .ok_or_else(|| "PAM commands must be an object keyed by command name".to_owned())?;
    let project_root = project_root
        .canonicalize()
        .map_err(|error| format!("cannot resolve {}: {error}", project_root.display()))?;
    for (name, definition) in entries {
        if !valid_command_name(name) {
            return Err(format!("invalid PAM command name {name:?}"));
        }
        let (target_kind, target, description, arguments, environment) = if let Some(script) =
            definition.as_str()
        {
            (
                "script",
                script,
                "Application command",
                Vec::new(),
                std::collections::BTreeMap::new(),
            )
        } else {
            let definition = definition
                .as_object()
                .ok_or_else(|| format!("PAM command {name} must be a string or object"))?;
            let script = definition.get("script").and_then(serde_json::Value::as_str);
            let executable = definition.get("bin").and_then(serde_json::Value::as_str);
            let (target_kind, target) = match (script, executable) {
                (Some(script), None) => ("script", script),
                (None, Some(executable)) => ("bin", executable),
                (None, None) => return Err(format!("PAM command {name} requires script or bin")),
                (Some(_), Some(_)) => {
                    return Err(format!(
                        "PAM command {name} must declare exactly one of script or bin"
                    ));
                }
            };
            (
                target_kind,
                target,
                definition
                    .get("description")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Application command"),
                command_arguments(name, definition.get("arguments"))?,
                command_environment(name, definition.get("environment"))?,
            )
        };
        let target = base
            .join(target)
            .canonicalize()
            .map_err(|error| format!("cannot resolve target for PAM command {name}: {error}"))?;
        if !target.starts_with(&project_root) || !target.is_file() {
            return Err(format!("PAM command {name} target escapes the project"));
        }
        output.push(RegisteredCommand {
            name: name.clone(),
            description: description.to_owned(),
            target: if target_kind == "script" {
                CommandTarget::PhpScript(target)
            } else {
                CommandTarget::Executable(target)
            },
            arguments,
            environment,
        });
    }
    Ok(())
}

fn command_environment(
    name: &str,
    value: Option<&serde_json::Value>,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    let Some(value) = value else {
        return Ok(std::collections::BTreeMap::new());
    };
    let values = value
        .as_object()
        .filter(|values| values.len() <= 32)
        .ok_or_else(|| {
            format!("PAM command {name} environment must be an object of at most 32 strings")
        })?;
    values
        .iter()
        .map(|(key, value)| {
            let valid_key = !key.is_empty()
                && key.len() <= 128
                && key
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_');
            let value = value
                .as_str()
                .filter(|value| value.len() <= 4096 && !value.contains(['\0', '\n', '\r']));
            if !valid_key || value.is_none() {
                return Err(format!(
                    "PAM command {name} contains an invalid environment entry"
                ));
            }
            Ok((key.clone(), value.unwrap().to_owned()))
        })
        .collect()
}

fn command_arguments(name: &str, value: Option<&serde_json::Value>) -> Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .filter(|values| values.len() <= 32)
        .ok_or_else(|| {
            format!("PAM command {name} arguments must be an array of at most 32 strings")
        })?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| value.len() <= 4096 && !value.contains(['\0', '\n', '\r']))
                .map(str::to_owned)
                .ok_or_else(|| format!("PAM command {name} contains an invalid argument"))
        })
        .collect()
}

fn valid_command_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 96
        && name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b":-_".contains(&byte)
        })
}

fn valid_version(value: &str) -> bool {
    let (core, suffix) = value
        .split_once('-')
        .map_or((value, None), |(core, suffix)| (core, Some(suffix)));
    let parts = core.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        && suffix.is_none_or(|suffix| {
            !suffix.is_empty()
                && suffix
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
        })
}

#[derive(Deserialize)]
struct PamManifest {
    #[serde(rename = "type")]
    kind: u8,
}

pub fn discover(start: &Path) -> Option<ProjectContext> {
    let start = if start.is_file() {
        start.parent()?
    } else {
        start
    };
    for directory in start.ancestors() {
        if let Some(kind) = manifest_kind(directory) {
            return Some(ProjectContext {
                root: directory.to_path_buf(),
                kind,
            });
        }
        if directory.join("pam-native.json").is_file() {
            return Some(ProjectContext {
                root: directory.to_path_buf(),
                kind: ProjectKind::Native,
            });
        }
        if directory.join("artisan").is_file() {
            return Some(ProjectContext {
                root: directory.to_path_buf(),
                kind: ProjectKind::Laravel,
            });
        }
        if let Some(kind) = composer_kind(directory) {
            return Some(ProjectContext {
                root: directory.to_path_buf(),
                kind,
            });
        }
    }
    None
}

fn manifest_kind(directory: &Path) -> Option<ProjectKind> {
    let source = fs::read(directory.join("pam.json")).ok()?;
    let manifest = serde_json::from_slice::<PamManifest>(&source).ok()?;
    match manifest.kind {
        1 => Some(ProjectKind::Api),
        2 => Some(ProjectKind::Native),
        3 => Some(ProjectKind::Laravel),
        4 => Some(ProjectKind::Desktop),
        5 => Some(ProjectKind::Raw),
        6 => Some(ProjectKind::Product),
        _ => None,
    }
}

fn composer_kind(directory: &Path) -> Option<ProjectKind> {
    let source = fs::read(directory.join("composer.json")).ok()?;
    let manifest = serde_json::from_slice::<serde_json::Value>(&source).ok()?;
    let require = manifest.get("require")?.as_object()?;
    if require.keys().any(|name| name.contains("pam-native")) {
        Some(ProjectKind::Native)
    } else if require.contains_key("laravel/framework") {
        Some(ProjectKind::Laravel)
    } else if require
        .keys()
        .any(|name| name == "pushinbr/pam-desktop" || name.contains("pam/desktop"))
    {
        Some(ProjectKind::Desktop)
    } else if require.keys().any(|name| name.starts_with("pam/")) {
        Some(ProjectKind::Api)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("pam-project-{name}-{}", std::process::id()))
    }

    #[test]
    fn discovers_a_native_project_from_nested_directories() {
        let root = temporary("native");
        let nested = root.join("src/Screens");
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            root.join("pam.json"),
            r#"{"schema":1,"type":2,"name":"Example"}"#,
        )
        .unwrap();

        let context = discover(&nested).unwrap();
        assert_eq!(context.root, root);
        assert_eq!(context.kind, ProjectKind::Native);
        fs::remove_dir_all(context.root).unwrap();
    }

    #[test]
    fn project_cleanup_is_scoped_previewable_and_tiered() {
        let root = temporary("clean");
        fs::create_dir_all(root.join("target/debug/incremental/session")).unwrap();
        fs::create_dir_all(root.join("target/debug/deps")).unwrap();
        fs::create_dir_all(root.join(".pam-native/android/app/build")).unwrap();
        fs::create_dir_all(root.join(".pam/cache/packages")).unwrap();
        fs::create_dir_all(root.join("android/.kotlin/session")).unwrap();
        fs::create_dir_all(root.join(".build/debug")).unwrap();
        fs::create_dir_all(root.join("ios/.build/debug")).unwrap();
        fs::create_dir_all(root.join("scripts/__pycache__")).unwrap();
        fs::create_dir_all(root.join("android/app/src/main")).unwrap();
        fs::write(root.join("index.php"), "<?php\n").unwrap();
        fs::write(root.join(".pam/cache/packages/index.json"), [4_u8; 8]).unwrap();
        fs::write(root.join(".pam/plugin-registry-state.json"), "{}\n").unwrap();
        fs::write(
            root.join("target/debug/incremental/session/cache.bin"),
            [1_u8; 32],
        )
        .unwrap();
        fs::write(root.join("target/debug/deps/application"), [2_u8; 16]).unwrap();
        fs::write(
            root.join(".pam-native/android/app/build/app.apk"),
            [3_u8; 64],
        )
        .unwrap();
        fs::write(root.join("android/.kotlin/session/state.bin"), [4_u8; 8]).unwrap();
        fs::write(root.join(".build/debug/PamMobileUiTests"), [5_u8; 8]).unwrap();
        fs::write(root.join("ios/.build/debug/PamNativeTests"), [5_u8; 8]).unwrap();
        fs::write(root.join("scripts/__pycache__/contract.pyc"), [6_u8; 8]).unwrap();
        fs::write(root.join("Package.swift"), "// swift-tools-version: 5.9\n").unwrap();
        fs::write(
            root.join("android/app/src/main/AndroidManifest.xml"),
            "<manifest />\n",
        )
        .unwrap();
        let context = ProjectContext {
            root: root.clone(),
            kind: ProjectKind::Native,
        };

        clean(&context, false, true, false).expect("preview");
        assert!(
            root.join("target/debug/incremental/session/cache.bin")
                .is_file()
        );
        clean(&context, false, false, false).expect("cache cleanup");
        assert!(!root.join("target/debug/incremental").exists());
        assert!(!root.join(".pam-native/android/app/build").exists());
        assert!(!root.join(".pam/cache").exists());
        assert!(!root.join("android/.kotlin").exists());
        assert!(!root.join(".build").exists());
        assert!(!root.join("ios/.build").exists());
        assert!(!root.join("scripts/__pycache__").exists());
        assert!(root.join("Package.swift").is_file());
        assert!(root.join(".pam/plugin-registry-state.json").is_file());
        assert!(
            root.join("android/app/src/main/AndroidManifest.xml")
                .is_file()
        );
        assert!(root.join("target/debug/deps/application").is_file());
        assert!(root.join("index.php").is_file());

        clean(&context, true, false, false).expect("full cleanup");
        assert!(!root.join("target").exists());
        assert!(!root.join(".pam-native/android").exists());
        assert!(root.join("index.php").is_file());
        assert_eq!(CleanupArtifactKind::Cache as u8, 1);
        assert_eq!(CleanupArtifactKind::Build as u8, 2);
        assert_eq!(ArtifactBudgetState::WithinBudget as u8, 1);
        assert_eq!(ArtifactBudgetState::Exceeded as u8, 2);
        assert_eq!(ArtifactBudgetState::IncompleteScan as u8, 3);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn development_budget_cleans_only_after_the_bound_is_exceeded() {
        let root = temporary("budget");
        fs::create_dir_all(root.join("target/debug/deps")).unwrap();
        fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
        let artifact = root.join("target/debug/deps/runtime.bin");
        let file = fs::File::create(&artifact).unwrap();
        file.set_len(MIN_DEV_ARTIFACT_BUDGET_BYTES + 1).unwrap();
        let context = ProjectContext {
            root: root.clone(),
            kind: ProjectKind::Raw,
        };

        assert_eq!(
            enforce_dev_artifact_budget(&context, MIN_DEV_ARTIFACT_BUDGET_BYTES).unwrap(),
            Some(MIN_DEV_ARTIFACT_BUDGET_BYTES + 1)
        );
        assert!(!root.join("target").exists());
        assert_eq!(
            enforce_dev_artifact_budget(&context, MIN_DEV_ARTIFACT_BUDGET_BYTES).unwrap(),
            None
        );
        assert!(enforce_dev_artifact_budget(&context, MIN_DEV_ARTIFACT_BUDGET_BYTES - 1).is_err());
        assert!(root.join("Cargo.toml").is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn product_cleanup_preserves_private_configuration_and_trust_state() {
        let root = temporary("product-clean");
        fs::create_dir_all(root.join("apps/server/.pam/cache")).unwrap();
        fs::create_dir_all(root.join("apps/native/.pam")).unwrap();
        fs::create_dir_all(root.join("apps/desktop/.pam/cache")).unwrap();
        fs::create_dir_all(root.join("scripts/__pycache__")).unwrap();
        fs::create_dir_all(root.join(".pytest_cache/v/cache")).unwrap();
        fs::create_dir_all(root.join("scripts/product")).unwrap();
        fs::write(root.join("apps/server/.pam/cache/routes.php"), "cache").unwrap();
        fs::write(root.join("apps/native/.pam/google-services.json"), "{}\n").unwrap();
        fs::write(
            root.join("apps/desktop/.pam/desktop-host.artifact.json"),
            "{}\n",
        )
        .unwrap();
        fs::write(root.join("apps/desktop/.pam/cache/session.bin"), "cache").unwrap();
        fs::write(root.join("scripts/__pycache__/evidence.pyc"), "cache").unwrap();
        fs::write(root.join(".pytest_cache/v/cache/nodeids"), "[]\n").unwrap();
        fs::write(root.join("scripts/product/release.py"), "# source\n").unwrap();
        let context = ProjectContext {
            root: root.clone(),
            kind: ProjectKind::Product,
        };

        clean(&context, true, false, false).unwrap();
        assert!(!root.join("apps/server/.pam/cache").exists());
        assert!(!root.join("apps/desktop/.pam/cache").exists());
        assert!(!root.join("scripts/__pycache__").exists());
        assert!(!root.join(".pytest_cache").exists());
        assert!(root.join("scripts/product/release.py").is_file());
        assert!(root.join("apps/native/.pam/google-services.json").is_file());
        assert!(
            root.join("apps/desktop/.pam/desktop-host.artifact.json")
                .is_file()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_rejects_symlinked_ancestor_without_touching_external_data() {
        use std::os::unix::fs::symlink;

        let root = temporary("clean-ancestor-link");
        let outside = temporary("clean-ancestor-outside");
        fs::create_dir_all(root.join(".pam/cache")).unwrap();
        fs::write(root.join(".pam/cache/must-survive.bin"), [6_u8; 8]).unwrap();
        fs::create_dir_all(outside.join(".gradle")).unwrap();
        fs::write(outside.join(".gradle/valuable.bin"), [7_u8; 16]).unwrap();
        symlink(&outside, root.join("android")).unwrap();
        let context = ProjectContext {
            root: root.clone(),
            kind: ProjectKind::Raw,
        };

        let footprint = artifact_footprint(&context).unwrap();
        assert_eq!(footprint["complete"], false);
        let error = clean(&context, false, false, false).unwrap_err();
        assert!(error.contains("resolves through a symlink or outside the project"));
        assert!(root.join(".pam/cache/must-survive.bin").is_file());
        assert!(outside.join(".gradle/valuable.bin").is_file());

        fs::remove_file(root.join("android")).unwrap();
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_refuses_an_incompletely_scanned_artifact() {
        use std::os::unix::fs::symlink;

        let root = temporary("clean-incomplete");
        let outside = temporary("clean-incomplete-sentinel");
        fs::create_dir_all(root.join(".pam/cache")).unwrap();
        fs::write(root.join(".pam/cache/local.bin"), [8_u8; 8]).unwrap();
        fs::write(&outside, [9_u8; 16]).unwrap();
        symlink(&outside, root.join(".pam/cache/external.link")).unwrap();
        let context = ProjectContext {
            root: root.clone(),
            kind: ProjectKind::Raw,
        };

        let footprint = artifact_footprint(&context).unwrap();
        assert_eq!(footprint["complete"], false);
        clean(&context, false, true, false).expect("incomplete preview");
        let error = clean(&context, false, false, false).unwrap_err();
        assert!(error.contains("incompletely scanned artifact"));
        assert!(root.join(".pam/cache/local.bin").is_file());
        assert!(outside.is_file());

        fs::remove_file(root.join(".pam/cache/external.link")).unwrap();
        fs::remove_dir_all(root).unwrap();
        fs::remove_file(outside).unwrap();
    }

    #[test]
    fn discovers_rust_workspaces_only_for_scoped_cleanup() {
        let root = temporary("rust-clean");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
        assert!(discover(&root).is_none());
        let context = discover_cleanable(&root).expect("cleanable Rust workspace");
        assert_eq!(context.root, fs::canonicalize(&root).unwrap());
        assert_eq!(context.kind, ProjectKind::Raw);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn keeps_legacy_native_projects_contextual() {
        let root = temporary("legacy-native");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("pam-native.json"), "{}").unwrap();
        assert_eq!(discover(&root).unwrap().kind, ProjectKind::Native);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn loads_bounded_application_commands() {
        let root = temporary("commands");
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::write(root.join("bin/import.php"), "<?php\n").unwrap();
        fs::write(
            root.join("pam.json"),
            r#"{"schema":1,"type":1,"name":"Commands","commands":{"app:import":{"script":"bin/import.php","description":"Import data"}}}"#,
        )
        .unwrap();
        let context = discover(&root).unwrap();
        let commands = registered_commands(&context).unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "app:import");
        assert_eq!(commands[0].description, "Import data");
        assert!(matches!(commands[0].target, CommandTarget::PhpScript(_)));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn loads_package_executable_commands() {
        let root = temporary("package-bin-commands");
        fs::create_dir_all(root.join("vendor/pushinbr/tool/bin")).unwrap();
        fs::create_dir_all(root.join("vendor/composer")).unwrap();
        fs::write(
            root.join("pam.json"),
            r#"{"schema":1,"type":1,"name":"Commands"}"#,
        )
        .unwrap();
        fs::write(root.join("vendor/pushinbr/tool/bin/tool"), "tool").unwrap();
        fs::write(
            root.join("vendor/composer/installed.json"),
            r#"{"packages":[{"name":"pushinbr/tool","install-path":"../pushinbr/tool","extra":{"pam":{"commands":{"tool:run":{"bin":"bin/tool","description":"Run tool","environment":{"TOOL_MODE":"ci"}}}}}}]}"#,
        )
        .unwrap();

        let context = discover(&root).unwrap();
        let commands = registered_commands(&context).unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "tool:run");
        assert_eq!(commands[0].environment.get("TOOL_MODE").unwrap(), "ci");
        assert!(matches!(commands[0].target, CommandTarget::Executable(_)));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn loads_package_commands_from_a_custom_composer_vendor_directory() {
        let root = temporary("custom-vendor-commands");
        fs::create_dir_all(root.join("dependencies/pushinbr/tool/bin")).unwrap();
        fs::create_dir_all(root.join("dependencies/composer")).unwrap();
        fs::write(
            root.join("pam.json"),
            r#"{"schema":1,"type":1,"name":"Commands"}"#,
        )
        .unwrap();
        fs::write(
            root.join("composer.json"),
            r#"{"name":"app/commands","config":{"vendor-dir":"dependencies"}}"#,
        )
        .unwrap();
        fs::write(root.join("dependencies/autoload.php"), "<?php\n").unwrap();
        fs::write(
            root.join("dependencies/pushinbr/tool/bin/tool.php"),
            "<?php\n",
        )
        .unwrap();
        fs::write(
            root.join("dependencies/composer/installed.json"),
            r#"{"packages":[{"name":"pushinbr/tool","install-path":"../pushinbr/tool","extra":{"pam":{"commands":{"tool:run":{"script":"bin/tool.php"}}}}}]}"#,
        )
        .unwrap();

        let context = discover(&root).unwrap();
        let commands = registered_commands(&context).unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "tool:run");
        assert!(matches!(commands[0].target, CommandTarget::PhpScript(_)));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn does_not_inherit_package_commands_from_an_ancestor_vendor() {
        let root = temporary("nested-composer-commands");
        let project = root.join("packages/app");
        fs::create_dir_all(root.join("vendor/pushinbr/tool/bin")).unwrap();
        fs::create_dir_all(root.join("vendor/composer")).unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(root.join("composer.json"), r#"{"name":"workspace/root"}"#).unwrap();
        fs::write(root.join("vendor/autoload.php"), "<?php\n").unwrap();
        fs::write(root.join("vendor/pushinbr/tool/bin/tool.php"), "<?php\n").unwrap();
        fs::write(
            root.join("vendor/composer/installed.json"),
            r#"{"packages":[{"name":"pushinbr/tool","install-path":"../pushinbr/tool","extra":{"pam":{"commands":{"android:diagnostics":{"script":"bin/tool.php"}}}}}]}"#,
        )
        .unwrap();
        fs::write(project.join("composer.json"), r#"{"name":"app/native"}"#).unwrap();
        fs::write(project.join("pam-native.json"), "{}").unwrap();

        let context = discover(&project).unwrap();
        assert!(registered_commands(&context).unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validates_the_integer_project_contract() {
        let root = temporary("validate");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("pam.json"),
            r#"{"schema":1,"type":1,"name":"Valid"}"#,
        )
        .unwrap();
        let context = discover(&root).unwrap();
        validate_context(&context).unwrap();
        fs::write(
            root.join("pam.json"),
            r#"{"schema":2,"type":1,"name":"Invalid"}"#,
        )
        .unwrap();
        assert!(validate_context(&context).unwrap_err().contains("schema 2"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_to_shadow_builtin_commands() {
        let root = temporary("shadow");
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::write(root.join("bin/start.php"), "<?php\n").unwrap();
        fs::write(
            root.join("pam.json"),
            r#"{"schema":1,"type":1,"name":"Shadow","commands":{"start":"bin/start.php"}}"#,
        )
        .unwrap();
        let context = discover(&root).unwrap();
        assert!(
            registered_commands(&context)
                .unwrap_err()
                .contains("shadows a built-in")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
