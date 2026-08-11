use std::fs;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bytes::Buf;
use rustls_pki_types::{CertificateDer, pem::PemObject};
use tungstenite::client::IntoClientRequest;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket, connect};

type TestSocket = WebSocket<MaybeTlsStream<TcpStream>>;

struct ServerProcess {
    child: Child,
    port: u16,
    temporary_directory: Option<PathBuf>,
    certificate: Option<PathBuf>,
}

impl ServerProcess {
    fn start() -> Self {
        Self::start_with_response_limits(None)
    }

    fn start_with_response_limits(response_limits: Option<(usize, usize)>) -> Self {
        Self::start_with_options(response_limits, None)
    }

    fn start_with_rate_limit(rate_limit_per_second: u32) -> Self {
        Self::start_with_options(None, Some(rate_limit_per_second))
    }

    fn start_with_response_cache() -> Self {
        let mut server = Self::start_with_options(None, None);
        server.child.kill().unwrap();
        server.child.wait().unwrap();

        let probe = TcpListener::bind(("127.0.0.1", 0)).expect("test port should be available");
        server.port = probe.local_addr().unwrap().port();
        drop(probe);
        server.child = Command::new(env!("CARGO_BIN_EXE_pam"))
            .arg("tests/fixtures/server.php")
            .env("PAM_TEST_PORT", server.port.to_string())
            .env("PAM_TEST_RESPONSE_CACHE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Pam cache server should start");
        server.wait_until_ready();
        server
    }

