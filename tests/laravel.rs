use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const SIGINT: i32 = 2;
const APP_KEY: &str = "base64:MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=";

unsafe extern "C" {
    fn kill(process_id: i32, signal: i32) -> i32;
}

struct LaravelProcess {
    child: Child,
    port: u16,
    storage: PathBuf,
    database: Option<PathBuf>,
}

impl LaravelProcess {
    fn start() -> Self {
        Self::start_with_options(None, false)
    }

    fn start_with_response_limit(max_response_bytes: Option<usize>) -> Self {
        Self::start_with_options(max_response_bytes, false)
    }

    fn start_with_observers() -> Self {
        Self::start_with_options(None, true)
    }

    fn start_with_options(max_response_bytes: Option<usize>, observers: bool) -> Self {
        let root = laravel_root();
        assert!(
            root.join("vendor/autoload.php").is_file(),
            "install compat/laravel-smoke dependencies before running the Laravel contract"
        );
        let optional_packages = root.join("vendor/laravel/socialite").is_dir()
            || root.join("vendor/laravel/horizon").is_dir();
        let probe = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        let storage = std::env::temp_dir()
            .join(format!("pam-laravel-storage-{}-{port}", std::process::id(),));
        fs::create_dir(&storage).unwrap();
        let database = observers.then(|| storage.join("observers.sqlite"));
        if let Some(database) = &database {
            fs::File::create(database).unwrap();
            let database = database.to_string_lossy();
            for arguments in [
                vec!["migrate", "--force", "--no-interaction"],
                vec![
                    "migrate",
                    "--path=vendor/laravel/telescope/database/migrations",
                    "--force",
                    "--no-interaction",
                ],
                vec![
                    "migrate",
                    "--path=vendor/laravel/pulse/database/migrations",
                    "--force",
                    "--no-interaction",
                ],
            ] {
                let migration = run_artisan(&root, &database, &arguments);
                assert!(
                    migration.status.success(),
                    "stdout={} stderr={}",
                    String::from_utf8_lossy(&migration.stdout),
                    String::from_utf8_lossy(&migration.stderr),
                );
            }
        }

        let mut command = Command::new(env!("CARGO_BIN_EXE_pam"));
        command
            .arg("pam.php")
            .current_dir(root)
            .env("PAM_LARAVEL_SMOKE_PORT", port.to_string())
            .env("PAM_LARAVEL_STORAGE_PATH", &storage)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(if observers || optional_packages {
                Stdio::inherit()
            } else {
                Stdio::null()
            });
        configure_laravel_environment(
            &mut command,
            database
                .as_deref()
                .map_or(std::ffi::OsStr::new(":memory:"), std::path::Path::as_os_str),
            observers,
        );
        if optional_packages {
            command.env("APP_DEBUG", "true");
        }
        if let Some(limit) = max_response_bytes {
            command.env("PAM_LARAVEL_MAX_RESPONSE_BYTES", limit.to_string());
        }
        let child = command.spawn().expect("Laravel on Pam should start");
        let mut process = Self {
            child,
            port,
            storage,
            database,
        };
        process.wait_until_ready();
        process
    }

