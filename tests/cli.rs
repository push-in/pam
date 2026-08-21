use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};

fn run_pam(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pam"))
        .args(arguments)
        .output()
        .expect("pam should start")
}

fn run_pam_in(directory: &std::path::Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pam"))
        .current_dir(directory)
        .args(arguments)
        .output()
        .expect("pam should start")
}

fn run_managed_pam(
    directory: &std::path::Path,
    state: &std::path::Path,
    port: u16,
    arguments: &[&str],
) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pam"))
        .current_dir(directory)
        .env("PAM_MANAGER_STATE_DIR", state)
        .env("PAM_MANAGER_RUNTIME_DIR", state.join("runtime"))
        .env("PAM_TEST_PORT", port.to_string())
        .args(arguments)
        .output()
        .expect("managed PAM command should start")
}

fn run_manager_daemon(state: &std::path::Path, runtime: &std::path::Path, action: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pam"))
        .env("PAM_MANAGER_STATE_DIR", state)
        .env("PAM_MANAGER_RUNTIME_DIR", runtime)
        .args(["daemon", action])
        .output()
        .expect("pamd command should start")
}

fn temporary_path(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("pam-{name}-{}-{unique}", std::process::id()))
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn fixed_http_server(body: &'static str) -> (u16, Arc<AtomicBool>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let running = Arc::new(AtomicBool::new(true));
    let active = Arc::clone(&running);
    let handle = thread::spawn(move || {
        while active.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut request = [0_u8; 4096];
                    let _ = stream.read(&mut request);
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
    });
    (port, running, handle)
}

fn traffic_request(port: u16, affinity: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .write_all(format!("GET /probe HTTP/1.1\r\nHost: localhost\r\nCookie: pam_affinity={affinity}\r\nConnection: close\r\n\r\n").as_bytes())
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

fn manager_dashboard_request(port: u16, path: &str, authorization: Option<&str>) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let authorization = authorization
        .map(|value| format!("Authorization: {value}\r\n"))
        .unwrap_or_default();
    stream
        .write_all(
            format!(
                "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{authorization}Connection: close\r\n\r\n"
            )
            .as_bytes(),
        )
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

fn css_hex(styles: &str, property: &str) -> [f64; 3] {
    let prefix = format!("{property}: #");
    let value = styles
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix(&prefix))
        .and_then(|value| value.strip_suffix(';'))
        .unwrap_or_else(|| panic!("missing CSS color token {property}"));
    assert_eq!(value.len(), 6, "{property} must use six hexadecimal digits");
    let channel =
        |offset| u8::from_str_radix(&value[offset..offset + 2], 16).unwrap() as f64 / 255.0;
    [channel(0), channel(2), channel(4)]
}

fn contrast_ratio(foreground: [f64; 3], background: [f64; 3]) -> f64 {
    let luminance = |color: [f64; 3]| {
        let linear = |channel: f64| {
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(color[0]) + 0.7152 * linear(color[1]) + 0.0722 * linear(color[2])
    };
    let foreground = luminance(foreground);
    let background = luminance(background);
    (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05)
}

#[test]
fn creates_and_verifies_benchmark_evidence_manifests() {
    let results = temporary_path("benchmark-evidence");
    fs::create_dir_all(&results).unwrap();
    fs::write(
        results.join("metadata.json"),
        r#"{"source":{"commit":"abc","dirty":false},"parameters":{"workers":1}}"#,
    )
    .unwrap();
    fs::write(
        results.join("report.json"),
        r#"{"measurement_gate":{"passed":true},"dynamic_gate":{"passed_frankenphp":true,"p99_passed":true,"zero_errors":true}}"#,
    )
    .unwrap();
    fs::write(results.join("pam.round-1.txt"), "Requests/sec: 100\n").unwrap();
    let script =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benchmarks/octane/evidence-manifest.php");

    let created = run_pam(&[script.to_str().unwrap(), results.to_str().unwrap(), "1"]);
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(results.join("evidence-manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(manifest["suite_id"], 1);
    assert_eq!(manifest["gates"]["measurement"], true);
    assert_eq!(manifest["gates"]["dynamic"], true);
    assert_eq!(manifest["artifacts"].as_array().unwrap().len(), 3);

    let verified = run_pam(&[
        script.to_str().unwrap(),
        results.to_str().unwrap(),
        "1",
        "--verify",
    ]);
    assert!(verified.status.success());
    fs::write(results.join("pam.round-1.txt"), "tampered\n").unwrap();
    let tampered = run_pam(&[
        script.to_str().unwrap(),
        results.to_str().unwrap(),
        "1",
        "--verify",
    ]);
    assert!(!tampered.status.success());
    assert!(String::from_utf8_lossy(&tampered.stderr).contains("do not match"));
    fs::remove_dir_all(results).unwrap();
}

#[test]
fn creates_and_verifies_manager_recovery_evidence() {
    let results = temporary_path("manager-recovery-evidence");
    fs::create_dir_all(&results).unwrap();
    fs::write(
        results.join("metadata.json"),
        r#"{"source":{"commit":"abc","dirty":false},"parameters":{"rounds":3}}"#,
    )
    .unwrap();
    fs::write(
        results.join("recovery.csv"),
        "round,recovery_millis,success\n1,100,1\n2,150,1\n3,200,1\n",
    )
    .unwrap();
    fs::write(
        results.join("recovery-phases.csv"),
        "round,detection_millis,backoff_millis,readiness_millis,accounted_millis,success\n1,10,10,70,90,1\n2,15,10,110,135,1\n3,20,10,150,180,1\n",
    )
    .unwrap();
    fs::write(
        results.join("resources.json"),
        r#"{"daemon_rss_before_bytes":1000000,"daemon_rss_after_bytes":2000000}"#,
    )
    .unwrap();
    fs::write(
        results.join("worker-startup.csv"),
        "round,workers,spawn_spread_millis,spawn_to_ready_p95_millis,spawn_to_ready_maximum_millis,spawn_to_process_p95_millis,php_engine_p95_millis,spawn_to_engine_p95_millis,composer_p95_millis,runtime_bootstrap_p95_millis,application_p95_millis,success\n1,4,3,70,75,8,12,20,5,10,35,1\n2,4,4,110,115,12,18,30,6,11,63,1\n3,4,5,150,155,16,24,40,7,12,91,1\n",
    )
    .unwrap();
    let report_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("benchmarks/process-manager/recovery-report.php");
    let reported = run_pam(&[
        report_path.to_str().unwrap(),
        results.to_str().unwrap(),
        "500",
        "2000000",
        "25",
        "20",
        "160",
    ]);
    assert!(reported.status.success());
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(results.join("recovery-report.json")).unwrap()).unwrap();
    assert_eq!(report["suite_code"], 5);
    assert_eq!(report["recovery_millis"]["p50"], 150);
    assert_eq!(report["recovery_millis"]["p95"], 200);
    assert_eq!(report["recovery_phases"]["detection_millis"]["p95"], 20);
    assert_eq!(report["recovery_phases"]["backoff_millis"]["p50"], 10);
    assert_eq!(report["recovery_phases"]["readiness_millis"]["p95"], 150);
    assert_eq!(report["worker_startup"]["workers"], 4);
    assert_eq!(report["worker_startup"]["spawn_spread_millis"]["p95"], 5);
    assert_eq!(
        report["worker_startup"]["spawn_to_ready_p95_millis"]["p95"],
        150
    );
    assert_eq!(
        report["worker_startup"]["spawn_to_engine_p95_millis"]["p95"],
        40
    );
    assert_eq!(
        report["worker_startup"]["application_p95_millis"]["p50"],
        63
    );
    assert_eq!(report["gate_codes"]["success"], 1);
    assert_eq!(report["gate_codes"]["detection"], 1);
    assert_eq!(report["gate_codes"]["backoff"], 1);
    assert_eq!(report["gate_codes"]["readiness"], 1);
    assert_eq!(report["passed"], true);
    let failed_gate = run_pam(&[
        report_path.to_str().unwrap(),
        results.to_str().unwrap(),
        "100",
        "2000000",
        "25",
        "20",
        "160",
    ]);
    assert!(!failed_gate.status.success());
    let failed_report: serde_json::Value =
        serde_json::from_slice(&fs::read(results.join("recovery-report.json")).unwrap()).unwrap();
    assert_eq!(failed_report["gate_codes"]["latency"], 2);
    assert_eq!(failed_report["passed"], false);
    let failed_phase_gate = run_pam(&[
        report_path.to_str().unwrap(),
        results.to_str().unwrap(),
        "500",
        "2000000",
        "15",
        "20",
        "160",
    ]);
    assert!(!failed_phase_gate.status.success());
    let failed_phase_report: serde_json::Value =
        serde_json::from_slice(&fs::read(results.join("recovery-report.json")).unwrap()).unwrap();
    assert_eq!(failed_phase_report["gate_codes"]["detection"], 2);
    assert_eq!(failed_phase_report["passed"], false);
    assert!(
        run_pam(&[
            report_path.to_str().unwrap(),
            results.to_str().unwrap(),
            "500",
            "2000000",
            "25",
            "20",
            "160",
        ])
        .status
        .success()
    );

    let manifest =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benchmarks/octane/evidence-manifest.php");
    assert!(
        run_pam(&[manifest.to_str().unwrap(), results.to_str().unwrap(), "5"])
            .status
            .success()
    );
    assert!(
        run_pam(&[
            manifest.to_str().unwrap(),
            results.to_str().unwrap(),
            "5",
            "--verify",
        ])
        .status
        .success()
    );
    fs::write(results.join("recovery.csv"), "tampered\n").unwrap();
    assert!(
        !run_pam(&[
            manifest.to_str().unwrap(),
            results.to_str().unwrap(),
            "5",
            "--verify",
        ])
        .status
        .success()
    );
    fs::remove_dir_all(results).unwrap();
}

#[test]
fn compares_pam_and_pm2_recovery_without_conflating_topologies() {
    let results = temporary_path("manager-recovery-comparison");
    for system in ["pam", "pm2"] {
        fs::create_dir_all(results.join(system)).unwrap();
        fs::write(
            results.join(system).join("resources.json"),
            r#"{"daemon_rss_before_bytes":1000000,"daemon_rss_after_bytes":1250000}"#,
        )
        .unwrap();
    }
    fs::write(
        results.join("pam/recovery.csv"),
        "round,recovery_millis,success\n1,600,1\n2,650,1\n3,700,1\n",
    )
    .unwrap();
    fs::write(
        results.join("pm2/recovery.csv"),
        "round,recovery_millis,success\n1,100,1\n2,110,1\n3,120,1\n",
    )
    .unwrap();
    fs::write(
        results.join("metadata.json"),
        r#"{"source":{"commit":"abc","dirty":false},"parameters":{"rounds":3}}"#,
    )
    .unwrap();
    let report = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("benchmarks/process-manager/comparison-report.php");
    assert!(
        run_pam(&[report.to_str().unwrap(), results.to_str().unwrap()])
            .status
            .success()
    );
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(results.join("comparison-report.json")).unwrap()).unwrap();
    assert_eq!(report["suite_code"], 6);
    assert_eq!(report["systems"]["pam"]["topology_code"], 1);
    assert_eq!(report["systems"]["pm2"]["topology_code"], 2);
    assert_eq!(report["comparison"]["p95_delta_millis"], 580);
    assert_eq!(report["comparison"]["rss_not_directly_comparable"], true);
    assert_eq!(report["passed"], true);

    let manifest =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benchmarks/octane/evidence-manifest.php");
    assert!(
        run_pam(&[manifest.to_str().unwrap(), results.to_str().unwrap(), "6"])
            .status
            .success()
    );
    assert!(
        run_pam(&[
            manifest.to_str().unwrap(),
            results.to_str().unwrap(),
            "6",
            "--verify",
        ])
        .status
        .success()
    );
    fs::remove_dir_all(results).unwrap();
}

#[test]
fn aggregates_and_verifies_manager_recovery_worker_matrix() {
    let results = temporary_path("manager-recovery-worker-matrix");
    for (index, workers) in [1, 4, 16].iter().enumerate() {
        let directory = results.join(format!("workers-{workers}"));
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("metadata.json"),
            format!(r#"{{"source":{{"commit":"abc","dirty":false}},"host":{{"kernel":"Linux"}},"tools":{{"pam_sha256":"sha","pam_native_commit":"native"}},"parameters":{{"rounds":3,"workers":{workers}}}}}"#),
        )
        .unwrap();
        fs::write(
            directory.join("recovery-report.json"),
            format!(r#"{{"schema_version":1,"suite_code":5,"rounds":3,"successful_rounds":3,"recovery_millis":{{"p50":{},"p95":{},"maximum":{}}},"recovery_phases":{{"readiness_millis":{{"p50":50,"p95":60,"maximum":70}}}},"worker_startup":{{"workers":{workers},"spawn_spread_millis":{{"p50":2,"p95":3,"maximum":3}},"spawn_to_ready_p95_millis":{{"p50":50,"p95":60,"maximum":60}},"spawn_to_ready_maximum_millis":{{"p50":55,"p95":65,"maximum":65}}}},"daemon_rss_growth_bytes":0,"thresholds":{{"maximum_p95_millis":500}},"gate_codes":{{"success":1,"latency":1,"detection":1,"backoff":1,"readiness":1,"resources":1}},"passed":true}}"#, 100 + index, 110 + index, 120 + index),
        )
        .unwrap();
    }
    let report_script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("benchmarks/process-manager/worker-matrix-report.php");
    assert!(
        run_pam(&[report_script.to_str().unwrap(), results.to_str().unwrap(),])
            .status
            .success()
    );
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(results.join("worker-matrix-report.json")).unwrap())
            .unwrap();
    assert_eq!(report["suite_code"], 7);
    assert_eq!(report["configurations"][0]["configuration_code"], 1);
    assert_eq!(report["configurations"][1]["workers"], 4);
    assert_eq!(report["configurations"][2]["workers"], 16);
    assert_eq!(report["gate_codes"]["all_configurations"], 1);
    assert_eq!(report["gate_codes"]["equivalent_rounds"], 1);
    assert_eq!(report["passed"], true);

    let manifest =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benchmarks/octane/evidence-manifest.php");
    assert!(
        run_pam(&[manifest.to_str().unwrap(), results.to_str().unwrap(), "7"])
            .status
            .success()
    );
    assert!(
        run_pam(&[
            manifest.to_str().unwrap(),
            results.to_str().unwrap(),
            "7",
            "--verify",
        ])
        .status
        .success()
    );
    fs::remove_dir_all(results).unwrap();
}

#[test]
fn compares_compatible_and_isolated_php_extension_profiles() {
    let results = temporary_path("manager-recovery-extension-profile");
    for (directory_name, extensions, total, readiness, engine) in [
        ("compatible", serde_json::json!([]), 200, 130, 80),
        ("isolated", serde_json::json!(["iconv"]), 150, 90, 20),
    ] {
        let directory = results.join(directory_name);
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("metadata.json"),
            serde_json::to_vec(&serde_json::json!({
                "source": {"commit": "abc", "dirty": false},
                "host": {"kernel": "Linux"},
                "tools": {"pam_sha256": "sha", "pam_native_commit": "native"},
                "parameters": {"rounds": 10, "workers": 16, "php_extensions": extensions},
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            directory.join("recovery-report.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 1,
                "suite_code": 5,
                "rounds": 10,
                "successful_rounds": 10,
                "recovery_millis": {"p50": total - 10, "p95": total, "maximum": total},
                "recovery_phases": {"readiness_millis": {"p50": readiness - 10, "p95": readiness, "maximum": readiness}},
                "worker_startup": {
                    "workers": 16,
                    "php_engine_p95_millis": {"p50": engine - 5, "p95": engine, "maximum": engine}
                },
                "daemon_rss_growth_bytes": 0,
                "gate_codes": {"success": 1, "latency": 1, "detection": 1, "backoff": 1, "readiness": 1, "resources": 1},
                "passed": true,
            }))
            .unwrap(),
        )
        .unwrap();
    }
    let report_script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("benchmarks/process-manager/extension-profile-report.php");
    assert!(
        run_pam(&[report_script.to_str().unwrap(), results.to_str().unwrap()])
            .status
            .success()
    );
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(results.join("extension-profile-report.json")).unwrap())
            .unwrap();
    assert_eq!(report["suite_code"], 8);
    assert_eq!(report["configurations"][0]["profile_code"], 1);
    assert_eq!(report["configurations"][1]["profile_code"], 2);
    assert_eq!(
        report["isolated_minus_compatible_millis"]["recovery_p95"],
        -50
    );
    assert_eq!(
        report["isolated_improvement_basis_points"]["php_engine_p95"],
        7500
    );
    assert_eq!(report["gate_codes"]["isolated_engine_not_slower"], 1);
    assert_eq!(report["passed"], true);

    let manifest =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benchmarks/octane/evidence-manifest.php");
    assert!(
        run_pam(&[manifest.to_str().unwrap(), results.to_str().unwrap(), "8"])
            .status
            .success()
    );
    assert!(
        run_pam(&[
            manifest.to_str().unwrap(),
            results.to_str().unwrap(),
            "8",
            "--verify",
        ])
        .status
        .success()
    );
    fs::remove_dir_all(results).unwrap();
}

