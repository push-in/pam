use std::ffi::OsStr;
use std::ffi::OsString;
use std::fs;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::project::ProjectKind;

pub fn release(
    executable: &OsStr,
    project: &Path,
    kind: ProjectKind,
    check_only: bool,
) -> Result<u8, String> {
    println!("PAM release gate for {}", kind.label());
    if kind == ProjectKind::Product {
        for (directory, arguments) in product_release_commands(check_only) {
            println!("\n$ (cd {directory} && pam {})", arguments.join(" "));
            let status = Command::new(executable)
                .args(&arguments)
                .current_dir(project.join(directory))
                .env("PAM_COLOR", "never")
                .status()
                .map_err(|error| format!("cannot run PAM Product release gate: {error}"))?;
            if !status.success() {
                return Err(format!(
                    "Product release gate `(cd {directory} && pam {})` failed with {status}",
                    arguments.join(" ")
                ));
            }
        }
        if check_only {
            println!("\nCross-surface release checks passed; no distributable was created.");
        } else {
            println!("\nCross-surface release candidate was indexed in dist/.");
        }
        return Ok(0);
    }
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

fn product_release_commands(check_only: bool) -> Vec<(&'static str, Vec<&'static str>)> {
    let mut commands = vec![
        (".", vec!["doctor", "--ci", "apps/server"]),
        (".", vec!["doctor", "--ci", "apps/native"]),
        (".", vec!["doctor", "--ci", "apps/desktop"]),
        ("apps/server", vec!["lint"]),
        ("apps/native", vec!["lint"]),
        ("apps/desktop", vec!["lint"]),
        ("apps/server", vec!["test"]),
        ("apps/native", vec!["test"]),
        ("apps/desktop", vec!["test"]),
        (".", vec!["packages/contracts/tests/contract.php"]),
    ];
    if !check_only {
        commands.push((".", vec!["package"]));
        commands.push((".", vec!["release:verify"]));
    }
    commands
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

pub fn package_product(
    project: &Path,
    arguments: impl Iterator<Item = OsString>,
) -> Result<u8, String> {
    let mut output = project.join("dist");
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
            option => return Err(format!("unknown product package option: {option}")),
        }
    }

    let mut artifacts = Vec::new();
    for (surface_code, application) in [(1_u8, "server"), (2, "native"), (3, "desktop")] {
        let directory = project.join("apps").join(application).join("dist");
        let mut surface_artifacts = Vec::new();
        let entries = fs::read_dir(&directory).map_err(|error| {
            format!(
                "{} artifacts are missing at {}: {error}; run `pam package` inside apps/{application} first",
                application,
                directory.display()
            )
        })?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                format!("cannot inspect {} artifacts: {error}", directory.display())
            })?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
            if metadata.file_type().is_symlink() {
                return Err(format!(
                    "product release artifact cannot be a symbolic link: {}",
                    path.display()
                ));
            }
            if !metadata.is_file() {
                continue;
            }
            let filename = entry.file_name();
            let filename = filename.to_str().ok_or_else(|| {
                format!(
                    "product release artifact name must be valid UTF-8: {}",
                    path.display()
                )
            })?;
            if filename.starts_with('.') || filename.ends_with(".sha256") {
                continue;
            }
            if filename.len() > 200
                || !filename.chars().enumerate().all(|(index, character)| {
                    character.is_ascii_alphanumeric()
                        || (index > 0 && matches!(character, '.' | '_' | '-'))
                })
            {
                return Err(format!(
                    "product release artifact name is not portable: {}",
                    path.display()
                ));
            }
            let relative = path.strip_prefix(project).map_err(|_| {
                format!("product artifact escapes the workspace: {}", path.display())
            })?;
            let relative = relative.to_str().ok_or_else(|| {
                format!(
                    "product artifact path must be valid UTF-8: {}",
                    path.display()
                )
            })?;
            let modified = metadata.modified().ok();
            let (sha256, size_bytes) = sha256_file(&path)?;
            let after = fs::symlink_metadata(&path)
                .map_err(|error| format!("cannot recheck {}: {error}", path.display()))?;
            if after.file_type().is_symlink()
                || after.len() != metadata.len()
                || (modified.is_some() && after.modified().ok() != modified)
                || size_bytes != metadata.len()
            {
                return Err(format!(
                    "product artifact changed while it was hashed: {}",
                    path.display()
                ));
            }
            surface_artifacts.push(serde_json::json!({
                "surfaceCode": surface_code,
                "path": relative.replace('\\', "/"),
                "sizeBytes": size_bytes,
                "sha256": sha256,
            }));
        }
        if surface_artifacts.is_empty() {
            return Err(format!(
                "no distributable found in {}; run `pam package` inside apps/{application} first",
                directory.display()
            ));
        }
        artifacts.append(&mut surface_artifacts);
    }
    artifacts.sort_by(|left, right| {
        left["path"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["path"].as_str().unwrap_or_default())
    });
    if artifacts.len() > 64 {
        return Err("product releases support at most 64 top-level artifacts".to_owned());
    }

    let (name, version) = package_identity(project)?;
    let mut document = serde_json::json!({
        "schemaVersionCode": 1,
        "name": name,
        "version": version,
        "artifacts": artifacts,
    });
    if let Some(evidence) = product_visual_evidence(project)? {
        document["visualEvidence"] = serde_json::Value::Array(evidence);
    }
    let mut encoded = serde_json::to_vec_pretty(&document)
        .map_err(|error| format!("cannot serialize product release manifest: {error}"))?;
    encoded.push(b'\n');
    fs::create_dir_all(&output)
        .map_err(|error| format!("cannot create {}: {error}", output.display()))?;
    let manifest = output.join("product-release.json");
    write_new_file(&manifest, &encoded)?;
    let checksum = output.join("product-release.json.sha256");
    let digest = Sha256::digest(&encoded);
    if let Err(error) = write_new_file(
        &checksum,
        format!("{digest:x}  product-release.json\n").as_bytes(),
    ) {
        let _ = fs::remove_file(&manifest);
        return Err(error);
    }
    println!("Product release manifest {}", manifest.display());
    println!("Checksum {}", checksum.display());
    Ok(0)
}

