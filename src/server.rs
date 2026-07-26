use std::collections::HashMap;
use std::convert::Infallible;
use std::fmt;
use std::future::Future;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::os::fd::{AsRawFd, RawFd};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::{Body, Bytes, HttpBody, to_bytes};
use axum::extract::{ConnectInfo, DefaultBodyLimit, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri, Version};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use bytes::Buf;
use futures_util::{SinkExt, StreamExt};
use hmac::{Hmac, Mac};
use hyper_util::rt::TokioTimer;
use quinn::crypto::rustls::QuicServerConfig;
use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use tokio::io::Interest;
use tokio::io::unix::AsyncFd;
use tokio::sync::{Mutex, OwnedSemaphorePermit, RwLock, Semaphore, mpsc, watch};
use yawc::close::CloseCode;
use yawc::frame::{Frame, OpCode};
use yawc::{CompressionLevel, IncomingUpgrade, Options, WebSocket};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use socket2::{Domain, Protocol, Socket, Type};

use crate::php::{self, ServerConfig};
use crate::terminal::Terminal;
use crate::worker_state::{
    WorkerLifecycle, WorkerMetricsSnapshot, WorkerStateReporter,
    epoch_millis as worker_epoch_millis,
};

type ClientSender = mpsc::Sender<Frame>;
type ResumeHmac = Hmac<Sha256>;
static NEXT_ATOMIC_FILE_ID: AtomicU64 = AtomicU64::new(1);
const SIGTERM: i32 = 15;
const SIGKILL: i32 = 9;

unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}

#[derive(Clone)]
struct ServerState {
    clients: Arc<RwLock<HashMap<String, ClientSender>>>,
    next_socket_id: Arc<AtomicU64>,
    next_request_id: Arc<AtomicU64>,
    request_id_prefix: Arc<str>,
    request_count: Arc<AtomicU64>,
    max_requests: u64,
    shutdown: watch::Sender<bool>,
    websocket_queue_capacity: usize,
    websocket_heartbeat: Duration,
    websocket_timeout: Duration,
    websocket_slots: Arc<Semaphore>,
    request_slots: Arc<Semaphore>,
    config: Arc<ServerConfig>,
    metrics: Arc<Metrics>,
    rate_limits: Arc<Mutex<HashMap<String, RateBucket>>>,
    worker_state: WorkerStateReporter,
    next_worker_metrics_at: Arc<AtomicU64>,
    last_reported_active_requests: Arc<AtomicU64>,
    worker_metrics_refresh_scheduled: Arc<AtomicBool>,
    active_php_executions: Arc<StdMutex<HashMap<String, u64>>>,
}

impl ServerState {
    fn new(config: ServerConfig) -> Self {
        let (shutdown, _) = watch::channel(false);
        let worker = std::env::var("PAM_WORKER_ID").unwrap_or_else(|_| "standalone".to_owned());
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
            next_socket_id: Arc::new(AtomicU64::new(1)),
            next_request_id: Arc::new(AtomicU64::new(1)),
            request_id_prefix: Arc::from(format!("{worker}-{}", std::process::id())),
            request_count: Arc::new(AtomicU64::new(0)),
            max_requests: std::env::var("PAM_MAX_REQUESTS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(u64::MAX),
            shutdown,
            websocket_queue_capacity: config.websocket_queue_capacity,
            websocket_heartbeat: Duration::from_millis(config.websocket_heartbeat_ms),
            websocket_timeout: Duration::from_millis(config.websocket_timeout_ms),
            websocket_slots: Arc::new(Semaphore::new(config.websocket_max_connections)),
            request_slots: Arc::new(Semaphore::new(config.max_concurrent_requests)),
            config: Arc::new(config),
            metrics: Arc::new(Metrics::default()),
            rate_limits: Arc::new(Mutex::new(HashMap::new())),
            worker_state: WorkerStateReporter::from_environment(),
            next_worker_metrics_at: Arc::new(AtomicU64::new(0)),
            last_reported_active_requests: Arc::new(AtomicU64::new(0)),
            worker_metrics_refresh_scheduled: Arc::new(AtomicBool::new(false)),
            active_php_executions: Arc::new(StdMutex::new(HashMap::new())),
        }
    }

    fn allocate_socket_id(&self) -> String {
        format!(
            "socket-{}",
            self.next_socket_id.fetch_add(1, Ordering::Relaxed)
        )
    }

    fn allocate_request_id(&self) -> String {
        format!(
            "{}-{}",
            self.request_id_prefix,
            self.next_request_id.fetch_add(1, Ordering::Relaxed)
        )
    }

    fn record_request(&self) {
        let requests = self.request_count.fetch_add(1, Ordering::Relaxed) + 1;
        if requests >= self.max_requests {
            let _ = self.shutdown.send(true);
        }
    }

    async fn rate_limited(&self, client: &str) -> bool {
        let limit = self.config.rate_limit_per_second;
        if limit == 0 {
            return false;
        }
        let now = Instant::now();
        let mut windows = self.rate_limits.lock().await;
        windows.retain(|_, bucket| now.duration_since(bucket.updated) < Duration::from_secs(2));
        let bucket = windows.entry(client.to_owned()).or_insert(RateBucket {
            available: f64::from(limit),
            updated: now,
        });
        let elapsed = now.duration_since(bucket.updated).as_secs_f64();
        bucket.available = (bucket.available + elapsed * f64::from(limit)).min(f64::from(limit));
        bucket.updated = now;
        if bucket.available < 1.0 {
            return true;
        }
        bucket.available -= 1.0;
        false
    }

    fn metrics_snapshot(&self) -> WorkerMetricsSnapshot {
        let runtime = php::runtime_metrics().unwrap_or_default();
        WorkerMetricsSnapshot {
            requests: self.metrics.requests.load(Ordering::Relaxed),
            errors: self.metrics.errors.load(Ordering::Relaxed),
            active_requests: self.metrics.active_requests.load(Ordering::Relaxed),
            request_duration_micros: self.metrics.request_duration_micros.load(Ordering::Relaxed),
            request_bytes: self.metrics.request_bytes.load(Ordering::Relaxed),
            response_bytes: self.metrics.response_bytes.load(Ordering::Relaxed),
            websocket_connections: self.metrics.websocket_connections.load(Ordering::Relaxed),
            websocket_messages: self.metrics.websocket_messages.load(Ordering::Relaxed),
            websocket_backpressure: self.metrics.websocket_backpressure.load(Ordering::Relaxed),
            event_loop_lag_micros: self.metrics.event_loop_lag_micros.load(Ordering::Relaxed),
            resident_memory_bytes: resident_memory_bytes(),
            php_memory_bytes: runtime.memory_bytes,
            php_peak_memory_bytes: runtime.peak_memory_bytes,
            php_fibers: runtime.fibers,
        }
    }

    fn report_worker_state(
        &self,
        lifecycle: WorkerLifecycle,
        deadline_at_millis: Option<u64>,
        request_id: Option<&str>,
    ) {
        if let Err(error) = self
            .worker_state
            .report(lifecycle, deadline_at_millis, request_id)
        {
            eprintln!("pam: {error}");
        }
    }

    fn refresh_worker_metrics(&self) {
        if !self.worker_state.enabled() {
            return;
        }
        let active_requests = self.metrics.active_requests.load(Ordering::Relaxed);
        let force_idle_update =
            active_requests == 0 && self.last_reported_active_requests.load(Ordering::Relaxed) > 0;
        let now = worker_epoch_millis();
        let next = self.next_worker_metrics_at.load(Ordering::Relaxed);
        if !force_idle_update {
            if now < next {
                self.schedule_worker_metrics_refresh(next.saturating_sub(now));
                return;
            }
            if self
                .next_worker_metrics_at
                .compare_exchange(
                    next,
                    now.saturating_add(1_000),
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_err()
            {
                let retry_at = self.next_worker_metrics_at.load(Ordering::Relaxed);
                self.schedule_worker_metrics_refresh(retry_at.saturating_sub(now));
                return;
            }
        } else {
            self.next_worker_metrics_at
                .store(now.saturating_add(1_000), Ordering::Relaxed);
        }
        let snapshot = self.metrics_snapshot();
        match self.worker_state.update_metrics(snapshot) {
            Ok(()) => self
                .last_reported_active_requests
                .store(active_requests, Ordering::Relaxed),
            Err(error) => eprintln!("pam: {error}"),
        }
    }

    fn schedule_worker_metrics_refresh(&self, delay_millis: u64) {
        if self
            .worker_metrics_refresh_scheduled
            .swap(true, Ordering::AcqRel)
        {
            return;
        }
        let state = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(delay_millis.max(1))).await;
            state
                .worker_metrics_refresh_scheduled
                .store(false, Ordering::Release);
            state.refresh_worker_metrics();
        });
    }

    fn begin_php_execution(&self, request_id: &str) {
        let deadline = worker_epoch_millis().saturating_add(self.config.request_timeout_ms);
        let (oldest_request, oldest_deadline) = {
            let mut executions = self
                .active_php_executions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            executions.insert(request_id.to_owned(), deadline);
            executions
                .iter()
                .min_by_key(|(request_id, deadline)| (**deadline, request_id.as_str()))
                .map(|(request_id, deadline)| (request_id.clone(), *deadline))
                .expect("the inserted PHP execution must be tracked")
        };
        self.report_worker_state(
            WorkerLifecycle::Busy,
            Some(oldest_deadline),
            Some(&oldest_request),
        );
    }

    fn finish_php_execution(&self, request_id: &str) {
        let oldest = {
            let mut executions = self
                .active_php_executions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            executions.remove(request_id);
            executions
                .iter()
                .min_by_key(|(request_id, deadline)| (**deadline, request_id.as_str()))
                .map(|(request_id, deadline)| (request_id.clone(), *deadline))
        };
        if let Some((oldest_request, oldest_deadline)) = oldest {
            self.report_worker_state(
                WorkerLifecycle::Busy,
                Some(oldest_deadline),
                Some(&oldest_request),
            );
        } else {
            self.report_worker_state(WorkerLifecycle::Ready, None, None);
        }
    }
}

struct PhpExecutionGuard {
    state: Option<ServerState>,
    request_id: String,
}

impl PhpExecutionGuard {
    fn begin(state: &ServerState, request_id: &str) -> Self {
        if !state.worker_state.enabled() {
            return Self {
                state: None,
                request_id: String::new(),
            };
        }
        state.begin_php_execution(request_id);
        Self {
            state: Some(state.clone()),
            request_id: request_id.to_owned(),
        }
    }
}

impl Drop for PhpExecutionGuard {
    fn drop(&mut self) {
        if let Some(state) = &self.state {
            state.finish_php_execution(&self.request_id);
        }
    }
}

struct ActiveResponseStreamGuard {
    state: ServerState,
}

