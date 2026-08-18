use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

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
        "resolve" => resolve_command(parse_resolve_options(arguments)?),
        unknown => Err(format!(
            "unknown registry command {unknown:?}; expected verify, resolve, or rotate"
        )),
    }
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
    let (current, _) = load_trusted_root(
        options.root.as_deref().expect("validated root"),
        options
            .root_sha256
            .as_deref()
            .expect("validated root fingerprint"),
        now,
    )?;
    let next: RegistryRoot = read_document(
        options.next_root.as_deref().expect("validated next root"),
        "next registry root",
    )?;
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

fn validate_catalog(catalog: &PluginCatalog, root: &RegistryRoot, now: u64) -> Result<(), String> {
    if catalog.schema_version != SCHEMA_VERSION
        || catalog.sequence == 0
        || catalog.plugins.len() > MAX_PLUGINS
        || catalog.revocations.len() > MAX_REVOCATIONS
        || catalog.signatures.len() > MAX_SIGNATURES
    {
        return Err("plugin catalog violates schema 1 bounds".to_owned());
    }
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
    let payload = canonical_catalog_payload(catalog)?;
    verify_threshold(&payload, &catalog.signatures, &root.keys, root.threshold)
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
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect {label} {}: {error}", path.display()))?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_DOCUMENT_BYTES
    {
        return Err(format!(
            "{label} must be a regular file no larger than 1 MiB"
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
        "Usage: pam registry verify --root <root.json> --root-sha256 <hex> --catalog <catalog.json> [--minimum-sequence <n>] [--at-unix <seconds>] [--json]\n       pam registry resolve --root <root.json> --root-sha256 <hex> --catalog <catalog.json> --package <vendor/package> --surface-code <1|2|3> [--pam-version <semver>] [--native-protocol <n>] [--desktop-protocol <n>] [--minimum-sequence <n>] [--at-unix <seconds>] [--json]\n       pam registry rotate --root <current.json> --root-sha256 <hex> --next-root <next.json> [--at-unix <seconds>] [--json]"
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
}
