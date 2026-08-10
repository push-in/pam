use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

pub fn install(arguments: impl Iterator<Item = OsString>) -> Result<u8, String> {
    let mut editor = "vscode".to_owned();
    let mut force = false;
    let mut positional = false;
    for argument in arguments {
        match argument.to_string_lossy().as_ref() {
            "--force" => force = true,
            option if option.starts_with('-') => {
                return Err(format!("unknown editor:install option: {option}"));
            }
            value if !positional => {
                editor = value.to_owned();
                positional = true;
            }
            _ => return Err("editor:install accepts one editor name".to_owned()),
        }
    }
    match editor.as_str() {
        "vscode" | "code" => install_vscode(force),
        "neovim" | "nvim" => install_neovim(force),
        "helix" | "hx" => install_helix(force),
        _ => Err("supported editors: vscode, neovim, helix".to_owned()),
    }
}

fn install_vscode(force: bool) -> Result<u8, String> {
    let source = editor_root()?.join("vscode");
    let extensions = if let Some(path) = std::env::var_os("PAM_VSCODE_EXTENSIONS") {
        PathBuf::from(path)
    } else {
        home_directory()?.join(".vscode/extensions")
    };
    let destination = extensions.join(extension_identifier(&source)?);
    if destination.exists() {
        if !force {
            return Err(format!(
                "{} already exists; use --force to replace this PAM extension",
                destination.display()
            ));
        }
        fs::remove_dir_all(&destination)
            .map_err(|error| format!("cannot replace {}: {error}", destination.display()))?;
    }
    fs::create_dir_all(&extensions)
        .map_err(|error| format!("cannot create {}: {error}", extensions.display()))?;
    copy_tree(&source, &destination)?;
    println!("Installed PAM Native language support for VS Code.");
    println!("Restart VS Code, then open a .pam file; formatting on save is enabled.");
    Ok(0)
}

fn extension_identifier(source: &Path) -> Result<String, String> {
    let manifest_path = source.join("package.json");
    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(&manifest_path)
            .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?,
    )
    .map_err(|error| format!("invalid {}: {error}", manifest_path.display()))?;
    let field = |name: &str| {
        manifest
            .get(name)
            .and_then(serde_json::Value::as_str)
            .filter(|value| {
                !value.is_empty()
                    && value.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')
                    })
            })
            .ok_or_else(|| format!("VS Code extension {name} is missing or unsafe"))
    };
    Ok(format!(
        "{}.{}-{}",
        field("publisher")?,
        field("name")?,
        field("version")?
    ))
}

fn install_neovim(force: bool) -> Result<u8, String> {
    let config = configured_directory("PAM_NEOVIM_CONFIG_DIR", "nvim")?;
    install_neovim_at(&config, force)?;
    println!(
        "Installed PAM Native language support for Neovim at {}.",
        config.display()
    );
    println!("Restart Neovim, then open a .pam file.");
    Ok(0)
}

fn install_neovim_at(config: &Path, force: bool) -> Result<PathBuf, String> {
    let source = editor_root()?.join("neovim.lua");
    let destination = config.join("plugin/pam-native.lua");
    write_dedicated_file(&source, &destination, force)?;
    Ok(destination)
}

const HELIX_START: &str = "# >>> PAM Native (managed by pam editor:install) >>>";
const HELIX_END: &str = "# <<< PAM Native (managed by pam editor:install) <<<";

fn install_helix(force: bool) -> Result<u8, String> {
    let config = configured_directory("PAM_HELIX_CONFIG_DIR", "helix")?;
    let path = install_helix_at(&config, force)?;
    println!(
        "Installed PAM Native language support for Helix at {}.",
        path.display()
    );
    println!("Restart Helix, then open a .pam file.");
    Ok(0)
}

fn install_helix_at(config: &Path, force: bool) -> Result<PathBuf, String> {
    let source = editor_root()?.join("helix-languages.toml");
    let snippet = fs::read_to_string(&source)
        .map_err(|error| format!("cannot read {}: {error}", source.display()))?;
    let destination = config.join("languages.toml");
    if fs::symlink_metadata(&destination).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(format!(
            "refusing to modify configuration symlink {}",
            destination.display()
        ));
    }
    if destination.exists() && !destination.is_file() {
        return Err(format!(
            "configuration path is not a file: {}",
            destination.display()
        ));
    }
    let existing = if destination.is_file() {
        fs::read_to_string(&destination)
            .map_err(|error| format!("cannot read {}: {error}", destination.display()))?
    } else {
        String::new()
    };
    let managed = format!(
        "{HELIX_START}\n{}{HELIX_END}\n",
        snippet.trim_end_matches('\n')
    );
    let updated = if let Some(start) = existing.find(HELIX_START) {
        if !force {
            return Err(format!(
                "PAM Native is already configured in {}; use --force to refresh its managed block",
                destination.display()
            ));
        }
        let relative_end = existing[start..].find(HELIX_END).ok_or_else(|| {
            format!(
                "{} contains an incomplete PAM managed block",
                destination.display()
            )
        })?;
        let end = start + relative_end + HELIX_END.len();
        let mut output = existing[..start].to_owned();
        output.push_str(&managed);
        output.push_str(existing[end..].trim_start_matches(['\r', '\n']));
        output
    } else {
        let mut output = existing;
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&managed);
        output
    };
    write_atomic(&destination, updated.as_bytes())?;
    Ok(destination)
}