    fn wait_until_ready(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            if request(self.port, "GET", "/api/ping", "", &[])
                .is_ok_and(|response| response.contains(r#"{"message":"pong"}"#))
            {
                return;
            }
            if let Some(status) = self.child.try_wait().unwrap() {
                panic!("Laravel on Pam exited during boot with {status}");
            }
            thread::sleep(Duration::from_millis(40));
        }
        panic!("Laravel on Pam did not become ready");
    }

    fn stop(&mut self) {
        if self.child.try_wait().unwrap().is_some() {
            return;
        }
        // SAFETY: This PID belongs to the child created by this test.
        unsafe {
            kill(self.child.id() as i32, SIGINT);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if self.child.try_wait().unwrap().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(25));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for LaravelProcess {
    fn drop(&mut self) {
        self.stop();
        fs::remove_dir_all(&self.storage).unwrap();
    }
}

#[test]
fn runs_artisan_with_real_cli_identity_and_arguments() {
    let root = laravel_root();
    let output = run_artisan(&root, ":memory:", &["--version"]);

    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Laravel Framework"),
        "{}",
        String::from_utf8_lossy(&output.stdout),
    );

    let runtime = run_artisan(&root, ":memory:", &["pam:runtime", "argument-compatible"]);
    assert!(
        runtime.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&runtime.stdout),
        String::from_utf8_lossy(&runtime.stderr),
    );
    let contract: serde_json::Value = serde_json::from_slice(runtime.stdout.trim_ascii()).unwrap();
    assert_eq!(contract["sapi"], "cli");
    assert_eq!(contract["console"], true);
    assert_eq!(contract["value"], "argument-compatible");
    assert_eq!(contract["binary"], "pam");
    assert_eq!(contract["stdin"], true);
    assert_eq!(contract["stdout"], true);
    assert_eq!(contract["stderr"], true);

    let routes = run_artisan(
        &root,
        ":memory:",
        &["route:list", "--path=api", "--except-vendor"],
    );

    assert!(
        routes.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&routes.stdout),
        String::from_utf8_lossy(&routes.stderr),
    );
    assert!(
        String::from_utf8_lossy(&routes.stdout).contains("api/ping"),
        "{}",
        String::from_utf8_lossy(&routes.stdout),
    );

    let commands = run_artisan(&root, ":memory:", &["list", "--raw"]);
    assert!(
        commands.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&commands.stdout),
        String::from_utf8_lossy(&commands.stderr),
    );
    let commands = String::from_utf8_lossy(&commands.stdout);
    for command in [
        "livewire:publish",
        "pulse:check",
        "reverb:start",
        "sanctum:prune-expired",
        "scout:flush",
        "telescope:prune",
    ] {
        assert!(commands.contains(command), "missing {command}: {commands}");
    }
    if root.join("vendor/laravel/horizon").is_dir() {
        assert!(commands.contains("horizon"), "missing horizon: {commands}");
    }

    let schedule = run_artisan(&root, ":memory:", &["schedule:list"]);
    assert!(
        schedule.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&schedule.stdout),
        String::from_utf8_lossy(&schedule.stderr),
    );
    assert!(
        String::from_utf8_lossy(&schedule.stdout).contains("pam:schedule-probe"),
        "{}",
        String::from_utf8_lossy(&schedule.stdout),
    );
}

#[test]
fn runs_migrations_and_a_real_database_queue_through_artisan() {
    let root = laravel_root();
    let database = std::env::temp_dir().join(format!(
        "pam-laravel-artisan-{}-{}.sqlite",
        std::process::id(),
        std::thread::current().name().unwrap_or("contract"),
    ));
    fs::File::create(&database).unwrap();
    let database = database.to_string_lossy();

    let migration = run_artisan(
        &root,
        &database,
        &["migrate", "--force", "--no-interaction"],
    );
    assert!(
        migration.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&migration.stdout),
        String::from_utf8_lossy(&migration.stderr),
    );

    let status = run_artisan(&root, &database, &["migrate:status", "--no-interaction"]);
    assert!(
        status.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr),
    );
    assert!(
        String::from_utf8_lossy(&status.stdout).contains("create_pam_compatibility_tables"),
        "{}",
        String::from_utf8_lossy(&status.stdout),
    );

    let seeded = run_artisan(&root, &database, &["pam:queue-seed", "queue-compatible"]);
    assert!(
        seeded.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&seeded.stdout),
        String::from_utf8_lossy(&seeded.stderr),
    );

    let worked = run_artisan(
        &root,
        &database,
        &[
            "queue:work",
            "database",
            "--once",
            "--no-interaction",
            "--tries=1",
        ],
    );
    assert!(
        worked.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&worked.stdout),
        String::from_utf8_lossy(&worked.stderr),
    );

    let result = run_artisan(&root, &database, &["pam:queue-result"]);
    assert!(
        result.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr),
    );
    assert!(
        String::from_utf8_lossy(&result.stdout).contains("queue-compatible"),
        "{}",
        String::from_utf8_lossy(&result.stdout),
    );

    fs::remove_file(database.as_ref()).unwrap();
}

