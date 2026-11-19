//! Accept loop and lossless connection draining for HTTP workers.
//!
//! A worker that stops (reload, `--max-requests` recycle, `pam stop`) must not
//! drop a request a client already sent. Hyper's graceful shutdown closes every
//! HTTP/1 connection that is idle *from hyper's point of view*, which includes a
//! connection accepted a moment ago whose first request is still in the socket
//! buffer, and a keep-alive connection whose next request is in flight. Both
//! end up reset.
//!
//! Draining here works in two steps instead:
//!
//! 1. When the drain starts, every response written from then on carries
//!    `Connection: close` (see [`close_when_draining`]), so each HTTP/1
//!    connection serves the request it is processing or the next one it
//!    receives and then closes cleanly. Clients reconnect, and new connections
//!    reach the replacement worker. Each connection gets an idle grace period
//!    to send that request.
//! 2. Only connections that stayed idle for the whole grace period, plus
//!    HTTP/2 connections, receive hyper's graceful shutdown (HTTP/2 `GOAWAY`).
//!
//! The accept loop keeps accepting while the drain starts (the master already
//! routes new work to the replacement). In a cluster the listener is also held
//! by the master and adopted by the replacement (see `listener_registry`), so
//! the worker only drops its copy and the queue stays with the replacement.
//! Without a master, it sweeps every connection the kernel queued on the
//! listener before closing it.

use std::convert::Infallible;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::body::HttpBody as _;
use axum::extract::{ConnectInfo, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use http::{HeaderValue, Method, StatusCode, Version, header};
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, watch};

/// Set when this worker's listener is also held by the cluster master.
static LISTENER_SHARED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn mark_listener_shared() {
    LISTENER_SHARED.store(true, Ordering::Release);
}

/// How long a draining connection may stay idle before it is closed. Long
/// enough for a client that holds a keep-alive connection, or that just
/// connected, to send the request it is about to send.
const DEFAULT_DRAIN_IDLE: Duration = Duration::from_millis(1_000);

/// The drain state of one worker process.
pub struct Drain {
    started: watch::Sender<bool>,
    wedged: watch::Sender<bool>,
    idle_grace: Duration,
}

impl Drain {
    pub fn new() -> Arc<Self> {
        let idle_grace = std::env::var("PAM_DRAIN_IDLE_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .map_or(DEFAULT_DRAIN_IDLE, |millis| {
                Duration::from_millis(millis.min(30_000))
            });
        Arc::new(Self {
            started: watch::channel(false).0,
            wedged: watch::channel(false).0,
            idle_grace,
        })
    }

    pub fn start(&self) {
        self.started.send_replace(true);
    }

    pub fn is_draining(&self) -> bool {
        *self.started.borrow()
    }

    pub fn subscribe(&self) -> watch::Receiver<bool> {
        self.started.subscribe()
    }

    /// Records that the PHP thread is stuck in a request past its hard
    /// deadline. Returns true for the first caller only.
    pub fn mark_wedged(&self) -> bool {
        !self.wedged.send_replace(true)
    }

    pub fn is_wedged(&self) -> bool {
        *self.wedged.borrow()
    }

    pub fn subscribe_wedged(&self) -> watch::Receiver<bool> {
        self.wedged.subscribe()
    }

    pub fn idle_grace(&self) -> Duration {
        if self.is_wedged() {
            // A wedged worker cannot serve another request.
            Duration::ZERO
        } else {
            self.idle_grace
        }
    }
}

/// Resolves once the watched flag becomes true, without holding the guard.
pub async fn raised(flag: &mut watch::Receiver<bool>) {
    let _ = flag.wait_for(|raised| *raised).await;
}

/// Adds `Connection: close` to HTTP/1 responses once the worker drains, so a
/// keep-alive client finishes its current exchange and reconnects elsewhere
/// instead of racing a connection close.
pub async fn close_when_draining(
    State(drain): State<Arc<Drain>>,
    request: Request,
    next: Next,
) -> Response {
    let http1 = matches!(request.version(), Version::HTTP_10 | Version::HTTP_11);
    let mut response = next.run(request).await;
    if http1 && drain.is_draining() && response.status() != StatusCode::SWITCHING_PROTOCOLS {
        response
            .headers_mut()
            .insert(header::CONNECTION, HeaderValue::from_static("close"));
    }
    response
}

/// HTTP settings shared by accepted and handed-off connections.
#[derive(Clone, Copy)]
pub struct HttpSettings {
    pub header_read_timeout: Duration,
    pub graceful_timeout: Duration,
}

