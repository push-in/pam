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
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if cluster.request_path("/api/pool").is_ok() {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }
    assert!(
        cluster
            .request_path("/api/pool")
            .unwrap()
            .contains(r#""pool":"api""#)
    );

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
    assert!(
        cluster
            .request_path("/api/pool")
            .unwrap()
            .contains(r#""pool":"api""#)
    );
    assert!(
        cluster
            .request_path("/pool")
            .unwrap()
            .contains(r#""pool":"web""#)
    );
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
                "2000",
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
                "2000",
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
        let deadline = Instant::now() + Duration::from_secs(8);
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
        let pid = self.child.id();
        fs::read_to_string(format!("/proc/{pid}/task/{pid}/children"))
            .unwrap_or_default()
            .split_whitespace()
            .filter_map(|value| value.parse().ok())
            .collect()
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

#[test]
fn recycles_workers_after_the_request_limit() {
    let mut cluster = ClusterProcess::start(3);
    let mut workers = HashSet::new();
    let mut processes = HashSet::new();
    for _ in 0..24 {
        let (worker, process) = request_identity(&cluster.request_with_retry());
        workers.insert(worker);
        processes.insert(process);
    }
    assert_eq!(workers, HashSet::from(["1".to_owned(), "2".to_owned()]));
    assert!(
        processes.len() >= 3,
        "workers were not recycled: {processes:?}"
    );
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

    let blocked = cluster.request_path("/block");
    assert!(blocked.is_err() || !blocked.unwrap().contains("late"));

    let deadline = Instant::now() + Duration::from_secs(4);
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
