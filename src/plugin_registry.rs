use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use ed25519_dalek::{Signature, VerifyingKey};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const SCHEMA_VERSION: u8 = 1;
const MAX_DOCUMENT_BYTES: u64 = 1024 * 1024;
const MAX_KEYS: usize = 32;
const MAX_SIGNATURES: usize = 64;
const MAX_PLUGINS: usize = 10_000;
const MAX_REVOCATIONS: usize = 10_000;
const CLOCK_SKEW_SECONDS: u64 = 300;
const ROOT_MAX_VALIDITY_SECONDS: u64 = 366 * 24 * 60 * 60;
const CATALOG_MAX_VALIDITY_SECONDS: u64 = 7 * 24 * 60 * 60;
const COMPATIBILITY_MATRIX_SCHEMA_URL: &str =
    "https://push-in.github.io/pam-docs/schemas/registry-compatibility-matrix.schema.json";
pub(crate) const PROJECT_REGISTRY_CONFIG: &str = "pam-registry.json";
pub(crate) const PROJECT_REGISTRY_STATE: &str = ".pam/plugin-registry-state.json";
const PROJECT_ROTATION_RECEIPT: &str = ".pam/plugin-registry-rotation.json";
const MAX_PROJECT_REGISTRY_BYTES: u64 = 16 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProjectRegistryConfig {
    pub schema_version: u8,
    pub root_path: PathBuf,
    pub root_sha256: String,
    pub catalog_path: PathBuf,
    #[serde(default)]
    pub native_protocol: Option<u32>,
    #[serde(default)]
    pub desktop_protocol: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProjectRegistryState {
    pub schema_version: u8,
    pub registry: String,
    pub root_sha256: String,
    pub root_generation: u32,
    pub catalog_sequence: u64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectRotationReceipt {
    schema_version: u8,
    operation_code: u8,
    previous_root_sha256: String,
    previous_state: Option<ProjectRegistryState>,
    next_config: ProjectRegistryConfig,
    next_state: ProjectRegistryState,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RotationAdoptionReport<'a> {
    schema_version: u8,
    result_code: u8,
    registry: &'a str,
    previous_root_generation: u32,
    root_generation: u32,
    catalog_sequence: u64,
    root_sha256: &'a str,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegistryRoot {
    schema_version: u8,
    registry: String,
    generation: u32,
    issued_at_unix: u64,
    expires_at_unix: u64,
    threshold: u8,
    keys: Vec<RegistryKey>,
    signatures: Vec<DocumentSignature>,
    #[serde(default)]
    previous_signatures: Vec<DocumentSignature>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegistryKey {
    key_id: String,
    public_key: String,
    state_code: u8,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DocumentSignature {
    key_id: String,
    signature: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RootPayload<'a> {
    schema_version: u8,
    registry: &'a str,
    generation: u32,
    issued_at_unix: u64,
    expires_at_unix: u64,
    threshold: u8,
    keys: &'a [RegistryKey],
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginCatalog {
    schema_version: u8,
    registry: String,
    root_generation: u32,
    sequence: u64,
    generated_at_unix: u64,
    expires_at_unix: u64,
    plugins: Vec<PluginRelease>,
    #[serde(default)]
    revocations: Vec<PluginRevocation>,
    signatures: Vec<DocumentSignature>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginRelease {
    package: String,
    version: String,
    artifact_kind_code: u8,
    artifact_url: String,
    sha256: String,
    published_at_unix: u64,
    surface_codes: Vec<u8>,
    compatibility: PluginCompatibility,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginCompatibility {
    pam: String,
    #[serde(default)]
    native_protocol: Option<u32>,
    #[serde(default)]
    desktop_protocol: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginRevocation {
    package: String,
    version: String,
    reason_code: u8,
    revoked_at_unix: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogPayload<'a> {
    schema_version: u8,
    registry: &'a str,
    root_generation: u32,
    sequence: u64,
    generated_at_unix: u64,
    expires_at_unix: u64,
    plugins: &'a [PluginRelease],
    revocations: &'a [PluginRevocation],
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VerificationReport<'a> {
    schema_version: u8,
    result_code: u8,
    registry: &'a str,
    root_generation: u32,
    catalog_sequence: Option<u64>,
    plugin_releases: usize,
    revocations: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResolutionReport<'a> {
    schema_version: u8,
    result_code: u8,
    registry: &'a str,
    catalog_sequence: u64,
    package: &'a str,
    version: &'a str,
    artifact_kind_code: u8,
    artifact_url: &'a str,
    sha256: &'a str,
}

#[repr(u8)]
enum CompatibilityResult {
    Compatible = 1,
    PamVersionMismatch = 2,
    ProtocolMismatch = 3,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompatibilityMatrixEntry<'a> {
    package: &'a str,
    version: &'a str,
    surface_code: u8,
    result_code: u8,
    artifact_kind_code: u8,
    sha256: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SurfaceCompatibilitySummary {
    surface_code: u8,
    total_entries: usize,
    compatible_entries: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompatibilityMatrixReport<'a> {
    #[serde(rename = "$schema")]
    schema_url: &'static str,
    schema_version: u8,
    result_code: u8,
    registry: &'a str,
    root_sha256: &'a str,
    root_generation: u32,
    catalog_sequence: u64,
    generated_at_unix: u64,
    expires_at_unix: u64,
    pam_version: String,
    native_protocol: u32,
    desktop_protocol: u32,
    compatible_entries: usize,
    surface_summaries: Vec<SurfaceCompatibilitySummary>,
    entries: Vec<CompatibilityMatrixEntry<'a>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VerifiedRelease {
    pub registry: String,
    pub root_sha256: String,
    pub root_generation: u32,
    pub catalog_sequence: u64,
    pub package: String,
    pub version: String,
    pub artifact_kind_code: u8,
    pub artifact_url: String,
    pub sha256: String,
}

pub(crate) fn load_project_config(project: &Path) -> Result<Option<ProjectRegistryConfig>, String> {
    recover_project_rotation(project)?;
    let path = project.join(PROJECT_REGISTRY_CONFIG);
    if !path.exists() {
        return Ok(None);
    }
    let config: ProjectRegistryConfig = read_small_document(&path, "registry configuration")?;
    if config.schema_version != SCHEMA_VERSION {
        return Err("unsupported pam-registry.json schema; expected integer 1".to_owned());
    }
    Ok(Some(config))
}

pub(crate) fn load_project_state(project: &Path) -> Result<Option<ProjectRegistryState>, String> {
    recover_project_rotation(project)?;
    let path = project.join(PROJECT_REGISTRY_STATE);
    if !path.exists() {
        return Ok(None);
    }
    let state: ProjectRegistryState = read_small_document(&path, "registry state")?;
    if state.schema_version != SCHEMA_VERSION
        || state.registry.is_empty()
        || state.root_generation == 0
        || state.catalog_sequence == 0
    {
        return Err("invalid project plugin-registry state".to_owned());
    }
    validate_lower_hex(&state.root_sha256, 32, "project root SHA-256")?;
    Ok(Some(state))
}

pub(crate) fn persist_project_state(
    project: &Path,
    release: &VerifiedRelease,
) -> Result<(), String> {
    write_project_state(
        project,
        &ProjectRegistryState {
            schema_version: SCHEMA_VERSION,
            registry: release.registry.clone(),
            root_sha256: release.root_sha256.clone(),
            root_generation: release.root_generation,
            catalog_sequence: release.catalog_sequence,
        },
    )
}

pub(crate) fn resolve_project_release(
    project: &Path,
    package: &str,
    surface_code: u8,
    native_protocol: Option<u32>,
    desktop_protocol: Option<u32>,
) -> Result<Option<VerifiedRelease>, String> {
    resolve_project_release_at(
        project,
        package,
        surface_code,
        native_protocol,
        desktop_protocol,
        None,
    )
}

fn resolve_project_release_at(
    project: &Path,
    package: &str,
    surface_code: u8,
    native_protocol: Option<u32>,
    desktop_protocol: Option<u32>,
    at_unix: Option<u64>,
) -> Result<Option<VerifiedRelease>, String> {
    let Some(config) = load_project_config(project)? else {
        return Ok(None);
    };
    if surface_code == 2 && config.native_protocol != native_protocol {
        return Err(format!(
            "pam-registry.json nativeProtocol {:?} does not match runtime protocol {:?}",
            config.native_protocol, native_protocol
        ));
    }
    if surface_code == 3 && config.desktop_protocol != desktop_protocol {
        return Err(format!(
            "pam-registry.json desktopProtocol {:?} does not match runtime protocol {:?}",
            config.desktop_protocol, desktop_protocol
        ));
    }
    let state = load_project_state(project)?;
    let root = project_document_path(project, &config.root_path, "rootPath")?;
    let catalog = project_document_path(project, &config.catalog_path, "catalogPath")?;
    let pam_version = Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|error| format!("invalid PAM package version: {error}"))?;
    let release = resolve_verified(
        &root,
        &config.root_sha256,
        &catalog,
        package,
        surface_code,
        &pam_version,
        native_protocol,
        desktop_protocol,
        state.as_ref().map(|value| value.catalog_sequence),
        at_unix,
    )?;
    if let Some(state) = state {
        if state.registry != release.registry {
            return Err(
                "signed registry identity does not match the accepted project state".to_owned(),
            );
        }
        if state.root_sha256 != release.root_sha256 {
            return Err(
                "trusted registry root changed without an authenticated rotation".to_owned(),
            );
        }
        if release.root_generation < state.root_generation {
            return Err("signed registry root generation would roll the project back".to_owned());
        }
    }
    Ok(Some(release))
}

fn write_project_state(project: &Path, state: &ProjectRegistryState) -> Result<(), String> {
    let path = project.join(PROJECT_REGISTRY_STATE);
    let parent = path.parent().expect("state path has parent");
    ensure_real_directory(parent)?;
    write_json_atomic(&path, state, "registry state")
}

fn recover_project_rotation(project: &Path) -> Result<(), String> {
    let receipt_path = project.join(PROJECT_ROTATION_RECEIPT);
    if !receipt_path.exists() {
        return Ok(());
    }
    let receipt: ProjectRotationReceipt =
        read_small_document(&receipt_path, "registry rotation receipt")?;
    if receipt.schema_version != SCHEMA_VERSION || receipt.operation_code != 1 {
        return Err("invalid project registry rotation receipt".to_owned());
    }
    let config_path = project.join(PROJECT_REGISTRY_CONFIG);
    let config: ProjectRegistryConfig =
        read_small_document(&config_path, "registry configuration")?;
    if config.root_sha256 == receipt.previous_root_sha256 {
        if let Some(previous_state) = &receipt.previous_state {
            write_project_state(project, previous_state)?;
        } else {
            let state_path = project.join(PROJECT_REGISTRY_STATE);
            if state_path.exists() {
                let metadata = fs::symlink_metadata(&state_path)
                    .map_err(|error| format!("cannot inspect {}: {error}", state_path.display()))?;
                if !metadata.file_type().is_file() {
                    return Err("registry state recovery target is not a regular file".to_owned());
                }
                fs::remove_file(&state_path)
                    .map_err(|error| format!("cannot restore absent registry state: {error}"))?;
            }
        }
        fs::remove_file(&receipt_path).map_err(|error| {
            format!(
                "cannot discard uncommitted registry rotation {}: {error}",
                receipt_path.display()
            )
        })?;
        return Ok(());
    }
    if config != receipt.next_config {
        return Err("registry rotation receipt does not match project configuration".to_owned());
    }
    write_project_state(project, &receipt.next_state)?;
    fs::remove_file(&receipt_path).map_err(|error| {
        format!(
            "cannot finalize registry rotation {}: {error}",
            receipt_path.display()
        )
    })
}

pub(crate) fn project_document_path(
    project: &Path,
    relative: &Path,
    field: &str,
) -> Result<PathBuf, String> {
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!(
            "pam-registry.json {field} must be a normalized project-relative path"
        ));
    }
    Ok(project.join(relative))
}

fn read_small_document<T: for<'de> Deserialize<'de>>(
    path: &Path,
    label: &str,
) -> Result<T, String> {
    serde_json::from_slice(&read_bounded_with_limit(
        path,
        label,
        MAX_PROJECT_REGISTRY_BYTES,
    )?)
    .map_err(|error| format!("invalid {}: {error}", path.display()))
}

fn ensure_real_directory(path: &Path) -> Result<(), String> {
    if path.exists() {
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
        if metadata.file_type().is_dir() {
            return Ok(());
        }
        return Err(format!("{} is not a real directory", path.display()));
    }
    fs::create_dir(path).map_err(|error| format!("cannot create {}: {error}", path.display()))
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T, label: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{label} path has no parent"))?;
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("cannot encode {label}: {error}"))?;
    for attempt in 0..32_u8 {
        let temporary = parent.join(format!(
            ".{}-{}-{attempt}.tmp",
            label.replace(' ', "-"),
            std::process::id()
        ));
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!("cannot create {}: {error}", temporary.display()));
            }
        };
        let result = file
            .write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("cannot write {}: {error}", temporary.display()))
            .and_then(|()| {
                fs::rename(&temporary, path)
                    .map_err(|error| format!("cannot replace {}: {error}", path.display()))
            });
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        return result;
    }
    Err(format!("cannot allocate temporary {label} file"))
}

#[derive(Default)]
struct VerifyOptions {
    root: Option<PathBuf>,
    root_sha256: Option<String>,
    catalog: Option<PathBuf>,
    next_root: Option<PathBuf>,
    at_unix: Option<u64>,
    minimum_sequence: Option<u64>,
    json: bool,
}

struct ResolveOptions {
    verify: VerifyOptions,
    package: String,
    surface_code: u8,
    pam_version: Version,
    native_protocol: Option<u32>,
    desktop_protocol: Option<u32>,
}

struct MatrixOptions {
    verify: VerifyOptions,
    pam_version: Version,
    native_protocol: u32,
    desktop_protocol: u32,
}

struct AdoptOptions {
    project: PathBuf,
    next_root: PathBuf,
    next_catalog: PathBuf,
    at_unix: Option<u64>,
    json: bool,
}

struct PayloadOptions {
    document: PathBuf,
    output: Option<PathBuf>,
}

pub fn run(arguments: Vec<OsString>) -> Result<u8, String> {
    let mut arguments = arguments.into_iter();
    let Some(command) = arguments.next() else {
        print_usage();
        return Ok(0);
    };
    match command.to_string_lossy().as_ref() {
        "--help" | "-h" | "help" => {
            print_usage();
            Ok(0)
        }
        "verify" => verify_command(parse_options(arguments, false)?),
        "rotate" => verify_rotation_command(parse_options(arguments, true)?),
        "adopt" => adopt_rotation_command(parse_adopt_options(arguments)?),
        "resolve" => resolve_command(parse_resolve_options(arguments)?),
        "matrix" => matrix_command(parse_matrix_options(arguments)?),
        "payload" => payload_command(parse_payload_options(arguments)?),
        "key-id" => key_id_command(arguments),
        unknown => Err(format!(
            "unknown registry command {unknown:?}; expected verify, resolve, matrix, rotate, adopt, payload, or key-id"
        )),
    }
}

fn parse_matrix_options(
    arguments: impl Iterator<Item = OsString>,
) -> Result<MatrixOptions, String> {
    let mut verify = VerifyOptions::default();
    let mut pam_version = Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|error| format!("invalid PAM package version: {error}"))?;
    let mut native_protocol = None;
    let mut desktop_protocol = None;
    let mut arguments = arguments;
    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--root" => verify.root = Some(required_path(&mut arguments, "--root")?),
            "--root-sha256" => {
                verify.root_sha256 = Some(required_utf8(&mut arguments, "--root-sha256")?)
            }
            "--catalog" => verify.catalog = Some(required_path(&mut arguments, "--catalog")?),
            "--at-unix" => {
                verify.at_unix = Some(
                    required_utf8(&mut arguments, "--at-unix")?
                        .parse()
                        .map_err(|_| "--at-unix must be an unsigned integer".to_owned())?,
                )
            }
            "--minimum-sequence" => {
                verify.minimum_sequence = Some(
                    required_utf8(&mut arguments, "--minimum-sequence")?
                        .parse()
                        .map_err(|_| "--minimum-sequence must be an unsigned integer".to_owned())?,
                )
            }
            "--pam-version" => {
                pam_version = Version::parse(&required_utf8(&mut arguments, "--pam-version")?)
                    .map_err(|error| format!("invalid --pam-version: {error}"))?
            }
            "--native-protocol" => {
                native_protocol = Some(
                    required_utf8(&mut arguments, "--native-protocol")?
                        .parse()
                        .map_err(|_| "--native-protocol must be an integer".to_owned())?,
                )
            }
            "--desktop-protocol" => {
                desktop_protocol = Some(
                    required_utf8(&mut arguments, "--desktop-protocol")?
                        .parse()
                        .map_err(|_| "--desktop-protocol must be an integer".to_owned())?,
                )
            }
            "--json" => verify.json = true,
            unknown => return Err(format!("unknown registry matrix option {unknown:?}")),
        }
    }
    if verify.root.is_none() || verify.root_sha256.is_none() || verify.catalog.is_none() {
        return Err("registry matrix requires --root, --root-sha256, and --catalog".to_owned());
    }
    Ok(MatrixOptions {
        verify,
        pam_version,
        native_protocol: native_protocol
            .ok_or_else(|| "registry matrix requires --native-protocol".to_owned())?,
        desktop_protocol: desktop_protocol
            .ok_or_else(|| "registry matrix requires --desktop-protocol".to_owned())?,
    })
}

