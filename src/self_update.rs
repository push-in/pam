use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use semver::Version;

const INSTALLER: &str = include_str!("../install.sh");
const MAX_RELEASE_METADATA_BYTES: usize = 1024 * 1024;
const MAX_UPDATE_MANIFEST_BYTES: usize = 256 * 1024;
const MAX_INSTALL_ARCHIVE_BYTES: u64 = 1024 * 1024 * 1024;
const RELEASE_CONNECT_TIMEOUT_SECONDS: &str = "15";
const RELEASE_MAX_TIME_SECONDS: &str = "30";
const UPDATE_SIGNING_IDENTITY: Option<&str> = option_env!("PAM_UPDATE_SIGNING_IDENTITY_SHA256");
const UPDATE_NEXT_SIGNING_IDENTITY: Option<&str> =
    option_env!("PAM_UPDATE_NEXT_SIGNING_IDENTITY_SHA256");

pub fn run(arguments: impl Iterator<Item = OsString>) -> Result<u8, String> {
    let mut check = false;
    let mut allow_downgrade = false;
    let mut version = None;
    for argument in arguments {
        match argument.to_string_lossy().as_ref() {
            "--check" => check = true,
            "--allow-downgrade" => allow_downgrade = true,
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
    if allow_downgrade && (check || version.is_none()) {
        return Err("--allow-downgrade requires one explicit release version".to_owned());
    }
    if check {
        return check_latest(version.as_deref());
    }
    let automatically_discovered = version.is_none();
    let selected = match version {
        Some(version) => version,
        None => latest_release()?,
    };
    if automatically_discovered {
        authorize_available_update(&selected, true)?;
    }
    match update_transition(&selected, allow_downgrade)? {
        UpdateTransition::Current => {
            println!("PAM {selected} is already installed.");
            return Ok(0);
        }
        UpdateTransition::Upgrade | UpdateTransition::ExplicitDowngrade => {}
    }
    install(&selected, automatically_discovered)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UpdateTransition {
    Current,
    Upgrade,
    ExplicitDowngrade,
}

fn update_transition(candidate: &str, allow_downgrade: bool) -> Result<UpdateTransition, String> {
    let candidate = candidate
        .strip_prefix('v')
        .ok_or_else(|| "release version must use canonical SemVer".to_owned())?;
    let candidate = Version::parse(candidate)
        .map_err(|_| "release version must use canonical SemVer".to_owned())?;
    let current = Version::parse(env!("CARGO_PKG_VERSION"))
        .expect("Cargo package version must be valid SemVer");
    match candidate.cmp(&current) {
        std::cmp::Ordering::Equal => Ok(UpdateTransition::Current),
        std::cmp::Ordering::Greater => Ok(UpdateTransition::Upgrade),
        std::cmp::Ordering::Less if allow_downgrade => Ok(UpdateTransition::ExplicitDowngrade),
        std::cmp::Ordering::Less => Err(format!(
            "refusing to replace PAM v{current} with older release v{candidate}; pass --allow-downgrade with that explicit version"
        )),
    }
}

fn check_latest(requested: Option<&str>) -> Result<u8, String> {
    let automatically_discovered = requested.is_none();
    let latest = if let Some(requested) = requested {
        requested.to_owned()
    } else {
        latest_release()?
    };
    let current = format!("v{}", env!("CARGO_PKG_VERSION"));
    if automatically_discovered {
        authorize_available_update(&latest, true)?;
    }
    match update_transition(&latest, false)? {
        UpdateTransition::Current => {
            println!("PAM {current} is up to date.");
            Ok(0)
        }
        UpdateTransition::Upgrade => {
            if !automatically_discovered {
                authorize_available_update(&latest, false)?;
            }
            println!(
                "PAM {latest} is available and cryptographically authorized; installed version is {current}."
            );
            println!("Run `pam self-update {latest}` to install it.");
            Ok(10)
        }
        UpdateTransition::ExplicitDowngrade => {
            unreachable!("--check never authorizes a downgrade")
        }
    }
}

fn authorize_available_update(version: &str, require_freshness: bool) -> Result<(), String> {
    #[cfg(unix)]
    {
        update_authorization(version, require_freshness).map(drop)
    }
    #[cfg(not(unix))]
    {
        let _ = version;
        Err("official PAM self-update does not support this target".to_owned())
    }
}

fn latest_release() -> Result<String, String> {
    let repository =
        std::env::var("PAM_GITHUB_REPOSITORY").unwrap_or_else(|_| "push-in/pam".to_owned());
    let url = std::env::var("PAM_RELEASE_API_URL")
        .unwrap_or_else(|_| format!("https://api.github.com/repos/{repository}/releases/latest"));
    validate_release_api_url(&url)?;
    let output = Command::new("curl")
        .args([
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--tlsv1.2",
            "--connect-timeout",
            RELEASE_CONNECT_TIMEOUT_SECONDS,
            "--max-time",
            RELEASE_MAX_TIME_SECONDS,
            "--max-filesize",
            &MAX_RELEASE_METADATA_BYTES.to_string(),
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
    decode_latest_release(&output.stdout)
}

fn validate_release_api_url(url: &str) -> Result<(), String> {
    validate_https_url(url, "release API")
}

fn validate_https_url(url: &str, label: &str) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err(format!("{label} must use HTTPS"));
    }
    if url.len() > 4096 || url.bytes().any(|byte| byte.is_ascii_control()) {
        return Err(format!("{label} URL is invalid or exceeds 4 KiB"));
    }
    Ok(())
}

fn decode_latest_release(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() > MAX_RELEASE_METADATA_BYTES {
        return Err("release metadata exceeds one MiB".to_owned());
    }
    let metadata: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("invalid release metadata: {error}"))?;
    let tag = metadata
        .get("tag_name")
        .and_then(serde_json::Value::as_str)
        .filter(|tag| valid_release(tag))
        .ok_or_else(|| "latest release does not contain a valid SemVer tag".to_owned())?;
    Ok(tag.to_owned())
}

fn install(version: &str, require_freshness: bool) -> Result<u8, String> {
    #[cfg(not(unix))]
    return Err("official PAM self-update currently supports Linux releases".to_owned());

    #[cfg(unix)]
    {
        let authorization = update_authorization(version, require_freshness)?;
        let path = temporary_installer()?;
        let mut command = Command::new(&path);
        command.arg(version).env(
            "PAM_EXPECTED_ARCHIVE_SHA256",
            &authorization.artifact_sha256,
        );
        let status = command
            .status()
            .map_err(|error| format!("cannot start the verified PAM installer: {error}"));
        let cleanup = fs::remove_file(&path);
        let status = match status {
            Ok(status) => status,
            Err(error) => {
                return match cleanup {
                    Ok(()) => Err(error),
                    Err(cleanup_error) => Err(format!(
                        "{error}; additionally cannot remove temporary installer: {cleanup_error}"
                    )),
                };
            }
        };
        if !status.success() {
            cleanup
                .map_err(|error| format!("cannot remove failed temporary installer: {error}"))?;
            return Err(format!("PAM installer failed with {status}"));
        }
        cleanup.map_err(|error| format!("cannot remove temporary installer: {error}"))?;
        println!("PAM update complete. The next command uses the new release.");
        Ok(0)
    }
}

#[cfg(unix)]
fn update_authorization(
    version: &str,
    require_freshness: bool,
) -> Result<crate::distribution::UpdateAuthorization, String> {
    let current_identity = UPDATE_SIGNING_IDENTITY
        .filter(|identity| identity.len() == 64)
        .ok_or_else(|| {
            "this PAM build has no pinned update-signing identity; install from a verified release"
                .to_owned()
        })?;
    let mut pinned_identities = vec![current_identity];
    if let Some(next_identity) =
        UPDATE_NEXT_SIGNING_IDENTITY.filter(|identity| !identity.is_empty())
    {
        pinned_identities.push(next_identity);
    }
    let (target, platform_code, architecture_code) = update_target()?;
    let repository =
        std::env::var("PAM_GITHUB_REPOSITORY").unwrap_or_else(|_| "push-in/pam".to_owned());
    let release_base = std::env::var("PAM_RELEASE_BASE_URL")
        .unwrap_or_else(|_| format!("https://github.com/{repository}/releases/download"));
    validate_https_url(&release_base, "release base")?;
    let manifest_name = format!("pam-{version}-{target}.update.json");
    let url = format!("{release_base}/{version}/{manifest_name}");
    validate_https_url(&url, "update manifest")?;
    let output = Command::new("curl")
        .args([
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--tlsv1.2",
            "--connect-timeout",
            RELEASE_CONNECT_TIMEOUT_SECONDS,
            "--max-time",
            RELEASE_MAX_TIME_SECONDS,
            "--max-filesize",
            &MAX_UPDATE_MANIFEST_BYTES.to_string(),
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            &url,
        ])
        .output()
        .map_err(|error| format!("cannot download signed PAM update manifest: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "signed update manifest download failed with {}",
            output.status
        ));
    }
    if output.stdout.len() > MAX_UPDATE_MANIFEST_BYTES {
        return Err("signed update manifest exceeds 256 KiB".to_owned());
    }
    let freshness_time_unix = require_freshness.then(current_unix_seconds).transpose()?;
    let authorization = crate::distribution::authorize_update_manifest_at(
        &output.stdout,
        &pinned_identities,
        version,
        platform_code,
        architecture_code,
        freshness_time_unix,
    )?;
    if authorization.artifact_bytes > MAX_INSTALL_ARCHIVE_BYTES {
        return Err("signed update archive exceeds the installer one-GiB limit".to_owned());
    }
    Ok(authorization)
}

fn current_unix_seconds() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| "system clock is before the Unix epoch".to_owned())
}

