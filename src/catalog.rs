use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use serde::Deserialize;

const MAX_CATALOG_BYTES: u64 = 256 * 1024;
const MAX_CATALOG_COMMANDS: usize = 128;

#[derive(Clone, Copy)]
pub struct CommandSpec {
    pub name: &'static str,
    pub summary: &'static str,
    group_code: CommandGroupCode,
    group_label: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum CommandGroupCode {
    Project = 1,
    Develop = 2,
    Generate = 3,
    Ecosystem = 4,
    Quality = 5,
    Ship = 6,
    Runtime = 7,
    Observe = 8,
    Advanced = 9,
}

impl CommandGroupCode {
    fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Project),
            2 => Some(Self::Develop),
            3 => Some(Self::Generate),
            4 => Some(Self::Ecosystem),
            5 => Some(Self::Quality),
            6 => Some(Self::Ship),
            7 => Some(Self::Runtime),
            8 => Some(Self::Observe),
            9 => Some(Self::Advanced),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Project => "Project",
            Self::Develop => "Develop",
            Self::Generate => "Generate",
            Self::Ecosystem => "Ecosystem",
            Self::Quality => "Quality",
            Self::Ship => "Ship",
            Self::Runtime => "Runtime",
            Self::Observe => "Observe",
            Self::Advanced => "Advanced",
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CatalogDocument {
    schema_version: u8,
    commands: Vec<CatalogCommand>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CatalogCommand {
    name: String,
    summary: String,
    group_code: u8,
    group_label: String,
    supports_json: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum CatalogCompatibilityChangeCode {
    CommandRemoved = 1,
    GroupChanged = 2,
    JsonSupportRemoved = 3,
}

pub struct CatalogCompatibilityChange {
    pub change_code: CatalogCompatibilityChangeCode,
    pub command: String,
}

pub struct CatalogCompatibilityReport {
    pub baseline_command_count: usize,
    pub candidate_command_count: usize,
    pub changes: Vec<CatalogCompatibilityChange>,
}

impl CatalogCompatibilityReport {
    pub fn compatible(&self) -> bool {
        self.changes.is_empty()
    }

    pub fn json(&self) -> String {
        serde_json::to_string_pretty(&serde_json::json!({
            "schemaVersion": 1,
            "compatible": self.compatible(),
            "baselineCommandCount": self.baseline_command_count,
            "candidateCommandCount": self.candidate_command_count,
            "changes": self.changes.iter().map(|change| serde_json::json!({
                "changeCode": change.change_code as u8,
                "command": change.command,
            })).collect::<Vec<_>>(),
        }))
        .expect("CLI compatibility report is serializable")
    }
}

impl CommandSpec {
    fn supports_json(self) -> bool {
        matches!(
            self.name,
            "catalog"
                | "clean"
                | "commands"
                | "distribution:verify"
                | "doctor"
                | "info"
                | "packages"
                | "top"
        )
    }
}

pub static COMMANDS: LazyLock<Vec<CommandSpec>> = LazyLock::new(|| {
    vec![
        command("new", "Create a project interactively", "Project"),
        command("init", "Create a project from a preset", "Project"),
        command("info", "Describe the active project", "Project"),
        command("doctor", "Validate or repair the active project", "Project"),
        command(
            "support",
            "Create a bounded redacted support report",
            "Project",
        ),
        command(
            "clean",
            "Remove bounded project development artifacts",
            "Project",
        ),
        command("dev", "Start the contextual development session", "Develop"),
        command("run", "Build and launch the active application", "Develop"),
        command("logs", "Stream logs from the active application", "Develop"),
        command("devices", "List connected development targets", "Develop"),
        command("devtools", "Toggle contextual development tools", "Develop"),
        command("console", "Open the application console", "Develop"),
        command(
            "commands",
            "List application and package commands",
            "Develop",
        ),
        command("make:screen", "Generate a native screen", "Generate"),
        command("make:component", "Generate a native component", "Generate"),
        command(
            "make:native-view",
            "Generate a native view bridge",
            "Generate",
        ),
        command("make:model", "Generate a Laravel model", "Generate"),
        command(
            "make:controller",
            "Generate a Laravel controller",
            "Generate",
        ),
        command(
            "make:request",
            "Generate a Laravel form request",
            "Generate",
        ),
        command(
            "make:resource",
            "Generate a Laravel API resource",
            "Generate",
        ),
        command("make:migration", "Generate a Laravel migration", "Generate"),
        command("make:test", "Generate a Laravel test", "Generate"),
        command(
            "make:command",
            "Generate a Laravel console command",
            "Generate",
        ),
        command("make:job", "Generate a Laravel job", "Generate"),
        command("packages", "List official PAM capabilities", "Ecosystem"),
        command(
            "registry",
            "Verify signed plugin metadata and compatibility",
            "Ecosystem",
        ),
        command("add", "Install an official capability", "Ecosystem"),
        command("remove", "Remove an official capability", "Ecosystem"),
        command(
            "outdated",
            "Inspect available dependency updates",
            "Ecosystem",
        ),
        command("composer", "Run Composer inside PAM", "Ecosystem"),
        command("artisan", "Run Laravel Artisan inside PAM", "Ecosystem"),
        command("format", "Format project source", "Quality"),
        command(
            "lint",
            "Run formatting and static-analysis gates",
            "Quality",
        ),
        command("test", "Run Pest or PHPUnit inside PAM", "Quality"),
        command("benchmark", "Run the contextual benchmark", "Quality"),
        command("profile", "Capture contextual performance data", "Quality"),
        command("build", "Create a release build", "Ship"),
        command("package", "Create a distributable package", "Ship"),
        command("sign", "Validate native release signing", "Ship"),
        command(
            "release",
            "Validate and publish a release candidate",
            "Ship",
        ),
        command("release:verify", "Verify a Product release offline", "Ship"),
        command(
            "distribution:verify",
            "Verify signed clean-host distribution evidence",
            "Ship",
        ),
        command(
            "distribution:sign",
            "Sign verified clean-host distribution evidence",
            "Ship",
        ),
        command(
            "distribution:desktop-report",
            "Bind native Desktop trust proofs to an installer",
            "Ship",
        ),
        command("start", "Run a supervised server cluster", "Runtime"),
        command(
            "octane:start",
            "Start Laravel Octane on the PAM runtime",
            "Runtime",
        ),
        command("octane:status", "Inspect the PAM Octane master", "Runtime"),
        command(
            "octane:reload",
            "Reload PAM Octane without downtime",
            "Runtime",
        ),
        command("octane:stop", "Gracefully stop PAM Octane", "Runtime"),
        command("exec", "Execute a PHP script explicitly", "Runtime"),
        command("inspect", "Inspect runtime capabilities", "Observe"),
        command("routes", "List application routes", "Observe"),
        command("diagnostics", "Capture runtime diagnostics", "Observe"),
        command(
            "timeline",
            "Export a bounded performance timeline",
            "Observe",
        ),
        command("top", "Stream live runtime metrics", "Observe"),
        command("catalog", "Discover the versioned CLI contract", "Advanced"),
        command("mobile", "Use explicit PAM Native commands", "Advanced"),
        command("desktop", "Use explicit PAM Desktop commands", "Advanced"),
        command("completion", "Generate shell completion", "Advanced"),
        command(
            "editor:install",
            "Install PAM language support in an editor",
            "Advanced",
        ),
        command(
            "self-update",
            "Install a cryptographically authorized PAM release",
            "Advanced",
        ),
        command("docs:generate", "Generate the CLI reference", "Advanced"),
    ]
});

fn command(name: &'static str, summary: &'static str, group_label: &'static str) -> CommandSpec {
    let group_code = match group_label {
        "Project" => CommandGroupCode::Project,
        "Develop" => CommandGroupCode::Develop,
        "Generate" => CommandGroupCode::Generate,
        "Ecosystem" => CommandGroupCode::Ecosystem,
        "Quality" => CommandGroupCode::Quality,
        "Ship" => CommandGroupCode::Ship,
        "Runtime" => CommandGroupCode::Runtime,
        "Observe" => CommandGroupCode::Observe,
        "Advanced" => CommandGroupCode::Advanced,
        _ => panic!("command catalog contains an unknown group"),
    };
    CommandSpec {
        name,
        summary,
        group_code,
        group_label,
    }
}

pub fn validate_file(path: &Path) -> Result<usize, String> {
    Ok(read_document(path)?.commands.len())
}

fn read_document(path: &Path) -> Result<CatalogDocument, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect CLI catalog {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "CLI catalog must be a regular non-symlink file: {}",
            path.display()
        ));
    }
    if metadata.len() == 0 || metadata.len() > MAX_CATALOG_BYTES {
        return Err(format!(
            "CLI catalog must contain 1 to {MAX_CATALOG_BYTES} bytes"
        ));
    }
    let source = fs::read(path)
        .map_err(|error| format!("cannot read CLI catalog {}: {error}", path.display()))?;
    let document: CatalogDocument = serde_json::from_slice(&source)
        .map_err(|error| format!("invalid CLI catalog JSON: {error}"))?;
    if document.schema_version != 1 {
        return Err("CLI catalog schemaVersion must be 1".to_owned());
    }
    if document.commands.is_empty() || document.commands.len() > MAX_CATALOG_COMMANDS {
        return Err(format!(
            "CLI catalog must contain 1 to {MAX_CATALOG_COMMANDS} commands"
        ));
    }
    let mut names = HashSet::with_capacity(document.commands.len());
    for command in &document.commands {
        if !valid_command_name(&command.name) {
            return Err(format!("invalid CLI command name {:?}", command.name));
        }
        if !names.insert(command.name.as_str()) {
            return Err(format!("duplicate CLI command name {:?}", command.name));
        }
        let summary_length = command.summary.chars().count();
        if summary_length == 0 || summary_length > 160 {
            return Err(format!(
                "CLI command {:?} summary must contain 1 to 160 characters",
                command.name
            ));
        }
        let group = CommandGroupCode::from_code(command.group_code).ok_or_else(|| {
            format!(
                "CLI command {:?} groupCode must be an integer from 1 to 9",
                command.name
            )
        })?;
        if command.group_label != group.label() {
            return Err(format!(
                "CLI command {:?} groupLabel does not match groupCode {}",
                command.name, command.group_code
            ));
        }
        let _ = command.supports_json;
    }
    Ok(document)
}