fn parse_payload_options(
    mut arguments: impl Iterator<Item = OsString>,
) -> Result<PayloadOptions, String> {
    let mut document = None;
    let mut output = None;
    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--document" => document = Some(required_path(&mut arguments, "--document")?),
            "--output" => output = Some(required_path(&mut arguments, "--output")?),
            unknown => return Err(format!("unknown registry payload option {unknown:?}")),
        }
    }
    Ok(PayloadOptions {
        document: document.ok_or_else(|| "registry payload requires --document".to_owned())?,
        output,
    })
}

fn payload_command(options: PayloadOptions) -> Result<u8, String> {
    let bytes = read_bounded(&options.document, "registry signing document")?;
    let shape: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid registry signing document: {error}"))?;
    let payload = if shape.get("generation").is_some() {
        let root: RegistryRoot = serde_json::from_slice(&bytes)
            .map_err(|error| format!("invalid registry root: {error}"))?;
        validate_root_shape(&root)?;
        canonical_root_payload(&root)?
    } else if shape.get("sequence").is_some() {
        let catalog: PluginCatalog = serde_json::from_slice(&bytes)
            .map_err(|error| format!("invalid plugin catalog: {error}"))?;
        validate_catalog_shape_for_signing(&catalog)?;
        canonical_catalog_payload(&catalog)?
    } else {
        return Err("registry signing document must be a root or catalog".to_owned());
    };
    if let Some(output) = options.output {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output)
            .map_err(|error| {
                format!(
                    "cannot create signing payload {}: {error}",
                    output.display()
                )
            })?;
        file.write_all(&payload)
            .and_then(|()| file.sync_all())
            .map_err(|error| {
                format!("cannot write signing payload {}: {error}", output.display())
            })?;
        println!(
            "Wrote {} canonical signing bytes to {}.",
            payload.len(),
            output.display()
        );
    } else {
        std::io::stdout()
            .write_all(&payload)
            .map_err(|error| format!("cannot write registry signing payload: {error}"))?;
    }
    Ok(0)
}