impl ActiveResponseStreamGuard {
    fn begin(state: &ServerState) -> Self {
        state
            .metrics
            .active_requests
            .fetch_add(1, Ordering::Relaxed);
        Self {
            state: state.clone(),
        }
    }
}

impl Drop for ActiveResponseStreamGuard {
    fn drop(&mut self) {
        self.state
            .metrics
            .active_requests
            .fetch_sub(1, Ordering::Relaxed);
        self.state.refresh_worker_metrics();
    }
}

struct PhpHttpDispatchGuard {
    request_id: String,
    active: bool,
}

impl PhpHttpDispatchGuard {
    fn new(request_id: &str) -> Self {
        Self {
            request_id: request_id.to_owned(),
            active: true,
        }
    }

    fn complete(&mut self) {
        self.active = false;
    }

    fn cancel(&mut self) {
        if !self.active {
            return;
        }
        if let Err(error) = php::cancel_http(&self.request_id) {
            eprintln!("pam: cannot cancel PHP dispatch: {error}");
        }
        self.active = false;
    }
}

impl Drop for PhpHttpDispatchGuard {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[derive(Default)]
struct Metrics {
    requests: AtomicU64,
    errors: AtomicU64,
    active_requests: AtomicU64,
    request_duration_micros: AtomicU64,
    request_bytes: AtomicU64,
    response_bytes: AtomicU64,
    websocket_connections: AtomicU64,
    websocket_messages: AtomicU64,
    websocket_backpressure: AtomicU64,
    event_loop_lag_micros: AtomicU64,
}

struct RateBucket {
    available: f64,
    updated: Instant,
}

impl Metrics {
    fn render(&self, websocket_connections: usize, runtime: &php::RuntimeMetrics) -> String {
        format!(
            concat!(
                "# TYPE pam_http_requests_total counter\n",
                "pam_http_requests_total {}\n",
                "# TYPE pam_http_errors_total counter\n",
                "pam_http_errors_total {}\n",
                "# TYPE pam_http_active_requests gauge\n",
                "pam_http_active_requests {}\n",
                "# TYPE pam_http_request_duration_seconds counter\n",
                "pam_http_request_duration_seconds {:.6}\n",
                "# TYPE pam_http_request_bytes_total counter\n",
                "pam_http_request_bytes_total {}\n",
                "# TYPE pam_http_response_bytes_total counter\n",
                "pam_http_response_bytes_total {}\n",
                "# TYPE pam_websocket_connections gauge\n",
                "pam_websocket_connections {}\n",
                "# TYPE pam_websocket_messages_total counter\n",
                "pam_websocket_messages_total {}\n",
                "# TYPE pam_websocket_backpressure_total counter\n",
                "pam_websocket_backpressure_total {}\n",
                "# TYPE pam_event_loop_lag_seconds gauge\n",
                "pam_event_loop_lag_seconds {:.6}\n",
                "# TYPE pam_process_resident_memory_bytes gauge\n",
                "pam_process_resident_memory_bytes {}\n",
                "# TYPE pam_php_memory_bytes gauge\n",
                "pam_php_memory_bytes {}\n",
                "# TYPE pam_php_peak_memory_bytes gauge\n",
                "pam_php_peak_memory_bytes {}\n",
                "# TYPE pam_php_fibers gauge\n",
                "pam_php_fibers {}\n",
                "# TYPE pam_process_info gauge\n",
                "pam_process_info{{worker=\"{}\",generation=\"{}\",pid=\"{}\"}} 1\n",
            ),
            self.requests.load(Ordering::Relaxed),
            self.errors.load(Ordering::Relaxed),
            self.active_requests.load(Ordering::Relaxed),
            self.request_duration_micros.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            self.request_bytes.load(Ordering::Relaxed),
            self.response_bytes.load(Ordering::Relaxed),
            websocket_connections,
            self.websocket_messages.load(Ordering::Relaxed),
            self.websocket_backpressure.load(Ordering::Relaxed),
            self.event_loop_lag_micros.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            resident_memory_bytes(),
            runtime.memory_bytes,
            runtime.peak_memory_bytes,
            runtime.fibers,
            std::env::var("PAM_WORKER_ID").unwrap_or_else(|_| "standalone".to_owned()),
            std::env::var("PAM_WORKER_GENERATION").unwrap_or_else(|_| "1".to_owned()),
            std::process::id(),
        )
    }
}

struct RequestTelemetry {
    state: ServerState,
    metrics: Arc<Metrics>,
    started: Instant,
    alt_svc: Option<HeaderValue>,
    telemetry_headers: bool,
    log_request: bool,
}

impl RequestTelemetry {
    fn begin(
        state: &ServerState,
        request_bytes: usize,
        alt_svc: Option<HeaderValue>,
        telemetry_headers: bool,
        access_log: bool,
        access_log_sample_rate: u64,
    ) -> Self {
        let metrics = state.metrics.clone();
        let request_number = metrics.requests.fetch_add(1, Ordering::Relaxed) + 1;
        metrics.active_requests.fetch_add(1, Ordering::Relaxed);
        metrics
            .request_bytes
            .fetch_add(request_bytes as u64, Ordering::Relaxed);
        Self {
            state: state.clone(),
            metrics,
            started: Instant::now(),
            alt_svc,
            telemetry_headers,
            log_request: access_log && request_number.is_multiple_of(access_log_sample_rate.max(1)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn finish(
        self,
        mut response: Response,
        response_bytes: usize,
        request_id: &str,
        traceparent: &str,
        method: &Method,
        target: &str,
        cors_origin: Option<&str>,
    ) -> Response {
        let duration = self.started.elapsed();
        self.metrics
            .request_duration_micros
            .fetch_add(duration.as_micros() as u64, Ordering::Relaxed);
        self.metrics
            .response_bytes
            .fetch_add(response_bytes as u64, Ordering::Relaxed);
        if response.status().is_server_error() {
            self.metrics.errors.fetch_add(1, Ordering::Relaxed);
        }

        if self.telemetry_headers {
            if let Ok(value) = HeaderValue::from_str(request_id) {
                response.headers_mut().insert("x-request-id", value);
            }
            if let Ok(value) = HeaderValue::from_str(traceparent) {
                response.headers_mut().insert("traceparent", value);
            }
            if let Ok(value) =
                HeaderValue::from_str(&format!("pam;dur={:.3}", duration.as_secs_f64() * 1_000.0))
            {
                response.headers_mut().insert("server-timing", value);
            }
        }
        if let Some(value) = &self.alt_svc {
            response.headers_mut().insert("alt-svc", value.clone());
        }
        if let Some(origin) = cors_origin
            && let Ok(value) = HeaderValue::from_str(origin)
        {
            response
                .headers_mut()
                .insert("access-control-allow-origin", value);
            response.headers_mut().insert(
                "access-control-allow-methods",
                HeaderValue::from_static("GET, POST, PUT, PATCH, DELETE, OPTIONS"),
            );
            response.headers_mut().insert(
                "access-control-allow-headers",
                HeaderValue::from_static("authorization, content-type, x-request-id, traceparent"),
            );
            response
                .headers_mut()
                .insert("vary", HeaderValue::from_static("Origin"));
        }

        if self.log_request || response.status().is_server_error() {
            let log = serde_json::json!({
                "timestamp": epoch_millis(),
                "level": if response.status().is_server_error() { "error" } else { "info" },
                "event": "http.request",
                "method": method.as_str(),
                "target": target,
                "status": response.status().as_u16(),
                "durationMs": duration.as_secs_f64() * 1_000.0,
                "requestId": request_id,
                "traceparent": traceparent,
                "worker": std::env::var("PAM_WORKER_ID").unwrap_or_else(|_| "standalone".to_owned()),
            });
            eprintln!("{log}");
        }
        response
    }
}

impl Drop for RequestTelemetry {
    fn drop(&mut self) {
        self.metrics.active_requests.fetch_sub(1, Ordering::Relaxed);
        self.state.refresh_worker_metrics();
    }
}

#[derive(Debug)]
pub enum ServerError {
    Runtime(std::io::Error),
    Bind {
        address: String,
        source: std::io::Error,
    },
    Tls(String),
    Configuration(String),
    Http3(String),
    Serve(std::io::Error),
}

impl fmt::Display for ServerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Runtime(error) => write!(formatter, "cannot create the event loop: {error}"),
            Self::Bind { address, source } => {
                write!(formatter, "cannot listen on {address}: {source}")
            }
            Self::Tls(error) => write!(formatter, "cannot configure TLS: {error}"),
            Self::Configuration(error) => {
                write!(formatter, "invalid server configuration: {error}")
            }
            Self::Http3(error) => write!(formatter, "cannot configure HTTP/3: {error}"),
            Self::Serve(error) => write!(formatter, "HTTP server failed: {error}"),
        }
    }
}

pub fn run(config: ServerConfig) -> Result<(), ServerError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(ServerError::Runtime)?;

    runtime.block_on(run_async(config))
}

