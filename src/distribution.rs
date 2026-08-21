use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const MAX_ARTIFACT_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_INVENTORY_BYTES: u64 = 16 * 1024 * 1024;
const UPDATE_MAX_VALIDITY_SECONDS: u64 = 31 * 24 * 60 * 60;
const UPDATE_CLOCK_SKEW_SECONDS: u64 = 5 * 60;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum SurfaceCode {
    Runtime = 1,
    Native = 2,
    Desktop = 3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum PlatformCode {
    Linux = 1,
    Macos = 2,
    Windows = 3,
    Android = 4,
    Ios = 5,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum ArchitectureCode {
    X86_64 = 1,
    Arm64 = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum PackageCode {
    Archive = 1,
    Deb = 2,
    Rpm = 3,
    AppImage = 4,
    Dmg = 5,
    Pkg = 6,
    Msi = 7,
    Nsis = 8,
    Aab = 9,
    Ipa = 10,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
enum CheckCode {
    Install = 1,
    Launch = 2,
    FirstSuccess = 3,
    Upgrade = 4,
    Rollback = 5,
    Signature = 6,
    DependencyInventory = 7,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum ResultCode {
    Passed = 1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum DesktopSignatureKindCode {
    AppleDeveloperId = 1,
    WindowsAuthenticode = 2,
    LinuxPackage = 3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum NotarizationResultCode {
    Passed = 1,
    NotApplicable = 2,
}

impl SurfaceCode {
    fn parse(value: u8) -> Result<Self, String> {
        match value {
            1 => Ok(Self::Runtime),
            2 => Ok(Self::Native),
            3 => Ok(Self::Desktop),
            _ => Err("surfaceCode must be Runtime (1), Native (2), or Desktop (3)".to_owned()),
        }
    }
}

impl PlatformCode {
    fn parse(value: u8) -> Result<Self, String> {
        match value {
            1 => Ok(Self::Linux),
            2 => Ok(Self::Macos),
            3 => Ok(Self::Windows),
            4 => Ok(Self::Android),
            5 => Ok(Self::Ios),
            _ => Err("platformCode must be between 1 and 5".to_owned()),
        }
    }
}

impl ArchitectureCode {
    fn parse(value: u8) -> Result<Self, String> {
        match value {
            1 => Ok(Self::X86_64),
            2 => Ok(Self::Arm64),
            _ => Err("architectureCode must be x86_64 (1) or arm64 (2)".to_owned()),
        }
    }
}

impl PackageCode {
    fn parse(value: u8) -> Result<Self, String> {
        match value {
            1 => Ok(Self::Archive),
            2 => Ok(Self::Deb),
            3 => Ok(Self::Rpm),
            4 => Ok(Self::AppImage),
            5 => Ok(Self::Dmg),
            6 => Ok(Self::Pkg),
            7 => Ok(Self::Msi),
            8 => Ok(Self::Nsis),
            9 => Ok(Self::Aab),
            10 => Ok(Self::Ipa),
            _ => Err("packageCode must be between 1 and 10".to_owned()),
        }
    }
}

impl CheckCode {
    const ALL: [Self; 7] = [
        Self::Install,
        Self::Launch,
        Self::FirstSuccess,
        Self::Upgrade,
        Self::Rollback,
        Self::Signature,
        Self::DependencyInventory,
    ];

    fn parse(value: u8) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|code| *code as u8 == value)
            .ok_or_else(|| "checkCode must be between 1 and 7".to_owned())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DistributionEvidence {
    schema_version: u8,
    #[serde(default)]
    release_version: Option<String>,
    #[serde(default)]
    issued_at_unix: Option<u64>,
    #[serde(default)]
    expires_at_unix: Option<u64>,
    surface_code: u8,
    platform_code: u8,
    architecture_code: u8,
    package_code: u8,
    revision: String,
    baseline_revision: String,
    host_image: String,
    generated_at_unix_ms: u64,
    artifact: EvidenceFile,
    baseline_artifact: EvidenceFile,
    dependency_inventory: EvidenceFile,
    provenance_inventory: EvidenceFile,
    #[serde(default)]
    platform_verification: Option<EvidenceFile>,
    installed_bytes: u64,
    launch_millis: u64,
    first_success_millis: u64,
    signing_identity_sha256: String,
    signing_public_key: String,
    manifest_signature: String,
    checks: Vec<EvidenceCheck>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EvidenceFile {
    path: String,
    sha256: String,
    bytes: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EvidenceCheck {
    check_code: u8,
    result_code: u8,
    duration_millis: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DesktopPlatformVerification {
    schema_version: u8,
    surface_code: u8,
    platform_code: u8,
    package_code: u8,
    signature_kind_code: u8,
    signature_result_code: u8,
    notarization_result_code: u8,
    sandbox_result_code: u8,
    update_recovery_result_code: u8,
    publisher_identity_sha256: String,
    publisher_certificate: EvidenceFile,
    artifact_sha256: String,
    verified_at_unix: u64,
    signature_proof: EvidenceFile,
    #[serde(default)]
    notarization_proof: Option<EvidenceFile>,
    sandbox_proof: EvidenceFile,
    update_recovery_proof: EvidenceFile,
}

fn parse_u8_argument(value: &OsString, option: &str) -> Result<u8, String> {
    value
        .to_str()
        .and_then(|value| value.parse::<u8>().ok())
        .ok_or_else(|| format!("{option} requires an integer between 1 and 255"))
}

fn report_proof(root: &Path, path: Option<PathBuf>, option: &str) -> Result<EvidenceFile, String> {
    let path = path.ok_or_else(|| format!("desktop-report requires {option}"))?;
    evidence_for_path(root, &path, MAX_INVENTORY_BYTES, option)
}

fn evidence_for_path(
    root: &Path,
    path: &Path,
    limit: u64,
    label: &str,
) -> Result<EvidenceFile, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| format!("{label} must be inside the Desktop report directory"))?;
    let relative = relative
        .to_str()
        .ok_or_else(|| format!("{label} path must be UTF-8"))?;
    validate_relative_path(relative, label)?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect {label} {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() == 0 {
        return Err(format!(
            "{label} must be a non-empty regular, non-symlink file"
        ));
    }
    if metadata.len() > limit {
        return Err(format!("{label} exceeds its {limit}-byte limit"));
    }
    let modified = metadata.modified().ok();
    let sha256 = hash_file(path, limit)?;
    let after = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot recheck {label} {}: {error}", path.display()))?;
    if after.file_type().is_symlink()
        || !after.is_file()
        || after.len() != metadata.len()
        || (modified.is_some() && after.modified().ok() != modified)
    {
        return Err(format!("{label} changed while it was hashed"));
    }
    Ok(EvidenceFile {
        path: relative.to_owned(),
        sha256,
        bytes: metadata.len(),
    })
}

pub fn desktop_report(arguments: impl Iterator<Item = OsString>) -> Result<PathBuf, String> {
    let mut artifact = None;
    let mut platform_code = None;
    let mut package_code = None;
    let mut publisher_certificate = None;
    let mut signature_proof = None;
    let mut notarization_proof = None;
    let mut sandbox_proof = None;
    let mut update_recovery_proof = None;
    let mut output = None;
    let mut arguments = arguments;
    while let Some(option) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| format!("{} requires a value", option.to_string_lossy()))?;
        let destination = match option.to_string_lossy().as_ref() {
            "--artifact" => &mut artifact,
            "--platform-code" => {
                platform_code = Some(parse_u8_argument(&value, "--platform-code")?);
                continue;
            }
            "--package-code" => {
                package_code = Some(parse_u8_argument(&value, "--package-code")?);
                continue;
            }
            "--publisher-certificate" => &mut publisher_certificate,
            "--signature-proof" => &mut signature_proof,
            "--notarization-proof" => &mut notarization_proof,
            "--sandbox-proof" => &mut sandbox_proof,
            "--update-recovery-proof" => &mut update_recovery_proof,
            "--output" => &mut output,
            unknown => return Err(format!("unknown desktop-report option: {unknown}")),
        };
        if destination.is_some() {
            return Err(format!(
                "{} may be provided only once",
                option.to_string_lossy()
            ));
        }
        *destination = Some(PathBuf::from(value));
    }
    let output = output.ok_or_else(|| "desktop-report requires --output".to_owned())?;
    if fs::symlink_metadata(&output).is_ok() {
        return Err(format!(
            "refusing to overwrite Desktop platform report: {}",
            output.display()
        ));
    }
    let root = output.parent().unwrap_or_else(|| Path::new("."));
    let artifact_path = artifact.ok_or_else(|| "desktop-report requires --artifact".to_owned())?;
    let artifact = evidence_for_path(
        root,
        &artifact_path,
        MAX_ARTIFACT_BYTES,
        "Desktop installer",
    )?;
    let certificate_path = publisher_certificate
        .ok_or_else(|| "desktop-report requires --publisher-certificate".to_owned())?;
    let publisher_certificate = evidence_for_path(
        root,
        &certificate_path,
        MAX_MANIFEST_BYTES,
        "publisher certificate",
    )?;
    let signature_proof = report_proof(root, signature_proof, "--signature-proof")?;
    let sandbox_proof = report_proof(root, sandbox_proof, "--sandbox-proof")?;
    let update_recovery_proof =
        report_proof(root, update_recovery_proof, "--update-recovery-proof")?;
    let platform = PlatformCode::parse(
        platform_code.ok_or_else(|| "desktop-report requires --platform-code".to_owned())?,
    )?;
    let package = PackageCode::parse(
        package_code.ok_or_else(|| "desktop-report requires --package-code".to_owned())?,
    )?;
    let (signature_kind, notarization_result) = match (platform, package) {
        (PlatformCode::Linux, PackageCode::Deb | PackageCode::Rpm | PackageCode::AppImage) => (
            DesktopSignatureKindCode::LinuxPackage,
            NotarizationResultCode::NotApplicable,
        ),
        (PlatformCode::Macos, PackageCode::Dmg | PackageCode::Pkg) => (
            DesktopSignatureKindCode::AppleDeveloperId,
            NotarizationResultCode::Passed,
        ),
        (PlatformCode::Windows, PackageCode::Msi | PackageCode::Nsis) => (
            DesktopSignatureKindCode::WindowsAuthenticode,
            NotarizationResultCode::NotApplicable,
        ),
        _ => return Err("desktop-report requires a native installer packageCode".to_owned()),
    };
    let notarization_proof = match (platform, notarization_proof) {
        (PlatformCode::Macos, proof) => Some(report_proof(root, proof, "--notarization-proof")?),
        (_, Some(_)) => return Err("--notarization-proof is valid only for macOS".to_owned()),
        (_, None) => None,
    };
    let verified_at_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock is before the Unix epoch".to_owned())?
        .as_secs();
    let report = DesktopPlatformVerification {
        schema_version: 1,
        surface_code: SurfaceCode::Desktop as u8,
        platform_code: platform as u8,
        package_code: package as u8,
        signature_kind_code: signature_kind as u8,
        signature_result_code: ResultCode::Passed as u8,
        notarization_result_code: notarization_result as u8,
        sandbox_result_code: ResultCode::Passed as u8,
        update_recovery_result_code: ResultCode::Passed as u8,
        publisher_identity_sha256: publisher_certificate.sha256.clone(),
        artifact_sha256: artifact.sha256,
        verified_at_unix,
        signature_proof,
        notarization_proof,
        sandbox_proof,
        update_recovery_proof,
        publisher_certificate,
    };
    let bytes = serde_json::to_vec_pretty(&report)
        .map_err(|error| format!("cannot encode Desktop platform report: {error}"))?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err("Desktop platform report exceeds 256 KiB".to_owned());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
        .map_err(|error| format!("cannot create {}: {error}", output.display()))?;
    file.write_all(&bytes)
        .and_then(|()| file.write_all(b"\n"))
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("cannot write {}: {error}", output.display()))?;
    Ok(output)
}

pub struct UpdateAuthorization {
    pub artifact_sha256: String,
    pub artifact_bytes: u64,
}

#[cfg(test)]
pub fn authorize_update_manifest(
    source: &[u8],
    pinned_identities: &[&str],
    expected_release_version: &str,
    expected_platform_code: u8,
    expected_architecture_code: u8,
) -> Result<UpdateAuthorization, String> {
    authorize_update_manifest_at(
        source,
        pinned_identities,
        expected_release_version,
        expected_platform_code,
        expected_architecture_code,
        None,
    )
}

pub fn authorize_update_manifest_at(
    source: &[u8],
    pinned_identities: &[&str],
    expected_release_version: &str,
    expected_platform_code: u8,
    expected_architecture_code: u8,
    freshness_time_unix: Option<u64>,
) -> Result<UpdateAuthorization, String> {
    if source.is_empty() || source.len() as u64 > MAX_MANIFEST_BYTES {
        return Err("update manifest must contain at most 256 KiB".to_owned());
    }
    if pinned_identities.is_empty() || pinned_identities.len() > 2 {
        return Err("update authority must pin one or two signing identities".to_owned());
    }
    for identity in pinned_identities {
        validate_sha256(identity, "pinned update signing identity")?;
    }
    if pinned_identities.len() == 2 && pinned_identities[0] == pinned_identities[1] {
        return Err("pinned update signing identities must be distinct".to_owned());
    }
    let evidence: DistributionEvidence = serde_json::from_slice(source)
        .map_err(|error| format!("invalid update manifest JSON: {error}"))?;
    validate_contract(&evidence)?;
    verify_manifest_signature(source, &evidence)?;
    let release_version = evidence
        .release_version
        .as_deref()
        .ok_or_else(|| "update manifest is missing its signed releaseVersion".to_owned())?;
    if !valid_release_version(release_version) || release_version != expected_release_version {
        return Err(
            "update manifest releaseVersion does not match the requested release".to_owned(),
        );
    }
    if let Some(now) = freshness_time_unix {
        validate_update_freshness(&evidence, now)?;
    }
    if !pinned_identities.contains(&evidence.signing_identity_sha256.as_str()) {
        return Err("update manifest was not signed by the pinned PAM authority".to_owned());
    }
    if evidence.surface_code != SurfaceCode::Runtime as u8
        || evidence.package_code != PackageCode::Archive as u8
        || evidence.platform_code != expected_platform_code
        || evidence.architecture_code != expected_architecture_code
    {
        return Err("update manifest does not authorize this runtime target".to_owned());
    }
    if evidence.artifact.path != "files/candidate.tar.gz" {
        return Err("update manifest artifact path is not the certified candidate".to_owned());
    }
    validate_evidence_file(&evidence.artifact, MAX_ARTIFACT_BYTES, "artifact")?;
    Ok(UpdateAuthorization {
        artifact_sha256: evidence.artifact.sha256,
        artifact_bytes: evidence.artifact.bytes,
    })
}

fn validate_update_freshness(evidence: &DistributionEvidence, now: u64) -> Result<(), String> {
    let issued = evidence.issued_at_unix.ok_or_else(|| {
        "automatically discovered update is missing signed issuedAtUnix".to_owned()
    })?;
    let expires = evidence.expires_at_unix.ok_or_else(|| {
        "automatically discovered update is missing signed expiresAtUnix".to_owned()
    })?;
    if issued == 0
        || expires <= issued
        || expires.saturating_sub(issued) > UPDATE_MAX_VALIDITY_SECONDS
    {
        return Err("signed update freshness window is invalid or exceeds 31 days".to_owned());
    }
    if issued > now.saturating_add(UPDATE_CLOCK_SKEW_SECONDS) {
        return Err("signed update was issued too far in the future".to_owned());
    }
    if expires <= now {
        return Err("signed update freshness window has expired".to_owned());
    }
    Ok(())
}

pub fn verify(manifest_path: &Path) -> Result<serde_json::Value, String> {
    let manifest_source = read_regular(manifest_path, MAX_MANIFEST_BYTES, "manifest")?;
    let manifest_sha256 = hex_digest(&manifest_source);
    let evidence: DistributionEvidence = serde_json::from_slice(&manifest_source)
        .map_err(|error| format!("invalid distribution evidence JSON: {error}"))?;
    validate_contract(&evidence)?;
    verify_manifest_signature(&manifest_source, &evidence)?;
    let root = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    verify_file(root, &evidence.artifact, MAX_ARTIFACT_BYTES, "artifact")?;
    verify_file(
        root,
        &evidence.baseline_artifact,
        MAX_ARTIFACT_BYTES,
        "baseline artifact",
    )?;
    verify_file(
        root,
        &evidence.dependency_inventory,
        MAX_INVENTORY_BYTES,
        "dependency inventory",
    )?;
    verify_file(
        root,
        &evidence.provenance_inventory,
        MAX_INVENTORY_BYTES,
        "provenance inventory",
    )?;
    verify_provenance_entries(root, &evidence.provenance_inventory.path)?;
    verify_desktop_platform_evidence(root, &evidence)?;
    Ok(serde_json::json!({
        "schemaVersion": 1,
        "resultCode": ResultCode::Passed as u8,
        "surfaceCode": evidence.surface_code,
        "platformCode": evidence.platform_code,
        "architectureCode": evidence.architecture_code,
        "packageCode": evidence.package_code,
        "revision": evidence.revision,
        "manifestSha256": manifest_sha256,
        "artifactSha256": evidence.artifact.sha256,
        "baselineArtifactSha256": evidence.baseline_artifact.sha256,
        "dependencyInventorySha256": evidence.dependency_inventory.sha256,
        "provenanceInventorySha256": evidence.provenance_inventory.sha256,
        "platformVerificationSha256": evidence.platform_verification
            .as_ref()
            .map(|verification| verification.sha256.as_str()),
        "signingIdentitySha256": evidence.signing_identity_sha256,
    }))
}

pub fn sign(draft_path: &Path, key_path: &Path, output_path: &Path) -> Result<(), String> {
    if output_path == draft_path || output_path == key_path {
        return Err(
            "distribution:sign output must differ from its draft and private key".to_owned(),
        );
    }
    if fs::symlink_metadata(output_path).is_ok() {
        return Err(format!(
            "refusing to overwrite distribution evidence: {}",
            output_path.display()
        ));
    }
    let draft_source = read_regular(draft_path, MAX_MANIFEST_BYTES, "draft manifest")?;
    let mut document: serde_json::Value = serde_json::from_slice(&draft_source)
        .map_err(|error| format!("invalid distribution draft JSON: {error}"))?;
    let object = document
        .as_object_mut()
        .ok_or_else(|| "distribution draft must be an object".to_owned())?;
    for field in [
        "signingIdentitySha256",
        "signingPublicKey",
        "manifestSignature",
    ] {
        if object.contains_key(field) {
            return Err(format!("distribution draft must not contain {field}"));
        }
    }
    let signing_key = read_signing_key(key_path)?;
    let public_key = signing_key.verifying_key().to_bytes();
    object.insert(
        "signingIdentitySha256".to_owned(),
        serde_json::Value::String(hex_digest(&public_key)),
    );
    object.insert(
        "signingPublicKey".to_owned(),
        serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(public_key)),
    );
    let payload = serde_json::to_vec(&document)
        .map_err(|error| format!("cannot canonicalize distribution draft: {error}"))?;
    let signature = signing_key.sign(&payload);
    document.as_object_mut().expect("validated object").insert(
        "manifestSignature".to_owned(),
        serde_json::Value::String(
            base64::engine::general_purpose::STANDARD.encode(signature.to_bytes()),
        ),
    );
    let signed = serde_json::to_vec_pretty(&document)
        .map_err(|error| format!("cannot serialize signed distribution evidence: {error}"))?;
    if signed.len() as u64 > MAX_MANIFEST_BYTES {
        return Err("signed distribution evidence exceeds the manifest limit".to_owned());
    }
    let evidence: DistributionEvidence = serde_json::from_slice(&signed)
        .map_err(|error| format!("invalid signed distribution evidence: {error}"))?;
    validate_contract(&evidence)?;
    verify_manifest_signature(&signed, &evidence)?;
    let root = output_path.parent().unwrap_or_else(|| Path::new("."));
    verify_file(root, &evidence.artifact, MAX_ARTIFACT_BYTES, "artifact")?;
    verify_file(
        root,
        &evidence.baseline_artifact,
        MAX_ARTIFACT_BYTES,
        "baseline artifact",
    )?;
    verify_file(
        root,
        &evidence.dependency_inventory,
        MAX_INVENTORY_BYTES,
        "dependency inventory",
    )?;
    verify_file(
        root,
        &evidence.provenance_inventory,
        MAX_INVENTORY_BYTES,
        "provenance inventory",
    )?;
    verify_provenance_entries(root, &evidence.provenance_inventory.path)?;
    verify_desktop_platform_evidence(root, &evidence)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output_path)
        .map_err(|error| format!("cannot create {}: {error}", output_path.display()))?;
    output
        .write_all(&signed)
        .and_then(|()| output.write_all(b"\n"))
        .and_then(|()| output.sync_all())
        .map_err(|error| format!("cannot write {}: {error}", output_path.display()))
}

fn read_signing_key(path: &Path) -> Result<SigningKey, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "cannot inspect evidence private key {}: {error}",
            path.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("evidence private key must be a regular, non-symlink file".to_owned());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(
                "evidence private key permissions must not grant group or other access".to_owned(),
            );
        }
    }
    if metadata.len() == 0 || metadata.len() > 128 {
        return Err("evidence private key must contain bounded canonical base64".to_owned());
    }
    let mut source = fs::read(path).map_err(|error| {
        format!(
            "cannot read evidence private key {}: {error}",
            path.display()
        )
    })?;
    while source
        .last()
        .is_some_and(|byte| matches!(byte, b'\r' | b'\n'))
    {
        source.pop();
    }
    let decoded = (|| {
        let mut seed = [0_u8; 32];
        let decoded =
            match base64::engine::general_purpose::STANDARD.decode_slice(&source, &mut seed) {
                Ok(decoded) => decoded,
                Err(_) => {
                    seed.fill(0);
                    return Err("evidence private key must be canonical base64".to_owned());
                }
            };
        if decoded != seed.len()
            || base64::engine::general_purpose::STANDARD
                .encode(seed)
                .as_bytes()
                != source
        {
            seed.fill(0);
            return Err("evidence private key must be canonical padded base64".to_owned());
        }
        Ok(seed)
    })();
    source.fill(0);
    let mut seed = decoded?;
    let signing_key = SigningKey::from_bytes(&seed);
    seed.fill(0);
    Ok(signing_key)
}

fn validate_contract(evidence: &DistributionEvidence) -> Result<(), String> {
    if evidence.schema_version != 1 {
        return Err("schemaVersion must be 1".to_owned());
    }
    if evidence
        .release_version
        .as_deref()
        .is_some_and(|version| !valid_release_version(version))
    {
        return Err("releaseVersion must use canonical v-prefixed SemVer".to_owned());
    }
    if evidence.issued_at_unix.is_some() != evidence.expires_at_unix.is_some() {
        return Err("issuedAtUnix and expiresAtUnix must be declared together".to_owned());
    }
    if let (Some(issued), Some(expires)) = (evidence.issued_at_unix, evidence.expires_at_unix)
        && (issued == 0
            || expires <= issued
            || expires.saturating_sub(issued) > UPDATE_MAX_VALIDITY_SECONDS)
    {
        return Err("signed update freshness window is invalid or exceeds 31 days".to_owned());
    }
    let surface = SurfaceCode::parse(evidence.surface_code)?;
    let platform = PlatformCode::parse(evidence.platform_code)?;
    ArchitectureCode::parse(evidence.architecture_code)?;
    let package = PackageCode::parse(evidence.package_code)?;
    let valid_platform = match surface {
        SurfaceCode::Runtime | SurfaceCode::Desktop => matches!(
            platform,
            PlatformCode::Linux | PlatformCode::Macos | PlatformCode::Windows
        ),
        SurfaceCode::Native => matches!(platform, PlatformCode::Android | PlatformCode::Ios),
    };
    if !valid_platform {
        return Err("platformCode is incompatible with surfaceCode".to_owned());
    }
    let valid_package = match platform {
        PlatformCode::Linux => matches!(
            package,
            PackageCode::Archive | PackageCode::Deb | PackageCode::Rpm | PackageCode::AppImage
        ),
        PlatformCode::Macos => {
            matches!(
                package,
                PackageCode::Archive | PackageCode::Dmg | PackageCode::Pkg
            )
        }
        PlatformCode::Windows => {
            matches!(
                package,
                PackageCode::Archive | PackageCode::Msi | PackageCode::Nsis
            )
        }
        PlatformCode::Android => package == PackageCode::Aab,
        PlatformCode::Ios => package == PackageCode::Ipa,
    };
    if !valid_package {
        return Err("packageCode is incompatible with platformCode".to_owned());
    }
    if surface == SurfaceCode::Desktop {
        let native_installer = match platform {
            PlatformCode::Linux => {
                matches!(
                    package,
                    PackageCode::Deb | PackageCode::Rpm | PackageCode::AppImage
                )
            }
            PlatformCode::Macos => matches!(package, PackageCode::Dmg | PackageCode::Pkg),
            PlatformCode::Windows => matches!(package, PackageCode::Msi | PackageCode::Nsis),
            PlatformCode::Android | PlatformCode::Ios => false,
        };
        if package == PackageCode::Archive {
            if evidence.platform_verification.is_some() {
                return Err(
                    "portable Desktop archives must not claim native platformVerification"
                        .to_owned(),
                );
            }
        } else if native_installer {
            let verification = evidence.platform_verification.as_ref().ok_or_else(|| {
                "native Desktop installer certification requires platformVerification".to_owned()
            })?;
            validate_evidence_file(
                verification,
                MAX_MANIFEST_BYTES,
                "Desktop platform verification",
            )?;
        } else {
            return Err(
                "Desktop certification requires a portable archive or native installer packageCode"
                    .to_owned(),
            );
        }
    } else if evidence.platform_verification.is_some() {
        return Err("platformVerification is reserved for Desktop certification".to_owned());
    }
    validate_revision(&evidence.revision, "revision")?;
    validate_revision(&evidence.baseline_revision, "baselineRevision")?;
    if evidence.host_image.is_empty() || evidence.host_image.len() > 256 {
        return Err("hostImage must contain 1 to 256 bytes".to_owned());
    }
    if evidence.generated_at_unix_ms == 0 {
        return Err("generatedAtUnixMs must be positive".to_owned());
    }
    if evidence.installed_bytes == 0 {
        return Err("installedBytes must be positive".to_owned());
    }
    if evidence.launch_millis == 0 || evidence.first_success_millis == 0 {
        return Err("launchMillis and firstSuccessMillis must be positive".to_owned());
    }
    validate_sha256(&evidence.signing_identity_sha256, "signingIdentitySha256")?;
    validate_checks(&evidence.checks)
}

fn verify_manifest_signature(source: &[u8], evidence: &DistributionEvidence) -> Result<(), String> {
    let public_key = base64::engine::general_purpose::STANDARD
        .decode(&evidence.signing_public_key)
        .map_err(|_| "signingPublicKey must be canonical base64".to_owned())?;
    if base64::engine::general_purpose::STANDARD.encode(&public_key) != evidence.signing_public_key
    {
        return Err("signingPublicKey must be canonical padded base64".to_owned());
    }
    let public_key: [u8; 32] = public_key
        .try_into()
        .map_err(|_| "signingPublicKey must decode to 32 bytes".to_owned())?;
    if hex_digest(&public_key) != evidence.signing_identity_sha256 {
        return Err("signingIdentitySha256 does not match signingPublicKey".to_owned());
    }
    let signature = base64::engine::general_purpose::STANDARD
        .decode(&evidence.manifest_signature)
        .map_err(|_| "manifestSignature must be canonical base64".to_owned())?;
    if base64::engine::general_purpose::STANDARD.encode(&signature) != evidence.manifest_signature {
        return Err("manifestSignature must be canonical padded base64".to_owned());
    }
    let signature = Signature::from_slice(&signature)
        .map_err(|_| "manifestSignature must decode to 64 bytes".to_owned())?;
    let mut document: serde_json::Value = serde_json::from_slice(source)
        .map_err(|error| format!("invalid distribution evidence JSON: {error}"))?;
    document
        .as_object_mut()
        .ok_or_else(|| "distribution evidence must be an object".to_owned())?
        .remove("manifestSignature")
        .ok_or_else(|| "distribution evidence is missing manifestSignature".to_owned())?;
    let payload = serde_json::to_vec(&document)
        .map_err(|error| format!("cannot canonicalize distribution evidence: {error}"))?;
    let key = VerifyingKey::from_bytes(&public_key)
        .map_err(|_| "signingPublicKey is not a valid Ed25519 key".to_owned())?;
    key.verify_strict(&payload, &signature)
        .map_err(|_| "manifestSignature did not verify against the canonical evidence".to_owned())
}

fn validate_checks(checks: &[EvidenceCheck]) -> Result<(), String> {
    if checks.len() != 7 {
        return Err("checks must contain every checkCode from 1 through 7 exactly once".to_owned());
    }
    let mut codes = BTreeSet::new();
    for check in checks {
        let code = CheckCode::parse(check.check_code)?;
        if !codes.insert(code) {
            return Err(
                "checks must contain every checkCode from 1 through 7 exactly once".to_owned(),
            );
        }
        if check.result_code != ResultCode::Passed as u8 {
            return Err(format!(
                "checkCode {} did not pass with resultCode 1",
                check.check_code
            ));
        }
        if check.duration_millis == 0 {
            return Err(format!(
                "checkCode {} durationMillis must be positive",
                check.check_code
            ));
        }
    }
    if codes != CheckCode::ALL.into_iter().collect() {
        return Err("checks must contain every checkCode from 1 through 7 exactly once".to_owned());
    }
    Ok(())
}

fn verify_file(
    root: &Path,
    evidence: &EvidenceFile,
    limit: u64,
    label: &str,
) -> Result<(), String> {
    validate_evidence_file(evidence, limit, label)?;
    let path = root.join(&evidence.path);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| format!("cannot inspect {label} {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("{label} must be a regular, non-symlink file"));
    }
    if metadata.len() != evidence.bytes {
        return Err(format!("{label} byte size does not match the manifest"));
    }
    let actual = hash_file(&path, limit)?;
    if actual != evidence.sha256 {
        return Err(format!("{label} SHA-256 does not match the manifest"));
    }
    Ok(())
}

fn validate_evidence_file(evidence: &EvidenceFile, limit: u64, label: &str) -> Result<(), String> {
    validate_relative_path(&evidence.path, label)?;
    validate_sha256(&evidence.sha256, &format!("{label} sha256"))?;
    if evidence.bytes == 0 || evidence.bytes > limit {
        return Err(format!("{label} bytes must be between 1 and {limit}"));
    }
    Ok(())
}

fn verify_provenance_entries(root: &Path, inventory_path: &str) -> Result<(), String> {
    let source = fs::read(root.join(inventory_path))
        .map_err(|error| format!("cannot read provenance inventory: {error}"))?;
    let source = std::str::from_utf8(&source)
        .map_err(|_| "provenance inventory must be UTF-8".to_owned())?;
    let mut paths = BTreeSet::new();
    let mut entries = 0_usize;
    for (index, line) in source.lines().enumerate() {
        entries += 1;
        if entries > 256 {
            return Err("provenance inventory exceeds 256 entries".to_owned());
        }
        let (sha256, path) = line
            .split_once("  ")
            .ok_or_else(|| format!("provenance inventory line {} is not canonical", index + 1))?;
        validate_sha256(sha256, "provenance entry sha256")?;
        validate_relative_path(path, "provenance entry")?;
        if !paths.insert(path) {
            return Err("provenance inventory contains a duplicate path".to_owned());
        }
        let referenced = root.join(path);
        let metadata = fs::symlink_metadata(&referenced).map_err(|error| {
            format!(
                "cannot inspect provenance entry {}: {error}",
                referenced.display()
            )
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("provenance entry must be a regular, non-symlink file".to_owned());
        }
        if metadata.len() > MAX_INVENTORY_BYTES {
            return Err("provenance entry exceeds the inventory file limit".to_owned());
        }
        if hash_file(&referenced, MAX_INVENTORY_BYTES)? != sha256 {
            return Err(format!("provenance entry SHA-256 mismatch: {path}"));
        }
    }
    if entries == 0 {
        return Err("provenance inventory must contain at least one entry".to_owned());
    }
    Ok(())
}

fn verify_desktop_platform_evidence(
    root: &Path,
    evidence: &DistributionEvidence,
) -> Result<(), String> {
    if evidence.surface_code != SurfaceCode::Desktop as u8
        || evidence.package_code == PackageCode::Archive as u8
    {
        return Ok(());
    }
    let file = evidence
        .platform_verification
        .as_ref()
        .ok_or_else(|| "Desktop certification requires platformVerification".to_owned())?;
    verify_file(
        root,
        file,
        MAX_MANIFEST_BYTES,
        "Desktop platform verification",
    )?;
    let source = read_regular(
        &root.join(&file.path),
        MAX_MANIFEST_BYTES,
        "Desktop platform verification",
    )?;
    let report: DesktopPlatformVerification = serde_json::from_slice(&source)
        .map_err(|error| format!("invalid Desktop platform verification JSON: {error}"))?;
    if report.schema_version != 1
        || report.surface_code != SurfaceCode::Desktop as u8
        || report.platform_code != evidence.platform_code
        || report.package_code != evidence.package_code
    {
        return Err(
            "Desktop platform verification does not match the certified installer".to_owned(),
        );
    }
    let platform = PlatformCode::parse(report.platform_code)?;
    let expected_signature = match platform {
        PlatformCode::Linux => DesktopSignatureKindCode::LinuxPackage,
        PlatformCode::Macos => DesktopSignatureKindCode::AppleDeveloperId,
        PlatformCode::Windows => DesktopSignatureKindCode::WindowsAuthenticode,
        PlatformCode::Android | PlatformCode::Ios => {
            return Err("Desktop platform verification requires a desktop platform".to_owned());
        }
    };
    let expected_notarization = if platform == PlatformCode::Macos {
        NotarizationResultCode::Passed
    } else {
        NotarizationResultCode::NotApplicable
    };
    if report.signature_kind_code != expected_signature as u8
        || report.signature_result_code != ResultCode::Passed as u8
        || report.notarization_result_code != expected_notarization as u8
        || report.sandbox_result_code != ResultCode::Passed as u8
        || report.update_recovery_result_code != ResultCode::Passed as u8
        || report.verified_at_unix == 0
    {
        return Err("Desktop platform verification contains an invalid trust result".to_owned());
    }
    validate_sha256(
        &report.publisher_identity_sha256,
        "Desktop publisher identity",
    )?;
    validate_sha256(&report.artifact_sha256, "Desktop verified artifact")?;
    if report.artifact_sha256 != evidence.artifact.sha256 {
        return Err(
            "Desktop platform verification digest does not match the certified installer"
                .to_owned(),
        );
    }
    verify_file(
        root,
        &report.publisher_certificate,
        MAX_MANIFEST_BYTES,
        "Desktop publisher certificate",
    )?;
    if report.publisher_certificate.sha256 != report.publisher_identity_sha256 {
        return Err("Desktop publisher identity does not match publisherCertificate".to_owned());
    }
    for (proof, label) in [
        (&report.signature_proof, "Desktop signature proof"),
        (&report.sandbox_proof, "Desktop sandbox proof"),
        (
            &report.update_recovery_proof,
            "Desktop update-recovery proof",
        ),
    ] {
        verify_file(root, proof, MAX_INVENTORY_BYTES, label)?;
    }
    match (platform, report.notarization_proof.as_ref()) {
        (PlatformCode::Macos, Some(proof)) => verify_file(
            root,
            proof,
            MAX_INVENTORY_BYTES,
            "Desktop notarization proof",
        )?,
        (PlatformCode::Macos, None) => {
            return Err("macOS Desktop verification requires notarizationProof".to_owned());
        }
        (_, Some(_)) => {
            return Err("notarizationProof is permitted only for macOS Desktop".to_owned());
        }
        (_, None) => {}
    }
    Ok(())
}

fn read_regular(path: &Path, limit: u64, label: &str) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect {label} {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("{label} must be a regular, non-symlink file"));
    }
    if metadata.len() > limit {
        return Err(format!("{label} exceeds the {limit}-byte limit"));
    }
    fs::read(path).map_err(|error| format!("cannot read {label} {}: {error}", path.display()))
}

