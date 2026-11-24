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
use std::future::Future;
use std::net::SocketAddr;
use std::os::fd::RawFd;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

static ENABLED: AtomicBool = AtomicBool::new(false);
static NEXT_CONNECTION: AtomicU64 = AtomicU64::new(1);

/// Plain TCP connections this worker serves, by connection id. A peer
/// address is not unique: the same peer can reach two listeners, and a port
/// can come back before the old connection's registration is dropped
/// (audit B3).
static CONNECTIONS: LazyLock<Mutex<HashMap<u64, (SocketAddr, RawFd)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

tokio::task_local! {
    /// The connection whose task is running (HTTP/1 requests run inside it).
    static CONNECTION: u64;
}

/// The request whose PHP code is running right now, if any.
static CALL: Mutex<Option<RunningCall>> = Mutex::new(None);

const REQUEST_ID_BYTES: usize = 96;

struct RunningCall {
    deadline_millis: u64,
    connection: Option<u64>,
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
pub struct Registration(Option<u64>);

impl Registration {
    pub fn new(remote: SocketAddr, fd: RawFd) -> Self {
        if !enabled() {
            return Self(None);
        }
        let id = NEXT_CONNECTION.fetch_add(1, Ordering::Relaxed);
        lock(&CONNECTIONS).insert(id, (remote, fd));
        Self(Some(id))
    }

    /// Runs the connection's future with its id visible to [`PhpCall`].
    pub async fn scope<F: Future>(self, future: F) -> F::Output {
        match self.0 {
            Some(id) => CONNECTION.scope(id, future).await,
            None => future.await,
        }
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        if let Some(id) = self.0 {
            lock(&CONNECTIONS).remove(&id);
        }
    }
}

/// The descriptor of a running call's connection: by id when the call ran
/// inside its connection's task, else by peer address when exactly one
/// connection has it (an HTTP/2 stream runs in a task of its own).
fn connection_fd(call: &RunningCall) -> Option<RawFd> {
    let connections = lock(&CONNECTIONS);
    if let Some(id) = call.connection {
        return connections.get(&id).map(|(_, fd)| *fd);
    }
    let mut matches = connections
        .values()
        .filter(|(remote, _)| *remote == call.remote);
    let first = matches.next().map(|(_, fd)| *fd);
    if matches.next().is_some() {
        return None;
    }
    first
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
            connection: CONNECTION.try_with(|id| *id).ok(),
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
                    if let Some(fd) = connection_fd(&expired) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn call(connection: Option<u64>, remote: SocketAddr) -> RunningCall {
        RunningCall {
            deadline_millis: 0,
            connection,
            remote,
            http10: false,
            http1: true,
            request_id: [0; REQUEST_ID_BYTES],
            request_id_len: 0,
        }
    }

    #[test]
    fn connections_with_the_same_peer_never_overwrite_each_other() {
        enable();
        let remote: SocketAddr = "192.0.2.10:40000".parse().unwrap();
        let first = Registration::new(remote, 1_001);
        let second = Registration::new(remote, 1_002);
        let (Some(first_id), Some(second_id)) = (first.0, second.0) else {
            panic!("registrations are enabled");
        };
        assert_ne!(first_id, second_id);
        assert_eq!(connection_fd(&call(Some(first_id), remote)), Some(1_001));
        assert_eq!(connection_fd(&call(Some(second_id), remote)), Some(1_002));
        // Without a task id two connections of one peer are ambiguous:
        // nothing is shut down rather than the wrong socket.
        assert_eq!(connection_fd(&call(None, remote)), None);

        // The old connection going away keeps the new one known.
        drop(first);
        assert_eq!(connection_fd(&call(Some(second_id), remote)), Some(1_002));
        assert_eq!(connection_fd(&call(None, remote)), Some(1_002));
        drop(second);
        assert_eq!(connection_fd(&call(Some(second_id), remote)), None);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn calls_inside_a_connection_task_carry_its_id() {
        enable();
        let remote: SocketAddr = "192.0.2.11:40001".parse().unwrap();
        let registration = Registration::new(remote, 1_003);
        let expected = registration.0;
        let seen = registration
            .scope(async { CONNECTION.try_with(|id| *id).ok() })
            .await;
        assert_eq!(seen, expected);
    }
}