async fn run_async(config: ServerConfig) -> Result<(), ServerError> {
    if config
        .websocket_resume_secret
        .as_ref()
        .is_some_and(|secret| secret.len() < 32)
    {
        return Err(ServerError::Configuration(
            "websocketResumeSecret must contain at least 32 bytes".to_owned(),
        ));
    }
    let address = format!("{}:{}", config.host, config.port);
    let header_read_timeout = config.header_read_timeout_ms;
    let tls_files = match (&config.tls_cert, &config.tls_key) {
        (Some(certificate), Some(key)) => Some((certificate.clone(), key.clone())),
        (None, None) => None,
        _ => {
            return Err(ServerError::Tls(
                "both tlsCert and tlsKey must be configured".to_owned(),
            ));
        }
    };
    let listener = bind_listener(&address).map_err(|source| ServerError::Bind {
        address: address.clone(),
        source,
    })?;
    let state = ServerState::new(config);
    let recycle = state.shutdown.subscribe();
    let shutdown_sender = state.shutdown.clone();
    let http3 = if state.config.http3 {
        match &tls_files {
            Some((certificate, key)) => {
                let endpoint = build_http3_endpoint(&address, certificate, key)?;
                let http3_state = state.clone();
                let http3_shutdown = state.shutdown.subscribe();
                Some(tokio::spawn(async move {
                    run_http3(endpoint, http3_state, http3_shutdown).await;
                }))
            }
            None => None,
        }
    } else {
        None
    };
    monitor_event_loop(state.metrics.clone());
    let app = Router::new()
        .route("/ws", any(websocket_upgrade))
        .fallback(http_dispatch)
        .layer(DefaultBodyLimit::max(state.config.max_body_bytes))
        .with_state(state.clone());

    schedule_worker_ready(state.clone());

    let worker = std::env::var("PAM_WORKER_ID")
        .map(|id| format!(" worker={id}"))
        .unwrap_or_default();
    let scheme = if tls_files.is_some() { "https" } else { "http" };
    let http3_label = if http3.is_some() { " http3=true" } else { "" };
    let ui = Terminal::stderr();
    eprintln!(
        "{} {} {}{}{}",
        ui.success("● READY"),
        ui.heading("Pam listening on"),
        ui.command(format!("{scheme}://{address}")),
        ui.muted(worker),
        ui.muted(http3_label)
    );

    let result = if let Some((certificate, key)) = tls_files {
        let tls = axum_server::tls_rustls::RustlsConfig::from_pem_file(certificate, key)
            .await
            .map_err(|error| ServerError::Tls(error.to_string()))?;
        let handle = axum_server::Handle::new();
        let shutdown_handle = handle.clone();
        let signal_sender = shutdown_sender.clone();
        tokio::spawn(async move {
            shutdown_signal(recycle, signal_sender, state.clone()).await;
            shutdown_handle.graceful_shutdown(Some(Duration::from_secs(30)));
        });
        let mut server = axum_server::from_tcp_rustls(listener, tls)
            .map_err(ServerError::Serve)?
            .handle(handle);
        server
            .http_builder()
            .http1()
            .timer(TokioTimer::new())
            .header_read_timeout(Duration::from_millis(header_read_timeout));
        server
            .http_builder()
            .http2()
            .timer(TokioTimer::new())
            .keep_alive_interval(Some(Duration::from_secs(15)))
            .keep_alive_timeout(Duration::from_secs(10));
        server
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .map_err(ServerError::Serve)
    } else {
        let handle = axum_server::Handle::new();
        let shutdown_handle = handle.clone();
        let signal_sender = shutdown_sender.clone();
        tokio::spawn(async move {
            shutdown_signal(recycle, signal_sender, state.clone()).await;
            shutdown_handle.graceful_shutdown(Some(Duration::from_secs(30)));
        });
        let mut server = axum_server::from_tcp(listener)
            .map_err(ServerError::Serve)?
            .handle(handle);
        server
            .http_builder()
            .http1()
            .timer(TokioTimer::new())
            .header_read_timeout(Duration::from_millis(header_read_timeout));
        server
            .http_builder()
            .http2()
            .timer(TokioTimer::new())
            .keep_alive_interval(Some(Duration::from_secs(15)))
            .keep_alive_timeout(Duration::from_secs(10));
        server
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .map_err(ServerError::Serve)
    };

    let _ = shutdown_sender.send(true);
    if let Some(http3) = http3 {
        let _ = tokio::time::timeout(Duration::from_secs(30), http3).await;
    }
    result
}

fn schedule_worker_ready(state: ServerState) {
    tokio::spawn(async move {
        // The task can only run after the server future has yielded to Tokio. This
        // prevents the master from draining the previous generation while the new
        // listener is bound but its accept loop has not started polling yet.
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        state.refresh_worker_metrics();
        state.report_worker_state(WorkerLifecycle::Ready, None, None);
    });
}

fn bind_listener(address: &str) -> std::io::Result<std::net::TcpListener> {
    let socket_address = address
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| std::io::Error::other("address did not resolve"))?;
    let socket = Socket::new(
        Domain::for_address(socket_address),
        Type::STREAM,
        Some(Protocol::TCP),
    )?;
    socket.set_reuse_address(true)?;
    if std::env::var_os("PAM_WORKER_ID").is_some() {
        socket.set_reuse_port(true)?;
    }
    socket.set_nonblocking(true)?;
    socket.bind(&socket_address.into())?;
    socket.listen(1024)?;
    Ok(socket.into())
}

fn build_http3_endpoint(
    address: &str,
    certificate_path: &str,
    key_path: &str,
) -> Result<quinn::Endpoint, ServerError> {
    let certificates = CertificateDer::pem_file_iter(certificate_path)
        .map_err(|error| ServerError::Http3(format!("cannot open certificate: {error}")))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| ServerError::Http3(format!("invalid certificate PEM: {error}")))?;
    if certificates.is_empty() {
        return Err(ServerError::Http3(
            "certificate PEM does not contain a certificate".to_owned(),
        ));
    }
    let private_key = PrivateKeyDer::from_pem_file(key_path)
        .map_err(|error| ServerError::Http3(format!("invalid private key PEM: {error}")))?;

    let mut crypto = quinn::rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certificates, private_key)
        .map_err(|error| ServerError::Http3(error.to_string()))?;
    crypto.alpn_protocols = vec![b"h3".to_vec()];
    let quic_crypto = QuicServerConfig::try_from(crypto)
        .map_err(|error| ServerError::Http3(error.to_string()))?;
    let server_config = quinn::ServerConfig::with_crypto(Arc::new(quic_crypto));

    let socket_address = address
        .to_socket_addrs()
        .map_err(|error| ServerError::Http3(error.to_string()))?
        .next()
        .ok_or_else(|| ServerError::Http3("HTTP/3 address did not resolve".to_owned()))?;
    let socket = Socket::new(
        Domain::for_address(socket_address),
        Type::DGRAM,
        Some(Protocol::UDP),
    )
    .map_err(|error| ServerError::Http3(error.to_string()))?;
    socket
        .set_reuse_address(true)
        .map_err(|error| ServerError::Http3(error.to_string()))?;
    if std::env::var_os("PAM_WORKER_ID").is_some() {
        socket
            .set_reuse_port(true)
            .map_err(|error| ServerError::Http3(error.to_string()))?;
    }
    socket
        .set_nonblocking(true)
        .map_err(|error| ServerError::Http3(error.to_string()))?;
    socket
        .bind(&socket_address.into())
        .map_err(|error| ServerError::Http3(format!("cannot bind UDP {address}: {error}")))?;
    let socket: std::net::UdpSocket = socket.into();

    quinn::Endpoint::new(
        quinn::EndpointConfig::default(),
        Some(server_config),
        socket,
        Arc::new(quinn::TokioRuntime),
    )
    .map_err(|error| ServerError::Http3(error.to_string()))
}

async fn run_http3(
    endpoint: quinn::Endpoint,
    state: ServerState,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            _ = shutdown.changed() => break,
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else { break };
                let connection_state = state.clone();
                tokio::spawn(async move {
                    if let Err(error) = handle_http3_connection(incoming, connection_state).await {
                        eprintln!("pam: HTTP/3 connection failed: {error}");
                    }
                });
            }
        }
    }
    endpoint.close(quinn::VarInt::from_u32(0), b"Pam graceful shutdown");
    let _ = tokio::time::timeout(Duration::from_secs(30), endpoint.wait_idle()).await;
}

type Http3RequestStream = h3::server::RequestStream<
    <h3_quinn::Connection as h3::quic::OpenStreams<Bytes>>::BidiStream,
    Bytes,
>;

async fn handle_http3_connection(
    incoming: quinn::Incoming,
    state: ServerState,
) -> Result<(), String> {
    let remote = incoming.remote_address();
    let connection = incoming.await.map_err(|error| error.to_string())?;
    let quic = h3_quinn::Connection::new(connection);
    let mut http3 = h3::server::builder()
        .max_field_section_size(state.config.max_header_bytes as u64)
        .build(quic)
        .await
        .map_err(|error| error.to_string())?;

    loop {
        let resolver = match http3.accept().await {
            Ok(Some(resolver)) => resolver,
            Ok(None) => return Ok(()),
            Err(error) => return Err(error.to_string()),
        };
        let (request, stream) = resolver
            .resolve_request()
            .await
            .map_err(|error| error.to_string())?;
        let request_state = state.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_http3_request(request, stream, remote, request_state).await {
                eprintln!("pam: HTTP/3 request failed: {error}");
            }
        });
    }
}

async fn handle_http3_request(
    request: http::Request<()>,
    mut stream: Http3RequestStream,
    remote: SocketAddr,
    state: ServerState,
) -> Result<(), String> {
    state.record_request();
    let (parts, ()) = request.into_parts();
    let method = parts.method;
    let uri = parts.uri;
    let headers = parts.headers;
    let target = uri
        .path_and_query()
        .map_or_else(|| uri.path().to_owned(), ToString::to_string);
    let request_id = request_identifier(&headers).unwrap_or_else(|| state.allocate_request_id());
    let traceparent = request_traceparent(&headers, &state);
    let telemetry = RequestTelemetry::begin(
        &state,
        0,
        None,
        state.config.telemetry_headers,
        state.config.access_log,
        state.config.access_log_sample_rate,
    );

    let (response, response_bytes, cors_origin) = if uri.path() == state.config.metrics_path {
        let runtime_metrics = php::runtime_metrics().unwrap_or_default();
        let body = state
            .metrics
            .render(state.clients.read().await.len(), &runtime_metrics);
        let response_bytes = body.len();
        (
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "text/plain; version=0.0.4; charset=utf-8")
                .body(Body::from(body))
                .map_err(|error| error.to_string())?,
            response_bytes,
            None,
        )
    } else if headers.len() > state.config.max_headers
        || header_bytes(&headers) > state.config.max_header_bytes
    {
        (
            error_response(
                StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
                "request headers exceed the configured limit",
            ),
            0,
            None,
        )
    } else {
        let client = client_address(remote, &headers, &state.config.trusted_proxies);
        if state.rate_limited(&client).await {
            let mut response = error_response(StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded");
            response
                .headers_mut()
                .insert("retry-after", HeaderValue::from_static("1"));
            (response, 0, None)
        } else {
            let cors_origin = allowed_cors_origin(&headers, &state.config.cors_origins);
            if method == Method::OPTIONS && headers.contains_key("access-control-request-method") {
                let response = Response::builder()
                    .status(StatusCode::NO_CONTENT)
                    .body(Body::empty())
                    .map_err(|error| error.to_string())?;
                (response, 0, cors_origin)
            } else {
                let mut body = Vec::new();
                let deadline =
                    Instant::now() + Duration::from_millis(state.config.body_read_timeout_ms);
                loop {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    let data = match tokio::time::timeout(remaining, stream.recv_data()).await {
                        Ok(Ok(data)) => data,
                        Ok(Err(error)) => return Err(error.to_string()),
                        Err(_) => {
                            let response = error_response(
                                StatusCode::REQUEST_TIMEOUT,
                                "request body was not received before its timeout",
                            );
                            let response = telemetry.finish(
                                response,
                                0,
                                &request_id,
                                &traceparent,
                                &method,
                                &target,
                                cors_origin.as_deref(),
                            );
                            return send_http3_response(&mut stream, response).await;
                        }
                    };
                    let Some(mut data) = data else { break };
                    if body.len().saturating_add(data.remaining()) > state.config.max_body_bytes {
                        let response = error_response(
                            StatusCode::PAYLOAD_TOO_LARGE,
                            "request body exceeds the configured limit",
                        );
                        let response = telemetry.finish(
                            response,
                            0,
                            &request_id,
                            &traceparent,
                            &method,
                            &target,
                            cors_origin.as_deref(),
                        );
                        return send_http3_response(&mut stream, response).await;
                    }
                    let remaining = data.remaining();
                    body.extend_from_slice(&data.copy_to_bytes(remaining));
                }
                state
                    .metrics
                    .request_bytes
                    .fetch_add(body.len() as u64, Ordering::Relaxed);
                let (response, bytes) = invoke_php_http(
                    &state,
                    remote,
                    &client,
                    &method,
                    &target,
                    Version::HTTP_3,
                    &headers,
                    &body,
                    &request_id,
                    &traceparent,
                )
                .await;
                (response, bytes, cors_origin)
            }
        }
    };

    let response = telemetry.finish(
        response,
        response_bytes,
        &request_id,
        &traceparent,
        &method,
        &target,
        cors_origin.as_deref(),
    );
    send_http3_response(&mut stream, response).await
}

