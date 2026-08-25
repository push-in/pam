use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::fmt;
use std::fs::OpenOptions;
use std::future::Future;
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::os::fd::{AsRawFd, RawFd};
use std::path::PathBuf;
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
use sha2::{Digest, Sha256};
use tokio::io::Interest;
use tokio::io::unix::AsyncFd;
use tokio::sync::{Mutex, OwnedSemaphorePermit, RwLock, Semaphore, mpsc, oneshot, watch};
use tokio::task::LocalSet;
use yawc::close::CloseCode;
use yawc::frame::{Frame, OpCode};
use yawc::{CompressionLevel, IncomingUpgrade, Options, WebSocket};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use socket2::{Domain, Protocol, Socket, Type};

use crate::php::{self, ServerConfig};
use crate::protocol::{
    PhpDispatchState, PhpHttpResponse, PhpOperation, PhpOperationKind, parse_dispatch_envelope,
};
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
const REQUEST_DURATION_UPPER_BOUNDS_MICROS: [u64; 10] = [
    100, 500, 1_000, 5_000, 10_000, 50_000, 100_000, 500_000, 1_000_000, 5_000_000,
];

unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}

#[derive(Clone)]
struct CachedResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
    expires_at: Instant,
    stale_until: Instant,
    last_access: Arc<AtomicU64>,
    tags: Arc<HashSet<String>>,
}

impl CachedResponse {
    fn response(&self) -> Response {
        let mut response = Response::new(Body::from(self.body.clone()));
        *response.status_mut() = self.status;
        *response.headers_mut() = self.headers.clone();
        response
    }
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
    otlp: Option<crate::otlp::Exporter>,
    rate_limits: Arc<Mutex<HashMap<String, RateBucket>>>,
    worker_state: WorkerStateReporter,
    next_worker_metrics_at: Arc<AtomicU64>,
    last_reported_active_requests: Arc<AtomicU64>,
    worker_metrics_refresh_scheduled: Arc<AtomicBool>,
    active_php_executions: Arc<StdMutex<HashMap<String, u64>>>,
    response_cache: Arc<RwLock<HashMap<String, CachedResponse>>>,
    response_cache_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    response_cache_clock: Arc<AtomicU64>,
    php_metrics: Arc<StdMutex<php::RuntimeMetrics>>,
    next_php_metrics_at: Arc<AtomicU64>,
    cache_invalidation_log: Option<Arc<PathBuf>>,
    cache_invalidation_offset: Arc<AtomicU64>,
    next_cache_invalidation_at: Arc<AtomicU64>,
    php: PhpExecutor,
}

impl ServerState {
    fn new(config: ServerConfig, php: PhpExecutor) -> Result<Self, String> {
        let (shutdown, _) = watch::channel(false);
        let worker = std::env::var("PAM_WORKER_ID").unwrap_or_else(|_| "standalone".to_owned());
        let route_metrics = config.route_metrics;
        let route_metrics_max_entries = config.route_metrics_max_entries;
        let otlp = crate::otlp::Exporter::from_environment()?;
        let otlp_counters = otlp
            .as_ref()
            .map_or_else(crate::otlp::Counters::default, |exporter| {
                exporter.counters.clone()
            });
        Ok(Self {
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
            metrics: Arc::new(Metrics::new(
                route_metrics,
                route_metrics_max_entries,
                otlp_counters,
            )),
            otlp,
            rate_limits: Arc::new(Mutex::new(HashMap::new())),
            worker_state: WorkerStateReporter::from_environment(),
            next_worker_metrics_at: Arc::new(AtomicU64::new(0)),
            last_reported_active_requests: Arc::new(AtomicU64::new(0)),
            worker_metrics_refresh_scheduled: Arc::new(AtomicBool::new(false)),
            active_php_executions: Arc::new(StdMutex::new(HashMap::new())),
            response_cache: Arc::new(RwLock::new(HashMap::new())),
            response_cache_locks: Arc::new(Mutex::new(HashMap::new())),
            response_cache_clock: Arc::new(AtomicU64::new(1)),
            php_metrics: Arc::new(StdMutex::new(php::RuntimeMetrics::default())),
            next_php_metrics_at: Arc::new(AtomicU64::new(0)),
            cache_invalidation_log: std::env::var_os("PAM_CACHE_INVALIDATION_LOG")
                .map(PathBuf::from)
                .map(Arc::new),
            cache_invalidation_offset: Arc::new(AtomicU64::new(0)),
            next_cache_invalidation_at: Arc::new(AtomicU64::new(0)),
            php,
        })
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

    fn response_cache_key(
        &self,
        method: &Method,
        uri: &Uri,
        headers: &HeaderMap,
    ) -> Option<String> {
        if method != Method::GET
            || self.config.response_cache_max_entries == 0
            || self.config.response_cache_max_bytes == 0
            || self.config.response_cache_ttl_ms == 0
            || !self
                .config
                .response_cache_paths
                .iter()
                .any(|path| path == uri.path())
            || headers.contains_key("authorization")
            || headers.contains_key("cookie")
            || headers
                .get("cache-control")
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| {
                    value
                        .split(',')
                        .any(|directive| directive.trim().eq_ignore_ascii_case("no-cache"))
                })
        {
            return None;
        }
        let mut material = uri
            .path_and_query()
            .map_or_else(|| uri.path().to_owned(), ToString::to_string);
        for configured in &self.config.response_cache_vary_headers {
            let name = configured.to_ascii_lowercase();
            material.push('\n');
            material.push_str(&name);
            material.push(':');
            for value in headers.get_all(name.as_str()) {
                if let Ok(value) = value.to_str() {
                    material.push_str(&value.len().to_string());
                    material.push(':');
                    material.push_str(value);
                    material.push(';');
                }
            }
        }
        Some(format!("{:x}", Sha256::digest(material.as_bytes())))
    }