fn key_id_command(mut arguments: impl Iterator<Item = OsString>) -> Result<u8, String> {
    let mut public_key = None;
    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--public-key" => public_key = Some(required_utf8(&mut arguments, "--public-key")?),
            unknown => return Err(format!("unknown registry key-id option {unknown:?}")),
        }
    }
    let public_key =
        public_key.ok_or_else(|| "registry key-id requires --public-key".to_owned())?;
    let decoded = decode_hex::<32>(&public_key, "registry public key")?;
    VerifyingKey::from_bytes(&decoded)
        .map_err(|_| "registry public key is not valid Ed25519".to_owned())?;
    println!("{}", encode_hex(&Sha256::digest(decoded)));
    Ok(0)
}

fn parse_adopt_options(
    mut arguments: impl Iterator<Item = OsString>,
) -> Result<AdoptOptions, String> {
    let mut project = None;
    let mut next_root = None;
    let mut next_catalog = None;
    let mut at_unix = None;
    let mut json = false;
    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--project" => project = Some(required_path(&mut arguments, "--project")?),
            "--next-root" => next_root = Some(required_path(&mut arguments, "--next-root")?),
            "--next-catalog" => {
                next_catalog = Some(required_path(&mut arguments, "--next-catalog")?)
            }
            "--at-unix" => {
                at_unix = Some(
                    required_utf8(&mut arguments, "--at-unix")?
                        .parse()
                        .map_err(|_| "--at-unix must be an unsigned integer".to_owned())?,
                )
            }
            "--json" => json = true,
            unknown => return Err(format!("unknown registry adopt option {unknown:?}")),
        }
    }
    Ok(AdoptOptions {
        project: project.ok_or_else(|| "registry adoption requires --project".to_owned())?,
        next_root: next_root.ok_or_else(|| "registry adoption requires --next-root".to_owned())?,
        next_catalog: next_catalog
            .ok_or_else(|| "registry adoption requires --next-catalog".to_owned())?,
        at_unix,
        json,
    })
}

fn parse_resolve_options(
    arguments: impl Iterator<Item = OsString>,
) -> Result<ResolveOptions, String> {
    let mut verify = VerifyOptions::default();
    let mut package = None;
    let mut surface_code = None;
    let mut pam_version = Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|error| format!("invalid PAM package version: {error}"))?;
    let mut native_protocol = None;
    let mut desktop_protocol = None;
    let mut arguments = arguments;
    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--root" => verify.root = Some(required_path(&mut arguments, "--root")?),
            "--root-sha256" => {
                verify.root_sha256 = Some(required_utf8(&mut arguments, "--root-sha256")?)
            }
            "--catalog" => verify.catalog = Some(required_path(&mut arguments, "--catalog")?),
            "--at-unix" => {
                verify.at_unix = Some(
                    required_utf8(&mut arguments, "--at-unix")?
                        .parse()
                        .map_err(|_| "--at-unix must be an unsigned integer".to_owned())?,
                )
            }
            "--minimum-sequence" => {
                verify.minimum_sequence = Some(
                    required_utf8(&mut arguments, "--minimum-sequence")?
                        .parse()
                        .map_err(|_| "--minimum-sequence must be an unsigned integer".to_owned())?,
                )
            }
            "--package" => package = Some(required_utf8(&mut arguments, "--package")?),
            "--surface-code" => {
                surface_code = Some(
                    required_utf8(&mut arguments, "--surface-code")?
                        .parse()
                        .map_err(|_| "--surface-code must be 1, 2, or 3".to_owned())?,
                )
            }
            "--pam-version" => {
                pam_version = Version::parse(&required_utf8(&mut arguments, "--pam-version")?)
                    .map_err(|error| format!("invalid --pam-version: {error}"))?;
            }
            "--native-protocol" => {
                native_protocol = Some(
                    required_utf8(&mut arguments, "--native-protocol")?
                        .parse()
                        .map_err(|_| "--native-protocol must be an integer".to_owned())?,
                )
            }
            "--desktop-protocol" => {
                desktop_protocol = Some(
                    required_utf8(&mut arguments, "--desktop-protocol")?
                        .parse()
                        .map_err(|_| "--desktop-protocol must be an integer".to_owned())?,
                )
            }
            "--json" => verify.json = true,
            unknown => return Err(format!("unknown registry resolve option {unknown:?}")),
        }
    }
    if verify.root.is_none() || verify.root_sha256.is_none() || verify.catalog.is_none() {
        return Err("registry resolution requires --root, --root-sha256, and --catalog".to_owned());
    }
    let package = package.ok_or_else(|| "registry resolution requires --package".to_owned())?;
    validate_package(&package)?;
    let surface_code = surface_code
        .filter(|code| (1..=3).contains(code))
        .ok_or_else(|| "registry resolution requires --surface-code 1, 2, or 3".to_owned())?;
    if surface_code == 2 && native_protocol.is_none() {
        return Err("Native registry resolution requires --native-protocol".to_owned());
    }
    if surface_code == 3 && desktop_protocol.is_none() {
        return Err("Desktop registry resolution requires --desktop-protocol".to_owned());
    }
    Ok(ResolveOptions {
        verify,
        package,
        surface_code,
        pam_version,
        native_protocol,
        desktop_protocol,
    })
}

fn parse_options(
    mut arguments: impl Iterator<Item = OsString>,
    rotation: bool,
) -> Result<VerifyOptions, String> {
    let mut options = VerifyOptions::default();
    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--root" => options.root = Some(required_path(&mut arguments, "--root")?),
            "--root-sha256" => {
                options.root_sha256 = Some(required_utf8(&mut arguments, "--root-sha256")?)
            }
            "--catalog" if !rotation => {
                options.catalog = Some(required_path(&mut arguments, "--catalog")?)
            }
            "--next-root" if rotation => {
                options.next_root = Some(required_path(&mut arguments, "--next-root")?)
            }
            "--at-unix" => {
                options.at_unix = Some(
                    required_utf8(&mut arguments, "--at-unix")?
                        .parse()
                        .map_err(|_| "--at-unix must be an unsigned integer".to_owned())?,
                )
            }
            "--minimum-sequence" if !rotation => {
                options.minimum_sequence = Some(
                    required_utf8(&mut arguments, "--minimum-sequence")?
                        .parse()
                        .map_err(|_| "--minimum-sequence must be an unsigned integer".to_owned())?,
                )
            }
            "--json" => options.json = true,
            unknown => return Err(format!("unknown registry option {unknown:?}")),
        }
    }
    if options.root.is_none() || options.root_sha256.is_none() {
        return Err("registry verification requires --root and --root-sha256".to_owned());
    }
    if rotation && options.next_root.is_none() {
        return Err("registry rotation verification requires --next-root".to_owned());
    }
    if !rotation && options.catalog.is_none() {
        return Err("registry verification requires --catalog".to_owned());
    }
    Ok(options)
}

fn required_path(
    arguments: &mut impl Iterator<Item = OsString>,
    option: &str,
) -> Result<PathBuf, String> {
    arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("{option} requires a path"))
}

fn required_utf8(
    arguments: &mut impl Iterator<Item = OsString>,
    option: &str,
) -> Result<String, String> {
    arguments
        .next()
        .ok_or_else(|| format!("{option} requires a value"))?
        .into_string()
        .map_err(|_| format!("{option} must be valid UTF-8"))
}

fn verify_command(options: VerifyOptions) -> Result<u8, String> {
    let now = options.at_unix.unwrap_or_else(unix_seconds);
    let (root, _) = load_trusted_root(
        options.root.as_deref().expect("validated root"),
        options
            .root_sha256
            .as_deref()
            .expect("validated root fingerprint"),
        now,
    )?;
    let catalog: PluginCatalog = read_document(
        options.catalog.as_deref().expect("validated catalog"),
        "plugin catalog",
    )?;
    validate_catalog(&catalog, &root, now)?;
    enforce_minimum_sequence(catalog.sequence, options.minimum_sequence)?;
    let report = VerificationReport {
        schema_version: SCHEMA_VERSION,
        result_code: 1,
        registry: &catalog.registry,
        root_generation: root.generation,
        catalog_sequence: Some(catalog.sequence),
        plugin_releases: catalog.plugins.len(),
        revocations: catalog.revocations.len(),
    };
    print_report(&report, options.json)?;
    Ok(0)
}

fn verify_rotation_command(options: VerifyOptions) -> Result<u8, String> {
    let now = options.at_unix.unwrap_or_else(unix_seconds);
    let (_current, next, _) = verify_rotation(
        options.root.as_deref().expect("validated root"),
        options
            .root_sha256
            .as_deref()
            .expect("validated root fingerprint"),
        options.next_root.as_deref().expect("validated next root"),
        now,
    )?;
    let report = VerificationReport {
        schema_version: SCHEMA_VERSION,
        result_code: 1,
        registry: &next.registry,
        root_generation: next.generation,
        catalog_sequence: None,
        plugin_releases: 0,
        revocations: 0,
    };
    print_report(&report, options.json)?;
    Ok(0)
}