/// Counts connections still being served so shutdown can wait for them.
#[derive(Clone, Default)]
pub struct ConnectionTracker {
    inner: Arc<(AtomicUsize, Notify)>,
}

pub struct ActiveConnection {
    inner: Arc<(AtomicUsize, Notify)>,
}

impl ConnectionTracker {
    pub fn begin(&self) -> ActiveConnection {
        self.inner.0.fetch_add(1, Ordering::AcqRel);
        ActiveConnection {
            inner: self.inner.clone(),
        }
    }

    /// Waits until every tracked connection finished or `timeout` elapses.
    pub async fn wait_idle(&self, timeout: Duration) {
        let _ = tokio::time::timeout(timeout, async {
            loop {
                let notified = self.inner.1.notified();
                if self.inner.0.load(Ordering::Acquire) == 0 {
                    return;
                }
                notified.await;
            }
        })
        .await;
    }
}

impl Drop for ActiveConnection {
    fn drop(&mut self) {
        if self.inner.0.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.inner.1.notify_waiters();
        }
    }
}

/// A response being produced by [`FastDispatch`].
pub type DispatchFuture = Pin<Box<dyn Future<Output = Response> + Send>>;

/// Handles a request without the router, or gives it back (`Err`) for the
/// router to serve. Behaves like a top-level fallback route of the router:
/// [`ConnectionService`] applies the same response rules axum's route future
/// and [`close_when_draining`] apply.
pub type FastDispatch =
    Arc<dyn Fn(Request, SocketAddr) -> Result<DispatchFuture, Request> + Send + Sync>;

/// The router of a worker plus an optional direct path for most requests:
/// axum's routing clones boxed services and allocates several futures per
/// request, which a server with one fallback for every path does not need.
#[derive(Clone)]
pub struct HttpApp {
    router: Router,
    fast: Option<FastDispatch>,
}

impl HttpApp {
    pub fn new(router: Router, fast: Option<FastDispatch>) -> Self {
        Self { router, fast }
    }
}

impl From<Router> for HttpApp {
    fn from(router: Router) -> Self {
        Self { router, fast: None }
    }
}

/// The hyper service of one connection.
#[derive(Clone)]
struct ConnectionService {
    router: hyper_util::service::TowerToHyperService<Router>,
    fast: Option<FastDispatch>,
    remote: SocketAddr,
    drain: Arc<Drain>,
}

type ServiceFuture = Pin<Box<dyn Future<Output = Result<Response, Infallible>> + Send>>;

impl hyper::service::Service<http::Request<hyper::body::Incoming>> for ConnectionService {
    type Response = Response;
    type Error = Infallible;
    type Future = ServiceFuture;

    fn call(&self, request: http::Request<hyper::body::Incoming>) -> Self::Future {
        let mut request = request.map(Body::new);
        if let Some(fast) = &self.fast {
            let method = request.method().clone();
            let http1 = matches!(request.version(), Version::HTTP_10 | Version::HTTP_11);
            match fast(request, self.remote) {
                Ok(response) => {
                    let drain = self.drain.clone();
                    return Box::pin(async move {
                        let mut response = response.await;
                        finish_route_response(&method, &mut response);
                        if http1
                            && drain.is_draining()
                            && response.status() != StatusCode::SWITCHING_PROTOCOLS
                        {
                            response
                                .headers_mut()
                                .insert(header::CONNECTION, HeaderValue::from_static("close"));
                        }
                        Ok(response)
                    });
                }
                Err(returned) => request = returned,
            }
        }
        request.extensions_mut().insert(ConnectInfo(self.remote));
        Box::pin(hyper::service::Service::call(&self.router, request))
    }
}

/// What axum's top-level route future does to a response: no body or framing
/// headers on a successful CONNECT, `Content-Length` from an exact body size,
/// and no body for HEAD.
fn finish_route_response(method: &Method, response: &mut Response) {
    if *method == Method::CONNECT && response.status().is_success() {
        if response.headers().contains_key(header::CONTENT_LENGTH)
            || response.headers().contains_key(header::TRANSFER_ENCODING)
            || response.body().size_hint().lower() != 0
        {
            *response.body_mut() = Body::empty();
        }
        return;
    }
    if !response.headers().contains_key(header::CONTENT_LENGTH)
        && let Some(size) = response.body().size_hint().exact()
    {
        let value = if size == 0 {
            HeaderValue::from_static("0")
        } else {
            HeaderValue::from(size)
        };
        response.headers_mut().insert(header::CONTENT_LENGTH, value);
    }
    if *method == Method::HEAD {
        *response.body_mut() = Body::empty();
    }
}

