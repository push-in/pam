use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Clone, Copy)]
#[repr(u8)]
enum Verdict {
    Pass = 1,
    Review = 2,
    Fail = 3,
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum Severity {
    Information = 1,
    Warning = 2,
    Critical = 3,
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum FindingKind {
    ComposerScript = 1,
    ComposerPlugin = 2,
    Maintainer = 3,
    License = 4,
    Provenance = 5,
    Advisory = 6,
    Capability = 7,
    Abandoned = 8,
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum AdvisoryState {
    Checked = 1,
    Skipped = 2,
}

#[derive(Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Policy {
    schema_version: u8,
    deny_scripts: bool,
    allowed_script_prefixes: Vec<String>,
    allowed_plugins: Vec<String>,
    allowed_maintainers: Vec<String>,
    allowed_licenses: Vec<String>,
    require_dist_reference: bool,
    reject_abandoned: bool,
    allowed_capabilities: Vec<u8>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            schema_version: 1,
            deny_scripts: false,
            allowed_script_prefixes: Vec::new(),
            allowed_plugins: Vec::new(),
            allowed_maintainers: Vec::new(),
            allowed_licenses: Vec::new(),
            require_dist_reference: false,
            reject_abandoned: true,
            allowed_capabilities: Vec::new(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Finding {
    kind: u8,
    severity: u8,
    subject: String,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    schema_version: u8,
    verdict: u8,
    advisory_state: u8,
    project: String,
    lock_sha256: String,
    packages: usize,
    capabilities: Vec<u8>,
    findings: Vec<Finding>,
}

pub struct Options {
    pub project: PathBuf,
    pub policy: Option<PathBuf>,
    pub capabilities: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub offline: bool,
}

pub fn run(executable: &OsStr, options: Options) -> Result<u8, String> {
    let project = fs::canonicalize(&options.project).map_err(|error| {
        format!(
            "cannot resolve supply-chain project {}: {error}",
            options.project.display()
        )
    })?;
    if !project.is_dir() {
        return Err(format!(
            "supply-chain project is not a directory: {}",
            project.display()
        ));
    }
    let manifest_path = project.join("composer.json");
    let lock_path = project.join("composer.lock");
    let manifest = read_json(&manifest_path, "Composer manifest")?;
    let lock_bytes = fs::read(&lock_path)
        .map_err(|error| format!("cannot read {}: {error}", lock_path.display()))?;
    let lock: Value = serde_json::from_slice(&lock_bytes)
        .map_err(|error| format!("invalid {}: {error}", lock_path.display()))?;
    let policy = options
        .policy
        .as_deref()
        .map(read_policy)
        .transpose()?
        .unwrap_or_default();
    if policy.schema_version != 1 {
        return Err(format!(
            "unsupported supply-chain policy schema {}",
            policy.schema_version
        ));
    }

    let mut findings = Vec::new();
    inspect_scripts(&manifest, &policy, &mut findings);
    let packages = packages(&lock)?;
    inspect_packages(&manifest, &packages, &policy, &mut findings);
    let capabilities = options
        .capabilities
        .as_deref()
        .map(|path| inspect_capabilities(path, &policy, &mut findings))
        .transpose()?
        .unwrap_or_default();
    let advisory_state = if options.offline {
        finding(
            &mut findings,
            FindingKind::Advisory,
            Severity::Warning,
            "composer-audit",
            "advisory lookup was explicitly skipped in offline mode",
        );
        AdvisoryState::Skipped
    } else {
        let audit = composer_audit(executable, &project)?;
        inspect_audit(&audit, &mut findings)?;
        AdvisoryState::Checked
    };
    findings.sort_by(|left, right| {
        (
            left.kind,
            left.severity,
            left.subject.as_str(),
            left.message.as_str(),
        )
            .cmp(&(
                right.kind,
                right.severity,
                right.subject.as_str(),
                right.message.as_str(),
            ))
    });
    let verdict = if findings
        .iter()
        .any(|finding| finding.severity == Severity::Critical as u8)
    {
        Verdict::Fail
    } else if findings
        .iter()
        .any(|finding| finding.severity == Severity::Warning as u8)
    {
        Verdict::Review
    } else {
        Verdict::Pass
    };
    let report = Report {
        schema_version: 1,
        verdict: verdict as u8,
        advisory_state: advisory_state as u8,
        project: manifest
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("unnamed/composer-project")
            .to_owned(),
        lock_sha256: format!("{:x}", Sha256::digest(lock_bytes)),
        packages: packages.len(),
        capabilities,
        findings,
    };
    let encoded = serde_json::to_vec_pretty(&report)
        .map_err(|error| format!("cannot serialize supply-chain report: {error}"))?;
    if let Some(output) = options.output {
        if output.exists() {
            return Err(format!(
                "refusing to overwrite supply-chain report {}",
                output.display()
            ));
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
        fs::write(&output, &encoded)
            .map_err(|error| format!("cannot write {}: {error}", output.display()))?;
        println!("{}", output.display());
    } else {
        println!("{}", String::from_utf8_lossy(&encoded));
    }
    Ok(u8::from(matches!(verdict, Verdict::Fail)))
}

fn read_json(path: &Path, label: &str) -> Result<Value, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("invalid {label}: {error}"))
}

fn read_policy(path: &Path) -> Result<Policy, String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "cannot read supply-chain policy {}: {error}",
            path.display()
        )
    })?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid supply-chain policy {}: {error}", path.display()))
}