async fn send_http3_response(
    stream: &mut Http3RequestStream,
    response: Response,
) -> Result<(), String> {
    let (parts, body) = response.into_parts();
    stream
        .send_response(http::Response::from_parts(parts, ()))
        .await
        .map_err(|error| error.to_string())?;
    let mut data = body.into_data_stream();
    while let Some(chunk) = data.next().await {
        let chunk = chunk.map_err(|error| error.to_string())?;
        if !chunk.is_empty() {
            stream
                .send_data(chunk)
                .await
                .map_err(|error| error.to_string())?;
        }
    }
    stream.finish().await.map_err(|error| error.to_string())
}

async fn shutdown_signal(
    mut recycle: watch::Receiver<bool>,
    shutdown: watch::Sender<bool>,
    state: ServerState,
) {
    #[cfg(unix)]
    let quiesce_listener = {
        let terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate());
        let Ok(mut terminate) = terminate else {
            let _ = tokio::signal::ctrl_c().await;
            return;
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => true,
            _ = terminate.recv() => true,
            _ = recycle.changed() => false,
        }
    };

    #[cfg(not(unix))]
    let quiesce_listener = tokio::select! {
        _ = tokio::signal::ctrl_c() => true,
        _ = recycle.changed() => false,
    };

    state.report_worker_state(WorkerLifecycle::Draining, None, None);
    // With SO_REUSEPORT a connection may already be queued on the old worker's
    // listener when the replacement becomes ready. Keep accepting briefly so
    // that those kernel-queued connections are promoted to graceful tasks before
    // the listener closes.
    if quiesce_listener {
        let quiesce = std::env::var("PAM_DRAIN_QUIESCE_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(100)
            .min(5_000);
        tokio::time::sleep(Duration::from_millis(quiesce)).await;
    }
    let _ = shutdown.send(true);
}

async fn http_dispatch(
    State(state): State<ServerState>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    method: Method,
    uri: Uri,
    version: Version,
    headers: HeaderMap,
    body: Body,
) -> Response {
    state.record_request();
    let request_id = request_identifier(&headers).unwrap_or_else(|| state.allocate_request_id());
    let traceparent = request_traceparent(&headers, &state);
    let telemetry = RequestTelemetry::begin(
        &state,
        0,
        advertised_http3(&state.config),
        state.config.telemetry_headers,
        state.config.access_log,
        state.config.access_log_sample_rate,
    );
    let target = uri
        .path_and_query()
        .map_or_else(|| uri.path(), |value| value.as_str());

    if uri.path() == state.config.metrics_path {
        let runtime_metrics = php::runtime_metrics().unwrap_or_default();
        let body = state
            .metrics
            .render(state.clients.read().await.len(), &runtime_metrics);
        let bytes = body.len();
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/plain; version=0.0.4; charset=utf-8")
            .body(Body::from(body))
            .expect("metrics response must be valid");
        return telemetry.finish(
            response,
            bytes,
            &request_id,
            &traceparent,
            &method,
            target,
            None,
        );
    }

    if headers.len() > state.config.max_headers
        || header_bytes(&headers) > state.config.max_header_bytes
    {
        let response = error_response(
            StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
            "request headers exceed the configured limit",
        );
        return telemetry.finish(
            response,
            0,
            &request_id,
            &traceparent,
            &method,
            target,
            None,
        );
    }

    let client = client_address(remote, &headers, &state.config.trusted_proxies);
    if state.rate_limited(&client).await {
        let mut response = error_response(StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded");
        response
            .headers_mut()
            .insert("retry-after", HeaderValue::from_static("1"));
        return telemetry.finish(
            response,
            0,
            &request_id,
            &traceparent,
            &method,
            target,
            None,
        );
    }

    let cors_origin = allowed_cors_origin(&headers, &state.config.cors_origins);
    if method == Method::OPTIONS && headers.contains_key("access-control-request-method") {
        let response = Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(Body::empty())
            .expect("CORS preflight response must be valid");
        return telemetry.finish(
            response,
            0,
            &request_id,
            &traceparent,
            &method,
            target,
            cors_origin.as_deref(),
        );
    }

    let body = if body.is_end_stream() {
        Bytes::new()
    } else {
        match tokio::time::timeout(
            Duration::from_millis(state.config.body_read_timeout_ms),
            to_bytes(body, state.config.max_body_bytes),
        )
        .await
        {
            Ok(Ok(body)) => body,
            Ok(Err(_)) => {
                let response = error_response(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "request body exceeds the configured limit",
                );
                return telemetry.finish(
                    response,
                    0,
                    &request_id,
                    &traceparent,
                    &method,
                    target,
                    cors_origin.as_deref(),
                );
            }
            Err(_) => {
                let response = error_response(
                    StatusCode::REQUEST_TIMEOUT,
                    "request body was not received before its timeout",
                );
                return telemetry.finish(
                    response,
                    0,
                    &request_id,
                    &traceparent,
                    &method,
                    target,
                    cors_origin.as_deref(),
                );
            }
        }
    };
    state
        .metrics
        .request_bytes
        .fetch_add(body.len() as u64, Ordering::Relaxed);

    let (response, response_bytes) = invoke_php_http(
        &state,
        remote,
        &client,
        &method,
        target,
        version,
        &headers,
        &body,
        &request_id,
        &traceparent,
    )
    .await;
    telemetry.finish(
        response,
        response_bytes,
        &request_id,
        &traceparent,
        &method,
        target,
        cors_origin.as_deref(),
    )
}

fn advertised_http3(config: &ServerConfig) -> Option<HeaderValue> {
    if !config.http3 || config.tls_cert.is_none() {
        return None;
    }

    HeaderValue::from_str(&format!("h3=\":{}\"; ma=86400", config.port)).ok()
}

#[allow(clippy::too_many_arguments)]
async fn invoke_php_http(
    state: &ServerState,
    remote: SocketAddr,
    client: &str,
    method: &Method,
    target: &str,
    version: Version,
    headers: &HeaderMap,
    body: &[u8],
    request_id: &str,
    traceparent: &str,
) -> (Response, usize) {
    let _request_slot = match state.request_slots.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            return (
                error_response(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "worker concurrent request limit reached",
                ),
                0,
            );
        }
    };
    let mut normalized_headers = HashMap::<String, Vec<String>>::new();
    for name in headers.keys() {
        let values = headers
            .get_all(name)
            .iter()
            .filter_map(|value| value.to_str().ok().map(str::to_owned))
            .collect::<Vec<_>>();
        normalized_headers.insert(name.as_str().to_owned(), values);
    }
    let headers_json = match serde_json::to_string(&normalized_headers) {
        Ok(json) => json,
        Err(error) => {
            return (
                internal_error(format!("cannot serialize request headers: {error}")),
                0,
            );
        }
    };
    let context = HttpRequestContext {
        protocol_version: match version {
            Version::HTTP_09 => "0.9",
            Version::HTTP_10 => "1.0",
            Version::HTTP_11 => "1.1",
            Version::HTTP_2 => "2",
            Version::HTTP_3 => "3",
            _ => "1.1",
        },
        remote_address: client,
        remote_port: remote.port(),
        scheme: if state.config.tls_cert.is_some() {
            "https"
        } else {
            "http"
        },
        request_id,
        traceparent,
    };
    let context_json = match serde_json::to_string(&context) {
        Ok(json) => json,
        Err(error) => {
            return (
                internal_error(format!("cannot serialize request context: {error}")),
                0,
            );
        }
    };

    let started = Instant::now();
    let _execution = PhpExecutionGuard::begin(state, request_id);
    let mut cancellation = PhpHttpDispatchGuard::new(request_id);
    let mut envelope = match php::dispatch_http(
        method.as_str(),
        target,
        &headers_json,
        body,
        &context_json,
        request_id,
    )
    .and_then(|json| parse_dispatch_envelope(&json))
    {
        Ok(envelope) => envelope,
        Err(error) => return (internal_error(error), 0),
    };
    let timeout = Duration::from_millis(state.config.request_timeout_ms);

    loop {
        match envelope.state {
            PhpDispatchState::Complete => {
                cancellation.complete();
                let Some(json) = envelope.response.take() else {
                    return (
                        internal_error("PHP dispatch completed without a response"),
                        0,
                    );
                };
                return match serde_json::from_str::<PhpHttpResponse>(&json) {
                    Ok(response) => match response.total_bytes() {
                        Some(bytes) if bytes <= state.config.max_response_bytes => {
                            (response.into_http_response(), bytes)
                        }
                        _ => (response_limit_error(state.config.max_response_bytes), 0),
                    },
                    Err(error) => (internal_error(format!("invalid PHP response: {error}")), 0),
                };
            }
            PhpDispatchState::Suspended => {
                let elapsed = started.elapsed();
                if elapsed >= timeout {
                    cancellation.cancel();
                    return (
                        error_response(
                            StatusCode::GATEWAY_TIMEOUT,
                            "request execution exceeded its timeout",
                        ),
                        0,
                    );
                }
                let Some(mut operation) = envelope.operation.take() else {
                    return (internal_error("suspended PHP dispatch has no operation"), 0);
                };
                let operation_result = match operation.kind {
                    PhpOperationKind::Timer => {
                        let delay = Duration::from_micros(operation.delay_micros);
                        let remaining = timeout.saturating_sub(elapsed);
                        if delay >= remaining {
                            tokio::time::sleep(remaining).await;
                            cancellation.cancel();
                            return (
                                error_response(
                                    StatusCode::GATEWAY_TIMEOUT,
                                    "request execution exceeded its timeout",
                                ),
                                0,
                            );
                        }
                        if delay.is_zero() {
                            tokio::task::yield_now().await;
                        } else {
                            tokio::time::sleep(delay).await;
                        }
                        "null".to_owned()
                    }
                    PhpOperationKind::Stream => {
                        let remaining = timeout.saturating_sub(started.elapsed());
                        match wait_for_php_streams(&operation, remaining).await {
                            Ok(timed_out) => {
                                serde_json::json!({ "timedOut": timed_out }).to_string()
                            }
                            Err(error) => return (internal_error(error), 0),
                        }
                    }
                    PhpOperationKind::Dns
                    | PhpOperationKind::FileRead
                    | PhpOperationKind::FileWrite
                    | PhpOperationKind::Process
                    | PhpOperationKind::Signal => {
                        let remaining = timeout.saturating_sub(started.elapsed());
                        match execute_php_native_operation(&operation, remaining).await {
                            Ok(result) => result.to_string(),
                            Err(error) => serde_json::json!({ "error": error }).to_string(),
                        }
                    }
                    PhpOperationKind::ResponseChunk => {
                        let Some(head) = operation.response.take() else {
                            return (internal_error("streaming response has no HTTP metadata"), 0);
                        };
                        let Some(encoded_chunk) = operation.chunk.take() else {
                            return (internal_error("streaming response has no chunk"), 0);
                        };
                        let chunk = match BASE64.decode(encoded_chunk) {
                            Ok(chunk) => chunk,
                            Err(error) => {
                                return (
                                    internal_error(format!(
                                        "streaming response chunk is invalid: {error}"
                                    )),
                                    0,
                                );
                            }
                        };
                        if chunk.len() > state.config.max_response_chunk_bytes {
                            return (
                                response_chunk_limit_error(state.config.max_response_chunk_bytes),
                                0,
                            );
                        }
                        if head
                            .chunks
                            .iter()
                            .any(|chunk| chunk.len() > state.config.max_response_chunk_bytes)
                        {
                            return (
                                response_chunk_limit_error(state.config.max_response_chunk_bytes),
                                0,
                            );
                        }
                        let initial_response_bytes = head
                            .total_bytes()
                            .and_then(|bytes| bytes.checked_add(chunk.len()));
                        let declared_response_bytes = head.declared_content_length();
                        let Some(initial_response_bytes) = initial_response_bytes else {
                            return (response_limit_error(state.config.max_response_bytes), 0);
                        };
                        if initial_response_bytes > state.config.max_response_bytes
                            || declared_response_bytes
                                .is_some_and(|bytes| bytes > state.config.max_response_bytes)
                        {
                            return (response_limit_error(state.config.max_response_bytes), 0);
                        }
                        let (sender, receiver) =
                            mpsc::channel(state.config.response_stream_queue_capacity.max(1));
                        if !head.body.is_empty()
                            && sender.try_send(Ok(Bytes::from(head.body.clone()))).is_err()
                        {
                            return (internal_error("streaming response queue is unavailable"), 0);
                        }
                        for buffered in &head.chunks {
                            if sender.try_send(Ok(Bytes::from(buffered.clone()))).is_err() {
                                return (internal_error("streaming response queue is full"), 0);
                            }
                        }
                        if sender.try_send(Ok(Bytes::from(chunk))).is_err() {
                            return (internal_error("streaming response queue is full"), 0);
                        }
                        let request_id = request_id.to_owned();
                        let request_body = body.to_vec();
                        tokio::spawn(
                            PhpResponseStream {
                                sender,
                                request_id,
                                request_body,
                                started,
                                timeout,
                                cancellation,
                                metrics: state.metrics.clone(),
                                initial_response_bytes,
                                max_response_bytes: state.config.max_response_bytes,
                                max_response_chunk_bytes: state.config.max_response_chunk_bytes,
                                _active_response_stream: ActiveResponseStreamGuard::begin(state),
                                _execution_guard: _execution,
                                _request_slot,
                            }
                            .drive(),
                        );
                        let stream = futures_util::stream::unfold(receiver, |mut receiver| async {
                            receiver.recv().await.map(|item| (item, receiver))
                        });
                        return (
                            head.into_streaming_http_response(Body::from_stream(stream)),
                            0,
                        );
                    }
                };
                envelope = match php::resume_http(request_id, body, &operation_result)
                    .and_then(|json| parse_dispatch_envelope(&json))
                {
                    Ok(envelope) => envelope,
                    Err(error) => return (internal_error(error), 0),
                };
            }
        }
    }
}

