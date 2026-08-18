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
}

impl ProjectKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Api => "API",
            Self::Native => "PAM Native",
            Self::Laravel => "Laravel",
            Self::Desktop => "PAM Desktop",
            Self::Raw => "PAM Runtime",
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
    pub script: PathBuf,
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

pub fn native_platforms(context: &ProjectContext) -> Result<Vec<u8>, String> {
    if context.kind != ProjectKind::Native {
        return Ok(Vec::new());
    }
    let path = context.root.join("pam.json");
    if !path.is_file() {
        return Ok(vec![1]);
    }
    let source =
        fs::read(&path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let manifest: serde_json::Value = serde_json::from_slice(&source)
        .map_err(|error| format!("invalid {}: {error}", path.display()))?;
    let platforms = manifest
        .pointer("/native/platforms")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "pam.json native.platforms must be an integer array".to_owned())?;
    if platforms.is_empty() {
        return Err(
            "pam.json native.platforms must select Android (1), iOS (2), or both".to_owned(),
        );
    }
    let mut result = Vec::new();
    for value in platforms {
        let value = value
            .as_u64()
            .and_then(|value| u8::try_from(value).ok())
            .filter(|value| matches!(value, 1 | 2))
            .ok_or_else(|| "pam.json native.platforms values must be integer 1 or 2".to_owned())?;
        if !result.contains(&value) {
            result.push(value);
        }
    }
    Ok(result)
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
        human_bytes(development.bytes),
        development.files,
        if development.complete {
            ""
        } else {
            " (partial scan)"
        }
    );
    println!(
        "  {}  {}",
        ui.heading("Location"),
        development.root.display()
    );
    println!();
    println!("{}", ui.heading("NEXT"));
    for command in next_commands {
        println!("  {}", ui.command(command));
    }
    Ok(0)
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
        }
    }
    Ok(true)
}

fn contextual_next_commands(kind: ProjectKind) -> &'static [&'static str] {
    match kind {
        ProjectKind::Native => &["pam doctor", "pam dev", "pam test", "pam build"],
        ProjectKind::Desktop => &["pam doctor", "pam dev", "pam test", "pam build"],
        ProjectKind::Laravel => &["pam doctor", "pam dev", "pam test", "pam benchmark <url>"],
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
            .ok_or_else(|| "pam.json type must be an integer from 1 through 5".to_owned())?;
        if kind != context.kind as u64 {
            return Err(format!(
                "pam.json type {kind} does not match the discovered {} project type {}",
                context.kind.label(),
                context.kind as u8
            ));
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
    let installed = context.root.join("vendor/composer/installed.json");
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
                    .get("install_path")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(".");
                let package_root = context.root.join("vendor/composer").join(install_path);
                collect_commands(extra, &package_root, &context.root, &mut commands)?;
            }
        }
    }
    commands.sort_by(|left, right| left.name.cmp(&right.name));
    if let Some(command) = commands.iter().find(|command| {
        crate::catalog::COMMANDS
            .iter()
            .any(|built_in| built_in.name == command.name)
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
        let (script, description) = if let Some(script) = definition.as_str() {
            (script, "Application command")
        } else {
            let definition = definition
                .as_object()
                .ok_or_else(|| format!("PAM command {name} must be a string or object"))?;
            (
                definition
                    .get("script")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| format!("PAM command {name} requires script"))?,
                definition
                    .get("description")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Application command"),
            )
        };
        let script = base
            .join(script)
            .canonicalize()
            .map_err(|error| format!("cannot resolve script for PAM command {name}: {error}"))?;
        if !script.starts_with(&project_root) || !script.is_file() {
            return Err(format!("PAM command {name} script escapes the project"));
        }
        output.push(RegisteredCommand {
            name: name.clone(),
            description: description.to_owned(),
            script,
        });
    }
    Ok(())
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
        fs::write(root.join("bin/dev.php"), "<?php\n").unwrap();
        fs::write(
            root.join("pam.json"),
            r#"{"schema":1,"type":1,"name":"Shadow","commands":{"dev":"bin/dev.php"}}"#,
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
