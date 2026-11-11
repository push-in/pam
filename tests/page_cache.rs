//! End-to-end tests of the edge page cache against a real PAM server. The
//! central property: a response produced for one user is never served to
//! another (cookies, Authorization, Set-Cookie, Vary, private, forwarded
//! headers, query variations, concurrent misses).

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

struct Server {
    child: Child,
    port: u16,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A free port outside the kernel's ephemeral range, never handed out twice
/// by this binary. PAM listens with `SO_REUSEPORT`, so a port probed with
/// `bind(0)` could also be picked by another process on the machine and the
/// two servers would silently share it; `wait_until_ours` double-checks.
fn free_port() -> u16 {
    static HANDED_OUT: std::sync::Mutex<Vec<u16>> = std::sync::Mutex::new(Vec::new());
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos() as u64
        ^ u64::from(std::process::id()) << 16;
    loop {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let port = 20_000 + (seed >> 33) as u16 % 12_000;
        let mut handed_out = HANDED_OUT.lock().unwrap();
        if handed_out.contains(&port) || TcpListener::bind(("127.0.0.1", port)).is_err() {
            continue;
        }
        handed_out.push(port);
        return port;
    }
}

fn token(port: u16) -> String {
    format!("{}-{port}-{:?}", std::process::id(), Instant::now())
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/page_cache.php")
}

impl Server {
    fn start(environment: &[(&str, &str)]) -> Self {
        let port = free_port();
        let mut command = Command::new(env!("CARGO_BIN_EXE_pam"));
        command
            .arg(fixture())
            .env("PAM_TEST_PORT", port.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for (name, value) in environment {
            command.env(name, value);
        }
        let token = token(port);
        command.env("PAM_PC_TOKEN", &token);
        let mut server = Self {
            child: command.spawn().expect("PAM page cache server should start"),
            port,
        };
        wait_for_port(port, &mut server.child, &token);
        server
    }

    fn get(&self, target: &str, headers: &[(&str, &str)]) -> HttpResponse {
        request(self.port, "GET", target, headers)
    }
}

/// Waits until this test's own server answers on `port`.
fn wait_for_port(port: u16, child: &mut Child, token: &str) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let _ = stream
                .write_all(b"GET /__ready HTTP/1.1\r\nHost: ready\r\nConnection: close\r\n\r\n");
            let mut response = Vec::new();
            let _ = stream.read_to_end(&mut response);
            if String::from_utf8_lossy(&response).contains(&format!("\"token\":\"{token}\"")) {
                return;
            }
        }
        if let Some(status) = child.try_wait().unwrap() {
            panic!("PAM stopped before accepting connections: {status}");
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!("PAM did not accept connections in time");
}

#[derive(Debug)]
struct HttpResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl HttpResponse {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    fn cache(&self) -> &str {
        self.header("x-pam-cache").unwrap_or("-")
    }

    fn json(&self) -> Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|error| panic!("{error}: {}", String::from_utf8_lossy(&self.body)))
    }

    fn calls(&self) -> u64 {
        self.json()["calls"].as_u64().unwrap()
    }
}

fn request(port: u16, method: &str, target: &str, headers: &[(&str, &str)]) -> HttpResponse {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let mut raw = format!("{method} {target} HTTP/1.1\r\n");
    if !headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("host"))
    {
        raw.push_str("Host: example.com\r\n");
    }
    for (name, value) in headers {
        raw.push_str(&format!("{name}: {value}\r\n"));
    }
    raw.push_str("Connection: close\r\n\r\n");
    stream.write_all(raw.as_bytes()).unwrap();
    let mut response = Vec::new();
    if let Err(error) = stream.read_to_end(&mut response) {
        panic!("{method} {target}: {error} after {} bytes", response.len());
    }
    let _ = stream.shutdown(Shutdown::Both);
    parse_response(&response, method == "HEAD")
}

