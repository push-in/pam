use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Value, json};

const MAX_SNAPSHOT_BYTES: u64 = 1024 * 1024;
const MAX_SERVER_EVENTS: usize = 1024;
const MAX_NATIVE_EVENTS: usize = 8;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TraceDocument {
    schema_version: u8,
    source_surface_code: u8,
    captured_at_unix_ms: u64,
    display_time_unit: &'static str,
    trace_events: Vec<TraceEvent>,
}

#[derive(Debug, Serialize)]
struct TraceEvent {
    name: &'static str,
    cat: &'static str,
    ph: &'static str,
    ts: u64,
    pid: u8,
    tid: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    dur: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    s: Option<&'static str>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    args: BTreeMap<&'static str, Value>,
}

pub fn run(arguments: impl IntoIterator<Item = OsString>) -> Result<u8, String> {
    let mut input = None;
    let mut output = None;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        if argument == "--output" {
            output = Some(PathBuf::from(
                arguments
                    .next()
                    .ok_or_else(|| "--output requires a path".to_owned())?,
            ));
        } else if argument.to_string_lossy().starts_with('-') && argument != "-" {
            return Err(format!(
                "unknown timeline option: {}",
                argument.to_string_lossy()
            ));
        } else if input.is_none() {
            input = Some(PathBuf::from(argument));
        } else {
            return Err("timeline accepts one diagnostic snapshot".to_owned());
        }
    }
    let input = input.ok_or_else(|| {
        "timeline requires a diagnostic snapshot path or - for standard input".to_owned()
    })?;
    let snapshot = read_snapshot(&input)?;
    let trace = export(&snapshot)?;
    let bytes = serde_json::to_vec_pretty(&trace)
        .map_err(|error| format!("cannot encode performance timeline: {error}"))?;
    write_output(output.as_deref(), &bytes)?;
    Ok(0)
}

fn read_snapshot(path: &Path) -> Result<Value, String> {
    let bytes = if path == Path::new("-") {
        let mut bytes = Vec::new();
        std::io::stdin()
            .take(MAX_SNAPSHOT_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("cannot read diagnostic snapshot: {error}"))?;
        if bytes.len() as u64 > MAX_SNAPSHOT_BYTES {
            return Err("diagnostic snapshot exceeds the 1 MiB limit".to_owned());
        }
        bytes
    } else {
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
        if !metadata.file_type().is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > MAX_SNAPSHOT_BYTES
        {
            return Err(
                "diagnostic snapshot must be a regular file no larger than 1 MiB".to_owned(),
            );
        }
        fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?
    };
    serde_json::from_slice(&bytes).map_err(|error| format!("invalid diagnostic snapshot: {error}"))
}

fn write_output(path: Option<&Path>, bytes: &[u8]) -> Result<(), String> {
    if let Some(path) = path {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| format!("cannot create timeline {}: {error}", path.display()))?;
        file.write_all(bytes)
            .and_then(|()| file.write_all(b"\n"))
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("cannot write timeline {}: {error}", path.display()))?;
        println!("Wrote bounded performance timeline to {}.", path.display());
    } else {
        let mut stdout = std::io::stdout().lock();
        stdout
            .write_all(bytes)
            .and_then(|()| stdout.write_all(b"\n"))
            .map_err(|error| format!("cannot write performance timeline: {error}"))?;
    }
    Ok(())
}

fn export(snapshot: &Value) -> Result<TraceDocument, String> {
    let schema = unsigned(snapshot, "schemaVersion")?;
    let surface = unsigned(snapshot, "surfaceCode")?;
    let captured = unsigned(snapshot, "capturedAtUnixMs")?;
    if schema != 1 || !(1..=3).contains(&surface) {
        return Err(
            "timeline requires a DevTools snapshot with schemaVersion 1 and surfaceCode 1, 2, or 3"
                .to_owned(),
        );
    }
    let surface = u8::try_from(surface).expect("validated surface code");
    let trace_events = match surface {
        1 => server_events(snapshot)?,
        2 => native_events(snapshot)?,
        3 => desktop_events(snapshot)?,
        _ => unreachable!(),
    };
    Ok(TraceDocument {
        schema_version: 1,
        source_surface_code: surface,
        captured_at_unix_ms: captured,
        display_time_unit: "ms",
        trace_events,
    })
}

fn server_events(snapshot: &Value) -> Result<Vec<TraceEvent>, String> {
    let events = snapshot
        .get("events")
        .and_then(Value::as_array)
        .ok_or_else(|| "Server diagnostic snapshot requires an events array".to_owned())?;
    if events.len() > MAX_SERVER_EVENTS {
        return Err("Server diagnostic timeline exceeds 1024 events".to_owned());
    }
    let first = events
        .iter()
        .filter_map(|event| event.get("timestampNanoseconds").and_then(Value::as_u64))
        .min()
        .unwrap_or(0);
    events
        .iter()
        .map(|event| {
            let kind = unsigned(event, "kind")?;
            let timestamp = unsigned(event, "timestampNanoseconds")?;
            let (name, category) = server_kind(kind)?;
            Ok(instant(
                name,
                category,
                timestamp.saturating_sub(first) / 1_000,
                1,
            ))
        })
        .collect()
}