const PRODUCT_MANIFEST_LIMIT: u64 = 1024 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProductReleaseManifest {
    schema_version_code: u8,
    name: String,
    version: String,
    artifacts: Vec<ProductReleaseArtifact>,
    #[serde(default)]
    visual_evidence: Vec<ProductVisualEvidence>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProductReleaseArtifact {
    surface_code: u8,
    path: String,
    size_bytes: u64,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProductVisualEvidence {
    mode_code: u8,
    token_sha256: String,
    report: ProductEvidenceFile,
    captures: Vec<ProductEvidenceCapture>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProductEvidenceFile {
    path: String,
    size_bytes: u64,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProductEvidenceCapture {
    surface_code: u8,
    path: String,
    size_bytes: u64,
    sha256: String,
}

pub fn verify_product_release(project: &Path, manifest: &Path) -> Result<u8, String> {
    let project = project
        .canonicalize()
        .map_err(|error| format!("cannot resolve Product workspace: {error}"))?;
    let manifest_metadata = fs::symlink_metadata(manifest)
        .map_err(|error| format!("cannot inspect {}: {error}", manifest.display()))?;
    if manifest_metadata.file_type().is_symlink() || !manifest_metadata.is_file() {
        return Err(format!(
            "Product release manifest must be a regular file: {}",
            manifest.display()
        ));
    }
    if manifest_metadata.len() > PRODUCT_MANIFEST_LIMIT {
        return Err(format!(
            "Product release manifest exceeds the {PRODUCT_MANIFEST_LIMIT} byte limit"
        ));
    }
    let bytes = fs::read(manifest)
        .map_err(|error| format!("cannot read {}: {error}", manifest.display()))?;
    let manifest_name = manifest
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| "Product release manifest name must be valid UTF-8".to_owned())?;
    let sidecar = manifest.with_file_name(format!("{manifest_name}.sha256"));
    verify_manifest_sidecar(&sidecar, manifest_name, &bytes)?;

    let release: ProductReleaseManifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid Product release manifest: {error}"))?;
    if release.schema_version_code != 1 {
        return Err(format!(
            "unsupported Product release schema version code {}",
            release.schema_version_code
        ));
    }
    if release.name.is_empty() || release.name.len() > 200 {
        return Err("Product release name must contain 1 to 200 bytes".to_owned());
    }
    if release.version.is_empty() || release.version.len() > 200 {
        return Err("Product release version must contain 1 to 200 bytes".to_owned());
    }
    if !(3..=64).contains(&release.artifacts.len()) {
        return Err("Product release must contain 3 to 64 artifacts".to_owned());
    }

    let mut previous = None::<&str>;
    let mut surfaces = [false; 3];
    for artifact in &release.artifacts {
        if let Some(previous) = previous {
            if artifact.path.as_str() <= previous {
                return Err("Product release artifact paths must be unique and sorted".to_owned());
            }
        }
        previous = Some(&artifact.path);
        let surface = match artifact.surface_code {
            1 => "server",
            2 => "native",
            3 => "desktop",
            code => return Err(format!("unknown Product surface code {code}")),
        };
        surfaces[usize::from(artifact.surface_code - 1)] = true;
        let expected_prefix = format!("apps/{surface}/dist/");
        let filename = artifact
            .path
            .strip_prefix(&expected_prefix)
            .ok_or_else(|| {
                format!(
                    "artifact path does not match surface code {}: {}",
                    artifact.surface_code, artifact.path
                )
            })?;
        if !portable_artifact_name(filename) {
            return Err(format!("artifact path is not portable: {}", artifact.path));
        }
        if artifact.sha256.len() != 64
            || !artifact
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(format!("artifact SHA-256 is invalid: {}", artifact.path));
        }

        let path = project.join(&artifact.path);
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!(
                "artifact must be a regular file: {}",
                path.display()
            ));
        }
        let resolved = path
            .canonicalize()
            .map_err(|error| format!("cannot resolve {}: {error}", path.display()))?;
        if !resolved.starts_with(&project) {
            return Err(format!(
                "artifact escapes the Product workspace: {}",
                path.display()
            ));
        }
        let modified = metadata.modified().ok();
        let (digest, size) = sha256_file(&resolved)?;
        let after = fs::symlink_metadata(&resolved)
            .map_err(|error| format!("cannot recheck {}: {error}", resolved.display()))?;
        if after.file_type().is_symlink()
            || after.len() != metadata.len()
            || (modified.is_some() && after.modified().ok() != modified)
        {
            return Err(format!(
                "artifact changed while it was verified: {}",
                artifact.path
            ));
        }
        if size != artifact.size_bytes {
            return Err(format!("artifact size mismatch: {}", artifact.path));
        }
        if digest != artifact.sha256 {
            return Err(format!("artifact SHA-256 mismatch: {}", artifact.path));
        }
    }
    if surfaces != [true, true, true] {
        return Err(
            "Product release must include Server, Native, and Desktop artifacts".to_owned(),
        );
    }
    verify_visual_evidence(&project, &release.visual_evidence)?;

    println!(
        "Verified Product release {} {} ({} artifacts, {} visual modes)",
        release.name,
        release.version,
        release.artifacts.len(),
        release.visual_evidence.len(),
    );
    Ok(0)
}

