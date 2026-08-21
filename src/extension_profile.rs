use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

const MAX_COMPOSER_DOCUMENT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_REQUIREMENTS: usize = 4096;
const MODULE_INVENTORY_SCRIPT: &str =
    "$m=get_loaded_extensions();sort($m,SORT_STRING);echo json_encode($m,JSON_THROW_ON_ERROR);";
const COMPOSER_CONTENT_HASH_SCRIPT: &str = r#"$c=json_decode(file_get_contents($argv[1]),true,512,JSON_THROW_ON_ERROR);$k=['name','version','require','require-dev','conflict','replace','provide','minimum-stability','prefer-stable','repositories','extra'];$r=array_intersect_key($c,array_flip($k));if(isset($c['config']['platform'])){$r['config']['platform']=$c['config']['platform'];}ksort($r);echo hash('md5',json_encode($r,JSON_THROW_ON_ERROR,512));"#;

#[derive(Clone, Copy, Debug, Serialize)]
#[repr(u8)]
enum RequirementSourceCode {
    RootProduction = 1,
    RootDevelopment = 2,
    LockedProduction = 3,
    LockedDevelopment = 4,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[repr(u8)]
enum ProfileStateCode {
    Ready = 1,
    MissingRequiredExtension = 2,
    NoDynamicExtensionsRequired = 3,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RequirementSource {
    source_code: u8,
    package: String,
    constraint: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExtensionRequirement {
    extension: String,
    sources: Vec<RequirementSource>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExtensionProfileReport {
    schema_version: u8,
    state_code: u8,
    ready: bool,
    include_dev: bool,
    project_root: String,
    manifest_sha256: String,
    lock_sha256: String,
    lock_content_hash: String,
    requirements: Vec<ExtensionRequirement>,
    provided_extensions: Vec<String>,
    builtin_extensions: Vec<String>,
    selected_extensions: Vec<String>,
    missing_extensions: Vec<String>,
    arguments: Vec<String>,
}

pub fn run(executable: &OsStr, arguments: Vec<OsString>) -> Result<u8, String> {
    let mut include_dev = true;
    let mut json = false;
    let mut target = None;
    for argument in arguments {
        match argument.to_string_lossy().as_ref() {
            "--no-dev" => include_dev = false,
            "--json" => json = true,
            option if option.starts_with('-') => {
                return Err(format!("unknown extensions option: {option}"));
            }
            _ if target.is_none() => target = Some(PathBuf::from(argument)),
            _ => return Err("extensions accepts at most one project path".to_owned()),
        }
    }
    let target = target.unwrap_or_else(|| PathBuf::from("."));
    let root = discover_root(&target)?;
    let lock_content_hash = verify_lock_freshness(executable, &root)?;
    let compatible = loaded_extensions(executable, false)?;
    let baseline = loaded_extensions(executable, true)?;
    let report = build_report(
        &root,
        include_dev,
        &compatible,
        &baseline,
        lock_content_hash,
    )?;
    let ready = report.ready;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
        );
    } else {
        print_human(&report);
    }
    Ok(if ready { 0 } else { 1 })
}

fn discover_root(target: &Path) -> Result<PathBuf, String> {
    let target = fs::canonicalize(target)
        .map_err(|error| format!("cannot resolve {}: {error}", target.display()))?;
    let start = if target.is_dir() {
        target
    } else {
        target
            .parent()
            .ok_or_else(|| format!("{} has no parent directory", target.display()))?
            .to_path_buf()
    };
    start
        .ancestors()
        .find(|directory| directory.join("composer.json").is_file())
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("no composer.json found from {}", start.display()))
}

fn loaded_extensions(executable: &OsStr, isolated: bool) -> Result<Vec<String>, String> {
    let mut command = Command::new(executable);
    command.args(["-r", MODULE_INVENTORY_SCRIPT]);
    command.env_remove("PAM_INI_ENTRIES");
    if isolated {
        command.env("PHP_INI_SCAN_DIR", "");
    }
    let output = command
        .output()
        .map_err(|error| format!("cannot inspect PHP extension inventory: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "PHP extension inventory failed with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    if output.stdout.len() > 64 * 1024 {
        return Err("PHP extension inventory exceeded 64 KiB".to_owned());
    }
    let modules: Vec<String> = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("invalid PHP extension inventory: {error}"))?;
    if modules.len() > 1024 || modules.iter().any(|module| module.len() > 128) {
        return Err("PHP extension inventory exceeds safety bounds".to_owned());
    }
    Ok(modules)
}

fn verify_lock_freshness(executable: &OsStr, root: &Path) -> Result<String, String> {
    let manifest_path = root.join("composer.json");
    let lock_path = root.join("composer.lock");
    let manifest_before = read_document(&manifest_path)?;
    let lock_bytes = read_document(&lock_path).map_err(|error| {
        format!("{error}; run `pam composer update --lock` before deriving a profile")
    })?;
    let lock: Value = serde_json::from_slice(&lock_bytes)
        .map_err(|error| format!("invalid {}: {error}", lock_path.display()))?;
    let recorded = lock
        .get("content-hash")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "composer.lock has no content-hash; run `pam composer update --lock`".to_owned()
        })?;
    if recorded.len() != 32
        || !recorded
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(
            "composer.lock content-hash must be 32 lowercase hexadecimal characters".to_owned(),
        );
    }
    let output = Command::new(executable)
        .args(["-r", COMPOSER_CONTENT_HASH_SCRIPT])
        .arg(&manifest_path)
        .env_remove("PAM_INI_ENTRIES")
        .output()
        .map_err(|error| format!("cannot verify Composer content-hash: {error}"))?;
    if !output.status.success() || output.stdout.len() > 64 {
        return Err(format!(
            "Composer content-hash verification failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let calculated = String::from_utf8(output.stdout)
        .map_err(|_| "calculated Composer content-hash is not UTF-8".to_owned())?;
    if calculated.len() != 32
        || !calculated
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("calculated Composer content-hash is invalid".to_owned());
    }
    let manifest_after = read_document(&manifest_path)?;
    if manifest_before != manifest_after {
        return Err("composer.json changed while its content-hash was verified; retry".to_owned());
    }
    if calculated != recorded {
        return Err(format!(
            "composer.lock is stale for composer.json; run `pam composer update --lock` (expected content-hash {calculated})"
        ));
    }
    Ok(calculated)
}

fn build_report(
    root: &Path,
    include_dev: bool,
    compatible_modules: &[String],
    baseline_modules: &[String],
    lock_content_hash: String,
) -> Result<ExtensionProfileReport, String> {
    let manifest_path = root.join("composer.json");
    let lock_path = root.join("composer.lock");
    let manifest_bytes = read_document(&manifest_path)?;
    let lock_bytes = read_document(&lock_path).map_err(|error| {
        format!("{error}; run `pam composer update --lock` before deriving a profile")
    })?;
    let manifest: Value = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("invalid {}: {error}", manifest_path.display()))?;
    let lock: Value = serde_json::from_slice(&lock_bytes)
        .map_err(|error| format!("invalid {}: {error}", lock_path.display()))?;
    let mut requirements = BTreeMap::<String, Vec<RequirementSource>>::new();
    let mut provided = BTreeSet::new();
    collect_package(
        &manifest,
        "root",
        RequirementSourceCode::RootProduction,
        "require",
        &mut requirements,
        &mut provided,
    )?;
    collect_provisions(&manifest, &mut provided)?;
    if include_dev {
        collect_package(
            &manifest,
            "root",
            RequirementSourceCode::RootDevelopment,
            "require-dev",
            &mut requirements,
            &mut provided,
        )?;
    }
    collect_locked_packages(
        &lock,
        "packages",
        RequirementSourceCode::LockedProduction,
        &mut requirements,
        &mut provided,
    )?;
    if include_dev {
        collect_locked_packages(
            &lock,
            "packages-dev",
            RequirementSourceCode::LockedDevelopment,
            &mut requirements,
            &mut provided,
        )?;
    }
    if requirements.len() > MAX_REQUIREMENTS {
        return Err(format!(
            "Composer extension requirements exceed {MAX_REQUIREMENTS} entries"
        ));
    }
    for sources in requirements.values_mut() {
        sources.sort_by(|left, right| {
            (left.source_code, &left.package, &left.constraint).cmp(&(
                right.source_code,
                &right.package,
                &right.constraint,
            ))
        });
    }

    let compatible = module_map(compatible_modules);
    let baseline = module_map(baseline_modules);
    let mut builtin = Vec::new();
    let mut selected = Vec::new();
    let mut missing = Vec::new();
    for extension in requirements.keys().filter(|name| !provided.contains(*name)) {
        if baseline.contains_key(extension) {
            builtin.push(extension.clone());
        } else if let Some(loader) = compatible.get(extension) {
            if loader.is_empty()
                || loader.len() > 64
                || !loader
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
            {
                return Err(format!(
                    "Composer extension {extension} resolves to an unsafe PAM loader name"
                ));
            }
            selected.push(loader.clone());
        } else {
            missing.push(extension.clone());
        }
    }
    selected.sort();
    selected.dedup();
    if selected.len() > 64 {
        return Err("derived profile exceeds PAM's 64-extension safety limit".to_owned());
    }
    let arguments = selected
        .iter()
        .flat_map(|extension| ["--php-extension".to_owned(), extension.clone()])
        .collect();
    let ready = missing.is_empty();
    let state_code = if !ready {
        ProfileStateCode::MissingRequiredExtension
    } else if selected.is_empty() {
        ProfileStateCode::NoDynamicExtensionsRequired
    } else {
        ProfileStateCode::Ready
    };
    Ok(ExtensionProfileReport {
        schema_version: 1,
        state_code: state_code as u8,
        ready,
        include_dev,
        project_root: root.display().to_string(),
        manifest_sha256: hex_sha256(&manifest_bytes),
        lock_sha256: hex_sha256(&lock_bytes),
        lock_content_hash,
        requirements: requirements
            .into_iter()
            .map(|(extension, sources)| ExtensionRequirement { extension, sources })
            .collect(),
        provided_extensions: provided.into_iter().collect(),
        builtin_extensions: builtin,
        selected_extensions: selected,
        missing_extensions: missing,
        arguments,
    })
}

