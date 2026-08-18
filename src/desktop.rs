use std::ffi::{OsStr, OsString};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::plugin_registry::{self, VerifiedRelease};

const DESKTOP_BINARY_ENV: &str = "PAM_DESKTOP_BINARY";
const HOST_PACKAGE: &str = "pushinbr/pam-desktop-host";
const DESKTOP_PROTOCOL: u32 = 6;
const MAX_BINARY_BYTES: u64 = 512 * 1024 * 1024;
const PROVENANCE_PATH: &str = ".pam/desktop-host.artifact.json";

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Provenance {
    schema_version: u8,
    registry: String,
    root_sha256: String,
    root_generation: u32,
    catalog_sequence: u64,
    package: String,
    version: String,
    artifact_sha256: String,
}

pub fn run(
    pam_executable: &OsStr,
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<u8, String> {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    let pam_binary = absolute_executable(pam_executable)?;
    let desktop_binary = match registry_project(&arguments)? {
        Some(project) => authenticated_executable(&project)?,
        None => desktop_executable(&pam_binary),
    };
    let status = Command::new(&desktop_binary)
        .args(&arguments)
        .env("PAM_BINARY", &pam_binary)
        .status()
        .map_err(|error| {
            format!(
                "cannot start {}: {error}. Install pam-desktop or set {DESKTOP_BINARY_ENV}",
                Path::new(&desktop_binary).display(),
            )
        })?;
    Ok(status
        .code()
        .and_then(|code| u8::try_from(code).ok())
        .unwrap_or(1))
}

fn registry_project(arguments: &[OsString]) -> Result<Option<PathBuf>, String> {
    for argument in arguments.iter().skip(1) {
        let candidate = Path::new(argument);
        if candidate.is_dir() {
            let candidate = fs::canonicalize(candidate)
                .map_err(|error| format!("cannot resolve {}: {error}", candidate.display()))?;
            if candidate
                .join(plugin_registry::PROJECT_REGISTRY_CONFIG)
                .is_file()
            {
                return Ok(Some(candidate));
            }
        }
    }
    let mut candidate = std::env::current_dir()
        .map_err(|error| format!("cannot resolve current directory: {error}"))?;
    loop {
        if candidate
            .join(plugin_registry::PROJECT_REGISTRY_CONFIG)
            .is_file()
        {
            return Ok(Some(candidate));
        }
        if !candidate.pop() {
            return Ok(None);
        }
    }
}

fn authenticated_executable(project: &Path) -> Result<OsString, String> {
    let release = plugin_registry::resolve_project_release(
        project,
        HOST_PACKAGE,
        3,
        None,
        Some(DESKTOP_PROTOCOL),
    )?
    .ok_or_else(|| "authenticated Desktop resolution requires pam-registry.json".to_owned())?;
    if release.artifact_kind_code != 3 {
        return Err("PAM Desktop host requires signed artifactKindCode 3".to_owned());
    }
    let state = project.join(".pam");
    ensure_directory(&state)?;
    let store = state.join("desktop-host");
    ensure_directory(&store)?;
    let release_directory = store.join(&release.sha256);
    ensure_directory(&release_directory)?;
    let executable = release_directory.join(desktop_binary_name());
    let provenance = provenance_for(&release);
    let reusable = read_provenance(project).ok().as_ref() == Some(&provenance)
        && verify_binary(&executable, &release).is_ok();
    if !reusable {
        install_binary(&executable, &release)?;
        write_provenance(project, &provenance)?;
    }
    plugin_registry::persist_project_state(project, &release)?;
    prune_store(&store, &release_directory)?;
    Ok(executable.into_os_string())
}

fn install_binary(destination: &Path, release: &VerifiedRelease) -> Result<(), String> {
    if destination.exists() {
        fs::remove_file(destination)
            .map_err(|error| format!("cannot replace invalid Desktop host: {error}"))?;
    }
    let temporary = destination.with_extension(format!("tmp-{}", std::process::id()));
    drop(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("cannot allocate Desktop host download: {error}"))?,
    );
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
                &MAX_BINARY_BYTES.to_string(),
                "--output",
            ])
            .arg(&temporary)
            .arg(&release.artifact_url)
            .status()
            .map_err(|error| format!("cannot download signed Desktop host: {error}"))?;
        if !status.success() {
            return Err(format!("signed Desktop host download failed with {status}"));
        }
        set_executable(&temporary)?;
        verify_binary(&temporary, release)?;
        fs::rename(&temporary, destination)
            .map_err(|error| format!("cannot publish verified Desktop host: {error}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn verify_binary(path: &Path, release: &VerifiedRelease) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect Desktop host: {error}"))?;
    if !metadata.file_type().is_file() || metadata.len() == 0 || metadata.len() > MAX_BINARY_BYTES {
        return Err("Desktop host must be a bounded, non-empty regular file".to_owned());
    }
    let actual = bounded_sha256(path)?;
    if actual != release.sha256 {
        return Err(format!(
            "signed Desktop host SHA-256 mismatch: expected {}, received {actual}",
            release.sha256
        ));
    }
    let output = Command::new(path)
        .arg("--version")
        .output()
        .map_err(|error| format!("cannot inspect Desktop host identity: {error}"))?;
    let identity = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !output.status.success() || identity != format!("pam-desktop {}", release.version) {
        return Err(format!(
            "Desktop host identity mismatch: expected pam-desktop {}, received {identity:?}",
            release.version
        ));
    }
    Ok(())
}