fn product_visual_evidence(project: &Path) -> Result<Option<Vec<serde_json::Value>>, String> {
    let reports = [
        (1_u8, "light", "artifacts/product-visual-light.json"),
        (2_u8, "dark", "artifacts/product-visual-dark.json"),
    ];
    let present = reports
        .iter()
        .filter(|(_, _, path)| project.join(path).is_file())
        .count();
    if present == 0 {
        return Ok(None);
    }
    if present != reports.len() {
        return Err("visual certification requires both light and dark reports".to_owned());
    }
    let token_path = project.join("packages/contracts/design-tokens.json");
    let (token_sha256, _) = sha256_regular_file(project, &token_path, 64 * 1024)?;
    let mut evidence = Vec::new();
    for (mode_code, name, report_relative) in reports {
        let report_path = project.join(report_relative);
        let report_bytes = bounded_regular_file(project, &report_path, 1024 * 1024)?;
        let report: serde_json::Value = serde_json::from_slice(&report_bytes)
            .map_err(|error| format!("invalid visual evidence {report_relative}: {error}"))?;
        validate_visual_report(&report, mode_code, &token_sha256)?;
        let (report_sha256, report_size) = sha256_regular_file(project, &report_path, 1024 * 1024)?;
        let mut captures = Vec::new();
        for (surface_code, surface) in [(2_u8, "native"), (3_u8, "desktop")] {
            let relative =
                format!("apps/{surface}/artifacts/screenshots/product-{surface}-{name}.png");
            let path = project.join(&relative);
            let (sha256, size_bytes) = sha256_regular_file(project, &path, 16 * 1024 * 1024)?;
            let reported = report["captures"]
                .as_array()
                .and_then(|items| {
                    items
                        .iter()
                        .find(|item| item["surfaceCode"] == surface_code)
                })
                .ok_or_else(|| format!("visual report is missing surface code {surface_code}"))?;
            if reported["sha256"] != sha256 || reported["bytes"] != size_bytes {
                return Err(format!("visual report does not match capture {relative}"));
            }
            captures.push(serde_json::json!({
                "surfaceCode": surface_code,
                "path": relative,
                "sizeBytes": size_bytes,
                "sha256": sha256,
            }));
        }
        evidence.push(serde_json::json!({
            "modeCode": mode_code,
            "tokenSha256": token_sha256,
            "report": {
                "path": report_relative,
                "sizeBytes": report_size,
                "sha256": report_sha256,
            },
            "captures": captures,
        }));
    }
    Ok(Some(evidence))
}