pub fn compare_files(
    baseline_path: &Path,
    candidate_path: &Path,
) -> Result<CatalogCompatibilityReport, String> {
    let baseline = read_document(baseline_path)?;
    let candidate = read_document(candidate_path)?;
    let candidate_by_name = candidate
        .commands
        .iter()
        .map(|command| (command.name.as_str(), command))
        .collect::<HashMap<_, _>>();
    let mut changes = Vec::new();

    for baseline_command in &baseline.commands {
        let Some(candidate_command) = candidate_by_name.get(baseline_command.name.as_str()) else {
            changes.push(CatalogCompatibilityChange {
                change_code: CatalogCompatibilityChangeCode::CommandRemoved,
                command: baseline_command.name.clone(),
            });
            continue;
        };
        if candidate_command.group_code != baseline_command.group_code {
            changes.push(CatalogCompatibilityChange {
                change_code: CatalogCompatibilityChangeCode::GroupChanged,
                command: baseline_command.name.clone(),
            });
        }
        if baseline_command.supports_json && !candidate_command.supports_json {
            changes.push(CatalogCompatibilityChange {
                change_code: CatalogCompatibilityChangeCode::JsonSupportRemoved,
                command: baseline_command.name.clone(),
            });
        }
    }

    Ok(CatalogCompatibilityReport {
        baseline_command_count: baseline.commands.len(),
        candidate_command_count: candidate.commands.len(),
        changes,
    })
}

