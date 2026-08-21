use std::fs;
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderName, HeaderValue, Request, Response, StatusCode, Uri, Version};
use http_body_util::BodyExt;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::{TokioExecutor, TokioIo};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cluster::{MasterState, linux_process_start};

const MAX_CONFIG_BYTES: u64 = 64 * 1024;
const MAX_TLS_FILE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrafficConfig {
    pub schema_version: u8,
    pub generation: u64,
    pub name: String,
    pub listen: SocketAddr,
    pub stable: SocketAddr,
    pub candidate: Option<SocketAddr>,
    pub candidate_weight_basis_points: u16,
    #[serde(default = "default_rollout_phase")]
    pub rollout_phase_code: u8,
    #[serde(default)]
    pub rollout_deadline_millis: Option<u64>,
    #[serde(default)]
    pub last_rollout_decision_code: Option<u8>,
    #[serde(default)]
    pub last_evaluated_at_millis: Option<u64>,
    #[serde(default)]
    pub last_evaluated_candidate_requests: Option<u64>,
    #[serde(default)]
    pub last_evaluated_candidate_errors: Option<u64>,
    #[serde(default)]
    pub tls_certificate: Option<PathBuf>,
    #[serde(default)]
    pub tls_private_key: Option<PathBuf>,
}

#[derive(Clone)]
struct TrafficState {
    config: Arc<RwLock<TrafficConfig>>,
    client: Client<HttpConnector, Body>,
    metrics: Arc<TrafficCounters>,
}

#[derive(Default)]
struct TrafficCounters {
    generation: AtomicU64,
    stable_requests: AtomicU64,
    stable_errors: AtomicU64,
    stable_latency_micros: AtomicU64,
    candidate_requests: AtomicU64,
    candidate_errors: AtomicU64,
    candidate_latency_micros: AtomicU64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficMetrics {
    pub schema_version: u8,
    #[serde(default)]
    pub generation: u64,
    pub stable_requests: u64,
    pub stable_errors: u64,
    pub stable_latency_micros: u64,
    pub candidate_requests: u64,
    pub candidate_errors: u64,
    pub candidate_latency_micros: u64,
}

pub fn run(config_path: PathBuf, state_path: PathBuf, metrics_path: PathBuf) -> Result<u8, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("cannot create traffic runtime: {error}"))?;
    runtime.block_on(serve(config_path, state_path, metrics_path))
}

async fn serve(
    config_path: PathBuf,
    state_path: PathBuf,
    metrics_path: PathBuf,
) -> Result<u8, String> {
    let config = read_config(&config_path)?;
    validate_config(&config)?;
    let listener = std::net::TcpListener::bind(config.listen)
        .map_err(|error| format!("cannot bind PAM traffic ingress {}: {error}", config.listen))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("cannot configure PAM traffic listener: {error}"))?;
    let shared = Arc::new(RwLock::new(config.clone()));
    let metrics = Arc::new(TrafficCounters::new(config.generation));
    let watcher = Arc::clone(&shared);
    let watched_metrics = Arc::clone(&metrics);
    let watched_path = config_path.clone();
    let watch_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(200));
        loop {
            interval.tick().await;
            if let Ok(candidate) = read_config(&watched_path)
                && validate_config(&candidate).is_ok()
            {
                let mut current = watcher
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if candidate.generation > current.generation {
                    watched_metrics.reset(candidate.generation);
                    *current = candidate;
                }
            }
        }
    });
    let metric_counters = Arc::clone(&metrics);
    let metric_path = metrics_path.clone();
    let metric_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        loop {
            interval.tick().await;
            let _ = write_metrics(&metric_path, &metric_counters.snapshot());
        }
    });
    let state = TrafficState {
        config: shared,
        client: Client::builder(TokioExecutor::new()).build(HttpConnector::new()),
        metrics: Arc::clone(&metrics),
    };
    let ready = ReadyState::publish(&state_path)?;
    let app = Router::new().fallback(proxy).with_state(state);
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|error| format!("cannot install traffic stop signal: {error}"))?;
    let result = if let (Some(certificate), Some(private_key)) =
        (&config.tls_certificate, &config.tls_private_key)
    {
        let tls = axum_server::tls_rustls::RustlsConfig::from_pem_file(certificate, private_key)
            .await
            .map_err(|error| format!("cannot load traffic TLS identity: {error}"))?;
        let handle = axum_server::Handle::new();
        let shutdown = handle.clone();
        tokio::spawn(async move {
            terminate.recv().await;
            shutdown.graceful_shutdown(Some(Duration::from_secs(30)));
        });
        axum_server::from_tcp_rustls(listener, tls)
            .map_err(|error| format!("cannot configure traffic TLS listener: {error}"))?
            .handle(handle)
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .map_err(|error| format!("traffic TLS ingress failed: {error}"))
    } else {
        let listener = tokio::net::TcpListener::from_std(listener)
            .map_err(|error| format!("cannot configure PAM traffic listener: {error}"))?;
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            terminate.recv().await;
        })
        .await
        .map_err(|error| format!("traffic ingress failed: {error}"))
    };
    watch_task.abort();
    metric_task.abort();
    let _ = write_metrics(&metrics_path, &metrics.snapshot());
    drop(ready);
    result.map(|()| 0)
}

