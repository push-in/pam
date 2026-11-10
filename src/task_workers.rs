//! Task workers: PHP workers that run background work (Octane's
//! `Octane::concurrently()`, task dispatch and ticks) instead of serving
//! public HTTP traffic.
//!
//! The master starts `--task-workers N` extra workers that listen on a private
//! loopback address shared with `SO_REUSEPORT`. Every request to it carries a
//! random per-cluster token. HTTP workers send tasks there from PHP through
//! `pam_native_task_resolve()` / `pam_native_task_dispatch()`; the requests run
//! concurrently on a small Tokio runtime that is created on first use, so a
//! worker that never dispatches a task pays nothing. The master sends one tick
//! per second to the pool, skipping a tick while the previous one still runs.

use std::net::SocketAddr;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};

pub const ADDRESS_ENV: &str = "PAM_TASK_ADDRESS";
pub const TOKEN_ENV: &str = "PAM_TASK_TOKEN";
pub const ROLE_ENV: &str = "PAM_TASK_WORKER";
pub const TOKEN_HEADER: &str = "x-pam-task-token";
pub const KIND_HEADER: &str = "x-pam-task";
/// Largest task result accepted back from a task worker.
const MAX_RESULT_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Run,
    Dispatch,
    Tick,
}

impl Kind {
    fn header(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::Dispatch => "dispatch",
            Self::Tick => "tick",
        }
    }
}

#[derive(Clone)]
pub struct Endpoint {
    pub address: SocketAddr,
    pub token: String,
}

impl Endpoint {
    /// The task pool of this worker's cluster, if it has one.
    pub fn from_environment() -> Option<Self> {
        let address = std::env::var(ADDRESS_ENV).ok()?.parse().ok()?;
        let token = std::env::var(TOKEN_ENV)
            .ok()
            .filter(|token| token.len() >= 32)?;
        Some(Self { address, token })
    }
}

/// Sends one task request and returns the response body.
pub async fn post(endpoint: &Endpoint, kind: Kind, body: Bytes) -> Result<Bytes, String> {
    post_notifying(endpoint, kind, body, None).await
}

type Connected = mpsc::Sender<Result<(), String>>;

async fn post_notifying(
    endpoint: &Endpoint,
    kind: Kind,
    body: Bytes,
    connected: Option<Connected>,
) -> Result<Bytes, String> {
    let stream = match tokio::net::TcpStream::connect(endpoint.address).await {
        Ok(stream) => stream,
        Err(error) => {
            let error = format!("cannot reach the task workers: {error}");
            if let Some(connected) = connected {
                let _ = connected.send(Err(error.clone()));
            }
            return Err(error);
        }
    };
    if let Some(connected) = connected {
        let _ = connected.send(Ok(()));
    }
    let _ = stream.set_nodelay(true);
    let (mut sender, connection) =
        hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(stream))
            .await
            .map_err(|error| format!("task worker handshake failed: {error}"))?;
    let connection = tokio::spawn(async move {
        let _ = connection.await;
    });
    let request = http::Request::post("/")
        .header(http::header::HOST, "pam-task-workers")
        .header(KIND_HEADER, kind.header())
        .header(TOKEN_HEADER, endpoint.token.as_str())
        .header(http::header::CONTENT_TYPE, "application/octet-stream")
        .body(Full::new(body))
        .map_err(|error| error.to_string())?;
    let result = async {
        let response = sender
            .send_request(request)
            .await
            .map_err(|error| format!("task request failed: {error}"))?;
        let status = response.status();
        let body = http_body_util::Limited::new(response.into_body(), MAX_RESULT_BYTES)
            .collect()
            .await
            .map_err(|error| format!("cannot read the task result: {error}"))?
            .to_bytes();
        if !status.is_success() {
            return Err(format!("task worker answered {status}"));
        }
        Ok(body)
    }
    .await;
    connection.abort();
    result
}

/// What happened to one task of `resolve`.
pub enum Outcome {
    Finished(Bytes),
    /// Still running when the wait ended (Octane reports `false`).
    Pending,
    Failed(String),
}