fn native_events(snapshot: &Value) -> Result<Vec<TraceEvent>, String> {
    let events = snapshot
        .get("timeline")
        .and_then(Value::as_array)
        .ok_or_else(|| "Native diagnostic snapshot requires a timeline array".to_owned())?;
    if events.len() > MAX_NATIVE_EVENTS {
        return Err("Native diagnostic timeline exceeds 8 events".to_owned());
    }
    let mut timestamp = 0_u64;
    events
        .iter()
        .map(|event| {
            let kind = unsigned(event, "kindCode")?;
            let duration = unsigned(event, "durationMicros")?;
            let failed = event
                .get("failed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let name = match kind {
                1 => "native.module_call",
                2 => "native.event",
                3 => "native.error",
                4 => "native.lifecycle",
                _ => return Err("Native timeline kindCode must be 1, 2, 3, or 4".to_owned()),
            };
            let mut args = BTreeMap::new();
            args.insert("failed", json!(failed));
            let trace = TraceEvent {
                name,
                cat: "pam.native",
                ph: "X",
                ts: timestamp,
                pid: 2,
                tid: 1,
                dur: Some(duration),
                s: None,
                args,
            };
            timestamp = timestamp.saturating_add(duration.max(1));
            Ok(trace)
        })
        .collect()
}

fn desktop_events(snapshot: &Value) -> Result<Vec<TraceEvent>, String> {
    let mut args = BTreeMap::new();
    for (source, target) in [
        ("totalCommands", "total_commands"),
        ("failedCommands", "failed_commands"),
        ("activeCommands", "active_commands"),
        ("averageCommandMicroseconds", "average_command_microseconds"),
        ("primaryWorkerGeneration", "primary_worker_generation"),
        ("parallelWorkers", "parallel_workers"),
        ("eventCursor", "event_cursor"),
    ] {
        args.insert(target, json!(unsigned(snapshot, source)?));
    }
    Ok(vec![TraceEvent {
        name: "desktop.runtime",
        cat: "pam.desktop",
        ph: "C",
        ts: 0,
        pid: 3,
        tid: 1,
        dur: None,
        s: None,
        args,
    }])
}

fn instant(name: &'static str, cat: &'static str, ts: u64, pid: u8) -> TraceEvent {
    TraceEvent {
        name,
        cat,
        ph: "i",
        ts,
        pid,
        tid: 1,
        dur: None,
        s: Some("t"),
        args: BTreeMap::new(),
    }
}

fn server_kind(kind: u64) -> Result<(&'static str, &'static str), String> {
    match kind {
        1 => Ok(("request.start", "pam.server.request")),
        2 => Ok(("request.end", "pam.server.request")),
        3 => Ok(("fiber.suspend", "pam.server.fiber")),
        4 => Ok(("fiber.resume", "pam.server.fiber")),
        5 => Ok(("io.start", "pam.server.io")),
        6 => Ok(("io.end", "pam.server.io")),
        7 => Ok(("request.cleanup", "pam.server.request")),
        8 => Ok(("runtime.error", "pam.server.error")),
        _ => Err("Server diagnostic event kind must be between 1 and 8".to_owned()),
    }
}

fn unsigned(value: &Value, field: &str) -> Result<u64, String> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("diagnostic snapshot requires unsigned integer {field}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_all_surfaces_without_sensitive_labels_or_context() {
        let server = json!({
            "schemaVersion": 1, "surfaceCode": 1, "capturedAtUnixMs": 100,
            "events": [{"kind": 1, "timestampNanoseconds": 10_000, "requestId": "secret-id", "context": {"authorization": "secret"}}]
        });
        let native = json!({
            "schemaVersion": 1, "surfaceCode": 2, "capturedAtUnixMs": 101,
            "timeline": [{"kindCode": 1, "durationMicros": 42, "failed": false, "label": "private/path"}]
        });
        let desktop = json!({
            "schemaVersion": 1, "surfaceCode": 3, "capturedAtUnixMs": 102,
            "totalCommands": 9, "failedCommands": 1, "activeCommands": 2,
            "averageCommandMicroseconds": 3, "primaryWorkerGeneration": 4,
            "parallelWorkers": 5, "eventCursor": 6, "bridgeToken": "secret"
        });
        let server_trace = serde_json::to_string(&export(&server).unwrap()).unwrap();
        let native_trace = serde_json::to_string(&export(&native).unwrap()).unwrap();
        let desktop_trace = serde_json::to_string(&export(&desktop).unwrap()).unwrap();
        assert!(server_trace.contains("request.start"));
        assert!(native_trace.contains("native.module_call"));
        assert!(desktop_trace.contains("average_command_microseconds"));
        for trace in [&server_trace, &native_trace, &desktop_trace] {
            assert!(!trace.contains("secret"));
            assert!(!trace.contains("private/path"));
        }
    }

    #[test]
    fn rejects_unbounded_or_unknown_timeline_contracts() {
        let oversized = json!({
            "schemaVersion": 1, "surfaceCode": 2, "capturedAtUnixMs": 1,
            "timeline": vec![json!({"kindCode": 1, "durationMicros": 1}); 9]
        });
        assert!(export(&oversized).unwrap_err().contains("exceeds 8"));
        let unknown = json!({
            "schemaVersion": 1, "surfaceCode": 4, "capturedAtUnixMs": 1
        });
        assert!(export(&unknown).is_err());
    }
}