async fn proxy(
    State(state): State<TrafficState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    mut request: Request<Body>,
) -> Response<Body> {
    let config = state
        .config
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let candidate = config
        .candidate
        .filter(|_| traffic_bucket(&request, peer) < config.candidate_weight_basis_points);
    let (target, release) =
        candidate.map_or((config.stable, "stable"), |address| (address, "candidate"));
    let started = Instant::now();
    let path_and_query = request
        .uri()
        .path_and_query()
        .map_or("/", |value| value.as_str());
    let Ok(uri) = format!("http://{target}{path_and_query}").parse::<Uri>() else {
        return unavailable("invalid PAM release target");
    };
    *request.uri_mut() = uri;
    *request.version_mut() = Version::HTTP_11;
    request.headers_mut().insert(
        HeaderName::from_static("x-forwarded-for"),
        HeaderValue::from_str(&peer.ip().to_string())
            .unwrap_or(HeaderValue::from_static("unknown")),
    );
    request.headers_mut().insert(
        HeaderName::from_static("x-forwarded-proto"),
        if config.tls_certificate.is_some() {
            HeaderValue::from_static("https")
        } else {
            HeaderValue::from_static("http")
        },
    );
    request.headers_mut().remove("x-pam-release");
    let client_upgrade = hyper::upgrade::on(&mut request);
    match state.client.request(request).await {
        Ok(mut response) => {
            state.metrics.observe(
                config.generation,
                candidate.is_some(),
                response.status().is_server_error(),
                started.elapsed(),
            );
            response.headers_mut().insert(
                HeaderName::from_static("x-pam-release"),
                HeaderValue::from_static(release),
            );
            if response.status() == StatusCode::SWITCHING_PROTOCOLS {
                let backend_upgrade = hyper::upgrade::on(&mut response);
                tokio::spawn(async move {
                    let (Ok(client), Ok(backend)) = (client_upgrade.await, backend_upgrade.await)
                    else {
                        return;
                    };
                    let (mut client, mut backend) = (TokioIo::new(client), TokioIo::new(backend));
                    let _ = tokio::io::copy_bidirectional(&mut client, &mut backend).await;
                });
            }
            response.map(|body| Body::new(body.map_err(std::io::Error::other)))
        }
        Err(_) => {
            state.metrics.observe(
                config.generation,
                candidate.is_some(),
                true,
                started.elapsed(),
            );
            unavailable("PAM release unavailable")
        }
    }
}

impl TrafficCounters {
    fn new(generation: u64) -> Self {
        Self {
            generation: AtomicU64::new(generation),
            ..Self::default()
        }
    }

    fn reset(&self, generation: u64) {
        self.stable_requests.store(0, Ordering::Relaxed);
        self.stable_errors.store(0, Ordering::Relaxed);
        self.stable_latency_micros.store(0, Ordering::Relaxed);
        self.candidate_requests.store(0, Ordering::Relaxed);
        self.candidate_errors.store(0, Ordering::Relaxed);
        self.candidate_latency_micros.store(0, Ordering::Relaxed);
        self.generation.store(generation, Ordering::Release);
    }

    fn observe(&self, generation: u64, candidate: bool, error: bool, latency: Duration) {
        if self.generation.load(Ordering::Acquire) != generation {
            return;
        }
        let micros = latency.as_micros().try_into().unwrap_or(u64::MAX);
        let (requests, errors, total) = if candidate {
            (
                &self.candidate_requests,
                &self.candidate_errors,
                &self.candidate_latency_micros,
            )
        } else {
            (
                &self.stable_requests,
                &self.stable_errors,
                &self.stable_latency_micros,
            )
        };
        requests.fetch_add(1, Ordering::Relaxed);
        if error {
            errors.fetch_add(1, Ordering::Relaxed);
        }
        total.fetch_add(micros, Ordering::Relaxed);
    }

    fn snapshot(&self) -> TrafficMetrics {
        TrafficMetrics {
            schema_version: 1,
            generation: self.generation.load(Ordering::Acquire),
            stable_requests: self.stable_requests.load(Ordering::Relaxed),
            stable_errors: self.stable_errors.load(Ordering::Relaxed),
            stable_latency_micros: self.stable_latency_micros.load(Ordering::Relaxed),
            candidate_requests: self.candidate_requests.load(Ordering::Relaxed),
            candidate_errors: self.candidate_errors.load(Ordering::Relaxed),
            candidate_latency_micros: self.candidate_latency_micros.load(Ordering::Relaxed),
        }
    }
}

pub fn read_metrics(path: &Path) -> Result<TrafficMetrics, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_CONFIG_BYTES {
        return Err("invalid traffic metrics file".to_owned());
    }
    serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}