fn runtime() -> Result<&'static tokio::runtime::Runtime, String> {
    static RUNTIME: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .thread_name("pam-tasks")
                .enable_all()
                .build()
                .map_err(|error| format!("cannot start the task client: {error}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}

static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

struct InFlight;

impl InFlight {
    fn start() -> Self {
        IN_FLIGHT.fetch_add(1, Ordering::AcqRel);
        Self
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        IN_FLIGHT.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Runs every payload on the task workers concurrently and waits up to
/// `wait` for the results.
pub fn resolve(
    endpoint: &Endpoint,
    payloads: Vec<Bytes>,
    wait: Duration,
) -> Result<Vec<Outcome>, String> {
    let runtime = runtime()?;
    let count = payloads.len();
    let (sender, receiver) = mpsc::channel();
    for (index, payload) in payloads.into_iter().enumerate() {
        let endpoint = endpoint.clone();
        let sender = sender.clone();
        runtime.spawn(async move {
            // Not counted as in flight: a result nobody waits for any more
            // does not delay the worker's shutdown.
            let result = post(&endpoint, Kind::Run, payload).await;
            let _ = sender.send((index, result));
        });
    }
    drop(sender);
    let mut outcomes: Vec<Outcome> = (0..count).map(|_| Outcome::Pending).collect();
    let deadline = Instant::now() + wait;
    let mut remaining = count;
    while remaining > 0 {
        let Some(left) = deadline.checked_duration_since(Instant::now()) else {
            break;
        };
        match receiver.recv_timeout(left) {
            Ok((index, result)) => {
                outcomes[index] = match result {
                    Ok(body) => Outcome::Finished(body),
                    Err(error) => Outcome::Failed(error),
                };
                remaining -= 1;
            }
            Err(_) => break,
        }
    }
    Ok(outcomes)
}

/// Queues every payload on the task workers without waiting for them to run.
/// Returns once each connection is established.
pub fn dispatch(endpoint: &Endpoint, payloads: Vec<Bytes>) -> Result<(), String> {
    let runtime = runtime()?;
    let (sender, receiver) = mpsc::channel();
    let count = payloads.len();
    for payload in payloads {
        let endpoint = endpoint.clone();
        let sender = sender.clone();
        let in_flight = InFlight::start();
        runtime.spawn(async move {
            let _in_flight = in_flight;
            let reached = sender.clone();
            drop(sender);
            if let Err(error) =
                post_notifying(&endpoint, Kind::Dispatch, payload, Some(reached)).await
                && !error.starts_with("cannot reach")
            {
                eprintln!("pam: dispatched task failed: {error}");
            }
        });
    }
    drop(sender);
    for _ in 0..count {
        receiver
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "timed out reaching the task workers".to_owned())??;
    }
    Ok(())
}

/// Waits up to `timeout` for dispatched tasks to be delivered (worker shutdown).
pub fn flush(timeout: Duration) -> usize {
    let deadline = Instant::now() + timeout;
    while IN_FLIGHT.load(Ordering::Acquire) > 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    IN_FLIGHT.load(Ordering::Acquire)
}

/// One tick per interval to the task pool, never two at once.
pub struct Ticker {
    endpoint: Endpoint,
    busy: std::sync::Arc<AtomicBool>,
}

impl Ticker {
    pub fn new(endpoint: Endpoint) -> Self {
        Self {
            endpoint,
            busy: std::sync::Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn tick(&self) {
        if self.busy.swap(true, Ordering::AcqRel) {
            return;
        }
        let endpoint = self.endpoint.clone();
        let busy = self.busy.clone();
        tokio::spawn(async move {
            let _ = tokio::time::timeout(
                Duration::from_secs(300),
                post(&endpoint, Kind::Tick, Bytes::new()),
            )
            .await;
            busy.store(false, Ordering::Release);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A minimal task worker: echoes the body reversed after `delay`.
    async fn fake_pool(delay: Duration) -> (SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    let mut buffer = [0_u8; 4096];
                    let body = loop {
                        let read = stream.read(&mut buffer).await.unwrap();
                        if read == 0 {
                            return;
                        }
                        request.extend_from_slice(&buffer[..read]);
                        let text = String::from_utf8_lossy(&request).to_string();
                        if let Some(end) = text.find("\r\n\r\n") {
                            let length: usize = text
                                .lines()
                                .find_map(|line| {
                                    line.to_ascii_lowercase()
                                        .strip_prefix("content-length: ")
                                        .map(|v| v.trim().parse().unwrap())
                                })
                                .unwrap_or(0);
                            if request.len() >= end + 4 + length {
                                assert!(text.to_ascii_lowercase().contains(
                                    "x-pam-task-token: 0123456789abcdef0123456789abcdef"
                                ));
                                break request[end + 4..end + 4 + length].to_vec();
                            }
                        }
                    };
                    tokio::time::sleep(delay).await;
                    let reply: Vec<u8> = body.iter().rev().copied().collect();
                    let head =
                        format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n", reply.len());
                    let _ = stream.write_all(head.as_bytes()).await;
                    let _ = stream.write_all(&reply).await;
                });
            }
        });
        (address, task)
    }

    fn endpoint(address: SocketAddr) -> Endpoint {
        Endpoint {
            address,
            token: "0123456789abcdef0123456789abcdef".to_owned(),
        }
    }

    #[test]
    fn resolves_tasks_concurrently_and_reports_unfinished_ones() {
        let pool = tokio::runtime::Runtime::new().unwrap();
        let (address, _task) = pool.block_on(fake_pool(Duration::from_millis(200)));
        let started = Instant::now();
        let payloads = (0..8)
            .map(|index| Bytes::from(format!("task-{index}")))
            .collect();
        let outcomes = resolve(&endpoint(address), payloads, Duration::from_secs(5)).unwrap();
        assert!(
            started.elapsed() < Duration::from_millis(1500),
            "tasks ran sequentially"
        );
        for (index, outcome) in outcomes.into_iter().enumerate() {
            match outcome {
                Outcome::Finished(body) => assert_eq!(
                    body,
                    Bytes::from(format!("task-{index}").chars().rev().collect::<String>())
                ),
                _ => panic!("task {index} did not finish"),
            }
        }
        let outcomes = resolve(
            &endpoint(address),
            vec![Bytes::from_static(b"slow")],
            Duration::from_millis(20),
        )
        .unwrap();
        assert!(matches!(outcomes[0], Outcome::Pending));
        assert_eq!(flush(Duration::from_secs(2)), 0);
    }

    #[test]
    fn unreachable_pools_fail_instead_of_hanging() {
        let unused = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let outcomes = resolve(
            &endpoint(unused),
            vec![Bytes::from_static(b"x")],
            Duration::from_secs(2),
        )
        .unwrap();
        assert!(matches!(outcomes[0], Outcome::Failed(_)));
        assert!(dispatch(&endpoint(unused), vec![Bytes::from_static(b"x")]).is_err());
    }
}