fn read_document(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "{} must be a regular non-symlink file",
            path.display()
        ));
    }
    if metadata.len() > MAX_COMPOSER_DOCUMENT_BYTES {
        return Err(format!("{} exceeds 4 MiB", path.display()));
    }
    fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))
}

fn collect_locked_packages(
    lock: &Value,
    key: &str,
    source_code: RequirementSourceCode,
    requirements: &mut BTreeMap<String, Vec<RequirementSource>>,
    provided: &mut BTreeSet<String>,
) -> Result<(), String> {
    let packages = lock
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("composer.lock {key} must be an array"))?;
    if packages.len() > MAX_REQUIREMENTS {
        return Err(format!(
            "composer.lock {key} exceeds {MAX_REQUIREMENTS} packages"
        ));
    }
    for package in packages {
        let name = package
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("composer.lock {key} package has no valid name"))?;
        if name.len() > 255 {
            return Err("Composer package name exceeds 255 bytes".to_owned());
        }
        collect_package(
            package,
            name,
            source_code,
            "require",
            requirements,
            provided,
        )?;
        collect_provisions(package, provided)?;
    }
    Ok(())
}

fn collect_package(
    package: &Value,
    package_name: &str,
    source_code: RequirementSourceCode,
    requirement_key: &str,
    requirements: &mut BTreeMap<String, Vec<RequirementSource>>,
    _provided: &mut BTreeSet<String>,
) -> Result<(), String> {
    let Some(require) = package.get(requirement_key) else {
        return Ok(());
    };
    let require = require
        .as_object()
        .ok_or_else(|| format!("{package_name} {requirement_key} must be an object"))?;
    for (name, constraint) in require {
        let Some(extension) = composer_extension(name)? else {
            continue;
        };
        let constraint = constraint
            .as_str()
            .ok_or_else(|| format!("{package_name} requirement {name} must be a string"))?;
        if constraint.len() > 255 {
            return Err(format!(
                "{package_name} requirement {name} exceeds 255 bytes"
            ));
        }
        requirements
            .entry(extension)
            .or_default()
            .push(RequirementSource {
                source_code: source_code as u8,
                package: package_name.to_owned(),
                constraint: constraint.to_owned(),
            });
    }
    Ok(())
}