fn valid_command_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 96
        && name.split(':').all(|segment| {
            !segment.is_empty()
                && segment.as_bytes()[0].is_ascii_lowercase()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

pub fn completion(shell: &str) -> Result<String, String> {
    let names = COMMANDS
        .iter()
        .map(|command| command.name)
        .collect::<Vec<_>>();
    let joined = names.join(" ");
    match shell {
        "bash" => Ok(format!(
            "_pam() {{ COMPREPLY=($(compgen -W '{joined}' -- \"${{COMP_WORDS[1]}}\")); }}\ncomplete -F _pam pam\n"
        )),
        "zsh" => Ok(format!(
            "#compdef pam\n_arguments '1:command:({joined})' '*::argument:->args'\n"
        )),
        "fish" => Ok(names
            .iter()
            .map(|name| format!("complete -c pam -n '__fish_use_subcommand' -a '{name}'"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"),
        "powershell" | "pwsh" => Ok(format!(
            "Register-ArgumentCompleter -Native -CommandName pam -ScriptBlock {{ param($wordToComplete) '{joined}'.Split(' ') | Where-Object {{ $_ -like \"$wordToComplete*\" }} }}\n"
        )),
        _ => Err("completion requires bash, zsh, fish, or powershell".to_owned()),
    }
}

pub fn reference() -> String {
    let mut output = String::from(
        "# PAM CLI reference\n\nThis file is generated from the CLI command catalog. Do not edit it manually.\n\n",
    );
    let mut group = "";
    for command in COMMANDS.iter() {
        if command.group_label != group {
            group = command.group_label;
            output.push_str(&format!("## {group}\n\n"));
        }
        output.push_str(&format!(
            "- `pam {}` — {}.\n",
            command.name, command.summary
        ));
    }
    output
}

pub fn json() -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "schemaVersion": 1,
        "commands": COMMANDS.iter().map(|command| serde_json::json!({
            "name": command.name,
            "summary": command.summary,
            "groupCode": command.group_code as u8,
            "groupLabel": command.group_label,
            "supportsJson": command.supports_json(),
        })).collect::<Vec<_>>(),
    }))
    .expect("static CLI catalog is serializable")
}

pub fn schema() -> &'static str {
    include_str!("../docs/schemas/cli-catalog.schema.json")
}