fn bounded_sha256(path: &Path) -> Result<String, String> {
    let mut input =
        fs::File::open(path).map_err(|error| format!("cannot read Desktop host: {error}"))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|error| format!("cannot hash Desktop host: {error}"))?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > MAX_BINARY_BYTES {
            return Err("Desktop host exceeds the 512 MiB limit".to_owned());
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn provenance_for(release: &VerifiedRelease) -> Provenance {
    Provenance {
        schema_version: 1,
        registry: release.registry.clone(),
        root_sha256: release.root_sha256.clone(),
        root_generation: release.root_generation,
        catalog_sequence: release.catalog_sequence,
        package: release.package.clone(),
        version: release.version.clone(),
        artifact_sha256: release.sha256.clone(),
    }
}

fn read_provenance(project: &Path) -> Result<Provenance, String> {
    let path = project.join(PROVENANCE_PATH);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| format!("cannot inspect Desktop host provenance: {error}"))?;
    if !metadata.file_type().is_file() || metadata.len() > 16 * 1024 {
        return Err("Desktop host provenance is not a bounded regular file".to_owned());
    }
    let value: Provenance = serde_json::from_slice(
        &fs::read(path).map_err(|error| format!("cannot read Desktop host provenance: {error}"))?,
    )
    .map_err(|error| format!("invalid Desktop host provenance: {error}"))?;
    (value.schema_version == 1)
        .then_some(value)
        .ok_or_else(|| "unsupported Desktop host provenance schema".to_owned())
}

fn write_provenance(project: &Path, provenance: &Provenance) -> Result<(), String> {
    let path = project.join(PROVENANCE_PATH);
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let bytes = serde_json::to_vec_pretty(provenance)
        .map_err(|error| format!("cannot encode Desktop host provenance: {error}"))?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("cannot allocate Desktop host provenance: {error}"))?;
    let result = output
        .write_all(&bytes)
        .and_then(|()| output.write_all(b"\n"))
        .and_then(|()| output.sync_all())
        .and_then(|()| fs::rename(&temporary, &path));
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(format!("cannot persist Desktop host provenance: {error}"));
    }
    Ok(())
}

fn prune_store(store: &Path, keep: &Path) -> Result<(), String> {
    for entry in fs::read_dir(store)
        .map_err(|error| format!("cannot inspect Desktop host store: {error}"))?
    {
        let entry = entry.map_err(|error| format!("cannot inspect Desktop host store: {error}"))?;
        let path = entry.path();
        if path == keep {
            continue;
        }
        let name = entry.file_name();
        let valid = name.len() == 64
            && name
                .to_string_lossy()
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase());
        if !valid || !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            return Err(format!(
                "refusing unexpected entry in Desktop host store: {}",
                path.display()
            ));
        }
        fs::remove_dir_all(&path)
            .map_err(|error| format!("cannot prune {}: {error}", path.display()))?;
    }
    Ok(())
}

fn ensure_directory(path: &Path) -> Result<(), String> {
    if path.exists() {
        return fs::symlink_metadata(path)
            .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?
            .file_type()
            .is_dir()
            .then_some(())
            .ok_or_else(|| format!("{} is not a real directory", path.display()));
    }
    fs::create_dir(path).map_err(|error| format!("cannot create {}: {error}", path.display()))
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|error| format!("cannot make Desktop host executable: {error}"))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn absolute_executable(executable: &OsStr) -> Result<PathBuf, String> {
    std::env::current_exe()
        .or_else(|_| {
            let executable = Path::new(executable);
            if executable.is_absolute() {
                Ok(executable.to_path_buf())
            } else {
                std::env::current_dir().map(|directory| directory.join(executable))
            }
        })
        .map_err(|error| format!("cannot resolve the Pam executable: {error}"))
}

fn desktop_executable(pam_binary: &Path) -> OsString {
    if let Some(configured) = std::env::var_os(DESKTOP_BINARY_ENV)
        && !configured.is_empty()
    {
        return configured;
    }
    let sibling = pam_binary.with_file_name(desktop_binary_name());
    if sibling.is_file() {
        return sibling.into_os_string();
    }
    OsString::from(desktop_binary_name())
}

fn desktop_binary_name() -> &'static str {
    if cfg!(windows) {
        "pam-desktop.exe"
    } else {
        "pam-desktop"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_captures_the_complete_trust_identity() {
        let release = VerifiedRelease {
            registry: "https://registry.example".to_owned(),
            root_sha256: "ab".repeat(32),
            root_generation: 2,
            catalog_sequence: 9,
            package: HOST_PACKAGE.to_owned(),
            version: "1.2.3".to_owned(),
            artifact_kind_code: 3,
            artifact_url: "https://registry.example/pam-desktop".to_owned(),
            sha256: "cd".repeat(32),
        };
        let provenance = provenance_for(&release);
        assert_eq!(provenance.catalog_sequence, 9);
        assert_eq!(provenance.artifact_sha256, "cd".repeat(32));
        assert_eq!(provenance.package, HOST_PACKAGE);
    }

    #[test]
    fn bounded_hash_uses_the_exact_file_bytes() {
        let path =
            std::env::temp_dir().join(format!("pam-desktop-hash-test-{}", std::process::id()));
        fs::write(&path, b"desktop-host").expect("fixture");
        assert_eq!(
            bounded_sha256(&path).expect("hash"),
            format!("{:x}", Sha256::digest(b"desktop-host"))
        );
        fs::remove_file(path).expect("cleanup");
    }
}