fn parse_response(raw: &[u8], head: bool) -> HttpResponse {
    let boundary = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("response has a header boundary");
    let head_text = String::from_utf8_lossy(&raw[..boundary]).into_owned();
    let mut lines = head_text.split("\r\n");
    let status = lines
        .next()
        .unwrap()
        .split(' ')
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    let mut body = raw[boundary + 4..].to_vec();
    let chunked = headers
        .iter()
        .any(|(name, value)| name == "transfer-encoding" && value.contains("chunked"));
    if chunked && !head {
        let mut decoded = Vec::new();
        let mut rest = &body[..];
        loop {
            let line_end = rest
                .windows(2)
                .position(|window| window == b"\r\n")
                .unwrap();
            let size = usize::from_str_radix(
                std::str::from_utf8(&rest[..line_end])
                    .unwrap()
                    .split(';')
                    .next()
                    .unwrap()
                    .trim(),
                16,
            )
            .unwrap();
            rest = &rest[line_end + 2..];
            if size == 0 {
                break;
            }
            decoded.extend_from_slice(&rest[..size]);
            rest = &rest[size + 2..];
        }
        body = decoded;
    }
    HttpResponse {
        status,
        headers,
        body,
    }
}

fn metric(port: u16, name: &str) -> u64 {
    let response = request(port, "GET", "/metrics", &[]);
    let text = String::from_utf8(response.body).unwrap();
    text.lines()
        .find_map(|line| line.strip_prefix(&format!("{name} ")))
        .unwrap_or_else(|| panic!("metric {name} missing:\n{text}"))
        .trim()
        .parse()
        .unwrap()
}

const PUBLIC: &str = "cc=public,%20s-maxage=60";

#[test]
fn serves_public_responses_without_php_and_answers_conditionals() {
    let server = Server::start(&[]);
    let target = format!("/page?{PUBLIC}");
    let first = server.get(&target, &[]);
    assert_eq!(first.status, 200);
    assert_eq!(first.cache(), "MISS");
    let second = server.get(&target, &[]);
    assert_eq!(second.cache(), "HIT");
    assert_eq!(second.calls(), first.calls(), "a hit never reaches PHP");
    assert_eq!(second.body, first.body);
    assert_eq!(second.header("age"), Some("0"));
    let etag = second
        .header("etag")
        .expect("stored responses carry an ETag")
        .to_owned();

    let not_modified = server.get(&target, &[("If-None-Match", &etag)]);
    assert_eq!(not_modified.status, 304);
    assert!(not_modified.body.is_empty());
    assert_eq!(not_modified.header("etag"), Some(etag.as_str()));

    // A non-matching conditional miss fills the cache.
    let fresh_target = format!("/fresh?{PUBLIC}");
    let conditional_miss = server.get(&fresh_target, &[("If-None-Match", "\"nope\"")]);
    assert_eq!(conditional_miss.status, 200);
    let full = server.get(&fresh_target, &[]);
    assert_eq!(full.cache(), "HIT");
    assert!(!full.body.is_empty());

    // An application that answers 304 itself still does on a miss; the 304
    // is not stored and the next full response is.
    let app_etag = format!("/app-etag?{PUBLIC}&etag=v1");
    let revalidated = server.get(&app_etag, &[("If-None-Match", "\"v1\"")]);
    assert_eq!(revalidated.status, 304);
    assert_eq!(revalidated.cache(), "MISS");
    assert_eq!(server.get(&app_etag, &[]).cache(), "MISS");
    let stored = server.get(&app_etag, &[]);
    assert_eq!(stored.cache(), "HIT");
    assert_eq!(stored.header("etag"), Some("\"v1\""));
    assert_eq!(
        server.get(&app_etag, &[("If-None-Match", "\"v1\"")]).status,
        304
    );

    let head = request(server.port, "HEAD", &target, &[]);
    assert_eq!(head.status, 200);
    assert_eq!(head.cache(), "HIT");
    assert!(head.body.is_empty());
    assert_eq!(
        head.header("content-length").unwrap(),
        first.body.len().to_string()
    );

    assert!(metric(server.port, "pam_http_response_cache_hits_total") >= 3);
    assert_eq!(
        metric(server.port, "pam_http_response_cache_not_modified_total"),
        2
    );
    assert!(metric(server.port, "pam_http_response_cache_entries") >= 2);
}