fn write_metrics(path: &Path, metrics: &TrafficMetrics) -> Result<(), String> {
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(
        &temporary,
        serde_json::to_vec(metrics).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
        .map_err(|error| error.to_string())?;
    fs::rename(temporary, path).map_err(|error| error.to_string())
}

fn traffic_bucket(request: &Request<Body>, peer: SocketAddr) -> u16 {
    let mut digest = Sha256::new();
    if let Some(cookie) = request
        .headers()
        .get("cookie")
        .and_then(|value| value.to_str().ok())
    {
        if let Some(affinity) = cookie
            .split(';')
            .map(str::trim)
            .find_map(|value| value.strip_prefix("pam_affinity="))
        {
            digest.update(affinity.as_bytes());
        } else {
            digest.update(peer.ip().to_string().as_bytes());
        }
    } else {
        digest.update(peer.ip().to_string().as_bytes());
    }
    digest.update(request.uri().path().as_bytes());
    let bytes = digest.finalize();
    u16::from_be_bytes([bytes[0], bytes[1]]) % 10_000
}

pub fn read_config(path: &Path) -> Result<TrafficConfig, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot read traffic config {}: {error}", path.display()))?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_CONFIG_BYTES {
        return Err(format!("invalid traffic config {}", path.display()));
    }
    serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
        .map_err(|error| format!("invalid traffic config: {error}"))
}

pub fn validate_config(config: &TrafficConfig) -> Result<(), String> {
    if config.schema_version != 1 || config.generation == 0 {
        return Err("unsupported traffic config contract".to_owned());
    }
    if config.candidate_weight_basis_points > 10_000 {
        return Err("candidate weight must be 0-10000 basis points".to_owned());
    }
    if !(1..=4).contains(&config.rollout_phase_code)
        || config
            .last_rollout_decision_code
            .is_some_and(|value| !(1..=4).contains(&value))
    {
        return Err("unsupported traffic rollout state".to_owned());
    }
    if config.candidate.is_none() && config.candidate_weight_basis_points != 0 {
        return Err("candidate weight requires a candidate upstream".to_owned());
    }
    if config.listen == config.stable || config.candidate == Some(config.listen) {
        return Err("traffic ingress cannot proxy to its own listener".to_owned());
    }
    match (&config.tls_certificate, &config.tls_private_key) {
        (Some(certificate), Some(private_key)) => {
            validate_tls_file(certificate, "certificate")?;
            validate_tls_file(private_key, "private key")?;
            if certificate == private_key {
                return Err(
                    "traffic TLS certificate and private key must be different files".to_owned(),
                );
            }
        }
        (None, None) => {}
        _ => return Err("traffic TLS requires both certificate and private key".to_owned()),
    }
    if !config.listen.ip().is_loopback()
        && (config.stable.ip().is_unspecified()
            || config
                .candidate
                .is_some_and(|value| value.ip().is_unspecified()))
    {
        return Err("public traffic ingress requires explicit upstream addresses".to_owned());
    }
    Ok(())
}

const fn default_rollout_phase() -> u8 {
    1
}

fn validate_tls_file(path: &Path, label: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "cannot inspect traffic TLS {label} {}: {error}",
            path.display()
        )
    })?;
    if !metadata.file_type().is_file() || metadata.len() == 0 || metadata.len() > MAX_TLS_FILE_BYTES
    {
        return Err(format!(
            "traffic TLS {label} must be a non-empty regular file up to 4 MiB"
        ));
    }
    Ok(())
}

struct ReadyState(PathBuf);

impl ReadyState {
    fn publish(path: &Path) -> Result<Self, String> {
        let state = MasterState {
            version: 1,
            pid: std::process::id(),
            process_start: linux_process_start(std::process::id()),
            workers: 1,
            admin_address: None,
            started_at_millis: crate::worker_state::epoch_millis(),
        };
        let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
        fs::write(
            &temporary,
            serde_json::to_vec_pretty(&state).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
        fs::rename(&temporary, path).map_err(|error| error.to_string())?;
        Ok(Self(path.to_path_buf()))
    }
}

impl Drop for ReadyState {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn unavailable(message: &'static str) -> Response<Body> {
    Response::builder()
        .status(StatusCode::SERVICE_UNAVAILABLE)
        .header("content-type", "application/json")
        .body(Body::from(format!(r#"{{"error":"{message}"}}"#)))
        .expect("static traffic response is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traffic_contract_rejects_weight_without_candidate() {
        let config = TrafficConfig {
            schema_version: 1,
            generation: 1,
            name: "edge".to_owned(),
            listen: "127.0.0.1:8080".parse().unwrap(),
            stable: "127.0.0.1:8081".parse().unwrap(),
            candidate: None,
            candidate_weight_basis_points: 1,
            rollout_phase_code: 2,
            rollout_deadline_millis: None,
            last_rollout_decision_code: None,
            last_evaluated_at_millis: None,
            last_evaluated_candidate_requests: None,
            last_evaluated_candidate_errors: None,
            tls_certificate: None,
            tls_private_key: None,
        };
        assert!(validate_config(&config).is_err());
    }
}
