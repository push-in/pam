use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

const INSTALLER: &str = include_str!("../install.sh");

pub fn run(arguments: impl Iterator<Item = OsString>) -> Result<u8, String> {
    let mut check = false;
    let mut version = None;
    for argument in arguments {
        match argument.to_string_lossy().as_ref() {
            "--check" => check = true,
            option if option.starts_with('-') => {
                return Err(format!("unknown self-update option: {option}"));
            }
            value if version.is_none() => version = Some(value.to_owned()),
            _ => return Err("self-update accepts at most one release version".to_owned()),
        }
    }
    if let Some(version) = version.as_deref()
        && !valid_release(version)
    {
        return Err("release version must use vMAJOR.MINOR.PATCH".to_owned());
    }
    if check {
        return check_latest(version.as_deref());
    }
    install(version.as_deref())
}

fn check_latest(requested: Option<&str>) -> Result<u8, String> {
    let latest = if let Some(requested) = requested {
        requested.to_owned()
    } else {
        latest_release()?
    };
    let current = format!("v{}", env!("CARGO_PKG_VERSION"));
    if latest == current {
        println!("PAM {current} is up to date.");
        Ok(0)
    } else {
        println!("PAM {latest} is available; installed version is {current}.");
        println!("Run `pam self-update {latest}` to install it.");
        Ok(10)
    }
}

fn latest_release() -> Result<String, String> {
    let repository =
        std::env::var("PAM_GITHUB_REPOSITORY").unwrap_or_else(|_| "push-in/pam".to_owned());
    let url = std::env::var("PAM_RELEASE_API_URL")
        .unwrap_or_else(|_| format!("https://api.github.com/repos/{repository}/releases/latest"));
    if !url.starts_with("https://") && std::env::var_os("PAM_RELEASE_API_URL").is_none() {
        return Err("refusing a non-HTTPS release API".to_owned());
    }
    let output = Command::new("curl")
        .args([
            "--proto",
            "=https",
            "--tlsv1.2",
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            &url,
        ])
        .output()
        .map_err(|error| format!("cannot query PAM releases with curl: {error}"))?;
    if !output.status.success() {
        return Err(format!("release lookup failed with {}", output.status));
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("invalid release metadata: {error}"))?;
    let tag = metadata
        .get("tag_name")
        .and_then(serde_json::Value::as_str)
        .filter(|tag| valid_release(tag))
        .ok_or_else(|| "latest release does not contain a valid SemVer tag".to_owned())?;
    Ok(tag.to_owned())
}

fn install(version: Option<&str>) -> Result<u8, String> {
    #[cfg(not(unix))]
    return Err("official PAM self-update currently supports Linux releases".to_owned());

    #[cfg(unix)]
    {
        let path = temporary_installer()?;
        let mut command = Command::new(&path);
        if let Some(version) = version {
            command.arg(version);
        }
        let status = command
            .status()
            .map_err(|error| format!("cannot start the verified PAM installer: {error}"));
        let cleanup = fs::remove_file(&path);
        let status = match status {
            Ok(status) => status,
            Err(error) => {
                let _ = cleanup;
                return Err(error);
            }
        };
        if !status.success() {
            return Err(format!("PAM installer failed with {status}"));
        }
        cleanup.map_err(|error| format!("cannot remove temporary installer: {error}"))?;
        println!("PAM update complete. The next command uses the new release.");
        Ok(0)
    }
}

#[cfg(unix)]
fn temporary_installer() -> Result<PathBuf, String> {
    for attempt in 0..32_u8 {
        let path = std::env::temp_dir().join(format!(
            "pam-self-update-{}-{attempt}.sh",
            std::process::id()
        ));
        let file = OpenOptions::new().write(true).create_new(true).open(&path);
        let mut file = match file {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("cannot create temporary installer: {error}")),
        };
        file.write_all(INSTALLER.as_bytes())
            .map_err(|error| format!("cannot write temporary installer: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("cannot sync temporary installer: {error}"))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("cannot secure temporary installer: {error}"))?;
        return Ok(path);
    }
    Err("cannot allocate a unique temporary installer path".to_owned())
}

fn valid_release(value: &str) -> bool {
    let Some(value) = value.strip_prefix('v') else {
        return false;
    };
    let (core, prerelease) = value
        .split_once('-')
        .map_or((value, None), |(core, suffix)| (core, Some(suffix)));
    let parts = core.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        && prerelease.is_none_or(|suffix| {
            !suffix.is_empty()
                && suffix
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_tags_are_strict_semver() {
        assert!(valid_release("v1.0.0"));
        assert!(valid_release("v1.0.0-rc.1"));
        assert!(!valid_release("1.0.0"));
        assert!(!valid_release("v1.0"));
        assert!(!valid_release("vnext"));
        assert!(!valid_release("v1.0.0-"));
    }

    #[cfg(unix)]
    #[test]
    fn embedded_installer_is_written_private_and_executable() {
        let path = temporary_installer().unwrap();
        let metadata = fs::metadata(&path).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
        assert_eq!(fs::read_to_string(&path).unwrap(), INSTALLER);
        fs::remove_file(path).unwrap();
    }
}