#[cfg(unix)]
fn update_target() -> Result<(&'static str, u8, u8), String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok(("linux-x86_64", 1, 1)),
        ("linux", "aarch64") => Ok(("linux-aarch64", 1, 2)),
        ("macos", "x86_64") => Ok(("macos-x86_64", 2, 1)),
        ("macos", "aarch64") => Ok(("macos-arm64", 2, 2)),
        _ => Err("official PAM self-update does not support this target".to_owned()),
    }
}

#[cfg(unix)]
struct PartialInstaller {
    path: Option<PathBuf>,
}

#[cfg(unix)]
impl PartialInstaller {
    fn new(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    fn persist(mut self) -> PathBuf {
        self.path
            .take()
            .expect("partial installer must own its path")
    }
}

#[cfg(unix)]
impl Drop for PartialInstaller {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = fs::remove_file(path);
        }
    }
}

#[cfg(unix)]
fn temporary_installer() -> Result<PathBuf, String> {
    for attempt in 0..32_u8 {
        let path = std::env::temp_dir().join(format!(
            "pam-self-update-{}-{attempt}.sh",
            std::process::id()
        ));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o700)
            .open(&path);
        let mut file = match file {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("cannot create temporary installer: {error}")),
        };
        let pending = PartialInstaller::new(path);
        file.write_all(INSTALLER.as_bytes())
            .map_err(|error| format!("cannot write temporary installer: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("cannot sync temporary installer: {error}"))?;
        fs::set_permissions(
            pending
                .path
                .as_deref()
                .expect("partial installer must own its path"),
            fs::Permissions::from_mode(0o700),
        )
        .map_err(|error| format!("cannot secure temporary installer: {error}"))?;
        return Ok(pending.persist());
    }
    Err("cannot allocate a unique temporary installer path".to_owned())
}