fn collect_provisions(package: &Value, provided: &mut BTreeSet<String>) -> Result<(), String> {
    for key in ["provide", "replace"] {
        let Some(values) = package.get(key) else {
            continue;
        };
        let values = values
            .as_object()
            .ok_or_else(|| format!("Composer {key} must be an object"))?;
        for name in values.keys() {
            if let Some(extension) = composer_extension(name)? {
                provided.insert(extension);
            }
        }
    }
    Ok(())
}

fn composer_extension(name: &str) -> Result<Option<String>, String> {
    let Some(extension) = name.strip_prefix("ext-") else {
        return Ok(None);
    };
    if extension.is_empty()
        || extension.len() > 64
        || !extension.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.-".contains(&byte)
        })
    {
        return Err(format!("invalid Composer extension requirement: {name}"));
    }
    Ok(Some(extension.to_owned()))
}

fn module_map(modules: &[String]) -> BTreeMap<String, String> {
    modules
        .iter()
        .map(|module| {
            if module.eq_ignore_ascii_case("Zend OPcache") {
                ("zend-opcache".to_owned(), "opcache".to_owned())
            } else {
                let canonical = module.to_ascii_lowercase().replace(' ', "-");
                (canonical.clone(), canonical)
            }
        })
        .collect()
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn print_human(report: &ExtensionProfileReport) {
    println!("Composer PHP extension profile (schema 1)");
    println!("Project: {}", report.project_root);
    println!("Requirements: {}", report.requirements.len());
    println!("Built into PHP: {}", report.builtin_extensions.join(", "));
    println!(
        "Provided by packages: {}",
        report.provided_extensions.join(", ")
    );
    println!(
        "Dynamic selection: {}",
        report.selected_extensions.join(", ")
    );
    if !report.missing_extensions.is_empty() {
        println!("Missing: {}", report.missing_extensions.join(", "));
        println!("Install the missing extensions and run `pam extensions` again.");
        return;
    }
    if report.arguments.is_empty() {
        println!("No --php-extension arguments are required by the locked project.");
    } else {
        println!("Review and append these explicit arguments to pam start or pam up:");
        println!("  {}", report.arguments.join(" "));
    }
    println!("Compatible mode remains the default until you apply these arguments.");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn project(manifest: &str, lock: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "pam-extension-profile-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("composer.json"), manifest).unwrap();
        fs::write(root.join("composer.lock"), lock).unwrap();
        root
    }

    #[test]
    fn derives_only_dynamic_locked_requirements_and_explains_sources() {
        let root = project(
            r#"{"require":{"ext-json":"*","vendor/app":"^1"},"require-dev":{"ext-xdebug":"^3"}}"#,
            r#"{"packages":[{"name":"vendor/app","require":{"ext-iconv":"*","ext-pdo":"^8"}}],"packages-dev":[]}"#,
        );
        let report = build_report(
            &root,
            false,
            &["json".into(), "iconv".into(), "PDO".into()],
            &["json".into(), "PDO".into()],
            "0123456789abcdef0123456789abcdef".to_owned(),
        )
        .unwrap();
        assert!(report.ready);
        assert_eq!(report.selected_extensions, ["iconv"]);
        assert_eq!(report.builtin_extensions, ["json", "pdo"]);
        assert_eq!(report.arguments, ["--php-extension", "iconv"]);
        assert_eq!(report.requirements.len(), 3);
        assert_eq!(report.requirements[0].sources[0].source_code, 3);
        assert_eq!(report.requirements[1].sources[0].source_code, 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn honors_locked_polyfills_and_reports_missing_extensions() {
        let root = project(
            r#"{"require":{"ext-mbstring":"*"}}"#,
            r#"{"packages":[{"name":"polyfill/mb","provide":{"ext-mbstring":"*"},"require":{"ext-gd":"*"}}],"packages-dev":[]}"#,
        );
        let report = build_report(
            &root,
            true,
            &[],
            &[],
            "0123456789abcdef0123456789abcdef".to_owned(),
        )
        .unwrap();
        assert!(!report.ready);
        assert_eq!(report.provided_extensions, ["mbstring"]);
        assert_eq!(report.missing_extensions, ["gd"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn maps_zend_opcache_to_its_safe_loader_name() {
        let root = project(
            r#"{"require":{"ext-zend-opcache":"*"}}"#,
            r#"{"packages":[],"packages-dev":[]}"#,
        );
        let report = build_report(
            &root,
            true,
            &["Zend OPcache".into()],
            &[],
            "0123456789abcdef0123456789abcdef".to_owned(),
        )
        .unwrap();
        assert_eq!(report.selected_extensions, ["opcache"]);
        fs::remove_dir_all(root).unwrap();
    }
}
