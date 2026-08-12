use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

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
    assert!(init_help.contains("mobile, or mobile-ui"));
    assert!(init_help.contains("--template mobile-ui"));

    let mobile_help = run_pam(&["mobile", "--help"]);
    assert!(mobile_help.status.success());
    let mobile_help = String::from_utf8_lossy(&mobile_help.stderr);
    assert!(mobile_help.contains("PAM / MOBILE"));
    assert!(mobile_help.contains("make:screen"));

    let version = run_pam(&["--version"]);
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("pam "));
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
        "#!/bin/sh\nprintf 'pam=%s\\n' \"$PAM_BINARY\"\nprintf 'args=%s|%s|%s\\n' \"$1\" \"$2\" \"$3\"\nexit 23\n",
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
    let package = Command::new(env!("CARGO_BIN_EXE_pam"))
        .arg("package")
        .current_dir(&directory)
        .env("PAM_DESKTOP_BINARY", &desktop)
        .output()
        .unwrap();
    assert_eq!(package.status.code(), Some(23));
    let package_output = String::from_utf8_lossy(&package.stdout);
    assert!(package_output.contains("args=build|"), "{package_output}");

    let tests = run_pam_in(&directory, &["test"]);
    assert!(tests.status.success());
    assert!(String::from_utf8_lossy(&tests.stdout).contains("No application test runner"));

    fs::remove_dir_all(directory).unwrap();
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
    assert_eq!(report["healthy"], true);
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
    assert!(html.contains("IPC v6"));
    assert!(html.contains("Native Lab"));
    assert!(html.contains("Atualizações com rollback"));
    assert!(styles.contains("prefers-reduced-motion"));
    assert!(styles.contains(":focus-visible"));
    assert!(javascript.contains("window.pam.invoke(\"greet\""));
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