fn valid_release(value: &str) -> bool {
    let Some(value) = value.strip_prefix('v') else {
        return false;
    };
    Version::parse(value)
        .is_ok_and(|version| version.build.is_empty() && version.to_string() == value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    use sha2::{Digest, Sha256};

    #[test]
    fn release_tags_are_strict_semver() {
        assert!(valid_release("v1.0.0"));
        assert!(valid_release("v1.0.0-rc.1"));
        assert!(!valid_release("1.0.0"));
        assert!(!valid_release("v1.0"));
        assert!(!valid_release("vnext"));
        assert!(!valid_release("v1.0.0-"));
        assert!(!valid_release("v01.0.0"));
        assert!(!valid_release("v1.0.0-rc..1"));
        assert!(!valid_release("v1.0.0-01"));
        assert!(!valid_release("v1.0.0+local"));
    }

    #[test]
    fn self_update_rejects_replayed_versions_without_explicit_downgrade() {
        assert_eq!(
            update_transition(&format!("v{}", env!("CARGO_PKG_VERSION")), false).unwrap(),
            UpdateTransition::Current
        );
        assert_eq!(
            update_transition("v999.0.0", false).unwrap(),
            UpdateTransition::Upgrade
        );
        assert!(update_transition("v0.0.1", false).is_err());
        assert_eq!(
            update_transition("v0.0.1", true).unwrap(),
            UpdateTransition::ExplicitDowngrade
        );
        assert!(update_transition("v1.0.3-alpha.1", false).is_err());
        assert_eq!(
            update_transition("v1.0.3-alpha.1", true).unwrap(),
            UpdateTransition::ExplicitDowngrade
        );
        assert!(run([OsString::from("--allow-downgrade")].into_iter()).is_err());
        assert!(
            run([
                OsString::from("--check"),
                OsString::from("--allow-downgrade"),
                OsString::from("v0.0.1"),
            ]
            .into_iter())
            .is_err()
        );
        assert_eq!(
            check_latest(Some(&format!("v{}", env!("CARGO_PKG_VERSION")))).unwrap(),
            0
        );
        assert!(check_latest(Some("v0.0.1")).is_err());
    }

    #[test]
    fn release_metadata_is_bounded_and_requires_a_valid_tag() {
        assert_eq!(
            decode_latest_release(br#"{"tag_name":"v1.2.3"}"#).unwrap(),
            "v1.2.3"
        );
        assert!(decode_latest_release(br#"{"tag_name":"latest"}"#).is_err());
        assert!(decode_latest_release(&vec![b' '; MAX_RELEASE_METADATA_BYTES + 1]).is_err());
    }

    #[test]
    fn release_api_requires_a_bounded_https_url() {
        assert!(validate_release_api_url("https://api.github.com/releases/latest").is_ok());
        assert!(validate_release_api_url("http://localhost/releases/latest").is_err());
        assert!(validate_release_api_url("https://example.test/\nheader").is_err());
        assert!(
            validate_release_api_url(&format!("https://example.test/{}", "x".repeat(4096)))
                .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn embedded_installer_is_written_private_and_executable() {
        let path = temporary_installer().unwrap();
        let metadata = fs::metadata(&path).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
        assert_eq!(fs::read_to_string(&path).unwrap(), INSTALLER);
        assert!(INSTALLER.contains("--max-filesize \"${maximum_bytes}\""));
        assert!(INSTALLER.contains("--proto-redir '=https'"));
        assert!(INSTALLER.contains("--no-same-owner --no-same-permissions"));
        assert!(INSTALLER.contains("checksum_lines=$(awk 'END { print NR }'"));
        assert!(INSTALLER.contains("release checksum must contain exactly one SHA-256 entry"));
        assert!(INSTALLER.contains("release archive checksum mismatch"));
        assert!(INSTALLER.contains("PAM_EXPECTED_ARCHIVE_SHA256"));
        assert!(INSTALLER.contains("signed update authorization"));
        assert!(INSTALLER.contains("release_extracted_max_bytes=4294967296"));
        assert!(INSTALLER.contains("release_extracted_max_entries=100000"));
        assert!(INSTALLER.contains("release archive expands beyond four GiB"));
        assert!(INSTALLER.contains("release archive expands to too many entries"));
        assert!(INSTALLER.contains("ulimit -t 900"));
        assert!(INSTALLER.contains("ulimit -f 4194304"));
        assert!(INSTALLER.contains("release_retained_previous=2"));
        assert!(INSTALLER.contains("prune_old_releases"));
        assert!(INSTALLER.contains("probe_runtime_identity"));
        assert!(INSTALLER.contains("ulimit -t 5"));
        assert!(INSTALLER.contains("identity_bytes"));
        assert!(INSTALLER.contains("identity_watchdog"));
        assert!(INSTALLER.contains("-type f -links +1"));
        assert!(INSTALLER.contains("activation_link=\"${binary_link}.next.$$.tmp\""));
        assert!(INSTALLER.contains("mv -f \"${activation_link}\" \"${binary_link}\""));
        assert!(INSTALLER.contains("test ! -L \"${release_directory}\""));
        assert!(INSTALLER.contains("expected_identity=\"pam ${requested_version#v}\""));
        assert!(INSTALLER.contains("test \"${installed_identity}\" = \"${expected_identity}\""));
        assert!(INSTALLER.contains("new_release_directory=${release_directory}"));
        assert!(INSTALLER.contains("release_stage_candidate=\"${release_directory}.installing\""));
        assert!(INSTALLER.contains("release_stage=${release_stage_candidate}"));
        assert!(INSTALLER.contains("mv \"${candidate_directory}\" \"${release_directory}\""));
        fs::remove_file(path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn partial_installer_guard_removes_only_uncommitted_files() {
        let path = std::env::temp_dir().join(format!(
            "pam-partial-installer-guard-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);
        fs::write(&path, b"partial").unwrap();
        drop(PartialInstaller::new(path.clone()));
        assert!(!path.exists());

        fs::write(&path, b"complete").unwrap();
        let persisted = PartialInstaller::new(path.clone()).persist();
        assert_eq!(persisted, path);
        assert_eq!(fs::read(&path).unwrap(), b"complete");
        fs::remove_file(path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn embedded_installer_rejects_noncanonical_semver_before_download() {
        if installer_test_target().is_none() {
            return;
        }
        let installer = temporary_installer().unwrap();
        for invalid in ["v01.0.0", "v1.0.0-rc..1", "v1.0.0-01", "v1.0.0+local"] {
            let output = Command::new("sh")
                .arg(&installer)
                .arg(invalid)
                .output()
                .unwrap();
            assert!(!output.status.success(), "installer accepted {invalid}");
            assert!(String::from_utf8_lossy(&output.stderr).contains("must use SemVer"));
        }
        fs::remove_file(installer).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn installer_rolls_back_identity_failure_then_activates_atomically() {
        let Some(target) = installer_test_target() else {
            return;
        };
        let fixture = std::env::temp_dir().join(format!(
            "pam-installer-rollback-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&fixture);
        let mirror = fixture.join("mirror");
        let staging = fixture.join("staging");
        let bin = fixture.join("bin");
        let install = fixture.join("install");
        let tools = fixture.join("tools");
        fs::create_dir_all(&mirror).unwrap();
        fs::create_dir_all(&staging).unwrap();
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(&tools).unwrap();

        let version = "v1.2.3";
        let archive_root = format!("pam-{version}-{target}");
        let archive_name = format!("{archive_root}.tar.gz");
        write_installer_archive(
            &mirror,
            &staging,
            &archive_root,
            &archive_name,
            "9.9.9",
            None,
        );

        let fake_curl = tools.join("curl");
        fs::write(
            &fake_curl,
            "#!/bin/sh\ndestination=\nurl=\nwhile test \"$#\" -gt 0; do\n  case \"$1\" in\n    --output) destination=$2; shift 2 ;;\n    http://*|https://*) url=$1; shift ;;\n    *) shift ;;\n  esac\ndone\ncp \"${PAM_TEST_MIRROR}/${url##*/}\" \"${destination}\"\n",
        )
        .unwrap();
        fs::set_permissions(&fake_curl, fs::Permissions::from_mode(0o755)).unwrap();
        let old_runtime = fixture.join("old-pam");
        fs::write(&old_runtime, "#!/bin/sh\nprintf 'pam 1.0.0\\n'\n").unwrap();
        fs::set_permissions(&old_runtime, fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink(&old_runtime, bin.join("pam")).unwrap();
        let installer = temporary_installer().unwrap();
        let invoke_installer = |expected_digest: Option<&str>| {
            let mut command = Command::new("sh");
            command
                .arg(&installer)
                .arg(version)
                .env("PAM_RELEASE_BASE_URL", "http://mirror")
                .env("PAM_TEST_MIRROR", &mirror)
                .env("PAM_INSTALL_DIR", &install)
                .env("PAM_BIN_DIR", &bin)
                .env("PATH", format!("{}:/usr/bin:/bin", tools.display()));
            if let Some(expected_digest) = expected_digest {
                command.env("PAM_EXPECTED_ARCHIVE_SHA256", expected_digest);
            }
            command.output().unwrap()
        };

        let output = invoke_installer(None);

        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("runtime identity mismatch"));
        assert_eq!(fs::read_link(bin.join("pam")).unwrap(), old_runtime);
        assert!(!install.join(format!("{version}-{target}")).exists());
        assert!(
            !install
                .join(format!("{version}-{target}.installing"))
                .exists()
        );

        fs::write(
            mirror.join(format!("{archive_name}.sha256")),
            format!("{}  {archive_name}\n", "0".repeat(64)),
        )
        .unwrap();
        let output = invoke_installer(None);
        assert!(
            !output.status.success(),
            "installer accepted a wrong digest"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("archive checksum mismatch"));
        assert_eq!(fs::read_link(bin.join("pam")).unwrap(), old_runtime);
        assert!(!install.join(format!("{version}-{target}")).exists());

        for invalid_checksum in [
            format!("{}  other.tar.gz\n", "0".repeat(64)),
            format!(
                "{}  {archive_name}\n{}  extra\n",
                "0".repeat(64),
                "0".repeat(64)
            ),
            format!("{}  {archive_name}\n", "g".repeat(64)),
        ] {
            fs::write(
                mirror.join(format!("{archive_name}.sha256")),
                invalid_checksum,
            )
            .unwrap();
            let output = invoke_installer(None);
            assert!(
                !output.status.success(),
                "installer accepted an invalid checksum contract"
            );
            assert!(
                String::from_utf8_lossy(&output.stderr)
                    .contains("checksum must contain exactly one SHA-256 entry")
            );
            assert_eq!(fs::read_link(bin.join("pam")).unwrap(), old_runtime);
            assert!(!install.join(format!("{version}-{target}")).exists());
        }

        write_installer_archive(
            &mirror,
            &staging,
            &archive_root,
            &archive_name,
            "1.2.3",
            None,
        );
        let competing_stage = install.join(format!("{version}-{target}.installing"));
        fs::create_dir(&competing_stage).unwrap();
        fs::write(competing_stage.join("owner"), b"other installer").unwrap();
        let output = invoke_installer(None);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("another installation"));
        assert!(competing_stage.join("owner").is_file());
        assert_eq!(fs::read_link(bin.join("pam")).unwrap(), old_runtime);
        fs::remove_dir_all(&competing_stage).unwrap();

        for unsafe_entry in ["symlink", "hardlink"] {
            write_installer_archive(
                &mirror,
                &staging,
                &archive_root,
                &archive_name,
                "1.2.3",
                Some(unsafe_entry),
            );
            let output = invoke_installer(None);
            assert!(
                !output.status.success(),
                "installer accepted {unsafe_entry}"
            );
            assert!(String::from_utf8_lossy(&output.stderr).contains("unexpected"));
            assert_eq!(fs::read_link(bin.join("pam")).unwrap(), old_runtime);
            assert!(!install.join(format!("{version}-{target}")).exists());
            assert!(
                !install
                    .join(format!("{version}-{target}.installing"))
                    .exists()
            );
        }

        for hostile_identity in ["__noisy__", "__hang__"] {
            write_installer_archive(
                &mirror,
                &staging,
                &archive_root,
                &archive_name,
                hostile_identity,
                None,
            );
            let started = std::time::Instant::now();
            let output = invoke_installer(None);
            assert!(!output.status.success());
            assert!(started.elapsed() < std::time::Duration::from_secs(8));
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("did not report its identity")
            );
            assert_eq!(fs::read_link(bin.join("pam")).unwrap(), old_runtime);
            assert!(!install.join(format!("{version}-{target}")).exists());
            assert!(
                !install
                    .join(format!("{version}-{target}.installing"))
                    .exists()
            );
        }

        for (index, old_version) in ["v0.1.0", "v0.2.0", "v0.3.0", "v0.4.0"]
            .into_iter()
            .enumerate()
        {
            let old_release = install.join(format!("{old_version}-{target}"));
            fs::create_dir_all(old_release.join("bin")).unwrap();
            fs::write(old_release.join("bin/pam-run"), b"#!/bin/sh\nexit 0\n").unwrap();
            fs::set_permissions(
                old_release.join("bin/pam-run"),
                fs::Permissions::from_mode(0o755),
            )
            .unwrap();
            assert!(
                Command::new("touch")
                    .args(["-t", &format!("20260{}010101", index + 1)])
                    .arg(&old_release)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        fs::create_dir(install.join("notes")).unwrap();
        std::os::unix::fs::symlink(&old_runtime, install.join(format!("v0.0.1-{target}"))).unwrap();

        write_installer_archive(
            &mirror,
            &staging,
            &archive_root,
            &archive_name,
            "1.2.3",
            None,
        );
        let wrong_authorization = "0".repeat(64);
        let output = invoke_installer(Some(&wrong_authorization));
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("checksum does not match the signed update authorization")
        );
        assert_eq!(fs::read_link(bin.join("pam")).unwrap(), old_runtime);
        assert!(!install.join(format!("{version}-{target}")).exists());

        let checksum = fs::read_to_string(mirror.join(format!("{archive_name}.sha256"))).unwrap();
        let authorized_digest = checksum.split_whitespace().next().unwrap();
        let output = invoke_installer(Some(authorized_digest));

        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let release = install.join(format!("{version}-{target}"));
        assert_eq!(
            fs::read_link(bin.join("pam")).unwrap(),
            release.join("bin/pam-run")
        );
        assert!(release.join("bin/pam-run").is_file());
        assert_eq!(fs::read_dir(&bin).unwrap().count(), 1);
        assert!(!install.join(format!("v0.1.0-{target}")).exists());
        assert!(!install.join(format!("v0.2.0-{target}")).exists());
        assert!(install.join(format!("v0.3.0-{target}")).is_dir());
        assert!(install.join(format!("v0.4.0-{target}")).is_dir());
        assert!(install.join("notes").is_dir());
        assert!(install.join(format!("v0.0.1-{target}")).is_symlink());
        fs::remove_file(installer).unwrap();
        fs::remove_dir_all(fixture).unwrap();
    }

    #[cfg(unix)]
    fn installer_test_target() -> Option<&'static str> {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("linux", "x86_64") => Some("linux-x86_64"),
            ("linux", "aarch64") => Some("linux-aarch64"),
            ("macos", "x86_64") => Some("macos-x86_64"),
            ("macos", "aarch64") => Some("macos-arm64"),
            _ => None,
        }
    }

    #[cfg(unix)]
    fn write_installer_archive(
        mirror: &std::path::Path,
        staging: &std::path::Path,
        archive_root: &str,
        archive_name: &str,
        identity: &str,
        unsafe_entry: Option<&str>,
    ) {
        let root = staging.join(archive_root);
        let _ = fs::remove_dir_all(&root);
        let candidate = root.join("bin");
        fs::create_dir_all(&candidate).unwrap();
        let candidate_binary = candidate.join("pam-run");
        let script = match identity {
            "__noisy__" => "#!/bin/sh\nwhile :; do printf 'pam noisy output\\n'; done\n".to_owned(),
            "__hang__" => "#!/bin/sh\nexec sleep 30\n".to_owned(),
            identity => format!("#!/bin/sh\nprintf 'pam {identity}\\n'\n"),
        };
        fs::write(&candidate_binary, script).unwrap();
        fs::set_permissions(&candidate_binary, fs::Permissions::from_mode(0o755)).unwrap();
        match unsafe_entry {
            Some("symlink") => {
                std::os::unix::fs::symlink("bin/pam-run", root.join("unexpected-symlink")).unwrap();
            }
            Some("hardlink") => {
                fs::hard_link(&candidate_binary, root.join("unexpected-hardlink")).unwrap();
            }
            Some(other) => panic!("unsupported unsafe fixture {other}"),
            None => {}
        }
        let archive = mirror.join(archive_name);
        assert!(
            Command::new("tar")
                .args(["-C", staging.to_str().unwrap(), "-czf"])
                .arg(&archive)
                .arg(archive_root)
                .status()
                .unwrap()
                .success()
        );
        let digest = format!("{:x}", Sha256::digest(fs::read(&archive).unwrap()));
        fs::write(
            mirror.join(format!("{archive_name}.sha256")),
            format!("{digest}  {archive_name}\n"),
        )
        .unwrap();
    }
}