#[test]
fn never_stores_or_serves_cookie_and_authorization_requests_by_default() {
    let server = Server::start(&[]);
    let target = format!("/account?{PUBLIC}");
    let alice = server.get(&target, &[("Cookie", "session=alice")]);
    assert_eq!(alice.cache(), "BYPASS");
    assert_eq!(alice.json()["session"], "alice");
    let anonymous = server.get(&target, &[]);
    assert_eq!(
        anonymous.cache(),
        "MISS",
        "the cookie response was not stored"
    );
    assert_eq!(anonymous.json()["session"], Value::Null);
    let bob = server.get(&target, &[("Cookie", "session=bob")]);
    assert_eq!(
        bob.cache(),
        "BYPASS",
        "a stored page is not served to a cookie request"
    );
    assert_eq!(bob.json()["session"], "bob");

    let token = server.get(&target, &[("Authorization", "Bearer alice")]);
    assert_eq!(token.cache(), "BYPASS");
    assert_eq!(token.json()["authorization"], "Bearer alice");
    let empty_cookie = server.get(&target, &[("Cookie", "")]);
    assert_eq!(empty_cookie.cache(), "BYPASS");
    let hit = server.get(&target, &[]);
    assert_eq!(hit.cache(), "HIT");
    assert_eq!(hit.json()["authorization"], Value::Null);
    assert_eq!(hit.json()["session"], Value::Null);
}

#[test]
fn never_stores_private_set_cookie_or_unmarked_responses() {
    let server = Server::start(&[("PAM_PC_ALLOW_COOKIES", "1")]);
    for query in [
        "cc=private,%20max-age=60",
        "cc=public,%20s-maxage=60,%20private",
        "cc=no-store",
        "cc=s-maxage=60,%20no-cache",
        "cc=max-age=60",
        "x=unmarked",
        "cc=public,%20s-maxage=60&setcookie=1",
        "cc=public,%20s-maxage=60&vary=*",
        "cc=public,%20s-maxage=60&status=500",
        "cc=public,%20s-maxage=60&status=401",
        "cc=public,%20s-maxage=60&status=302",
    ] {
        let target = format!("/private?{query}");
        let alice = server.get(&target, &[("Cookie", "session=alice")]);
        let bob = server.get(&target, &[("Cookie", "session=bob")]);
        assert_ne!(bob.cache(), "HIT", "{query}");
        assert_eq!(bob.json()["session"], "bob", "{query}");
        assert!(bob.calls() > alice.calls(), "{query}: bob reached PHP");
        if query.contains("setcookie") {
            assert!(
                bob.header("set-cookie")
                    .unwrap()
                    .contains(&format!("fresh-{}", bob.calls()))
            );
        }
    }
    assert_eq!(metric(server.port, "pam_http_response_cache_entries"), 0);
}

#[test]
fn vary_on_cookie_keeps_sessions_apart_when_cookies_are_allowed() {
    let server = Server::start(&[("PAM_PC_ALLOW_COOKIES", "1")]);
    let target = format!("/dashboard?{PUBLIC}&vary=Cookie");
    for _ in 0..3 {
        for user in ["alice", "bob", "carol"] {
            let response = server.get(&target, &[("Cookie", &format!("session={user}"))]);
            assert_eq!(response.json()["session"], user);
        }
    }
    let anonymous = server.get(&target, &[]);
    assert_eq!(anonymous.json()["session"], Value::Null);
    assert_eq!(anonymous.cache(), "MISS");
    let alice_again = server.get(&target, &[("Cookie", "session=alice")]);
    assert_eq!(alice_again.cache(), "HIT");
    assert_eq!(alice_again.json()["session"], "alice");
}