#[test]
fn records_reproducible_soak_metadata() {
    let results = temporary_path("soak-metadata");
    fs::create_dir_all(&results).unwrap();
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benchmarks/octane/metadata.php");
    let output = Command::new(env!("CARGO_BIN_EXE_pam"))
        .arg(script)
        .arg(&results)
        .env("PAM_BENCH_BINARY", env!("CARGO_BIN_EXE_pam"))
        .env("PAM_BENCH_WORKERS", "4")
        .env("PAM_BENCH_THREADS", "2")
        .env("PAM_BENCH_CONNECTIONS", "64")
        .env("PAM_BENCH_DURATION", "10m")
        .env("PAM_BENCH_WARMUP_DURATION", "5s")
        .env("PAM_BENCH_ROUNDS", "1")
        .env("PAM_BENCH_RUNTIME_ORDER", "pam")
        .env("PAM_SOAK_MAX_RSS_GROWTH_BYTES", "33554432")
        .output()
        .expect("metadata script should start");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(results.join("metadata.json")).unwrap()).unwrap();
    assert!(
        metadata["source"]["commit"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
    assert!(
        metadata["tools"]["pam_sha256"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
    assert_eq!(metadata["parameters"]["workers"], 4);
    assert_eq!(metadata["parameters"]["threads"], 2);
    assert_eq!(metadata["parameters"]["connections"], 64);
    assert_eq!(metadata["parameters"]["duration"], "10m");
    assert_eq!(metadata["parameters"]["runtime_order"][0], "pam");
    assert_eq!(
        metadata["parameters"]["soak_rss_growth_limit_bytes"],
        33_554_432
    );
    fs::remove_dir_all(results).unwrap();
}

#[test]
fn manages_a_detached_runtime_through_its_complete_lifecycle() {
    let state = temporary_path("process-manager-state");
    fs::create_dir_all(&state).unwrap();
    let environment_file = state.join("managed.env");
    fs::write(&environment_file, "PAM_TEST_MANAGED_ENV='private-value'\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&environment_file, fs::Permissions::from_mode(0o644)).unwrap();
    }
    let probe = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let live_probe = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let live_port = live_probe.local_addr().unwrap().port();
    drop(live_probe);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = fixture("server.php");
    let script = script.to_str().unwrap();

    let weak_environment = run_managed_pam(
        &root,
        &state,
        port,
        &[
            "up",
            script,
            "--name",
            "managed-smoke",
            "--env-file",
            environment_file.to_str().unwrap(),
        ],
    );
    assert!(!weak_environment.status.success());
    assert!(String::from_utf8_lossy(&weak_environment.stderr).contains("mode 0600"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&environment_file, fs::Permissions::from_mode(0o600)).unwrap();
    }

    let started = run_managed_pam(
        &root,
        &state,
        port,
        &[
            "up",
            script,
            "--name",
            "managed-smoke",
            "--workers",
            "1",
            "--php-extension",
            "iconv",
            "--env-file",
            environment_file.to_str().unwrap(),
            "--json",
        ],
    );
    assert!(
        started.status.success(),
        "{}",
        String::from_utf8_lossy(&started.stderr)
    );
    let started_snapshot: serde_json::Value = serde_json::from_slice(&started.stdout).unwrap();
    let original_pid = started_snapshot["pid"].as_u64().unwrap() as i32;
    unsafe {
        libc::kill(original_pid, libc::SIGKILL);
    }
    let recovered = (0..60)
        .find_map(|_| {
            thread::sleep(Duration::from_millis(100));
            let output =
                run_managed_pam(&root, &state, port, &["status", "managed-smoke", "--json"]);
            let snapshot: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
            (output.status.success()
                && snapshot["pid"].as_u64() != Some(original_pid as u64)
                && snapshot["recovery"]["totalAutoRestartCount"]
                    .as_u64()
                    .unwrap_or(0)
                    >= 1)
                .then_some(snapshot)
        })
        .expect("pamd should recover an unexpectedly killed master");
    assert_eq!(recovered["desiredStateCode"], 1);
    assert_eq!(recovered["recovery"]["stateCode"], 3);
    let detected = recovered["recovery"]["lastExitDetectedAtMillis"]
        .as_u64()
        .unwrap();
    let recovery_started = recovered["recovery"]["lastRecoveryStartedAtMillis"]
        .as_u64()
        .unwrap();
    let recovery_ready = recovered["recovery"]["lastRecoveryReadyAtMillis"]
        .as_u64()
        .unwrap();
    assert!(detected <= recovery_started && recovery_started <= recovery_ready);
    assert_eq!(recovered["workerStartup"]["spawnSpreadMillis"], 0);
    assert!(
        recovered["workerStartup"]["spawnToReadyP95Millis"]
            .as_u64()
            .is_some_and(|value| value > 0)
    );
    assert!(
        recovered["workerStartup"]["spawnToReadyMaximumMillis"]
            .as_u64()
            .is_some_and(|value| value > 0)
    );
    for phase in [
        "spawnToProcessMillis",
        "phpEngineMillis",
        "spawnToEngineMillis",
        "composerMillis",
        "runtimeBootstrapMillis",
        "applicationMillis",
    ] {
        assert!(
            recovered["workerStartup"]["phaseP95Millis"][phase]
                .as_u64()
                .is_some()
        );
    }
    assert_eq!(recovered["environmentFileConfigured"], true);
    assert_eq!(recovered["phpExtensions"], serde_json::json!(["iconv"]));
    assert!(!recovered.to_string().contains("private-value"));
    assert!(
        !recovered
            .to_string()
            .contains(environment_file.to_str().unwrap())
    );
    let environment_response = manager_dashboard_request(port, "/managed-env", None);
    assert!(environment_response.contains("private-value"));
    fs::write(
        state.join("logs/managed-smoke.out.log.1"),
        "needle-old-output\n",
    )
    .unwrap();
    fs::write(
        state.join("logs/managed-smoke.out.log"),
        "ignored\nneedle-new-output\n",
    )
    .unwrap();
    fs::write(
        state.join("logs/managed-smoke.error.log"),
        "needle-new-error\n",
    )
    .unwrap();
    let queried_logs = run_managed_pam(
        &root,
        &state,
        port,
        &[
            "logs",
            "managed-smoke",
            "--both",
            "--include-rotated",
            "--query",
            "needle-",
            "--lines",
            "3",
            "--json",
        ],
    );
    let listed = run_managed_pam(&root, &state, port, &["ps", "--json"]);
    let scaled = run_managed_pam(
        &root,
        &state,
        port,
        &["scale", "managed-smoke", "2", "--json"],
    );
    let reloaded = run_managed_pam(&root, &state, port, &["reload", "managed-smoke", "--json"]);
    let restarted = run_managed_pam(&root, &state, port, &["restart", "managed-smoke", "--json"]);
    let daemon_recycled = run_managed_pam(&root, &state, port, &["daemon", "stop"]);
    assert!(daemon_recycled.status.success());
    let daemon_restarted = run_managed_pam(&root, &state, port, &["daemon", "start"]);
    assert!(daemon_restarted.status.success());
    let automatic_history = run_managed_pam(
        &root,
        &state,
        port,
        &["monit:history", "managed-smoke", "--json"],
    );
    assert!(automatic_history.status.success());
    let automatic_history: serde_json::Value =
        serde_json::from_slice(&automatic_history.stdout).unwrap();
    assert_eq!(
        automatic_history["applications"][0]["entries"][0]["stateCode"],
        1
    );
    let history_recorded = run_managed_pam(
        &root,
        &state,
        port,
        &["monit:history", "managed-smoke", "--record", "--json"],
    );
    let dashboard_token = state.join("dashboard-token");
    let token = "0123456789abcdef0123456789abcdef";
    fs::write(&dashboard_token, token).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dashboard_token, fs::Permissions::from_mode(0o644)).unwrap();
    }
    let weak_token_rejected = run_managed_pam(
        &root,
        &state,
        port,
        &[
            "dashboard:start",
            "--listen",
            &format!("127.0.0.1:{live_port}"),
            "--token-file",
            dashboard_token.to_str().unwrap(),
        ],
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dashboard_token, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let live_started = run_managed_pam(
        &root,
        &state,
        port,
        &[
            "dashboard:start",
            "--listen",
            &format!("127.0.0.1:{live_port}"),
            "--token-file",
            dashboard_token.to_str().unwrap(),
            "--json",
        ],
    );
    assert!(
        live_started.status.success(),
        "{}",
        String::from_utf8_lossy(&live_started.stderr)
    );
    let unauthorized_dashboard = manager_dashboard_request(live_port, "/", None);
    let wrong_dashboard = manager_dashboard_request(live_port, "/", Some("Bearer wrong"));
    let authorized_dashboard =
        manager_dashboard_request(live_port, "/", Some(&format!("Bearer {token}")));
    let basic = base64::engine::general_purpose::STANDARD.encode(format!("pam:{token}"));
    let healthy_dashboard =
        manager_dashboard_request(live_port, "/health", Some(&format!("Basic {basic}")));
    let live_status = run_managed_pam(&root, &state, port, &["dashboard:status", "--json"]);
    let live_config = fs::read_to_string(state.join("live-dashboard.json")).unwrap();
    let remote_rejected = run_managed_pam(
        &root,
        &state,
        port,
        &[
            "dashboard:start",
            "--listen",
            "0.0.0.0:9616",
            "--token-file",
            dashboard_token.to_str().unwrap(),
        ],
    );
    let live_stopped = run_managed_pam(&root, &state, port, &["dashboard:stop", "--json"]);
    let stopped_status = run_managed_pam(&root, &state, port, &["dashboard:status", "--json"]);
    let dashboard = state.join("managed-dashboard.html");
    let dashboard_created = run_managed_pam(
        &root,
        &state,
        port,
        &["dashboard", "--output", dashboard.to_str().unwrap()],
    );
    let dashboard_overwrite = run_managed_pam(
        &root,
        &state,
        port,
        &["dashboard", "--output", dashboard.to_str().unwrap()],
    );
    let history_path = state.join("history/managed-smoke.json");
    let history_bytes = fs::read(&history_path).unwrap();
    #[cfg(unix)]
    let history_mode = {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(&history_path).unwrap().permissions().mode() & 0o777
    };
    let saved = run_managed_pam(&root, &state, port, &["save", "--json"]);
    let stopped = run_managed_pam(&root, &state, port, &["stop", "managed-smoke", "--json"]);
    thread::sleep(Duration::from_millis(600));
    let intentionally_stopped =
        run_managed_pam(&root, &state, port, &["status", "managed-smoke", "--json"]);
    let resurrected = run_managed_pam(&root, &state, port, &["resurrect", "--json"]);
    let stopped_again = run_managed_pam(&root, &state, port, &["stop", "managed-smoke", "--json"]);
    let deleted = run_managed_pam(&root, &state, port, &["delete", "managed-smoke", "--json"]);

    assert!(queried_logs.status.success());
    let queried_logs: serde_json::Value = serde_json::from_slice(&queried_logs.stdout).unwrap();
    assert_eq!(queried_logs["schemaVersion"], 1);
    assert_eq!(queried_logs["entries"].as_array().unwrap().len(), 3);
    assert_eq!(queried_logs["entries"][0]["rotatedIndex"], 1);
    assert_eq!(queried_logs["entries"][0]["streamCode"], 1);
    assert_eq!(queried_logs["entries"][2]["streamCode"], 2);
    assert_eq!(queried_logs["truncated"], false);
    assert!(!queried_logs.to_string().contains("/logs/"));
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    assert!(
        scaled.status.success(),
        "{}",
        String::from_utf8_lossy(&scaled.stderr)
    );
    assert!(
        reloaded.status.success(),
        "{}",
        String::from_utf8_lossy(&reloaded.stderr)
    );
    assert!(
        restarted.status.success(),
        "{}",
        String::from_utf8_lossy(&restarted.stderr)
    );
    assert!(history_recorded.status.success());
    let resource_history: serde_json::Value =
        serde_json::from_slice(&history_recorded.stdout).unwrap();
    assert_eq!(resource_history["schemaVersion"], 1);
    assert_eq!(resource_history["sampleIntervalSeconds"], 60);
    assert_eq!(resource_history["retentionLimit"], 120);
    assert_eq!(resource_history["applications"][0]["name"], "managed-smoke");
    assert_eq!(
        resource_history["applications"][0]["entries"][0]["stateCode"],
        1
    );
    assert!(
        resource_history["applications"][0]["entries"][0]["rssBytes"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(history_bytes.len() < 1024 * 1024);
    let history_text = String::from_utf8(history_bytes).unwrap();
    assert!(!history_text.contains(script));
    assert!(!history_text.contains("needle-new-output"));
    #[cfg(unix)]
    assert_eq!(history_mode, 0o600);
    assert!(unauthorized_dashboard.starts_with("HTTP/1.1 401 Unauthorized"));
    assert!(!weak_token_rejected.status.success());
    assert!(String::from_utf8_lossy(&weak_token_rejected.stderr).contains("owner-only"));
    assert!(unauthorized_dashboard.contains("WWW-Authenticate: Basic"));
    assert!(wrong_dashboard.starts_with("HTTP/1.1 401 Unauthorized"));
    assert!(authorized_dashboard.starts_with("HTTP/1.1 200 OK"));
    assert!(authorized_dashboard.contains("Content-Security-Policy: default-src 'none'"));
    assert!(authorized_dashboard.contains("Cache-Control: no-store"));
    assert!(authorized_dashboard.contains("Live local view"));
    assert!(authorized_dashboard.contains("managed-smoke"));
    assert!(!authorized_dashboard.contains(script));
    assert!(!authorized_dashboard.contains(token));
    assert!(!authorized_dashboard.contains("<script"));
    assert!(healthy_dashboard.starts_with("HTTP/1.1 200 OK"));
    assert!(healthy_dashboard.contains("\"stateCode\":1"));
    assert!(live_status.status.success());
    let live_status: serde_json::Value = serde_json::from_slice(&live_status.stdout).unwrap();
    assert_eq!(live_status["stateCode"], 1);
    assert_eq!(live_status["online"], true);
    assert!(!live_status.to_string().contains(token));
    assert!(!live_config.contains(token));
    assert!(!live_config.contains(dashboard_token.to_str().unwrap()));
    assert!(!remote_rejected.status.success());
    assert!(String::from_utf8_lossy(&remote_rejected.stderr).contains("loopback"));
    assert!(live_stopped.status.success());
    assert!(!stopped_status.status.success());
    let stopped_status: serde_json::Value = serde_json::from_slice(&stopped_status.stdout).unwrap();
    assert_eq!(stopped_status["stateCode"], 2);
    assert!(!state.join("runtime/live-dashboard.json").exists());
    assert!(!state.join("live-dashboard.json").exists());
    assert!(
        dashboard_created.status.success(),
        "{}",
        String::from_utf8_lossy(&dashboard_created.stderr)
    );
    assert!(!dashboard_overwrite.status.success());
    assert!(
        String::from_utf8_lossy(&dashboard_overwrite.stderr)
            .contains("cannot create new manager dashboard")
    );
    let dashboard_html = fs::read_to_string(&dashboard).unwrap();
    assert!(dashboard_html.len() < 2 * 1024 * 1024);
    assert!(dashboard_html.contains("managed-smoke"));
    assert!(dashboard_html.contains("Online"));
    assert!(dashboard_html.contains("Resident memory"));
    assert!(!dashboard_html.contains("<script"));
    assert!(!dashboard_html.contains(script));
    assert!(!dashboard_html.contains("needle-new-output"));
    assert!(dashboard_html.contains("Peak RSS"));
    assert!(dashboard_html.contains("Stable"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&dashboard).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    assert!(
        saved.status.success(),
        "{}",
        String::from_utf8_lossy(&saved.stderr)
    );
    assert!(
        stopped.status.success(),
        "{}",
        String::from_utf8_lossy(&stopped.stderr)
    );
    assert!(!intentionally_stopped.status.success());
    let intentionally_stopped: serde_json::Value =
        serde_json::from_slice(&intentionally_stopped.stdout).unwrap();
    assert_eq!(intentionally_stopped["stateCode"], 2);
    assert_eq!(intentionally_stopped["desiredStateCode"], 2);
    assert_eq!(intentionally_stopped["recovery"]["stateCode"], 5);
    assert!(
        resurrected.status.success(),
        "{}",
        String::from_utf8_lossy(&resurrected.stderr)
    );
    assert!(stopped_again.status.success());
    assert!(
        deleted.status.success(),
        "{}",
        String::from_utf8_lossy(&deleted.stderr)
    );
    let started = started_snapshot;
    let listed: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    let scaled: serde_json::Value = serde_json::from_slice(&scaled.stdout).unwrap();
    let restarted: serde_json::Value = serde_json::from_slice(&restarted.stdout).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&saved.stdout).unwrap();
    let resurrected: serde_json::Value = serde_json::from_slice(&resurrected.stdout).unwrap();
    assert_eq!(started["kindCode"], 1);
    assert_eq!(started["stateCode"], 1);
    assert_eq!(listed["applications"][0]["name"], "managed-smoke");
    assert_eq!(scaled["workers"], 2);
    assert_eq!(saved["applications"][0]["desiredStateCode"], 1);
    assert_eq!(resurrected["resurrected"][0], "managed-smoke");
    assert_ne!(started["pid"], restarted["pid"]);
    assert!(!state.join("applications/managed-smoke.json").exists());
    assert!(!state.join("history/managed-smoke.json").exists());
    let daemon_stopped = run_managed_pam(&root, &state, port, &["daemon", "stop"]);
    assert!(daemon_stopped.status.success());
    fs::remove_dir_all(state).unwrap();
}

#[test]
fn manages_a_private_per_user_daemon() {
    let root = temporary_path("manager-daemon");
    let state = root.join("state");
    let runtime = root.join("runtime");
    let started = run_manager_daemon(&state, &runtime, "start");
    assert!(
        started.status.success(),
        "{}",
        String::from_utf8_lossy(&started.stderr)
    );
    let status = run_manager_daemon(&state, &runtime, "status");
    assert!(status.status.success());
    assert!(String::from_utf8_lossy(&status.stdout).contains("pamd is online"));
    #[cfg(unix)]
    {
        use std::net::Shutdown;
        use std::os::unix::fs::PermissionsExt;
        use std::os::unix::net::UnixStream;
        assert_eq!(
            fs::metadata(&runtime).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(runtime.join("pamd.sock"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let mut hostile = UnixStream::connect(runtime.join("pamd.sock")).unwrap();
        hostile.write_all(b"not-json").unwrap();
        hostile.shutdown(Shutdown::Write).unwrap();
        let mut response = String::new();
        hostile.read_to_string(&mut response).unwrap();
        let response: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(response["ok"], false);
        assert!(
            run_manager_daemon(&state, &runtime, "status")
                .status
                .success()
        );
    }
    let stopped = run_manager_daemon(&state, &runtime, "stop");
    assert!(stopped.status.success());
    assert!(!runtime.join("pamd.sock").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn replaces_a_live_master_after_bounded_health_check_failures() {
    let state = temporary_path("manager-health-check");
    let probe = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let health_url = format!("http://127.0.0.1:{port}/block");
    let started = run_managed_pam(
        &root,
        &state,
        port,
        &[
            "up",
            fixture("server.php").to_str().unwrap(),
            "--name",
            "unhealthy-smoke",
            "--health-check-url",
            &health_url,
            "--health-check-interval-ms",
            "250",
            "--health-check-timeout-ms",
            "50",
            "--health-check-start-period-ms",
            "1000",
            "--health-check-failures",
            "2",
            "--json",
        ],
    );
    assert!(
        started.status.success(),
        "{}",
        String::from_utf8_lossy(&started.stderr)
    );
    let started: serde_json::Value = serde_json::from_slice(&started.stdout).unwrap();
    let original_pid = started["pid"].as_u64().unwrap();
    assert_eq!(started["healthCheck"]["configured"], true);
    assert_eq!(started["healthCheck"]["stateCode"], 5);
    assert_eq!(started["healthCheck"]["startPeriodMillis"], 1000);
    assert!(!started.to_string().contains("/block"));

    thread::sleep(Duration::from_millis(600));
    let warming = run_managed_pam(
        &root,
        &state,
        port,
        &["status", "unhealthy-smoke", "--json"],
    );
    let warming: serde_json::Value = serde_json::from_slice(&warming.stdout).unwrap();
    assert_eq!(warming["pid"].as_u64(), Some(original_pid));
    assert_eq!(warming["healthCheck"]["stateCode"], 5);
    assert_eq!(
        warming["healthCheck"]["lastCheckedAtMillis"],
        serde_json::Value::Null
    );
    assert_eq!(warming["healthCheck"]["totalUnhealthyRestartCount"], 0);

    let recovered = (0..100)
        .find_map(|_| {
            thread::sleep(Duration::from_millis(100));
            let status = run_managed_pam(
                &root,
                &state,
                port,
                &["status", "unhealthy-smoke", "--json"],
            );
            let snapshot: serde_json::Value = serde_json::from_slice(&status.stdout).ok()?;
            (status.status.success()
                && snapshot["pid"].as_u64() != Some(original_pid)
                && snapshot["healthCheck"]["totalUnhealthyRestartCount"]
                    .as_u64()
                    .unwrap_or(0)
                    >= 1
                && snapshot["recovery"]["totalAutoRestartCount"]
                    .as_u64()
                    .unwrap_or(0)
                    >= 1)
                .then_some(snapshot)
        })
        .expect("pamd should replace a live but unhealthy master");
    assert_eq!(recovered["desiredStateCode"], 1);

    assert!(
        run_managed_pam(&root, &state, port, &["stop", "unhealthy-smoke"])
            .status
            .success()
    );
    assert!(
        run_managed_pam(&root, &state, port, &["delete", "unhealthy-smoke"])
            .status
            .success()
    );
    assert!(
        run_managed_pam(&root, &state, port, &["daemon", "stop"])
            .status
            .success()
    );
    fs::remove_dir_all(state).unwrap();
}

#[test]
fn validates_and_reconciles_a_multi_application_pam_toml() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let state = temporary_path("ecosystem-state");
    let config = root.join(format!("pam-test-{}.toml", std::process::id()));
    fs::write(
        &config,
        r#"schema_version = 1

[applications.ecosystem-smoke]
kind_code = 1
script = "tests/fixtures/server.php"
workers = 1
cwd = "."
autostart = true
php_extensions = ["iconv"]
memory_warning_bytes = 1
task_warning_count = 1
"#,
    )
    .unwrap();
    let probe = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);

    let checked = run_managed_pam(
        &root,
        &state,
        port,
        &["config:check", config.to_str().unwrap(), "--json"],
    );
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let applied = run_managed_pam(
        &root,
        &state,
        port,
        &["apply", config.to_str().unwrap(), "--json"],
    );
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let converged = run_managed_pam(
        &root,
        &state,
        port,
        &["apply", config.to_str().unwrap(), "--json"],
    );
    assert!(converged.status.success());
    let applied: serde_json::Value = serde_json::from_slice(&applied.stdout).unwrap();
    let converged: serde_json::Value = serde_json::from_slice(&converged.stdout).unwrap();
    assert_eq!(applied["results"][0]["actionCode"], 1);
    assert_eq!(converged["results"][0]["actionCode"], 2);
    let monitored = run_managed_pam(&root, &state, port, &["monit", "--json"]);
    assert!(monitored.status.success());
    let monitored: serde_json::Value = serde_json::from_slice(&monitored.stdout).unwrap();
    assert_eq!(monitored["applications"][0]["resourceAlertStateCode"], 4);
    assert!(
        monitored["applications"][0]["resources"]["rssBytes"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(
        monitored["applications"][0]["resources"]["tasks"]
            .as_u64()
            .unwrap()
            > 0
    );

    fs::write(
        &config,
        r#"schema_version = 1

[applications.ecosystem-smoke]
kind_code = 1
script = "tests/fixtures/server.php"
workers = 1
cwd = "."
autostart = true
php_extensions = ["iconv"]
memory_warning_bytes = 1099511627776
task_warning_count = 1000000
"#,
    )
    .unwrap();
    let updated = run_managed_pam(
        &root,
        &state,
        port,
        &["apply", config.to_str().unwrap(), "--json"],
    );
    assert!(updated.status.success());
    let updated: serde_json::Value = serde_json::from_slice(&updated.stdout).unwrap();
    assert_eq!(updated["results"][0]["actionCode"], 6);
    let described = run_managed_pam(
        &root,
        &state,
        port,
        &["describe", "ecosystem-smoke", "--json"],
    );
    let described: serde_json::Value = serde_json::from_slice(&described.stdout).unwrap();
    assert_eq!(described["resourceAlertStateCode"], 1);
    assert_eq!(described["phpExtensions"], serde_json::json!(["iconv"]));
    assert_eq!(described["resourcePolicy"]["taskWarningCount"], 1000000);

    let stopped = run_managed_pam(&root, &state, port, &["stop", "ecosystem-smoke", "--json"]);
    assert!(stopped.status.success());
    let deleted = run_managed_pam(
        &root,
        &state,
        port,
        &["delete", "ecosystem-smoke", "--json"],
    );
    assert!(deleted.status.success());
    let daemon_stopped = run_managed_pam(&root, &state, port, &["daemon", "stop"]);
    assert!(daemon_stopped.status.success());
    fs::remove_file(config).unwrap();
    fs::remove_dir_all(state).unwrap();
}

#[test]
fn deploys_idempotently_and_rolls_back_to_a_healthy_release() {
    let root = temporary_path("deploy-releases");
    let release_one = root.join("release-1");
    let release_two = root.join("release-2");
    fs::create_dir_all(&release_one).unwrap();
    fs::create_dir_all(&release_two).unwrap();
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let autoload = repository.join("compat/composer-smoke/vendor/autoload.php");
    let fixture = fs::read_to_string(fixture("server.php")).unwrap();
    let fixture = fixture.replace(
        "__DIR__ . '/../../compat/composer-smoke/vendor/autoload.php'",
        &format!("'{}'", autoload.display()),
    );
    fs::write(release_one.join("server.php"), &fixture).unwrap();
    fs::write(release_two.join("server.php"), &fixture).unwrap();
    let state = root.join("state");
    let probe = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);

    let started = run_managed_pam(
        &release_one,
        &state,
        port,
        &[
            "up",
            "server.php",
            "--name",
            "deploy-smoke",
            "--workers",
            "1",
            "--json",
        ],
    );
    assert!(
        started.status.success(),
        "{}",
        String::from_utf8_lossy(&started.stderr)
    );
    let deployed = run_managed_pam(
        &root,
        &state,
        port,
        &[
            "deploy",
            "deploy-smoke",
            release_two.to_str().unwrap(),
            "--json",
        ],
    );
    assert!(
        deployed.status.success(),
        "{}",
        String::from_utf8_lossy(&deployed.stderr)
    );
    let unchanged = run_managed_pam(
        &root,
        &state,
        port,
        &[
            "deploy",
            "deploy-smoke",
            release_two.to_str().unwrap(),
            "--json",
        ],
    );
    let history = run_managed_pam(
        &root,
        &state,
        port,
        &["deploy:history", "deploy-smoke", "--json"],
    );
    let rolled_back = run_managed_pam(&root, &state, port, &["rollback", "deploy-smoke", "--json"]);
    assert!(unchanged.status.success());
    assert!(history.status.success());
    assert!(
        rolled_back.status.success(),
        "{}",
        String::from_utf8_lossy(&rolled_back.stderr)
    );
    let deployed: serde_json::Value = serde_json::from_slice(&deployed.stdout).unwrap();
    let unchanged: serde_json::Value = serde_json::from_slice(&unchanged.stdout).unwrap();
    let history: serde_json::Value = serde_json::from_slice(&history.stdout).unwrap();
    let rolled_back: serde_json::Value = serde_json::from_slice(&rolled_back.stdout).unwrap();
    assert_eq!(deployed["actionCode"], 1);
    assert_eq!(unchanged["actionCode"], 3);
    assert_eq!(history["entries"].as_array().unwrap().len(), 2);
    assert_eq!(rolled_back["actionCode"], 2);
    assert_eq!(
        rolled_back["releaseDirectory"],
        release_one.to_str().unwrap()
    );

    assert!(
        run_managed_pam(&root, &state, port, &["stop", "deploy-smoke"])
            .status
            .success()
    );
    assert!(
        run_managed_pam(&root, &state, port, &["delete", "deploy-smoke"])
            .status
            .success()
    );
    assert!(
        run_managed_pam(&root, &state, port, &["daemon", "stop"])
            .status
            .success()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn shifts_aborts_and_promotes_weighted_release_traffic() {
    let (stable_port, stable_running, stable_thread) = fixed_http_server("stable");
    let (candidate_port, candidate_running, candidate_thread) = fixed_http_server("candidate");
    let probe = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let ingress_port = probe.local_addr().unwrap().port();
    drop(probe);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let state = temporary_path("traffic-state");
    let started = run_managed_pam(
        &root,
        &state,
        ingress_port,
        &[
            "traffic:start",
            "edge-smoke",
            "--listen",
            &format!("127.0.0.1:{ingress_port}"),
            "--stable",
            &format!("127.0.0.1:{stable_port}"),
            "--candidate",
            &format!("127.0.0.1:{candidate_port}"),
            "--weight-bps",
            "5000",
            "--json",
        ],
    );
    assert!(
        started.status.success(),
        "{}",
        String::from_utf8_lossy(&started.stderr)
    );
    let mut stable_seen = false;
    let mut candidate_seen = false;
    for index in 0..100 {
        let response = traffic_request(ingress_port, &index.to_string());
        stable_seen |= response.ends_with("stable");
        candidate_seen |= response.ends_with("candidate");
    }
    assert!(stable_seen && candidate_seen);
    thread::sleep(Duration::from_millis(600));
    let status = run_managed_pam(
        &root,
        &state,
        ingress_port,
        &["traffic:status", "edge-smoke", "--json"],
    );
    assert!(status.status.success());
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["rolloutPhaseCode"], 2);
    assert_eq!(status["metrics"]["generation"], status["generation"]);
    assert_eq!(
        status["metrics"]["stableRequests"].as_u64().unwrap()
            + status["metrics"]["candidateRequests"].as_u64().unwrap(),
        100
    );

    let aborted = run_managed_pam(
        &root,
        &state,
        ingress_port,
        &["traffic:abort", "edge-smoke", "--json"],
    );
    assert!(aborted.status.success());
    thread::sleep(Duration::from_millis(300));
    for index in 0..10 {
        assert!(traffic_request(ingress_port, &index.to_string()).ends_with("stable"));
    }

    let candidate = format!("127.0.0.1:{candidate_port}");
    let shifted = run_managed_pam(
        &root,
        &state,
        ingress_port,
        &[
            "traffic:set",
            "edge-smoke",
            "--candidate",
            &candidate,
            "--weight-bps",
            "10000",
            "--deadline-seconds",
            "1",
            "--json",
        ],
    );
    assert!(shifted.status.success());
    thread::sleep(Duration::from_millis(300));
    assert!(traffic_request(ingress_port, "deadline").ends_with("candidate"));
    thread::sleep(Duration::from_millis(900));
    let expired = run_managed_pam(
        &root,
        &state,
        ingress_port,
        &[
            "traffic:evaluate",
            "edge-smoke",
            "--min-candidate-requests",
            "1",
            "--max-candidate-error-bps",
            "0",
            "--json",
        ],
    );
    assert!(expired.status.success());
    let expired: serde_json::Value = serde_json::from_slice(&expired.stdout).unwrap();
    assert_eq!(expired["decisionCode"], 4);
    let shifted = run_managed_pam(
        &root,
        &state,
        ingress_port,
        &[
            "traffic:set",
            "edge-smoke",
            "--candidate",
            &candidate,
            "--weight-bps",
            "10000",
            "--deadline-seconds",
            "300",
            "--json",
        ],
    );
    assert!(shifted.status.success());
    thread::sleep(Duration::from_millis(300));
    for index in 0..10 {
        assert!(traffic_request(ingress_port, &format!("gate-{index}")).ends_with("candidate"));
    }
    thread::sleep(Duration::from_millis(600));
    let pending = run_managed_pam(
        &root,
        &state,
        ingress_port,
        &[
            "traffic:evaluate",
            "edge-smoke",
            "--min-candidate-requests",
            "100",
            "--max-candidate-error-bps",
            "0",
            "--json",
        ],
    );
    assert_eq!(pending.status.code(), Some(1));
    let pending: serde_json::Value = serde_json::from_slice(&pending.stdout).unwrap();
    assert_eq!(pending["decisionCode"], 1);
    let promoted = run_managed_pam(
        &root,
        &state,
        ingress_port,
        &[
            "traffic:evaluate",
            "edge-smoke",
            "--min-candidate-requests",
            "10",
            "--max-candidate-error-bps",
            "0",
            "--json",
        ],
    );
    assert!(promoted.status.success());
    let promoted: serde_json::Value = serde_json::from_slice(&promoted.stdout).unwrap();
    assert_eq!(promoted["decisionCode"], 2);
    thread::sleep(Duration::from_millis(300));
    assert!(traffic_request(ingress_port, "after").ends_with("candidate"));
    let status = run_managed_pam(
        &root,
        &state,
        ingress_port,
        &["traffic:status", "edge-smoke", "--json"],
    );
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["rolloutPhaseCode"], 3);
    assert_eq!(status["lastRolloutDecisionCode"], 2);

    assert!(
        run_managed_pam(&root, &state, ingress_port, &["traffic:stop", "edge-smoke"])
            .status
            .success()
    );
    assert!(
        run_managed_pam(&root, &state, ingress_port, &["daemon", "stop"])
            .status
            .success()
    );
    stable_running.store(false, Ordering::Relaxed);
    candidate_running.store(false, Ordering::Relaxed);
    stable_thread.join().unwrap();
    candidate_thread.join().unwrap();
    fs::remove_dir_all(state).unwrap();
}

#[test]
fn terminates_tls_for_release_traffic_without_exposing_key_paths() {
    let (stable_port, stable_running, stable_thread) = fixed_http_server("secure-stable");
    let probe = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let ingress_port = probe.local_addr().unwrap().port();
    drop(probe);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let state = temporary_path("traffic-tls-state");
    let identity = temporary_path("traffic-tls-identity");
    fs::create_dir_all(&identity).unwrap();
    let certificate = identity.join("certificate.pem");
    let private_key = identity.join("private-key.pem");
    let generated = Command::new("openssl")
        .args(["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout"])
        .arg(&private_key)
        .arg("-out")
        .arg(&certificate)
        .args([
            "-subj",
            "/CN=localhost",
            "-addext",
            "subjectAltName=DNS:localhost",
            "-addext",
            "basicConstraints=critical,CA:FALSE",
            "-days",
            "1",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("openssl should be installed for TLS integration tests");
    assert!(generated.success());
    let started = run_managed_pam(
        &root,
        &state,
        ingress_port,
        &[
            "traffic:start",
            "secure-edge",
            "--listen",
            &format!("127.0.0.1:{ingress_port}"),
            "--stable",
            &format!("127.0.0.1:{stable_port}"),
            "--tls-cert",
            certificate.to_str().unwrap(),
            "--tls-key",
            private_key.to_str().unwrap(),
            "--json",
        ],
    );
    assert!(
        started.status.success(),
        "{}",
        String::from_utf8_lossy(&started.stderr)
    );
    let status: serde_json::Value = serde_json::from_slice(&started.stdout).unwrap();
    assert_eq!(status["tlsEnabled"], true);
    assert!(!String::from_utf8_lossy(&started.stdout).contains(private_key.to_str().unwrap()));
    let response = Command::new("curl")
        .args(["--silent", "--show-error", "--fail", "--cacert"])
        .arg(&certificate)
        .arg(format!("https://localhost:{ingress_port}/tls"))
        .output()
        .expect("curl should be installed for TLS integration tests");
    assert!(
        response.status.success(),
        "{}",
        String::from_utf8_lossy(&response.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&response.stdout), "secure-stable");
    assert!(
        run_managed_pam(
            &root,
            &state,
            ingress_port,
            &["traffic:stop", "secure-edge"]
        )
        .status
        .success()
    );
    assert!(
        run_managed_pam(&root, &state, ingress_port, &["daemon", "stop"])
            .status
            .success()
    );
    stable_running.store(false, Ordering::Relaxed);
    stable_thread.join().unwrap();
    fs::remove_dir_all(state).unwrap();
    fs::remove_dir_all(identity).unwrap();
}

#[test]
fn evaluates_bounded_overload_evidence() {
    let results = temporary_path("overload-evidence");
    fs::create_dir_all(&results).unwrap();
    let samples = results.join("samples.tsv");
    let report = results.join("overload-report.json");
    fs::write(&samples, "200\t\t0.051\n503\t1\t0.002\n503\t1\t0.003\n").unwrap();
    let script =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benchmarks/octane/overload-report.php");

    let accepted = run_pam(&[
        script.to_str().unwrap(),
        samples.to_str().unwrap(),
        report.to_str().unwrap(),
        "200",
    ]);
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    let evidence: serde_json::Value = serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
    assert_eq!(evidence["passed"], true);
    assert_eq!(evidence["requests"], 3);
    assert_eq!(evidence["status_counts"][0]["status"], 200);
    assert_eq!(evidence["status_counts"][1]["status"], 503);
    assert_eq!(evidence["retry_after_missing"], 0);
    assert_eq!(evidence["recovery_status"], 200);

    fs::write(&samples, "200\t\t0.051\n503\t\t0.002\n").unwrap();
    let rejected = run_pam(&[
        script.to_str().unwrap(),
        samples.to_str().unwrap(),
        report.to_str().unwrap(),
        "200",
    ]);
    assert!(!rejected.status.success());
    let evidence: serde_json::Value = serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
    assert_eq!(evidence["passed"], false);
    assert_eq!(evidence["retry_after_missing"], 1);
    fs::remove_dir_all(results).unwrap();
}

#[test]
fn executes_a_php_file() {
    let output = run_pam(&[fixture("hello.php").to_str().unwrap()]);

    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "Hello from PHP!\n");
}

#[test]
fn executes_inline_php_with_ini_entries() {
    let output = run_pam(&[
        "-d",
        "precision=6",
        "-r",
        "echo PHP_SAPI, '|', ini_get('precision'), '|', basename(PHP_BINARY);",
    ]);

    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "embed|6|pam");
}

#[test]
fn accepts_php_cli_ini_options_for_composer_tool_workers() {
    let ini = temporary_path("tool-worker.ini");
    fs::write(&ini, "precision=7\n").unwrap();
    let output = run_pam(&[
        "-c",
        ini.to_str().unwrap(),
        "-d",
        "memory_limit=256M",
        "-r",
        "echo ini_get('precision'), '|', ini_get('memory_limit');",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "7|256M");
    fs::remove_file(ini).unwrap();
}

#[test]
fn initializes_and_discovers_a_contextual_native_project() {
    let parent = temporary_path("contextual-native");
    let project = parent.join("shop");
    fs::create_dir_all(&parent).unwrap();
    let created = run_pam_in(
        &parent,
        &[
            "init",
            "shop",
            "--template",
            "mobile",
            "--name",
            "PAM Shop",
            "--application-id",
            "com.example.shop",
            "--starter",
            "ecommerce",
            "--platform",
            "android",
            "--no-install",
            "--no-interaction",
        ],
    );
    assert!(
        created.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&created.stdout),
        String::from_utf8_lossy(&created.stderr),
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(project.join("pam.json")).unwrap()).unwrap();
    assert_eq!(manifest["type"], 2);
    assert_eq!(manifest["name"], "shop");
    assert_eq!(manifest["native"]["applicationId"], "com.example.shop");
    assert_eq!(manifest["native"]["starter"], 4);
    assert_eq!(manifest["native"]["platforms"], serde_json::json!([1]));
    let composer: serde_json::Value =
        serde_json::from_slice(&fs::read(project.join("composer.json")).unwrap()).unwrap();
    assert_eq!(composer["require"]["pushinbr/pam-native"], "^0.6");
    let native: serde_json::Value =
        serde_json::from_slice(&fs::read(project.join("pam-native.json")).unwrap()).unwrap();
    assert_eq!(native["applicationId"], "com.example.shop");
    assert_eq!(native["name"], "PAM Shop");
    assert!(native.get("starter").is_none());
    assert!(native.get("platforms").is_none());

    let info = run_pam_in(&project, &["info", "--json"]);
    assert!(
        info.status.success(),
        "{}",
        String::from_utf8_lossy(&info.stderr)
    );
    let payload: serde_json::Value = serde_json::from_slice(&info.stdout).unwrap();
    assert_eq!(payload["type"], 2);
    assert_eq!(payload["typeLabel"], "PAM Native");
    assert_eq!(payload["root"], project.to_string_lossy().as_ref());
    assert_eq!(payload["developmentArtifacts"]["exists"], false);
    assert_eq!(payload["developmentArtifacts"]["bytes"], 0);
    assert_eq!(payload["artifactFootprint"]["bytes"], 0);
    assert_eq!(payload["artifactFootprint"]["files"], 0);
    assert_eq!(payload["artifactFootprint"]["complete"], true);
    assert_eq!(payload["artifactBudget"]["limitBytes"], 8_589_934_592_u64);
    assert_eq!(payload["artifactBudget"]["stateCode"], 1);
    assert_eq!(
        payload["artifactBudget"]["cleanupCommand"],
        "pam clean --all"
    );
    assert_eq!(
        payload["nextCommands"],
        serde_json::json!(["pam doctor", "pam dev", "pam test", "pam build"])
    );
    fs::create_dir_all(project.join(".pam-native/android/app/build")).unwrap();
    fs::write(
        project.join(".pam-native/android/app/build/artifact.bin"),
        [0_u8; 64],
    )
    .unwrap();
    let measured = run_pam_in(&project, &["info", "--json"]);
    assert!(measured.status.success());
    let measured: serde_json::Value = serde_json::from_slice(&measured.stdout).unwrap();
    assert_eq!(measured["developmentArtifacts"]["exists"], true);
    assert_eq!(measured["developmentArtifacts"]["bytes"], 64);
    assert_eq!(measured["developmentArtifacts"]["files"], 1);
    assert_eq!(measured["developmentArtifacts"]["complete"], true);
    assert_eq!(measured["artifactFootprint"]["bytes"], 64);
    assert_eq!(measured["artifactFootprint"]["files"], 1);
    assert!(
        measured["artifactFootprint"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["path"] == ".pam-native/android")
    );
    let preview = run_pam_in(&project, &["clean", "--dry-run", "--json"]);
    assert!(preview.status.success());
    let preview: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(preview["schemaVersion"], 1);
    assert_eq!(preview["resultCode"], 1);
    assert_eq!(preview["operationCode"], 1);
    assert_eq!(preview["projectTypeCode"], 2);
    assert_eq!(preview["bytes"], 64);
    assert!(
        project
            .join(".pam-native/android/app/build/artifact.bin")
            .is_file()
    );
    let cleaned = run_pam_in(&project, &["clean", "--json"]);
    assert!(cleaned.status.success());
    let cleaned: serde_json::Value = serde_json::from_slice(&cleaned.stdout).unwrap();
    assert_eq!(cleaned["operationCode"], 2);
    assert_eq!(cleaned["bytes"], 64);
    let android_build = cleaned["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["path"] == ".pam-native/android/app/build")
        .unwrap();
    assert_eq!(android_build["kindCode"], 2);
    assert_eq!(android_build["removed"], true);
    assert!(!project.join(".pam-native/android/app/build").exists());
    assert!(project.join("index.php").is_file());
    fs::remove_dir_all(parent).unwrap();
}

#[test]
fn discovers_ecosystem_and_runs_extensible_project_commands() {
    let packages = run_pam(&["packages", "--json"]);
    assert!(packages.status.success());
    let catalog: serde_json::Value = serde_json::from_slice(&packages.stdout).unwrap();
    assert_eq!(catalog["schema"], 1);
    assert!(
        catalog["packages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|package| {
                package["alias"] == "native" && package["composer"] == "pushinbr/pam-native"
            })
    );

    let project = temporary_path("custom-command");
    fs::create_dir_all(project.join("bin")).unwrap();
    fs::write(
        project.join("pam.json"),
        r#"{"schema":1,"type":5,"name":"commands","commands":{"app:greet":{"script":"bin/greet.php","description":"Greet from the app"},"make:report":{"script":"bin/greet.php","description":"Generate a report"}}}"#,
    )
    .unwrap();
    fs::write(
        project.join("bin/greet.php"),
        "<?php echo 'hello '.($argv[1] ?? 'world').'!';\n",
    )
    .unwrap();

    let commands = run_pam_in(&project, &["commands", "--json"]);
    assert!(commands.status.success());
    let commands: serde_json::Value = serde_json::from_slice(&commands.stdout).unwrap();
    assert!(
        commands["commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|command| command["name"] == "app:greet")
    );
    let greeting = run_pam_in(&project, &["app:greet", "PAM"]);
    assert!(greeting.status.success());
    assert_eq!(String::from_utf8_lossy(&greeting.stdout), "hello PAM!");
    let generated = run_pam_in(&project, &["make:report", "monthly"]);
    assert!(generated.status.success());
    assert_eq!(String::from_utf8_lossy(&generated.stdout), "hello monthly!");
    fs::remove_dir_all(project).unwrap();
}

#[test]
fn exposes_native_diagnostics_and_builds_a_portable_bundle() {
    let diagnostics = run_pam(&["diagnostics", fixture("hello.php").to_str().unwrap()]);
    assert!(
        diagnostics.status.success(),
        "{}",
        String::from_utf8_lossy(&diagnostics.stderr)
    );
    let snapshot: serde_json::Value = serde_json::from_slice(&diagnostics.stdout).unwrap();
    assert_eq!(snapshot["schemaVersion"], 1);
    assert_eq!(snapshot["surfaceCode"], 1);
    assert!(snapshot["capturedAtUnixMs"].as_u64().is_some());
    assert!(snapshot["memory"]["allocatedBytes"].as_u64().is_some());
    assert_eq!(snapshot["fibers"]["pending"], 0);
    for (command, expected_key) in [
        ("heap", "allocatedBytes"),
        ("fibers", "pending"),
        ("connections", "httpDispatches"),
        ("profile", "profiles"),
        ("trace", "events"),
    ] {
        let output = run_pam(&[command, fixture("hello.php").to_str().unwrap()]);
        assert!(
            output.status.success(),
            "{command}: {}",
            String::from_utf8_lossy(&output.stderr),
        );
        let snapshot: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(
            snapshot.get(expected_key).is_some(),
            "{command}: {snapshot}"
        );
    }

    let project = temporary_path("build-project");
    let bundle = temporary_path("build-bundle");
    fs::create_dir(&project).unwrap();
    fs::copy(fixture("hello.php"), project.join("index.php")).unwrap();
    fs::write(
        project.join("pam.json"),
        r#"{"schema":1,"type":5,"name":"portable-test","version":"1.2.3"}"#,
    )
    .unwrap();
    let output = run_pam(&[
        "build",
        project.to_str().unwrap(),
        "--output",
        bundle.to_str().unwrap(),
    ]);
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(bundle.join("manifest.json").is_file());
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(bundle.join("manifest.json")).unwrap()).unwrap();
    let php_library = manifest["phpLibrary"].as_str().unwrap();
    assert!(
        php_library == "embedded" || bundle.join(php_library).is_file(),
        "invalid bundled PHP library: {php_library}"
    );
    let bundled = Command::new(bundle.join("bin/pam-run")).output().unwrap();
    assert!(
        bundled.status.success(),
        "{}",
        String::from_utf8_lossy(&bundled.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&bundled.stdout),
        "Hello from PHP!\n"
    );
    let packaged = run_pam_in(&project, &["package"]);
    assert!(
        packaged.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&packaged.stdout),
        String::from_utf8_lossy(&packaged.stderr),
    );
    let archive = project.join(format!(
        "dist/portable-test-1.2.3-{}-{}.tar.gz",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    let checksum = fs::read_to_string(archive.with_extension("gz.sha256")).unwrap();
    assert_eq!(
        checksum.split_whitespace().next().unwrap(),
        format!("{:x}", Sha256::digest(fs::read(&archive).unwrap()))
    );
    fs::remove_dir_all(project).unwrap();
    fs::remove_dir_all(bundle).unwrap();
}

#[cfg(unix)]
#[test]
fn production_build_rejects_symlinks_outside_the_project() {
    use std::os::unix::fs::symlink;

    let project = temporary_path("build-symlink-project");
    let bundle = temporary_path("build-symlink-bundle");
    let outside = temporary_path("build-symlink-secret");
    fs::create_dir(&project).unwrap();
    fs::copy(fixture("hello.php"), project.join("index.php")).unwrap();
    fs::write(&outside, "must-not-be-bundled").unwrap();
    symlink(&outside, project.join("secret.txt")).unwrap();

    let output = run_pam(&[
        "build",
        project.to_str().unwrap(),
        "--output",
        bundle.to_str().unwrap(),
    ]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("symlink escapes the project"),
        "{}",
        String::from_utf8_lossy(&output.stderr),
    );

    fs::remove_dir_all(project).unwrap();
    fs::remove_dir_all(bundle).unwrap();
    fs::remove_file(outside).unwrap();
}

#[test]
fn exposes_the_script_path_and_arguments_to_php() {
    let script = fixture("context.php").canonicalize().unwrap();
    let output = run_pam(&[script.to_str().unwrap(), "first", "second"]);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("{}\nfirst,second\n", script.display())
    );
}

#[test]
fn propagates_php_exit_codes() {
    let output = run_pam(&[fixture("exit.php").to_str().unwrap()]);

    assert_eq!(output.status.code(), Some(7));
}

#[test]
fn treats_an_explicit_zero_exit_as_success() {
    let output = run_pam(&[fixture("exit-zero.php").to_str().unwrap()]);

    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn fails_for_invalid_php() {
    let output = run_pam(&[fixture("syntax-error.php").to_str().unwrap()]);

    assert!(!output.status.success());
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(diagnostics.contains("Parse error"));
}

#[test]
fn reports_a_missing_script_with_ex_noinput() {
    let output = run_pam(&["tests/fixtures/missing.php"]);

    assert_eq!(output.status.code(), Some(66));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot open"));
    assert!(stderr.contains("Verify: pam doctor"));
}

#[test]
fn exposes_stable_structured_error_envelopes() {
    let output = run_pam(&["--json-errors", "missing-script.php"]);

    assert_eq!(output.status.code(), Some(66));
    assert!(output.stderr.is_empty());
    let error: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(error["schema"], 1);
    assert_eq!(error["errorCode"], 1);
    assert_eq!(error["exitCode"], 66);
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("missing-script.php")
    );
    assert!(
        error["remediation"]
            .as_str()
            .unwrap()
            .contains("check that the path")
    );
    assert_eq!(error["verificationCommand"], "pam doctor");

    let usage = run_pam(&["--json-errors", "benchmark"]);
    assert_eq!(usage.status.code(), Some(70));
    let usage: serde_json::Value = serde_json::from_slice(&usage.stdout).unwrap();
    assert_eq!(usage["errorCode"], 5);
    assert!(usage["remediation"].as_str().unwrap().contains("pam help"));
    assert_eq!(usage["verificationCommand"], "pam --help");
}

#[test]
fn exposes_the_authoritative_machine_readable_cli_catalog() {
    let output = run_pam(&["catalog", "--json"]);
    assert!(output.status.success());
    let catalog: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(catalog["schemaVersion"], 1);
    let commands = catalog["commands"].as_array().unwrap();
    assert!(commands.len() >= 60);
    assert!(commands.iter().any(|command| {
        command["name"] == "doctor" && command["groupCode"] == 1 && command["supportsJson"] == true
    }));
    assert!(commands.iter().any(|command| {
        command["name"] == "dev" && command["groupCode"] == 2 && command["supportsJson"] == false
    }));
    assert!(commands.iter().all(|command| {
        command["groupCode"]
            .as_u64()
            .is_some_and(|code| (1..=9).contains(&code))
            && command["groupLabel"]
                .as_str()
                .is_some_and(|label| !label.is_empty())
    }));

    let schema = run_pam(&["catalog", "--schema"]);
    assert!(schema.status.success());
    let schema: serde_json::Value = serde_json::from_slice(&schema.stdout).unwrap();
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["properties"]["schemaVersion"]["const"], 1);
    assert_eq!(
        schema["$defs"]["command"]["properties"]["groupCode"]["enum"],
        serde_json::json!([1, 2, 3, 4, 5, 6, 7, 8, 9])
    );

    let compatibility_schema = run_pam(&["catalog", "--compat-schema"]);
    assert!(compatibility_schema.status.success());
    let compatibility_schema: serde_json::Value =
        serde_json::from_slice(&compatibility_schema.stdout).unwrap();
    assert_eq!(
        compatibility_schema["properties"]["schemaVersion"]["const"],
        1
    );
    assert_eq!(
        compatibility_schema["$defs"]["change"]["properties"]["changeCode"]["enum"],
        serde_json::json!([1, 2, 3])
    );

    let directory = temporary_path("cli-catalog-validation");
    fs::create_dir_all(&directory).unwrap();
    let saved = directory.join("catalog.json");
    fs::write(&saved, &output.stdout).unwrap();
    let validation = run_pam(&["catalog", "--validate", saved.to_str().unwrap(), "--json"]);
    assert!(validation.status.success());
    let validation: serde_json::Value = serde_json::from_slice(&validation.stdout).unwrap();
    assert_eq!(validation["valid"], true);
    assert_eq!(validation["commandCount"], commands.len());

    let compatible = run_pam(&[
        "catalog",
        "--compat",
        saved.to_str().unwrap(),
        saved.to_str().unwrap(),
        "--json",
    ]);
    assert!(compatible.status.success());
    let compatible: serde_json::Value = serde_json::from_slice(&compatible.stdout).unwrap();
    assert_eq!(compatible["compatible"], true);
    assert_eq!(compatible["changes"], serde_json::json!([]));

    let baseline: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut breaking = baseline.clone();
    breaking["commands"].as_array_mut().unwrap().remove(0);
    breaking["commands"][0]["groupCode"] = serde_json::json!(9);
    breaking["commands"][0]["groupLabel"] = serde_json::json!("Advanced");
    let json_command = breaking["commands"]
        .as_array()
        .unwrap()
        .iter()
        .position(|command| command["supportsJson"] == true)
        .unwrap();
    breaking["commands"][json_command]["supportsJson"] = serde_json::json!(false);
    let breaking_path = directory.join("breaking.json");
    fs::write(&breaking_path, serde_json::to_vec(&breaking).unwrap()).unwrap();
    let incompatible = run_pam(&[
        "catalog",
        "--compat",
        saved.to_str().unwrap(),
        breaking_path.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(incompatible.status.code(), Some(1));
    let incompatible: serde_json::Value = serde_json::from_slice(&incompatible.stdout).unwrap();
    assert_eq!(incompatible["compatible"], false);
    let codes = incompatible["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|change| change["changeCode"].as_u64().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(codes, vec![1, 2, 3]);

    let mut additive = baseline.clone();
    additive["commands"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "name": "future-command",
            "summary": "Additive capability",
            "groupCode": 9,
            "groupLabel": "Advanced",
            "supportsJson": true,
        }));
    let additive_path = directory.join("additive.json");
    fs::write(&additive_path, serde_json::to_vec(&additive).unwrap()).unwrap();
    let compatible = run_pam(&[
        "catalog",
        "--compat",
        saved.to_str().unwrap(),
        additive_path.to_str().unwrap(),
    ]);
    assert!(compatible.status.success());

    let mut mismatched: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    mismatched["commands"][0]["groupLabel"] = serde_json::json!("Advanced");
    let mismatched_path = directory.join("mismatched.json");
    fs::write(&mismatched_path, serde_json::to_vec(&mismatched).unwrap()).unwrap();
    let rejected = run_pam(&["catalog", "--validate", mismatched_path.to_str().unwrap()]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("does not match groupCode"));

    let mut duplicate: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let first = duplicate["commands"][0].clone();
    duplicate["commands"].as_array_mut().unwrap().push(first);
    let duplicate_path = directory.join("duplicate.json");
    fs::write(&duplicate_path, serde_json::to_vec(&duplicate).unwrap()).unwrap();
    let rejected = run_pam(&["catalog", "--validate", duplicate_path.to_str().unwrap()]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("duplicate CLI command"));

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&saved, directory.join("catalog-link.json")).unwrap();
        let rejected = run_pam(&[
            "catalog",
            "--validate",
            directory.join("catalog-link.json").to_str().unwrap(),
        ]);
        assert!(!rejected.status.success());
        assert!(String::from_utf8_lossy(&rejected.stderr).contains("non-symlink"));
    }

    let invalid = run_pam(&["catalog"]);
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("catalog requires"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn automatically_loads_composer_with_a_custom_vendor_directory() {
    let output = run_pam(&[fixture("composer-app/index.php").to_str().unwrap()]);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Composer autoload works\n"
    );
}

#[test]
fn runs_fibers_and_isolated_process_tasks() {
    let output = run_pam(&[fixture("async.php").to_str().unwrap()]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["concurrent"], true);
    assert_eq!(payload["cancelled"], true);
    assert!(payload["boundedDuration"].as_f64().unwrap() >= 0.18);
    assert_eq!(
        payload["boundedProcesses"],
        serde_json::json!(["one", "two"])
    );
    assert_eq!(payload["context"], "fiber-context");
    assert_eq!(payload["deadlineExpired"], true);
    assert!(
        payload["dns"]
            .as_array()
            .is_some_and(|values| !values.is_empty())
    );
    assert_eq!(payload["process"], "ISOLATED");
    assert_eq!(payload["processes"], serde_json::json!(["one", "two"]));
    assert_eq!(payload["processesConcurrent"], true);
    assert_eq!(payload["stdoutBytes"], 1024);
    assert_eq!(payload["stdoutTruncated"], true);
    assert_eq!(
        payload["signalHandlerRegistered"], payload["signalSupported"],
        "{payload}"
    );
    assert_eq!(payload["signalStateRestored"], true, "{payload}");
    assert_eq!(
        payload["stream"],
        serde_json::json!(["stream-ready", "written"])
    );
    assert_eq!(payload["successful"], true);
    assert_eq!(payload["timedOut"], 2);
    assert_eq!(payload["values"], serde_json::json!(["first", "second"]));
}

#[test]
fn handles_bounded_adapter_queues_and_fragmented_nats_frames() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let mock = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream.write_all(b"INFO {}\r\n").unwrap();
        let mut input = Vec::new();
        let payload = loop {
            let mut chunk = [0_u8; 1024];
            let read = stream.read(&mut chunk).unwrap();
            assert!(read > 0, "NATS client disconnected before publish");
            input.extend_from_slice(&chunk[..read]);
            let text = String::from_utf8_lossy(&input);
            let Some(header_start) = text.find("PUB pam.ws ") else {
                continue;
            };
            let Some(relative_end) = text[header_start..].find("\r\n") else {
                continue;
            };
            let header_end = header_start + relative_end;
            let length = text[header_start..header_end]
                .split_whitespace()
                .last()
                .unwrap()
                .parse::<usize>()
                .unwrap();
            let payload_start = header_end + 2;
            if input.len() >= payload_start + length + 2 {
                break input[payload_start..payload_start + length].to_vec();
            }
        };
        let frame = format!("MSG pam.ws 1 {}\r\n", payload.len());
        for part in [frame.as_bytes(), &payload[..3], &payload[3..], b"\r\n"] {
            stream.write_all(part).unwrap();
            thread::sleep(Duration::from_millis(2));
        }
    });

    let output = run_pam(&[fixture("adapters.php").to_str().unwrap(), &port.to_string()]);
    mock.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["memory"][0]["payload"], "second");
    assert_eq!(payload["memory"][1]["payload"], "third");
    assert_eq!(payload["nats"][0]["channel"], "broadcast");
    assert_eq!(payload["nats"][0]["payload"], r#"{"value":"fragmented"}"#);
}

#[test]
fn exposes_inspect_routes_exec_help_and_version_commands() {
    let inspect = run_pam(&["inspect", fixture("hello.php").to_str().unwrap()]);
    assert!(inspect.status.success());
    let information: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(information["sapi"], "embed");
    assert!(information["phpVersionId"].as_u64().unwrap() >= 80400);
    assert_eq!(information["nativeAbiVersion"], 1);

    let routes = Command::new(env!("CARGO_BIN_EXE_pam"))
        .args(["routes", fixture("server.php").to_str().unwrap()])
        .env("PAM_TEST_PORT", "3000")
        .output()
        .unwrap();
    assert!(routes.status.success());
    let routes: serde_json::Value = serde_json::from_slice(&routes.stdout).unwrap();
    assert!(routes.as_array().is_some_and(|routes| {
        routes
            .iter()
            .any(|route| route["method"] == "GET" && route["path"] == "/ping" && route["kind"] == 4)
    }));

    let execute = run_pam(&["exec", fixture("hello.php").to_str().unwrap()]);
    assert!(execute.status.success());
    assert_eq!(
        String::from_utf8_lossy(&execute.stdout),
        "Hello from PHP!\n"
    );

    let help = run_pam(&["--help"]);
    assert!(help.status.success());
    let help_text = String::from_utf8_lossy(&help.stderr);
    assert!(help_text.contains("PHP ALWAYS IN MEMORY"));
    assert!(help_text.contains("benchmark"));
    assert!(
        !help_text.contains("\x1b["),
        "captured help must not contain ANSI escapes"
    );

    let start_help = run_pam(&["help", "start"]);
    assert!(start_help.status.success());
    let start_help = String::from_utf8_lossy(&start_help.stderr);
    assert!(start_help.contains("PAM / START"));
    assert!(start_help.contains("--admin-address IP:PORT"));
    assert!(start_help.contains("$ pam start index.php --workers 4"));

    let octane_help = run_pam(&["help", "octane:start"]);
    assert!(octane_help.status.success());
    let octane_help = String::from_utf8_lossy(&octane_help.stderr);
    assert!(octane_help.contains("PAM / OCTANE:START"));
    assert!(octane_help.contains("--host ADDRESS"));

    let init_help = run_pam(&["init", "--help"]);
    assert!(init_help.status.success());
    let init_help = String::from_utf8_lossy(&init_help.stderr);
    assert!(init_help.contains("mobile-ui, or product"));
    assert!(init_help.contains("--template mobile-ui"));

    let mobile_help = run_pam(&["mobile", "--help"]);
    assert!(mobile_help.status.success());
    let mobile_help = String::from_utf8_lossy(&mobile_help.stderr);
    assert!(mobile_help.contains("PAM / MOBILE"));
    assert!(mobile_help.contains("make:screen"));

    let registry_help = run_pam(&["registry", "--help"]);
    assert!(registry_help.status.success());
    let registry_help = String::from_utf8_lossy(&registry_help.stdout);
    assert!(registry_help.contains("registry verify"));
    assert!(registry_help.contains("registry resolve"));
    assert!(registry_help.contains("registry rotate"));
    assert!(registry_help.contains("registry payload"));
    assert!(registry_help.contains("registry key-id"));

    let timeline_help = run_pam(&["help", "timeline"]);
    assert!(timeline_help.status.success());
    assert!(String::from_utf8_lossy(&timeline_help.stderr).contains("Chrome Trace Event JSON"));

    let version = run_pam(&["--version"]);
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("pam "));
}

#[test]
fn exports_a_bounded_redacted_cross_surface_timeline() {
    let directory = temporary_path("timeline-export");
    fs::create_dir(&directory).unwrap();
    let snapshot = directory.join("native-snapshot.json");
    let output = directory.join("native-trace.json");
    fs::write(
        &snapshot,
        r#"{"schemaVersion":1,"surfaceCode":2,"capturedAtUnixMs":1234,"timeline":[{"kindCode":5,"durationMicros":42,"failed":false,"methodCode":1,"statusCode":204,"requestBytes":0,"responseBytes":17,"label":"private-label","url":"https://secret.example/private"}]}"#,
    )
    .unwrap();
    let first = Command::new(env!("CARGO_BIN_EXE_pam"))
        .args(["timeline", snapshot.to_str().unwrap(), "--output"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let trace = fs::read_to_string(&output).unwrap();
    assert!(trace.contains("\"traceEvents\""));
    assert!(trace.contains("native.network"));
    assert!(trace.contains("\"method_code\": 1"));
    assert!(trace.contains("\"status_code\": 204"));
    assert!(trace.contains("\"response_bytes\": 17"));
    assert!(!trace.contains("private-label"));
    assert!(!trace.contains("secret.example"));

    let second = Command::new(env!("CARGO_BIN_EXE_pam"))
        .args(["timeline", snapshot.to_str().unwrap(), "--output"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        !second.status.success(),
        "timeline evidence must not be overwritten"
    );
    fs::remove_dir_all(directory).unwrap();
}

#[cfg(unix)]
#[test]
fn delegates_desktop_commands_and_exposes_the_pam_binary() {
    use std::os::unix::fs::PermissionsExt;

    let directory = temporary_path("desktop-delegation");
    let desktop = directory.join("pam-desktop");
    fs::create_dir(&directory).unwrap();
    fs::write(
        &desktop,
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then printf 'pam-desktop 9.9.9\\n'; exit 0; fi\nprintf 'pam=%s\\n' \"$PAM_BINARY\"\nprintf 'args=%s|%s|%s\\n' \"$1\" \"$2\" \"$3\"\nexit 23\n",
    )
    .unwrap();
    fs::set_permissions(&desktop, fs::Permissions::from_mode(0o755)).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_pam"))
        .args(["desktop", "dev", ".", "--watch"])
        .env("PAM_DESKTOP_BINARY", &desktop)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(23));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("pam="), "{stdout}");
    assert!(stdout.contains("args=dev|.|--watch"), "{stdout}");
    assert!(
        stdout.contains(env!("CARGO_BIN_EXE_pam")),
        "PAM_BINARY was not propagated: {stdout}",
    );

    fs::write(
        directory.join("pam.json"),
        r#"{"schema":1,"type":4,"name":"desktop","version":"0.1.0"}"#,
    )
    .unwrap();
    fs::write(
        directory.join("composer.json"),
        r#"{"name":"app/desktop","require":{"pam/desktop":"^0.5"}}"#,
    )
    .unwrap();
    let diagnostics = Command::new(env!("CARGO_BIN_EXE_pam"))
        .arg("diagnostics")
        .current_dir(&directory)
        .env("PAM_DESKTOP_BINARY", &desktop)
        .output()
        .unwrap();
    assert_eq!(diagnostics.status.code(), Some(23));
    let diagnostics_output = String::from_utf8_lossy(&diagnostics.stdout);
    assert!(
        diagnostics_output.contains(&format!("args=diagnostics|{}|", directory.display())),
        "{diagnostics_output}"
    );

    let package = Command::new(env!("CARGO_BIN_EXE_pam"))
        .arg("package")
        .current_dir(&directory)
        .env("PAM_DESKTOP_BINARY", &desktop)
        .output()
        .unwrap();
    assert_eq!(package.status.code(), Some(23));
    let package_output = String::from_utf8_lossy(&package.stdout);
    assert!(package_output.contains("args=build|"), "{package_output}");

    let host_doctor = Command::new(env!("CARGO_BIN_EXE_pam"))
        .args(["desktop", "host:doctor", ".", "--json"])
        .current_dir(&directory)
        .env("PAM_DESKTOP_BINARY", &desktop)
        .output()
        .unwrap();
    assert_eq!(host_doctor.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&host_doctor.stdout).unwrap();
    assert_eq!(report["schemaVersion"], 1);
    assert_eq!(report["surfaceCode"], 3);
    assert_eq!(report["resultCode"], 2);
    assert_eq!(report["sourceCode"], 2);
    assert_eq!(report["authenticated"], false);
    assert_eq!(report["checks"][0]["checkCode"], 1);
    assert_eq!(report["checks"][0]["resultCode"], 2);
    assert_eq!(report["checks"][1]["checkCode"], 2);
    assert_eq!(report["checks"][1]["resultCode"], 2);
    assert_eq!(report["checks"][2]["checkCode"], 3);
    assert_eq!(report["checks"][2]["resultCode"], 1);

    let tests = run_pam_in(&directory, &["test"]);
    assert!(tests.status.success());
    assert!(String::from_utf8_lossy(&tests.stdout).contains("No application test runner"));

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn rejects_a_desktop_capture_command_missing_from_protocol_six() {
    let output = Command::new(env!("CARGO_BIN_EXE_pam"))
        .args(["desktop", "screenshot", "."])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("protocol 6"), "{stderr}");
    assert!(stderr.contains("platform driver"), "{stderr}");
    assert!(stderr.contains("desktop visual verify"), "{stderr}");
}

#[test]
fn rejects_unbounded_top_lag_warning_thresholds_before_connecting() {
    for value in ["0", "60001", "invalid"] {
        let output = run_pam(&["top", "http://127.0.0.1:9", "--lag-warn-ms", value]);
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("lag-warn-ms"), "{stderr}");
    }
}

#[test]
fn accepts_top_json_before_or_after_the_admin_url() {
    for arguments in [
        ["top", "--json", "--iterations", "0", "http://127.0.0.1:9"],
        ["top", "http://127.0.0.1:9", "--json", "--iterations", "0"],
    ] {
        let output = run_pam(&arguments);
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("must be positive"), "{stderr}");
        assert!(!stderr.contains("unknown top option"), "{stderr}");
    }
}

#[test]
fn rejects_unauthenticated_public_control_planes_before_startup() {
    let output = Command::new(env!("CARGO_BIN_EXE_pam"))
        .args([
            "start",
            fixture("hello.php").to_str().unwrap(),
            "--admin-address",
            "0.0.0.0:3010",
        ])
        .env_remove("PAM_ADMIN_TOKEN")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("non-loopback"), "{stderr}");
    assert!(stderr.contains("PAM_ADMIN_TOKEN"), "{stderr}");
}

#[test]
fn rejects_weak_control_plane_tokens_before_startup() {
    let output = Command::new(env!("CARGO_BIN_EXE_pam"))
        .args([
            "start",
            fixture("hello.php").to_str().unwrap(),
            "--admin-address",
            "127.0.0.1:3010",
        ])
        .env("PAM_ADMIN_TOKEN", "too-short")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("32 to 256"), "{stderr}");
}

