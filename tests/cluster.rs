use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tungstenite::{Message, connect};

const SIGHUP: i32 = 1;
const SIGKILL: i32 = 9;
const SIGTERM: i32 = 15;

unsafe extern "C" {
    fn kill(process_id: i32, signal: i32) -> i32;
}

#[test]
fn routes_requests_to_isolated_specialized_pools() {
    let probe = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let admin_probe = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let admin_port = admin_probe.local_addr().unwrap().port();
    drop(admin_probe);
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/server.php");
    let child = Command::new(env!("CARGO_BIN_EXE_pam"))
        .args([
            "start",
            script.to_str().unwrap(),
            "--ingress-address",
            &format!("127.0.0.1:{port}"),
            "--pool",
            "api=1@/api,/a/very-long-prefix",
            "--pool",
            "admin=1@/api/admin",
            "--pool",
            "web=1@*",
            "--startup-timeout",
            "3000",
            "--admin-address",
            &format!("127.0.0.1:{admin_port}"),
        ])
        .env("PAM_TEST_PORT", "3000")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut cluster = ClusterProcess {
        child,
        port,
        admin_port,
    };
    cluster.wait_for_workers(3);
    let api = cluster.request_path("/api/pool").unwrap();
    let web = cluster.request_path("/pool").unwrap();
    assert!(api.contains(r#""pool":"api""#), "{api}");
    assert!(web.contains(r#""pool":"web""#), "{web}");
    let admin = cluster.request_path("/api/admin/pool").unwrap();
    assert!(admin.contains(r#""pool":"admin""#), "{admin}");

    let ready = cluster.admin_request("/ready");
    let ready_body = ready.split_once("\r\n\r\n").unwrap().1;
    let snapshot: serde_json::Value = serde_json::from_str(ready_body).unwrap();
    let api_pid = snapshot["workers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|worker| worker["pool"] == "api")
        .unwrap()["pid"]
        .as_u64()
        .unwrap() as i32;
    // A crash in the API heap must not interrupt the independent web pool.
    assert_eq!(unsafe { kill(api_pid, SIGKILL) }, 0);
    assert!(
        cluster
            .request_path("/pool")
            .unwrap()
            .contains(r#""pool":"web""#)
    );
    cluster.wait_for_path("/api/pool", r#""pool":"api""#, Duration::from_secs(5));

    let metrics_deadline = Instant::now() + Duration::from_secs(2);
    let metrics = loop {
        let metrics = cluster.admin_request("/metrics");
        if metrics.contains("pam_pool_http_requests_total{pool=\"api\"}") {
            break metrics;
        }
        assert!(
            Instant::now() < metrics_deadline,
            "pool metrics were not published: {metrics}"
        );
        thread::sleep(Duration::from_millis(20));
    };
    assert!(
        metrics.contains("pam_pool_workers{pool=\"api\"} 1"),
        "{metrics}"
    );
    assert!(
        metrics.contains("pam_pool_workers{pool=\"web\"} 1"),
        "{metrics}"
    );

    cluster.signal(SIGHUP);
    let reload_deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < reload_deadline {
        let response = cluster.admin_request("/ready");
        if response.contains(r#""generation":2"#) && response.starts_with("HTTP/1.1 200") {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }
    cluster.wait_for_path("/api/pool", r#""pool":"api""#, Duration::from_secs(5));
    cluster.wait_for_path("/pool", r#""pool":"web""#, Duration::from_secs(5));
    let (mut socket, upgrade) = connect(format!("ws://127.0.0.1:{port}/ws")).unwrap();
    assert_eq!(upgrade.status().as_u16(), 101);
    let welcome = socket.read().unwrap().into_text().unwrap();
    assert!(welcome.contains("welcome"), "{welcome}");
    socket.send(Message::Close(None)).unwrap();
}

struct ClusterProcess {
    child: Child,
    port: u16,
    admin_port: u16,
}

impl ClusterProcess {
    fn start(max_requests: u64) -> Self {
        Self::start_with(max_requests, 2, None, None)
    }

    fn start_with_cache() -> Self {
        let mut cluster = Self::start_with(1_000, 2, None, None);
        cluster.stop();

        let probe = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        let admin_probe = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let admin_port = admin_probe.local_addr().unwrap().port();
        drop(admin_probe);
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/server.php");
        let child = Command::new(env!("CARGO_BIN_EXE_pam"))
            .args([
                "start",
                script.to_str().unwrap(),
                "--workers",
                "2",
                "--max-requests",
                "1000",
                "--startup-timeout",
                "10000",
                "--admin-address",
                &format!("127.0.0.1:{admin_port}"),
            ])
            .env("PAM_TEST_PORT", port.to_string())
            .env("PAM_TEST_RESPONSE_CACHE", "1")
            .env("PAM_TEST_CACHE_TTL", "30000")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        cluster = Self {
            child,
            port,
            admin_port,
        };
        cluster.wait_for_workers(2);
        cluster
    }

    fn start_with(
        max_requests: u64,
        workers: usize,
        request_timeout_ms: Option<u64>,
        failed_generation: Option<u64>,
    ) -> Self {
        Self::start_with_env(
            max_requests,
            workers,
            request_timeout_ms,
            failed_generation,
            &[],
        )
    }

    fn start_with_env(
        max_requests: u64,
        workers: usize,
        request_timeout_ms: Option<u64>,
        failed_generation: Option<u64>,
        environment: &[(&str, &str)],
    ) -> Self {
        let probe = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        let admin_probe = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let admin_port = admin_probe.local_addr().unwrap().port();
        drop(admin_probe);
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/server.php");
        let mut command = Command::new(env!("CARGO_BIN_EXE_pam"));
        command
            .args([
                "start",
                script.to_str().unwrap(),
                "--workers",
                &workers.to_string(),
                "--max-requests",
                &max_requests.to_string(),
                "--graceful-timeout",
                "1000",
                "--startup-timeout",
                "10000",
                "--restart-backoff",
                "10",
                "--watchdog-grace",
                "50",
                "--admin-address",
                &format!("127.0.0.1:{admin_port}"),
            ])
            .env("PAM_TEST_PORT", port.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(timeout) = request_timeout_ms {
            command.env("PAM_TEST_REQUEST_TIMEOUT", timeout.to_string());
        }
        if let Some(generation) = failed_generation {
            command.env("PAM_TEST_FAIL_GENERATION", generation.to_string());
        }
        if let Some((_, path)) = environment
            .iter()
            .find(|(name, _)| *name == "PAM_TEST_STDERR_FILE")
        {
            command.stderr(fs::File::create(path).unwrap());
        }
        command.envs(environment.iter().copied());
        let child = command.spawn().unwrap();
        let mut cluster = Self {
            child,
            port,
            admin_port,
        };
        cluster.wait_for_workers(workers);
        cluster
    }

    fn wait_for_workers(&mut self, workers: usize) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            if self.children().len() == workers
                && self.request().is_ok()
                && self.admin_request("/ready").starts_with("HTTP/1.1 200 OK")
            {
                return;
            }
            if let Some(status) = self.child.try_wait().unwrap() {
                panic!("Pam cluster exited before becoming ready: {status}");
            }
            thread::sleep(Duration::from_millis(25));
        }
        panic!("Pam cluster did not become ready");
    }

    fn wait_for_path(&mut self, path: &str, expected: &str, timeout: Duration) -> String {
        let deadline = Instant::now() + timeout;
        let mut last_response = String::new();
        while Instant::now() < deadline {
            match self.request_path(path) {
                Ok(response) if response.contains(expected) => return response,
                Ok(response) | Err(response) => last_response = response,
            }
            if let Some(status) = self.child.try_wait().unwrap() {
                panic!("Pam cluster exited while waiting for {path}: {status}");
            }
            thread::sleep(Duration::from_millis(25));
        }
        panic!("Pam cluster did not serve {path} with {expected}: {last_response}");
    }

    fn request(&self) -> Result<String, String> {
        self.request_path("/ping?query=cluster")
    }

    fn request_path(&self, path: &str) -> Result<String, String> {
        let mut stream =
            TcpStream::connect(("127.0.0.1", self.port)).map_err(|error| error.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(|error| error.to_string())?;
        stream
            .write_all(
                format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .map_err(|error| error.to_string())?;
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .map_err(|error| error.to_string())?;
        if !response.starts_with("HTTP/1.1 200 OK") {
            return Err(response);
        }
        Ok(response)
    }

    fn raw_request(&self, request: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    fn admin_request(&self, path: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", self.admin_port)).unwrap();
        stream
            .write_all(
                format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    fn request_with_retry(&self) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Ok(response) = self.request() {
                return response;
            }
            thread::sleep(Duration::from_millis(20));
        }
        panic!("cluster did not return a successful response");
    }

    fn children(&self) -> HashSet<u32> {
        // Multi-worker clusters fork their workers from a prefork zygote, so
        // workers are the zygote's children rather than the master's.
        let direct = |pid: u32| {
            fs::read_to_string(format!("/proc/{pid}/task/{pid}/children"))
                .unwrap_or_default()
                .split_whitespace()
                .filter_map(|value| value.parse::<u32>().ok())
                .collect::<Vec<_>>()
        };
        let mut workers = HashSet::new();
        for pid in direct(self.child.id()) {
            let command = fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
            if command
                .split(|byte| *byte == 0)
                .any(|argument| argument == b"__zygote")
            {
                workers.extend(direct(pid));
            } else {
                workers.insert(pid);
            }
        }
        workers
    }

    fn signal(&self, signal: i32) {
        // SAFETY: The PID belongs to the cluster supervisor created by this test.
        let result = unsafe { kill(self.child.id() as i32, signal) };
        assert_eq!(result, 0);
    }

    fn stop(&mut self) {
        if self.child.try_wait().unwrap().is_some() {
            return;
        }
        self.signal(SIGTERM);
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if self.child.try_wait().unwrap().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(25));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        panic!("Pam cluster did not stop gracefully");
    }
}

impl Drop for ClusterProcess {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = unsafe { kill(self.child.id() as i32, SIGTERM) };
            let deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < deadline {
                if self.child.try_wait().ok().flatten().is_some() {
                    return;
                }
                thread::sleep(Duration::from_millis(20));
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
fn kills_workers_when_the_master_is_killed() {
    let mut cluster = ClusterProcess::start_with(1_000, 2, None, None);
    let workers = cluster.children();
    assert_eq!(workers.len(), 2);
    cluster.signal(SIGKILL);
    cluster.child.wait().unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && workers.iter().any(|pid| linux_process_is_running(*pid)) {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        workers.iter().all(|pid| !linux_process_is_running(*pid)),
        "workers survived their killed PAM master: {workers:?}"
    );
}

#[cfg(target_os = "linux")]
fn linux_process_is_running(pid: u32) -> bool {
    let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    let Some(after_name) = stat.rfind(')').and_then(|end| stat.get(end + 2..)) else {
        return false;
    };

    after_name
        .split_whitespace()
        .next()
        .is_some_and(|state| state != "Z")
}

fn request_identity(response: &str) -> (String, String) {
    let request_id = response
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with("x-request-id:"))
        .and_then(|line| line.split_once(':'))
        .map(|(_, value)| value.trim())
        .expect("cluster response should contain x-request-id");
    let mut parts = request_id.split('-');
    let worker = parts.next().unwrap().to_owned();
    let process = parts.next().unwrap().to_owned();
    (worker, process)
}

fn response_calls(response: &str) -> u64 {
    let body = response.split_once("\r\n\r\n").unwrap().1;
    serde_json::from_str::<serde_json::Value>(body).unwrap()["calls"]
        .as_u64()
        .unwrap()
}

#[test]
fn propagates_tagged_cache_purges_to_every_worker() {
    let mut cluster = ClusterProcess::start_with_cache();
    let mut warmed = HashSet::new();
    for _ in 0..40 {
        let response = cluster.request_path("/cached").unwrap();
        warmed.insert(request_identity(&response).0);
        assert_eq!(response_calls(&response), 1, "{response}");
        if warmed.len() == 2 {
            break;
        }
    }
    assert_eq!(warmed.len(), 2, "both worker caches must be warmed");

    let purge = cluster.raw_request(
        "POST /__pam/cache/purge HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer pam-test-cache-purge-secret-32-bytes\r\nContent-Type: application/json\r\nContent-Length: 17\r\nConnection: close\r\n\r\n{\"tag\":\"catalog\"}",
    );
    assert!(purge.starts_with("HTTP/1.1 200"), "{purge}");
    thread::sleep(Duration::from_millis(150));

    let mut refreshed = HashSet::new();
    for _ in 0..40 {
        let response = cluster.request_path("/cached").unwrap();
        refreshed.insert(request_identity(&response).0);
        assert_eq!(response_calls(&response), 2, "{response}");
        if refreshed.len() == 2 {
            break;
        }
    }
    assert_eq!(refreshed.len(), 2, "purge must reach both workers");
    cluster.stop();
}

/// Reads one keep-alive HTTP response without waiting for the connection to end.
fn read_keep_alive_response(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let read = stream.read(&mut chunk).unwrap();
        assert!(read > 0, "connection closed before a full response");
        bytes.extend_from_slice(&chunk[..read]);
        let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let head = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
        let length = head
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .and_then(|value| value.trim().parse::<usize>().ok())
            .expect("response should declare its content length");
        if bytes.len() >= end + 4 + length {
            return String::from_utf8_lossy(&bytes).into_owned();
        }
    }
}

#[test]
fn balances_keep_alive_connections_between_workers() {
    let workers = 4;
    let per_worker = 10;
    let mut cluster = ClusterProcess::start_with(1_000_000, workers, None, None);
    let mut connections = (0..workers * per_worker)
        .map(|_| {
            let stream = TcpStream::connect(("127.0.0.1", cluster.port)).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            stream
        })
        .collect::<Vec<_>>();

    let mut owners = Vec::new();
    for round in 0..2 {
        for (index, stream) in connections.iter_mut().enumerate() {
            stream
                .write_all(b"GET /ping?query=balance HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
                .unwrap();
            let response = read_keep_alive_response(stream);
            assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
            let (worker, _) = request_identity(&response);
            if round == 0 {
                owners.push(worker);
            } else {
                // A connection, moved or not, stays with the worker serving it.
                assert_eq!(owners[index], worker);
            }
        }
    }

    let mut counts = std::collections::HashMap::<&str, usize>::new();
    for owner in &owners {
        *counts.entry(owner).or_default() += 1;
    }
    let most = counts.values().copied().max().unwrap();
    let least = counts.values().copied().min().unwrap();
    assert_eq!(counts.len(), workers, "{counts:?}");
    assert!(most - least <= 4, "unbalanced connections: {counts:?}");

    drop(connections);
    cluster.stop();
}

#[test]
fn recycles_workers_after_the_request_limit() {
    let mut cluster = ClusterProcess::start(3);
    let mut workers = HashSet::new();
    let mut processes = HashSet::new();
    // A recycling worker quiesces its SO_REUSEPORT listener briefly (and keeps
    // serving when no sibling can take its connections), so recycling is
    // observed over time rather than at an exact request count.
    let deadline = Instant::now() + Duration::from_secs(5);
    while processes.len() < 3 && Instant::now() < deadline {
        let (worker, process) = request_identity(&cluster.request_with_retry());
        workers.insert(worker);
        processes.insert(process);
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(workers, HashSet::from(["1".to_owned(), "2".to_owned()]));
    assert!(
        processes.len() >= 3,
        "workers were not recycled: {processes:?}"
    );
    cluster.stop();
}

#[test]
fn recycles_workers_make_before_break_without_dropping_requests() {
    // Tiny limits recycle each worker many times while clients keep sending
    // requests without retrying: every request must still succeed.
    let cluster = std::sync::Arc::new(ClusterProcess::start_with_env(
        40,
        2,
        None,
        None,
        &[("PAM_TEST_RATE_LIMIT", "0")],
    ));
    // Clients keep going until several replacements served (at least 600
    // requests): a fast machine can finish a fixed number of requests before
    // the first replacement is even ready.
    let seen = std::sync::Arc::new(std::sync::Mutex::new(HashSet::new()));
    let deadline = Instant::now() + Duration::from_secs(20);
    let clients: Vec<_> = (0..4)
        .map(|_| {
            let cluster = cluster.clone();
            let seen = seen.clone();
            thread::spawn(move || {
                let mut failures = Vec::new();
                let mut processes = HashSet::new();
                let mut sent = 0;
                while sent < 150 || (seen.lock().unwrap().len() < 4 && Instant::now() < deadline) {
                    sent += 1;
                    match cluster.request() {
                        Ok(response) => {
                            let process = request_identity(&response).1;
                            seen.lock().unwrap().insert(process.clone());
                            processes.insert(process);
                        }
                        Err(error) => {
                            failures.push(error.lines().next().unwrap_or_default().to_owned())
                        }
                    }
                }
                (failures, processes)
            })
        })
        .collect();
    let mut failures = Vec::new();
    let mut processes = HashSet::new();
    for client in clients {
        let (client_failures, client_processes) = client.join().unwrap();
        failures.extend(client_failures);
        processes.extend(client_processes);
    }
    assert!(failures.is_empty(), "dropped requests: {failures:?}");
    assert!(
        processes.len() > 2,
        "workers were not recycled: {processes:?}"
    );
    let mut cluster = std::sync::Arc::into_inner(cluster).unwrap();
    cluster.stop();
}

#[test]
fn recycles_a_worker_whose_engine_hit_a_fatal_error() {
    let mut cluster = ClusterProcess::start(1_000);
    let before = cluster.children();
    assert_eq!(before.len(), 2);
    // The fatal request itself fails, but the poisoned worker must be
    // replaced instead of answering every later request with a 500.
    let _ = cluster.request_path("/engine-fatal");

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let children = cluster.children();
        if children.len() == 2 && children != before {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }
    let after = cluster.children();
    assert_eq!(after.len(), 2);
    assert_ne!(after, before, "the failed worker was not recycled");
    for _ in 0..20 {
        let response = cluster.request_with_retry();
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    }
    cluster.stop();
}

#[test]
fn recovers_crashed_workers_and_keeps_serving_during_reload() {
    let mut cluster = ClusterProcess::start(1_000);
    let initial = cluster.children();
    assert_eq!(initial.len(), 2);
    let crashed = *initial.iter().next().unwrap();
    // SAFETY: This PID is a worker owned by the test cluster.
    assert_eq!(unsafe { kill(crashed as i32, SIGKILL) }, 0);

    let recovery_deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < recovery_deadline {
        let children = cluster.children();
        if children.len() == 2 && !children.contains(&crashed) {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }
    let recovered = cluster.children();
    assert_eq!(recovered.len(), 2);
    assert!(!recovered.contains(&crashed));
    cluster.request_with_retry();

    cluster.signal(SIGHUP);
    let reload_deadline = Instant::now() + Duration::from_millis(1_200);
    while Instant::now() < reload_deadline {
        let response = cluster.request_with_retry();
        assert!(response.contains(r#""query":"cluster""#));
        thread::sleep(Duration::from_millis(20));
    }
    let reloaded = cluster.children();
    assert_eq!(reloaded.len(), 2);
    assert!(reloaded.is_disjoint(&recovered));
    cluster.stop();
}

#[test]
fn exposes_independent_health_and_aggregate_metrics() {
    let mut cluster = ClusterProcess::start(1_000);
    cluster.request_with_retry();

    for path in ["/live", "/startup", "/ready"] {
        let response = cluster.admin_request(path);
        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert!(response.contains(r#""healthy":true"#), "{response}");
        assert!(response.contains(r#""readyWorkers":2"#), "{response}");
    }
    let metrics_deadline = Instant::now() + Duration::from_secs(1);
    let metrics = loop {
        let metrics = cluster.admin_request("/metrics");
        if metrics.contains("pam_http_active_requests 0") {
            break metrics;
        }
        assert!(
            Instant::now() < metrics_deadline,
            "cluster active-request gauge remained stale: {metrics}"
        );
        thread::sleep(Duration::from_millis(20));
    };
    assert!(metrics.starts_with("HTTP/1.1 200 OK"), "{metrics}");
    assert!(metrics.contains("pam_cluster_ready 1"), "{metrics}");
    assert!(metrics.contains("pam_cluster_workers 2"), "{metrics}");
    assert!(metrics.contains("pam_http_requests_total"), "{metrics}");
    assert!(metrics.contains("pam_http_errors_total 0"), "{metrics}");
    assert!(
        metrics.contains("# TYPE pam_http_request_duration_seconds histogram"),
        "{metrics}"
    );
    assert!(
        metrics.contains("pam_http_request_duration_seconds_bucket{le=\"+Inf\"}"),
        "{metrics}"
    );
    let baseline_requests = metric_value(&metrics, "pam_http_requests_total");
    for _ in 0..10 {
        cluster.request_with_retry();
    }
    let flush_deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let flushed = cluster.admin_request("/metrics");
        if metric_value(&flushed, "pam_http_requests_total") >= baseline_requests + 10
            && metric_value(&flushed, "pam_http_active_requests") == 0
        {
            break;
        }
        assert!(
            Instant::now() < flush_deadline,
            "cluster metrics did not flush after traffic stopped: {flushed}"
        );
        thread::sleep(Duration::from_millis(20));
    }
    let top = Command::new(env!("CARGO_BIN_EXE_pam"))
        .args([
            "top",
            &format!("http://127.0.0.1:{}", cluster.admin_port),
            "--iterations",
            "1",
            "--interval-ms",
            "1",
        ])
        .output()
        .unwrap();
    assert!(
        top.status.success(),
        "{}",
        String::from_utf8_lossy(&top.stderr)
    );
    assert!(
        String::from_utf8_lossy(&top.stdout).contains("pam_http_requests_total"),
        "{}",
        String::from_utf8_lossy(&top.stdout),
    );
    cluster.stop();
}

fn metric_value(response: &str, name: &str) -> u64 {
    response
        .lines()
        .find_map(|line| {
            line.strip_prefix(name)?
                .strip_prefix(' ')?
                .parse::<u64>()
                .ok()
        })
        .unwrap_or(0)
}

#[test]
fn watchdog_replaces_a_worker_stuck_inside_php() {
    let mut cluster = ClusterProcess::start_with(1_000, 1, Some(100), None);
    let original = cluster.children();
    assert_eq!(original.len(), 1);

    // The transport answers the blocked request itself (the PHP thread is
    // stuck), closes the connection and recycles the worker.
    let blocked =
        cluster.raw_request("GET /block HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    assert!(blocked.starts_with("HTTP/1.1 504"), "{blocked}");
    assert!(
        blocked.to_ascii_lowercase().contains("connection: close"),
        "{blocked}"
    );

    // Shared CI runners (especially ARM) can take seconds to boot a replacement.
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        let current = cluster.children();
        if current.len() == 1 && current.is_disjoint(&original) && cluster.request().is_ok() {
            cluster.stop();
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!("watchdog did not replace the stuck worker");
}

#[test]
fn failed_reload_keeps_the_healthy_generation() {
    let mut cluster = ClusterProcess::start_with(1_000, 2, None, Some(2));
    let original = cluster.children();
    cluster.signal(SIGHUP);

    let deadline = Instant::now() + Duration::from_millis(1_000);
    while Instant::now() < deadline {
        cluster
            .request()
            .expect("failed reload interrupted traffic");
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(cluster.children(), original);
    let ready = cluster.admin_request("/ready");
    assert!(ready.starts_with("HTTP/1.1 200 OK"), "{ready}");
    assert!(ready.contains(r#""generation":1"#), "{ready}");
    cluster.stop();
}

#[test]
fn a_raw_status_line_fails_neither_the_request_nor_the_next_one() {
    // Symfony's Response::send() sends `HTTP/1.1 200 OK` with header(); the
    // end-of-request reset must not warn about it (PHP 8.5 does for
    // http_response_code()), or an error handler turns it into a 500.
    let mut cluster =
        ClusterProcess::start_with_env(1_000, 1, None, None, &[("PAM_TEST_STRICT_ERRORS", "1")]);
    let original = cluster.children();
    for _ in 0..3 {
        let sent = cluster.raw_request(
            "GET /status-line HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
        );
        assert!(sent.starts_with("HTTP/1.1 200"), "{sent}");
        assert!(sent.ends_with("status-line"), "{sent}");
        let created = cluster.raw_request(
            "GET /response-code HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
        );
        assert!(created.starts_with("HTTP/1.1 201"), "{created}");
        assert!(created.ends_with("created"), "{created}");
    }
    assert_eq!(cluster.children(), original, "the worker was recycled");
    cluster.stop();
}

fn upload_request(path: &str, bytes: usize) -> String {
    let mut body = String::from(
        "--pam\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.bin\"\r\n\
         Content-Type: application/octet-stream\r\n\r\n",
    );
    body.push_str(&"a".repeat(bytes));
    body.push_str("\r\n--pam--\r\n");
    format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\
         Content-Type: multipart/form-data; boundary=pam\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
}

fn wait_for_files(directory: &std::path::Path, expected: usize, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let files: Vec<_> = fs::read_dir(directory)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name())
            .collect();
        if files.len() == expected {
            return;
        }
        assert!(Instant::now() < deadline, "{what}: {files:?}");
        thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn deletes_the_uploads_of_requests_cut_by_a_deadline_or_a_killed_worker() {
    let uploads = std::env::temp_dir().join(format!("pam-upload-test-{}", std::process::id()));
    fs::create_dir_all(&uploads).unwrap();
    let ini = format!("upload_tmp_dir={}\n", uploads.display());

    // The PHP thread is blocked past the deadline: 504, then the worker exits
    // without the request ever ending.
    let mut cluster =
        ClusterProcess::start_with_env(1_000, 1, Some(300), None, &[("PAM_INI_ENTRIES", &ini)]);
    let timed_out = cluster.raw_request(&upload_request("/upload-block", 512 * 1024));
    assert!(timed_out.starts_with("HTTP/1.1 504"), "{timed_out}");
    wait_for_files(&uploads, 0, "a timed-out request left its upload");
    cluster.stop();

    // The worker is killed while PHP holds the upload.
    let mut cluster =
        ClusterProcess::start_with_env(1_000, 1, None, None, &[("PAM_INI_ENTRIES", &ini)]);
    let worker = *cluster.children().iter().next().unwrap();
    let port = cluster.port;
    let client = thread::spawn(move || {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let _ = stream.write_all(upload_request("/upload-block", 512 * 1024).as_bytes());
        let mut response = Vec::new();
        let _ = stream.read_to_end(&mut response);
    });
    wait_for_files(&uploads, 1, "the upload was not streamed to disk");
    // SAFETY: signals a worker process of this test's cluster.
    unsafe {
        kill(worker as i32, SIGKILL);
    }
    wait_for_files(&uploads, 0, "a killed worker left its upload");
    client.join().unwrap();
    cluster.stop();
    fs::remove_dir(&uploads).unwrap();
}

/// Inodes of the TCP sockets listening on `port`.
#[cfg(target_os = "linux")]
fn listening_inodes(port: u16) -> HashSet<u64> {
    let mut inodes = HashSet::new();
    for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
        for line in fs::read_to_string(table)
            .unwrap_or_default()
            .lines()
            .skip(1)
        {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            let local_port = fields[1].rsplit(':').next().unwrap();
            if fields[3] == "0A" && u16::from_str_radix(local_port, 16) == Ok(port) {
                inodes.insert(fields[9].parse().unwrap());
            }
        }
    }
    inodes
}

/// Socket inodes a process holds open.
#[cfg(target_os = "linux")]
fn socket_inodes(pid: u32) -> HashSet<u64> {
    fs::read_dir(format!("/proc/{pid}/fd"))
        .map(|entries| {
            entries
                .filter_map(|entry| fs::read_link(entry.ok()?.path()).ok())
                .filter_map(|target| {
                    target
                        .to_str()?
                        .strip_prefix("socket:[")?
                        .strip_suffix(']')?
                        .parse()
                        .ok()
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(target_os = "linux")]
fn fork_probe(cluster: &ClusterProcess) -> serde_json::Value {
    let response = cluster.request_path("/fork").unwrap();
    let body = response.split("\r\n\r\n").nth(1).unwrap();
    serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}: {response}"))
}

/// Samples `/fork` until `workers` distinct worker PIDs answered, keeping
/// every response in order.
#[cfg(target_os = "linux")]
fn fork_probes(cluster: &ClusterProcess, workers: usize) -> Vec<serde_json::Value> {
    let mut probes = Vec::new();
    let mut pids = HashSet::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while pids.len() < workers {
        assert!(
            Instant::now() < deadline,
            "only {} workers answered",
            pids.len()
        );
        let probe = fork_probe(cluster);
        pids.insert(probe["pid"].as_u64().unwrap());
        probes.push(probe);
    }
    probes
}

#[cfg(target_os = "linux")]
#[test]
fn recycled_workers_and_reloads_reuse_the_listeners_the_master_keeps() {
    let mut cluster =
        ClusterProcess::start_with_env(30, 2, None, None, &[("PAM_TEST_RATE_LIMIT", "0")]);
    let listeners = listening_inodes(cluster.port);
    assert_eq!(listeners.len(), 2, "one listener per worker");
    // The master holds every listener, so no recycle or reload closes one.
    assert!(listeners.is_subset(&socket_inodes(cluster.child.id())));

    let first_workers = cluster.children();
    let deadline = Instant::now() + Duration::from_secs(20);
    while cluster.children().is_subset(&first_workers) {
        assert!(Instant::now() < deadline, "workers were not recycled");
        let _ = cluster.request();
    }
    while cluster.children().len() != 2 {
        assert!(
            Instant::now() < deadline,
            "the recycled worker did not exit"
        );
        thread::sleep(Duration::from_millis(25));
    }
    cluster.signal(SIGHUP);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !cluster
        .admin_request("/ready")
        .contains(r#""generation":2"#)
    {
        assert!(Instant::now() < deadline, "the reload did not activate");
        thread::sleep(Duration::from_millis(25));
    }
    cluster.request_with_retry();
    // Recycled and reloaded workers adopted the same two sockets.
    assert_eq!(listening_inodes(cluster.port), listeners);
    for worker in cluster.children() {
        let held = socket_inodes(worker);
        assert_eq!(held.intersection(&listeners).count(), 1, "worker {worker}");
    }
    cluster.stop();
}

#[cfg(target_os = "linux")]
#[test]
fn post_boot_zygote_forks_isolated_workers_from_the_booted_application() {
    let directory = std::env::temp_dir().join(format!("pam-post-boot-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let log = directory.join("fork.log");
    let _ = fs::remove_file(&log);
    // A connection the application opens while booting and never closes.
    let peer = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let peer_port = peer.local_addr().unwrap().port().to_string();
    let (accepted, connections) = std::sync::mpsc::channel();
    thread::spawn(move || {
        for stream in peer.incoming() {
            let Ok(mut stream) = stream else { return };
            let accepted = accepted.clone();
            thread::spawn(move || {
                stream
                    .set_read_timeout(Some(Duration::from_secs(30)))
                    .unwrap();
                let mut received = Vec::new();
                let closed = stream.read_to_end(&mut received).is_ok();
                let _ = accepted.send((closed, received));
            });
        }
    });

    let workers = 4;
    let mut cluster = ClusterProcess::start_with_env(
        1_000,
        workers,
        None,
        None,
        &[
            ("PAM_PREFORK_BOOT", "app"),
            ("PAM_TEST_FORK", "1"),
            ("PAM_TEST_FORK_LOG", log.to_str().unwrap()),
            ("PAM_TEST_BOOT_SOCKET_PORT", &peer_port),
        ],
    );

    // The zygote closed the boot connection before forking: its peer sees
    // EOF without a single byte, and no worker ever writes into it.
    let (closed, received) = connections.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(closed && received.is_empty(), "{received:?}");

    let probes = fork_probes(&cluster, workers);
    let boot_pid = probes[0]["bootPid"].as_u64().unwrap();
    let mut first_by_pid = std::collections::HashMap::new();
    let mut requests_by_pid = std::collections::HashMap::<u64, u64>::new();
    for probe in &probes {
        let pid = probe["pid"].as_u64().unwrap();
        assert_ne!(pid, boot_pid, "workers must not be the zygote");
        assert_eq!(
            probe["bootPid"].as_u64(),
            Some(boot_pid),
            "booted once: {probe}"
        );
        assert_eq!(probe["forked"], serde_json::Value::Bool(true), "{probe}");
        assert_eq!(
            probe["socket"], "failed",
            "a worker wrote into a boot-time connection"
        );
        // Each worker has its own offset in files opened while booting.
        let seen = requests_by_pid.entry(pid).or_default();
        assert_eq!(probe["offset"].as_u64(), Some(*seen * 5), "{probe}");
        assert_eq!(
            probe["chunk"], probe["expected"],
            "a shared file offset: {probe}"
        );
        *seen += 1;
        first_by_pid.entry(pid).or_insert_with(|| probe.clone());
    }
    for key in ["mt", "rand", "lcg", "uniqid", "shuffle"] {
        let values = first_by_pid
            .values()
            .map(|probe| probe[key].to_string())
            .collect::<HashSet<_>>();
        assert_eq!(
            values.len(),
            workers,
            "workers drew the same {key}: {values:?}"
        );
    }
    // The OOM killer prefers a worker over the zygote all workers depend on.
    let adj = |pid: u64| {
        fs::read_to_string(format!("/proc/{pid}/oom_score_adj"))
            .unwrap()
            .trim()
            .parse::<i32>()
            .unwrap()
    };
    for pid in first_by_pid.keys() {
        assert_eq!(adj(*pid), (adj(boot_pid) + 300).min(1000), "worker {pid}");
    }
    let workers_seen = first_by_pid
        .values()
        .map(|probe| probe["worker"].as_str().unwrap().to_owned())
        .collect::<HashSet<_>>();
    assert_eq!(workers_seen.len(), workers);

    let log_text = fs::read_to_string(&log).unwrap();
    let before = log_text
        .lines()
        .filter(|line| line.starts_with("before:"))
        .collect::<Vec<_>>();
    assert_eq!(before, vec![format!("before:{boot_pid}")], "{log_text}");
    let after = log_text
        .lines()
        .filter(|line| line.starts_with("after:"))
        .count();
    assert_eq!(after, workers, "{log_text}");

    // A recycled or crashed worker is forked again from the same booted
    // zygote, in milliseconds, and the before-fork hooks do not run again.
    let victim = *first_by_pid.keys().next().unwrap();
    // SAFETY: the PID belongs to a worker of the cluster this test started.
    unsafe { kill(victim as i32, SIGKILL) };
    let deadline = Instant::now() + Duration::from_secs(10);
    while cluster.children().contains(&(victim as u32)) || cluster.children().len() < workers {
        assert!(
            Instant::now() < deadline,
            "the killed worker was not replaced"
        );
        thread::sleep(Duration::from_millis(10));
    }
    let replacement = fork_probes(&cluster, workers);
    assert!(
        replacement
            .iter()
            .all(|probe| probe["bootPid"].as_u64() == Some(boot_pid))
    );
    let log_text = fs::read_to_string(&log).unwrap();
    assert_eq!(
        log_text
            .lines()
            .filter(|line| line.starts_with("before:"))
            .count(),
        1
    );

    // A reload boots a fresh zygote from the current code.
    cluster.signal(SIGHUP);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "the reload did not activate");
        let ready = cluster.admin_request("/ready");
        if ready.starts_with("HTTP/1.1 200") && ready.contains(r#""generation":2"#) {
            let probe = fork_probe(&cluster);
            if probe["bootPid"].as_u64() != Some(boot_pid) {
                break;
            }
        }
        thread::sleep(Duration::from_millis(25));
    }
    let log_text = fs::read_to_string(&log).unwrap();
    assert_eq!(
        log_text
            .lines()
            .filter(|line| line.starts_with("before:"))
            .count(),
        2
    );
    cluster.stop();
    let _ = fs::remove_dir_all(&directory);
}

#[cfg(target_os = "linux")]
#[test]
fn post_boot_zygote_falls_back_to_pre_boot_forking_with_a_log_line() {
    let directory =
        std::env::temp_dir().join(format!("pam-post-boot-fallback-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let log = directory.join("fork.log");
    let stderr = directory.join("stderr.log");
    let mut cluster = ClusterProcess::start_with_env(
        1_000,
        2,
        None,
        None,
        &[
            ("PAM_PREFORK_BOOT", "app"),
            ("PAM_TEST_FORK", "1"),
            ("PAM_TEST_FORK_FAIL_IN_ZYGOTE", "1"),
            ("PAM_TEST_FORK_LOG", log.to_str().unwrap()),
            ("PAM_TEST_STDERR_FILE", stderr.to_str().unwrap()),
        ],
    );
    let probes = fork_probes(&cluster, 2);
    for probe in &probes {
        // Pre-boot workers boot the application themselves.
        assert_eq!(probe["forked"], serde_json::Value::Bool(false), "{probe}");
        assert_eq!(probe["bootPid"], probe["pid"], "{probe}");
    }
    cluster.stop();
    let output = fs::read_to_string(&stderr).unwrap();
    assert!(
        output.contains("post-boot zygote disabled (the application")
            && output.contains("falling back to pre-boot forking"),
        "{output}"
    );
    assert!(
        !fs::read_to_string(&log)
            .unwrap_or_default()
            .contains("before:")
    );
    let _ = fs::remove_dir_all(&directory);
}