#[test]
fn response_vary_and_forwarded_headers_partition_the_cache() {
    let server = Server::start(&[]);
    let target = format!("/tenant?{PUBLIC}&vary=X-Tenant,%20Accept-Language");
    let acme = server.get(&target, &[("X-Tenant", "acme"), ("Accept-Language", "en")]);
    let globex = server.get(
        &target,
        &[("X-Tenant", "globex"), ("Accept-Language", "en")],
    );
    let acme_pt = server.get(&target, &[("X-Tenant", "acme"), ("Accept-Language", "pt")]);
    let none = server.get(&target, &[]);
    assert_eq!(globex.json()["tenant"], "globex");
    assert_eq!(acme_pt.json()["language"], "pt");
    assert_eq!(none.json()["tenant"], Value::Null);
    for response in [&globex, &acme_pt, &none] {
        assert_eq!(response.cache(), "MISS");
    }
    let acme_again = server.get(&target, &[("X-Tenant", "acme"), ("Accept-Language", "en")]);
    assert_eq!(acme_again.cache(), "HIT");
    assert_eq!(acme_again.calls(), acme.calls());
    let case = server.get(&target, &[("X-Tenant", "ACME"), ("Accept-Language", "en")]);
    assert_eq!(case.cache(), "MISS", "vary values match byte for byte");

    let poisoned_target = format!("/links?{PUBLIC}");
    let poisoned = server.get(&poisoned_target, &[("X-Forwarded-Host", "evil.test")]);
    assert_eq!(poisoned.json()["forwardedHost"], "evil.test");
    let clean = server.get(&poisoned_target, &[]);
    assert_eq!(
        clean.cache(),
        "MISS",
        "unkeyed forwarded headers cannot poison"
    );
    assert_eq!(clean.json()["forwardedHost"], Value::Null);
    let other_host = server.get(&poisoned_target, &[("Host", "other.test")]);
    assert_eq!(other_host.cache(), "MISS");
    assert_eq!(other_host.json()["host"], "other.test");
}

#[test]
fn query_normalization_never_merges_different_php_inputs() {
    let server = Server::start(&[]);
    let first = server.get(&format!("/q?a=1&b=2&{PUBLIC}"), &[]);
    let reordered = server.get(&format!("/q?{PUBLIC}&b=2&utm_source=mail&a=1"), &[]);
    assert_eq!(reordered.cache(), "HIT");
    assert_eq!(reordered.calls(), first.calls());
    for (left, right) in [
        ("a=1&a=2", "a=2&a=1"),
        ("x[]=1&x[]=2", "x[]=2&x[]=1"),
        ("a.b=1&a_b=2", "a_b=2&a.b=1"),
    ] {
        let left = server.get(&format!("/q?{PUBLIC}&{left}"), &[]);
        let right = server.get(&format!("/q?{PUBLIC}&{right}"), &[]);
        assert_eq!(right.cache(), "MISS", "{left:?}");
        assert_ne!(left.json()["query"], Value::Null);
    }
}