fn configured_directory(environment: &str, editor: &str) -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os(environment) {
        return Ok(PathBuf::from(path));
    }
    if let Some(path) = std::env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(path).join(editor));
    }
    Ok(home_directory()?.join(".config").join(editor))
}

fn write_dedicated_file(source: &Path, destination: &Path, force: bool) -> Result<(), String> {
    if fs::symlink_metadata(destination).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(format!(
            "refusing to replace configuration symlink {}",
            destination.display()
        ));
    }
    if destination.exists() && !destination.is_file() {
        return Err(format!(
            "configuration path is not a file: {}",
            destination.display()
        ));
    }
    if destination.exists() && !force {
        return Err(format!(
            "{} already exists; use --force to replace the PAM-managed file",
            destination.display()
        ));
    }
    let contents =
        fs::read(source).map_err(|error| format!("cannot read {}: {error}", source.display()))?;
    write_atomic(destination, &contents)
}

fn write_atomic(destination: &Path, contents: &[u8]) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", destination.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    let temporary = parent.join(format!(".pam-editor-{}.tmp", std::process::id()));
    if temporary.exists() {
        return Err(format!(
            "temporary editor path already exists: {}",
            temporary.display()
        ));
    }
    fs::write(&temporary, contents)
        .map_err(|error| format!("cannot write {}: {error}", temporary.display()))?;
    let backup = parent.join(format!(".pam-editor-{}.backup", std::process::id()));
    let had_destination = destination.exists();
    if had_destination {
        if backup.exists() {
            let _ = fs::remove_file(&temporary);
            return Err(format!(
                "temporary editor backup already exists: {}",
                backup.display()
            ));
        }
        fs::rename(destination, &backup).map_err(|error| {
            format!(
                "cannot stage {} for replacement: {error}",
                destination.display()
            )
        })?;
    }
    if let Err(error) = fs::rename(&temporary, destination) {
        if had_destination {
            let _ = fs::rename(&backup, destination);
        }
        let _ = fs::remove_file(&temporary);
        return Err(format!("cannot install {}: {error}", destination.display()));
    }
    if had_destination {
        fs::remove_file(&backup)
            .map_err(|error| format!("cannot remove {}: {error}", backup.display()))?;
    }
    Ok(())
}

fn editor_root() -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("PAM_NATIVE_HOME") {
        candidates.push(PathBuf::from(path).join("editors"));
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pam-native/editors"));
    if let Ok(executable) = std::env::current_exe()
        && let Some(binary) = executable.parent()
    {
        candidates.push(binary.join("../share/pam/native/editors"));
        candidates.push(binary.join("../lib/pam/native/editors"));
    }
    candidates
        .into_iter()
        .find(|path| path.join("vscode/package.json").is_file())
        .ok_or_else(|| {
            "PAM Native editor assets were not found; set PAM_NATIVE_HOME to the SDK directory"
                .to_owned()
        })
}

fn home_directory() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or_else(|| "cannot locate the user profile for VS Code extensions".to_owned())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination)
        .map_err(|error| format!("cannot create {}: {error}", destination.display()))?;
    for entry in fs::read_dir(source)
        .map_err(|error| format!("cannot read {}: {error}", source.display()))?
    {
        let entry = entry.map_err(|error| format!("cannot read editor asset: {error}"))?;
        let kind = entry
            .file_type()
            .map_err(|error| format!("cannot inspect {}: {error}", entry.path().display()))?;
        let target = destination.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &target)
                .map_err(|error| format!("cannot copy {}: {error}", target.display()))?;
        } else {
            return Err(format!(
                "editor asset {} must be a regular file or directory",
                entry.path().display()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_assets_are_shipped_with_the_cli_workspace() {
        let root = editor_root().unwrap();
        assert!(root.join("vscode/extension.js").is_file());
        assert!(root.join("neovim.lua").is_file());
        assert!(root.join("helix-languages.toml").is_file());
        assert_eq!(
            extension_identifier(&root.join("vscode")).unwrap(),
            "pushin.pam-native-0.1.0"
        );
    }

    #[test]
    fn installs_neovim_and_merges_helix_without_destroying_user_configuration() {
        let root = std::env::temp_dir().join(format!("pam-editors-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let neovim = install_neovim_at(&root.join("nvim"), false).unwrap();
        assert!(
            fs::read_to_string(neovim)
                .unwrap()
                .contains("vim.lsp.config.pam_native")
        );

        let helix_root = root.join("helix");
        fs::create_dir_all(&helix_root).unwrap();
        fs::write(helix_root.join("languages.toml"), "# user configuration\n").unwrap();
        let helix = install_helix_at(&helix_root, false).unwrap();
        let installed = fs::read_to_string(&helix).unwrap();
        assert!(installed.starts_with("# user configuration\n"));
        assert!(installed.contains(HELIX_START));
        assert!(install_helix_at(&helix_root, false).is_err());
        install_helix_at(&helix_root, true).unwrap();
        let refreshed = fs::read_to_string(&helix).unwrap();
        assert_eq!(refreshed.matches(HELIX_START).count(), 1);
        assert!(refreshed.starts_with("# user configuration\n"));
        fs::remove_dir_all(root).unwrap();
    }
}