#[test]
fn rejects_ambiguous_control_plane_token_sources() {
    let path = temporary_path("admin-token-source");
    fs::write(&path, "0123456789abcdef0123456789abcdef\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_pam"))
        .args([
            "start",
            fixture("hello.php").to_str().unwrap(),
            "--admin-address",
            "127.0.0.1:3010",
        ])
        .env("PAM_ADMIN_TOKEN", "0123456789abcdef0123456789abcdef")
        .env("PAM_ADMIN_TOKEN_FILE", &path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("set only one"), "{stderr}");
    fs::remove_file(path).unwrap();
}

#[test]
fn doctor_uses_embed_without_requiring_a_php_cli() {
    let output = Command::new(env!("CARGO_BIN_EXE_pam"))
        .args(["doctor", fixture("hello.php").to_str().unwrap()])
        .env("PATH", "/pam-doctor-no-system-tools")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(report.contains("[ok] PHP Embed version:"), "{report}");
    assert!(
        report.contains("PHP CLI comparison unavailable")
            && report.contains("Pam uses PHP Embed directly"),
        "{report}"
    );
}

#[test]
fn exposes_structured_doctor_and_offline_update_checks() {
    let doctor = run_pam(&["doctor", fixture("hello.php").to_str().unwrap(), "--json"]);
    assert!(doctor.status.success());
    let report: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
    assert_eq!(report["schema"], 1);
    assert_eq!(report["schemaVersion"], 1);
    assert_eq!(report["resultCode"], 1);
    assert_eq!(report["healthy"], true);
    assert!(report["target"].as_str().is_some());
    assert_eq!(report["nextActions"][0]["actionCode"], 1);
    assert_eq!(report["nextActions"][0]["arguments"][0], report["target"]);
    assert!(
        report["nextActions"][0]["command"]
            .as_str()
            .unwrap()
            .contains("hello.php")
    );
    assert!(
        report["diagnostics"]
            .as_str()
            .unwrap()
            .contains("PHP Embed version")
    );

    let update = run_pam(&[
        "self-update",
        concat!("v", env!("CARGO_PKG_VERSION")),
        "--check",
    ]);
    assert!(update.status.success());
    assert!(String::from_utf8_lossy(&update.stdout).contains("is up to date"));
}