#[test]
fn concurrent_misses_of_different_users_never_cross() {
    let server = Arc::new(Server::start(&[("PAM_PC_ALLOW_COOKIES", "1")]));
    for (round, query) in [
        "cc=private,%20max-age=60&delay=50",
        "cc=public,%20s-maxage=60&vary=Cookie&delay=50",
        "cc=public,%20s-maxage=60&setcookie=1&delay=50",
    ]
    .into_iter()
    .enumerate()
    {
        let barrier = Arc::new(Barrier::new(24));
        let threads: Vec<_> = (0..24)
            .map(|user| {
                let server = server.clone();
                let barrier = barrier.clone();
                let target = format!("/race{round}?{query}");
                thread::spawn(move || {
                    barrier.wait();
                    let session = format!("user{user}");
                    let response =
                        server.get(&target, &[("Cookie", &format!("session={session}"))]);
                    assert_eq!(response.status, 200);
                    assert_eq!(response.json()["session"], session.as_str(), "{target}");
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
    }
}

#[test]
fn concurrent_anonymous_misses_collapse_into_one_php_call() {
    let server = Arc::new(Server::start(&[]));
    let target = format!("/collapse?{PUBLIC}&delay=200");
    let barrier = Arc::new(Barrier::new(32));
    let threads: Vec<_> = (0..32)
        .map(|_| {
            let server = server.clone();
            let barrier = barrier.clone();
            let target = target.clone();
            thread::spawn(move || {
                barrier.wait();
                server.get(&target, &[]).calls()
            })
        })
        .collect();
    let calls: Vec<u64> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    assert!(
        calls.iter().all(|call| *call == calls[0]),
        "one PHP execution served all: {calls:?}"
    );
    assert!(metric(server.port, "pam_http_response_cache_collapsed_total") >= 1);

    // Uncacheable answers stop collapsing: these run concurrently.
    let target = "/no-collapse?cc=private&delay=200";
    server.get(target, &[]);
    let started = Instant::now();
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let server = server.clone();
            thread::spawn(move || server.get(target, &[]).calls())
        })
        .collect();
    let mut calls: Vec<u64> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    calls.sort_unstable();
    calls.dedup();
    assert_eq!(calls.len(), 8, "each uncacheable request reached PHP");
    assert!(
        started.elapsed() < Duration::from_millis(1_200),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn stale_while_revalidate_answers_at_once_and_refreshes_in_the_background() {
    let server = Server::start(&[]);
    let target = "/swr?cc=public,%20s-maxage=1,%20stale-while-revalidate=30&delay=300";
    let first = server.get(target, &[]);
    thread::sleep(Duration::from_millis(1_100));
    let started = Instant::now();
    let stale = server.get(target, &[]);
    assert!(
        started.elapsed() < Duration::from_millis(250),
        "stale answers at once"
    );
    assert_eq!(stale.cache(), "STALE");
    assert_eq!(stale.calls(), first.calls());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let response = server.get(target, &[]);
        if response.cache() == "HIT" && response.calls() > first.calls() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "background revalidation never landed"
        );
        thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(
        metric(server.port, "pam_http_response_cache_revalidations_total"),
        1
    );
}

#[test]
fn failed_revalidations_keep_serving_the_stale_copy() {
    let server = Server::start(&[]);
    let target = "/flaky?cc=public,%20s-maxage=1,%20stale-while-revalidate=30&fail=2";
    let first = server.get(target, &[]);
    thread::sleep(Duration::from_millis(1_100));
    assert_eq!(server.get(target, &[]).cache(), "STALE");
    thread::sleep(Duration::from_millis(200));
    let still = server.get(target, &[]);
    assert_eq!(
        still.cache(),
        "STALE",
        "a 5xx revalidation keeps the stale copy"
    );
    assert_eq!(still.calls(), first.calls());
}

#[test]
fn purges_by_tag_url_and_all_from_php() {
    let server = Server::start(&[]);
    let product = format!("/products/42?{PUBLIC}&tags=products,product:42");
    let list = format!("/products?{PUBLIC}&tags=products");
    let home = format!("/?{PUBLIC}&tags=home");
    for target in [&product, &list, &home] {
        server.get(target, &[]);
        assert_eq!(server.get(target, &[]).cache(), "HIT");
    }
    assert!(
        server.get(&product, &[]).header("pam-cache-tags").is_none(),
        "tags never leak"
    );

    let purged = server.get("/purge?tag=product:42", &[]).json();
    assert_eq!(purged["purged"], 1);
    assert_eq!(server.get(&product, &[]).cache(), "MISS");
    assert_eq!(server.get(&list, &[]).cache(), "HIT");

    let purged = server.get("/purge?url=/products*", &[]).json();
    assert_eq!(purged["purged"], 2);
    assert_eq!(server.get(&list, &[]).cache(), "MISS");
    assert_eq!(server.get(&home, &[]).cache(), "HIT");

    let purged = server.get("/purge?all=1", &[]).json();
    assert!(purged["purged"].as_u64().unwrap() >= 2);
    assert_eq!(server.get(&home, &[]).cache(), "MISS");

    let invalid = server.get("/purge-invalid", &[]).json();
    assert!(
        invalid["error"]
            .as_str()
            .unwrap()
            .contains("invalid cache tag")
    );
    assert!(metric(server.port, "pam_http_response_cache_purges_total") >= 3);
}

#[test]
fn cli_purges_reach_a_running_server_through_the_invalidation_log() {
    let directory = std::env::temp_dir().join(format!("pam-pc-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    let log = directory.join("invalidations.log");
    let server = Server::start(&[("PAM_PC_LOG", log.to_str().unwrap())]);
    let target = format!("/cli?{PUBLIC}&tags=products");
    server.get(&target, &[]);
    assert_eq!(server.get(&target, &[]).cache(), "HIT");
    let output = Command::new(env!("CARGO_BIN_EXE_pam"))
        .args(["cache:purge", "--tag=products", "--log"])
        .arg(&log)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    thread::sleep(Duration::from_millis(150));
    // The first request after the poll interval applies the log.
    let deadline = Instant::now() + Duration::from_secs(2);
    while server.get(&target, &[]).cache() == "HIT" {
        assert!(Instant::now() < deadline, "CLI purge never applied");
        thread::sleep(Duration::from_millis(50));
    }
    let invalid = Command::new(env!("CARGO_BIN_EXE_pam"))
        .args(["cache:purge", "--tag=bad tag", "--log"])
        .arg(&log)
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn precompressed_variants_are_negotiated() {
    let server = Server::start(&[]);
    let target = format!("/big?{PUBLIC}&size=20000");
    let identity = server.get(&target, &[]);
    let deadline = Instant::now() + Duration::from_secs(5);
    let brotli = loop {
        let response = server.get(&target, &[("Accept-Encoding", "gzip, br")]);
        if response.header("content-encoding") == Some("br") {
            break response;
        }
        assert!(Instant::now() < deadline, "brotli variant never appeared");
        thread::sleep(Duration::from_millis(20));
    };
    assert!(brotli.body.len() < identity.body.len() / 4);
    assert!(
        brotli
            .header("vary")
            .unwrap()
            .to_ascii_lowercase()
            .contains("accept-encoding")
    );
    let gzip = server.get(&target, &[("Accept-Encoding", "gzip")]);
    assert_eq!(gzip.header("content-encoding"), Some("gzip"));
    let plain = server.get(&target, &[("Accept-Encoding", "identity")]);
    assert_eq!(plain.header("content-encoding"), None);
    assert_eq!(plain.body, identity.body);
}

#[test]
fn memory_stays_bounded_under_a_cache_busting_load() {
    let max_bytes = 256 * 1024;
    let server = Server::start(&[
        ("PAM_PC_MAX_BYTES", &max_bytes.to_string()),
        ("PAM_PC_COMPRESS", "0"),
    ]);
    for index in 0..3_000 {
        let response = server.get(&format!("/bust/{index}?{PUBLIC}&size=4000"), &[]);
        assert_eq!(response.status, 200);
    }
    let bytes = metric(server.port, "pam_http_response_cache_bytes");
    assert!(bytes <= max_bytes, "{bytes} > {max_bytes}");
    assert!(metric(server.port, "pam_http_response_cache_evictions_total") > 2_500);
}

#[test]
fn purges_reach_every_worker_of_a_cluster() {
    let port = free_port();
    let cluster_token = token(port);
    let child = Command::new(env!("CARGO_BIN_EXE_pam"))
        .args(["start"])
        .arg(fixture())
        .args(["--workers", "3", "--startup-timeout", "10000"])
        .env("PAM_TEST_PORT", port.to_string())
        .env("PAM_PC_TOKEN", &cluster_token)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut server = Server { child, port };
    wait_for_port(port, &mut server.child, &cluster_token);
    let target = format!("/shared?{PUBLIC}&tags=catalog");

    // Warm every worker: new connections spread over the workers.
    let mut warmed = std::collections::HashMap::new();
    let deadline = Instant::now() + Duration::from_secs(15);
    while warmed.len() < 3 {
        assert!(
            Instant::now() < deadline,
            "only {} workers answered",
            warmed.len()
        );
        let response = server.get(&target, &[]);
        let body = response.json();
        warmed.insert(
            body["worker"].as_u64().unwrap(),
            body["calls"].as_u64().unwrap(),
        );
    }
    let before: std::collections::HashSet<(u64, u64)> = warmed.into_iter().collect();

    let purged = server.get("/purge?tag=catalog", &[]).json();
    assert_eq!(purged["purged"], 1, "the purging worker dropped its copy");
    thread::sleep(Duration::from_millis(250));
    for _ in 0..60 {
        let body = server.get(&target, &[]).json();
        let served = (
            body["worker"].as_u64().unwrap(),
            body["calls"].as_u64().unwrap(),
        );
        assert!(
            !before.contains(&served),
            "worker {} served a purged page",
            served.0
        );
    }
}