fn verify_rotation(
    current_path: &Path,
    current_sha256: &str,
    next_path: &Path,
    now: u64,
) -> Result<(RegistryRoot, RegistryRoot, String), String> {
    let (current, _) = load_trusted_root(current_path, current_sha256, now)?;
    let next_bytes = read_bounded(next_path, "next registry root")?;
    let next_sha256 = encode_hex(&Sha256::digest(&next_bytes));
    let next: RegistryRoot = serde_json::from_slice(&next_bytes)
        .map_err(|error| format!("invalid next registry root: {error}"))?;
    validate_root_structure(&next, now)?;
    if next.registry != current.registry || next.generation != current.generation + 1 {
        return Err(
            "next registry root must retain the registry and increment generation by one"
                .to_owned(),
        );
    }
    let payload = canonical_root_payload(&next)?;
    verify_threshold(&payload, &next.signatures, &next.keys, next.threshold)?;
    verify_threshold(
        &payload,
        &next.previous_signatures,
        &current.keys,
        current.threshold,
    )?;
    Ok((current, next, next_sha256))
}

fn adopt_rotation_command(options: AdoptOptions) -> Result<u8, String> {
    let project = fs::canonicalize(&options.project).map_err(|error| {
        format!(
            "cannot resolve registry project {}: {error}",
            options.project.display()
        )
    })?;
    if !project.is_dir() {
        return Err("registry adoption project must be a directory".to_owned());
    }
    let current_config = load_project_config(&project)?
        .ok_or_else(|| "registry adoption requires project pam-registry.json".to_owned())?;
    let current_state = load_project_state(&project)?;
    let current_root_path = project_document_path(&project, &current_config.root_path, "rootPath")?;
    let next_root_path = project_document_path(&project, &options.next_root, "nextRoot")?;
    let next_catalog_path = project_document_path(&project, &options.next_catalog, "nextCatalog")?;
    let now = options.at_unix.unwrap_or_else(unix_seconds);
    let (current_root, next_root, next_root_sha256) = verify_rotation(
        &current_root_path,
        &current_config.root_sha256,
        &next_root_path,
        now,
    )?;
    if let Some(state) = &current_state
        && (state.registry != current_root.registry
            || state.root_sha256 != current_config.root_sha256
            || state.root_generation != current_root.generation)
    {
        return Err("project registry state does not match the current trusted root".to_owned());
    }
    let next_catalog: PluginCatalog = read_document(&next_catalog_path, "next plugin catalog")?;
    validate_catalog(&next_catalog, &next_root, now)?;
    enforce_minimum_sequence(
        next_catalog.sequence,
        current_state.as_ref().map(|state| state.catalog_sequence),
    )?;
    let next_config = ProjectRegistryConfig {
        schema_version: SCHEMA_VERSION,
        root_path: options.next_root,
        root_sha256: next_root_sha256.clone(),
        catalog_path: options.next_catalog,
        native_protocol: current_config.native_protocol,
        desktop_protocol: current_config.desktop_protocol,
    };
    let next_state = ProjectRegistryState {
        schema_version: SCHEMA_VERSION,
        registry: next_root.registry.clone(),
        root_sha256: next_root_sha256.clone(),
        root_generation: next_root.generation,
        catalog_sequence: next_catalog.sequence,
    };
    let receipt = ProjectRotationReceipt {
        schema_version: SCHEMA_VERSION,
        operation_code: 1,
        previous_root_sha256: current_config.root_sha256,
        previous_state: current_state,
        next_config: next_config.clone(),
        next_state: next_state.clone(),
    };
    let receipt_path = project.join(PROJECT_ROTATION_RECEIPT);
    let receipt_parent = receipt_path.parent().expect("receipt path has parent");
    ensure_real_directory(receipt_parent)?;
    write_json_atomic(&receipt_path, &receipt, "registry rotation receipt")?;
    write_json_atomic(
        &project.join(PROJECT_REGISTRY_CONFIG),
        &next_config,
        "registry configuration",
    )?;
    write_project_state(&project, &next_state)?;
    fs::remove_file(&receipt_path).map_err(|error| {
        format!("registry rotation was adopted but its receipt could not be removed: {error}")
    })?;
    let report = RotationAdoptionReport {
        schema_version: SCHEMA_VERSION,
        result_code: 1,
        registry: &next_root.registry,
        previous_root_generation: current_root.generation,
        root_generation: next_root.generation,
        catalog_sequence: next_catalog.sequence,
        root_sha256: &next_root_sha256,
    };
    if options.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .map_err(|error| format!("cannot encode registry adoption: {error}"))?
        );
    } else {
        println!(
            "Adopted registry {} root generation {} and catalog sequence {} (SHA-256 {}).",
            report.registry, report.root_generation, report.catalog_sequence, report.root_sha256
        );
    }
    Ok(0)
}

fn resolve_command(options: ResolveOptions) -> Result<u8, String> {
    let release = resolve_verified(
        options.verify.root.as_deref().expect("validated root"),
        options
            .verify
            .root_sha256
            .as_deref()
            .expect("validated root fingerprint"),
        options
            .verify
            .catalog
            .as_deref()
            .expect("validated catalog"),
        &options.package,
        options.surface_code,
        &options.pam_version,
        options.native_protocol,
        options.desktop_protocol,
        options.verify.minimum_sequence,
        options.verify.at_unix,
    )?;
    let report = ResolutionReport {
        schema_version: SCHEMA_VERSION,
        result_code: 1,
        registry: &release.registry,
        catalog_sequence: release.catalog_sequence,
        package: &release.package,
        version: &release.version,
        artifact_kind_code: release.artifact_kind_code,
        artifact_url: &release.artifact_url,
        sha256: &release.sha256,
    };
    if options.verify.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .map_err(|error| format!("cannot encode registry resolution: {error}"))?
        );
    } else {
        println!(
            "Resolved {} {} to {} (SHA-256 {}).",
            report.package, report.version, report.artifact_url, report.sha256
        );
    }
    Ok(0)
}

fn matrix_command(options: MatrixOptions) -> Result<u8, String> {
    let now = options.verify.at_unix.unwrap_or_else(unix_seconds);
    let root_sha256 = options
        .verify
        .root_sha256
        .as_deref()
        .expect("validated root fingerprint");
    let (root, _) = load_trusted_root(
        options.verify.root.as_deref().expect("validated root"),
        root_sha256,
        now,
    )?;
    let catalog: PluginCatalog = read_document(
        options
            .verify
            .catalog
            .as_deref()
            .expect("validated catalog"),
        "plugin catalog",
    )?;
    validate_catalog(&catalog, &root, now)?;
    enforce_minimum_sequence(catalog.sequence, options.verify.minimum_sequence)?;
    let entries = compatibility_matrix_entries(
        &catalog,
        &options.pam_version,
        options.native_protocol,
        options.desktop_protocol,
    );
    let compatible_entries = entries
        .iter()
        .filter(|entry| entry.result_code == CompatibilityResult::Compatible as u8)
        .count();
    let surface_summaries = compatibility_surface_summaries(&entries);
    let report = CompatibilityMatrixReport {
        schema_url: COMPATIBILITY_MATRIX_SCHEMA_URL,
        schema_version: SCHEMA_VERSION,
        result_code: 1,
        registry: &catalog.registry,
        root_sha256,
        root_generation: root.generation,
        catalog_sequence: catalog.sequence,
        generated_at_unix: catalog.generated_at_unix,
        expires_at_unix: catalog.expires_at_unix,
        pam_version: options.pam_version.to_string(),
        native_protocol: options.native_protocol,
        desktop_protocol: options.desktop_protocol,
        compatible_entries,
        surface_summaries,
        entries,
    };
    if options.verify.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .map_err(|error| format!("cannot encode registry compatibility matrix: {error}"))?
        );
    } else {
        println!(
            "Verified compatibility matrix for PAM {} / Native protocol {} / Desktop protocol {}: {}/{} compatible entries (catalog sequence {}).",
            report.pam_version,
            report.native_protocol,
            report.desktop_protocol,
            report.compatible_entries,
            report.entries.len(),
            report.catalog_sequence
        );
        for entry in &report.entries {
            println!(
                "{} {} surface {} result {}",
                entry.package, entry.version, entry.surface_code, entry.result_code
            );
        }
    }
    Ok(0)
}

