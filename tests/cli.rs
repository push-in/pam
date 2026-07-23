use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

fn run_pam(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pam"))
        .args(arguments)
        .output()
        .expect("pam should start")
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
fn exposes_native_diagnostics_and_builds_a_portable_bundle() {
    let diagnostics = run_pam(&["diagnostics", fixture("hello.php").to_str().unwrap()]);
    assert!(
        diagnostics.status.success(),
        "{}",
        String::from_utf8_lossy(&diagnostics.stderr)
    );
    let snapshot: serde_json::Value = serde_json::from_slice(&diagnostics.stdout).unwrap();
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
    assert!(bundle.join("lib").read_dir().unwrap().next().is_some());
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
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot open"));
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
    assert_eq!(payload["signalReceived"], true);
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
    assert!(String::from_utf8_lossy(&help.stderr).contains("benchmark"));
    let version = run_pam(&["--version"]);
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("pam "));
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
    assert!(manifest.contains("\"pam/api\""));

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
    assert!(manifest.contains("pam/socket"));

    fs::remove_dir_all(raw).unwrap();
    fs::remove_dir_all(api).unwrap();
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
    assert!(report.contains("successful: 12"), "{report}");
    assert!(report.contains("latency p95:"), "{report}");
}