/// Serves one connection until it ends, draining it as described in the
/// module documentation.
pub async fn serve_connection<I>(
    io: I,
    remote: SocketAddr,
    app: HttpApp,
    drain: Arc<Drain>,
    http: HttpSettings,
) where
    I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let service = ConnectionService {
        router: hyper_util::service::TowerToHyperService::new(app.router),
        fast: app.fast,
        remote,
        drain: drain.clone(),
    };
    let mut builder = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new());
    builder
        .http1()
        .timer(TokioTimer::new())
        .header_read_timeout(http.header_read_timeout);
    builder
        .http2()
        .timer(TokioTimer::new())
        .keep_alive_interval(Some(Duration::from_secs(15)))
        .keep_alive_timeout(Duration::from_secs(10));
    let connection = builder.serve_connection_with_upgrades(TokioIo::new(io), service);
    tokio::pin!(connection);
    let mut draining = drain.subscribe();
    tokio::select! {
        _ = connection.as_mut() => return,
        _ = raised(&mut draining) => {}
    }
    // HTTP/1 connections close by themselves after their next response.
    let grace = drain.idle_grace();
    if !grace.is_zero()
        && tokio::time::timeout(grace, connection.as_mut())
            .await
            .is_ok()
    {
        return;
    }
    connection.as_mut().graceful_shutdown();
    let _ = tokio::time::timeout(http.graceful_timeout, connection).await;
}

/// Accepts connections until `stop` is raised, then sweeps the listener's
/// queue, closes it and waits for every connection to finish.
///
/// `place` decides whether an accepted stream is served here (returning the
/// stream to serve) or was moved to a sibling worker (returning `None`).
/// `handshake` wraps the stream (TLS) before HTTP is served.
#[allow(clippy::too_many_arguments)]
pub async fn accept_loop<P, S, H, F, T>(
    listener: TcpListener,
    place: P,
    handshake: H,
    app: HttpApp,
    drain: Arc<Drain>,
    http: HttpSettings,
    tracker: ConnectionTracker,
    mut stop: watch::Receiver<bool>,
) where
    P: Fn(TcpStream) -> Option<S>,
    S: Send + 'static,
    H: Fn(S) -> F + Clone + Send + 'static,
    F: Future<Output = io::Result<T>> + Send + 'static,
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let serve = |stream: TcpStream, remote: SocketAddr| {
        let _ = stream.set_nodelay(true);
        let Some(stream) = place(stream) else {
            return;
        };
        let active = tracker.begin();
        let handshake = handshake.clone();
        let app = app.clone();
        let drain = drain.clone();
        tokio::spawn(async move {
            let _active = active;
            if let Ok(io) = handshake(stream).await {
                serve_connection(io, remote, app, drain, http).await;
            }
        });
    };
    loop {
        tokio::select! {
            biased;
            _ = raised(&mut stop) => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, remote)) => serve(stream, remote),
                Err(error) if is_connection_error(&error) => {}
                Err(error) => {
                    eprintln!("pam: accept failed: {error}");
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            },
        }
    }
    // A listener the master keeps open (see `listener_registry`) is not
    // closed by this worker: its queue stays with the next incarnation, which
    // is already accepting from it.
    if LISTENER_SHARED.load(Ordering::Acquire) {
        drop(listener);
        tracker
            .wait_idle(http.graceful_timeout.saturating_add(drain.idle_grace))
            .await;
        return;
    }
    // Take everything the kernel already queued before the listener closes.
    // `accept()` on the Tokio listener only tries the syscall when the
    // reactor already saw readiness, so sweep with plain non-blocking
    // accept(2) calls until the queue is empty.
    if let Ok(listener) = listener.into_std() {
        loop {
            match listener.accept() {
                Ok((stream, remote)) => {
                    if stream.set_nonblocking(true).is_ok()
                        && let Ok(stream) = TcpStream::from_std(stream)
                    {
                        serve(stream, remote);
                    }
                }
                Err(error) if is_connection_error(&error) => {}
                Err(_) => break,
            }
        }
    }
    tracker
        .wait_idle(http.graceful_timeout.saturating_add(drain.idle_grace))
        .await;
}

fn is_connection_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::ConnectionRefused
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::Interrupted
    )
}

/// The body of a response that closes the connection after a hard timeout.
pub fn closing(mut response: Response<Body>) -> Response<Body> {
    response
        .headers_mut()
        .insert(header::CONNECTION, HeaderValue::from_static("close"));
    response
}