fn hash_file(path: &Path, limit: u64) -> Result<String, String> {
    let mut file =
        File::open(path).map_err(|error| format!("cannot open {}: {error}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut bytes = 0_u64;
    loop {
        let read = match file.read(&mut buffer) {
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(format!("cannot hash {}: {error}", path.display())),
        };
        if read == 0 {
            break;
        }
        bytes = bytes.saturating_add(read as u64);
        if bytes > limit {
            return Err(format!("{} exceeds the hashing limit", path.display()));
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn validate_relative_path(value: &str, label: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.is_empty()
        || value.contains('\\')
        || path.is_absolute()
        || path.components().any(|component| match component {
            Component::Normal(part) => {
                let part = part.to_string_lossy();
                part.is_empty()
                    || !part.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
                    })
            }
            _ => true,
        })
    {
        return Err(format!(
            "{label} path must be a portable canonical relative path"
        ));
    }
    Ok(())
}

fn validate_revision(value: &str, name: &str) -> Result<(), String> {
    if !matches!(value.len(), 40 | 64)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(format!(
            "{name} must be a lowercase 40- or 64-character commit hash"
        ));
    }
    Ok(())
}

fn validate_sha256(value: &str, name: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(format!("{name} must be a lowercase SHA-256"));
    }
    Ok(())
}

fn valid_release_version(value: &str) -> bool {
    let Some(value) = value.strip_prefix('v') else {
        return false;
    };
    Version::parse(value)
        .is_ok_and(|version| version.build.is_empty() && version.to_string() == value)
}

fn hex_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_platform_verification_binds_installer_and_native_trust_results() {
        let root = std::env::temp_dir().join(format!(
            "pam-desktop-platform-evidence-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("files")).unwrap();
        let artifact = b"signed macOS installer";
        fs::write(root.join("files/candidate.dmg"), artifact).unwrap();
        let artifact_sha = hex_digest(artifact);
        let report_path = root.join("files/platform-verification.json");
        let proof = |name: &str, bytes: &[u8]| {
            fs::write(root.join("files").join(name), bytes).unwrap();
            serde_json::json!({
                "path": format!("files/{name}"),
                "sha256": hex_digest(bytes),
                "bytes": bytes.len()
            })
        };
        let publisher_certificate = proof("publisher.cer", b"publisher certificate DER");
        let publisher_identity = publisher_certificate["sha256"].as_str().unwrap().to_owned();
        let report = serde_json::json!({
            "schemaVersion": 1,
            "surfaceCode": 3,
            "platformCode": 2,
            "packageCode": 5,
            "signatureKindCode": 1,
            "signatureResultCode": 1,
            "notarizationResultCode": 1,
            "sandboxResultCode": 1,
            "updateRecoveryResultCode": 1,
            "publisherIdentitySha256": publisher_identity,
            "publisherCertificate": publisher_certificate,
            "artifactSha256": artifact_sha,
            "verifiedAtUnix": 1_800_000_000_u64,
            "signatureProof": proof("codesign.log", b"codesign valid"),
            "notarizationProof": proof("notarization.log", b"notarization valid"),
            "sandboxProof": proof("sandbox.log", b"sandbox denied undeclared access"),
            "updateRecoveryProof": proof("update-recovery.log", b"rollback restored")
        });
        let report_bytes = serde_json::to_vec(&report).unwrap();
        fs::write(&report_path, &report_bytes).unwrap();
        let mut evidence: DistributionEvidence = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "surfaceCode": 3,
            "platformCode": 2,
            "architectureCode": 2,
            "packageCode": 5,
            "revision": "1".repeat(40),
            "baselineRevision": "2".repeat(40),
            "hostImage": "macos-15",
            "generatedAtUnixMs": 1,
            "artifact": {"path": "files/candidate.dmg", "sha256": hex_digest(artifact), "bytes": artifact.len()},
            "baselineArtifact": {"path": "files/baseline.dmg", "sha256": "b".repeat(64), "bytes": 1},
            "dependencyInventory": {"path": "files/dependencies.sha256", "sha256": "c".repeat(64), "bytes": 1},
            "provenanceInventory": {"path": "files/provenance.sha256", "sha256": "d".repeat(64), "bytes": 1},
            "platformVerification": {
                "path": "files/platform-verification.json",
                "sha256": hex_digest(&report_bytes),
                "bytes": report_bytes.len()
            },
            "installedBytes": 1,
            "launchMillis": 1,
            "firstSuccessMillis": 1,
            "signingIdentitySha256": "f".repeat(64),
            "signingPublicKey": base64::engine::general_purpose::STANDARD.encode([0_u8; 32]),
            "manifestSignature": base64::engine::general_purpose::STANDARD.encode([0_u8; 64]),
            "checks": (1..=7).map(|code| serde_json::json!({"checkCode": code, "resultCode": 1, "durationMillis": 1})).collect::<Vec<_>>()
        }))
        .unwrap();
        validate_contract(&evidence).unwrap();
        verify_desktop_platform_evidence(&root, &evidence).unwrap();

        let produced_path = root.join("produced-platform-verification.json");
        let args = vec![
            "--artifact".into(),
            root.join("files/candidate.dmg").into_os_string(),
            "--platform-code".into(),
            "2".into(),
            "--package-code".into(),
            "5".into(),
            "--publisher-certificate".into(),
            root.join("files/publisher.cer").into_os_string(),
            "--signature-proof".into(),
            root.join("files/codesign.log").into_os_string(),
            "--notarization-proof".into(),
            root.join("files/notarization.log").into_os_string(),
            "--sandbox-proof".into(),
            root.join("files/sandbox.log").into_os_string(),
            "--update-recovery-proof".into(),
            root.join("files/update-recovery.log").into_os_string(),
            "--output".into(),
            produced_path.clone().into_os_string(),
        ];
        assert_eq!(
            desktop_report(args.clone().into_iter()).unwrap(),
            produced_path
        );
        let produced: DesktopPlatformVerification =
            serde_json::from_slice(&fs::read(&produced_path).unwrap()).unwrap();
        assert_eq!(produced.artifact_sha256, hex_digest(artifact));
        assert_eq!(
            produced.publisher_identity_sha256,
            produced.publisher_certificate.sha256
        );
        let outside = std::env::temp_dir().join(format!(
            "pam-desktop-outside-artifact-{}",
            std::process::id()
        ));
        fs::write(&outside, b"outside installer").unwrap();
        let mut outside_args = args.clone();
        outside_args[1] = outside.clone().into_os_string();
        outside_args[17] = root.join("outside-rejected.json").into_os_string();
        assert!(desktop_report(outside_args.into_iter()).is_err());
        fs::remove_file(outside).unwrap();
        assert!(desktop_report(args.into_iter()).is_err());

        let mut mismatched = report;
        mismatched["artifactSha256"] = serde_json::Value::String("a".repeat(64));
        let mismatched_bytes = serde_json::to_vec(&mismatched).unwrap();
        fs::write(&report_path, &mismatched_bytes).unwrap();
        let platform = evidence.platform_verification.as_mut().unwrap();
        platform.sha256 = hex_digest(&mismatched_bytes);
        platform.bytes = mismatched_bytes.len() as u64;
        assert!(verify_desktop_platform_evidence(&root, &evidence).is_err());

        mismatched["artifactSha256"] = serde_json::Value::String(hex_digest(artifact));
        mismatched["sandboxResultCode"] = serde_json::Value::from(2_u8);
        let failed_sandbox = serde_json::to_vec(&mismatched).unwrap();
        fs::write(&report_path, &failed_sandbox).unwrap();
        let platform = evidence.platform_verification.as_mut().unwrap();
        platform.sha256 = hex_digest(&failed_sandbox);
        platform.bytes = failed_sandbox.len() as u64;
        assert!(verify_desktop_platform_evidence(&root, &evidence).is_err());

        evidence.package_code = PackageCode::Archive as u8;
        assert!(validate_contract(&evidence).is_err());
        evidence.package_code = PackageCode::Dmg as u8;
        evidence.platform_verification = None;
        assert!(validate_contract(&evidence).is_err());
        evidence.package_code = PackageCode::Archive as u8;
        validate_contract(&evidence).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn signed_update_authorization_is_pinned_to_identity_and_target() {
        let (source, identity) =
            signed_update_manifest(7, 1, 1, "files/candidate.tar.gz", Some("v1.2.3"), true);
        let authorization =
            authorize_update_manifest(&source, &[&identity], "v1.2.3", 1, 1).unwrap();
        assert_eq!(authorization.artifact_sha256, "a".repeat(64));
        assert_eq!(authorization.artifact_bytes, 1024);
        assert!(
            authorize_update_manifest_at(&source, &[&identity], "v1.2.3", 1, 1, Some(1_001))
                .is_ok()
        );
        assert!(
            authorize_update_manifest_at(&source, &[&identity], "v1.2.3", 1, 1, Some(2_679_400))
                .is_err()
        );
        assert!(
            authorize_update_manifest_at(&source, &[&identity], "v1.2.3", 1, 1, Some(600)).is_err()
        );
        assert!(authorize_update_manifest(&source, &[&identity], "v1.2.4", 1, 1).is_err());

        assert!(authorize_update_manifest(&source, &[&"b".repeat(64)], "v1.2.3", 1, 1).is_err());
        assert!(authorize_update_manifest(&source, &[&identity], "v1.2.3", 2, 1).is_err());
        assert!(authorize_update_manifest(&source, &[&identity], "v1.2.3", 1, 2).is_err());
        assert!(authorize_update_manifest(&source, &[], "v1.2.3", 1, 1).is_err());
        assert!(
            authorize_update_manifest(&source, &[&identity, &identity], "v1.2.3", 1, 1).is_err()
        );
        assert!(
            authorize_update_manifest(
                &source,
                &[&identity, &"b".repeat(64), &"c".repeat(64)],
                "v1.2.3",
                1,
                1
            )
            .is_err()
        );
        assert!(
            authorize_update_manifest(&source, &[&identity.to_uppercase()], "v1.2.3", 1, 1)
                .is_err()
        );
        let (successor_source, successor_identity) =
            signed_update_manifest(8, 1, 1, "files/candidate.tar.gz", Some("v1.2.3"), true);
        assert!(
            authorize_update_manifest(
                &successor_source,
                &[&identity, &successor_identity],
                "v1.2.3",
                1,
                1
            )
            .is_ok()
        );

        let (wrong_path, _) =
            signed_update_manifest(7, 1, 1, "files/baseline.tar.gz", Some("v1.2.3"), true);
        assert!(authorize_update_manifest(&wrong_path, &[&identity], "v1.2.3", 1, 1).is_err());

        let mut tampered = source;
        let offset = tampered
            .windows(64)
            .position(|window| window == "a".repeat(64).as_bytes())
            .unwrap();
        tampered[offset] = b'c';
        assert!(authorize_update_manifest(&tampered, &[&identity], "v1.2.3", 1, 1).is_err());

        let (legacy_source, legacy_identity) =
            signed_update_manifest(7, 1, 1, "files/candidate.tar.gz", None, false);
        assert!(
            authorize_update_manifest(&legacy_source, &[&legacy_identity], "v1.2.3", 1, 1).is_err()
        );
        let (timeless_source, timeless_identity) =
            signed_update_manifest(7, 1, 1, "files/candidate.tar.gz", Some("v1.2.3"), false);
        assert!(
            authorize_update_manifest_at(
                &timeless_source,
                &[&timeless_identity],
                "v1.2.3",
                1,
                1,
                Some(1_001)
            )
            .is_err()
        );
    }

    fn signed_update_manifest(
        seed: u8,
        platform: u8,
        architecture: u8,
        path: &str,
        release_version: Option<&str>,
        freshness: bool,
    ) -> (Vec<u8>, String) {
        let signing_key = SigningKey::from_bytes(&[seed; 32]);
        let public_key = signing_key.verifying_key().to_bytes();
        let identity = hex_digest(&public_key);
        let mut document = serde_json::json!({
            "schemaVersion": 1,
            "surfaceCode": 1,
            "platformCode": platform,
            "architectureCode": architecture,
            "packageCode": 1,
            "revision": "1".repeat(40),
            "baselineRevision": "2".repeat(40),
            "hostImage": "test-host",
            "generatedAtUnixMs": 1,
            "artifact": {"path": path, "sha256": "a".repeat(64), "bytes": 1024},
            "baselineArtifact": {"path": "files/baseline.tar.gz", "sha256": "b".repeat(64), "bytes": 1024},
            "dependencyInventory": {"path": "files/dependencies.sha256", "sha256": "c".repeat(64), "bytes": 128},
            "provenanceInventory": {"path": "files/provenance.sha256", "sha256": "d".repeat(64), "bytes": 128},
            "installedBytes": 2048,
            "launchMillis": 1,
            "firstSuccessMillis": 1,
            "signingIdentitySha256": identity.clone(),
            "signingPublicKey": base64::engine::general_purpose::STANDARD.encode(public_key),
            "checks": (1..=7).map(|code| serde_json::json!({
                "checkCode": code,
                "resultCode": 1,
                "durationMillis": 1
            })).collect::<Vec<_>>()
        });
        if let Some(release_version) = release_version {
            document.as_object_mut().unwrap().insert(
                "releaseVersion".to_owned(),
                serde_json::Value::String(release_version.to_owned()),
            );
        }
        if freshness {
            document.as_object_mut().unwrap().insert(
                "issuedAtUnix".to_owned(),
                serde_json::Value::from(1_000_u64),
            );
            document.as_object_mut().unwrap().insert(
                "expiresAtUnix".to_owned(),
                serde_json::Value::from(2_679_400_u64),
            );
        }
        let payload = serde_json::to_vec(&document).unwrap();
        let signature = signing_key.sign(&payload);
        document.as_object_mut().unwrap().insert(
            "manifestSignature".to_owned(),
            serde_json::Value::String(
                base64::engine::general_purpose::STANDARD.encode(signature.to_bytes()),
            ),
        );
        (serde_json::to_vec_pretty(&document).unwrap(), identity)
    }
}