    fn start_with_redis(redis_port: u16) -> Self {
        let mut server = Self::start_with_options(None, None);
        server.child.kill().unwrap();
        server.child.wait().unwrap();

        let probe = TcpListener::bind(("127.0.0.1", 0)).expect("test port should be available");
        server.port = probe.local_addr().unwrap().port();
        drop(probe);
        server.child = Command::new(env!("CARGO_BIN_EXE_pam"))
            .arg("tests/fixtures/server.php")
            .env("PAM_TEST_PORT", server.port.to_string())
            .env("PAM_TEST_REDIS_PORT", redis_port.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Pam Redis test server should start");
        server.wait_until_ready();
        server
    }

    fn start_with_http_upstream(upstream_port: u16) -> Self {
        let mut server = Self::start_with_options(None, None);
        server.child.kill().unwrap();
        server.child.wait().unwrap();

        let probe = TcpListener::bind(("127.0.0.1", 0)).expect("test port should be available");
        server.port = probe.local_addr().unwrap().port();
        drop(probe);
        server.child = Command::new(env!("CARGO_BIN_EXE_pam"))
            .arg("tests/fixtures/server.php")
            .env("PAM_TEST_PORT", server.port.to_string())
            .env("PAM_TEST_HTTP_UPSTREAM_PORT", upstream_port.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Pam HTTP client test server should start");
        server.wait_until_ready();
        server
    }

    fn start_with_isolated_database(database: &std::path::Path) -> Self {
        let mut server = Self::start_with_options(None, None);
        server.child.kill().unwrap();
        server.child.wait().unwrap();

        let probe = TcpListener::bind(("127.0.0.1", 0)).expect("test port should be available");
        server.port = probe.local_addr().unwrap().port();
        drop(probe);
        server.child = Command::new(env!("CARGO_BIN_EXE_pam"))
            .arg("tests/fixtures/server.php")
            .env("PAM_TEST_PORT", server.port.to_string())
            .env("PAM_TEST_ISOLATED_DATABASE", database)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("Pam isolated database test server should start");
        server.wait_until_ready();
        server
    }

    fn start_with_options(
        response_limits: Option<(usize, usize)>,
        rate_limit_per_second: Option<u32>,
    ) -> Self {
        let probe = TcpListener::bind(("127.0.0.1", 0)).expect("test port should be available");
        let port = probe.local_addr().unwrap().port();
        drop(probe);

        let mut command = Command::new(env!("CARGO_BIN_EXE_pam"));
        command
            .arg("tests/fixtures/server.php")
            .env("PAM_TEST_PORT", port.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some((max_response_bytes, max_response_chunk_bytes)) = response_limits {
            command
                .env(
                    "PAM_TEST_MAX_RESPONSE_BYTES",
                    max_response_bytes.to_string(),
                )
                .env(
                    "PAM_TEST_MAX_RESPONSE_CHUNK_BYTES",
                    max_response_chunk_bytes.to_string(),
                );
        }
        if let Some(rate_limit_per_second) = rate_limit_per_second {
            command.env("PAM_TEST_RATE_LIMIT", rate_limit_per_second.to_string());
        }
        let child = command.spawn().expect("Pam server should start");
        let mut server = Self {
            child,
            port,
            temporary_directory: None,
            certificate: None,
        };
        server.wait_until_ready();
        server
    }

    fn start_tls() -> Self {
        let probe = TcpListener::bind(("127.0.0.1", 0)).expect("test port should be available");
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("pam-tls-test-{}-{unique}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let certificate = directory.join("certificate.pem");
        let key = directory.join("key.pem");
        let generated = Command::new("openssl")
            .args(["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout"])
            .arg(&key)
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
            .expect("openssl should be installed for the TLS integration test");
        assert!(generated.success(), "openssl certificate generation failed");

        let child = Command::new(env!("CARGO_BIN_EXE_pam"))
            .arg("tests/fixtures/server.php")
            .env("PAM_TEST_PORT", port.to_string())
            .env("PAM_TLS_CERT", &certificate)
            .env("PAM_TLS_KEY", &key)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Pam TLS server should start");
        let mut server = Self {
            child,
            port,
            temporary_directory: Some(directory),
            certificate: Some(certificate),
        };
        server.wait_until_ready();
        server
    }

    fn wait_until_ready(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if TcpStream::connect(("127.0.0.1", self.port)).is_ok() {
                return;
            }
            if let Some(status) = self.child.try_wait().unwrap() {
                panic!("Pam server stopped before accepting connections: {status}");
            }
            thread::sleep(Duration::from_millis(20));
        }

        panic!("Pam server did not accept connections in time");
    }

    fn http_request(&self, request: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }
}

#[test]
fn caches_only_explicit_public_anonymous_responses() {
    let server = ServerProcess::start_with_response_cache();
    let first = response_json(
        &server
            .http_request("GET /cached HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"),
    );
    let second = response_json(
        &server
            .http_request("GET /cached HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"),
    );
    let authenticated = response_json(&server.http_request(
        "GET /cached HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer test\r\nConnection: close\r\n\r\n",
    ));

    assert_eq!(first["calls"], 1);
    assert_eq!(second["calls"], 1, "the public response should be cached");
    assert_eq!(
        authenticated["calls"], 2,
        "authenticated requests must bypass cache"
    );

    let metrics = server
        .http_request("GET /metrics HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    assert!(
        metrics.contains("pam_http_response_cache_hits_total 1"),
        "{metrics}"
    );
    assert!(
        metrics.contains("pam_http_response_cache_misses_total 1"),
        "{metrics}"
    );
    assert!(
        metrics.contains("# TYPE pam_http_request_duration_seconds histogram"),
        "{metrics}"
    );
    assert!(
        metrics.contains("pam_http_request_duration_seconds_bucket{le=\"+Inf\"}"),
        "{metrics}"
    );
}

#[test]
fn serves_stale_cache_entries_while_one_request_revalidates() {
    let server = ServerProcess::start_with_response_cache();
    let warm =
        response_json(&server.http_request(
            "GET /cached-slow HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
        ));
    assert_eq!(warm["calls"], 1);
    thread::sleep(Duration::from_millis(150));

    let port = server.port;
    let refresh = thread::spawn(move || {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .write_all(b"GET /cached-slow HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response_json(&response)
    });
    thread::sleep(Duration::from_millis(40));

    let stale =
        response_json(&server.http_request(
            "GET /cached-slow HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
        ));
    assert_eq!(stale["calls"], 1, "stale response should not wait for PHP");
    assert_eq!(refresh.join().unwrap()["calls"], 2);

    let metrics = server
        .http_request("GET /metrics HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    assert!(
        metrics.contains("pam_http_response_cache_stale_total 1"),
        "{metrics}"
    );
}

#[test]
fn partitions_cache_entries_by_configured_vary_headers() {
    let server = ServerProcess::start_with_response_cache();
    let english = response_json(&server.http_request(
        "GET /cached HTTP/1.1\r\nHost: 127.0.0.1\r\nAccept-Language: en\r\nConnection: close\r\n\r\n",
    ));
    let portuguese = response_json(&server.http_request(
        "GET /cached HTTP/1.1\r\nHost: 127.0.0.1\r\nAccept-Language: pt-BR\r\nConnection: close\r\n\r\n",
    ));
    let english_again = response_json(&server.http_request(
        "GET /cached HTTP/1.1\r\nHost: 127.0.0.1\r\nAccept-Language: en\r\nConnection: close\r\n\r\n",
    ));

    assert_eq!(english["calls"], 1);
    assert_eq!(portuguese["calls"], 2);
    assert_eq!(english_again["calls"], 1);
}

#[test]
fn redis_client_yields_the_php_owner_while_waiting_for_responses() {
    let redis = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let redis_port = redis.local_addr().unwrap().port();
    let fake_redis = thread::spawn(move || {
        let (mut stream, _) = redis.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        while !request
            .windows(b"fixture-key".len())
            .any(|part| part == b"fixture-key")
        {
            let mut chunk = [0_u8; 256];
            let length = stream.read(&mut chunk).unwrap();
            assert!(
                length > 0,
                "Redis client closed before sending its pipeline"
            );
            request.extend_from_slice(&chunk[..length]);
        }
        let request = String::from_utf8_lossy(&request);
        assert!(request.contains("PING"), "{request}");
        assert!(request.contains("fixture-key"), "{request}");
        thread::sleep(Duration::from_millis(120));
        stream.write_all(b"+PONG\r\n$5\r\n").unwrap();
        thread::sleep(Duration::from_millis(10));
        stream.write_all(b"value\r\n").unwrap();
    });
    let server = ServerProcess::start_with_redis(redis_port);
    let port = server.port;
    let redis_request = thread::spawn(move || {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .write_all(b"GET /redis HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    });
    thread::sleep(Duration::from_millis(30));

    let started = Instant::now();
    let ping =
        server.http_request("GET /ping HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    assert!(ping.starts_with("HTTP/1.1 200"), "{ping}");
    assert!(
        started.elapsed() < Duration::from_millis(80),
        "Redis I/O blocked the PHP owner for {:?}",
        started.elapsed(),
    );

    let response = redis_request.join().unwrap();
    assert_eq!(
        response_json(&response)["responses"],
        serde_json::json!(["PONG", "value"]),
    );
    fake_redis.join().unwrap();
}

#[test]
fn http_client_yields_the_php_owner_while_waiting_for_an_upstream() {
    let upstream = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let upstream_port = upstream.local_addr().unwrap().port();
    let fake_upstream = thread::spawn(move || {
        let (mut stream, _) = upstream.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        while !request.windows(4).any(|part| part == b"\r\n\r\n") {
            let mut chunk = [0_u8; 256];
            let length = stream.read(&mut chunk).unwrap();
            assert!(length > 0, "HTTP client closed before sending headers");
            request.extend_from_slice(&chunk[..length]);
        }
        assert!(
            String::from_utf8_lossy(&request).starts_with("GET /upstream HTTP/1.1"),
            "{}",
            String::from_utf8_lossy(&request),
        );
        thread::sleep(Duration::from_millis(120));
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello")
            .unwrap();
    });
    let server = ServerProcess::start_with_http_upstream(upstream_port);
    let port = server.port;
    let upstream_request = thread::spawn(move || {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .write_all(
                b"GET /http-client-slow HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    });
    thread::sleep(Duration::from_millis(30));

    let started = Instant::now();
    let ping =
        server.http_request("GET /ping HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    assert!(ping.starts_with("HTTP/1.1 200"), "{ping}");
    assert!(
        started.elapsed() < Duration::from_millis(80),
        "upstream HTTP I/O blocked the PHP owner for {:?}",
        started.elapsed(),
    );
    assert_eq!(
        response_json(&upstream_request.join().unwrap())["body"],
        "hello",
    );
    fake_upstream.join().unwrap();
}

#[test]
fn isolated_database_pool_yields_and_keeps_blocking_pdo_outside_the_worker() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let database = std::env::temp_dir().join(format!(
        "pam-isolated-pdo-{}-{unique}.sqlite",
        std::process::id(),
    ));
    let server = ServerProcess::start_with_isolated_database(&database);
    let port = server.port;
    let database_request = thread::spawn(move || {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .write_all(
                b"GET /isolated-database HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    });
    thread::sleep(Duration::from_millis(30));

    let started = Instant::now();
    let ping =
        server.http_request("GET /ping HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    assert!(ping.starts_with("HTTP/1.1 200"), "{ping}");
    assert!(
        started.elapsed() < Duration::from_millis(80),
        "isolated PDO blocked the PHP owner for {:?}",
        started.elapsed(),
    );
    let response = database_request.join().unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert_eq!(response_json(&response)["total"], 45_000_150_000_u64);
    let _ = fs::remove_file(database);
}

#[test]
fn purges_tagged_cache_entries_only_with_valid_credentials() {
    let server = ServerProcess::start_with_response_cache();
    let first_raw =
        server.http_request("GET /cached HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    assert!(
        !first_raw.to_ascii_lowercase().contains("x-pam-cache-tags"),
        "internal cache tags must not leak: {first_raw}"
    );
    assert_eq!(response_json(&first_raw)["calls"], 1);
    assert_eq!(
        response_json(
            &server.http_request(
                "GET /cached HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
            )
        )["calls"],
        1,
    );

    let unauthorized = server.http_request(
        "POST /__pam/cache/purge HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: 17\r\nConnection: close\r\n\r\n{\"tag\":\"catalog\"}",
    );
    assert!(unauthorized.starts_with("HTTP/1.1 401"), "{unauthorized}");

    let authorized = server.http_request(
        "POST /__pam/cache/purge HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer pam-test-cache-purge-secret-32-bytes\r\nContent-Type: application/json\r\nContent-Length: 17\r\nConnection: close\r\n\r\n{\"tag\":\"catalog\"}",
    );
    assert!(authorized.starts_with("HTTP/1.1 200"), "{authorized}");
    assert_eq!(response_json(&authorized)["purged"], 1);
    assert_eq!(
        response_json(
            &server.http_request(
                "GET /cached HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
            )
        )["calls"],
        2,
    );
}

#[test]
fn enforces_buffered_and_streaming_response_limits() {
    let server = ServerProcess::start_with_response_limits(Some((4 * 1024, 1024)));

    let buffered = server.http_request(
        "GET /oversized-response HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(buffered.starts_with("HTTP/1.1 500"), "{buffered}");
    assert!(!buffered.contains(&"o".repeat(1024)), "{buffered}");

    let chunk = server.http_request(
        "GET /oversized-chunk HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(chunk.starts_with("HTTP/1.1 500"), "{chunk}");
    assert!(!chunk.contains(&"c".repeat(1024)), "{chunk}");

    let total = server.http_request(
        "GET /over-total-stream HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(total.starts_with("HTTP/1.1 200"), "{total}");
    let body = total.split_once("\r\n\r\n").unwrap().1;
    let streamed_bytes = body.bytes().filter(|byte| *byte == b's').count();
    assert!((1..=4 * 1024).contains(&streamed_bytes), "{streamed_bytes}");
    assert!(
        !total.ends_with("0\r\n\r\n"),
        "an over-limit stream must terminate as an HTTP body error"
    );
}

fn connect_websocket(port: u16) -> (TestSocket, serde_json::Value) {
    let url = format!("ws://127.0.0.1:{port}/ws");
    let (mut socket, upgrade) = connect(url.as_str()).expect("WebSocket upgrade should succeed");
    assert_eq!(upgrade.status().as_u16(), 101);
    if let MaybeTlsStream::Plain(stream) = socket.get_mut() {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
    }

    let welcome = socket.read().unwrap().into_text().unwrap();
    let welcome = serde_json::from_str(&welcome).unwrap();
    (socket, welcome)
}

fn read_event(socket: &mut TestSocket) -> serde_json::Value {
    let event = socket.read().unwrap().into_text().unwrap();
    serde_json::from_str(&event).unwrap()
}

fn response_json(response: &str) -> serde_json::Value {
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .expect("HTTP response should contain a body boundary");
    serde_json::from_str(body).expect("HTTP response body should be valid JSON")
}

async fn http3_request(server: &ServerProcess) -> (http::StatusCode, String) {
    let certificate = server
        .certificate
        .as_ref()
        .expect("TLS test server must retain its certificate");
    let certificates = CertificateDer::pem_file_iter(certificate)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut roots = quinn::rustls::RootCertStore::empty();
    for certificate in certificates {
        roots.add(certificate).unwrap();
    }

    let mut crypto = quinn::rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    crypto.alpn_protocols = vec![b"h3".to_vec()];
    let quic_crypto = quinn::crypto::rustls::QuicClientConfig::try_from(crypto).unwrap();
    let client_config = quinn::ClientConfig::new(Arc::new(quic_crypto));
    let bind_address = SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0));
    let mut endpoint = quinn::Endpoint::client(bind_address).unwrap();
    endpoint.set_default_client_config(client_config);

    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, server.port));
    let connection = endpoint
        .connect(address, "localhost")
        .unwrap()
        .await
        .unwrap();
    let quic = h3_quinn::Connection::new(connection);
    let (mut driver, mut sender) = h3::client::new(quic).await.unwrap();
    let driver_task = tokio::spawn(async move {
        futures_util::future::poll_fn(|context| driver.poll_close(context)).await
    });

    let request = http::Request::get(format!("https://localhost:{}/ping?query=quic", server.port))
        .body(())
        .unwrap();
    let mut stream = sender.send_request(request).await.unwrap();
    stream.finish().await.unwrap();
    let response = stream.recv_response().await.unwrap();
    let status = response.status();
    let mut body = Vec::new();
    while let Some(mut chunk) = stream.recv_data().await.unwrap() {
        let remaining = chunk.remaining();
        body.extend_from_slice(&chunk.copy_to_bytes(remaining));
    }

    endpoint.close(
        quinn::VarInt::from_u32(0),
        b"HTTP/3 integration test complete",
    );
    driver_task.abort();
    (status, String::from_utf8(body).unwrap())
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(directory) = self.temporary_directory.take() {
            let _ = fs::remove_dir_all(directory);
        }
    }
}

#[test]
fn interleaves_suspended_php_requests_without_context_leaks() {
    let server = ServerProcess::start();
    let started = Instant::now();
    let requests = (0..20)
        .map(|index| {
            let port = server.port;
            thread::spawn(move || {
                let body = format!("request-{index}");
                let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
                stream
                    .write_all(
                        format!(
                            "POST /async-context HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len(),
                        )
                        .as_bytes(),
                    )
                    .unwrap();
                let mut response = String::new();
                stream.read_to_string(&mut response).unwrap();
                assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
                assert!(
                    response
                        .to_ascii_lowercase()
                        .contains("x-async-context: isolated"),
                    "{response}"
                );
                let payload = response_json(&response);
                assert_eq!(payload["bodyBefore"], body);
                assert_eq!(payload["bodyAfter"], body);
                assert_eq!(payload["requestIdBefore"], payload["requestIdAfter"]);
                payload["requestIdAfter"].as_str().unwrap().to_owned()
            })
        })
        .collect::<Vec<_>>();
    let mut request_ids = requests
        .into_iter()
        .map(|request| request.join().unwrap())
        .collect::<Vec<_>>();
    request_ids.sort();
    request_ids.dedup();

    assert_eq!(
        request_ids.len(),
        20,
        "request contexts must remain isolated"
    );
    assert!(
        started.elapsed() < Duration::from_millis(700),
        "20 x 50ms timers were serialized instead of interleaved: {:?}",
        started.elapsed(),
    );

    let response = server
        .http_request("GET /async-stream HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert_eq!(response_json(&response)["payload"], "stream-ready");

    let response = server
        .http_request("GET /native-io HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    let payload = response_json(&response);
    assert_eq!(payload["contents"], "native-file");
    assert_eq!(payload["written"], 11);
    assert_eq!(payload["process"], "NATIVE-PROCESS");
    assert_eq!(payload["successful"], true);
    assert!(
        payload["addresses"]
            .as_array()
            .is_some_and(|value| !value.is_empty())
    );

    let response = server.http_request(
        "GET /native-process-timeout HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    let payload = response_json(&response);
    assert_eq!(payload["kind"], 2);
    assert!(
        payload["durationMs"]
            .as_f64()
            .is_some_and(|value| value < 500.0),
        "process group cleanup exceeded its deadline: {payload}",
    );

    let mut stream = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(30)))
        .unwrap();
    stream
        .write_all(b"GET /stream HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut first_delivery = Vec::new();
    let mut buffer = [0_u8; 4096];
    while let Ok(read) = stream.read(&mut buffer) {
        if read == 0 {
            break;
        }
        first_delivery.extend_from_slice(&buffer[..read]);
    }
    let first_delivery = String::from_utf8(first_delivery).unwrap();
    assert!(first_delivery.contains("first-chunk"), "{first_delivery}");
    assert!(!first_delivery.contains("second-chunk"), "{first_delivery}");
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let mut remainder = String::new();
    stream.read_to_string(&mut remainder).unwrap();
    assert!(remainder.contains("second-chunk"), "{remainder}");

    let response =
        server.http_request("GET /sse HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    assert!(
        response.contains("content-type: text/event-stream"),
        "{response}"
    );
    assert!(response.contains("data: {\"event\":1}"), "{response}");
    assert!(response.contains("data: {\"event\":2}"), "{response}");

    let response = server.http_request(&format!(
        "GET /http-client HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
        server.port,
    ));
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    let payload = response_json(&response);
    assert_eq!(payload["status"], 200);
    assert_eq!(payload["upstream"]["message"], "pong");
    assert_eq!(payload["upstream"]["query"], "client");

    let response = server.http_request(
        "GET /request-scope HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    let payload = response_json(&response);
    assert_eq!(payload["value"], "lifecycle");
    assert!(
        payload["id"]
            .as_str()
            .is_some_and(|id| id.starts_with("standalone-"))
    );
    let response = server.http_request(
        "GET /request-scope-state HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    let payload = response_json(&response);
    assert_eq!(payload["cleanups"], 1);
    assert_eq!(payload["metrics"]["activeScopes"], 1);
    assert_eq!(payload["metrics"]["cleanupFailures"], 0);
}

#[test]
fn serves_https_and_negotiates_http2_and_http3() {
    let server = ServerProcess::start_tls();
    let output = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--insecure",
            "--http2",
            "--dump-header",
            "-",
        ])
        .arg(format!("https://127.0.0.1:{}/ping?query=tls", server.port))
        .args(["--write-out", "\n%{http_version}"])
        .output()
        .expect("curl should be installed for the HTTP/2 integration test");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response = String::from_utf8_lossy(&output.stdout);
    assert!(response.contains(r#""query":"tls""#), "{response}");
    assert!(
        response
            .to_ascii_lowercase()
            .contains(&format!("alt-svc: h3=\":{}\"; ma=86400", server.port)),
        "{response}"
    );
    assert!(response.ends_with("\n2"), "{response}");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (status, body) = runtime.block_on(http3_request(&server));
    assert_eq!(status, http::StatusCode::OK);
    assert!(body.contains(r#""query":"quic""#), "{body}");
}

#[test]
fn serves_rest_and_websocket_events_on_the_same_port() {
    let server = ServerProcess::start();

    let mut slow_client = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    slow_client
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    slow_client
        .write_all(b"GET /ping HTTP/1.1\r\nHost:")
        .unwrap();
    thread::sleep(Duration::from_millis(200));
    let mut slow_response = String::new();
    let _ = slow_client.read_to_string(&mut slow_response);
    assert!(
        slow_response.is_empty() || slow_response.contains("408 Request Timeout"),
        "{slow_response}"
    );

    let mut slow_body = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    slow_body
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    slow_body
        .write_all(
            b"POST /echo HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 100\r\nConnection: close\r\n\r\nx",
        )
        .unwrap();
    thread::sleep(Duration::from_millis(200));
    let mut slow_body_response = String::new();
    let _ = slow_body.read_to_string(&mut slow_body_response);
    assert!(
        slow_body_response.contains("408 Request Timeout"),
        "{slow_body_response}"
    );

    let oversized_body = "x".repeat(1025);
    let response = server.http_request(&format!(
        "POST /echo HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{oversized_body}",
        oversized_body.len(),
    ));
    assert!(
        response.starts_with("HTTP/1.1 413 Payload Too Large"),
        "{response}"
    );

    let many_headers = (0..17)
        .map(|index| format!("X-Test-{index}: value\r\n"))
        .collect::<String>();
    let response = server.http_request(&format!(
        "GET /ping HTTP/1.1\r\nHost: 127.0.0.1\r\n{many_headers}Connection: close\r\n\r\n"
    ));
    assert!(
        response.starts_with("HTTP/1.1 431 Request Header Fields Too Large"),
        "{response}"
    );

    let response = server.http_request(
        "GET /ping?query=memory HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(
        response.contains(r#"{"message":"pong","query":"memory"}"#),
        "{response}"
    );

    let body = r#"{"value":"request-body"}"#;
    let response = server.http_request(&format!(
        "POST /echo HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nX-Pam-Test: header-value\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len(),
    ));
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(response.contains("request-body"), "{response}");
    assert!(response.contains("header-value"), "{response}");

    let body = "name=David&role=developer";
    let response = server.http_request(&format!(
        "POST /context HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/x-www-form-urlencoded\r\nCookie: client=browser\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len(),
    ));
    assert!(response.starts_with("HTTP/1.1 202 Accepted"), "{response}");
    assert!(response.contains("x-native-header: captured"), "{response}");
    assert!(
        response.contains("set-cookie: pam=compatible"),
        "{response}"
    );
    assert!(response.contains(r#""cookie":"browser""#), "{response}");
    assert!(response.contains(r#""name":"David""#), "{response}");
    assert!(
        response.contains(r#""raw":"name=David&role=developer""#),
        "{response}"
    );
    assert!(
        response.contains(r#""requestId":"standalone-"#),
        "{response}"
    );

    let response = server.http_request(
        "OPTIONS /ping HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: https://app.example\r\nAccess-Control-Request-Method: GET\r\nConnection: close\r\n\r\n",
    );
    assert!(
        response.starts_with("HTTP/1.1 204 No Content"),
        "{response}"
    );
    assert!(
        response.contains("access-control-allow-origin: https://app.example"),
        "{response}"
    );

    let response = server
        .http_request("GET /metrics HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(response.contains("pam_http_requests_total"), "{response}");
    assert!(
        response.contains("pam_event_loop_lag_seconds"),
        "{response}"
    );
    assert!(
        response.contains("pam_process_resident_memory_bytes"),
        "{response}"
    );
    assert!(response.contains("pam_php_fibers"), "{response}");

    let boundary = "pam-boundary";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"description\"\r\n\r\ncontract\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"document\"; filename=\"proof.txt\"\r\nContent-Type: text/plain\r\n\r\nuploaded-content\r\n--{boundary}--\r\n"
    );
    let response = server.http_request(&format!(
        "POST /upload HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: multipart/form-data; boundary={boundary}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len(),
    ));
    assert!(response.contains(r#""field":"contract""#), "{response}");
    assert!(response.contains(r#""filename":"proof.txt""#), "{response}");
    assert!(
        response.contains(r#""contents":"uploaded-content""#),
        "{response}"
    );

    let response = server
        .http_request("GET /session HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    assert!(response.contains(r#""visits":1"#), "{response}");
    let cookie = response
        .lines()
        .find(|line| {
            line.to_ascii_lowercase()
                .starts_with("set-cookie: pamsessid=")
        })
        .and_then(|line| line.split_once(':'))
        .map(|(_, value)| value.trim().split(';').next().unwrap().to_owned())
        .expect("session response should set a cookie");
    let response = server.http_request(&format!(
        "GET /session HTTP/1.1\r\nHost: 127.0.0.1\r\nCookie: {cookie}\r\nConnection: close\r\n\r\n"
    ));
    assert!(response.contains(r#""visits":2"#), "{response}");
    let response = server
        .http_request("GET /session HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    assert!(response.contains(r#""visits":1"#), "{response}");

    let mut compression_client = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    compression_client
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    compression_client
        .write_all(
            b"GET /ws HTTP/1.1\r\nHost: 127.0.0.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Extensions: permessage-deflate; client_max_window_bits\r\n\r\n",
        )
        .unwrap();
    let mut compression_response = Vec::new();
    let mut chunk = [0_u8; 1024];
    while !compression_response
        .windows(4)
        .any(|window| window == b"\r\n\r\n")
    {
        let read = compression_client.read(&mut chunk).unwrap();
        assert!(read > 0);
        compression_response.extend_from_slice(&chunk[..read]);
    }
    let compression_response = String::from_utf8_lossy(&compression_response);
    assert!(compression_response.starts_with("HTTP/1.1 101"));
    assert!(
        compression_response
            .to_ascii_lowercase()
            .contains("sec-websocket-extensions: permessage-deflate"),
        "{compression_response}"
    );
    drop(compression_client);
    thread::sleep(Duration::from_millis(30));

    let (mut first_socket, first_welcome) = connect_websocket(server.port);
    assert_eq!(first_welcome["event"], "welcome");
    let first_socket_id = first_welcome["data"]["socketId"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(first_socket_id.starts_with("socket-"));

    let (mut second_socket, second_welcome) = connect_websocket(server.port);
    assert_eq!(second_welcome["event"], "welcome");
    let second_socket_id = second_welcome["data"]["socketId"].as_str().unwrap();
    assert_ne!(first_socket_id, second_socket_id);

    let third = connect(format!("ws://127.0.0.1:{}/ws", server.port));
    assert!(matches!(
        third,
        Err(tungstenite::Error::Http(response)) if response.status().as_u16() == 503
    ));

    first_socket
        .send(Message::Text(
            r#"{"event":"echo","data":{"value":"realtime"}}"#.into(),
        ))
        .unwrap();
    for echo in [
        read_event(&mut first_socket),
        read_event(&mut second_socket),
    ] {
        assert_eq!(echo["event"], "echo");
        assert_eq!(echo["data"]["value"], "realtime");
        assert_eq!(echo["data"]["socketId"], first_socket_id);
    }

    second_socket
        .send(Message::Text(
            r#"{"event":"room_echo","data":{"value":"room-value"}}"#.into(),
        ))
        .unwrap();
    for room_echo in [
        read_event(&mut first_socket),
        read_event(&mut second_socket),
    ] {
        assert_eq!(room_echo["event"], "room_echo");
        assert_eq!(room_echo["data"]["value"], "room-value");
    }

    first_socket
        .send(Message::Text(
            r#"{"id":"ack-1","event":"echo","data":{"value":"with-ack"}}"#.into(),
        ))
        .unwrap();
    let _ = read_event(&mut first_socket);
    let acknowledgement = read_event(&mut first_socket);
    let _ = read_event(&mut second_socket);
    assert_eq!(acknowledgement["ack"], "ack-1");
    assert_eq!(acknowledgement["data"]["accepted"], true);

    second_socket
        .send(Message::Binary(b"binary".to_vec().into()))
        .unwrap();
    assert_eq!(
        second_socket.read().unwrap().into_data().as_ref(),
        b"BINARY"
    );

    first_socket.close(None).unwrap();
    second_socket.close(None).unwrap();
    thread::sleep(Duration::from_millis(30));

    let (mut resumable, resumable_welcome) = connect_websocket(server.port);
    let stable_session = resumable_welcome["data"]["socketId"]
        .as_str()
        .unwrap()
        .to_owned();
    let resume_token = resumable_welcome["data"]["resumeToken"]
        .as_str()
        .unwrap()
        .to_owned();
    resumable.close(None).unwrap();
    drop(resumable);
    thread::sleep(Duration::from_millis(30));

    let invalid_resume = connect(format!(
        "ws://127.0.0.1:{}/ws?sessionId={stable_session}&resumeToken=invalid",
        server.port
    ));
    assert!(matches!(
        invalid_resume,
        Err(tungstenite::Error::Http(response)) if response.status().as_u16() == 401
    ));

    let (mut reconnected, _) = connect(format!(
        "ws://127.0.0.1:{}/ws?sessionId={stable_session}&resumeToken={resume_token}",
        server.port
    ))
    .unwrap();
    let welcome = read_event(&mut reconnected);
    assert_eq!(welcome["data"]["socketId"], stable_session);
    reconnected
        .send(Message::Text("x".repeat(2048).into()))
        .unwrap();
    assert!(matches!(reconnected.read(), Ok(Message::Close(_)) | Err(_)));
    drop(reconnected);
    thread::sleep(Duration::from_millis(30));

    let mut denied_request = format!("ws://127.0.0.1:{}/ws", server.port)
        .into_client_request()
        .unwrap();
    denied_request
        .headers_mut()
        .insert("x-deny", "yes".parse().unwrap());
    let (mut denied_socket, _) = connect(denied_request).unwrap();
    assert!(matches!(
        denied_socket.read(),
        Ok(Message::Close(_)) | Err(_)
    ));

    let rate_limited_server = Arc::new(ServerProcess::start_with_rate_limit(1));
    let barrier = Arc::new(Barrier::new(8));
    let requests = (0..8)
        .map(|_| {
            let server = Arc::clone(&rate_limited_server);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                server.http_request(
                    "GET /ping HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
                )
            })
        })
        .collect::<Vec<_>>();
    let saw_rate_limit = requests.into_iter().any(|request| {
        request
            .join()
            .unwrap()
            .starts_with("HTTP/1.1 429 Too Many Requests")
    });
    assert!(saw_rate_limit, "rate limiter did not reject a request");
}