    async fn cached_response(&self, key: &str, allow_stale: bool) -> Option<(Response, usize)> {
        let now = Instant::now();
        let cached = self.response_cache.read().await.get(key).cloned();
        match cached {
            Some(cached)
                if cached.expires_at > now || (allow_stale && cached.stale_until > now) =>
            {
                cached.last_access.store(
                    self.response_cache_clock.fetch_add(1, Ordering::Relaxed),
                    Ordering::Relaxed,
                );
                let bytes = cached.body.len();
                Some((cached.response(), bytes))
            }
            Some(cached) if cached.stale_until <= now => {
                self.response_cache.write().await.remove(key);
                None
            }
            Some(_) => None,
            None => None,
        }
    }

    async fn response_cache_lock(&self, key: &str) -> Arc<Mutex<()>> {
        self.response_cache_locks
            .lock()
            .await
            .entry(key.to_owned())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    async fn store_cached_response(&self, key: String, cached: CachedResponse) {
        let mut cache = self.response_cache.write().await;
        let now = Instant::now();
        cache.retain(|_, entry| entry.stale_until > now);
        if cached.body.len() > self.config.response_cache_max_bytes {
            return;
        }
        while !cache.contains_key(&key)
            && (cache.len() >= self.config.response_cache_max_entries
                || cache.values().map(|entry| entry.body.len()).sum::<usize>() + cached.body.len()
                    > self.config.response_cache_max_bytes)
        {
            let Some(eviction_key) = cache
                .iter()
                .min_by_key(|(_, entry)| entry.last_access.load(Ordering::Relaxed))
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            cache.remove(&eviction_key);
        }
        cache.insert(key, cached);
    }

    async fn purge_cached_responses(&self, tag: Option<&str>) -> usize {
        let mut cache = self.response_cache.write().await;
        let before = cache.len();
        match tag {
            Some(tag) => cache.retain(|_, entry| !entry.tags.contains(tag)),
            None => cache.clear(),
        }
        let purged = before.saturating_sub(cache.len());
        self.metrics
            .response_cache_purges
            .fetch_add(1, Ordering::Relaxed);
        purged
    }

    async fn synchronize_cache_invalidations(&self, force: bool) {
        let Some(path) = self.cache_invalidation_log.as_deref() else {
            return;
        };
        let now = worker_epoch_millis();
        let next = self.next_cache_invalidation_at.load(Ordering::Relaxed);
        if !force
            && (now < next
                || self
                    .next_cache_invalidation_at
                    .compare_exchange(
                        next,
                        now.saturating_add(100),
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    )
                    .is_err())
        {
            return;
        }
        self.next_cache_invalidation_at
            .store(now.saturating_add(100), Ordering::Relaxed);

        let Ok(mut file) = OpenOptions::new().read(true).open(path) else {
            return;
        };
        let length = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
        let mut offset = self.cache_invalidation_offset.load(Ordering::Relaxed);
        let mut purge_all = length < offset;
        if purge_all {
            offset = 0;
        }
        if file.seek(SeekFrom::Start(offset)).is_err() {
            return;
        }
        let mut reader = BufReader::new(file);
        let mut tags = HashSet::new();
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let invalidation = line.trim();
                    if invalidation == "*" {
                        purge_all = true;
                    } else if valid_cache_tag(invalidation) {
                        tags.insert(invalidation.to_owned());
                    }
                }
                Err(_) => return,
            }
        }
        if let Ok(position) = reader.stream_position() {
            self.cache_invalidation_offset
                .store(position, Ordering::Relaxed);
        }
        if purge_all || !tags.is_empty() {
            let mut cache = self.response_cache.write().await;
            if purge_all {
                cache.clear();
            } else {
                cache.retain(|_, entry| tags.is_disjoint(&entry.tags));
            }
        }
    }

    async fn publish_cache_invalidation(&self, tag: Option<&str>) -> Result<(), String> {
        let Some(path) = self.cache_invalidation_log.as_deref() else {
            return Ok(());
        };
        self.synchronize_cache_invalidations(true).await;
        #[cfg(unix)]
        use std::os::unix::fs::OpenOptionsExt;
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options
            .open(path)
            .map_err(|error| format!("cannot open cache invalidation log: {error}"))?;
        let record = format!("{}\n", tag.unwrap_or("*"));
        file.write_all(record.as_bytes())
            .and_then(|()| file.sync_data())
            .map_err(|error| format!("cannot publish cache invalidation: {error}"))?;
        if let Ok(length) = file.metadata().map(|metadata| metadata.len()) {
            self.cache_invalidation_offset
                .store(length, Ordering::Relaxed);
        }
        Ok(())
    }

    fn metrics_snapshot(&self) -> WorkerMetricsSnapshot {
        let runtime = self
            .php_metrics
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        WorkerMetricsSnapshot {
            requests: self.metrics.requests.load(Ordering::Relaxed),
            errors: self.metrics.errors.load(Ordering::Relaxed),
            client_disconnect_cancellations: self
                .metrics
                .client_disconnect_cancellations
                .load(Ordering::Relaxed),
            active_requests: self.metrics.active_requests.load(Ordering::Relaxed),
            request_duration_micros: self.metrics.request_duration_micros.load(Ordering::Relaxed),
            request_duration_buckets: std::array::from_fn(|index| {
                self.metrics.request_duration_buckets[index].load(Ordering::Relaxed)
            }),
            request_bytes: self.metrics.request_bytes.load(Ordering::Relaxed),
            response_bytes: self.metrics.response_bytes.load(Ordering::Relaxed),
            response_cache_hits: self.metrics.response_cache_hits.load(Ordering::Relaxed),
            response_cache_misses: self.metrics.response_cache_misses.load(Ordering::Relaxed),
            response_cache_collapsed: self
                .metrics
                .response_cache_collapsed
                .load(Ordering::Relaxed),
            response_cache_stale: self.metrics.response_cache_stale.load(Ordering::Relaxed),
            response_cache_purges: self.metrics.response_cache_purges.load(Ordering::Relaxed),
            websocket_connections: self.metrics.websocket_connections.load(Ordering::Relaxed),
            websocket_messages: self.metrics.websocket_messages.load(Ordering::Relaxed),
            websocket_backpressure: self.metrics.websocket_backpressure.load(Ordering::Relaxed),
            event_loop_lag_micros: self.metrics.event_loop_lag_micros.load(Ordering::Relaxed),
            event_loop_lag_max_micros: self
                .metrics
                .event_loop_lag_max_micros
                .load(Ordering::Relaxed),
            event_loop_lag_total_micros: self
                .metrics
                .event_loop_lag_total_micros
                .load(Ordering::Relaxed),
            event_loop_lag_samples: self.metrics.event_loop_lag_samples.load(Ordering::Relaxed),
            resident_memory_bytes: resident_memory_bytes(),
            php_memory_bytes: runtime.memory_bytes,
            php_peak_memory_bytes: runtime.peak_memory_bytes,
            php_fibers: runtime.fibers,
        }
    }

    fn refresh_php_metrics_on_owner_thread(&self, force: bool) -> php::RuntimeMetrics {
        let now = worker_epoch_millis();
        let next = self.next_php_metrics_at.load(Ordering::Relaxed);
        if force
            || (now >= next
                && self
                    .next_php_metrics_at
                    .compare_exchange(
                        next,
                        now.saturating_add(1_000),
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    )
                    .is_ok())
        {
            let runtime = php::runtime_metrics().unwrap_or_default();
            *self
                .php_metrics
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = runtime.clone();
            runtime
        } else {
            self.php_metrics
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
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

#[derive(Clone)]
struct PhpExecutor {
    sender: mpsc::Sender<PhpJob>,
}

struct OwnedHttpRequest {
    remote: SocketAddr,
    client: String,
    method: Method,
    target: String,
    version: Version,
    headers: HeaderMap,
    body: Bytes,
    request_id: String,
    traceparent: String,
}

enum PhpJob {
    Http {
        request: Box<OwnedHttpRequest>,
        response: oneshot::Sender<(Response, usize)>,
    },
    WebSocket {
        event_kind: i32,
        socket_id: String,
        payload: String,
        response: oneshot::Sender<Result<String, String>>,
    },
    Metrics {
        response: oneshot::Sender<php::RuntimeMetrics>,
    },
}

impl PhpExecutor {
    async fn http(&self, request: OwnedHttpRequest) -> (Response, usize) {
        let (response, receiver) = oneshot::channel();
        if let Err(error) = self.sender.try_send(PhpJob::Http {
            request: Box::new(request),
            response,
        }) {
            return match error {
                mpsc::error::TrySendError::Full(_) => {
                    (service_unavailable("PHP executor queue is full"), 0)
                }
                mpsc::error::TrySendError::Closed(_) => {
                    (internal_error("PHP executor is unavailable"), 0)
                }
            };
        }
        receiver
            .await
            .unwrap_or_else(|_| (internal_error("PHP executor stopped"), 0))
    }

    async fn websocket(
        &self,
        event_kind: i32,
        socket_id: &str,
        payload: &str,
    ) -> Result<String, String> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .try_send(PhpJob::WebSocket {
                event_kind,
                socket_id: socket_id.to_owned(),
                payload: payload.to_owned(),
                response,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => "PHP executor queue is full".to_owned(),
                mpsc::error::TrySendError::Closed(_) => "PHP executor is unavailable".to_owned(),
            })?;
        receiver
            .await
            .map_err(|_| "PHP executor stopped".to_owned())?
    }

    async fn metrics(&self) -> php::RuntimeMetrics {
        let (response, receiver) = oneshot::channel();
        if self.sender.try_send(PhpJob::Metrics { response }).is_err() {
            return php::RuntimeMetrics::default();
        }
        receiver.await.unwrap_or_default()
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

struct Metrics {
    requests: AtomicU64,
    errors: AtomicU64,
    client_disconnect_cancellations: AtomicU64,
    active_requests: AtomicU64,
    request_duration_micros: AtomicU64,
    request_duration_buckets: [AtomicU64; 11],
    request_bytes: AtomicU64,
    response_bytes: AtomicU64,
    response_cache_hits: AtomicU64,
    response_cache_misses: AtomicU64,
    response_cache_collapsed: AtomicU64,
    response_cache_stale: AtomicU64,
    response_cache_purges: AtomicU64,
    websocket_connections: AtomicU64,
    websocket_messages: AtomicU64,
    websocket_backpressure: AtomicU64,
    event_loop_lag_micros: AtomicU64,
    event_loop_lag_max_micros: AtomicU64,
    event_loop_lag_total_micros: AtomicU64,
    event_loop_lag_samples: AtomicU64,
    route_metrics_enabled: bool,
    route_metrics_max_entries: usize,
    route_metrics: StdMutex<HashMap<RouteMetricKey, RouteMetric>>,
    route_metrics_overflow: AtomicU64,
    otlp: crate::otlp::Counters,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct RouteMetricKey {
    method: String,
    route: String,
    status: u16,
}

#[derive(Default)]
struct RouteMetric {
    requests: u64,
    duration_micros: u64,
    duration_buckets: [u64; 11],
}

struct RateBucket {
    available: f64,
    updated: Instant,
}

impl Metrics {
    fn new(
        route_metrics_enabled: bool,
        route_metrics_max_entries: usize,
        otlp: crate::otlp::Counters,
    ) -> Self {
        Self {
            requests: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            client_disconnect_cancellations: AtomicU64::new(0),
            active_requests: AtomicU64::new(0),
            request_duration_micros: AtomicU64::new(0),
            request_duration_buckets: Default::default(),
            request_bytes: AtomicU64::new(0),
            response_bytes: AtomicU64::new(0),
            response_cache_hits: AtomicU64::new(0),
            response_cache_misses: AtomicU64::new(0),
            response_cache_collapsed: AtomicU64::new(0),
            response_cache_stale: AtomicU64::new(0),
            response_cache_purges: AtomicU64::new(0),
            websocket_connections: AtomicU64::new(0),
            websocket_messages: AtomicU64::new(0),
            websocket_backpressure: AtomicU64::new(0),
            event_loop_lag_micros: AtomicU64::new(0),
            event_loop_lag_max_micros: AtomicU64::new(0),
            event_loop_lag_total_micros: AtomicU64::new(0),
            event_loop_lag_samples: AtomicU64::new(0),
            route_metrics_enabled,
            route_metrics_max_entries,
            route_metrics: StdMutex::new(HashMap::new()),
            route_metrics_overflow: AtomicU64::new(0),
            otlp,
        }
    }

    fn render(&self, websocket_connections: usize, runtime: &php::RuntimeMetrics) -> String {
        let mut output = format!(
            concat!(
                "# TYPE pam_http_requests_total counter\n",
                "pam_http_requests_total {}\n",
                "# TYPE pam_http_errors_total counter\n",
                "pam_http_errors_total {}\n",
                "# TYPE pam_http_client_disconnect_cancellations_total counter\n",
                "pam_http_client_disconnect_cancellations_total {}\n",
                "# TYPE pam_http_active_requests gauge\n",
                "pam_http_active_requests {}\n",
                "# TYPE pam_http_request_bytes_total counter\n",
                "pam_http_request_bytes_total {}\n",
                "# TYPE pam_http_response_bytes_total counter\n",
                "pam_http_response_bytes_total {}\n",
                "# TYPE pam_http_response_cache_hits_total counter\n",
                "pam_http_response_cache_hits_total {}\n",
                "# TYPE pam_http_response_cache_misses_total counter\n",
                "pam_http_response_cache_misses_total {}\n",
                "# TYPE pam_http_response_cache_collapsed_total counter\n",
                "pam_http_response_cache_collapsed_total {}\n",
                "# TYPE pam_http_response_cache_stale_total counter\n",
                "pam_http_response_cache_stale_total {}\n",
                "# TYPE pam_http_response_cache_purges_total counter\n",
                "pam_http_response_cache_purges_total {}\n",
                "# TYPE pam_websocket_connections gauge\n",
                "pam_websocket_connections {}\n",
                "# TYPE pam_websocket_messages_total counter\n",
                "pam_websocket_messages_total {}\n",
                "# TYPE pam_websocket_backpressure_total counter\n",
                "pam_websocket_backpressure_total {}\n",
                "# TYPE pam_event_loop_lag_seconds gauge\n",
                "pam_event_loop_lag_seconds {:.6}\n",
                "# TYPE pam_event_loop_lag_max_seconds gauge\n",
                "pam_event_loop_lag_max_seconds {:.6}\n",
                "# TYPE pam_event_loop_lag_average_seconds gauge\n",
                "pam_event_loop_lag_average_seconds {:.6}\n",
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
                "# TYPE pam_otlp_spans_exported_total counter\n",
                "pam_otlp_spans_exported_total {}\n",
                "# TYPE pam_otlp_spans_dropped_total counter\n",
                "pam_otlp_spans_dropped_total {}\n",
                "# TYPE pam_otlp_export_errors_total counter\n",
                "pam_otlp_export_errors_total {}\n",
                "# TYPE pam_otlp_spans_rejected_total counter\n",
                "pam_otlp_spans_rejected_total {}\n",
            ),
            self.requests.load(Ordering::Relaxed),
            self.errors.load(Ordering::Relaxed),
            self.client_disconnect_cancellations.load(Ordering::Relaxed),
            self.active_requests.load(Ordering::Relaxed),
            self.request_bytes.load(Ordering::Relaxed),
            self.response_bytes.load(Ordering::Relaxed),
            self.response_cache_hits.load(Ordering::Relaxed),
            self.response_cache_misses.load(Ordering::Relaxed),
            self.response_cache_collapsed.load(Ordering::Relaxed),
            self.response_cache_stale.load(Ordering::Relaxed),
            self.response_cache_purges.load(Ordering::Relaxed),
            websocket_connections,
            self.websocket_messages.load(Ordering::Relaxed),
            self.websocket_backpressure.load(Ordering::Relaxed),
            self.event_loop_lag_micros.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            self.event_loop_lag_max_micros.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            event_loop_lag_average_micros(
                self.event_loop_lag_total_micros.load(Ordering::Relaxed),
                self.event_loop_lag_samples.load(Ordering::Relaxed),
            ) / 1_000_000.0,
            resident_memory_bytes(),
            runtime.memory_bytes,
            runtime.peak_memory_bytes,
            runtime.fibers,
            crate::prometheus::label(
                &std::env::var("PAM_WORKER_ID").unwrap_or_else(|_| "standalone".to_owned())
            ),
            crate::prometheus::label(
                &std::env::var("PAM_WORKER_GENERATION").unwrap_or_else(|_| "1".to_owned())
            ),
            std::process::id(),
            self.otlp.exported.load(Ordering::Relaxed),
            self.otlp.dropped.load(Ordering::Relaxed),
            self.otlp.errors.load(Ordering::Relaxed),
            self.otlp.rejected.load(Ordering::Relaxed),
        );
        output.push_str("# TYPE pam_http_request_duration_seconds histogram\n");
        let labels = [
            "0.0001", "0.0005", "0.001", "0.005", "0.01", "0.05", "0.1", "0.5", "1", "5", "+Inf",
        ];
        let mut cumulative = 0_u64;
        for (label, bucket) in labels.iter().zip(&self.request_duration_buckets) {
            cumulative = cumulative.saturating_add(bucket.load(Ordering::Relaxed));
            output.push_str(&format!(
                "pam_http_request_duration_seconds_bucket{{le=\"{label}\"}} {cumulative}\n"
            ));
        }
        output.push_str(&format!(
            "pam_http_request_duration_seconds_sum {:.6}\npam_http_request_duration_seconds_count {}\n",
            self.request_duration_micros.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            self.requests.load(Ordering::Relaxed),
        ));
        output.push_str("# TYPE pam_http_route_metrics_overflow_total counter\n");
        output.push_str(&format!(
            "pam_http_route_metrics_overflow_total {}\n",
            self.route_metrics_overflow.load(Ordering::Relaxed),
        ));
        if self.route_metrics_enabled {
            output.push_str("# TYPE pam_http_route_requests_total counter\n");
            output.push_str("# TYPE pam_http_route_request_duration_seconds histogram\n");
            let routes = self
                .route_metrics
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            for (key, metric) in routes.iter() {
                let method = crate::prometheus::label(&key.method);
                let route = crate::prometheus::label(&key.route);
                let labels = format!(
                    "method=\"{method}\",route=\"{route}\",status=\"{}\"",
                    key.status,
                );
                output.push_str(&format!(
                    "pam_http_route_requests_total{{{labels}}} {}\n",
                    metric.requests,
                ));
                let mut cumulative = 0_u64;
                for (label, bucket) in [
                    "0.0001", "0.0005", "0.001", "0.005", "0.01", "0.05", "0.1", "0.5", "1", "5",
                    "+Inf",
                ]
                .iter()
                .zip(metric.duration_buckets)
                {
                    cumulative = cumulative.saturating_add(bucket);
                    output.push_str(&format!(
                        "pam_http_route_request_duration_seconds_bucket{{{labels},le=\"{label}\"}} {cumulative}\n"
                    ));
                }
                output.push_str(&format!(
                    "pam_http_route_request_duration_seconds_sum{{{labels}}} {:.6}\npam_http_route_request_duration_seconds_count{{{labels}}} {}\n",
                    metric.duration_micros as f64 / 1_000_000.0,
                    metric.requests,
                ));
            }
        }
        output
    }

    fn observe_request_duration(&self, duration: Duration) {
        let micros = u64::try_from(duration.as_micros()).unwrap_or(u64::MAX);
        let bucket = REQUEST_DURATION_UPPER_BOUNDS_MICROS
            .iter()
            .position(|upper| micros <= *upper)
            .unwrap_or(REQUEST_DURATION_UPPER_BOUNDS_MICROS.len());
        self.request_duration_buckets[bucket].fetch_add(1, Ordering::Relaxed);
    }

    fn observe_route(&self, method: &Method, route: &str, status: StatusCode, duration: Duration) {
        if !self.route_metrics_enabled || self.route_metrics_max_entries == 0 {
            return;
        }
        let key = RouteMetricKey {
            method: method.as_str().to_owned(),
            route: route.to_owned(),
            status: status.as_u16(),
        };
        let mut routes = self
            .route_metrics
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !routes.contains_key(&key) && routes.len() >= self.route_metrics_max_entries {
            self.route_metrics_overflow.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let metric = routes.entry(key).or_default();
        metric.requests = metric.requests.saturating_add(1);
        let micros = u64::try_from(duration.as_micros()).unwrap_or(u64::MAX);
        metric.duration_micros = metric.duration_micros.saturating_add(micros);
        let bucket = REQUEST_DURATION_UPPER_BOUNDS_MICROS
            .iter()
            .position(|upper| micros <= *upper)
            .unwrap_or(REQUEST_DURATION_UPPER_BOUNDS_MICROS.len());
        metric.duration_buckets[bucket] = metric.duration_buckets[bucket].saturating_add(1);
    }
}

struct RequestTelemetry {
    state: ServerState,
    metrics: Arc<Metrics>,
    started: Instant,
    started_unix_nanos: Option<u64>,
    parent_span_id: Option<String>,
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
        parent_span_id: Option<String>,
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
            started_unix_nanos: state.otlp.as_ref().map(|_| epoch_nanos()),
            parent_span_id,
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
        let route_template = response
            .headers_mut()
            .remove("x-pam-route-template")
            .and_then(|value| value.to_str().ok().map(ToOwned::to_owned))
            .filter(|route| valid_route_template(route));
        self.metrics
            .request_duration_micros
            .fetch_add(duration.as_micros() as u64, Ordering::Relaxed);
        self.metrics.observe_request_duration(duration);
        if let Some(route) = route_template.as_deref() {
            self.metrics
                .observe_route(method, route, response.status(), duration);
        }
        self.metrics
            .response_bytes
            .fetch_add(response_bytes as u64, Ordering::Relaxed);
        if response.status().is_server_error() {
            self.metrics.errors.fetch_add(1, Ordering::Relaxed);
        }

        if let Some(exporter) = &self.state.otlp
            && let Some(start_unix_nano) = self.started_unix_nanos
            && let Some((trace_id, span_id, flags)) = traceparent_parts(traceparent)
            && flags & 1 == 1
        {
            exporter.export(crate::otlp::Span {
                trace_id: trace_id.to_owned(),
                span_id: span_id.to_owned(),
                parent_span_id: self.parent_span_id.clone(),
                trace_flags: flags,
                method: method.as_str().to_owned(),
                route: route_template,
                status: response.status().as_u16(),
                start_unix_nano,
                end_unix_nano: epoch_nanos(),
            });
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
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("pam-io")
        .enable_all()
        .build()
        .map_err(ServerError::Runtime)?;
    let local = LocalSet::new();
    runtime.block_on(local.run_until(run_async(config)))
}

fn start_php_executor(state: ServerState, mut receiver: mpsc::Receiver<PhpJob>) {
    tokio::task::spawn_local(async move {
        while let Some(job) = receiver.recv().await {
            match job {
                PhpJob::Http {
                    request,
                    mut response,
                } => {
                    let request_state = state.clone();
                    tokio::task::spawn_local(async move {
                        let Some(result) = await_http_response(
                            &mut response,
                            invoke_php_http_local(&request_state, &request),
                        )
                        .await
                        else {
                            request_state
                                .metrics
                                .client_disconnect_cancellations
                                .fetch_add(1, Ordering::Relaxed);
                            request_state.refresh_worker_metrics();
                            return;
                        };
                        request_state.refresh_php_metrics_on_owner_thread(false);
                        let _ = response.send(result);
                    });
                }
                PhpJob::WebSocket {
                    event_kind,
                    socket_id,
                    payload,
                    response,
                } => {
                    let _ = response.send(php::dispatch_ws(event_kind, &socket_id, &payload));
                }
                PhpJob::Metrics { response } => {
                    let _ = response.send(state.refresh_php_metrics_on_owner_thread(true));
                }
            }
        }
    });
}

async fn await_http_response<T>(
    response: &mut oneshot::Sender<T>,
    operation: impl Future<Output = T>,
) -> Option<T> {
    tokio::select! {
        _ = response.closed() => None,
        result = operation => Some(result),
    }
}

async fn run_async(mut config: ServerConfig) -> Result<(), ServerError> {
    if let Some(address) = std::env::var_os("PAM_INTERNAL_LISTEN_ADDRESS") {
        let address = address.to_string_lossy();
        let (host, port) = address.rsplit_once(':').ok_or_else(|| {
            ServerError::Configuration("PAM_INTERNAL_LISTEN_ADDRESS must be host:port".to_owned())
        })?;
        config.host = host.to_owned();
        config.port = port.parse().map_err(|_| {
            ServerError::Configuration("PAM_INTERNAL_LISTEN_ADDRESS has an invalid port".to_owned())
        })?;
        // TLS terminates at the public ingress. Internal pool traffic remains on
        // loopback, avoiding duplicate handshakes and certificate configuration.
        config.tls_cert = None;
        config.tls_key = None;
        config.http3 = false;
    }
    for configured in &config.response_cache_vary_headers {
        let normalized = configured.to_ascii_lowercase();
        if HeaderName::try_from(normalized.as_str()).is_err() {
            return Err(ServerError::Configuration(format!(
                "responseCacheVaryHeaders contains invalid header {configured:?}"
            )));
        }
        if matches!(
            normalized.as_str(),
            "authorization" | "cookie" | "set-cookie"
        ) {
            return Err(ServerError::Configuration(format!(
                "responseCacheVaryHeaders cannot contain sensitive header {configured:?}"
            )));
        }
    }
    match (
        config.response_cache_purge_path.as_deref(),
        config.response_cache_purge_secret.as_deref(),
    ) {
        (Some(path), Some(secret)) => {
            if !path.starts_with('/') || path.contains('?') {
                return Err(ServerError::Configuration(
                    "responseCachePurgePath must be an absolute path without a query string"
                        .to_owned(),
                ));
            }
            if path == config.metrics_path {
                return Err(ServerError::Configuration(
                    "responseCachePurgePath cannot equal metricsPath".to_owned(),
                ));
            }
            if secret.len() < 32 {
                return Err(ServerError::Configuration(
                    "responseCachePurgeSecret must contain at least 32 bytes".to_owned(),
                ));
            }
        }
        (None, None) => {}
        _ => {
            return Err(ServerError::Configuration(
                "responseCachePurgePath and responseCachePurgeSecret must be configured together"
                    .to_owned(),
            ));
        }
    }
    if HeaderName::try_from(config.response_cache_tag_header.as_str()).is_err() {
        return Err(ServerError::Configuration(
            "responseCacheTagHeader must be a valid HTTP header name".to_owned(),
        ));
    }
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
    // Bound work waiting for the single embedded-PHP owner thread. This keeps
    // overload deterministic and prevents a slow application from turning a
    // traffic spike into unbounded process memory growth.
    let (php_sender, php_receiver) = mpsc::channel(config.php_executor_queue_capacity.max(1));
    let state = ServerState::new(config, PhpExecutor { sender: php_sender })
        .map_err(ServerError::Configuration)?;
    start_php_executor(state.clone(), php_receiver);
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
    let parent_span_id = state
        .otlp
        .as_ref()
        .and_then(|_| incoming_parent_span_id(&headers));
    let telemetry = RequestTelemetry::begin(
        &state,
        0,
        None,
        state.config.telemetry_headers,
        state.config.access_log,
        state.config.access_log_sample_rate,
        parent_span_id,
    );

    let (response, response_bytes, cors_origin) = if uri.path() == state.config.metrics_path {
        let runtime_metrics = state.php.metrics().await;
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
                    Bytes::from(body),
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
    let parent_span_id = state
        .otlp
        .as_ref()
        .and_then(|_| incoming_parent_span_id(&headers));
    let telemetry = RequestTelemetry::begin(
        &state,
        0,
        advertised_http3(&state.config),
        state.config.telemetry_headers,
        state.config.access_log,
        state.config.access_log_sample_rate,
        parent_span_id,
    );
    let target = uri
        .path_and_query()
        .map_or_else(|| uri.path(), |value| value.as_str());

    if uri.path() == state.config.metrics_path {
        let runtime_metrics = state.php.metrics().await;
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
    state.synchronize_cache_invalidations(false).await;

    if state.config.response_cache_purge_path.as_deref() == Some(uri.path()) {
        let response = handle_cache_purge(&state, &method, &headers, &body).await;
        let response_bytes = response
            .body()
            .size_hint()
            .exact()
            .and_then(|bytes| usize::try_from(bytes).ok())
            .unwrap_or(0);
        return telemetry.finish(
            response,
            response_bytes,
            &request_id,
            &traceparent,
            &method,
            target,
            cors_origin.as_deref(),
        );
    }

    let cache_key = state.response_cache_key(&method, &uri, &headers);
    let _cache_guard = if let Some(key) = cache_key.as_deref() {
        if let Some((response, response_bytes)) = state.cached_response(key, false).await {
            state
                .metrics
                .response_cache_hits
                .fetch_add(1, Ordering::Relaxed);
            return telemetry.finish(
                response,
                response_bytes,
                &request_id,
                &traceparent,
                &method,
                target,
                cors_origin.as_deref(),
            );
        }
        state
            .metrics
            .response_cache_misses
            .fetch_add(1, Ordering::Relaxed);
        let lock = state.response_cache_lock(key).await;
        let (guard, collapsed) = match lock.clone().try_lock_owned() {
            Ok(guard) => (guard, false),
            Err(_) => {
                if let Some((response, response_bytes)) = state.cached_response(key, true).await {
                    state
                        .metrics
                        .response_cache_stale
                        .fetch_add(1, Ordering::Relaxed);
                    return telemetry.finish(
                        response,
                        response_bytes,
                        &request_id,
                        &traceparent,
                        &method,
                        target,
                        cors_origin.as_deref(),
                    );
                }
                (lock.lock_owned().await, true)
            }
        };
        if collapsed {
            state
                .metrics
                .response_cache_collapsed
                .fetch_add(1, Ordering::Relaxed);
            if let Some((response, response_bytes)) = state.cached_response(key, false).await {
                state
                    .metrics
                    .response_cache_hits
                    .fetch_add(1, Ordering::Relaxed);
                return telemetry.finish(
                    response,
                    response_bytes,
                    &request_id,
                    &traceparent,
                    &method,
                    target,
                    cors_origin.as_deref(),
                );
            }
        }
        Some(guard)
    } else {
        None
    };

    let (mut response, mut response_bytes) = invoke_php_http(
        &state,
        remote,
        &client,
        &method,
        target,
        version,
        &headers,
        body,
        &request_id,
        &traceparent,
    )
    .await;
    let cache_tags = extract_cache_tags(&mut response, &state.config.response_cache_tag_header);
    if let Some(cache_key) = cache_key
        && cacheable_response(&response)
    {
        let (parts, response_body) = response.into_parts();
        match to_bytes(response_body, state.config.max_response_bytes).await {
            Ok(bytes) => {
                response_bytes = bytes.len();
                let cached = CachedResponse {
                    status: parts.status,
                    headers: parts.headers,
                    body: bytes,
                    expires_at: Instant::now()
                        + Duration::from_millis(state.config.response_cache_ttl_ms),
                    stale_until: Instant::now()
                        + Duration::from_millis(
                            state.config.response_cache_ttl_ms.saturating_add(
                                state.config.response_cache_stale_while_revalidate_ms,
                            ),
                        ),
                    last_access: Arc::new(AtomicU64::new(
                        state.response_cache_clock.fetch_add(1, Ordering::Relaxed),
                    )),
                    tags: Arc::new(cache_tags),
                };
                response = cached.response();
                state.store_cached_response(cache_key, cached).await;
            }
            Err(error) => {
                response = error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("cannot buffer cacheable response: {error}"),
                );
                response_bytes = 0;
            }
        }
    }
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

async fn handle_cache_purge(
    state: &ServerState,
    method: &Method,
    headers: &HeaderMap,
    body: &[u8],
) -> Response {
    if method != Method::POST {
        return error_response(StatusCode::METHOD_NOT_ALLOWED, "cache purge requires POST");
    }
    let Some(secret) = state.config.response_cache_purge_secret.as_deref() else {
        return error_response(StatusCode::NOT_FOUND, "cache purge is disabled");
    };
    let supplied = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if !supplied.is_some_and(|token| constant_time_token_eq(secret, token)) {
        return error_response(StatusCode::UNAUTHORIZED, "invalid cache purge credentials");
    }
    let payload: Value = match serde_json::from_slice(body) {
        Ok(payload) => payload,
        Err(_) => return error_response(StatusCode::BAD_REQUEST, "invalid purge JSON payload"),
    };
    let all = payload.get("all").and_then(Value::as_bool) == Some(true);
    let tag = payload.get("tag").and_then(Value::as_str);
    if all == tag.is_some() || tag.is_some_and(|tag| !valid_cache_tag(tag)) {
        return error_response(
            StatusCode::BAD_REQUEST,
            "provide either all=true or one valid cache tag",
        );
    }
    if let Err(error) = state.publish_cache_invalidation(tag).await {
        return internal_error(error);
    }
    let purged = state.purge_cached_responses(tag).await;
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json; charset=utf-8")
        .header("cache-control", "no-store")
        .body(Body::from(
            serde_json::json!({"purged": purged}).to_string(),
        ))
        .expect("cache purge response must be valid")
}

fn constant_time_token_eq(expected: &str, supplied: &str) -> bool {
    let expected = Sha256::digest(expected.as_bytes());
    let supplied = Sha256::digest(supplied.as_bytes());
    expected
        .iter()
        .zip(supplied.iter())
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn valid_cache_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= 64
        && tag.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        })
}

fn valid_route_template(route: &str) -> bool {
    route.starts_with('/')
        && route.len() <= 256
        && route
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && !matches!(byte, b'\\' | b'"'))
}

fn extract_cache_tags(response: &mut Response, configured_header: &str) -> HashSet<String> {
    let Ok(header) = HeaderName::try_from(configured_header) else {
        return HashSet::new();
    };
    let tags = response
        .headers()
        .get_all(&header)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split([',', ' ', '\t']))
        .filter(|tag| valid_cache_tag(tag))
        .take(32)
        .map(ToOwned::to_owned)
        .collect();
    response.headers_mut().remove(header);
    tags
}

fn cacheable_response(response: &Response) -> bool {
    response.status() == StatusCode::OK
        && !response.headers().contains_key("set-cookie")
        && !response
            .headers()
            .get("cache-control")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value.split(',').any(|directive| {
                    matches!(
                        directive.trim().to_ascii_lowercase().as_str(),
                        "private" | "no-store"
                    )
                })
            })
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
    body: Bytes,
    request_id: &str,
    traceparent: &str,
) -> (Response, usize) {
    state
        .php
        .http(OwnedHttpRequest {
            remote,
            client: client.to_owned(),
            method: method.clone(),
            target: target.to_owned(),
            version,
            headers: headers.clone(),
            body,
            request_id: request_id.to_owned(),
            traceparent: traceparent.to_owned(),
        })
        .await
}

async fn invoke_php_http_local(
    state: &ServerState,
    request: &OwnedHttpRequest,
) -> (Response, usize) {
    let remote = request.remote;
    let client = request.client.as_str();
    let method = &request.method;
    let target = request.target.as_str();
    let version = request.version;
    let headers = &request.headers;
    let body = request.body.as_ref();
    let request_id = request.request_id.as_str();
    let traceparent = request.traceparent.as_str();
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
                let Some(response) = envelope.response.take() else {
                    return (
                        internal_error("PHP dispatch completed without a response"),
                        0,
                    );
                };
                return match response.total_bytes() {
                    Some(bytes) if bytes <= state.config.max_response_bytes => {
                        (response.into_http_response(), bytes)
                    }
                    _ => (response_limit_error(state.config.max_response_bytes), 0),
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
                        let head = head.into_php();
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
                        if !head.body.is_empty() && sender.try_send(Ok(head.body.clone())).is_err()
                        {
                            return (internal_error("streaming response queue is unavailable"), 0);
                        }
                        for buffered in &head.chunks {
                            if sender.try_send(Ok(buffered.clone())).is_err() {
                                return (internal_error("streaming response queue is full"), 0);
                            }
                        }
                        if sender.try_send(Ok(Bytes::from(chunk))).is_err() {
                            return (internal_error("streaming response queue is full"), 0);
                        }
                        let request_id = request_id.to_owned();
                        let request_body = body.to_vec();
                        tokio::task::spawn_local(
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
                    if let Some(response) = envelope.response.take() {
                        if !response.body.is_empty()
                            && !send_response_chunk(
                                &sender,
                                response.body,
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
                                chunk,
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
    let json = match state
        .php
        .websocket(event_kind as i32, socket_id, payload)
        .await
    {
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

fn service_unavailable(message: impl Into<String>) -> Response {
    let message = message.into();
    eprintln!(
        "{}",
        serde_json::json!({
            "level": "warning",
            "event": "runtime_overloaded",
            "message": message,
        })
    );
    Response::builder()
        .status(StatusCode::SERVICE_UNAVAILABLE)
        .header("retry-after", "1")
        .body(Body::from("Service Unavailable"))
        .expect("static overload response must be valid")
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
                .map(Ok::<Bytes, Infallible>);
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
    if value.len() % 2 != 0 {
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
        let mut parts = value.split('-');
        let version = parts.next().expect("validated trace version");
        let trace_id = parts.next().expect("validated trace ID");
        let _parent_id = parts.next().expect("validated parent ID");
        let flags = parts.next().expect("validated trace flags");
        return format!(
            "{version}-{trace_id}-{}-{flags}",
            server_span_identifier(state)
        );
    }

    let sequence = state.next_request_id.fetch_add(1, Ordering::Relaxed) as u128;
    let timestamp = epoch_millis() as u128;
    let pid = std::process::id() as u128;
    let trace_id = (timestamp << 64) ^ (pid << 32) ^ sequence;
    let span_id = ((timestamp as u64).rotate_left(17)) ^ sequence as u64;
    format!("00-{trace_id:032x}-{span_id:016x}-01")
}

fn server_span_identifier(state: &ServerState) -> String {
    let sequence = state.next_request_id.fetch_add(1, Ordering::Relaxed);
    let timestamp = epoch_millis();
    let pid = u64::from(std::process::id());
    let mut span_id = timestamp.rotate_left(17) ^ sequence.rotate_left(7) ^ (pid << 32);
    if span_id == 0 {
        span_id = 1;
    }
    format!("{span_id:016x}")
}

fn request_traceparent(headers: &HeaderMap, state: &ServerState) -> String {
    if state.config.telemetry_headers || state.otlp.is_some() || headers.contains_key("traceparent")
    {
        traceparent(headers, state)
    } else {
        String::new()
    }
}

fn incoming_parent_span_id(headers: &HeaderMap) -> Option<String> {
    let value = headers.get("traceparent")?.to_str().ok()?;
    valid_traceparent(value).then(|| {
        value
            .split('-')
            .nth(2)
            .expect("validated parent span ID")
            .to_owned()
    })
}

fn traceparent_parts(value: &str) -> Option<(&str, &str, u8)> {
    if !valid_traceparent(value) {
        return None;
    }
    let mut parts = value.split('-');
    let _version = parts.next()?;
    let trace_id = parts.next()?;
    let span_id = parts.next()?;
    let flags = u8::from_str_radix(parts.next()?, 16).ok()?;
    Some((trace_id, span_id, flags))
}

fn epoch_nanos() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
        })
}

fn valid_traceparent(value: &str) -> bool {
    let parts = value.split('-').collect::<Vec<_>>();
    parts.len() == 4
        && parts[0] == "00"
        && parts[1].len() == 32
        && parts[1] != "00000000000000000000000000000000"
        && parts[2].len() == 16
        && parts[2] != "0000000000000000"
        && parts[3].len() == 2
        && parts.iter().all(|part| {
            part.bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
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
            let lag_micros = now.saturating_duration_since(expected).as_micros() as u64;
            metrics
                .event_loop_lag_micros
                .store(lag_micros, Ordering::Relaxed);
            metrics
                .event_loop_lag_max_micros
                .fetch_max(lag_micros, Ordering::Relaxed);
            let _ = metrics.event_loop_lag_total_micros.fetch_update(
                Ordering::Relaxed,
                Ordering::Relaxed,
                |total| Some(total.saturating_add(lag_micros)),
            );
            let _ = metrics.event_loop_lag_samples.fetch_update(
                Ordering::Relaxed,
                Ordering::Relaxed,
                |samples| Some(samples.saturating_add(1)),
            );
            expected = now + period;
        }
    });
}

fn event_loop_lag_average_micros(total_micros: u64, samples: u64) -> f64 {
    if samples == 0 {
        0.0
    } else {
        total_micros as f64 / samples as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::task::{Context, Poll};

    struct PendingOperation(Arc<AtomicBool>);

    impl Future for PendingOperation {
        type Output = ();

        fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
            Poll::Pending
        }
    }

    impl Drop for PendingOperation {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    #[test]
    fn dropping_the_http_receiver_cancels_the_inflight_operation() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let dropped = Arc::new(AtomicBool::new(false));
        runtime.block_on(async {
            let (mut response, receiver) = oneshot::channel();
            let disconnect = tokio::spawn(async move {
                tokio::task::yield_now().await;
                drop(receiver);
            });
            let result =
                await_http_response(&mut response, PendingOperation(dropped.clone())).await;
            disconnect.await.unwrap();
            assert!(result.is_none());
        });
        assert!(dropped.load(Ordering::Relaxed));
    }
}