pub fn compatibility_schema() -> &'static str {
    include_str!("../docs/schemas/cli-catalog-compatibility.schema.json")
}

pub fn write_reference(path: &Path, check: bool) -> Result<u8, String> {
    let expected = reference();
    if check {
        let actual = fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        return Ok(if actual == expected { 0 } else { 1 });
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    fs::write(path, expected)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    println!("Generated {}", path.display());
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_names_are_unique_and_all_shells_are_supported() {
        let mut names = COMMANDS
            .iter()
            .map(|command| command.name)
            .collect::<Vec<_>>();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count);
        for shell in ["bash", "zsh", "fish", "powershell"] {
            let script = completion(shell).unwrap();
            assert!(script.contains("doctor"));
            assert!(script.contains("make:component"));
        }
        assert_eq!(CommandGroupCode::Project as u8, 1);
        assert_eq!(CommandGroupCode::Develop as u8, 2);
        assert_eq!(CommandGroupCode::Generate as u8, 3);
        assert_eq!(CommandGroupCode::Ecosystem as u8, 4);
        assert_eq!(CommandGroupCode::Quality as u8, 5);
        assert_eq!(CommandGroupCode::Ship as u8, 6);
        assert_eq!(CommandGroupCode::Runtime as u8, 7);
        assert_eq!(CommandGroupCode::Observe as u8, 8);
        assert_eq!(CommandGroupCode::Advanced as u8, 9);
        assert!(COMMANDS.iter().all(|command| command.group_code as u8 > 0));
    }
}