#[test]
fn rejects_unsafe_laravel_intra_worker_concurrency() {
    let root = laravel_root();
    let output = Command::new(env!("CARGO_BIN_EXE_pam"))
        .arg("pam.php")
        .current_dir(root)
        .env("PAM_LARAVEL_SMOKE_PORT", "31399")
        .env("PAM_LARAVEL_MAX_CONCURRENT_REQUESTS", "2")
        .stdin(Stdio::null())
        .output()
        .expect("unsafe Laravel configuration should be evaluated");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Laravel requires maxConcurrentRequests=1 per worker"),
        "{stderr}"
    );
}

#[test]
fn supports_laravel_database_cache_auth_validation_files_and_web_state() {
    let mut pam = LaravelProcess::start();

    let packages = request(pam.port, "GET", "/api/packages", "", &[]).unwrap();
    assert!(packages.starts_with("HTTP/1.1 200"), "{packages}");
    for package in [
        "inertia",
        "livewire",
        "pulse",
        "reverb",
        "sanctum",
        "scout",
        "telescope",
    ] {
        assert!(
            packages.contains(&format!(r#""{package}":true"#)),
            "{packages}"
        );
    }
    for optional in ["horizon", "socialite"] {
        if laravel_root()
            .join(format!("vendor/laravel/{optional}"))
            .is_dir()
        {
            assert!(
                packages.contains(&format!(r#""{optional}":true"#)),
                "{packages}"
            );
        }
    }
    if packages.contains(r#""socialite":true"#) {
        let socialite = request(pam.port, "GET", "/api/socialite-redirect", "", &[]).unwrap();
        assert!(socialite.starts_with("HTTP/1.1 200"), "{socialite}");
        let socialite_body: serde_json::Value =
            serde_json::from_str(response_body(&socialite)).unwrap();
        assert!(
            socialite_body["redirect"]
                .as_str()
                .is_some_and(|redirect| redirect.contains("github.com/login/oauth/authorize")),
            "{socialite}"
        );
    }

    let storage = request(pam.port, "GET", "/api/storage-path", "", &[]).unwrap();
    assert!(storage.starts_with("HTTP/1.1 200"), "{storage}");
    assert!(
        storage.contains("pam-laravel-storage-"),
        "custom storage path was not applied: {storage}"
    );

    let token_response = request(
        pam.port,
        "POST",
        "/api/sanctum/token",
        "",
        &[("accept", "application/json")],
    )
    .unwrap();
    assert!(
        token_response.starts_with("HTTP/1.1 200"),
        "{token_response}"
    );
    let token = serde_json::from_str::<serde_json::Value>(response_body(&token_response)).unwrap()
        ["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let authorization = format!("Bearer {token}");
    let sanctum_user = request(
        pam.port,
        "GET",
        "/api/sanctum/user",
        "",
        &[
            ("accept", "application/json"),
            ("authorization", &authorization),
        ],
    )
    .unwrap();
    assert!(sanctum_user.starts_with("HTTP/1.1 200"), "{sanctum_user}");
    assert!(
        sanctum_user.contains(r#""email":"pam@example.test""#),
        "{sanctum_user}"
    );
    let sanctum_anonymous = request(
        pam.port,
        "GET",
        "/api/sanctum/user",
        "",
        &[("accept", "application/json")],
    )
    .unwrap();
    assert!(
        sanctum_anonymous.starts_with("HTTP/1.1 401"),
        "{sanctum_anonymous}"
    );

    let scout = request(pam.port, "GET", "/api/scout/PAM", "", &[]).unwrap();
    assert!(scout.starts_with("HTTP/1.1 200"), "{scout}");
    assert!(scout.contains(r#""pam@example.test""#), "{scout}");

    let inertia = request(pam.port, "GET", "/inertia-contract", "", &[]).unwrap();
    assert!(inertia.starts_with("HTTP/1.1 200"), "{inertia}");
    assert!(inertia.contains("inertia-compatible"), "{inertia}");
    assert!(inertia.contains("data-page="), "{inertia}");

    let livewire = request(pam.port, "GET", "/livewire-contract", "", &[]).unwrap();
    assert!(livewire.starts_with("HTTP/1.1 200"), "{livewire}");
    assert!(livewire.contains("livewire-compatible:0"), "{livewire}");
    assert!(livewire.contains("wire:snapshot"), "{livewire}");

    let authenticated = request(
        pam.port,
        "GET",
        "/api/auth",
        "",
        &[("authorization", "Bearer pam-secret")],
    )
    .unwrap();
    assert!(
        authenticated.contains(r#"{"authenticated":true,"id":42}"#),
        "{authenticated}"
    );
    let anonymous = request(pam.port, "GET", "/api/auth", "", &[]).unwrap();
    assert!(
        anonymous.contains(r#"{"authenticated":false,"id":null}"#),
        "{anonymous}"
    );

    let valid = request(
        pam.port,
        "POST",
        "/api/validate",
        r#"{"email":"pam@example.test","count":2}"#,
        &[
            ("content-type", "application/json"),
            ("accept", "application/json"),
        ],
    )
    .unwrap();
    assert!(valid.starts_with("HTTP/1.1 200"), "{valid}");
    assert!(valid.contains(r#""email":"pam@example.test""#), "{valid}");
    let invalid = request(
        pam.port,
        "POST",
        "/api/validate",
        r#"{"email":"invalid","count":0}"#,
        &[
            ("content-type", "application/json"),
            ("accept", "application/json"),
        ],
    )
    .unwrap();
    assert!(invalid.starts_with("HTTP/1.1 422"), "{invalid}");

    let cached = request(pam.port, "PUT", "/api/cache/persistent", "", &[]).unwrap();
    assert!(cached.contains(r#"{"value":"persistent"}"#), "{cached}");
    let cached_next = request(pam.port, "GET", "/api/cache", "", &[]).unwrap();
    assert!(
        cached_next.contains(r#"{"value":"persistent"}"#),
        "{cached_next}"
    );

    let first_row = request(pam.port, "POST", "/api/database/first", "", &[]).unwrap();
    assert!(
        first_row.contains(r#"{"count":1,"latest":"first"}"#),
        "{first_row}"
    );
    let second_row = request(pam.port, "POST", "/api/database/second", "", &[]).unwrap();
    assert!(
        second_row.contains(r#"{"count":2,"latest":"second"}"#),
        "{second_row}"
    );

    let job = request(pam.port, "POST", "/api/sync-job/dispatched", "", &[]).unwrap();
    assert!(job.contains(r#"{"value":"dispatched"}"#), "{job}");

    for value in ["first-context", "second-context"] {
        let injected = request(
            pam.port,
            "GET",
            &format!("/api/container-injection/{value}"),
            "",
            &[],
        )
        .unwrap();
        assert!(
            injected.contains(&format!(r#"{{"event":"{value}","bus":"{value}"}}"#)),
            "{injected}"
        );
    }

    let changed_locale = request(pam.port, "GET", "/api/locale/pt_BR", "", &[]).unwrap();
    assert!(
        changed_locale.contains(r#"{"application":"pt_BR","translator":"pt_BR"}"#),
        "{changed_locale}"
    );
    let default_locale = request(pam.port, "GET", "/api/locale", "", &[]).unwrap();
    assert!(
        default_locale.contains(r#"{"application":"en","translator":"en"}"#),
        "request locale leaked into the next request: {default_locale}"
    );

    let filesystem = request(
        pam.port,
        "PUT",
        "/api/filesystem/storage-compatible",
        "",
        &[],
    )
    .unwrap();
    assert!(
        filesystem.contains(r#"{"contents":"storage-compatible"}"#),
        "{filesystem}"
    );

    let boundary = "pam-laravel-boundary";
    let upload_body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"description\"\r\n\r\ncontract\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"document\"; filename=\"proof.txt\"\r\nContent-Type: text/plain\r\n\r\nuploaded-content\r\n--{boundary}--\r\n"
    );
    let upload_content_type = format!("multipart/form-data; boundary={boundary}");
    let uploaded = request(
        pam.port,
        "POST",
        "/api/upload",
        &upload_body,
        &[
            ("content-type", &upload_content_type),
            ("accept", "application/json"),
        ],
    )
    .unwrap();
    assert!(uploaded.starts_with("HTTP/1.1 200"), "{uploaded}");
    assert!(
        uploaded.contains(r#""description":"contract""#),
        "{uploaded}"
    );
    assert!(uploaded.contains(r#""name":"proof.txt""#), "{uploaded}");
    assert!(
        uploaded.contains(r#""contents":"uploaded-content""#),
        "{uploaded}"
    );

    let first_session = request(pam.port, "GET", "/session", "", &[]).unwrap();
    assert!(first_session.contains(r#"{"count":1}"#), "{first_session}");
    let session_cookie = response_cookies(&first_session)
        .into_iter()
        .find(|cookie| cookie.to_ascii_lowercase().contains("_session="))
        .expect("Laravel should issue a session cookie");
    let second_session = request(
        pam.port,
        "GET",
        "/session",
        "",
        &[("cookie", &session_cookie)],
    )
    .unwrap();
    assert!(
        second_session.contains(r#"{"count":2}"#),
        "{second_session}"
    );
    let isolated_session = request(pam.port, "GET", "/session", "", &[]).unwrap();
    assert!(
        isolated_session.contains(r#"{"count":1}"#),
        "{isolated_session}"
    );

    let csrf = request(pam.port, "GET", "/csrf", "", &[("cookie", &session_cookie)]).unwrap();
    let token = serde_json::from_str::<serde_json::Value>(response_body(&csrf)).unwrap()["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let accepted = request(
        pam.port,
        "POST",
        "/csrf",
        "",
        &[("cookie", &session_cookie), ("x-csrf-token", &token)],
    )
    .unwrap();
    assert!(accepted.starts_with("HTTP/1.1 200"), "{accepted}");
    assert!(accepted.contains(r#"{"accepted":true}"#), "{accepted}");
    let rejected = request(pam.port, "POST", "/csrf", "", &[]).unwrap();
    assert!(rejected.starts_with("HTTP/1.1 419"), "{rejected}");

    let cookie = request(pam.port, "GET", "/cookie", "", &[]).unwrap();
    assert!(cookie.starts_with("HTTP/1.1 200"), "{cookie}");
    assert!(
        response_cookies(&cookie)
            .iter()
            .any(|value| value.starts_with("pam_laravel=")),
        "{cookie}"
    );

    pam.stop();
}

#[test]
fn records_real_requests_with_telescope_and_pulse_enabled() {
    let mut pam = LaravelProcess::start_with_observers();

    for request_number in 0..8 {
        let value = format!("observer-{request_number}");
        let cached = request(pam.port, "PUT", &format!("/api/cache/{value}"), "", &[]).unwrap();
        assert!(cached.starts_with("HTTP/1.1 200"), "{cached}");

        let database =
            request(pam.port, "POST", &format!("/api/database/{value}"), "", &[]).unwrap();
        assert!(database.starts_with("HTTP/1.1 200"), "{database}");
    }

    pam.stop();
    let database = pam
        .database
        .as_ref()
        .expect("observer contract should use a persistent SQLite database")
        .to_string_lossy();
    let observed = run_artisan(&laravel_root(), &database, &["pam:observer-counts"]);
    assert!(
        observed.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&observed.stdout),
        String::from_utf8_lossy(&observed.stderr),
    );
    let counts: serde_json::Value = serde_json::from_slice(observed.stdout.trim_ascii()).unwrap();
    assert!(
        counts["telescope"].as_u64().is_some_and(|count| count > 0),
        "{}",
        String::from_utf8_lossy(&observed.stdout),
    );
    assert!(
        counts["pulse"].as_u64().is_some_and(|count| count > 0),
        "{}",
        String::from_utf8_lossy(&observed.stdout),
    );
}

#[test]
fn serves_laravel_with_isolated_requests_and_bounded_memory() {
    let mut pam = LaravelProcess::start();

    let response = request(
        pam.port,
        "POST",
        "/api/echo",
        r#"{"value":"works"}"#,
        &[("content-type", "application/json"), ("x-pam-test", "yes")],
    )
    .unwrap();
    assert!(response.contains(r#"{"value":"works","header":"yes"}"#));

    let state = request(pam.port, "GET", "/api/state/secret", "", &[]).unwrap();
    assert!(state.contains(r#"{"value":"secret"}"#));
    let next = request(pam.port, "GET", "/api/state", "", &[]).unwrap();
    assert!(next.contains(r#"{"leaked":false}"#), "{next}");

    let port = pam.port;
    let suspended = thread::spawn(move || request(port, "GET", "/api/hold/isolated", "", &[]));
    thread::sleep(Duration::from_millis(75));
    let rejected = request(pam.port, "GET", "/api/ping", "", &[]).unwrap();
    assert!(
        rejected.starts_with("HTTP/1.1 503"),
        "Laravel must reject interleaving inside a worker: {rejected}"
    );
    let suspended = suspended.join().unwrap().unwrap();
    assert!(suspended.contains(r#"{"value":"isolated"}"#), "{suspended}");
    let next = request(pam.port, "GET", "/api/state", "", &[]).unwrap();
    assert!(next.contains(r#"{"leaked":false}"#), "{next}");

    let token_response = request(
        pam.port,
        "POST",
        "/api/sanctum/token",
        "",
        &[("accept", "application/json")],
    )
    .unwrap();
    let token = serde_json::from_str::<serde_json::Value>(response_body(&token_response)).unwrap()
        ["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let authorization = format!("Bearer {token}");
    let session = request(pam.port, "GET", "/session", "", &[]).unwrap();
    let session_cookie = response_cookies(&session)
        .into_iter()
        .find(|cookie| cookie.to_ascii_lowercase().contains("_session="))
        .unwrap();

    for request_number in 0..256 {
        let response =
            package_lifecycle_request(pam.port, request_number, &authorization, &session_cookie);
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    }
    let baseline = resident_bytes(pam.child.id());
    let mut high_water = baseline;
    for request_number in 1..=2_000 {
        let response =
            package_lifecycle_request(pam.port, request_number, &authorization, &session_cookie);
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        if request_number % 4 == 1 {
            assert!(response.contains("livewire-compatible:0"), "{response}");
        }
        if request_number % 250 == 0 {
            high_water = high_water.max(resident_bytes(pam.child.id()));
        }
    }
    let final_rss = resident_bytes(pam.child.id());
    let allowed_growth = 24 * 1024 * 1024;
    eprintln!(
        "Laravel RSS: baseline={} MiB, high={} MiB, final={} MiB",
        baseline / 1024 / 1024,
        high_water / 1024 / 1024,
        final_rss / 1024 / 1024,
    );
    assert!(
        final_rss.saturating_sub(baseline) <= allowed_growth,
        "Laravel RSS grew from {baseline} to {final_rss} bytes (high water {high_water})"
    );

    pam.stop();
}

#[test]
fn streams_laravel_responses_progressively_and_preserves_binary_ranges() {
    let mut pam = LaravelProcess::start();

    let mut stream = TcpStream::connect(("127.0.0.1", pam.port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(175)))
        .unwrap();
    stream
        .write_all(b"GET /api/stream HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .unwrap();
    let started = Instant::now();
    let mut progressive = Vec::new();
    let mut buffer = [0_u8; 8 * 1024];
    while !progressive
        .windows(b"pam-stream-first|".len())
        .any(|window| window == b"pam-stream-first|")
    {
        let read = stream.read(&mut buffer).unwrap();
        assert!(read > 0, "stream ended before its first chunk");
        progressive.extend_from_slice(&buffer[..read]);
    }
    assert!(
        started.elapsed() < Duration::from_millis(225),
        "Laravel buffered the streamed response instead of flushing its first chunk"
    );
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream.read_to_end(&mut progressive).unwrap();
    assert!(
        progressive
            .windows(b"|pam-stream-last".len())
            .any(|window| window == b"|pam-stream-last"),
        "stream did not contain its final chunk"
    );
    assert!(progressive.len() >= 128 * 1024);

    let ranged = request(
        pam.port,
        "GET",
        "/api/download",
        "",
        &[("range", "bytes=5-9")],
    )
    .unwrap();
    assert!(ranged.starts_with("HTTP/1.1 206"), "{ranged}");
    assert!(
        ranged
            .to_ascii_lowercase()
            .contains("content-range: bytes 5-9/37"),
        "{ranged}"
    );
    assert_eq!(response_body(&ranged).as_bytes(), b"56789");

    let head = request(pam.port, "HEAD", "/api/download", "", &[]).unwrap();
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert!(
        head.to_ascii_lowercase().contains("content-length: 37"),
        "{head}"
    );
    assert_eq!(response_body(&head), "");

    pam.stop();
}

#[test]
fn cancels_disconnected_streams_and_enforces_response_limits() {
    let mut pam = LaravelProcess::start();
    let mut stream = TcpStream::connect(("127.0.0.1", pam.port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .write_all(
            b"GET /api/stream-unbounded HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
        )
        .unwrap();
    let mut first = [0_u8; 8 * 1024];
    assert!(stream.read(&mut first).unwrap() > 0);
    drop(stream);

    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if request(pam.port, "GET", "/api/ping", "", &[])
            .is_ok_and(|response| response.starts_with("HTTP/1.1 200"))
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "a disconnected client retained the Laravel request slot"
        );
        thread::sleep(Duration::from_millis(20));
    }
    pam.stop();

    let mut limited = LaravelProcess::start_with_response_limit(Some(128 * 1024));
    let oversized = request(limited.port, "GET", "/api/oversized", "", &[]).unwrap();
    assert!(oversized.starts_with("HTTP/1.1 500"), "{oversized}");
    assert!(
        !oversized.contains(&"x".repeat(1024)),
        "oversized application data leaked into the error response"
    );
    let healthy = request(limited.port, "GET", "/api/ping", "", &[]).unwrap();
    assert!(healthy.starts_with("HTTP/1.1 200"), "{healthy}");
    limited.stop();
}

fn request(
    port: u16,
    method: &str,
    path: &str,
    body: &str,
    headers: &[(&str, &str)],
) -> std::io::Result<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    )?;
    for (name, value) in headers {
        write!(stream, "{name}: {value}\r\n")?;
    }
    write!(stream, "\r\n{body}")?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}

fn response_body(response: &str) -> &str {
    response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .expect("HTTP response must contain a header boundary")
}

fn response_cookies(response: &str) -> Vec<String> {
    response
        .lines()
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("set-cookie").then(|| {
                value
                    .trim()
                    .split(';')
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            })
        })
        .collect()
}

fn laravel_root() -> PathBuf {
    std::env::var_os("PAM_LARAVEL_COMPAT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("compat/laravel-smoke"))
}

fn package_lifecycle_request(
    port: u16,
    request_number: usize,
    authorization: &str,
    session_cookie: &str,
) -> String {
    let result = match request_number % 4 {
        0 => request(port, "GET", "/api/ping", "", &[]),
        1 => request(
            port,
            "GET",
            "/livewire-contract",
            "",
            &[("cookie", session_cookie)],
        ),
        2 => request(
            port,
            "GET",
            "/inertia-contract",
            "",
            &[("cookie", session_cookie)],
        ),
        _ => request(
            port,
            "GET",
            "/api/sanctum/user",
            "",
            &[
                ("accept", "application/json"),
                ("authorization", authorization),
            ],
        ),
    };

    result.unwrap()
}

fn run_artisan(root: &PathBuf, database: &str, arguments: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pam"));
    command
        .arg("artisan")
        .args(arguments)
        .current_dir(root)
        .stdin(Stdio::null());
    configure_laravel_environment(&mut command, database, false);
    command.output().expect("Artisan should run through Pam")
}

fn configure_laravel_environment(
    command: &mut Command,
    database: impl AsRef<std::ffi::OsStr>,
    observers: bool,
) {
    command
        .env("APP_NAME", "Pam Laravel Compatibility")
        .env("APP_ENV", "testing")
        .env("APP_DEBUG", "false")
        .env("APP_KEY", APP_KEY)
        .env("APP_URL", "http://127.0.0.1")
        .env("LOG_CHANNEL", "stderr")
        .env("CACHE_STORE", "array")
        .env("SESSION_DRIVER", "array")
        .env("DB_CONNECTION", "sqlite")
        .env("DB_DATABASE", database)
        .env("QUEUE_CONNECTION", "database")
        .env("FILESYSTEM_DISK", "local")
        .env("SCOUT_DRIVER", "collection")
        .env(
            "TELESCOPE_ENABLED",
            if observers { "true" } else { "false" },
        )
        .env("PULSE_ENABLED", if observers { "true" } else { "false" });
}

fn resident_bytes(process_id: u32) -> u64 {
    let status = fs::read_to_string(format!("/proc/{process_id}/status")).unwrap();
    let kibibytes = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse::<u64>().ok())
        .expect("VmRSS should be present");
    kibibytes * 1024
}
