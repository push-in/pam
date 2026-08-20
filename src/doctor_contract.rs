use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

const MAX_REPORT_BYTES: u64 = 1024 * 1024;

pub fn validate_file(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect Doctor report {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "Doctor report must be a regular, non-symlink file: {}",
            path.display()
        ));
    }
    if metadata.len() > MAX_REPORT_BYTES {
        return Err(format!(
            "Doctor report exceeds the {}-byte validation limit: {}",
            MAX_REPORT_BYTES,
            path.display()
        ));
    }
    let source = fs::read(path)
        .map_err(|error| format!("cannot read Doctor report {}: {error}", path.display()))?;
    let report: Value = serde_json::from_slice(&source)
        .map_err(|error| format!("invalid Doctor report JSON: {error}"))?;
    validate(&report)
}

fn validate(report: &Value) -> Result<(), String> {
    let root = object(report, "report")?;
    exact_keys(
        root,
        &[
            "diagnostics",
            "errors",
            "exitCode",
            "healthy",
            "nextActions",
            "project",
            "projectType",
            "resultCode",
            "root",
            "schema",
            "schemaVersion",
            "target",
        ],
        "report",
    )?;
    exact_integer(root.get("schema"), 1, "schema")?;
    exact_integer(root.get("schemaVersion"), 1, "schemaVersion")?;
    let healthy = boolean(root.get("healthy"), "healthy")?;
    let result = integer(root.get("resultCode"), "resultCode")?;
    let exit = integer(root.get("exitCode"), "exitCode")?;
    if exit > 255 {
        return Err("exitCode must be between 0 and 255".to_owned());
    }
    if (healthy && (result != 1 || exit != 0)) || (!healthy && (result != 2 || exit == 0)) {
        return Err("healthy, resultCode, and exitCode are inconsistent".to_owned());
    }
    nonempty_string(root.get("target"), "target")?;
    string(root.get("diagnostics"), "diagnostics")?;
    string(root.get("errors"), "errors")?;

    let project_type = optional_code(root.get("projectType"), 1, 6, "projectType")?;
    let project_root = optional_string(root.get("root"), "root")?;
    match root.get("project") {
        Some(Value::Null) if project_type.is_none() && project_root.is_none() => {}
        Some(value) => {
            let expected_type = project_type.ok_or_else(|| {
                "projectType and root must be present when project is present".to_owned()
            })?;
            let expected_root = project_root.ok_or_else(|| {
                "projectType and root must be present when project is present".to_owned()
            })?;
            validate_project(value, expected_type, expected_root)?;
        }
        None => return Err("report is missing project".to_owned()),
    }
    validate_actions(root.get("nextActions"))
}

fn validate_project(value: &Value, expected_type: u64, expected_root: &str) -> Result<(), String> {
    let project = object(value, "project")?;
    exact_keys(
        project,
        &[
            "developmentArtifacts",
            "nextCommands",
            "paths",
            "root",
            "typeCode",
            "typeLabel",
        ],
        "project",
    )?;
    if integer(project.get("typeCode"), "project.typeCode")? != expected_type {
        return Err("project.typeCode does not match projectType".to_owned());
    }
    if nonempty_string(project.get("root"), "project.root")? != expected_root {
        return Err("project.root does not match root".to_owned());
    }
    nonempty_string(project.get("typeLabel"), "project.typeLabel")?;
    let paths = object(
        project.get("paths").unwrap_or(&Value::Null),
        "project.paths",
    )?;
    exact_keys(
        paths,
        &["composerManifest", "manifest", "nativeManifest"],
        "project.paths",
    )?;
    for key in ["composerManifest", "manifest", "nativeManifest"] {
        nonempty_string(paths.get(key), &format!("project.paths.{key}"))?;
    }
    validate_artifacts(project.get("developmentArtifacts"))?;
    let commands = array(project.get("nextCommands"), "project.nextCommands")?;
    if commands.is_empty() || commands.len() > 16 {
        return Err("project.nextCommands must contain 1 to 16 commands".to_owned());
    }
    for (index, command) in commands.iter().enumerate() {
        nonempty_string(Some(command), &format!("project.nextCommands[{index}]"))?;
    }
    Ok(())
}