fn inspect_scripts(manifest: &Value, policy: &Policy, findings: &mut Vec<Finding>) {
    let Some(scripts) = manifest.get("scripts").and_then(Value::as_object) else {
        return;
    };
    for (event, value) in scripts {
        for command in script_commands(value) {
            let suspicious = suspicious_script(&command);
            let allowed = policy.allowed_script_prefixes.is_empty()
                || policy
                    .allowed_script_prefixes
                    .iter()
                    .any(|prefix| command.starts_with(prefix));
            let severity = if policy.deny_scripts || !allowed || suspicious {
                Severity::Critical
            } else {
                Severity::Warning
            };
            finding(
                findings,
                FindingKind::ComposerScript,
                severity,
                event,
                &format!("Composer executes {command:?}"),
            );
        }
    }
}

fn script_commands(value: &Value) -> Vec<String> {
    match value {
        Value::String(command) => vec![command.clone()],
        Value::Array(commands) => commands
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn suspicious_script(command: &str) -> bool {
    let normalized = command.to_ascii_lowercase();
    [
        "curl ",
        "wget ",
        "sudo ",
        "rm -rf",
        "bash -c",
        "sh -c",
        "base64 -d",
        "eval ",
        "php -r",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn packages(lock: &Value) -> Result<Vec<&Value>, String> {
    let mut result = Vec::new();
    for key in ["packages", "packages-dev"] {
        let values = lock
            .get(key)
            .and_then(Value::as_array)
            .ok_or_else(|| format!("composer.lock field {key} must be an array"))?;
        result.extend(values);
    }
    Ok(result)
}

fn inspect_packages(
    manifest: &Value,
    packages: &[&Value],
    policy: &Policy,
    findings: &mut Vec<Finding>,
) {
    let allowed_by_composer = manifest
        .pointer("/config/allow-plugins")
        .and_then(Value::as_object);
    for package in packages {
        let name = package
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("<unknown-package>");
        if package.get("type").and_then(Value::as_str) == Some("composer-plugin") {
            let composer_allows = allowed_by_composer
                .and_then(|plugins| plugins.get(name))
                .and_then(Value::as_bool)
                == Some(true);
            let policy_allows = policy.allowed_plugins.is_empty()
                || policy.allowed_plugins.iter().any(|plugin| plugin == name);
            let (severity, message) = if composer_allows && policy_allows {
                (
                    Severity::Information,
                    "Composer plugin is explicitly allowed by both policies",
                )
            } else if composer_allows {
                (
                    Severity::Critical,
                    "Composer activates a plugin outside the PAM allowlist",
                )
            } else {
                (
                    Severity::Information,
                    "Composer plugin package is installed but execution is disabled",
                )
            };
            finding(
                findings,
                FindingKind::ComposerPlugin,
                severity,
                name,
                message,
            );
        }
        inspect_maintainers(package, name, policy, findings);
        inspect_licenses(package, name, policy, findings);
        let reference = package
            .pointer("/dist/reference")
            .or_else(|| package.pointer("/source/reference"))
            .and_then(Value::as_str);
        if reference.is_none_or(str::is_empty) {
            finding(
                findings,
                FindingKind::Provenance,
                if policy.require_dist_reference {
                    Severity::Critical
                } else {
                    Severity::Warning
                },
                name,
                "package has no immutable dist/source reference",
            );
        }
        if package.get("abandoned").is_some_and(|value| value != false) {
            finding(
                findings,
                FindingKind::Abandoned,
                if policy.reject_abandoned {
                    Severity::Critical
                } else {
                    Severity::Warning
                },
                name,
                "package is marked abandoned",
            );
        }
    }
}

fn inspect_maintainers(package: &Value, name: &str, policy: &Policy, findings: &mut Vec<Finding>) {
    if policy.allowed_maintainers.is_empty() {
        return;
    }
    let trusted = package
        .get("authors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|author| {
            ["name", "email"]
                .into_iter()
                .filter_map(|key| author.get(key).and_then(Value::as_str))
                .any(|identity| {
                    policy
                        .allowed_maintainers
                        .iter()
                        .any(|allowed| allowed.eq_ignore_ascii_case(identity))
                })
        });
    if !trusted {
        finding(
            findings,
            FindingKind::Maintainer,
            Severity::Critical,
            name,
            "no package author matches the trusted maintainer policy",
        );
    }
}

fn inspect_licenses(package: &Value, name: &str, policy: &Policy, findings: &mut Vec<Finding>) {
    let licenses = package
        .get("license")
        .and_then(Value::as_array)
        .map(|licenses| {
            licenses
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if licenses.is_empty() {
        finding(
            findings,
            FindingKind::License,
            Severity::Warning,
            name,
            "package does not declare an SPDX license",
        );
    } else if !policy.allowed_licenses.is_empty()
        && !licenses.iter().any(|license| {
            policy
                .allowed_licenses
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(license))
        })
    {
        finding(
            findings,
            FindingKind::License,
            Severity::Critical,
            name,
            "package license is outside the allowed policy",
        );
    }
}

fn inspect_capabilities(
    path: &Path,
    policy: &Policy,
    findings: &mut Vec<Finding>,
) -> Result<Vec<u8>, String> {
    let value = read_json(path, "capability manifest")?;
    if value.get("schemaVersion").and_then(Value::as_u64) != Some(1) {
        return Err("unsupported capability manifest schema".to_owned());
    }
    let capabilities = value
        .get("capabilities")
        .and_then(Value::as_array)
        .ok_or("capability manifest requires a capabilities array")?;
    let mut kinds = Vec::new();
    for capability in capabilities {
        let kind = capability
            .get("kind")
            .and_then(Value::as_u64)
            .and_then(|kind| u8::try_from(kind).ok())
            .filter(|kind| (1..=5).contains(kind))
            .ok_or("capability kind must be an integer from 1 to 5")?;
        kinds.push(kind);
        if !policy.allowed_capabilities.is_empty() && !policy.allowed_capabilities.contains(&kind) {
            finding(
                findings,
                FindingKind::Capability,
                Severity::Critical,
                &kind.to_string(),
                "capability is outside the allowed policy",
            );
        } else if matches!(kind, 3 | 4) {
            finding(
                findings,
                FindingKind::Capability,
                Severity::Warning,
                &kind.to_string(),
                "package requests unrestricted network or process access",
            );
        }
    }
    kinds.sort_unstable();
    kinds.dedup();
    Ok(kinds)
}

fn composer_audit(executable: &OsStr, project: &Path) -> Result<Value, String> {
    let output = Command::new(executable)
        .args([
            "composer",
            "audit",
            "--locked",
            "--format=json",
            "--no-interaction",
            "--no-plugins",
            "--no-scripts",
            "--working-dir",
        ])
        .arg(project)
        .output()
        .map_err(|error| format!("cannot execute Composer audit: {error}"))?;
    serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "Composer audit did not return valid JSON: {error}; {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
    })
}

fn inspect_audit(audit: &Value, findings: &mut Vec<Finding>) -> Result<(), String> {
    match audit.get("advisories") {
        Some(Value::Object(advisories)) => {
            for (package, values) in advisories {
                let values = values
                    .as_array()
                    .ok_or("Composer advisory package entry must be an array")?;
                for advisory in values {
                    let id = advisory
                        .get("advisoryId")
                        .or_else(|| advisory.get("cve"))
                        .and_then(Value::as_str)
                        .unwrap_or("unknown-advisory");
                    let title = advisory
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or("security advisory");
                    finding(
                        findings,
                        FindingKind::Advisory,
                        Severity::Critical,
                        package,
                        &format!("{id}: {title}"),
                    );
                }
            }
        }
        Some(Value::Array(advisories)) if advisories.is_empty() => {}
        _ => return Err("Composer audit JSON has invalid advisories".to_owned()),
    }
    match audit.get("abandoned") {
        Some(Value::Object(abandoned)) => {
            for package in abandoned.keys() {
                finding(
                    findings,
                    FindingKind::Abandoned,
                    Severity::Critical,
                    package,
                    "Composer audit reports this package as abandoned",
                );
            }
        }
        Some(Value::Array(abandoned)) if abandoned.is_empty() => {}
        _ => return Err("Composer audit JSON has invalid abandoned packages".to_owned()),
    }
    Ok(())
}

fn finding(
    findings: &mut Vec<Finding>,
    kind: FindingKind,
    severity: Severity,
    subject: &str,
    message: &str,
) {
    findings.push(Finding {
        kind: kind as u8,
        severity: severity as u8,
        subject: subject.to_owned(),
        message: message.to_owned(),
    });
}
