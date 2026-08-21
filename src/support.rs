use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_DOCTOR_BYTES: usize = 256 * 1024;
const MAX_REPORT_BYTES: usize = 512 * 1024;

pub fn run(executable: &OsStr, arguments: impl Iterator<Item = OsString>) -> Result<u8, String> {
    let mut target = OsString::from(".");
    let mut target_seen = false;
    let mut output = None::<PathBuf>;
    let mut include_manager = false;
    let mut arguments = arguments.peekable();

    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--output" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--output requires a new JSON file path".to_owned())?;
                output = Some(PathBuf::from(value));
            }
            "--manager" => include_manager = true,
            option if option.starts_with('-') => {
                return Err(format!("unknown support option: {option}"));
            }
            _ if !target_seen => {
                target = argument;
                target_seen = true;
            }
            _ => return Err("support accepts at most one project path".to_owned()),
        }
    }

    let target = fs::canonicalize(&target)
        .map_err(|error| format!("cannot resolve support target: {error}"))?;
    let doctor = Command::new(executable)
        .args(["doctor", "--json"])
        .arg(&target)
        .output()
        .map_err(|error| format!("cannot run support diagnostics: {error}"))?;
    if doctor.stdout.len() > MAX_DOCTOR_BYTES || doctor.stderr.len() > MAX_DOCTOR_BYTES {
        return Err("support diagnostics exceed the 256 KiB safety limit".to_owned());
    }

    let mut diagnostics: Value = serde_json::from_slice(&doctor.stdout)
        .map_err(|error| format!("doctor returned invalid structured diagnostics: {error}"))?;
    let mut secrets = vec![target.to_string_lossy().into_owned()];
    if let Some(home) = env::var_os("HOME") {
        secrets.push(home.to_string_lossy().into_owned());
    }
    redact(&mut diagnostics, &secrets);

    let (manager, manager_ok) = if include_manager {
        let snapshot = Command::new(executable)
            .args(["monit", "--json"])
            .output()
            .map_err(|error| format!("cannot run manager diagnostics: {error}"))?;
        if snapshot.stdout.len() > MAX_DOCTOR_BYTES || snapshot.stderr.len() > MAX_DOCTOR_BYTES {
            return Err("manager diagnostics exceed the 256 KiB safety limit".to_owned());
        }
        let mut value: Value = serde_json::from_slice(&snapshot.stdout)
            .map_err(|error| format!("manager returned invalid structured diagnostics: {error}"))?;
        redact(&mut value, &secrets);
        (Some(value), snapshot.status.success())
    } else {
        (None, true)
    };

    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before Unix epoch: {error}"))?
        .as_millis();
    let diagnostics_bytes = serde_json::to_vec(&diagnostics).map_err(|error| error.to_string())?;
    let digest = format!("{:x}", Sha256::digest(&diagnostics_bytes));
    let manager_digest = manager
        .as_ref()
        .map(|value| serde_json::to_vec(value).map_err(|error| error.to_string()))
        .transpose()?
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)));
    let success = doctor.status.success() && manager_ok;
    let report = json!({
        "schemaVersion": 1,
        "resultCode": if success { 1 } else { 2 },
        "surfaceCode": 1,
        "generatedAtUnixMs": generated_at,
        "pamVersion": env!("CARGO_PKG_VERSION"),
        "host": host_contract()?,
        "privacy": {
            "redactionCode": 1,
            "includesEnvironment": false,
            "includesFileContents": false,
            "includesNetworkData": false,
            "includesProcessMetadata": include_manager,
            "includesLogContents": false,
            "pathToken": "$PROJECT"
        },
        "diagnosticsSha256": digest,
        "diagnostics": diagnostics,
        "managerSha256": manager_digest,
        "manager": manager
    });
    let bytes = serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?;
    if bytes.len() > MAX_REPORT_BYTES {
        return Err("support report exceeds the 512 KiB safety limit".to_owned());
    }

    if let Some(path) = output {
        write_new_private(&path, &bytes)?;
        println!("Wrote redacted PAM support report to {}", path.display());
    } else {
        println!("{}", String::from_utf8_lossy(&bytes));
    }
    Ok(if success { 0 } else { 1 })
}

fn host_contract() -> Result<Value, String> {
    let os_code = match env::consts::OS {
        "linux" => 1,
        "macos" => 2,
        "windows" => 3,
        other => {
            return Err(format!(
                "unsupported support-report operating system: {other}"
            ));
        }
    };
    let architecture_code = match env::consts::ARCH {
        "x86_64" => 1,
        "aarch64" => 2,
        other => return Err(format!("unsupported support-report architecture: {other}")),
    };
    Ok(json!({"osCode": os_code, "architectureCode": architecture_code}))
}

fn redact(value: &mut Value, secrets: &[String]) {
    match value {
        Value::String(text) => {
            for secret in secrets.iter().filter(|secret| !secret.is_empty()) {
                *text = text.replace(
                    secret,
                    if secret == &secrets[0] {
                        "$PROJECT"
                    } else {
                        "$HOME"
                    },
                );
            }
        }
        Value::Array(values) => values.iter_mut().for_each(|value| redact(value, secrets)),
        Value::Object(values) => values.values_mut().for_each(|value| redact(value, secrets)),
        _ => {}
    }
}

fn write_new_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if path.extension() != Some(OsStr::new("json")) {
        return Err("support output must use the .json extension".to_owned());
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create support output directory: {error}"))?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|error| {
        format!(
            "cannot create new support report {}: {error}",
            path.display()
        )
    })?;
    file.write_all(bytes)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("cannot persist support report: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_nested_project_and_home_paths() {
        let mut value = json!({"root": "/home/alice/app", "lines": ["at /home/alice/app/index.php", "/home/alice/.cache"]});
        redact(
            &mut value,
            &["/home/alice/app".to_owned(), "/home/alice".to_owned()],
        );
        assert_eq!(value["root"], "$PROJECT");
        assert_eq!(value["lines"][0], "at $PROJECT/index.php");
        assert_eq!(value["lines"][1], "$HOME/.cache");
    }
}