fn validate_visual_report(
    report: &serde_json::Value,
    mode_code: u8,
    token_sha256: &str,
) -> Result<(), String> {
    let captures = report["captures"].as_array();
    if report["schemaVersion"] != 1
        || report["modeCode"] != mode_code
        || report["tokenSha256"] != token_sha256
        || report["toleranceChannelDelta"] != 12
        || report["passed"] != true
        || captures.is_none_or(|items| items.len() != 2)
    {
        return Err(format!(
            "visual evidence mode {mode_code} did not pass its contract"
        ));
    }
    let roles = [
        "background",
        "surface",
        "foreground",
        "primary",
        "focus",
        "danger",
    ];
    for (index, capture) in captures
        .expect("validated capture array")
        .iter()
        .enumerate()
    {
        let surface_code = u8::try_from(index + 2).expect("two visual surfaces");
        let width = capture["width"].as_u64().unwrap_or(0);
        let height = capture["height"].as_u64().unwrap_or(0);
        let anchors = capture["anchors"].as_array();
        if capture["surfaceCode"] != surface_code
            || capture["passed"] != true
            || width == 0
            || height == 0
            || width.saturating_mul(height) > 4_000_000
            || capture["bytes"]
                .as_u64()
                .is_none_or(|bytes| bytes == 0 || bytes > 16 * 1024 * 1024)
            || capture["sha256"]
                .as_str()
                .is_none_or(|digest| !valid_sha256(digest))
            || anchors.is_none_or(|items| items.len() != roles.len())
        {
            return Err(format!(
                "visual capture contract failed for surface {surface_code}"
            ));
        }
        for (anchor, role) in anchors.expect("validated anchors").iter().zip(roles) {
            if anchor["role"] != role
                || anchor["passed"] != true
                || anchor["matchingPixels"].as_u64().unwrap_or(0)
                    < anchor["requiredPixels"].as_u64().unwrap_or(1)
            {
                return Err(format!(
                    "visual anchor {role} failed for surface {surface_code}"
                ));
            }
        }
    }
    Ok(())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn verify_visual_evidence(
    project: &Path,
    evidence: &[ProductVisualEvidence],
) -> Result<(), String> {
    if evidence.is_empty() {
        return Ok(());
    }
    if evidence.len() != 2 {
        return Err("Product visual evidence must contain light and dark modes".to_owned());
    }
    let token_path = project.join("packages/contracts/design-tokens.json");
    let (token_sha256, _) = sha256_regular_file(project, &token_path, 64 * 1024)?;
    for (index, item) in evidence.iter().enumerate() {
        let expected_mode = u8::try_from(index + 1).expect("two visual modes");
        let mode_name = if expected_mode == 1 { "light" } else { "dark" };
        if item.mode_code != expected_mode || item.token_sha256 != token_sha256 {
            return Err("Product visual evidence modes or design tokens do not match".to_owned());
        }
        if item.report.path != format!("artifacts/product-visual-{mode_name}.json") {
            return Err("Product visual evidence report path is not canonical".to_owned());
        }
        let report_path = verified_evidence_path(project, &item.report.path, "json")?;
        let (report_sha256, report_size) = sha256_regular_file(project, &report_path, 1024 * 1024)?;
        if report_sha256 != item.report.sha256 || report_size != item.report.size_bytes {
            return Err("Product visual evidence report digest does not match".to_owned());
        }
        let report: serde_json::Value =
            serde_json::from_slice(&bounded_regular_file(project, &report_path, 1024 * 1024)?)
                .map_err(|error| format!("invalid Product visual evidence report: {error}"))?;
        validate_visual_report(&report, expected_mode, &token_sha256)?;
        if item.captures.len() != 2 {
            return Err("Product visual evidence requires Native and Desktop captures".to_owned());
        }
        for (capture_index, capture) in item.captures.iter().enumerate() {
            let expected_surface = u8::try_from(capture_index + 2).expect("two visual surfaces");
            let surface_name = if expected_surface == 2 {
                "native"
            } else {
                "desktop"
            };
            if capture.surface_code != expected_surface {
                return Err("Product visual capture surface codes must be sorted".to_owned());
            }
            if capture.path
                != format!(
                    "apps/{surface_name}/artifacts/screenshots/product-{surface_name}-{mode_name}.png"
                )
            {
                return Err("Product visual capture path is not canonical".to_owned());
            }
            let path = verified_evidence_path(project, &capture.path, "png")?;
            let (sha256, size) = sha256_regular_file(project, &path, 16 * 1024 * 1024)?;
            if sha256 != capture.sha256 || size != capture.size_bytes {
                return Err(format!(
                    "Product visual capture digest mismatch: {}",
                    capture.path
                ));
            }
            let reported = &report["captures"][capture_index];
            if reported["surfaceCode"] != capture.surface_code
                || reported["sha256"] != capture.sha256
                || reported["bytes"] != capture.size_bytes
            {
                return Err(format!(
                    "Product visual report does not bind {}",
                    capture.path
                ));
            }
        }
    }
    Ok(())
}

fn verified_evidence_path(
    project: &Path,
    relative: &str,
    extension: &str,
) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path.extension() != Some(OsStr::new(extension))
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
        || !(relative.starts_with("artifacts/")
            || relative.starts_with("apps/native/artifacts/")
            || relative.starts_with("apps/desktop/artifacts/"))
    {
        return Err(format!("unsafe Product evidence path: {relative}"));
    }
    Ok(project.join(path))
}