struct PhpResponseStream {
    sender: mpsc::Sender<Result<Bytes, std::io::Error>>,
    request_id: String,
    request_body: Vec<u8>,
    started: Instant,
    timeout: Duration,
    cancellation: PhpHttpDispatchGuard,
    metrics: Arc<Metrics>,
    initial_response_bytes: usize,
    max_response_bytes: usize,
    max_response_chunk_bytes: usize,
    _active_response_stream: ActiveResponseStreamGuard,
    _execution_guard: PhpExecutionGuard,
    _request_slot: OwnedSemaphorePermit,
}

impl PhpResponseStream {
    async fn drive(self) {
        let Self {
            sender,
            request_id,
            request_body,
            started,
            timeout,
            mut cancellation,
            metrics,
            initial_response_bytes,
            max_response_bytes,
            max_response_chunk_bytes,
            _active_response_stream,
            _execution_guard,
            _request_slot,
        } = self;
        metrics
            .response_bytes
            .fetch_add(initial_response_bytes as u64, Ordering::Relaxed);
        let mut sent_bytes = initial_response_bytes;
        let mut envelope = match php::resume_http(&request_id, &request_body, "null")
            .and_then(|json| parse_dispatch_envelope(&json))
        {
            Ok(envelope) => envelope,
            Err(error) => {
                eprintln!("pam: streaming PHP dispatch failed: {error}");
                return;
            }
        };

        loop {
            if started.elapsed() >= timeout {
                cancellation.cancel();
                return;
            }
            match envelope.state {
                PhpDispatchState::Complete => {
                    cancellation.complete();
                    if let Some(response) = envelope.response.take()
                        && let Ok(response) = serde_json::from_str::<PhpHttpResponse>(&response)
                    {
                        if !response.body.is_empty()
                            && !send_response_chunk(
                                &sender,
                                Bytes::from(response.body),
                                timeout.saturating_sub(started.elapsed()),
                                &metrics,
                                &mut sent_bytes,
                                max_response_bytes,
                                max_response_chunk_bytes,
                            )
                            .await
                        {
                            return;
                        }
                        for chunk in response.chunks {
                            if !send_response_chunk(
                                &sender,
                                Bytes::from(chunk),
                                timeout.saturating_sub(started.elapsed()),
                                &metrics,
                                &mut sent_bytes,
                                max_response_bytes,
                                max_response_chunk_bytes,
                            )
                            .await
                            {
                                return;
                            }
                        }
                    }
                    return;
                }
                PhpDispatchState::Suspended => {
                    let Some(mut operation) = envelope.operation.take() else {
                        eprintln!("pam: streaming PHP dispatch suspended without an operation");
                        return;
                    };
                    let remaining = timeout.saturating_sub(started.elapsed());
                    let result = match operation.kind {
                        PhpOperationKind::ResponseChunk => {
                            let chunk = operation
                                .chunk
                                .take()
                                .ok_or_else(|| "streaming response has no chunk".to_owned())
                                .and_then(|chunk| {
                                    BASE64.decode(chunk).map_err(|error| {
                                        format!("streaming response chunk is invalid: {error}")
                                    })
                                });
                            match chunk {
                                Ok(chunk) => {
                                    if !send_response_chunk(
                                        &sender,
                                        Bytes::from(chunk),
                                        remaining,
                                        &metrics,
                                        &mut sent_bytes,
                                        max_response_bytes,
                                        max_response_chunk_bytes,
                                    )
                                    .await
                                    {
                                        return;
                                    }
                                    "null".to_owned()
                                }
                                Err(error) => serde_json::json!({ "error": error }).to_string(),
                            }
                        }
                        PhpOperationKind::Timer => {
                            let delay = Duration::from_micros(operation.delay_micros);
                            if delay >= remaining {
                                tokio::time::sleep(remaining).await;
                                cancellation.cancel();
                                return;
                            }
                            if delay.is_zero() {
                                tokio::task::yield_now().await;
                            } else {
                                tokio::time::sleep(delay).await;
                            }
                            "null".to_owned()
                        }
                        PhpOperationKind::Stream => {
                            match wait_for_php_streams(&operation, remaining).await {
                                Ok(timed_out) => {
                                    serde_json::json!({ "timedOut": timed_out }).to_string()
                                }
                                Err(error) => serde_json::json!({ "error": error }).to_string(),
                            }
                        }
                        PhpOperationKind::Dns
                        | PhpOperationKind::FileRead
                        | PhpOperationKind::FileWrite
                        | PhpOperationKind::Process
                        | PhpOperationKind::Signal => {
                            match execute_php_native_operation(&operation, remaining).await {
                                Ok(result) => result.to_string(),
                                Err(error) => serde_json::json!({ "error": error }).to_string(),
                            }
                        }
                    };
                    envelope = match php::resume_http(&request_id, &request_body, &result)
                        .and_then(|json| parse_dispatch_envelope(&json))
                    {
                        Ok(envelope) => envelope,
                        Err(error) => {
                            eprintln!("pam: cannot resume streaming PHP dispatch: {error}");
                            return;
                        }
                    };
                }
            }
        }
    }
}

async fn send_response_chunk(
    sender: &mpsc::Sender<Result<Bytes, std::io::Error>>,
    chunk: Bytes,
    timeout: Duration,
    metrics: &Metrics,
    sent_bytes: &mut usize,
    max_response_bytes: usize,
    max_response_chunk_bytes: usize,
) -> bool {
    let bytes = chunk.len();
    let Some(next_total) = sent_bytes.checked_add(bytes) else {
        eprintln!("pam: streaming response byte counter overflowed");
        send_response_stream_error(
            sender,
            timeout,
            "streaming response byte counter overflowed",
        )
        .await;
        return false;
    };
    if bytes > max_response_chunk_bytes || next_total > max_response_bytes {
        let message = format!(
            "streaming response exceeded its configured byte limit (chunk={bytes}, total={next_total})"
        );
        eprintln!("pam: {message}");
        send_response_stream_error(sender, timeout, message).await;
        return false;
    }
    let sent = matches!(
        tokio::time::timeout(timeout, sender.send(Ok(chunk))).await,
        Ok(Ok(()))
    );
    if sent {
        *sent_bytes = next_total;
        metrics
            .response_bytes
            .fetch_add(bytes as u64, Ordering::Relaxed);
    }
    sent
}

async fn send_response_stream_error(
    sender: &mpsc::Sender<Result<Bytes, std::io::Error>>,
    timeout: Duration,
    message: impl Into<String>,
) {
    let error = std::io::Error::other(message.into());
    let _ = tokio::time::timeout(timeout, sender.send(Err(error))).await;
}