#[test]
fn exposes_the_exact_versioned_doctor_schema_offline() {
    let output = run_pam(&["doctor", "--schema"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let schema: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["properties"]["schemaVersion"]["const"], 1);
    assert_eq!(
        schema["properties"]["resultCode"]["enum"],
        serde_json::json!([1, 2])
    );
    assert_eq!(
        schema["$defs"]["action"]["properties"]["actionCode"]["enum"],
        serde_json::json!([1, 2, 3])
    );
    assert_eq!(schema["additionalProperties"], false);

    for arguments in [
        vec!["doctor", "--schema", "--json"],
        vec!["doctor", "--schema", "--ci"],
        vec!["doctor", "--schema", "."],
    ] {
        let rejected = run_pam(&arguments);
        assert!(!rejected.status.success());
        assert!(
            String::from_utf8_lossy(&rejected.stderr)
                .contains("doctor --schema must be used alone")
        );
    }
}

#[test]
fn verifies_signed_clean_host_distribution_evidence_offline() {
    let directory = temporary_path("distribution-evidence");
    fs::create_dir_all(directory.join("files")).unwrap();
    let artifact = directory.join("files/pam.tar.zst");
    let baseline_artifact = directory.join("files/pam-baseline.tar.zst");
    let inventory = directory.join("files/sbom.spdx.json");
    let provenance_inventory = directory.join("files/provenance.sha256");
    let provenance_bundle = directory.join("attestations/bundle.json");
    fs::write(&artifact, b"immutable-package").unwrap();
    fs::write(&baseline_artifact, b"immutable-baseline").unwrap();
    fs::write(&inventory, br#"{"spdxVersion":"SPDX-2.3"}"#).unwrap();
    fs::create_dir_all(directory.join("attestations")).unwrap();
    fs::write(&provenance_bundle, b"signed attestation fixture").unwrap();
    fs::write(
        &provenance_inventory,
        format!(
            "{:x}  attestations/bundle.json\n",
            Sha256::digest(fs::read(&provenance_bundle).unwrap())
        ),
    )
    .unwrap();
    let artifact_bytes = fs::read(&artifact).unwrap();
    let baseline_artifact_bytes = fs::read(&baseline_artifact).unwrap();
    let inventory_bytes = fs::read(&inventory).unwrap();
    let provenance_inventory_bytes = fs::read(&provenance_inventory).unwrap();
    let checks = (1..=7)
        .map(|check_code| {
            serde_json::json!({
                "checkCode": check_code,
                "resultCode": 1,
                "durationMillis": check_code * 10,
            })
        })
        .collect::<Vec<_>>();
    let signing_key = SigningKey::from_bytes(&[17_u8; 32]);
    let public_key = signing_key.verifying_key().to_bytes();
    let sign_manifest = |manifest: &mut serde_json::Value| {
        manifest
            .as_object_mut()
            .unwrap()
            .remove("manifestSignature");
        let signature = signing_key.sign(&serde_json::to_vec(manifest).unwrap());
        manifest["manifestSignature"] = serde_json::json!(
            base64::engine::general_purpose::STANDARD.encode(signature.to_bytes())
        );
    };
    let mut manifest = serde_json::json!({
        "schemaVersion": 1,
        "surfaceCode": 1,
        "platformCode": 1,
        "architectureCode": 1,
        "packageCode": 1,
        "revision": "0123456789abcdef0123456789abcdef01234567",
        "baselineRevision": "89abcdef0123456789abcdef0123456789abcdef",
        "hostImage": "ubuntu-24.04@sha256:fixture",
        "generatedAtUnixMs": 1_800_000_000_000_u64,
        "artifact": {
            "path": "files/pam.tar.zst",
            "sha256": format!("{:x}", Sha256::digest(&artifact_bytes)),
            "bytes": artifact_bytes.len(),
        },
        "baselineArtifact": {
            "path": "files/pam-baseline.tar.zst",
            "sha256": format!("{:x}", Sha256::digest(&baseline_artifact_bytes)),
            "bytes": baseline_artifact_bytes.len(),
        },
        "dependencyInventory": {
            "path": "files/sbom.spdx.json",
            "sha256": format!("{:x}", Sha256::digest(&inventory_bytes)),
            "bytes": inventory_bytes.len(),
        },
        "provenanceInventory": {
            "path": "files/provenance.sha256",
            "sha256": format!("{:x}", Sha256::digest(&provenance_inventory_bytes)),
            "bytes": provenance_inventory_bytes.len(),
        },
        "installedBytes": 4096,
        "launchMillis": 30,
        "firstSuccessMillis": 45,
        "signingIdentitySha256": format!("{:x}", Sha256::digest(public_key)),
        "signingPublicKey": base64::engine::general_purpose::STANDARD.encode(public_key),
        "checks": checks,
    });
    sign_manifest(&mut manifest);
    let manifest_path = directory.join("distribution.json");
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let verified = run_pam(&[
        "distribution:verify",
        manifest_path.to_str().unwrap(),
        "--json",
    ]);
    assert!(
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&verified.stdout).unwrap();
    assert_eq!(result["schemaVersion"], 1);
    assert_eq!(result["resultCode"], 1);
    assert_eq!(result["surfaceCode"], 1);
    assert_eq!(result["platformCode"], 1);
    assert_eq!(result["packageCode"], 1);
    assert_eq!(
        result["signingIdentitySha256"],
        manifest["signingIdentitySha256"]
    );

    fs::write(&provenance_bundle, b"tampered attestation fixture").unwrap();
    let tampered_provenance = run_pam(&["distribution:verify", manifest_path.to_str().unwrap()]);
    assert!(!tampered_provenance.status.success());
    assert!(
        String::from_utf8_lossy(&tampered_provenance.stderr)
            .contains("provenance entry SHA-256 mismatch")
    );
    fs::write(&provenance_bundle, b"signed attestation fixture").unwrap();

    let mut draft = manifest.clone();
    for field in [
        "signingIdentitySha256",
        "signingPublicKey",
        "manifestSignature",
    ] {
        draft.as_object_mut().unwrap().remove(field);
    }
    let draft_path = directory.join("draft.json");
    let key_path = directory.join("evidence.key");
    let signed_path = directory.join("signed-by-pam.json");
    fs::write(&draft_path, serde_json::to_vec_pretty(&draft).unwrap()).unwrap();
    fs::write(
        &key_path,
        base64::engine::general_purpose::STANDARD.encode(signing_key.to_bytes()),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let signed = run_pam(&[
        "distribution:sign",
        draft_path.to_str().unwrap(),
        "--key",
        key_path.to_str().unwrap(),
        "--output",
        signed_path.to_str().unwrap(),
    ]);
    assert!(
        signed.status.success(),
        "{}",
        String::from_utf8_lossy(&signed.stderr)
    );
    let produced = fs::read(&signed_path).unwrap();
    assert!(
        !produced
            .windows(32)
            .any(|window| window == signing_key.to_bytes())
    );
    let produced_document: serde_json::Value = serde_json::from_slice(&produced).unwrap();
    assert_eq!(
        produced_document["signingIdentitySha256"],
        manifest["signingIdentitySha256"]
    );
    let produced_verification = run_pam(&["distribution:verify", signed_path.to_str().unwrap()]);
    assert!(produced_verification.status.success());
    let overwrite = run_pam(&[
        "distribution:sign",
        draft_path.to_str().unwrap(),
        "--key",
        key_path.to_str().unwrap(),
        "--output",
        signed_path.to_str().unwrap(),
    ]);
    assert!(!overwrite.status.success());
    assert!(String::from_utf8_lossy(&overwrite.stderr).contains("refusing to overwrite"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o644)).unwrap();
        let insecure = run_pam(&[
            "distribution:sign",
            draft_path.to_str().unwrap(),
            "--key",
            key_path.to_str().unwrap(),
            "--output",
            directory.join("insecure.json").to_str().unwrap(),
        ]);
        assert!(!insecure.status.success());
        assert!(String::from_utf8_lossy(&insecure.stderr).contains("permissions"));
    }

    let signed_manifest = manifest.clone();
    manifest["installedBytes"] = serde_json::json!(8192);
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let forged = run_pam(&["distribution:verify", manifest_path.to_str().unwrap()]);
    assert!(!forged.status.success());
    assert!(String::from_utf8_lossy(&forged.stderr).contains("manifestSignature did not verify"));
    manifest = signed_manifest;
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();

    fs::write(&artifact, b"tampered-package").unwrap();
    let tampered = run_pam(&["distribution:verify", manifest_path.to_str().unwrap()]);
    assert!(!tampered.status.success());
    assert!(String::from_utf8_lossy(&tampered.stderr).contains("byte size does not match"));
    fs::write(&artifact, &artifact_bytes).unwrap();

    manifest["artifact"]["path"] = serde_json::json!("../outside.tar.zst");
    sign_manifest(&mut manifest);
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let traversal = run_pam(&["distribution:verify", manifest_path.to_str().unwrap()]);
    assert!(!traversal.status.success());
    assert!(String::from_utf8_lossy(&traversal.stderr).contains("canonical relative path"));

    manifest["artifact"]["path"] = serde_json::json!("files/pam.tar.zst");
    manifest["checks"][6]["resultCode"] = serde_json::json!(2);
    sign_manifest(&mut manifest);
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let failed_gate = run_pam(&["distribution:verify", manifest_path.to_str().unwrap()]);
    assert!(!failed_gate.status.success());
    assert!(String::from_utf8_lossy(&failed_gate.stderr).contains("did not pass"));

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn validates_doctor_reports_offline_and_rejects_tampering() {
    let directory = temporary_path("doctor-contract");
    fs::create_dir_all(&directory).unwrap();
    let report_path = directory.join("doctor.json");
    let doctor = run_pam(&["doctor", fixture("hello.php").to_str().unwrap(), "--json"]);
    assert!(doctor.status.success());
    fs::write(&report_path, &doctor.stdout).unwrap();

    let valid = run_pam(&["doctor", "--validate", report_path.to_str().unwrap()]);
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );

    let mut report: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
    report["unexpected"] = serde_json::json!(true);
    fs::write(&report_path, serde_json::to_vec(&report).unwrap()).unwrap();
    let unknown = run_pam(&["doctor", "--validate", report_path.to_str().unwrap()]);
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("fields do not match"));

    report.as_object_mut().unwrap().remove("unexpected");
    report["healthy"] = serde_json::json!(false);
    fs::write(&report_path, serde_json::to_vec(&report).unwrap()).unwrap();
    let inconsistent = run_pam(&["doctor", "--validate", report_path.to_str().unwrap()]);
    assert!(!inconsistent.status.success());
    assert!(String::from_utf8_lossy(&inconsistent.stderr).contains("inconsistent"));

    fs::write(&report_path, vec![b' '; 1024 * 1024 + 1]).unwrap();
    let oversized = run_pam(&["doctor", "--validate", report_path.to_str().unwrap()]);
    assert!(!oversized.status.success());
    assert!(String::from_utf8_lossy(&oversized.stderr).contains("1048576-byte"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let real = directory.join("real.json");
        let link = directory.join("linked.json");
        fs::write(&real, &doctor.stdout).unwrap();
        symlink(&real, &link).unwrap();
        let linked = run_pam(&["doctor", "--validate", link.to_str().unwrap()]);
        assert!(!linked.status.success());
        assert!(String::from_utf8_lossy(&linked.stderr).contains("non-symlink"));
    }

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn creates_a_bounded_redacted_support_report_without_persisting_by_default() {
    let target = fixture("hello.php");
    let support = run_pam(&["support", target.to_str().unwrap()]);
    assert!(
        support.status.success(),
        "{}",
        String::from_utf8_lossy(&support.stderr)
    );
    assert!(support.stdout.len() < 256 * 1024);
    let report: serde_json::Value = serde_json::from_slice(&support.stdout).unwrap();
    assert_eq!(report["schemaVersion"], 1);
    assert_eq!(report["resultCode"], 1);
    assert_eq!(report["surfaceCode"], 1);
    assert_eq!(report["privacy"]["redactionCode"], 1);
    assert_eq!(report["privacy"]["includesEnvironment"], false);
    assert_eq!(report["privacy"]["includesFileContents"], false);
    assert_eq!(report["privacy"]["includesNetworkData"], false);
    assert_eq!(report["privacy"]["includesProcessMetadata"], false);
    assert_eq!(report["privacy"]["includesLogContents"], false);
    assert_eq!(report["diagnostics"]["target"], "$PROJECT");
    assert!(!String::from_utf8_lossy(&support.stdout).contains(target.to_str().unwrap()));

    let diagnostics = serde_json::to_vec(&report["diagnostics"]).unwrap();
    assert_eq!(
        report["diagnosticsSha256"],
        format!("{:x}", Sha256::digest(diagnostics))
    );

    let manager_root = temporary_path("support-manager");
    let manager_state = manager_root.join("state");
    let manager_runtime = manager_root.join("runtime");
    let manager_support = Command::new(env!("CARGO_BIN_EXE_pam"))
        .env("PAM_MANAGER_STATE_DIR", &manager_state)
        .env("PAM_MANAGER_RUNTIME_DIR", &manager_runtime)
        .args(["support", target.to_str().unwrap(), "--manager"])
        .output()
        .unwrap();
    assert!(manager_support.status.success());
    let manager_report: serde_json::Value =
        serde_json::from_slice(&manager_support.stdout).unwrap();
    assert_eq!(manager_report["privacy"]["includesProcessMetadata"], true);
    assert_eq!(manager_report["privacy"]["includesLogContents"], false);
    assert_eq!(manager_report["manager"]["schemaVersion"], 1);
    assert_eq!(
        manager_report["manager"]["applications"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    let manager = serde_json::to_vec(&manager_report["manager"]).unwrap();
    assert_eq!(
        manager_report["managerSha256"],
        format!("{:x}", Sha256::digest(manager))
    );
    assert!(
        run_manager_daemon(&manager_state, &manager_runtime, "stop")
            .status
            .success()
    );
    fs::remove_dir_all(manager_root).unwrap();
}

#[test]
fn support_report_writes_once_and_refuses_to_overwrite() {
    let directory = temporary_path("support-output");
    fs::create_dir_all(&directory).unwrap();
    let output = directory.join("report.json");
    let target = fixture("hello.php");
    let first = run_pam(&[
        "support",
        target.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
    ]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(output.is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&output).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let original = fs::read(&output).unwrap();

    let second = run_pam(&[
        "support",
        target.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
    ]);
    assert!(!second.status.success());
    assert!(String::from_utf8_lossy(&second.stderr).contains("cannot create new support report"));
    assert_eq!(fs::read(&output).unwrap(), original);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn doctor_reports_project_target_paths_and_reclaimable_artifacts() {
    let directory = temporary_path("doctor-project-context");
    fs::create_dir_all(directory.join("target/debug")).unwrap();
    fs::write(
        directory.join("pam.json"),
        r#"{"schema":1,"type":5,"name":"doctor-fixture","version":"0.1.0"}"#,
    )
    .unwrap();
    fs::write(directory.join("target/debug/cache.bin"), [0_u8; 64]).unwrap();

    let doctor = run_pam_in(&directory, &["doctor", "--json"]);
    assert!(doctor.status.success());
    let report: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
    assert_eq!(report["project"]["typeCode"], 5);
    assert_eq!(report["project"]["typeLabel"], "PAM Runtime");
    assert_eq!(report["project"]["developmentArtifacts"]["bytes"], 64);
    assert_eq!(report["project"]["developmentArtifacts"]["files"], 1);
    assert!(
        report["project"]["developmentArtifacts"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["path"] == "target")
    );
    assert!(
        report["project"]["paths"]["manifest"]
            .as_str()
            .unwrap()
            .ends_with("pam.json")
    );
    assert_eq!(
        report["nextActions"][0]["arguments"],
        serde_json::json!(["dev"])
    );
    assert_eq!(
        report["nextActions"][0]["verificationCommand"],
        "pam doctor --json"
    );

    let report_path = directory.join("doctor-report.json");
    fs::write(&report_path, &doctor.stdout).unwrap();
    let validated = run_pam(&["doctor", "--validate", report_path.to_str().unwrap()]);
    assert!(
        validated.status.success(),
        "{}",
        String::from_utf8_lossy(&validated.stderr)
    );

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn info_reports_runtime_target_artifacts_without_changing_the_legacy_native_field() {
    let directory = temporary_path("info-runtime-artifacts");
    fs::create_dir_all(directory.join("target/debug")).unwrap();
    fs::write(
        directory.join("pam.json"),
        r#"{"schema":1,"type":5,"name":"runtime-fixture","version":"0.1.0"}"#,
    )
    .unwrap();
    fs::write(directory.join("target/debug/cache.bin"), [0_u8; 64]).unwrap();

    let info = run_pam_in(&directory, &["info", "--json"]);
    assert!(info.status.success());
    let report: serde_json::Value = serde_json::from_slice(&info.stdout).unwrap();
    assert_eq!(report["type"], 5);
    assert_eq!(report["developmentArtifacts"]["exists"], false);
    assert_eq!(report["developmentArtifacts"]["bytes"], 0);
    assert_eq!(report["artifactFootprint"]["bytes"], 64);
    assert_eq!(report["artifactFootprint"]["files"], 1);
    assert_eq!(report["artifactFootprint"]["complete"], true);
    assert!(
        report["artifactFootprint"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["path"] == "target")
    );

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn initializes_a_project_without_overwriting_files() {
    let directory = temporary_path("init-test");
    let output = run_pam(&[
        "init",
        directory.to_str().unwrap(),
        "--template",
        "api",
        "--no-install",
        "--no-interaction",
    ]);
    assert!(output.status.success());
    assert!(directory.join("composer.json").is_file());
    assert!(directory.join("index.php").is_file());
    assert!(directory.join(".gitignore").is_file());
    assert!(directory.join(".env.example").is_file());
    assert!(directory.join("phpunit.xml").is_file());
    assert!(directory.join("tests/ApplicationTest.php").is_file());
    let manifest = fs::read_to_string(directory.join("composer.json")).unwrap();
    assert!(manifest.contains("\"pushinbr/pam-api\""));
    let manifest_json: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    assert_eq!(
        manifest_json["description"],
        "A PHP application powered by the PAM runtime."
    );
    assert_eq!(manifest_json["license"], "proprietary");
    assert_eq!(manifest_json["require"]["pushinbr/pam-api"], "^1.0");
    assert_eq!(manifest_json["require-dev"]["laravel/pint"], "^1.30");
    assert_eq!(manifest_json["require-dev"]["pushinbr/pam-testing"], "^1.0");

    let repeated = run_pam(&["init", directory.to_str().unwrap()]);
    assert!(!repeated.status.success());
    assert!(String::from_utf8_lossy(&repeated.stderr).contains("is not empty"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn initializes_raw_and_socket_presets_without_composer() {
    let raw = temporary_path("init-raw");
    let output = run_pam(&[
        "init",
        raw.to_str().unwrap(),
        "--template",
        "raw",
        "--socket",
        "--no-install",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!raw.join("composer.json").exists());
    assert!(
        fs::read_to_string(raw.join("index.php"))
            .unwrap()
            .contains("Pam\\WS\\Server")
    );

    let api = temporary_path("init-api-socket");
    let output = run_pam(&[
        "init",
        api.to_str().unwrap(),
        "--template",
        "api",
        "--socket",
        "--no-install",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest = fs::read_to_string(api.join("composer.json")).unwrap();
    assert!(manifest.contains("pushinbr/pam-socket"));
    let manifest_json: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    assert_eq!(manifest_json["require"]["pushinbr/pam-api"], "^1.0");
    assert_eq!(manifest_json["require"]["pushinbr/pam-socket"], "^1.0");

    fs::remove_dir_all(raw).unwrap();
    fs::remove_dir_all(api).unwrap();
}

#[test]
fn initializes_mobile_with_tree_default_and_pam_components_enabled() {
    let directory = temporary_path("init-mobile");
    let output = run_pam(&[
        "init",
        directory.to_str().unwrap(),
        "--template",
        "mobile",
        "--no-install",
        "--no-interaction",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let manifest = fs::read_to_string(directory.join("composer.json")).unwrap();
    let manifest_json: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    let entry = fs::read_to_string(directory.join("index.php")).unwrap();
    let hello = fs::read_to_string(directory.join("src/Hello.php")).unwrap();
    assert!(
        manifest_json["require"]["pushinbr/pam-native"] == "^0.6"
            || manifest_json["require"]["pam/native"] == "^0.6"
    );
    assert!(!manifest.contains("pushinbr/pam-mobile-ui"));
    assert!(entry.contains("App::components(__DIR__.'/src'"));
    assert!(entry.contains("App::run(new Hello())"));
    assert!(hello.contains("public function render(): Element"));
    assert!(hello.contains("Screen::make("));
    assert!(
        !directory
            .join("resources/native/screens/hello.pam")
            .exists()
    );

    fs::remove_dir_all(directory).unwrap();
}

#[cfg(unix)]
#[test]
fn captures_contextual_redacted_android_diagnostics() {
    use std::os::unix::fs::PermissionsExt;

    let directory = temporary_path("native-diagnostics");
    let output = run_pam(&[
        "init",
        directory.to_str().unwrap(),
        "--template",
        "mobile",
        "--no-install",
        "--no-interaction",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tools = directory.join("test-tools");
    fs::create_dir(&tools).unwrap();
    let adb = tools.join("adb");
    fs::write(
        &adb,
        r#"#!/bin/sh
case "$*" in
  "shell pidof "*) printf '42\n' ;;
  *" cat cache/pam-diagnostics-"*) printf '%s' '{"schemaVersion":1,"surfaceCode":2,"capturedAtUnixMs":1234,"platformCode":1,"timeline":[{"kindCode":3,"durationMicros":8,"failed":true}]}' ;;
esac
exit 0
"#,
    )
    .unwrap();
    fs::set_permissions(&adb, fs::Permissions::from_mode(0o755)).unwrap();

    let diagnostics = Command::new(env!("CARGO_BIN_EXE_pam"))
        .arg("diagnostics")
        .current_dir(&directory)
        .env("PATH", &tools)
        .output()
        .unwrap();
    assert!(
        diagnostics.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&diagnostics.stdout),
        String::from_utf8_lossy(&diagnostics.stderr),
    );
    let snapshot: serde_json::Value = serde_json::from_slice(&diagnostics.stdout).unwrap();
    assert_eq!(snapshot["schemaVersion"], 1);
    assert_eq!(snapshot["surfaceCode"], 2);
    assert_eq!(snapshot["timeline"][0]["kindCode"], 3);

    fs::remove_dir_all(directory).unwrap();
}

#[cfg(unix)]
#[test]
fn captures_redacted_ios_simulator_diagnostics() {
    use std::os::unix::fs::PermissionsExt;

    let directory = temporary_path("ios-native-diagnostics");
    let output = run_pam(&[
        "init",
        directory.to_str().unwrap(),
        "--template",
        "mobile",
        "--platform",
        "ios",
        "--no-install",
        "--no-interaction",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tools = directory.join("test-tools");
    let container = directory.join("simulator-container");
    fs::create_dir_all(&tools).unwrap();
    fs::create_dir_all(&container).unwrap();
    let xcrun = tools.join("xcrun");
    fs::write(
        &xcrun,
        format!(
            r#"#!/bin/sh
case "$*" in
  "simctl list devices booted --json") printf '%s' '{{"devices":{{"runtime":[{{"udid":"SIM-1"}}]}}}}' ;;
  "simctl openurl "*)
    for last do :; done
    case "$last" in
      *://devtools) ;;
      *)
        request="${{last##*/}}"
        /bin/mkdir -p '{container}/Library/Caches'
        printf '%s' '{{"schemaVersion":1,"surfaceCode":2,"capturedAtUnixMs":1234,"platformCode":2,"timeline":[{{"kindCode":4,"durationMicros":0,"failed":false}}]}}' > '{container}/Library/Caches/pam-diagnostics-'"$request"'.json'
        ;;
    esac
    ;;
  "simctl get_app_container "*) printf '%s\n' '{container}' ;;
esac
exit 0
"#,
            container = container.display(),
        ),
    )
    .unwrap();
    fs::set_permissions(&xcrun, fs::Permissions::from_mode(0o755)).unwrap();

    let diagnostics = Command::new(env!("CARGO_BIN_EXE_pam"))
        .arg("diagnostics")
        .current_dir(&directory)
        .env("PATH", &tools)
        .output()
        .unwrap();
    assert!(
        diagnostics.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&diagnostics.stdout),
        String::from_utf8_lossy(&diagnostics.stderr),
    );
    let snapshot: serde_json::Value = serde_json::from_slice(&diagnostics.stdout).unwrap();
    assert_eq!(snapshot["schemaVersion"], 1);
    assert_eq!(snapshot["surfaceCode"], 2);
    assert_eq!(snapshot["platformCode"], 2);
    assert!(
        fs::read_dir(container.join("Library/Caches"))
            .unwrap()
            .next()
            .is_none(),
        "the simulator snapshot must be removed after capture"
    );

    let devtools = Command::new(env!("CARGO_BIN_EXE_pam"))
        .arg("devtools")
        .current_dir(&directory)
        .env("PATH", &tools)
        .output()
        .unwrap();
    assert!(
        devtools.status.success(),
        "{}",
        String::from_utf8_lossy(&devtools.stderr)
    );
    assert!(String::from_utf8_lossy(&devtools.stdout).contains("Toggled Pam Native DevTools"));

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn initializes_mobile_with_the_official_ui_and_single_file_components() {
    let directory = temporary_path("init-mobile-ui");
    let output = run_pam(&[
        "init",
        directory.to_str().unwrap(),
        "--template",
        "mobile-ui",
        "--no-install",
        "--no-interaction",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let manifest = fs::read_to_string(directory.join("composer.json")).unwrap();
    let manifest_json: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    let entry = fs::read_to_string(directory.join("index.php")).unwrap();
    let hello = fs::read_to_string(directory.join("src/Hello.pam")).unwrap();
    assert!(
        manifest_json["require"]["pushinbr/pam-native"] == "^0.6"
            || (manifest_json["require"]["pam/native"] == "^0.6"
                && manifest_json["replace"]["pushinbr/pam-native"] == env!("CARGO_PKG_VERSION"))
    );
    assert!(manifest.contains("\"pushinbr/pam-mobile-ui\": \"^0.4\""));
    assert!(entry.contains("PamUI::mode(ThemeMode::System)"));
    assert!(entry.contains("App::run(App::make(Hello::class))"));
    assert!(hello.contains("#[State]"));
    assert!(hello.contains("<PamUIProvider mode=\"system\">"));
    assert!(hello.contains("<Button size=\"lg\" on:press=\"increment\">"));

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn initializes_a_bounded_cross_surface_product_workspace() {
    let directory = temporary_path("init-product");
    let output = run_pam(&[
        "init",
        directory.to_str().unwrap(),
        "--template",
        "product",
        "--no-install",
        "--no-interaction",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let root: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("pam.json")).unwrap()).unwrap();
    assert_eq!(root["type"], 6);
    assert_eq!(
        root["workspace"]["surfaceCodes"],
        serde_json::json!([1, 2, 3])
    );
    assert_eq!(root["workspace"]["contractPath"], "packages/contracts");
    assert_eq!(
        root["workspace"]["designTokenPath"],
        "packages/contracts/design-tokens.json"
    );
    assert!(
        fs::read_to_string(directory.join(".gitignore"))
            .unwrap()
            .contains("/dist/")
    );
    let info = run_pam_in(&directory, &["info", "--json"]);
    assert!(info.status.success());
    let info: serde_json::Value = serde_json::from_slice(&info.stdout).unwrap();
    assert_eq!(info["type"], 6);
    assert_eq!(info["typeLabel"], "PAM Product");

    let surface =
        fs::read_to_string(directory.join("packages/contracts/src/ProductSurface.php")).unwrap();
    let state =
        fs::read_to_string(directory.join("packages/contracts/src/ReadinessState.php")).unwrap();
    let version =
        fs::read_to_string(directory.join("packages/contracts/src/ContractVersion.php")).unwrap();
    let mutation_kind =
        fs::read_to_string(directory.join("packages/contracts/src/ProductMutationKind.php"))
            .unwrap();
    let mutation_state =
        fs::read_to_string(directory.join("packages/contracts/src/MutationResultState.php"))
            .unwrap();
    let delivery_state =
        fs::read_to_string(directory.join("packages/contracts/src/MutationDeliveryState.php"))
            .unwrap();
    let snapshot =
        fs::read_to_string(directory.join("packages/contracts/src/ProductSnapshot.php")).unwrap();
    assert!(surface.contains("case Server = 1;"));
    assert!(surface.contains("case Native = 2;"));
    assert!(surface.contains("case Desktop = 3;"));
    assert!(state.contains("case Operational = 1;"));
    assert!(state.contains("case Degraded = 2;"));
    assert!(state.contains("case Offline = 3;"));
    assert!(version.contains("case V1 = 1;"));
    assert!(mutation_kind.contains("case CheckIn = 1;"));
    assert!(mutation_state.contains("case Accepted = 1;"));
    assert!(delivery_state.contains("case Delivered = 1;"));
    assert!(delivery_state.contains("case Queued = 2;"));
    assert!(snapshot.contains("public static function fromArray(array $payload): self"));
    assert!(snapshot.contains("ContractVersion::tryFrom"));
    assert!(snapshot.contains("'versionCode' => $this->version->value"));
    let contract_test =
        fs::read_to_string(directory.join("packages/contracts/tests/contract.php")).unwrap();
    assert!(contract_test.contains("function expect(bool $condition, string $message): void"));
    assert!(!contract_test.contains("assert("));
    let snapshot_schema: serde_json::Value = serde_json::from_slice(
        &fs::read(directory.join("packages/contracts/schema/product-snapshot.schema.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(snapshot_schema["additionalProperties"], false);
    assert_eq!(snapshot_schema["properties"]["versionCode"]["const"], 1);
    assert_eq!(
        snapshot_schema["properties"]["surfaceCode"]["enum"],
        serde_json::json!([1, 2, 3])
    );
    let mutation_schema: serde_json::Value = serde_json::from_slice(
        &fs::read(directory.join("packages/contracts/schema/product-mutation.schema.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(mutation_schema["additionalProperties"], false);
    assert_eq!(
        mutation_schema["properties"]["mutationKindCode"]["const"],
        1
    );
    assert!(snapshot.contains("'surfaceCode' => $this->surface->value"));
    assert!(snapshot.contains("'stateCode' => $this->state->value"));
    let design_tokens: serde_json::Value = serde_json::from_slice(
        &fs::read(directory.join("packages/contracts/design-tokens.json")).unwrap(),
    )
    .unwrap();
    let design_token_schema: serde_json::Value = serde_json::from_slice(
        &fs::read(directory.join("packages/contracts/schema/product-design-tokens.schema.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(design_token_schema["additionalProperties"], false);
    assert_eq!(design_tokens["schemaVersion"], 1);
    assert_eq!(design_tokens["themes"][0]["modeCode"], 1);
    assert_eq!(design_tokens["themes"][1]["modeCode"], 2);
    assert_eq!(design_tokens["minimumTouchTarget"], 48);
    assert_eq!(
        design_tokens["motionMs"],
        serde_json::json!([150, 240, 360])
    );
    assert_eq!(
        design_tokens["spacing"],
        serde_json::json!([4, 8, 12, 16, 24, 32, 48])
    );
    let token_color = |theme: usize, role: &str| {
        let value = design_tokens["themes"][theme]["colors"][role]
            .as_str()
            .unwrap();
        let channel = |offset: usize| u8::from_str_radix(&value[offset..offset + 2], 16).unwrap();
        [
            channel(1) as f64 / 255.0,
            channel(3) as f64 / 255.0,
            channel(5) as f64 / 255.0,
        ]
    };
    for theme in 0..2 {
        for (foreground, background) in [
            ("foreground", "background"),
            ("mutedForeground", "background"),
            ("onPrimary", "primary"),
        ] {
            let ratio = contrast_ratio(
                token_color(theme, foreground),
                token_color(theme, background),
            );
            assert!(
                ratio >= 4.5,
                "theme {theme} {foreground} on {background} has insufficient contrast: {ratio:.2}:1"
            );
        }
    }
    assert!(contract_test.contains("Theme modes must use sequential integer codes."));
    assert!(contract_test.contains("Touch targets must remain accessible."));
    let contract = run_pam(&[directory
        .join("packages/contracts/tests/contract.php")
        .to_str()
        .unwrap()]);
    assert!(
        contract.status.success(),
        "{}",
        String::from_utf8_lossy(&contract.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&contract.stdout),
        "Cross-surface product contract verified.\n"
    );

    for application in ["server", "native", "desktop"] {
        let manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(directory.join(format!("apps/{application}/composer.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["require"]["app/product-contracts"], "^1.0");
        assert!(
            manifest["repositories"]
                .as_array()
                .unwrap()
                .iter()
                .any(|repository| {
                    repository["type"] == "path"
                        && repository["url"] == "../../packages/contracts"
                        && repository["options"]["symlink"] == false
                })
        );
    }
    assert!(
        fs::read_to_string(directory.join("apps/server/index.php"))
            .unwrap()
            .contains("ProductSurface::Server")
    );
    assert!(
        fs::read_to_string(directory.join("apps/server/tests/ApplicationTest.php"))
            .unwrap()
            .contains("assertJson(['versionCode' => 1, 'surfaceCode' => 1, 'stateCode' => 1")
    );
    let server_entry = fs::read_to_string(directory.join("apps/server/index.php")).unwrap();
    assert!(server_entry.contains("$app->post('/api/check-ins'"));
    assert!(server_entry.contains("hash_equals($mutation->idempotencyKey, $header)"));
    assert!(server_entry.contains("ProductMutationReceipt::accepted($mutation)"));
    assert!(
        fs::read_to_string(directory.join("apps/server/tests/ApplicationTest.php"))
            .unwrap()
            .contains("assertHeader('cache-control', 'no-store')")
    );
    assert!(
        fs::read_to_string(directory.join("apps/native/src/Hello.pam"))
            .unwrap()
            .contains("ProductSurface::Native")
    );
    let native_component = fs::read_to_string(directory.join("apps/native/src/Hello.pam")).unwrap();
    let native_theme =
        fs::read_to_string(directory.join("apps/native/src/ProductTheme.php")).unwrap();
    assert!(native_component.contains("ProductTheme::install();"));
    assert!(native_theme.contains("final class ProductTheme"));
    assert!(native_theme.contains("dirname(__DIR__, 3).'/packages/contracts/design-tokens.json'"));
    assert!(native_theme.contains("file_get_contents($path, false, null, 0, 32_769)"));
    assert!(native_theme.contains("array_keys($document) !== ['schemaVersion', 'themes', 'spacing', 'radii', 'motionMs', 'minimumTouchTarget']"));
    assert!(native_theme.contains("$payload['modeCode'] !== $modeCode"));
    assert!(native_theme.contains("PamUI::theme($light, $dark);"));
    assert!(native_theme.contains("Themes::light()"));
    assert!(native_theme.contains("Themes::dark()"));
    assert!(native_theme.contains("ColorToken::Primary->value"));
    assert!(native_theme.contains("ColorToken::Focus->value"));
    assert!(native_theme.contains("Product color must use canonical lowercase hex."));
    let native_theme_syntax = run_pam(&[directory
        .join("apps/native/src/ProductTheme.php")
        .to_str()
        .unwrap()]);
    assert!(
        native_theme_syntax.status.success(),
        "{}",
        String::from_utf8_lossy(&native_theme_syntax.stderr)
    );
    assert!(native_component.contains("use Pam\\Native\\Http\\Http;"));
    assert!(native_component.contains("PAM_PRODUCT_SERVER_URL"));
    assert!(native_component.contains("strlen($response->body) > 65_536"));
    assert!(native_component.contains("ProductSnapshot::fromArray($payload)"));
    assert!(native_component.contains("$snapshot->surface !== ProductSurface::Server"));
    assert!(native_component.contains("timeoutMs: 5_000"));
    assert!(native_component.contains("Server request could not start"));
    assert!(native_component.contains("use Pam\\Native\\Sync\\OfflineMutationQueue;"));
    assert!(native_component.contains("private const MAX_PENDING_MUTATIONS = 32;"));
    assert!(native_component.contains("$this->pendingMutations >= self::MAX_PENDING_MUTATIONS"));
    assert!(native_component.contains("Storage::get('product.outbox.v1'"));
    assert!(native_component.contains("Storage::set('product.outbox.v1'"));
    assert!(native_component.contains("PAM_PRODUCT_MUTATION_URL"));
    assert!(native_component.contains("ProductMutation::checkIn($key)"));
    assert!(native_component.contains("$this->outbox->retry"));
    assert!(native_component.contains("$this->outbox->prune()"));
    assert!(
        fs::read_to_string(directory.join("apps/desktop/app.php"))
            .unwrap()
            .contains("ProductSurface::Desktop")
    );
    let desktop_application = fs::read_to_string(directory.join("apps/desktop/app.php")).unwrap();
    assert!(desktop_application.contains("#[Command('product.server-status')]"));
    assert!(desktop_application.contains("#[Command('product.theme')]"));
    assert!(desktop_application.contains("public function productTheme(int $modeCode): array"));
    assert!(
        desktop_application
            .contains("dirname(__DIR__, 2).'/packages/contracts/design-tokens.json'")
    );
    assert!(desktop_application.contains("file_get_contents($path, false, null, 0, 32_769)"));
    assert!(desktop_application.contains("$theme['modeCode'] !== $modeCode"));
    assert!(desktop_application.contains("Product theme contains an invalid color."));
    assert!(desktop_application.contains("#[Command('product.telemetry-history')]"));
    assert!(desktop_application.contains("count($payload['samples']) > 24"));
    assert!(desktop_application.contains("product-telemetry-v1.json"));
    assert!(desktop_application.contains("array_slice($samples, -24)"));
    assert!(desktop_application.contains("Desktop product telemetry history exceeds 16 KiB"));
    assert!(desktop_application.contains("elapsedProductMilliseconds"));
    assert!(desktop_application.contains("['127.0.0.1', 'localhost', '::1']"));
    assert!(desktop_application.contains("$scheme !== 'https'"));
    assert!(desktop_application.contains("strtolower($parts['scheme'])"));
    assert!(desktop_application.contains("'follow_location' => 0"));
    assert!(
        desktop_application.contains("file_get_contents($endpoint, false, $context, 0, 65_537)")
    );
    assert!(desktop_application.contains("ProductSnapshot::fromArray($payload)"));
    assert!(desktop_application.contains("#[Command('product.check-in')]"));
    assert!(desktop_application.contains("#[Command('product.outbox.replay')]"));
    assert!(desktop_application.contains("count($outbox) >= 32"));
    assert!(desktop_application.contains("file_get_contents($path, false, null, 0, 65_537)"));
    assert!(desktop_application.contains("fopen($temporary, 'x+b')"));
    assert!(desktop_application.contains("fsync($handle)"));
    assert!(desktop_application.contains("rename($temporary, $path)"));
    let desktop_html =
        fs::read_to_string(directory.join("apps/desktop/resources/index.html")).unwrap();
    let desktop_styles =
        fs::read_to_string(directory.join("apps/desktop/resources/styles.css")).unwrap();
    let desktop_javascript =
        fs::read_to_string(directory.join("apps/desktop/resources/app.js")).unwrap();
    assert!(desktop_html.contains("id=\"product-refresh\""));
    assert!(desktop_html.contains("id=\"product-version-code\""));
    assert!(desktop_html.contains("id=\"product-check-in\""));
    assert!(desktop_html.contains("id=\"product-outbox-status\""));
    assert!(desktop_html.contains("aria-live=\"polite\""));
    assert!(desktop_html.contains("role=\"status\" aria-live=\"polite\""));
    assert!(desktop_styles.contains(".product-console button:focus-visible"));
    assert!(desktop_styles.contains("min-height: 48px"));
    assert!(desktop_styles.contains("@media (prefers-reduced-motion: reduce)"));
    assert!(desktop_styles.contains("@media (max-width: 520px)"));
    assert!(desktop_html.contains("class=\"product-console\""));
    assert!(desktop_html.contains("id=\"product-surfaces-title\""));
    assert!(desktop_html.contains("id=\"product-outbox-meter\""));
    assert!(desktop_html.contains("id=\"product-history-chart\""));
    assert!(desktop_html.contains("aria-label=\"Histórico cronológico de consultas ao Server\""));
    assert!(desktop_html.contains("Não monitorado nesta sessão Desktop"));
    assert!(desktop_javascript.contains("window.pam.invoke(\"product.server-status\""));
    assert!(desktop_javascript.contains("window.pam.invoke(\"product.theme\", { modeCode }"));
    assert!(desktop_javascript.contains("window.matchMedia(\"(prefers-color-scheme: dark)\")"));
    assert!(desktop_javascript.contains("themeQuery.addEventListener(\"change\""));
    assert!(
        desktop_javascript.contains("document.documentElement.style.setProperty(property, value)")
    );
    assert!(
        desktop_javascript
            .contains("Object.keys(theme.colors).join(\",\") !== themeRoles.join(\",\")")
    );
    assert!(
        desktop_styles
            .contains("background: linear-gradient(145deg, var(--surface-raised), var(--ink))")
    );
    assert!(desktop_styles.contains("background: var(--surface-raised)"));
    assert!(desktop_javascript.contains("snapshot.versionCode !== 1"));
    assert!(desktop_javascript.contains("snapshot.surfaceCode !== 1"));
    assert!(desktop_javascript.contains("Number.isInteger(snapshot.stateCode)"));
    assert!(desktop_javascript.contains("window.pam.invoke(\"product.check-in\""));
    assert!(desktop_javascript.contains("window.pam.invoke(\"product.outbox.replay\""));
    assert!(desktop_javascript.contains("result.pendingCount > 32"));
    assert!(desktop_javascript.contains("window.pam.invoke(\"product.telemetry-history\""));
    assert!(desktop_javascript.contains("result.samples.length > 24"));
    assert!(desktop_javascript.contains("Number.isSafeInteger(sample.observedAtUnixMs)"));
    assert!(desktop_javascript.contains("historyChart.replaceChildren()"));
    assert!(desktop_javascript.contains("availability}% operacional"));
    assert!(desktop_javascript.contains("Intl.DateTimeFormat"));
    assert!(desktop_javascript.contains("surface.dataset.state === \"ready\""));

    for (application, artifact, contents) in [
        ("server", "product-server.tar.gz", b"server".as_slice()),
        ("native", "product-native.aab", b"native".as_slice()),
        ("desktop", "product-desktop.zip", b"desktop".as_slice()),
    ] {
        let dist = directory.join("apps").join(application).join("dist");
        fs::create_dir_all(&dist).unwrap();
        fs::write(dist.join(artifact), contents).unwrap();
        fs::write(dist.join(format!("{artifact}.sha256")), "ignored sidecar").unwrap();
    }
    let native_screenshots = directory.join("apps/native/artifacts/screenshots");
    let desktop_screenshots = directory.join("apps/desktop/artifacts/screenshots");
    fs::create_dir_all(directory.join("artifacts")).unwrap();
    fs::create_dir_all(&native_screenshots).unwrap();
    fs::create_dir_all(&desktop_screenshots).unwrap();
    let token_bytes = fs::read(directory.join("packages/contracts/design-tokens.json")).unwrap();
    let token_sha256 = format!("{:x}", Sha256::digest(&token_bytes));
    for (mode_code, mode) in [(1_u8, "light"), (2_u8, "dark")] {
        let mut captures = Vec::new();
        for (surface_code, surface) in [(2_u8, "native"), (3_u8, "desktop")] {
            let screenshots = if surface_code == 2 {
                &native_screenshots
            } else {
                &desktop_screenshots
            };
            let path = screenshots.join(format!("product-{surface}-{mode}.png"));
            let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
            png.extend_from_slice(&[0, 0, 0, 13]);
            png.extend_from_slice(b"IHDR");
            png.extend_from_slice(&1_u32.to_be_bytes());
            png.extend_from_slice(&1_u32.to_be_bytes());
            fs::write(&path, &png).unwrap();
            let anchors = [
                "background",
                "surface",
                "foreground",
                "primary",
                "focus",
                "danger",
            ]
            .into_iter()
            .map(|role| {
                serde_json::json!({
                    "role": role,
                    "target": "#000000",
                    "closestChannelDelta": 0,
                    "matchingPixels": 1,
                    "requiredPixels": 1,
                    "passed": true,
                })
            })
            .collect::<Vec<_>>();
            captures.push(serde_json::json!({
                "surfaceCode": surface_code,
                "name": surface,
                "width": 1,
                "height": 1,
                "bytes": png.len(),
                "sha256": format!("{:x}", Sha256::digest(&png)),
                "visiblePixels": 1,
                "anchors": anchors,
                "passed": true,
            }));
        }
        fs::write(
            directory.join(format!("artifacts/product-visual-{mode}.json")),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schemaVersion": 1,
                "modeCode": mode_code,
                "tokenSha256": token_sha256,
                "toleranceChannelDelta": 12,
                "captures": captures,
                "passed": true,
            }))
            .unwrap(),
        )
        .unwrap();
    }
    let package = run_pam_in(&directory, &["package"]);
    assert!(
        package.status.success(),
        "{}",
        String::from_utf8_lossy(&package.stderr)
    );
    let release_bytes = fs::read(directory.join("dist/product-release.json")).unwrap();
    let release: serde_json::Value = serde_json::from_slice(&release_bytes).unwrap();
    assert_eq!(release["schemaVersionCode"], 1);
    assert_eq!(release["artifacts"].as_array().unwrap().len(), 3);
    assert_eq!(release["visualEvidence"].as_array().unwrap().len(), 2);
    assert_eq!(release["visualEvidence"][0]["modeCode"], 1);
    assert_eq!(release["visualEvidence"][1]["modeCode"], 2);
    assert_eq!(
        release["visualEvidence"][0]["captures"][0]["surfaceCode"],
        2
    );
    assert_eq!(
        release["visualEvidence"][0]["captures"][1]["surfaceCode"],
        3
    );
    assert_eq!(release["artifacts"][0]["surfaceCode"], 3);
    assert_eq!(
        release["artifacts"][0]["path"],
        "apps/desktop/dist/product-desktop.zip"
    );
    assert_eq!(release["artifacts"][1]["surfaceCode"], 2);
    assert_eq!(release["artifacts"][2]["surfaceCode"], 1);
    for artifact in release["artifacts"].as_array().unwrap() {
        assert_eq!(artifact["sha256"].as_str().unwrap().len(), 64);
        assert!(artifact["sizeBytes"].as_u64().unwrap() > 0);
        assert!(!artifact["path"].as_str().unwrap().ends_with(".sha256"));
    }
    let release_digest = format!("{:x}", Sha256::digest(&release_bytes));
    assert_eq!(
        fs::read_to_string(directory.join("dist/product-release.json.sha256")).unwrap(),
        format!("{release_digest}  product-release.json\n")
    );
    let verified = run_pam_in(&directory, &["release:verify"]);
    assert!(
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stderr)
    );
    let verification_output = String::from_utf8_lossy(&verified.stdout);
    assert!(verification_output.contains("Verified Product release"));
    assert!(verification_output.contains("(3 artifacts, 2 visual modes)"));
    let native_dark = directory.join("apps/native/artifacts/screenshots/product-native-dark.png");
    let native_dark_bytes = fs::read(&native_dark).unwrap();
    fs::write(&native_dark, b"tampered visual capture").unwrap();
    let tampered_visual = run_pam_in(&directory, &["release:verify"]);
    assert!(!tampered_visual.status.success());
    assert!(
        String::from_utf8_lossy(&tampered_visual.stderr).contains("visual capture digest mismatch")
    );
    fs::write(&native_dark, native_dark_bytes).unwrap();
    let overwrite = run_pam_in(&directory, &["package"]);
    assert!(!overwrite.status.success());
    assert!(String::from_utf8_lossy(&overwrite.stderr).contains("refusing to overwrite"));
    let reproduced = run_pam_in(&directory, &["package", "--output", "dist-copy"]);
    assert!(reproduced.status.success());
    assert_eq!(
        fs::read(directory.join("dist-copy/product-release.json")).unwrap(),
        release_bytes
    );
    assert_eq!(
        fs::read(directory.join("dist-copy/product-release.json.sha256")).unwrap(),
        fs::read(directory.join("dist/product-release.json.sha256")).unwrap()
    );
    let copied = run_pam_in(
        &directory,
        &["release:verify", "dist-copy/product-release.json"],
    );
    assert!(copied.status.success());

    let server_artifact = directory.join("apps/server/dist/product-server.tar.gz");
    fs::write(&server_artifact, b"tampered").unwrap();
    let tampered = run_pam_in(&directory, &["release:verify"]);
    assert!(!tampered.status.success());
    assert!(String::from_utf8_lossy(&tampered.stderr).contains("artifact size mismatch"));
    fs::write(&server_artifact, b"server").unwrap();

    fs::write(
        directory.join("dist-copy/product-release.json.sha256"),
        format!("{}  product-release.json\n", "0".repeat(64)),
    )
    .unwrap();
    let invalid_sidecar = run_pam_in(
        &directory,
        &["release:verify", "dist-copy/product-release.json"],
    );
    assert!(!invalid_sidecar.status.success());
    assert!(
        String::from_utf8_lossy(&invalid_sidecar.stderr).contains("manifest checksum mismatch")
    );

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            directory.join("apps/server/dist/product-server.tar.gz"),
            directory.join("apps/server/dist/linked-release.tar.gz"),
        )
        .unwrap();
        let linked = run_pam_in(&directory, &["package", "--output", "dist-linked"]);
        assert!(!linked.status.success());
        assert!(String::from_utf8_lossy(&linked.stderr).contains("symbolic link"));
        fs::remove_file(directory.join("apps/server/dist/linked-release.tar.gz")).unwrap();
    }
    let unsafe_name = directory.join("apps/server/dist/not portable.zip");
    fs::write(&unsafe_name, b"unsafe").unwrap();
    let unsafe_package = run_pam_in(&directory, &["package", "--output", "dist-unsafe"]);
    assert!(!unsafe_package.status.success());
    assert!(String::from_utf8_lossy(&unsafe_package.stderr).contains("not portable"));
    fs::remove_file(unsafe_name).unwrap();

    fs::remove_file(directory.join("artifacts/product-visual-dark.json")).unwrap();
    let partial_visual = run_pam_in(&directory, &["package", "--output", "dist-partial-visual"]);
    assert!(!partial_visual.status.success());
    assert!(
        String::from_utf8_lossy(&partial_visual.stderr)
            .contains("requires both light and dark reports")
    );

    let release_schema: serde_json::Value = serde_json::from_slice(
        &fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("docs/schemas/product-release.schema.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        release_schema["properties"]["schemaVersionCode"]["const"],
        1
    );
    assert_eq!(release_schema["properties"]["artifacts"]["maxItems"], 64);
    assert_eq!(
        release_schema["properties"]["visualEvidence"]["minItems"],
        2
    );

    let generated_cache = directory.join("apps/desktop/target/debug/incremental");
    fs::create_dir_all(&generated_cache).unwrap();
    fs::write(generated_cache.join("cache.bin"), [0_u8; 64]).unwrap();
    let clean = run_pam_in(&directory, &["clean", "--all"]);
    assert!(
        clean.status.success(),
        "{}",
        String::from_utf8_lossy(&clean.stderr)
    );
    assert!(!directory.join("apps/desktop/target").exists());
    assert!(
        directory
            .join("packages/contracts/src/ProductSnapshot.php")
            .is_file()
    );

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn initializes_a_servo_desktop_project_with_php_commands() {
    let directory = temporary_path("init-desktop");
    let output = run_pam(&[
        "init",
        directory.to_str().unwrap(),
        "--template",
        "desktop",
        "--no-install",
        "--no-interaction",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let manifest = fs::read_to_string(directory.join("composer.json")).unwrap();
    let application = fs::read_to_string(directory.join("app.php")).unwrap();
    let html = fs::read_to_string(directory.join("resources/index.html")).unwrap();
    let styles = fs::read_to_string(directory.join("resources/styles.css")).unwrap();
    let javascript = fs::read_to_string(directory.join("resources/app.js")).unwrap();
    let inspector = fs::read_to_string(directory.join("resources/inspector.html")).unwrap();
    let inspector_styles = fs::read_to_string(directory.join("resources/inspector.css")).unwrap();
    let inspector_javascript =
        fs::read_to_string(directory.join("resources/inspector.js")).unwrap();

    assert!(manifest.contains("\"pushinbr/pam-desktop\""));
    assert!(manifest.contains("\"pushinbr/pam-desktop\": \"^1.2\""));
    assert!(manifest.contains("pam desktop build ."));
    assert!(manifest.contains("pam desktop dev ."));
    assert!(application.contains("final class HelloApp extends App"));
    assert!(application.contains("#[DesktopApplication("));
    assert!(application.contains("#[Command]"));
    assert!(application.contains("#[Listen('client.ready')]"));
    assert!(application.contains("ApplicationCategory::Development"));
    assert!(application.contains("extends DesktopWindow"));
    assert!(application.contains("Events $events"));
    assert!(application.contains("->timeout(10_000)"));
    assert!(application.contains("Permissions $permissions"));
    assert!(application.contains("->filesystem('data'"));
    assert!(application.contains("$window->title"));
    assert!(html.contains("/_pam/bridge.js"));
    assert!(html.contains("aria-live=\"polite\""));
    assert!(html.contains("aria-atomic=\"true\""));
    assert!(html.contains("runtime conectando"));
    assert!(html.contains("NATIVE AUTHORITY · API 1"));
    assert!(html.contains("SIGNED UPDATES · API 1"));
    assert!(html.contains("<strong>Servo LTS</strong>"));
    assert!(!html.contains("<strong>Servo 0."));
    assert!(html.contains("aria-describedby=\"name-hint\""));
    assert!(!html.contains("value=\"David\""));
    assert!(!html.contains("CAPABILITIES 0.3"));
    assert!(!html.contains("SIGNED UPDATES · 0.5"));
    assert!(html.contains("IPC v6"));
    assert!(html.contains("Native Lab"));
    assert!(html.contains("Atualizações com rollback"));
    assert!(styles.contains("prefers-reduced-motion"));
    assert!(styles.contains("prefers-contrast: more"));
    assert!(styles.contains("forced-colors: active"));
    assert!(styles.contains(":focus-visible"));
    assert!(styles.contains("width: min(100%, 440px)"));
    assert!(!styles.contains("width: min(96vw, 440px)"));
    for (foreground, background) in [
        ("--text", "--ink"),
        ("--text-soft", "--ink"),
        ("--text-faint", "--ink"),
        ("--run-ink", "--run"),
    ] {
        let ratio = contrast_ratio(css_hex(&styles, foreground), css_hex(&styles, background));
        assert!(
            ratio >= 4.5,
            "{foreground} on {background} has insufficient contrast: {ratio:.2}:1"
        );
    }
    assert!(javascript.contains("window.pam.invoke(\"greet\""));
    assert!(javascript.contains("runtimeStatus.dataset.state = \"ready\""));
    assert!(javascript.contains("state === \"error\" ? \"assertive\" : \"polite\""));
    assert!(javascript.contains("document.querySelectorAll(\"button, input\")"));
    assert!(javascript.contains("response.focus()"));
    assert!(javascript.contains("window.pam.on(\"pam.dev.reloaded\""));
    assert!(javascript.contains("{ timeout: 5_000 }"));
    assert!(javascript.contains("window.pam.fs.writeText"));
    assert!(javascript.contains("window.pam.dialog.openFile"));
    assert!(javascript.contains("window.pam.clipboard.writeText"));
    assert!(javascript.contains("window.pam.notification.show"));
    assert!(javascript.contains("window.pam.on(\"pam.drag.drop\""));
    assert!(javascript.contains("window.pam.updater.status"));
    assert!(directory.join("storage/.gitkeep").is_file());
    assert!(directory.join("resources/icon.svg").is_file());
    assert!(inspector.contains("Runtime Inspector"));
    assert!(inspector.contains("/_pam/bridge.js"));
    assert!(inspector_styles.contains("prefers-reduced-motion"));
    assert!(inspector_javascript.contains("window.pam.windowId"));

    let invalid = run_pam(&[
        "init",
        temporary_path("init-desktop-socket").to_str().unwrap(),
        "--template",
        "desktop",
        "--socket",
        "--no-install",
    ]);
    assert!(!invalid.status.success());
    assert!(
        String::from_utf8_lossy(&invalid.stderr).contains("do not use --socket"),
        "{}",
        String::from_utf8_lossy(&invalid.stderr),
    );

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn benchmarks_an_http_endpoint() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        for stream in listener.incoming().take(12) {
            let mut stream = stream.unwrap();
            let mut request = Vec::new();
            let mut chunk = [0_u8; 256];
            loop {
                let read = stream.read(&mut chunk).unwrap();
                request.extend_from_slice(&chunk[..read]);
                if read == 0 || request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .unwrap();
        }
    });
    let output = run_pam(&[
        "benchmark",
        &format!("http://127.0.0.1:{port}/health"),
        "--requests",
        "12",
        "--concurrency",
        "3",
    ]);
    server.join().unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(
        report.contains("Successful") && report.contains("12"),
        "{report}"
    );
    assert!(report.contains("Latency p95"), "{report}");
}
