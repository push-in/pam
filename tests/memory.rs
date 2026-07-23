use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

struct MemoryServer {
    child: Child,
    port: u16,
}

impl MemoryServer {
    fn start() -> Self {
        let probe = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/server.php");
        let child = Command::new(env!("CARGO_BIN_EXE_pam"))
            .arg(script)
            .env("PAM_TEST_PORT", port.to_string())
            .env("PAM_TEST_RATE_LIMIT", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut server = Self { child, port };
        server.wait_ready();
        server
    }

    fn wait_ready(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if self.request("/ping").is_ok() {
                return;
            }
            if let Some(status) = self.child.try_wait().unwrap() {
                panic!("memory-test server exited early: {status}");
            }
            thread::sleep(Duration::from_millis(20));
        }
        panic!("memory-test server did not become ready");
    }

    fn request(&self, path: &str) -> Result<String, String> {
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

    fn rss_bytes(&self) -> u64 {
        let status = fs::read_to_string(format!("/proc/{}/status", self.child.id())).unwrap();
        status
            .lines()
            .find_map(|line| line.strip_prefix("VmRSS:"))
            .and_then(|value| value.split_whitespace().next())
            .and_then(|value| value.parse::<u64>().ok())
            .map(|kilobytes| kilobytes * 1024)
            .expect("VmRSS must be available on Linux")
    }
}

impl Drop for MemoryServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn request_cleanup_keeps_fibers_context_and_rss_bounded() {
    let server = MemoryServer::start();

    for _ in 0..100 {
        server.request("/abandon-async").unwrap();
        let state = server.request("/runtime-state").unwrap();
        assert!(state.contains(r#""fibers":0"#), "{state}");
        assert!(state.contains(r#""context":null"#), "{state}");
    }

    for _ in 0..1_000 {
        server.request("/memory-cycle").unwrap();
    }
    let baseline = server.rss_bytes();
    let mut samples = Vec::new();
    for request in 1..=10_000 {
        server.request("/memory-cycle").unwrap();
        if request % 1_000 == 0 {
            samples.push(server.rss_bytes());
        }
    }

    let highest = samples.iter().copied().max().unwrap_or(baseline);
    let growth = highest.saturating_sub(baseline);
    eprintln!(
        "memory soak: baseline={} MiB highest={} MiB growth={} MiB samples={samples:?}",
        baseline / 1024 / 1024,
        highest / 1024 / 1024,
        growth / 1024 / 1024,
    );
    assert!(
        growth <= 32 * 1024 * 1024,
        "RSS grew by {} MiB after warmup; baseline={} samples={samples:?}",
        growth / 1024 / 1024,
        baseline,
    );

    let metrics = server.request("/metrics").unwrap();
    assert!(metrics.contains("pam_php_fibers 0"), "{metrics}");
}
