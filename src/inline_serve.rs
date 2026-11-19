//! Inline serving: HTTP connections and PHP on one thread.
//!
//! By default a worker runs Tokio's current-thread runtime: hyper reads the
//! request, PHP runs it and hyper writes the response on the same thread, so a
//! request costs no thread handoff (no futex wake-ups, no second epoll, no
//! cache lines moving between cores). `PAM_IO_THREADS=1..16` brings back the
//! separate I/O threads; TLS listeners always use them.
//!
//! The one thing a separate I/O thread did that this thread cannot do is to
//! answer a request whose PHP code blocks the thread past its hard deadline
//! (`sleep()`, a blocking call that never returns). A watchdog thread covers
//! it: while PHP code runs synchronously, [`PhpCall`] records the request's
//! deadline and connection; past deadline + grace the watchdog writes the
//! same `504 Gateway Timeout` the transport writes, closes the connection and
//! reports the worker wedged so it is replaced, as with I/O threads.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::os::fd::RawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

static ENABLED: AtomicBool = AtomicBool::new(false);

/// Plain TCP connections this worker serves, by peer address.
static CONNECTIONS: LazyLock<Mutex<HashMap<SocketAddr, RawFd>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The request whose PHP code is running right now, if any.
static CALL: Mutex<Option<RunningCall>> = Mutex::new(None);

const REQUEST_ID_BYTES: usize = 96;

struct RunningCall {
    deadline_millis: u64,
    remote: SocketAddr,
    http10: bool,
    http1: bool,
    request_id: [u8; REQUEST_ID_BYTES],
    request_id_len: usize,
}

pub fn enable() {
    ENABLED.store(true, Ordering::Release);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Keeps a connection's descriptor known to the watchdog while it is served.
pub struct Registration(Option<SocketAddr>);

impl Registration {
    pub fn new(remote: SocketAddr, fd: RawFd) -> Self {
        if !enabled() {
            return Self(None);
        }
        lock(&CONNECTIONS).insert(remote, fd);
        Self(Some(remote))
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        if let Some(remote) = self.0 {
            lock(&CONNECTIONS).remove(&remote);
        }
    }
}

/// Marks PHP code of a request running synchronously on the worker thread.
pub struct PhpCall(bool);

impl PhpCall {
    pub fn enter(
        deadline_millis: u64,
        remote: SocketAddr,
        version: http::Version,
        request_id: &str,
    ) -> Self {
        if !enabled() {
            return Self(false);
        }
        let mut id = [0_u8; REQUEST_ID_BYTES];
        let len = request_id.len().min(REQUEST_ID_BYTES);
        id[..len].copy_from_slice(&request_id.as_bytes()[..len]);
        *lock(&CALL) = Some(RunningCall {
            deadline_millis,
            remote,
            http10: version == http::Version::HTTP_10,
            http1: matches!(version, http::Version::HTTP_10 | http::Version::HTTP_11),
            request_id: id,
            request_id_len: len,
        });
        Self(true)
    }
}

impl Drop for PhpCall {
    fn drop(&mut self) {
        if self.0 {
            *lock(&CALL) = None;
        }
    }
}

/// Starts the watchdog. `wedged` runs once per blocked request, after its
/// client was answered.
pub fn spawn_watchdog(grace_millis: u64, wedged: impl Fn(&str) + Send + 'static) {
    let spawned = std::thread::Builder::new()
        .name("pam-watchdog".to_owned())
        .spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(50));
                let now = crate::worker_state::epoch_millis();
                let expired = {
                    let mut call = lock(&CALL);
                    if !call.as_ref().is_some_and(|call| {
                        now >= call.deadline_millis.saturating_add(grace_millis)
                    }) {
                        continue;
                    }
                    let Some(expired) = call.take() else {
                        continue;
                    };
                    // `CALL` is still locked, so the PHP call has not
                    // returned and its connection (owned by the blocked
                    // thread) is still open.
                    if let Some(fd) = lock(&CONNECTIONS).get(&expired.remote).copied() {
                        if expired.http1 {
                            write_gateway_timeout(fd, expired.http10);
                        }
                        // SAFETY: `fd` is an open socket of this process.
                        unsafe {
                            libc::shutdown(fd, libc::SHUT_RDWR);
                        }
                    }
                    expired
                };
                wedged(&String::from_utf8_lossy(
                    &expired.request_id[..expired.request_id_len],
                ));
            }
        });
    if let Err(error) = spawned {
        eprintln!("pam: cannot start the request watchdog: {error}");
    }
}

fn write_gateway_timeout(fd: RawFd, http10: bool) {
    let body = r#"{"error":"Gateway Timeout","message":"request execution exceeded its timeout"}"#;
    let response = format!(
        "{} 504 Gateway Timeout\r\ncontent-type: application/json; charset=utf-8\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{body}",
        if http10 { "HTTP/1.0" } else { "HTTP/1.1" },
        body.len(),
    );
    let mut written = 0;
    let bytes = response.as_bytes();
    while written < bytes.len() {
        // SAFETY: valid buffer; `fd` is an open socket (see the caller).
        let sent = unsafe {
            libc::send(
                fd,
                bytes[written..].as_ptr().cast(),
                bytes.len() - written,
                libc::MSG_NOSIGNAL,
            )
        };
        if sent <= 0 {
            break;
        }
        written += sent as usize;
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