fn compatibility_surface_summaries(
    entries: &[CompatibilityMatrixEntry<'_>],
) -> Vec<SurfaceCompatibilitySummary> {
    (1..=3)
        .map(|surface_code| {
            let surface_entries = entries
                .iter()
                .filter(|entry| entry.surface_code == surface_code);
            let total_entries = surface_entries.clone().count();
            let compatible_entries = surface_entries
                .filter(|entry| entry.result_code == CompatibilityResult::Compatible as u8)
                .count();
            SurfaceCompatibilitySummary {
                surface_code,
                total_entries,
                compatible_entries,
            }
        })
        .collect()
}

fn compatibility_matrix_entries<'a>(
    catalog: &'a PluginCatalog,
    pam_version: &Version,
    native_protocol: u32,
    desktop_protocol: u32,
) -> Vec<CompatibilityMatrixEntry<'a>> {
    let mut entries = Vec::new();
    for release in &catalog.plugins {
        let pam_matches = VersionReq::parse(&release.compatibility.pam)
            .is_ok_and(|requirement| requirement.matches(pam_version));
        for &surface_code in &release.surface_codes {
            let result = if !pam_matches {
                CompatibilityResult::PamVersionMismatch
            } else if (surface_code == 2
                && release.compatibility.native_protocol != Some(native_protocol))
                || (surface_code == 3
                    && release.compatibility.desktop_protocol != Some(desktop_protocol))
            {
                CompatibilityResult::ProtocolMismatch
            } else {
                CompatibilityResult::Compatible
            };
            entries.push(CompatibilityMatrixEntry {
                package: &release.package,
                version: &release.version,
                surface_code,
                result_code: result as u8,
                artifact_kind_code: release.artifact_kind_code,
                sha256: &release.sha256,
            });
        }
    }
    entries
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_verified(
    root_path: &Path,
    root_sha256: &str,
    catalog_path: &Path,
    package: &str,
    surface_code: u8,
    pam_version: &Version,
    native_protocol: Option<u32>,
    desktop_protocol: Option<u32>,
    minimum_sequence: Option<u64>,
    at_unix: Option<u64>,
) -> Result<VerifiedRelease, String> {
    let now = at_unix.unwrap_or_else(unix_seconds);
    let (root, _) = load_trusted_root(root_path, root_sha256, now)?;
    let catalog: PluginCatalog = read_document(catalog_path, "plugin catalog")?;
    validate_catalog(&catalog, &root, now)?;
    enforce_minimum_sequence(catalog.sequence, minimum_sequence)?;
    let release = resolve_release(
        &catalog,
        package,
        surface_code,
        pam_version,
        native_protocol,
        desktop_protocol,
    )?;
    Ok(VerifiedRelease {
        registry: catalog.registry.clone(),
        root_sha256: root_sha256.to_owned(),
        root_generation: catalog.root_generation,
        catalog_sequence: catalog.sequence,
        package: release.package.clone(),
        version: release.version.clone(),
        artifact_kind_code: release.artifact_kind_code,
        artifact_url: release.artifact_url.clone(),
        sha256: release.sha256.clone(),
    })
}

fn resolve_release<'a>(
    catalog: &'a PluginCatalog,
    package: &str,
    surface_code: u8,
    pam_version: &Version,
    native_protocol: Option<u32>,
    desktop_protocol: Option<u32>,
) -> Result<&'a PluginRelease, String> {
    catalog
        .plugins
        .iter()
        .filter(|release| release.package == package)
        .filter(|release| release.surface_codes.contains(&surface_code))
        .filter(|release| {
            VersionReq::parse(&release.compatibility.pam)
                .is_ok_and(|requirement| requirement.matches(pam_version))
        })
        .filter(|release| match surface_code {
            2 => release.compatibility.native_protocol == native_protocol,
            3 => release.compatibility.desktop_protocol == desktop_protocol,
            _ => true,
        })
        .filter_map(|release| {
            Version::parse(&release.version)
                .ok()
                .map(|version| (version, release))
        })
        .max_by(|(left, _), (right, _)| left.cmp(right))
        .map(|(_, release)| release)
        .ok_or_else(|| {
            format!(
                "no signed, non-revoked {package} release is compatible with PAM {pam_version} on surface {surface_code}"
            )
        })
}

fn load_trusted_root(
    path: &Path,
    expected_sha256: &str,
    now: u64,
) -> Result<(RegistryRoot, Vec<u8>), String> {
    validate_lower_hex(expected_sha256, 32, "root SHA-256")?;
    let bytes = read_bounded(path, "registry root")?;
    let actual = encode_hex(&Sha256::digest(&bytes));
    if actual != expected_sha256 {
        return Err(format!(
            "registry root fingerprint mismatch: expected {expected_sha256}, received {actual}"
        ));
    }
    let root: RegistryRoot = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid registry root: {error}"))?;
    validate_root_structure(&root, now)?;
    let payload = canonical_root_payload(&root)?;
    verify_threshold(&payload, &root.signatures, &root.keys, root.threshold)?;
    Ok((root, bytes))
}

fn validate_root_structure(root: &RegistryRoot, now: u64) -> Result<(), String> {
    if root.schema_version != SCHEMA_VERSION
        || root.generation == 0
        || root.keys.is_empty()
        || root.keys.len() > MAX_KEYS
        || root.signatures.len() > MAX_SIGNATURES
        || root.previous_signatures.len() > MAX_SIGNATURES
    {
        return Err("registry root violates schema 1 bounds".to_owned());
    }
    validate_https(&root.registry, "registry")?;
    if root.expires_at_unix <= now {
        return Err("registry root is expired".to_owned());
    }
    if root.issued_at_unix > now.saturating_add(CLOCK_SKEW_SECONDS)
        || root.expires_at_unix <= root.issued_at_unix
        || root.expires_at_unix - root.issued_at_unix > ROOT_MAX_VALIDITY_SECONDS
    {
        return Err("registry root validity window is invalid or exceeds 366 days".to_owned());
    }
    let active = root.keys.iter().filter(|key| key.state_code == 1).count();
    if root.threshold == 0 || usize::from(root.threshold) > active {
        return Err("registry root threshold exceeds its active key count".to_owned());
    }
    ensure_sorted_unique(
        &root.keys,
        |left, right| left.key_id < right.key_id,
        "registry keys",
    )?;
    ensure_sorted_unique(
        &root.signatures,
        |left, right| left.key_id < right.key_id,
        "root signatures",
    )?;
    ensure_sorted_unique(
        &root.previous_signatures,
        |left, right| left.key_id < right.key_id,
        "previous root signatures",
    )?;
    for signature in root.signatures.iter().chain(&root.previous_signatures) {
        validate_signature_shape(signature)?;
    }
    for key in &root.keys {
        if !(1..=3).contains(&key.state_code) {
            return Err("registry key stateCode must be 1, 2, or 3".to_owned());
        }
        let public_key = decode_hex::<32>(&key.public_key, "registry public key")?;
        let expected_id = encode_hex(&Sha256::digest(public_key));
        if key.key_id != expected_id {
            return Err("registry keyId does not match its public key".to_owned());
        }
    }
    Ok(())
}

fn validate_root_shape(root: &RegistryRoot) -> Result<(), String> {
    validate_root_structure(root, root.issued_at_unix)
}

fn validate_catalog(catalog: &PluginCatalog, root: &RegistryRoot, now: u64) -> Result<(), String> {
    if catalog.registry != root.registry || catalog.root_generation != root.generation {
        return Err("plugin catalog does not match the trusted registry root".to_owned());
    }
    if catalog.generated_at_unix > now.saturating_add(CLOCK_SKEW_SECONDS)
        || catalog.generated_at_unix < root.issued_at_unix
        || catalog.expires_at_unix <= now
        || catalog.expires_at_unix <= catalog.generated_at_unix
        || catalog.expires_at_unix > root.expires_at_unix
        || catalog.expires_at_unix - catalog.generated_at_unix > CATALOG_MAX_VALIDITY_SECONDS
    {
        return Err("plugin catalog timestamps are invalid or expired".to_owned());
    }
    validate_catalog_shape_for_signing(catalog)?;
    let payload = canonical_catalog_payload(catalog)?;
    verify_threshold(&payload, &catalog.signatures, &root.keys, root.threshold)
}

fn validate_catalog_shape_for_signing(catalog: &PluginCatalog) -> Result<(), String> {
    if catalog.schema_version != SCHEMA_VERSION
        || catalog.root_generation == 0
        || catalog.sequence == 0
        || catalog.plugins.len() > MAX_PLUGINS
        || catalog.revocations.len() > MAX_REVOCATIONS
        || catalog.signatures.len() > MAX_SIGNATURES
        || catalog.expires_at_unix <= catalog.generated_at_unix
        || catalog.expires_at_unix - catalog.generated_at_unix > CATALOG_MAX_VALIDITY_SECONDS
    {
        return Err("plugin catalog violates schema 1 bounds".to_owned());
    }
    validate_https(&catalog.registry, "registry")?;
    ensure_sorted_unique(
        &catalog.plugins,
        |left, right| (&left.package, &left.version) < (&right.package, &right.version),
        "plugin releases",
    )?;
    for signature in &catalog.signatures {
        validate_signature_shape(signature)?;
    }
    ensure_sorted_unique(
        &catalog.revocations,
        |left, right| (&left.package, &left.version) < (&right.package, &right.version),
        "plugin revocations",
    )?;
    ensure_sorted_unique(
        &catalog.signatures,
        |left, right| left.key_id < right.key_id,
        "catalog signatures",
    )?;
    let revoked = catalog
        .revocations
        .iter()
        .map(|item| (item.package.as_str(), item.version.as_str()))
        .collect::<BTreeSet<_>>();
    for plugin in &catalog.plugins {
        validate_package(&plugin.package)?;
        validate_version(&plugin.version)?;
        if !(1..=3).contains(&plugin.artifact_kind_code) {
            return Err("plugin artifactKindCode must be 1, 2, or 3".to_owned());
        }
        if (plugin.artifact_kind_code == 2 && !plugin.surface_codes.contains(&2))
            || (plugin.artifact_kind_code == 3 && !plugin.surface_codes.contains(&3))
        {
            return Err(
                "native and Desktop artifacts must target their matching surface".to_owned(),
            );
        }
        validate_https(&plugin.artifact_url, "plugin artifact URL")?;
        validate_lower_hex(&plugin.sha256, 32, "plugin SHA-256")?;
        if plugin.published_at_unix > catalog.generated_at_unix
            || plugin.surface_codes.is_empty()
            || plugin.surface_codes.len() > 3
            || !plugin
                .surface_codes
                .windows(2)
                .all(|pair| pair[0] < pair[1])
            || !plugin
                .surface_codes
                .iter()
                .all(|code| (1..=3).contains(code))
        {
            return Err("plugin release has invalid timestamps or surface codes".to_owned());
        }
        validate_version_requirement(&plugin.compatibility.pam)?;
        if plugin.surface_codes.contains(&2) && plugin.compatibility.native_protocol.is_none() {
            return Err("Native plugins must declare nativeProtocol compatibility".to_owned());
        }
        if plugin.surface_codes.contains(&3) && plugin.compatibility.desktop_protocol.is_none() {
            return Err("Desktop plugins must declare desktopProtocol compatibility".to_owned());
        }
        if revoked.contains(&(plugin.package.as_str(), plugin.version.as_str())) {
            return Err("a revoked plugin release cannot remain installable".to_owned());
        }
    }
    for item in &catalog.revocations {
        validate_package(&item.package)?;
        validate_version(&item.version)?;
        if !(1..=4).contains(&item.reason_code) || item.revoked_at_unix > catalog.generated_at_unix
        {
            return Err("plugin revocation has an invalid reasonCode or timestamp".to_owned());
        }
    }
    Ok(())
}