fn validate_artifacts(value: Option<&Value>) -> Result<(), String> {
    let artifacts = object(
        value.unwrap_or(&Value::Null),
        "project.developmentArtifacts",
    )?;
    exact_keys(
        artifacts,
        &["bytes", "complete", "entries", "files"],
        "project.developmentArtifacts",
    )?;
    let total_bytes = integer(artifacts.get("bytes"), "project.developmentArtifacts.bytes")?;
    let total_files = integer(artifacts.get("files"), "project.developmentArtifacts.files")?;
    boolean(
        artifacts.get("complete"),
        "project.developmentArtifacts.complete",
    )?;
    let entries = array(
        artifacts.get("entries"),
        "project.developmentArtifacts.entries",
    )?;
    if entries.len() > 64 {
        return Err("project.developmentArtifacts.entries exceeds 64 items".to_owned());
    }
    let mut bytes = 0_u64;
    let mut files = 0_u64;
    for (index, value) in entries.iter().enumerate() {
        let name = format!("project.developmentArtifacts.entries[{index}]");
        let entry = object(value, &name)?;
        exact_keys(
            entry,
            &["bytes", "complete", "exists", "files", "kindCode", "path"],
            &name,
        )?;
        nonempty_string(entry.get("path"), &format!("{name}.path"))?;
        let kind = integer(entry.get("kindCode"), &format!("{name}.kindCode"))?;
        if !matches!(kind, 1 | 2) {
            return Err(format!("{name}.kindCode must be 1 or 2"));
        }
        boolean(entry.get("exists"), &format!("{name}.exists"))?;
        boolean(entry.get("complete"), &format!("{name}.complete"))?;
        bytes = bytes.saturating_add(integer(entry.get("bytes"), &format!("{name}.bytes"))?);
        files = files.saturating_add(integer(entry.get("files"), &format!("{name}.files"))?);
    }
    if bytes != total_bytes || files != total_files {
        return Err("artifact entry totals do not match the report totals".to_owned());
    }
    Ok(())
}

fn validate_actions(value: Option<&Value>) -> Result<(), String> {
    let actions = array(value, "nextActions")?;
    if actions.is_empty() || actions.len() > 8 {
        return Err("nextActions must contain 1 to 8 actions".to_owned());
    }
    for (index, value) in actions.iter().enumerate() {
        let name = format!("nextActions[{index}]");
        let action = object(value, &name)?;
        exact_keys(
            action,
            &[
                "actionCode",
                "arguments",
                "command",
                "summary",
                "verificationCommand",
            ],
            &name,
        )?;
        let code = integer(action.get("actionCode"), &format!("{name}.actionCode"))?;
        if !matches!(code, 1..=3) {
            return Err(format!("{name}.actionCode must be between 1 and 3"));
        }
        for key in ["summary", "command", "verificationCommand"] {
            nonempty_string(action.get(key), &format!("{name}.{key}"))?;
        }
        let arguments = array(action.get("arguments"), &format!("{name}.arguments"))?;
        if arguments.is_empty() || arguments.len() > 32 {
            return Err(format!("{name}.arguments must contain 1 to 32 strings"));
        }
        for (argument_index, argument) in arguments.iter().enumerate() {
            string(
                Some(argument),
                &format!("{name}.arguments[{argument_index}]"),
            )?;
        }
    }
    Ok(())
}

fn exact_keys(object: &Map<String, Value>, expected: &[&str], name: &str) -> Result<(), String> {
    let actual = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(format!("{name} fields do not match schema version 1"));
    }
    Ok(())
}

fn object<'a>(value: &'a Value, name: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{name} must be an object"))
}

fn array<'a>(value: Option<&'a Value>, name: &str) -> Result<&'a Vec<Value>, String> {
    value
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{name} must be an array"))
}

fn integer(value: Option<&Value>, name: &str) -> Result<u64, String> {
    value
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{name} must be a non-negative integer"))
}

fn exact_integer(value: Option<&Value>, expected: u64, name: &str) -> Result<(), String> {
    if integer(value, name)? != expected {
        return Err(format!("{name} must be {expected}"));
    }
    Ok(())
}

fn boolean(value: Option<&Value>, name: &str) -> Result<bool, String> {
    value
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("{name} must be a boolean"))
}

fn string<'a>(value: Option<&'a Value>, name: &str) -> Result<&'a str, String> {
    value
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{name} must be a string"))
}

fn nonempty_string<'a>(value: Option<&'a Value>, name: &str) -> Result<&'a str, String> {
    let value = string(value, name)?;
    if value.is_empty() {
        return Err(format!("{name} must not be empty"));
    }
    Ok(value)
}

fn optional_string<'a>(value: Option<&'a Value>, name: &str) -> Result<Option<&'a str>, String> {
    match value {
        Some(Value::Null) => Ok(None),
        Some(value) => string(Some(value), name).map(Some),
        None => Err(format!("report is missing {name}")),
    }
}

fn optional_code(
    value: Option<&Value>,
    minimum: u64,
    maximum: u64,
    name: &str,
) -> Result<Option<u64>, String> {
    match value {
        Some(Value::Null) => Ok(None),
        Some(value) => {
            let code = integer(Some(value), name)?;
            if !(minimum..=maximum).contains(&code) {
                return Err(format!("{name} must be between {minimum} and {maximum}"));
            }
            Ok(Some(code))
        }
        None => Err(format!("report is missing {name}")),
    }
}