async fn websocket_upgrade(
    websocket: IncomingUpgrade,
    State(state): State<ServerState>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    let permit = match state.websocket_slots.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "WebSocket connection limit reached",
            );
        }
    };
    let context = serde_json::json!({
        "headers": headers.iter().filter_map(|(name, value)| {
            value.to_str().ok().map(|value| (name.as_str(), value))
        }).collect::<HashMap<_, _>>(),
        "query": uri.query().unwrap_or_default(),
        "path": uri.path(),
    });
    let session_id = query_parameter(uri.query(), "sessionId");
    let resume_token = query_parameter(uri.query(), "resumeToken");
    let requested_session = match (session_id, resume_token) {
        (None, None) => None,
        (Some(session_id), Some(token))
            if is_safe_session_id(&session_id)
                && state
                    .config
                    .websocket_resume_secret
                    .as_deref()
                    .is_some_and(|secret| verify_resume_token(secret, &session_id, &token)) =>
        {
            Some(session_id)
        }
        _ => {
            return error_response(
                StatusCode::UNAUTHORIZED,
                "WebSocket reconnection requires a valid server-issued resume token",
            );
        }
    };
    let options = Options::default()
        .with_max_payload_read(state.config.websocket_max_message_bytes)
        .with_max_read_buffer(state.config.websocket_max_message_bytes.saturating_mul(2))
        .with_utf8();
    let options = if state.config.websocket_compression {
        options.with_compression_level(CompressionLevel::fast())
    } else {
        options.without_compression()
    };
    let (response, upgrade) = match websocket.upgrade(options) {
        Ok(upgrade) => upgrade,
        Err(error) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                &format!("WebSocket upgrade failed: {error}"),
            );
        }
    };
    tokio::spawn(async move {
        match upgrade.await {
            Ok(socket) => {
                websocket_session(socket, state, requested_session, context, permit).await;
            }
            Err(error) => eprintln!("pam: WebSocket upgrade failed: {error}"),
        }
    });
    response.into_response()
}

async fn websocket_session(
    socket: WebSocket<yawc::HttpStream>,
    state: ServerState,
    requested_session: Option<String>,
    mut context: Value,
    _permit: OwnedSemaphorePermit,
) {
    state
        .metrics
        .websocket_connections
        .fetch_add(1, Ordering::Relaxed);
    let socket_id = requested_session.unwrap_or_else(|| state.allocate_socket_id());
    if let Some(context) = context.as_object_mut() {
        context.insert("sessionId".to_owned(), Value::String(socket_id.clone()));
        if let Some(secret) = state.config.websocket_resume_secret.as_deref() {
            context.insert(
                "resumeToken".to_owned(),
                Value::String(create_resume_token(
                    secret,
                    &socket_id,
                    state.config.websocket_resume_ttl_seconds,
                )),
            );
        }
    }
    let (client_sender, mut client_receiver) = mpsc::channel(state.websocket_queue_capacity);
    if let Some(previous) = state
        .clients
        .write()
        .await
        .insert(socket_id.clone(), client_sender)
    {
        let _ = previous.try_send(Frame::close(CloseCode::Normal, "session replaced"));
    }

    let (mut websocket_sender, mut websocket_receiver) = socket.split();
    let last_seen = Arc::new(AtomicU64::new(epoch_millis()));
    let writer_last_seen = last_seen.clone();
    let writer_state = state.clone();
    let writer_socket_id = socket_id.clone();
    let heartbeat_period = state.websocket_heartbeat;
    let heartbeat_timeout = state.websocket_timeout;
    let writer = tokio::spawn(async move {
        let mut heartbeat = tokio::time::interval_at(
            tokio::time::Instant::now() + heartbeat_period,
            heartbeat_period,
        );
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                message = client_receiver.recv() => {
                    let Some(message) = message else { break };
                    if websocket_sender.send(message).await.is_err() {
                        break;
                    }
                }
                _ = heartbeat.tick() => {
                    if epoch_millis().saturating_sub(writer_last_seen.load(Ordering::Relaxed))
                        > heartbeat_timeout.as_millis() as u64
                    {
                        let _ = websocket_sender
                            .send(Frame::close(CloseCode::Away, "heartbeat timeout"))
                            .await;
                        break;
                    }
                    dispatch_websocket_event(
                        InboundEventKind::Tick,
                        &writer_socket_id,
                        "",
                        &writer_state,
                    ).await;
                    if websocket_sender
                        .send(Frame::ping(epoch_millis().to_be_bytes().to_vec()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
    });

    dispatch_websocket_event(
        InboundEventKind::Connection,
        &socket_id,
        &context.to_string(),
        &state,
    )
    .await;

    while let Some(message) = websocket_receiver.next().await {
        last_seen.store(epoch_millis(), Ordering::Relaxed);
        match message.opcode() {
            OpCode::Text => {
                state
                    .metrics
                    .websocket_messages
                    .fetch_add(1, Ordering::Relaxed);
                let payload = match std::str::from_utf8(message.payload()) {
                    Ok(payload) => payload,
                    Err(_) => break,
                };
                dispatch_websocket_event(InboundEventKind::Message, &socket_id, payload, &state)
                    .await;
            }
            OpCode::Binary => {
                state
                    .metrics
                    .websocket_messages
                    .fetch_add(1, Ordering::Relaxed);
                dispatch_websocket_event(
                    InboundEventKind::Binary,
                    &socket_id,
                    &BASE64.encode(message.payload()),
                    &state,
                )
                .await;
            }
            OpCode::Close => break,
            OpCode::Ping | OpCode::Pong | OpCode::Continuation => {}
        }
    }

    dispatch_websocket_event(InboundEventKind::Disconnect, &socket_id, "", &state).await;
    state.clients.write().await.remove(&socket_id);
    state
        .metrics
        .websocket_connections
        .fetch_sub(1, Ordering::Relaxed);
    writer.abort();
}

async fn dispatch_websocket_event(
    event_kind: InboundEventKind,
    socket_id: &str,
    payload: &str,
    state: &ServerState,
) {
    let Ok(_request_slot) = state.request_slots.clone().acquire_owned().await else {
        return;
    };
    let execution_id = format!("ws:{}:{}", socket_id, event_kind as i32);
    let _execution = PhpExecutionGuard::begin(state, &execution_id);
    let json = match php::dispatch_ws(event_kind as i32, socket_id, payload) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("pam: WebSocket PHP dispatch failed: {error}");
            return;
        }
    };
    let commands = match serde_json::from_str::<Vec<WsCommand>>(&json) {
        Ok(commands) => commands,
        Err(error) => {
            eprintln!("pam: invalid WebSocket command from PHP: {error}");
            return;
        }
    };

    apply_websocket_commands(commands, state).await;
}

async fn apply_websocket_commands(commands: Vec<WsCommand>, state: &ServerState) {
    for command in commands {
        if !command.has_valid_recipients() {
            eprintln!(
                "pam: ignored an invalid {:?} WebSocket command",
                command.target
            );
            continue;
        }

        let message = match command.kind {
            OutboundMessageKind::Event => serde_json::to_string(&WsEvent {
                event: &command.event,
                data: &command.data,
            })
            .map(Frame::text),
            OutboundMessageKind::Acknowledgement => serde_json::to_string(&WsAcknowledgement {
                acknowledgement: &command.event,
                data: &command.data,
            })
            .map(Frame::text),
            OutboundMessageKind::Binary => command
                .data
                .as_str()
                .ok_or_else(|| {
                    serde_json::Error::io(std::io::Error::other("binary payload is not a string"))
                })
                .and_then(|payload| {
                    BASE64
                        .decode(payload)
                        .map(Frame::binary)
                        .map_err(|error| serde_json::Error::io(std::io::Error::other(error)))
                }),
            OutboundMessageKind::Close => Ok(Frame::close(CloseCode::Normal, "application close")),
        };
        let message = match message {
            Ok(message) => message,
            Err(error) => {
                eprintln!("pam: cannot serialize WebSocket event: {error}");
                continue;
            }
        };
        let recipients = {
            let clients = state.clients.read().await;
            command
                .socket_ids
                .iter()
                .filter_map(|socket_id| clients.get(socket_id).cloned())
                .collect::<Vec<_>>()
        };

        for recipient in recipients {
            if recipient.try_send(message.clone()).is_err() {
                state
                    .metrics
                    .websocket_backpressure
                    .fetch_add(1, Ordering::Relaxed);
                eprintln!("pam: WebSocket backpressure limit reached; dropping client message");
            }
        }
    }
}

fn internal_error(message: impl Into<String>) -> Response {
    let message = message.into();
    eprintln!(
        "{}",
        serde_json::json!({
            "level": "error",
            "event": "runtime_internal_error",
            "message": message,
        })
    );
    let payload = serde_json::json!({
        "error": "Internal Server Error",
        "message": "The request failed. Use the request ID to inspect server logs.",
    });
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .header("content-type", "application/json; charset=utf-8")
        .body(Body::from(payload.to_string()))
        .expect("static HTTP error response must be valid")
}

fn response_limit_error(limit: usize) -> Response {
    internal_error(format!(
        "PHP response exceeded the configured maxResponseBytes limit of {limit} bytes"
    ))
}

fn response_chunk_limit_error(limit: usize) -> Response {
    internal_error(format!(
        "PHP response chunk exceeded the configured maxResponseChunkBytes limit of {limit} bytes"
    ))
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(try_from = "u8")]
enum PhpDispatchState {
    Complete = 1,
    Suspended = 2,
}

impl TryFrom<u8> for PhpDispatchState {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Complete),
            2 => Ok(Self::Suspended),
            _ => Err(format!("unknown PHP dispatch state {value}")),
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(try_from = "u8")]
enum PhpOperationKind {
    Timer = 1,
    Stream = 2,
    Dns = 3,
    FileRead = 4,
    FileWrite = 5,
    Process = 6,
    Signal = 7,
    ResponseChunk = 8,
}

impl TryFrom<u8> for PhpOperationKind {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Timer),
            2 => Ok(Self::Stream),
            3 => Ok(Self::Dns),
            4 => Ok(Self::FileRead),
            5 => Ok(Self::FileWrite),
            6 => Ok(Self::Process),
            7 => Ok(Self::Signal),
            8 => Ok(Self::ResponseChunk),
            _ => Err(format!("unknown PHP async operation kind {value}")),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PhpOperation {
    kind: PhpOperationKind,
    delay_micros: u64,
    #[serde(default)]
    read_file_descriptors: Vec<RawFd>,
    #[serde(default)]
    write_file_descriptors: Vec<RawFd>,
    #[serde(default)]
    host: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    data: Option<String>,
    #[serde(default)]
    atomic: bool,
    #[serde(default)]
    arguments: Vec<String>,
    #[serde(default)]
    stdin: Option<String>,
    #[serde(default = "default_native_output_limit")]
    max_output_bytes: usize,
    #[serde(default)]
    signal: Option<i32>,
    #[serde(default)]
    response: Option<PhpHttpResponse>,
    #[serde(default)]
    chunk: Option<String>,
}

fn default_native_output_limit() -> usize {
    8 * 1024 * 1024
}

#[derive(Debug, Deserialize)]
struct PhpDispatchEnvelope {
    state: PhpDispatchState,
    #[serde(default)]
    response: Option<String>,
    #[serde(default)]
    operation: Option<PhpOperation>,
}