fn canonical_root_payload(root: &RegistryRoot) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&RootPayload {
        schema_version: root.schema_version,
        registry: &root.registry,
        generation: root.generation,
        issued_at_unix: root.issued_at_unix,
        expires_at_unix: root.expires_at_unix,
        threshold: root.threshold,
        keys: &root.keys,
    })
    .map_err(|error| format!("cannot canonicalize registry root: {error}"))
}

fn canonical_catalog_payload(catalog: &PluginCatalog) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&CatalogPayload {
        schema_version: catalog.schema_version,
        registry: &catalog.registry,
        root_generation: catalog.root_generation,
        sequence: catalog.sequence,
        generated_at_unix: catalog.generated_at_unix,
        expires_at_unix: catalog.expires_at_unix,
        plugins: &catalog.plugins,
        revocations: &catalog.revocations,
    })
    .map_err(|error| format!("cannot canonicalize plugin catalog: {error}"))
}

fn verify_threshold(
    payload: &[u8],
    signatures: &[DocumentSignature],
    keys: &[RegistryKey],
    threshold: u8,
) -> Result<(), String> {
    let keys = keys
        .iter()
        .filter(|key| key.state_code == 1)
        .map(|key| (key.key_id.as_str(), key))
        .collect::<BTreeMap<_, _>>();
    let mut verified = 0_usize;
    for signed in signatures {
        let Some(key) = keys.get(signed.key_id.as_str()) else {
            continue;
        };
        let public_key = decode_hex::<32>(&key.public_key, "registry public key")?;
        let signature = decode_hex::<64>(&signed.signature, "registry signature")?;
        let verifying_key = VerifyingKey::from_bytes(&public_key)
            .map_err(|_| "registry public key is not valid Ed25519".to_owned())?;
        let signature = Signature::from_bytes(&signature);
        verifying_key
            .verify_strict(payload, &signature)
            .map_err(|_| format!("registry signature from {} is invalid", signed.key_id))?;
        verified += 1;
    }
    if verified < usize::from(threshold) {
        return Err(format!(
            "registry signature threshold is not met: {verified}/{threshold}"
        ));
    }
    Ok(())
}

fn read_document<T: for<'de> Deserialize<'de>>(path: &Path, label: &str) -> Result<T, String> {
    serde_json::from_slice(&read_bounded(path, label)?)
        .map_err(|error| format!("invalid {label}: {error}"))
}

fn read_bounded(path: &Path, label: &str) -> Result<Vec<u8>, String> {
    read_bounded_with_limit(path, label, MAX_DOCUMENT_BYTES)
}

fn read_bounded_with_limit(path: &Path, label: &str, maximum: u64) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect {label} {}: {error}", path.display()))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > maximum {
        return Err(format!(
            "{label} must be a regular file no larger than {maximum} bytes"
        ));
    }
    fs::read(path).map_err(|error| format!("cannot read {label} {}: {error}", path.display()))
}

fn validate_package(value: &str) -> Result<(), String> {
    let Some((vendor, package)) = value.split_once('/') else {
        return Err("plugin package must use Composer vendor/package syntax".to_owned());
    };
    if !valid_slug(vendor) || !valid_slug(package) {
        return Err("plugin package must use lowercase Composer vendor/package syntax".to_owned());
    }
    Ok(())
}

fn validate_signature_shape(signature: &DocumentSignature) -> Result<(), String> {
    validate_lower_hex(&signature.key_id, 32, "registry signature keyId")?;
    validate_lower_hex(&signature.signature, 64, "registry signature")
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

fn validate_version(value: &str) -> Result<(), String> {
    Version::parse(value)
        .map(|_| ())
        .map_err(|error| format!("plugin version is not SemVer: {error}"))
}

fn validate_version_requirement(value: &str) -> Result<(), String> {
    if value.len() > 128 {
        return Err("plugin PAM compatibility requirement is too long".to_owned());
    }
    VersionReq::parse(value)
        .map(|_| ())
        .map_err(|error| format!("plugin PAM compatibility requirement is invalid: {error}"))
}

fn validate_https(value: &str, label: &str) -> Result<(), String> {
    let authority = value
        .strip_prefix("https://")
        .and_then(|value| value.split('/').next())
        .unwrap_or_default();
    if !value.starts_with("https://")
        || value.len() > 2_048
        || value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte == 0)
        || authority.is_empty()
        || authority.contains('@')
        || value.contains(['#', '\\'])
    {
        return Err(format!("{label} must be a bounded HTTPS URL"));
    }
    Ok(())
}

fn ensure_sorted_unique<T>(
    values: &[T],
    less: impl Fn(&T, &T) -> bool,
    label: &str,
) -> Result<(), String> {
    if !values.windows(2).all(|pair| less(&pair[0], &pair[1])) {
        return Err(format!("{label} must be sorted and unique"));
    }
    Ok(())
}

fn validate_lower_hex(value: &str, bytes: usize, label: &str) -> Result<(), String> {
    if value.len() != bytes * 2
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(format!("{label} must contain {bytes} lowercase hex bytes"));
    }
    Ok(())
}

fn decode_hex<const N: usize>(value: &str, label: &str) -> Result<[u8; N], String> {
    validate_lower_hex(value, N, label)?;
    let mut decoded = [0_u8; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(pair).map_err(|_| format!("invalid {label}"))?;
        decoded[index] = u8::from_str_radix(text, 16).map_err(|_| format!("invalid {label}"))?;
    }
    Ok(decoded)
}

fn encode_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(
        String::with_capacity(bytes.len() * 2),
        |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to a String cannot fail");
            output
        },
    )
}

fn print_report(report: &VerificationReport<'_>, json: bool) -> Result<(), String> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(report)
                .map_err(|error| format!("cannot encode registry report: {error}"))?
        );
    } else if let Some(sequence) = report.catalog_sequence {
        println!(
            "Verified plugin catalog sequence {sequence}: {} release(s), {} revocation(s), root generation {}.",
            report.plugin_releases, report.revocations, report.root_generation
        );
    } else {
        println!(
            "Verified plugin registry root rotation to generation {}.",
            report.root_generation
        );
    }
    Ok(())
}

fn enforce_minimum_sequence(sequence: u64, minimum: Option<u64>) -> Result<(), String> {
    if let Some(minimum) = minimum
        && sequence < minimum
    {
        return Err(format!(
            "plugin catalog sequence {sequence} is older than the required minimum {minimum}"
        ));
    }
    Ok(())
}

fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn print_usage() {
    println!(
        "Usage: pam registry verify --root <root.json> --root-sha256 <hex> --catalog <catalog.json> [--minimum-sequence <n>] [--at-unix <seconds>] [--json]\n       pam registry resolve --root <root.json> --root-sha256 <hex> --catalog <catalog.json> --package <vendor/package> --surface-code <1|2|3> [--pam-version <semver>] [--native-protocol <n>] [--desktop-protocol <n>] [--minimum-sequence <n>] [--at-unix <seconds>] [--json]\n       pam registry matrix --root <root.json> --root-sha256 <hex> --catalog <catalog.json> [--pam-version <semver>] --native-protocol <n> --desktop-protocol <n> [--minimum-sequence <n>] [--at-unix <seconds>] [--json]\n       pam registry rotate --root <current.json> --root-sha256 <hex> --next-root <next.json> [--at-unix <seconds>] [--json]\n       pam registry adopt --project <directory> --next-root <project-relative.json> --next-catalog <project-relative.json> [--at-unix <seconds>] [--json]\n       pam registry payload --document <root-or-catalog.json> [--output <payload.json>]\n       pam registry key-id --public-key <64-lowercase-hex>"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn key(seed: u8, state_code: u8) -> (SigningKey, RegistryKey) {
        let signing = SigningKey::from_bytes(&[seed; 32]);
        let public = signing.verifying_key().to_bytes();
        (
            signing,
            RegistryKey {
                key_id: encode_hex(&Sha256::digest(public)),
                public_key: encode_hex(&public),
                state_code,
            },
        )
    }

    fn signature(key: &SigningKey, key_id: &str, payload: &[u8]) -> DocumentSignature {
        DocumentSignature {
            key_id: key_id.to_owned(),
            signature: encode_hex(&key.sign(payload).to_bytes()),
        }
    }

    fn rotation_project(
        label: &str,
        next_sequence: u64,
        accepted_sequence: u64,
    ) -> (PathBuf, AdoptOptions, String, String) {
        let project =
            std::env::temp_dir().join(format!("pam-registry-adopt-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&project);
        fs::create_dir_all(project.join("registry")).unwrap();
        let (old_signing, old_key) = key(10, 1);
        let (new_signing, new_key) = key(11, 1);
        let mut current = RegistryRoot {
            schema_version: 1,
            registry: "https://plugins.pam.dev/v1".to_owned(),
            generation: 1,
            issued_at_unix: 900,
            expires_at_unix: 5_000,
            threshold: 1,
            keys: vec![old_key],
            signatures: Vec::new(),
            previous_signatures: Vec::new(),
        };
        let current_payload = canonical_root_payload(&current).unwrap();
        current.signatures = vec![signature(
            &old_signing,
            &current.keys[0].key_id,
            &current_payload,
        )];
        let mut next = RegistryRoot {
            schema_version: 1,
            registry: current.registry.clone(),
            generation: 2,
            issued_at_unix: 1_000,
            expires_at_unix: 8_000,
            threshold: 1,
            keys: vec![new_key],
            signatures: Vec::new(),
            previous_signatures: Vec::new(),
        };
        let next_payload = canonical_root_payload(&next).unwrap();
        next.signatures = vec![signature(&new_signing, &next.keys[0].key_id, &next_payload)];
        next.previous_signatures = vec![signature(
            &old_signing,
            &current.keys[0].key_id,
            &next_payload,
        )];
        let mut catalog = PluginCatalog {
            schema_version: 1,
            registry: next.registry.clone(),
            root_generation: 2,
            sequence: next_sequence,
            generated_at_unix: 1_100,
            expires_at_unix: 2_000,
            plugins: vec![PluginRelease {
                package: "pushinbr/pam-android-runtime".to_owned(),
                version: "1.0.3".to_owned(),
                artifact_kind_code: 2,
                artifact_url: "https://plugins.pam.dev/pam-android-runtime.tar.gz".to_owned(),
                sha256: "cd".repeat(32),
                published_at_unix: 1_050,
                surface_codes: vec![2],
                compatibility: PluginCompatibility {
                    pam: "^1.0".to_owned(),
                    native_protocol: Some(1),
                    desktop_protocol: None,
                },
            }],
            revocations: Vec::new(),
            signatures: Vec::new(),
        };
        let catalog_payload = canonical_catalog_payload(&catalog).unwrap();
        catalog.signatures = vec![signature(
            &new_signing,
            &next.keys[0].key_id,
            &catalog_payload,
        )];
        let current_bytes = serde_json::to_vec_pretty(&current).unwrap();
        let next_bytes = serde_json::to_vec_pretty(&next).unwrap();
        let current_sha256 = encode_hex(&Sha256::digest(&current_bytes));
        let next_sha256 = encode_hex(&Sha256::digest(&next_bytes));
        fs::write(project.join("registry/root-v1.json"), current_bytes).unwrap();
        fs::write(project.join("registry/root-v2.json"), next_bytes).unwrap();
        fs::write(
            project.join("registry/catalog-v2.json"),
            serde_json::to_vec_pretty(&catalog).unwrap(),
        )
        .unwrap();
        write_json_atomic(
            &project.join(PROJECT_REGISTRY_CONFIG),
            &ProjectRegistryConfig {
                schema_version: 1,
                root_path: PathBuf::from("registry/root-v1.json"),
                root_sha256: current_sha256.clone(),
                catalog_path: PathBuf::from("registry/catalog-v1.json"),
                native_protocol: Some(1),
                desktop_protocol: None,
            },
            "test registry config",
        )
        .unwrap();
        write_project_state(
            &project,
            &ProjectRegistryState {
                schema_version: 1,
                registry: current.registry,
                root_sha256: current_sha256.clone(),
                root_generation: 1,
                catalog_sequence: accepted_sequence,
            },
        )
        .unwrap();
        let options = AdoptOptions {
            project: project.clone(),
            next_root: PathBuf::from("registry/root-v2.json"),
            next_catalog: PathBuf::from("registry/catalog-v2.json"),
            at_unix: Some(1_200),
            json: false,
        };
        (project, options, current_sha256, next_sha256)
    }

    #[test]
    fn verifies_threshold_catalog_and_rejects_revoked_installable_release() {
        let (first, first_key) = key(1, 1);
        let (second, second_key) = key(2, 1);
        let mut root = RegistryRoot {
            schema_version: 1,
            registry: "https://plugins.pam.dev/v1".to_owned(),
            generation: 1,
            issued_at_unix: 900,
            expires_at_unix: 5_000,
            threshold: 2,
            keys: vec![first_key, second_key],
            signatures: Vec::new(),
            previous_signatures: Vec::new(),
        };
        root.keys
            .sort_by(|left, right| left.key_id.cmp(&right.key_id));
        let root_payload = canonical_root_payload(&root).unwrap();
        root.signatures = root
            .keys
            .iter()
            .map(|item| {
                let signing = if item.public_key == encode_hex(&first.verifying_key().to_bytes()) {
                    &first
                } else {
                    &second
                };
                signature(signing, &item.key_id, &root_payload)
            })
            .collect();
        validate_root_structure(&root, 1_000).unwrap();
        verify_threshold(&root_payload, &root.signatures, &root.keys, 2).unwrap();

        let mut catalog = PluginCatalog {
            schema_version: 1,
            registry: root.registry.clone(),
            root_generation: 1,
            sequence: 1,
            generated_at_unix: 1_000,
            expires_at_unix: 2_000,
            plugins: vec![PluginRelease {
                package: "pushinbr/pam-native-camera".to_owned(),
                version: "1.0.0".to_owned(),
                artifact_kind_code: 1,
                artifact_url: "https://plugins.pam.dev/artifacts/camera.zip".to_owned(),
                sha256: "ab".repeat(32),
                published_at_unix: 900,
                surface_codes: vec![2],
                compatibility: PluginCompatibility {
                    pam: "^1.0".to_owned(),
                    native_protocol: Some(1),
                    desktop_protocol: None,
                },
            }],
            revocations: Vec::new(),
            signatures: Vec::new(),
        };
        let mut newer = catalog.plugins[0].clone();
        newer.version = "1.2.0".to_owned();
        newer.artifact_url = "https://plugins.pam.dev/artifacts/camera-1.2.zip".to_owned();
        catalog.plugins.push(newer);
        catalog.plugins.sort_by(|left, right| {
            (&left.package, &left.version).cmp(&(&right.package, &right.version))
        });
        let payload = canonical_catalog_payload(&catalog).unwrap();
        catalog.signatures = root
            .keys
            .iter()
            .map(|item| {
                let signing = if item.public_key == encode_hex(&first.verifying_key().to_bytes()) {
                    &first
                } else {
                    &second
                };
                signature(signing, &item.key_id, &payload)
            })
            .collect();
        validate_catalog(&catalog, &root, 1_100).unwrap();
        let resolved = resolve_release(
            &catalog,
            "pushinbr/pam-native-camera",
            2,
            &Version::parse("1.0.3").unwrap(),
            Some(1),
            None,
        )
        .unwrap();
        assert_eq!(resolved.version, "1.2.0");
        assert!(
            resolve_release(
                &catalog,
                "pushinbr/pam-native-camera",
                2,
                &Version::parse("2.0.0").unwrap(),
                Some(1),
                None,
            )
            .is_err()
        );

        let fixture = std::env::temp_dir().join(format!(
            "pam-plugin-registry-{}-{}",
            std::process::id(),
            unix_seconds()
        ));
        fs::create_dir(&fixture).unwrap();
        let root_path = fixture.join("root.json");
        let catalog_path = fixture.join("catalog.json");
        let root_bytes = serde_json::to_vec_pretty(&root).unwrap();
        fs::write(&root_path, &root_bytes).unwrap();
        fs::write(&catalog_path, serde_json::to_vec_pretty(&catalog).unwrap()).unwrap();
        let root_sha256 = encode_hex(&Sha256::digest(&root_bytes));
        assert_eq!(
            verify_command(VerifyOptions {
                root: Some(root_path.clone()),
                root_sha256: Some(root_sha256.clone()),
                catalog: Some(catalog_path.clone()),
                at_unix: Some(1_100),
                ..VerifyOptions::default()
            })
            .unwrap(),
            0
        );
        let mut tampered: serde_json::Value =
            serde_json::from_slice(&fs::read(&catalog_path).unwrap()).unwrap();
        tampered["plugins"][0]["artifactUrl"] =
            serde_json::Value::String("https://evil.example/plugin.zip".to_owned());
        fs::write(&catalog_path, serde_json::to_vec_pretty(&tampered).unwrap()).unwrap();
        assert!(
            verify_command(VerifyOptions {
                root: Some(root_path),
                root_sha256: Some(root_sha256),
                catalog: Some(catalog_path),
                at_unix: Some(1_100),
                ..VerifyOptions::default()
            })
            .is_err()
        );
        fs::remove_dir_all(fixture).unwrap();

        catalog.revocations.push(PluginRevocation {
            package: "pushinbr/pam-native-camera".to_owned(),
            version: "1.0.0".to_owned(),
            reason_code: 1,
            revoked_at_unix: 1_000,
        });
        assert!(validate_catalog(&catalog, &root, 1_100).is_err());
    }

    #[test]
    fn rotation_requires_old_and_new_thresholds() {
        let (old_signing, old_key) = key(3, 1);
        let (new_signing, new_key) = key(4, 1);
        let current = RegistryRoot {
            schema_version: 1,
            registry: "https://plugins.pam.dev/v1".to_owned(),
            generation: 1,
            issued_at_unix: 900,
            expires_at_unix: 5_000,
            threshold: 1,
            keys: vec![old_key],
            signatures: Vec::new(),
            previous_signatures: Vec::new(),
        };
        let mut next = RegistryRoot {
            schema_version: 1,
            registry: current.registry.clone(),
            generation: 2,
            issued_at_unix: 1_000,
            expires_at_unix: 8_000,
            threshold: 1,
            keys: vec![new_key],
            signatures: Vec::new(),
            previous_signatures: Vec::new(),
        };
        let payload = canonical_root_payload(&next).unwrap();
        next.signatures = vec![signature(&new_signing, &next.keys[0].key_id, &payload)];
        assert!(
            verify_threshold(
                &payload,
                &next.previous_signatures,
                &current.keys,
                current.threshold
            )
            .is_err()
        );
        next.previous_signatures = vec![signature(&old_signing, &current.keys[0].key_id, &payload)];
        verify_threshold(
            &payload,
            &next.previous_signatures,
            &current.keys,
            current.threshold,
        )
        .unwrap();
        verify_threshold(&payload, &next.signatures, &next.keys, next.threshold).unwrap();
    }

    #[test]
    fn adopts_a_rotated_root_and_catalog_as_one_recoverable_project_change() {
        let (project, options, current_sha256, next_sha256) = rotation_project("success", 8, 7);
        assert_eq!(adopt_rotation_command(options).unwrap(), 0);
        let config = load_project_config(&project).unwrap().unwrap();
        let state = load_project_state(&project).unwrap().unwrap();
        assert_eq!(config.root_path, Path::new("registry/root-v2.json"));
        assert_eq!(config.catalog_path, Path::new("registry/catalog-v2.json"));
        assert_eq!(config.root_sha256, next_sha256);
        assert_eq!(config.native_protocol, Some(1));
        assert_eq!(state.root_generation, 2);
        assert_eq!(state.catalog_sequence, 8);
        assert!(!project.join(PROJECT_ROTATION_RECEIPT).exists());
        let runtime = resolve_project_release_at(
            &project,
            "pushinbr/pam-android-runtime",
            2,
            Some(1),
            None,
            Some(1_200),
        )
        .unwrap()
        .unwrap();
        assert_eq!(runtime.artifact_kind_code, 2);
        assert_eq!(runtime.catalog_sequence, 8);

        write_project_state(
            &project,
            &ProjectRegistryState {
                schema_version: 1,
                registry: state.registry.clone(),
                root_sha256: current_sha256.clone(),
                root_generation: 1,
                catalog_sequence: 7,
            },
        )
        .unwrap();
        write_json_atomic(
            &project.join(PROJECT_ROTATION_RECEIPT),
            &ProjectRotationReceipt {
                schema_version: 1,
                operation_code: 1,
                previous_root_sha256: current_sha256,
                previous_state: None,
                next_config: config.clone(),
                next_state: state.clone(),
            },
            "test rotation receipt",
        )
        .unwrap();
        let recovered = load_project_state(&project).unwrap().unwrap();
        assert_eq!(recovered.root_generation, 2);
        assert_eq!(recovered.catalog_sequence, 8);
        assert!(!project.join(PROJECT_ROTATION_RECEIPT).exists());

        let previous_state = ProjectRegistryState {
            schema_version: 1,
            registry: state.registry.clone(),
            root_sha256: "aa".repeat(32),
            root_generation: 1,
            catalog_sequence: 7,
        };
        let mut previous_config = config.clone();
        previous_config.root_path = PathBuf::from("registry/root-v1.json");
        previous_config.root_sha256 = previous_state.root_sha256.clone();
        previous_config.catalog_path = PathBuf::from("registry/catalog-v1.json");
        write_json_atomic(
            &project.join(PROJECT_REGISTRY_CONFIG),
            &previous_config,
            "test previous config",
        )
        .unwrap();
        write_project_state(&project, &state).unwrap();
        write_json_atomic(
            &project.join(PROJECT_ROTATION_RECEIPT),
            &ProjectRotationReceipt {
                schema_version: 1,
                operation_code: 1,
                previous_root_sha256: previous_config.root_sha256.clone(),
                previous_state: Some(previous_state.clone()),
                next_config: config,
                next_state: state,
            },
            "test rollback receipt",
        )
        .unwrap();
        let rolled_back_config = load_project_config(&project).unwrap().unwrap();
        let rolled_back_state = load_project_state(&project).unwrap().unwrap();
        assert_eq!(rolled_back_config, previous_config);
        assert_eq!(rolled_back_state, previous_state);
        assert!(!project.join(PROJECT_ROTATION_RECEIPT).exists());
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn rejects_catalog_rollback_before_writing_a_rotation_receipt() {
        let (project, options, current_sha256, _next_sha256) = rotation_project("rollback", 6, 7);
        assert!(
            adopt_rotation_command(options)
                .unwrap_err()
                .contains("older than the required minimum")
        );
        let config = load_project_config(&project).unwrap().unwrap();
        assert_eq!(config.root_sha256, current_sha256);
        assert!(!project.join(PROJECT_ROTATION_RECEIPT).exists());
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn project_resolution_rejects_a_protocol_different_from_the_runtime() {
        let project =
            std::env::temp_dir().join(format!("pam-registry-protocol-{}", std::process::id()));
        let _ = fs::remove_dir_all(&project);
        fs::create_dir(&project).unwrap();
        write_json_atomic(
            &project.join(PROJECT_REGISTRY_CONFIG),
            &ProjectRegistryConfig {
                schema_version: 1,
                root_path: PathBuf::from("registry/root.json"),
                root_sha256: "ab".repeat(32),
                catalog_path: PathBuf::from("registry/catalog.json"),
                native_protocol: Some(2),
                desktop_protocol: None,
            },
            "test protocol config",
        )
        .unwrap();
        let error =
            resolve_project_release(&project, "pushinbr/pam-android-runtime", 2, Some(1), None)
                .unwrap_err();
        assert!(error.contains("does not match runtime protocol"));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn signing_payload_is_the_verifiers_exact_canonical_input() {
        let directory =
            std::env::temp_dir().join(format!("pam-registry-payload-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let (_signing, registry_key) = key(31, 1);
        let root = RegistryRoot {
            schema_version: 1,
            registry: "https://plugins.pam.dev/v1".to_owned(),
            generation: 1,
            issued_at_unix: 1_000,
            expires_at_unix: 2_000,
            threshold: 1,
            keys: vec![registry_key],
            signatures: Vec::new(),
            previous_signatures: Vec::new(),
        };
        let document = directory.join("root.json");
        let output = directory.join("root.payload.json");
        fs::write(&document, serde_json::to_vec_pretty(&root).unwrap()).unwrap();
        payload_command(PayloadOptions {
            document,
            output: Some(output.clone()),
        })
        .unwrap();
        assert_eq!(
            fs::read(output).unwrap(),
            canonical_root_payload(&root).unwrap()
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn compatibility_matrix_reports_each_declared_surface_with_stable_integer_codes() {
        let catalog = PluginCatalog {
            schema_version: 1,
            registry: "https://plugins.pam.dev/v1".to_owned(),
            root_generation: 1,
            sequence: 9,
            generated_at_unix: 1_000,
            expires_at_unix: 2_000,
            plugins: vec![
                PluginRelease {
                    package: "pushinbr/pam-everywhere".to_owned(),
                    version: "1.2.0".to_owned(),
                    artifact_kind_code: 1,
                    artifact_url: "https://plugins.pam.dev/everywhere.zip".to_owned(),
                    sha256: "ab".repeat(32),
                    published_at_unix: 900,
                    surface_codes: vec![1, 2, 3],
                    compatibility: PluginCompatibility {
                        pam: "^1.0".to_owned(),
                        native_protocol: Some(1),
                        desktop_protocol: Some(5),
                    },
                },
                PluginRelease {
                    package: "pushinbr/pam-future".to_owned(),
                    version: "2.0.0".to_owned(),
                    artifact_kind_code: 1,
                    artifact_url: "https://plugins.pam.dev/future.zip".to_owned(),
                    sha256: "cd".repeat(32),
                    published_at_unix: 900,
                    surface_codes: vec![1],
                    compatibility: PluginCompatibility {
                        pam: "^2.0".to_owned(),
                        native_protocol: None,
                        desktop_protocol: None,
                    },
                },
            ],
            revocations: Vec::new(),
            signatures: Vec::new(),
        };

        let entries =
            compatibility_matrix_entries(&catalog, &Version::parse("1.4.0").unwrap(), 1, 6);
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].result_code, 1);
        assert_eq!(entries[1].result_code, 1);
        assert_eq!(entries[2].result_code, 3);
        assert_eq!(entries[3].result_code, 2);
        assert_eq!(entries[2].surface_code, 3);

        let root_sha256 = "ef".repeat(32);
        let surface_summaries = compatibility_surface_summaries(&entries);
        assert_eq!(surface_summaries[0].compatible_entries, 1);
        assert_eq!(surface_summaries[1].compatible_entries, 1);
        assert_eq!(surface_summaries[2].compatible_entries, 0);
        let report = CompatibilityMatrixReport {
            schema_url: COMPATIBILITY_MATRIX_SCHEMA_URL,
            schema_version: 1,
            result_code: 1,
            registry: &catalog.registry,
            root_sha256: &root_sha256,
            root_generation: catalog.root_generation,
            catalog_sequence: catalog.sequence,
            generated_at_unix: catalog.generated_at_unix,
            expires_at_unix: catalog.expires_at_unix,
            pam_version: "1.4.0".to_owned(),
            native_protocol: 1,
            desktop_protocol: 6,
            compatible_entries: 2,
            surface_summaries,
            entries,
        };
        let json = serde_json::to_value(report).unwrap();
        assert_eq!(json["$schema"], COMPATIBILITY_MATRIX_SCHEMA_URL);
        assert_eq!(json["entries"][0]["surfaceCode"], 1);
        assert_eq!(json["entries"][2]["resultCode"], 3);
        assert!(json["entries"][0]["resultCode"].is_number());
        assert_eq!(json["surfaceSummaries"][2]["surfaceCode"], 3);
        assert_eq!(json["surfaceSummaries"][2]["compatibleEntries"], 0);
    }
}
