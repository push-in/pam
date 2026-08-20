use std::ffi::{OsStr, OsString};
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::desktop_transaction::{
    publish_file_transactionally, temporary_sibling, write_file_transactionally,
};
use crate::plugin_registry::{self, VerifiedRelease};

const DESKTOP_BINARY_ENV: &str = "PAM_DESKTOP_BINARY";
const HOST_PACKAGE: &str = "pushinbr/pam-desktop-host";
const DESKTOP_PROTOCOL: u32 = 6;
const MAX_BINARY_BYTES: u64 = 512 * 1024 * 1024;
const DOWNLOAD_CONNECT_TIMEOUT_SECONDS: &str = "15";
const DOWNLOAD_MAX_TIME_SECONDS: &str = "600";
const IDENTITY_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_IDENTITY_BYTES: usize = 4 * 1024;
const PROVENANCE_PATH: &str = ".pam/desktop-host.artifact.json";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

#[derive(Clone, Copy)]
#[repr(u8)]
enum HostDoctorResultCode {
    Verified = 1,
    NeedsAttention = 2,
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum HostSourceCode {
    SignedRegistry = 1,
    ExplicitBinary = 2,
    SiblingBinary = 3,
    SearchPath = 4,
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum HostCheckCode {
    Provenance = 1,
    ArtifactDigest = 2,
    BinaryIdentity = 3,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HostDoctorCheck {
    check_code: u8,
    result_code: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HostDoctorReport {
    schema_version: u8,
    surface_code: u8,
    result_code: u8,
    source_code: u8,
    authenticated: bool,
    binary: String,
    checks: Vec<HostDoctorCheck>,
    remediation: String,
    verification_command: String,
}

pub fn run(
    pam_executable: &OsStr,
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<u8, String> {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    if arguments
        .first()
        .is_some_and(|argument| argument == "host:doctor")
    {
        return host_doctor(pam_executable, &arguments[1..]);
    }
    if arguments
        .first()
        .is_some_and(|argument| argument == "screenshot")
    {
        return Err(
            "PAM Desktop protocol 6 does not expose a screenshot command; capture the window with a platform driver, then run `pam desktop visual verify --name <case> --actual <project-relative.png>`"
                .to_owned(),
        );
    }
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

fn host_doctor(pam_executable: &OsStr, arguments: &[OsString]) -> Result<u8, String> {
    let mut project = PathBuf::from(".");
    let mut positional = false;
    let mut json = false;
    for argument in arguments {
        match argument.to_str() {
            Some("--json") => json = true,
            Some(value) if !value.starts_with('-') && !positional => {
                project = PathBuf::from(value);
                positional = true;
            }
            _ => {
                return Err("usage: pam desktop host:doctor [project] [--json]".to_owned());
            }
        }
    }
    let project = fs::canonicalize(&project).map_err(|error| {
        format!(
            "cannot resolve Desktop project {}: {error}",
            project.display()
        )
    })?;
    let pam_binary = absolute_executable(pam_executable)?;
    let report = if project
        .join(plugin_registry::PROJECT_REGISTRY_CONFIG)
        .is_file()
    {
        inspect_authenticated_host(&project)?
    } else {
        inspect_unverified_host(&pam_binary)
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .map_err(|error| format!("cannot encode Desktop host diagnostics: {error}"))?
        );
    } else {
        println!(
            "PAM Desktop host: {} (sourceCode {}, resultCode {})",
            report.binary, report.source_code, report.result_code
        );
        println!("Fix: {}", report.remediation);
        println!("Verify: {}", report.verification_command);
    }
    Ok(
        if report.result_code == HostDoctorResultCode::Verified as u8 {
            0
        } else {
            1
        },
    )
}

fn inspect_authenticated_host(project: &Path) -> Result<HostDoctorReport, String> {
    let release = plugin_registry::resolve_project_release(
        project,
        HOST_PACKAGE,
        3,
        None,
        Some(DESKTOP_PROTOCOL),
    )?
    .ok_or_else(|| "authenticated Desktop diagnosis requires pam-registry.json".to_owned())?;
    let binary = project
        .join(".pam/desktop-host")
        .join(&release.sha256)
        .join(desktop_binary_name());
    let results = authenticated_host_checks(project, &binary, &release);
    let authenticated = results.into_iter().all(|passed| passed);
    Ok(host_doctor_report(
        HostSourceCode::SignedRegistry,
        binary,
        authenticated,
        results,
    ))
}

fn authenticated_host_checks(
    project: &Path,
    binary: &Path,
    release: &VerifiedRelease,
) -> [bool; 3] {
    let expected_provenance = provenance_for(release);
    let provenance_ok =
        read_provenance(project).is_ok_and(|provenance| provenance == expected_provenance);
    let digest_ok = bounded_sha256(binary).is_ok_and(|digest| digest == release.sha256);
    let identity_ok = digest_ok && verify_identity(binary, &release.version).is_ok();
    [provenance_ok, digest_ok, identity_ok]
}

fn inspect_unverified_host(pam_binary: &Path) -> HostDoctorReport {
    let (source, binary) = if let Some(configured) = std::env::var_os(DESKTOP_BINARY_ENV)
        && !configured.is_empty()
    {
        (HostSourceCode::ExplicitBinary, PathBuf::from(configured))
    } else {
        let sibling = pam_binary.with_file_name(desktop_binary_name());
        if sibling.is_file() {
            (HostSourceCode::SiblingBinary, sibling)
        } else {
            (
                HostSourceCode::SearchPath,
                PathBuf::from(desktop_binary_name()),
            )
        }
    };
    let mut command = Command::new(&binary);
    command.arg("--version");
    let identity_ok = bounded_command_output(command, IDENTITY_TIMEOUT, "Desktop host identity")
        .is_ok_and(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .starts_with("pam-desktop ")
        });
    host_doctor_report(source, binary, false, [false, false, identity_ok])
}

fn host_doctor_report(
    source: HostSourceCode,
    binary: PathBuf,
    authenticated: bool,
    results: [bool; 3],
) -> HostDoctorReport {
    let result = if authenticated {
        HostDoctorResultCode::Verified
    } else {
        HostDoctorResultCode::NeedsAttention
    };
    let codes = [
        HostCheckCode::Provenance,
        HostCheckCode::ArtifactDigest,
        HostCheckCode::BinaryIdentity,
    ];
    HostDoctorReport {
        schema_version: 1,
        surface_code: 3,
        result_code: result as u8,
        source_code: source as u8,
        authenticated,
        binary: binary.display().to_string(),
        checks: codes
            .into_iter()
            .zip(results)
            .map(|(code, passed)| HostDoctorCheck {
                check_code: code as u8,
                result_code: if passed { 1 } else { 2 },
            })
            .collect(),
        remediation: if authenticated {
            "No action required; keep the signed registry and host provenance together".to_owned()
        } else {
            "Configure pam-registry.json and run `pam desktop dev` once to acquire the signed host"
                .to_owned()
        },
        verification_command: "pam desktop host:doctor . --json".to_owned(),
    }
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
    if verify_binary(&executable, &release).is_err() {
        install_binary(&executable, &release)?;
    }
    if read_provenance(project).ok().as_ref() != Some(&provenance) {
        write_provenance(project, &provenance)?;
    }
    plugin_registry::persist_project_state(project, &release)?;
    prune_store(&store, &release_directory)?;
    Ok(executable.into_os_string())
}

fn install_binary(destination: &Path, release: &VerifiedRelease) -> Result<(), String> {
    let temporary = temporary_sibling(destination, "download");
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
                "--proto-redir",
                "=https",
                "--tlsv1.2",
                "--connect-timeout",
                DOWNLOAD_CONNECT_TIMEOUT_SECONDS,
                "--max-time",
                DOWNLOAD_MAX_TIME_SECONDS,
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
        sync_verified_binary(&temporary)?;
        publish_verified_binary(&temporary, destination)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn publish_verified_binary(temporary: &Path, destination: &Path) -> Result<(), String> {
    publish_file_transactionally(temporary, destination, "Desktop host")
}

fn verify_binary(path: &Path, release: &VerifiedRelease) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect Desktop host: {error}"))?;
    validate_binary_metadata(&metadata)?;
    let actual = bounded_sha256(path)?;
    if actual != release.sha256 {
        return Err(format!(
            "signed Desktop host SHA-256 mismatch: expected {}, received {actual}",
            release.sha256
        ));
    }
    verify_identity(path, &release.version)
}

fn verify_identity(path: &Path, expected_version: &str) -> Result<(), String> {
    let mut command = Command::new(path);
    command.arg("--version");
    let output = bounded_command_output(command, IDENTITY_TIMEOUT, "Desktop host identity")?;
    let identity = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !output.status.success() || identity != format!("pam-desktop {expected_version}") {
        return Err(format!(
            "Desktop host identity mismatch: expected pam-desktop {expected_version}, received {identity:?}"
        ));
    }
    Ok(())
}

fn bounded_command_output(
    mut command: Command,
    timeout: Duration,
    label: &str,
) -> Result<Output, String> {
    isolate_command_process(&mut command);
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot inspect {label}: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("cannot capture {label} stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("cannot capture {label} stderr"))?;
    let output_exceeded = Arc::new(AtomicBool::new(false));
    let stdout_reader = spawn_bounded_reader(stdout, Arc::clone(&output_exceeded));
    let stderr_reader = spawn_bounded_reader(stderr, Arc::clone(&output_exceeded));
    let deadline = Instant::now() + timeout;
    loop {
        if output_exceeded.load(Ordering::Acquire) {
            terminate_command_process(&mut child);
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(format!("{label} output exceeds 4 KiB"));
        }
        match child
            .try_wait()
            .map_err(|error| format!("cannot wait for {label}: {error}"))?
        {
            Some(status) => {
                let stdout = join_bounded_reader(stdout_reader, label, "stdout")?;
                let stderr = join_bounded_reader(stderr_reader, label, "stderr")?;
                if output_exceeded.load(Ordering::Acquire) {
                    return Err(format!("{label} output exceeds 4 KiB"));
                }
                return Ok(Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            None if Instant::now() >= deadline => {
                terminate_command_process(&mut child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!(
                    "{label} exceeded its {} ms timeout",
                    timeout.as_millis()
                ));
            }
            None => thread::sleep(Duration::from_millis(10)),
        }
    }
}

#[cfg(unix)]
fn isolate_command_process(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn isolate_command_process(_command: &mut Command) {}

#[cfg(unix)]
fn terminate_command_process(child: &mut std::process::Child) {
    let process_group = -(child.id() as i32);
    // SAFETY: the child was placed in its own process group immediately before
    // spawning, and a negative PID targets only that group.
    unsafe {
        libc::kill(process_group, libc::SIGKILL);
    }
    let _ = child.wait();
}

#[cfg(not(unix))]
fn terminate_command_process(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn spawn_bounded_reader(
    mut input: impl Read + Send + 'static,
    exceeded: Arc<AtomicBool>,
) -> thread::JoinHandle<std::io::Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut output = Vec::with_capacity(MAX_IDENTITY_BYTES);
        let mut buffer = [0_u8; 1024];
        loop {
            let read = input.read(&mut buffer)?;
            if read == 0 {
                return Ok(output);
            }
            let remaining = MAX_IDENTITY_BYTES.saturating_sub(output.len());
            output.extend_from_slice(&buffer[..read.min(remaining)]);
            if read > remaining {
                exceeded.store(true, Ordering::Release);
            }
        }
    })
}

fn join_bounded_reader(
    reader: thread::JoinHandle<std::io::Result<Vec<u8>>>,
    label: &str,
    stream: &str,
) -> Result<Vec<u8>, String> {
    reader
        .join()
        .map_err(|_| format!("cannot read {label} {stream}: reader panicked"))?
        .map_err(|error| format!("cannot read {label} {stream}: {error}"))
}

fn bounded_sha256(path: &Path) -> Result<String, String> {
    let expected = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect Desktop host: {error}"))?;
    validate_binary_metadata(&expected)?;
    let mut options = OpenOptions::new();
    options.read(true);
    configure_safe_binary_open(&mut options);
    let mut input = options
        .open(path)
        .map_err(|error| format!("cannot read Desktop host: {error}"))?;
    let opened = input
        .metadata()
        .map_err(|error| format!("cannot inspect opened Desktop host: {error}"))?;
    validate_binary_metadata(&opened)?;
    if opened.len() != expected.len() || !same_binary_file(&expected, &opened) {
        return Err("Desktop host changed while it was opened".to_owned());
    }
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
    if total != opened.len() {
        return Err("Desktop host changed while it was hashed".to_owned());
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn validate_binary_metadata(metadata: &fs::Metadata) -> Result<(), String> {
    if !metadata.file_type().is_file() || metadata.len() == 0 || metadata.len() > MAX_BINARY_BYTES {
        return Err("Desktop host must be a bounded, non-empty regular file".to_owned());
    }
    Ok(())
}

#[cfg(unix)]
fn configure_safe_binary_open(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
}

#[cfg(not(unix))]
fn configure_safe_binary_open(_options: &mut OpenOptions) {}

#[cfg(unix)]
fn same_binary_file(expected: &fs::Metadata, opened: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    expected.dev() == opened.dev() && expected.ino() == opened.ino()
}

#[cfg(not(unix))]
fn same_binary_file(_expected: &fs::Metadata, _opened: &fs::Metadata) -> bool {
    true
}

fn sync_verified_binary(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect verified Desktop host: {error}"))?;
    validate_binary_metadata(&metadata).map_err(|_| {
        "verified Desktop host must be a bounded, non-empty regular file".to_owned()
    })?;
    fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("cannot persist verified Desktop host: {error}"))
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
    let mut bytes = serde_json::to_vec_pretty(provenance)
        .map_err(|error| format!("cannot encode Desktop host provenance: {error}"))?;
    bytes.push(b'\n');
    write_file_transactionally(&path, &bytes, "Desktop host provenance")
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

    #[cfg(unix)]
    #[test]
    fn bounded_hash_rejects_symlinks_and_named_pipes() {
        use std::os::unix::fs::symlink;

        let directory =
            std::env::temp_dir().join(format!("pam-desktop-hash-kind-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).expect("fixture directory");
        let regular = directory.join("regular");
        let link = directory.join("link");
        let pipe = directory.join("pipe");
        fs::write(&regular, b"desktop-host").expect("regular fixture");
        symlink(&regular, &link).expect("symlink fixture");
        assert!(
            Command::new("mkfifo")
                .arg(&pipe)
                .status()
                .expect("mkfifo")
                .success()
        );

        assert!(bounded_sha256(&link).is_err());
        assert!(bounded_sha256(&pipe).is_err());
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn persists_only_bounded_regular_desktop_hosts_before_activation() {
        let directory =
            std::env::temp_dir().join(format!("pam-desktop-sync-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).expect("fixture directory");
        let binary = directory.join(desktop_binary_name());
        fs::write(&binary, b"verified-host").expect("fixture binary");

        sync_verified_binary(&binary).expect("durable binary");
        assert!(sync_verified_binary(&directory).is_err());

        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn desktop_host_doctor_codes_are_sequential_and_fail_closed() {
        assert_eq!(HostDoctorResultCode::Verified as u8, 1);
        assert_eq!(HostDoctorResultCode::NeedsAttention as u8, 2);
        assert_eq!(HostSourceCode::SignedRegistry as u8, 1);
        assert_eq!(HostSourceCode::ExplicitBinary as u8, 2);
        assert_eq!(HostSourceCode::SiblingBinary as u8, 3);
        assert_eq!(HostSourceCode::SearchPath as u8, 4);
        assert_eq!(HostCheckCode::Provenance as u8, 1);
        assert_eq!(HostCheckCode::ArtifactDigest as u8, 2);
        assert_eq!(HostCheckCode::BinaryIdentity as u8, 3);

        let report = host_doctor_report(
            HostSourceCode::ExplicitBinary,
            PathBuf::from("pam-desktop"),
            false,
            [false, false, true],
        );
        assert_eq!(report.result_code, 2);
        assert!(!report.authenticated);
        assert_eq!(report.checks[2].result_code, 1);
    }

    #[test]
    fn publishes_a_verified_desktop_host_transactionally() {
        let directory =
            std::env::temp_dir().join(format!("pam-desktop-publish-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).expect("fixture directory");
        let destination = directory.join(desktop_binary_name());
        let temporary = destination.with_extension("verified");
        fs::write(&destination, b"previous").expect("previous host");
        fs::write(&temporary, b"verified").expect("verified host");

        publish_verified_binary(&temporary, &destination).expect("publish");

        assert_eq!(fs::read(&destination).expect("active host"), b"verified");
        assert!(!temporary.exists());
        assert!(!destination.with_extension("previous").exists());
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn restores_the_previous_desktop_host_when_activation_fails() {
        let directory =
            std::env::temp_dir().join(format!("pam-desktop-rollback-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).expect("fixture directory");
        let destination = directory.join(desktop_binary_name());
        let missing_temporary = destination.with_extension("missing");
        fs::write(&destination, b"previous").expect("previous host");

        let error = publish_verified_binary(&missing_temporary, &destination)
            .expect_err("missing verified host must fail");

        assert!(error.contains("cannot publish Desktop host"));
        assert_eq!(fs::read(&destination).expect("restored host"), b"previous");
        assert!(!destination.with_extension("previous").exists());
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn recovers_an_interrupted_desktop_host_activation() {
        let directory = std::env::temp_dir().join(format!(
            "pam-desktop-interrupted-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).expect("fixture directory");
        let destination = directory.join(desktop_binary_name());
        let backup = destination.with_extension("previous");
        let missing_temporary = destination.with_extension("missing");
        fs::write(&backup, b"previous").expect("interrupted backup");

        let error = publish_verified_binary(&missing_temporary, &destination)
            .expect_err("missing verified host must fail");

        assert!(error.contains("cannot publish Desktop host"));
        assert_eq!(fs::read(&destination).expect("restored host"), b"previous");
        assert!(!backup.exists());
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn replaces_desktop_host_provenance_transactionally() {
        let directory = std::env::temp_dir().join(format!(
            "pam-desktop-provenance-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(directory.join(".pam")).expect("fixture directory");
        let first = Provenance {
            schema_version: 1,
            registry: "https://registry.example".to_owned(),
            root_sha256: "ab".repeat(32),
            root_generation: 1,
            catalog_sequence: 1,
            package: HOST_PACKAGE.to_owned(),
            version: "1.0.0".to_owned(),
            artifact_sha256: "cd".repeat(32),
        };
        let second = Provenance {
            catalog_sequence: 2,
            version: "1.0.1".to_owned(),
            ..first.clone()
        };

        write_provenance(&directory, &first).expect("first provenance");
        write_provenance(&directory, &second).expect("replacement provenance");

        assert_eq!(
            read_provenance(&directory).expect("published provenance"),
            second
        );
        assert!(
            !directory
                .join(PROVENANCE_PATH)
                .with_extension("previous")
                .exists()
        );
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn allocates_distinct_desktop_host_temporary_siblings() {
        let destination = Path::new("pam-desktop");
        let first = temporary_sibling(destination, "download");
        let second = temporary_sibling(destination, "download");
        assert_ne!(first, second);
        assert_eq!(first.parent(), destination.parent());
    }

    #[cfg(unix)]
    #[test]
    fn authenticated_doctor_never_executes_a_digest_mismatch() {
        use std::os::unix::fs::PermissionsExt;

        let directory = std::env::temp_dir().join(format!(
            "pam-desktop-doctor-tamper-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(directory.join(".pam")).expect("fixture directory");
        let binary = directory.join(desktop_binary_name());
        let marker = PathBuf::from(format!("{}.executed", binary.display()));
        fs::write(
            &binary,
            b"#!/bin/sh\ntouch \"$0.executed\"\nprintf 'pam-desktop 1.2.3\\n'\n",
        )
        .expect("fixture binary");
        let mut permissions = fs::metadata(&binary).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&binary, permissions).expect("executable fixture");
        let release = VerifiedRelease {
            registry: "https://registry.example".to_owned(),
            root_sha256: "ab".repeat(32),
            root_generation: 1,
            catalog_sequence: 1,
            package: HOST_PACKAGE.to_owned(),
            version: "1.2.3".to_owned(),
            artifact_kind_code: 3,
            artifact_url: "https://registry.example/pam-desktop".to_owned(),
            sha256: "00".repeat(32),
        };

        let results = authenticated_host_checks(&directory, &binary, &release);

        assert_eq!(results, [false, false, false]);
        assert!(!marker.exists(), "untrusted host was executed");
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn desktop_identity_probe_times_out() {
        let mut command = Command::new("sleep");
        command.arg("1");
        let error =
            bounded_command_output(command, Duration::from_millis(20), "Desktop host identity")
                .expect_err("sleeping identity probe must time out");

        assert!(error.contains("20 ms timeout"));
    }

    #[cfg(unix)]
    #[test]
    fn desktop_identity_probe_rejects_unbounded_output() {
        let mut command = Command::new("printf");
        command.arg("x".repeat(MAX_IDENTITY_BYTES + 1));
        let error =
            bounded_command_output(command, Duration::from_secs(1), "Desktop host identity")
                .expect_err("oversized identity output must fail");

        assert!(error.contains("output exceeds 4 KiB"));
    }

    #[cfg(unix)]
    #[test]
    fn desktop_identity_probe_stops_noisy_processes_before_the_deadline() {
        let mut command = Command::new("sh");
        command.args(["-c", "printf '%05000d' 0; sleep 2"]);
        let started = Instant::now();
        let error =
            bounded_command_output(command, Duration::from_secs(5), "Desktop host identity")
                .expect_err("noisy identity probe must fail immediately");

        assert!(error.contains("output exceeds 4 KiB"));
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
