use std::ffi::OsStr;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

use crate::project::ProjectKind;

pub fn release(
    executable: &OsStr,
    project: &Path,
    kind: ProjectKind,
    check_only: bool,
) -> Result<u8, String> {
    println!("PAM release gate for {}", kind.label());
    for arguments in release_commands(kind, check_only) {
        println!("\n$ pam {}", arguments.join(" "));
        let status = Command::new(executable)
            .args(&arguments)
            .current_dir(project)
            .env("PAM_COLOR", "never")
            .status()
            .map_err(|error| format!("cannot run PAM release gate: {error}"))?;
        if !status.success() {
            return Err(format!(
                "release gate `pam {}` failed with {status}",
                arguments.join(" ")
            ));
        }
    }
    if check_only {
        println!("\nRelease checks passed; no distributable was created.");
    } else {
        println!("\nRelease candidate passed every local gate and was packaged in dist/.");
    }
    Ok(0)
}

fn release_commands(kind: ProjectKind, check_only: bool) -> Vec<Vec<&'static str>> {
    let mut commands = vec![vec!["doctor", "--ci"], vec!["lint"], vec!["test"]];
    if kind == ProjectKind::Native {
        commands.push(vec!["sign"]);
    }
    if !check_only {
        commands.push(vec!["package"]);
    }
    commands
}

pub fn package_server(
    project: &Path,
    kind: ProjectKind,
    arguments: impl Iterator<Item = OsString>,
) -> Result<u8, String> {
    let mut output = project.join("dist");
    let mut entry = None::<PathBuf>;
    let mut arguments = arguments;
    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--output" => {
                let path = arguments
                    .next()
                    .ok_or_else(|| "--output requires a directory".to_owned())?;
                output = if Path::new(&path).is_absolute() {
                    PathBuf::from(path)
                } else {
                    project.join(path)
                };
            }
            "--entry" => {
                entry = Some(PathBuf::from(
                    arguments
                        .next()
                        .ok_or_else(|| "--entry requires a PHP file".to_owned())?,
                ));
            }
            option => return Err(format!("unknown package option: {option}")),
        }
    }
    let entry = entry.unwrap_or_else(|| default_entry(project, kind));
    let (name, version) = package_identity(project)?;
    let stem = format!(
        "{name}-{version}-{}-{}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    fs::create_dir_all(&output)
        .map_err(|error| format!("cannot create {}: {error}", output.display()))?;
    let bundle = output.join(&stem);
    let archive = output.join(format!("{stem}.tar.gz"));
    if archive.exists() {
        return Err(format!(
            "refusing to overwrite package {}; remove it or choose --output",
            archive.display()
        ));
    }
    crate::commands::build(project, &bundle, &entry)?;
    let status = Command::new("tar")
        .args(["-czf"])
        .arg(&archive)
        .arg(&stem)
        .current_dir(&output)
        .status()
        .map_err(|error| format!("cannot create production archive with tar: {error}"))?;
    if !status.success() {
        return Err(format!("production archive failed with {status}"));
    }
    let digest = Sha256::digest(
        fs::read(&archive)
            .map_err(|error| format!("cannot hash {}: {error}", archive.display()))?,
    );
    let checksum = archive.with_extension("gz.sha256");
    fs::write(
        &checksum,
        format!(
            "{digest:x}  {}\n",
            archive.file_name().unwrap_or_default().to_string_lossy()
        ),
    )
    .map_err(|error| format!("cannot write {}: {error}", checksum.display()))?;
    println!("Packaged {}", archive.display());
    println!("Checksum {}", checksum.display());
    Ok(0)
}

fn default_entry(project: &Path, kind: ProjectKind) -> PathBuf {
    if kind == ProjectKind::Laravel && project.join("pam.php").is_file() {
        PathBuf::from("pam.php")
    } else if project.join("public/index.php").is_file() {
        PathBuf::from("public/index.php")
    } else {
        PathBuf::from("index.php")
    }
}

fn package_identity(project: &Path) -> Result<(String, String), String> {
    let mut name = project
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("pam-app")
        .to_owned();
    let mut version = "0.1.0".to_owned();
    for filename in ["pam.json", "composer.json"] {
        let path = project.join(filename);
        if !path.is_file() {
            continue;
        }
        let manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(&path).map_err(|error| format!("cannot read {}: {error}", path.display()))?,
        )
        .map_err(|error| format!("invalid {}: {error}", path.display()))?;
        if let Some(value) = manifest.get("name").and_then(serde_json::Value::as_str) {
            name = value.rsplit('/').next().unwrap_or(value).to_owned();
        }
        if let Some(value) = manifest.get("version").and_then(serde_json::Value::as_str) {
            version = value.to_owned();
        }
    }
    let safe = |value: &str| {
        value
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || matches!(character, '.' | '-') {
                    character.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .trim_matches('-')
            .to_owned()
    };
    let name = safe(&name);
    let version = safe(&version);
    if name.is_empty() || version.is_empty() {
        return Err("package name and version must contain safe characters".to_owned());
    }
    Ok((name, version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_release_requires_signing_before_packaging() {
        let commands = release_commands(ProjectKind::Native, false);
        assert_eq!(commands[3], vec!["sign"]);
        assert_eq!(commands[4], vec!["package"]);
        assert!(
            !release_commands(ProjectKind::Native, true)
                .iter()
                .any(|command| command == &vec!["package"])
        );
    }

    #[test]
    fn every_release_creates_a_package() {
        for kind in [ProjectKind::Api, ProjectKind::Laravel, ProjectKind::Raw] {
            assert_eq!(
                release_commands(kind, false).last().unwrap(),
                &vec!["package"]
            );
        }
    }
}