fn parse_dispatch_envelope(json: &str) -> Result<PhpDispatchEnvelope, String> {
    serde_json::from_str(json).map_err(|error| format!("invalid PHP dispatch envelope: {error}"))
}

#[derive(Clone, Copy)]
struct ObservedFileDescriptor(RawFd);

impl AsRawFd for ObservedFileDescriptor {
    fn as_raw_fd(&self) -> RawFd {
        self.0
    }
}

async fn wait_for_observed_file_descriptor(
    file_descriptor: RawFd,
    readable: bool,
    writable: bool,
) -> Result<(), String> {
    let descriptor = AsyncFd::new(ObservedFileDescriptor(file_descriptor)).map_err(|error| {
        format!("cannot register PHP stream file descriptor {file_descriptor}: {error}")
    })?;
    let interest = match (readable, writable) {
        (true, true) => Interest::READABLE.add(Interest::WRITABLE),
        (true, false) => Interest::READABLE,
        (false, true) => Interest::WRITABLE,
        (false, false) => return Err("PHP stream operation has no readiness interest".to_owned()),
    };
    let _readiness = descriptor
        .ready(interest)
        .await
        .map_err(|error| format!("PHP stream readiness failed: {error}"))?;
    Ok(())
}

async fn wait_for_php_streams(
    operation: &PhpOperation,
    request_remaining: Duration,
) -> Result<bool, String> {
    let mut descriptors = HashMap::<RawFd, (bool, bool)>::new();
    for file_descriptor in &operation.read_file_descriptors {
        if *file_descriptor < 0 {
            return Err(format!(
                "invalid PHP read file descriptor {file_descriptor}"
            ));
        }
        descriptors.entry(*file_descriptor).or_default().0 = true;
    }
    for file_descriptor in &operation.write_file_descriptors {
        if *file_descriptor < 0 {
            return Err(format!(
                "invalid PHP write file descriptor {file_descriptor}"
            ));
        }
        descriptors.entry(*file_descriptor).or_default().1 = true;
    }
    if descriptors.is_empty() {
        return Err("PHP stream operation contains no file descriptors".to_owned());
    }

    let waiters = descriptors
        .into_iter()
        .map(|(file_descriptor, (readable, writable))| {
            Box::pin(wait_for_observed_file_descriptor(
                file_descriptor,
                readable,
                writable,
            )) as Pin<Box<dyn Future<Output = Result<(), String>> + Send>>
        })
        .collect::<Vec<_>>();
    let operation_timeout = Duration::from_micros(operation.delay_micros);
    let timeout = operation_timeout.min(request_remaining);
    match tokio::time::timeout(timeout, futures_util::future::select_all(waiters)).await {
        Ok((result, _, _)) => result.map(|()| false),
        Err(_) => Ok(true),
    }
}

async fn execute_php_native_operation(
    operation: &PhpOperation,
    request_remaining: Duration,
) -> Result<Value, String> {
    let operation_timeout = Duration::from_micros(operation.delay_micros).min(request_remaining);
    let kind = operation.kind;
    let task_timeout = if matches!(kind, PhpOperationKind::Process) {
        operation_timeout
            .saturating_add(Duration::from_millis(250))
            .min(request_remaining)
    } else {
        operation_timeout
    };
    let host = operation.host.clone();
    let path = operation.path.clone();
    let data = operation.data.clone();
    let atomic = operation.atomic;
    let arguments = operation.arguments.clone();
    let stdin = operation.stdin.clone();
    let max_output_bytes = operation.max_output_bytes;
    let signal_number = operation.signal;

    let task = async move {
        match kind {
            PhpOperationKind::Dns => {
                let host = host.ok_or_else(|| "native DNS operation requires a host".to_owned())?;
                if host.is_empty() || host.len() > 253 || host.as_bytes().contains(&0) {
                    return Err("native DNS host is invalid".to_owned());
                }
                let addresses = tokio::net::lookup_host((host.as_str(), 0))
                    .await
                    .map_err(|error| format!("DNS resolution failed: {error}"))?
                    .map(|address| address.ip().to_string())
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                Ok(serde_json::json!({ "addresses": addresses }))
            }
            PhpOperationKind::FileRead => {
                let path = path.ok_or_else(|| "native file read requires a path".to_owned())?;
                let limit = max_output_bytes.clamp(1, 256 * 1024 * 1024);
                let contents = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
                    let metadata = std::fs::metadata(&path)
                        .map_err(|error| format!("cannot inspect {path}: {error}"))?;
                    if metadata.len() > limit as u64 {
                        return Err(format!("file {path} exceeds the configured byte limit"));
                    }
                    let contents = std::fs::read(&path)
                        .map_err(|error| format!("cannot read {path}: {error}"))?;
                    if contents.len() > limit {
                        return Err(format!("file {path} exceeds the configured byte limit"));
                    }
                    Ok(contents)
                })
                .await
                .map_err(|error| format!("native file read task failed: {error}"))??;
                Ok(serde_json::json!({ "data": BASE64.encode(contents) }))
            }
            PhpOperationKind::FileWrite => {
                let path = path.ok_or_else(|| "native file write requires a path".to_owned())?;
                let contents = BASE64
                    .decode(data.ok_or_else(|| "native file write requires data".to_owned())?)
                    .map_err(|error| format!("native file write data is invalid: {error}"))?;
                if contents.len() > 256 * 1024 * 1024 {
                    return Err("native file write exceeds the 256 MiB limit".to_owned());
                }
                let bytes_written = contents.len();
                tokio::task::spawn_blocking(move || write_native_file(&path, &contents, atomic))
                    .await
                    .map_err(|error| format!("native file write task failed: {error}"))??;
                Ok(serde_json::json!({ "bytesWritten": bytes_written }))
            }
            PhpOperationKind::Process => {
                let timeout = operation_timeout;
                let result = tokio::task::spawn_blocking(move || {
                    run_native_process(arguments, stdin, max_output_bytes, timeout)
                })
                .await
                .map_err(|error| format!("native process task failed: {error}"))??;
                Ok(result)
            }
            PhpOperationKind::Signal => {
                let signal_number = signal_number
                    .filter(|signal| (1..=64).contains(signal))
                    .ok_or_else(|| {
                        "native signal operation requires a signal from 1 to 64".to_owned()
                    })?;
                let mut signal = tokio::signal::unix::signal(
                    tokio::signal::unix::SignalKind::from_raw(signal_number),
                )
                .map_err(|error| format!("cannot register signal {signal_number}: {error}"))?;
                signal
                    .recv()
                    .await
                    .ok_or_else(|| format!("signal {signal_number} watcher closed"))?;
                Ok(serde_json::json!({ "signal": signal_number }))
            }
            PhpOperationKind::Timer
            | PhpOperationKind::Stream
            | PhpOperationKind::ResponseChunk => {
                Err("invalid generic native operation kind".to_owned())
            }
        }
    };

    match tokio::time::timeout(task_timeout, task).await {
        Ok(result) => result,
        Err(_) => Err("native operation timed out".to_owned()),
    }
}

fn write_native_file(path: &str, contents: &[u8], atomic: bool) -> Result<(), String> {
    use std::io::Write as _;

    if !atomic {
        return std::fs::write(path, contents)
            .map_err(|error| format!("cannot write {path}: {error}"));
    }
    let destination = std::path::Path::new(path);
    let parent = destination
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "atomic file destination has an invalid name".to_owned())?;
    let temporary = parent.join(format!(
        ".{name}.pam-{}-{}-{}",
        std::process::id(),
        worker_epoch_millis(),
        NEXT_ATOMIC_FILE_ID.fetch_add(1, Ordering::Relaxed),
    ));
    let write_result = (|| -> Result<(), String> {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| format!("cannot create {}: {error}", temporary.display()))?;
        file.write_all(contents)
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("cannot persist {}: {error}", temporary.display()))?;
        std::fs::rename(&temporary, destination)
            .map_err(|error| format!("cannot replace {path}: {error}"))?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    write_result
}

fn run_native_process(
    arguments: Vec<String>,
    stdin: Option<String>,
    max_output_bytes: usize,
    timeout: Duration,
) -> Result<Value, String> {
    use std::io::{Read as _, Write as _};
    use std::os::unix::process::CommandExt as _;
    use std::os::unix::process::ExitStatusExt as _;
    use std::process::Stdio;

    let (program, arguments) = arguments
        .split_first()
        .ok_or_else(|| "native process requires a command".to_owned())?;
    if arguments
        .iter()
        .chain(std::iter::once(program))
        .any(|argument| argument.as_bytes().contains(&0))
    {
        return Err("native process arguments cannot contain NUL bytes".to_owned());
    }
    let input = BASE64
        .decode(stdin.unwrap_or_default())
        .map_err(|error| format!("native process stdin is invalid: {error}"))?;
    if input.len() > 64 * 1024 * 1024 {
        return Err("native process stdin exceeds the 64 MiB limit".to_owned());
    }
    let output_limit = max_output_bytes.clamp(1, 64 * 1024 * 1024);
    let mut command = std::process::Command::new(program);
    command
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot start {program}: {error}"))?;
    let process_group = child.id();

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "process stdout is unavailable".to_owned())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "process stderr is unavailable".to_owned())?;
    let stdout_reader = std::thread::spawn(move || {
        let mut output = Vec::new();
        stdout
            .take((output_limit + 1) as u64)
            .read_to_end(&mut output)
            .map(|_| output)
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut output = Vec::new();
        stderr
            .take((output_limit + 1) as u64)
            .read_to_end(&mut output)
            .map(|_| output)
    });
    let stdin_writer = child
        .stdin
        .take()
        .map(|mut pipe| std::thread::spawn(move || pipe.write_all(&input)));

    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("cannot inspect process {program}: {error}"))?
        {
            signal_process_group(process_group, SIGKILL);
            break status;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            signal_process_group(process_group, SIGTERM);
            let termination_deadline = Instant::now() + Duration::from_millis(100);
            let terminated_status = loop {
                if let Some(status) = child
                    .try_wait()
                    .map_err(|error| format!("cannot inspect process {program}: {error}"))?
                {
                    signal_process_group(process_group, SIGKILL);
                    break status;
                }
                if Instant::now() >= termination_deadline {
                    signal_process_group(process_group, SIGKILL);
                    break child
                        .wait()
                        .map_err(|error| format!("cannot reap process {program}: {error}"))?;
                }
                std::thread::sleep(Duration::from_millis(1));
            };
            break terminated_status;
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    if let Some(writer) = stdin_writer {
        let _ = writer.join();
    }
    let mut stdout = stdout_reader
        .join()
        .map_err(|_| "native process stdout reader panicked".to_owned())?
        .map_err(|error| format!("cannot read process stdout: {error}"))?;
    let mut stderr = stderr_reader
        .join()
        .map_err(|_| "native process stderr reader panicked".to_owned())?
        .map_err(|error| format!("cannot read process stderr: {error}"))?;
    stdout.truncate(output_limit);
    stderr.truncate(output_limit);
    let kind = if timed_out {
        2
    } else if status.signal().is_some() {
        3
    } else {
        1
    };
    Ok(serde_json::json!({
        "kind": kind,
        "exitCode": status.code().unwrap_or(-1),
        "stdout": BASE64.encode(stdout),
        "stderr": BASE64.encode(stderr),
    }))
}

