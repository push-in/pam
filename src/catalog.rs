use std::fs;
use std::path::Path;

#[derive(Clone, Copy)]
pub struct CommandSpec {
    pub name: &'static str,
    pub summary: &'static str,
    pub group: &'static str,
}

pub const COMMANDS: &[CommandSpec] = &[
    command("new", "Create a project interactively", "Project"),
    command("init", "Create a project from a preset", "Project"),
    command("info", "Describe the active project", "Project"),
    command("doctor", "Validate or repair the active project", "Project"),
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
    command("top", "Stream live runtime metrics", "Observe"),
    command("mobile", "Use explicit PAM Native commands", "Advanced"),
    command("desktop", "Use explicit PAM Desktop commands", "Advanced"),
    command("completion", "Generate shell completion", "Advanced"),
    command(
        "editor:install",
        "Install PAM language support in an editor",
        "Advanced",
    ),
    command("self-update", "Install a verified PAM release", "Advanced"),
    command("docs:generate", "Generate the CLI reference", "Advanced"),
];

const fn command(name: &'static str, summary: &'static str, group: &'static str) -> CommandSpec {
    CommandSpec {
        name,
        summary,
        group,
    }
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
    for command in COMMANDS {
        if command.group != group {
            group = command.group;
            output.push_str(&format!("## {group}\n\n"));
        }
        output.push_str(&format!(
            "- `pam {}` — {}.\n",
            command.name, command.summary
        ));
    }
    output
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
    }
}