fn bounded_regular_file(project: &Path, path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > limit {
        return Err(format!(
            "evidence file must be a bounded regular file: {}",
            path.display()
        ));
    }
    let resolved = path
        .canonicalize()
        .map_err(|error| format!("cannot resolve {}: {error}", path.display()))?;
    if !resolved.starts_with(project) {
        return Err(format!(
            "evidence file escapes the Product workspace: {}",
            path.display()
        ));
    }
    fs::read(&resolved).map_err(|error| format!("cannot read {}: {error}", path.display()))
}

fn sha256_regular_file(project: &Path, path: &Path, limit: u64) -> Result<(String, u64), String> {
    let bytes = bounded_regular_file(project, path, limit)?;
    Ok((format!("{:x}", Sha256::digest(&bytes)), bytes.len() as u64))
}

fn verify_manifest_sidecar(
    sidecar: &Path,
    manifest_name: &str,
    bytes: &[u8],
) -> Result<(), String> {
    let metadata = fs::symlink_metadata(sidecar)
        .map_err(|error| format!("cannot inspect {}: {error}", sidecar.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 512 {
        return Err(format!(
            "manifest checksum must be a small regular file: {}",
            sidecar.display()
        ));
    }
    let expected = format!("{:x}  {manifest_name}\n", Sha256::digest(bytes));
    let actual =
        fs::read(sidecar).map_err(|error| format!("cannot read {}: {error}", sidecar.display()))?;
    if actual != expected.as_bytes() {
        return Err("Product release manifest checksum mismatch".to_owned());
    }
    Ok(())
}

fn portable_artifact_name(filename: &str) -> bool {
    !filename.is_empty()
        && filename.len() <= 200
        && filename.chars().enumerate().all(|(index, character)| {
            character.is_ascii_alphanumeric() || (index > 0 && matches!(character, '.' | '_' | '-'))
        })
}

fn sha256_file(path: &Path) -> Result<(String, u64), String> {
    let mut file = fs::File::open(path)
        .map_err(|error| format!("cannot open {} for hashing: {error}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut size = 0_u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("cannot hash {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
        size = size
            .checked_add(read as u64)
            .ok_or_else(|| format!("artifact is too large to index: {}", path.display()))?;
    }
    Ok((format!("{:x}", digest.finalize()), size))
}

fn write_new_file(path: &Path, contents: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("refusing to overwrite {}: {error}", path.display()))?;
    file.write_all(contents)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
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
        for kind in [
            ProjectKind::Api,
            ProjectKind::Laravel,
            ProjectKind::Raw,
            ProjectKind::Product,
        ] {
            assert_eq!(
                release_commands(kind, false).last().unwrap(),
                &vec!["package"]
            );
        }
    }

    #[test]
    fn product_release_gates_each_surface_and_the_shared_contract() {
        let checks = product_release_commands(true);
        for application in ["apps/server", "apps/native", "apps/desktop"] {
            assert!(checks.contains(&(".", vec!["doctor", "--ci", application])));
            assert!(checks.contains(&(application, vec!["lint"])));
            assert!(checks.contains(&(application, vec!["test"])));
        }
        assert!(checks.contains(&(".", vec!["packages/contracts/tests/contract.php"])));
        assert!(
            !checks
                .iter()
                .any(|(_, command)| command == &vec!["package"])
        );
        let release = product_release_commands(false);
        assert_eq!(release[release.len() - 2], (".", vec!["package"]));
        assert_eq!(release.last(), Some(&(".", vec!["release:verify"])));
    }
}