fn signal_process_group(process_group: u32, signal: i32) {
    let Ok(process_group) = i32::try_from(process_group) else {
        return;
    };
    // SAFETY: The child starts in a dedicated process group whose ID is its PID.
    // A negative PID targets only that group; failures such as ESRCH are harmless.
    unsafe {
        kill(-process_group, signal);
    }
}

#[derive(Debug, Deserialize)]
struct PhpHttpResponse {
    status: u16,
    headers: HashMap<String, Vec<String>>,
    body: String,
    #[serde(default)]
    chunks: Vec<String>,
}

impl PhpHttpResponse {
    fn total_bytes(&self) -> Option<usize> {
        self.chunks
            .iter()
            .try_fold(self.body.len(), |total, chunk| {
                total.checked_add(chunk.len())
            })
    }

    fn declared_content_length(&self) -> Option<usize> {
        self.headers
            .get("content-length")
            .and_then(|values| values.first())
            .and_then(|value| value.parse().ok())
    }

    fn into_http_response(self) -> Response {
        let status = match StatusCode::from_u16(self.status) {
            Ok(status) => status,
            Err(error) => return internal_error(format!("invalid HTTP status from PHP: {error}")),
        };
        let mut response = Response::builder().status(status);

        for (name, values) in self.headers {
            let name = match HeaderName::try_from(name) {
                Ok(name) => name,
                Err(error) => {
                    return internal_error(format!("invalid header name from PHP: {error}"));
                }
            };
            for value in values {
                let value = match HeaderValue::try_from(value) {
                    Ok(value) => value,
                    Err(error) => {
                        return internal_error(format!("invalid header value from PHP: {error}"));
                    }
                };
                response = response.header(name.clone(), value);
            }
        }

        if self.chunks.is_empty() {
            response
                .body(Body::from(self.body))
                .unwrap_or_else(|error| {
                    internal_error(format!("cannot build PHP response: {error}"))
                })
        } else {
            let chunks = std::iter::once(self.body)
                .chain(self.chunks)
                .map(|chunk| Ok::<Bytes, Infallible>(Bytes::from(chunk)));
            response
                .body(Body::from_stream(futures_util::stream::iter(chunks)))
                .unwrap_or_else(|error| {
                    internal_error(format!("cannot build PHP response: {error}"))
                })
        }
    }

    fn into_streaming_http_response(self, body: Body) -> Response {
        let status = match StatusCode::from_u16(self.status) {
            Ok(status) => status,
            Err(error) => return internal_error(format!("invalid HTTP status from PHP: {error}")),
        };
        let mut response = Response::builder().status(status);
        for (name, values) in self.headers {
            let name = match HeaderName::try_from(name) {
                Ok(name) => name,
                Err(error) => {
                    return internal_error(format!("invalid header name from PHP: {error}"));
                }
            };
            for value in values {
                let value = match HeaderValue::try_from(value) {
                    Ok(value) => value,
                    Err(error) => {
                        return internal_error(format!("invalid header value from PHP: {error}"));
                    }
                };
                response = response.header(name.clone(), value);
            }
        }
        response
            .body(body)
            .unwrap_or_else(|error| internal_error(format!("cannot build PHP stream: {error}")))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HttpRequestContext<'a> {
    protocol_version: &'a str,
    remote_address: &'a str,
    remote_port: u16,
    scheme: &'a str,
    request_id: &'a str,
    traceparent: &'a str,
}

#[repr(i32)]
#[derive(Clone, Copy)]
enum InboundEventKind {
    Connection = 1,
    Message = 2,
    Disconnect = 3,
    Binary = 4,
    Tick = 5,
}

#[repr(u8)]
#[derive(Debug, Deserialize)]
#[serde(try_from = "u8")]
enum OutboundTarget {
    Socket = 1,
    Broadcast = 2,
    Room = 3,
}

impl TryFrom<u8> for OutboundTarget {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Socket),
            2 => Ok(Self::Broadcast),
            3 => Ok(Self::Room),
            _ => Err(format!("unknown outbound target {value}")),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WsCommand {
    target: OutboundTarget,
    kind: OutboundMessageKind,
    socket_ids: Vec<String>,
    event: String,
    data: Value,
}

#[repr(u8)]
#[derive(Debug, Deserialize)]
#[serde(try_from = "u8")]
enum OutboundMessageKind {
    Event = 1,
    Acknowledgement = 2,
    Binary = 3,
    Close = 4,
}

impl TryFrom<u8> for OutboundMessageKind {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Event),
            2 => Ok(Self::Acknowledgement),
            3 => Ok(Self::Binary),
            4 => Ok(Self::Close),
            _ => Err(format!("unknown outbound message kind {value}")),
        }
    }
}

impl WsCommand {
    fn has_valid_recipients(&self) -> bool {
        match self.target {
            OutboundTarget::Socket => self.socket_ids.len() == 1,
            OutboundTarget::Broadcast | OutboundTarget::Room => true,
        }
    }
}

#[derive(Serialize)]
struct WsEvent<'a> {
    event: &'a str,
    data: &'a Value,
}

#[derive(Serialize)]
struct WsAcknowledgement<'a> {
    #[serde(rename = "ack")]
    acknowledgement: &'a str,
    data: &'a Value,
}

fn epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn query_parameter(query: Option<&str>, name: &str) -> Option<String> {
    query?.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        (key == name).then(|| value.to_owned())
    })
}

fn is_safe_session_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn create_resume_token(secret: &str, session_id: &str, ttl_seconds: u64) -> String {
    let expires = epoch_millis() / 1_000 + ttl_seconds;
    let signature = resume_signature(secret, session_id, expires);
    format!("{expires}.{}", encode_hex(&signature))
}

fn verify_resume_token(secret: &str, session_id: &str, token: &str) -> bool {
    let Some((expires, signature)) = token.split_once('.') else {
        return false;
    };
    let Ok(expires) = expires.parse::<u64>() else {
        return false;
    };
    if expires < epoch_millis() / 1_000 {
        return false;
    }
    let Some(signature) = decode_hex(signature) else {
        return false;
    };
    let Ok(mut hmac) = ResumeHmac::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    hmac.update(b"pam.websocket.resume.v1\0");
    hmac.update(session_id.as_bytes());
    hmac.update(b":");
    hmac.update(expires.to_string().as_bytes());
    hmac.verify_slice(&signature).is_ok()
}

fn resume_signature(secret: &str, session_id: &str, expires: u64) -> Vec<u8> {
    let mut hmac = ResumeHmac::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts keys of any non-empty length");
    hmac.update(b"pam.websocket.resume.v1\0");
    hmac.update(session_id.as_bytes());
    hmac.update(b":");
    hmac.update(expires.to_string().as_bytes());
    hmac.finalize().into_bytes().to_vec()
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16)? as u8;
            let low = (pair[1] as char).to_digit(16)? as u8;
            Some((high << 4) | low)
        })
        .collect()
}

fn request_identifier(headers: &HeaderMap) -> Option<String> {
    let value = headers.get("x-request-id")?.to_str().ok()?;
    (value.len() <= 128
        && !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')))
    .then(|| value.to_owned())
}

fn traceparent(headers: &HeaderMap, state: &ServerState) -> String {
    if let Some(value) = headers
        .get("traceparent")
        .and_then(|value| value.to_str().ok())
        .filter(|value| valid_traceparent(value))
    {
        return value.to_owned();
    }

    let sequence = state.next_request_id.load(Ordering::Relaxed) as u128;
    let timestamp = epoch_millis() as u128;
    let pid = std::process::id() as u128;
    let trace_id = (timestamp << 64) ^ (pid << 32) ^ sequence;
    let span_id = ((timestamp as u64).rotate_left(17)) ^ sequence as u64;
    format!("00-{trace_id:032x}-{span_id:016x}-01")
}

fn request_traceparent(headers: &HeaderMap, state: &ServerState) -> String {
    if state.config.telemetry_headers || headers.contains_key("traceparent") {
        traceparent(headers, state)
    } else {
        String::new()
    }
}

fn valid_traceparent(value: &str) -> bool {
    let parts = value.split('-').collect::<Vec<_>>();
    parts.len() == 4
        && parts[0].len() == 2
        && parts[0] != "ff"
        && parts[1].len() == 32
        && parts[1] != "00000000000000000000000000000000"
        && parts[2].len() == 16
        && parts[2] != "0000000000000000"
        && parts[3].len() == 2
        && parts
            .iter()
            .all(|part| part.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn header_bytes(headers: &HeaderMap) -> usize {
    headers
        .iter()
        .map(|(name, value)| name.as_str().len() + value.as_bytes().len() + 4)
        .sum()
}

fn client_address(remote: SocketAddr, headers: &HeaderMap, trusted_proxies: &[String]) -> String {
    let direct = remote.ip().to_string();
    let trusted = trusted_proxies
        .iter()
        .any(|proxy| proxy == "*" || proxy == &direct);
    if !trusted {
        return direct;
    }

    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .map(str::to_owned)
        .unwrap_or(direct)
}

fn allowed_cors_origin(headers: &HeaderMap, configured: &[String]) -> Option<String> {
    let origin = headers.get("origin")?.to_str().ok()?;
    configured
        .iter()
        .find(|allowed| allowed.as_str() == "*" || allowed.as_str() == origin)
        .map(|allowed| {
            if allowed == "*" {
                "*".to_owned()
            } else {
                origin.to_owned()
            }
        })
}

fn error_response(status: StatusCode, message: &str) -> Response {
    let payload = serde_json::json!({
        "error": status.canonical_reason().unwrap_or("Request rejected"),
        "message": message,
    });
    Response::builder()
        .status(status)
        .header("content-type", "application/json; charset=utf-8")
        .body(Body::from(payload.to_string()))
        .expect("static error response must be valid")
}

fn resident_memory_bytes() -> u64 {
    std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|contents| contents.split_whitespace().nth(1)?.parse::<u64>().ok())
        .map(|pages| pages * 4096)
        .unwrap_or(0)
}

fn monitor_event_loop(metrics: Arc<Metrics>) {
    tokio::spawn(async move {
        let period = Duration::from_millis(500);
        let mut expected = Instant::now() + period;
        let mut interval = tokio::time::interval_at(expected.into(), period);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            let now = Instant::now();
            metrics.event_loop_lag_micros.store(
                now.saturating_duration_since(expected).as_micros() as u64,
                Ordering::Relaxed,
            );
            expected = now + period;
        }
    });
}
