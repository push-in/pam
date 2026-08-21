use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::worker_state::{WorkerLifecycle, WorkerRuntimeRecord, epoch_millis};

pub const CONTROL_PLANE_DIAGNOSTICS_SCHEMA_VERSION: u8 = 1;
pub const CONTROL_PLANE_DIAGNOSTICS_SURFACE_CODE: u8 = 1;
pub const CONTROL_PLANE_HEALTH_SCHEMA_VERSION: u8 = 1;
pub const CONTROL_PLANE_HEALTH_SURFACE_CODE: u8 = 1;
pub const MAX_DIAGNOSTIC_WORKERS: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum DiagnosticsResultCode {
    Available = 1,
    Unavailable = 2,
}

impl TryFrom<u8> for DiagnosticsResultCode {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Available),
            2 => Ok(Self::Unavailable),
            _ => Err(format!("unknown diagnostics result code {value}")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum WorkerResultCode {
    Operational = 1,
    NeedsAttention = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum HealthResultCode {
    Healthy = 1,
    Unhealthy = 2,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ControlPlaneHealth {
    pub schema_version: u8,
    pub surface_code: u8,
    pub result_code: u8,
    pub healthy: bool,
    pub generation: u64,
    pub desired_workers: usize,
    pub ready_workers: usize,
    pub workers: Vec<HealthWorker>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HealthWorker {
    pub id: usize,
    pub generation: u64,
    pub pid: u32,
    pub pool: Option<String>,
    pub state: u8,
}

impl TryFrom<u8> for WorkerResultCode {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Operational),
            2 => Ok(Self::NeedsAttention),
            _ => Err(format!("unknown worker result code {value}")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ControlPlaneDiagnostics {
    pub schema_version: u8,
    pub surface_code: u8,
    pub result_code: u8,
    pub generation: u64,
    pub desired_workers: usize,
    pub ready_workers: usize,
    pub workers: Vec<WorkerDiagnostics>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerDiagnostics {
    pub worker_id: usize,
    pub generation: u64,
    pub pid: u32,
    pub pool: String,
    pub lifecycle_code: u8,
    pub result_code: u8,
    pub current_lag_micros: u64,
    pub max_lag_micros: u64,
    pub average_lag_micros: u64,
    pub lag_sample_count: u64,
}

impl ControlPlaneDiagnostics {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != CONTROL_PLANE_DIAGNOSTICS_SCHEMA_VERSION {
            return Err(format!(
                "unsupported control-plane diagnostics schema version {}",
                self.schema_version
            ));
        }
        if self.surface_code != CONTROL_PLANE_DIAGNOSTICS_SURFACE_CODE {
            return Err(format!(
                "unsupported control-plane diagnostics surface code {}",
                self.surface_code
            ));
        }
        DiagnosticsResultCode::try_from(self.result_code)?;
        if self.desired_workers > MAX_DIAGNOSTIC_WORKERS
            || self.workers.len() > MAX_DIAGNOSTIC_WORKERS
        {
            return Err(format!(
                "control-plane diagnostics exceed the {MAX_DIAGNOSTIC_WORKERS}-worker limit"
            ));
        }
        if self.ready_workers > self.workers.len() {
            return Err("ready worker count exceeds reported workers".to_owned());
        }
        let mut identities = HashSet::with_capacity(self.workers.len());
        for worker in &self.workers {
            WorkerLifecycle::try_from(worker.lifecycle_code)?;
            WorkerResultCode::try_from(worker.result_code)?;
            if worker.pid == 0 {
                return Err("worker PID must be positive".to_owned());
            }
            if worker.pool.is_empty() || worker.pool.len() > 128 {
                return Err("worker pool names must contain between 1 and 128 bytes".to_owned());
            }
            if worker.lag_sample_count == 0 && worker.average_lag_micros != 0 {
                return Err("worker lag average requires at least one sample".to_owned());
            }
            if !identities.insert((worker.worker_id, worker.generation, worker.pid)) {
                return Err(
                    "control-plane diagnostics contain a duplicate worker identity".to_owned(),
                );
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct ClusterSnapshot {
    pub live: bool,
    pub startup_complete: bool,
    pub desired_workers: usize,
    pub generation: u64,
    pub workers: Vec<WorkerRuntimeRecord>,
}

impl ClusterSnapshot {
    fn ready(&self) -> bool {
        self.live
            && self.startup_complete
            && self.workers.len() == self.desired_workers
            && self.workers.iter().all(|worker| {
                matches!(
                    worker.lifecycle(),
                    Ok(WorkerLifecycle::Ready | WorkerLifecycle::Busy)
                ) && !worker.deadline_exceeded(epoch_millis())
            })
    }

    fn health(&self, healthy: bool) -> Result<ControlPlaneHealth, String> {
        if self.desired_workers > MAX_DIAGNOSTIC_WORKERS
            || self.workers.len() > MAX_DIAGNOSTIC_WORKERS
        {
            return Err(format!(
                "control-plane health exceeds the {MAX_DIAGNOSTIC_WORKERS}-worker limit"
            ));
        }
        let now = epoch_millis();
        let ready_workers = self
            .workers
            .iter()
            .filter(|worker| {
                matches!(
                    worker.lifecycle(),
                    Ok(WorkerLifecycle::Ready | WorkerLifecycle::Busy)
                ) && !worker.deadline_exceeded(now)
            })
            .count();
        let workers = self
            .workers
            .iter()
            .map(|worker| {
                let lifecycle = worker.lifecycle()?;
                if worker.pid == 0 {
                    return Err("worker PID must be positive".to_owned());
                }
                if worker.pool.as_ref().is_some_and(|pool| pool.len() > 128) {
                    return Err("worker pool names cannot exceed 128 bytes".to_owned());
                }
                Ok(HealthWorker {
                    id: worker.worker_id,
                    generation: worker.generation,
                    pid: worker.pid,
                    pool: worker.pool.clone(),
                    state: lifecycle as u8,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(ControlPlaneHealth {
            schema_version: CONTROL_PLANE_HEALTH_SCHEMA_VERSION,
            surface_code: CONTROL_PLANE_HEALTH_SURFACE_CODE,
            result_code: if healthy {
                HealthResultCode::Healthy
            } else {
                HealthResultCode::Unhealthy
            } as u8,
            healthy,
            generation: self.generation,
            desired_workers: self.desired_workers,
            ready_workers,
            workers,
        })
    }

    pub fn diagnostics(&self) -> Result<ControlPlaneDiagnostics, String> {
        if self.workers.len() > MAX_DIAGNOSTIC_WORKERS {
            return Err(format!(
                "control-plane diagnostics exceed the {MAX_DIAGNOSTIC_WORKERS}-worker limit"
            ));
        }
        let now = epoch_millis();
        let ready_workers = self
            .workers
            .iter()
            .filter(|worker| {
                matches!(
                    worker.lifecycle(),
                    Ok(WorkerLifecycle::Ready | WorkerLifecycle::Busy)
                ) && !worker.deadline_exceeded(now)
            })
            .count();
        let workers = self
            .workers
            .iter()
            .map(|worker| {
                let lifecycle = worker.lifecycle()?;
                let operational =
                    matches!(lifecycle, WorkerLifecycle::Ready | WorkerLifecycle::Busy)
                        && !worker.deadline_exceeded(now);
                Ok(WorkerDiagnostics {
                    worker_id: worker.worker_id,
                    generation: worker.generation,
                    pid: worker.pid,
                    pool: worker.pool.clone().unwrap_or_else(|| "default".to_owned()),
                    lifecycle_code: lifecycle as u8,
                    result_code: if operational {
                        WorkerResultCode::Operational
                    } else {
                        WorkerResultCode::NeedsAttention
                    } as u8,
                    current_lag_micros: worker.metrics.event_loop_lag_micros,
                    max_lag_micros: worker.metrics.event_loop_lag_max_micros,
                    average_lag_micros: event_loop_lag_average_micros_integer(
                        worker.metrics.event_loop_lag_total_micros,
                        worker.metrics.event_loop_lag_samples,
                    ),
                    lag_sample_count: worker.metrics.event_loop_lag_samples,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let diagnostics = ControlPlaneDiagnostics {
            schema_version: CONTROL_PLANE_DIAGNOSTICS_SCHEMA_VERSION,
            surface_code: CONTROL_PLANE_DIAGNOSTICS_SURFACE_CODE,
            result_code: DiagnosticsResultCode::Available as u8,
            generation: self.generation,
            desired_workers: self.desired_workers,
            ready_workers,
            workers,
        };
        diagnostics.validate()?;
        Ok(diagnostics)
    }

    fn metrics(&self) -> String {
        let mut output = String::from(concat!(
            "# TYPE pam_cluster_ready gauge\n",
            "# TYPE pam_cluster_workers gauge\n",
            "# TYPE pam_cluster_worker_info gauge\n",
            "# TYPE pam_http_requests_total counter\n",
            "# TYPE pam_http_errors_total counter\n",
            "# TYPE pam_http_client_disconnect_cancellations_total counter\n",
            "# TYPE pam_http_active_requests gauge\n",
            "# TYPE pam_http_request_duration_seconds histogram\n",
            "# TYPE pam_http_request_bytes_total counter\n",
            "# TYPE pam_http_response_bytes_total counter\n",
            "# TYPE pam_http_response_cache_hits_total counter\n",
            "# TYPE pam_http_response_cache_misses_total counter\n",
            "# TYPE pam_http_response_cache_collapsed_total counter\n",
            "# TYPE pam_http_response_cache_stale_total counter\n",
            "# TYPE pam_http_response_cache_purges_total counter\n",
            "# TYPE pam_websocket_connections gauge\n",
            "# TYPE pam_websocket_messages_total counter\n",
            "# TYPE pam_websocket_backpressure_total counter\n",
            "# TYPE pam_event_loop_lag_seconds gauge\n",
            "# TYPE pam_event_loop_lag_max_seconds gauge\n",
            "# TYPE pam_event_loop_lag_average_seconds gauge\n",
            "# TYPE pam_worker_event_loop_lag_seconds gauge\n",
            "# TYPE pam_worker_event_loop_lag_max_seconds gauge\n",
            "# TYPE pam_worker_event_loop_lag_average_seconds gauge\n",
            "# TYPE pam_process_resident_memory_bytes gauge\n",
            "# TYPE pam_php_memory_bytes gauge\n",
            "# TYPE pam_php_peak_memory_bytes gauge\n",
            "# TYPE pam_php_fibers gauge\n",
            "# TYPE pam_pool_workers gauge\n",
            "# TYPE pam_pool_http_requests_total counter\n",
            "# TYPE pam_pool_http_errors_total counter\n",
            "# TYPE pam_pool_http_active_requests gauge\n",
            "# TYPE pam_pool_event_loop_lag_seconds gauge\n",
            "# TYPE pam_pool_event_loop_lag_max_seconds gauge\n",
            "# TYPE pam_pool_event_loop_lag_average_seconds gauge\n",
            "# TYPE pam_pool_process_resident_memory_bytes gauge\n",
            "# TYPE pam_pool_php_memory_bytes gauge\n",
        ));
        let sum = |value: fn(&WorkerRuntimeRecord) -> u64| {
            self.workers
                .iter()
                .map(value)
                .fold(0_u64, u64::saturating_add)
        };
        let event_loop_total = sum(|worker| worker.metrics.event_loop_lag_total_micros);
        let event_loop_samples = sum(|worker| worker.metrics.event_loop_lag_samples);
        output.push_str(&format!(
            concat!(
                "pam_cluster_ready {}\n",
                "pam_cluster_workers {}\n",
                "pam_http_requests_total {}\n",
                "pam_http_errors_total {}\n",
                "pam_http_client_disconnect_cancellations_total {}\n",
                "pam_http_active_requests {}\n",
                "pam_http_request_bytes_total {}\n",
                "pam_http_response_bytes_total {}\n",
                "pam_http_response_cache_hits_total {}\n",
                "pam_http_response_cache_misses_total {}\n",
                "pam_http_response_cache_collapsed_total {}\n",
                "pam_http_response_cache_stale_total {}\n",
                "pam_http_response_cache_purges_total {}\n",
                "pam_websocket_connections {}\n",
                "pam_websocket_messages_total {}\n",
                "pam_websocket_backpressure_total {}\n",
                "pam_event_loop_lag_seconds {:.6}\n",
                "pam_event_loop_lag_max_seconds {:.6}\n",
                "pam_event_loop_lag_average_seconds {:.6}\n",
                "pam_process_resident_memory_bytes {}\n",
                "pam_php_memory_bytes {}\n",
                "pam_php_peak_memory_bytes {}\n",
                "pam_php_fibers {}\n",
            ),
            u8::from(self.ready()),
            self.workers.len(),
            sum(|worker| worker.metrics.requests),
            sum(|worker| worker.metrics.errors),
            sum(|worker| worker.metrics.client_disconnect_cancellations),
            sum(|worker| worker.metrics.active_requests),
            sum(|worker| worker.metrics.request_bytes),
            sum(|worker| worker.metrics.response_bytes),
            sum(|worker| worker.metrics.response_cache_hits),
            sum(|worker| worker.metrics.response_cache_misses),
            sum(|worker| worker.metrics.response_cache_collapsed),
            sum(|worker| worker.metrics.response_cache_stale),
            sum(|worker| worker.metrics.response_cache_purges),
            sum(|worker| worker.metrics.websocket_connections),
            sum(|worker| worker.metrics.websocket_messages),
            sum(|worker| worker.metrics.websocket_backpressure),
            self.workers
                .iter()
                .map(|worker| worker.metrics.event_loop_lag_micros)
                .max()
                .unwrap_or(0) as f64
                / 1_000_000.0,
            self.workers
                .iter()
                .map(|worker| worker.metrics.event_loop_lag_max_micros)
                .max()
                .unwrap_or(0) as f64
                / 1_000_000.0,
            event_loop_lag_average_micros(event_loop_total, event_loop_samples) / 1_000_000.0,
            sum(|worker| worker.metrics.resident_memory_bytes),
            sum(|worker| worker.metrics.php_memory_bytes),
            sum(|worker| worker.metrics.php_peak_memory_bytes),
            sum(|worker| worker.metrics.php_fibers),
        ));
        let labels = [
            "0.0001", "0.0005", "0.001", "0.005", "0.01", "0.05", "0.1", "0.5", "1", "5", "+Inf",
        ];
        let mut cumulative = 0_u64;
        for (index, label) in labels.iter().enumerate() {
            cumulative = cumulative.saturating_add(
                self.workers
                    .iter()
                    .map(|worker| worker.metrics.request_duration_buckets[index])
                    .sum::<u64>(),
            );
            output.push_str(&format!(
                "pam_http_request_duration_seconds_bucket{{le=\"{label}\"}} {cumulative}\n"
            ));
        }
        output.push_str(&format!(
            "pam_http_request_duration_seconds_sum {:.6}\npam_http_request_duration_seconds_count {}\n",
            sum(|worker| worker.metrics.request_duration_micros) as f64 / 1_000_000.0,
            sum(|worker| worker.metrics.requests),
        ));
        for worker in &self.workers {
            let pool = worker.pool.as_deref().unwrap_or("default");
            let pool_label = crate::prometheus::label(pool);
            output.push_str(&format!(
                "pam_cluster_worker_info{{worker=\"{}\",generation=\"{}\",pid=\"{}\",state=\"{}\",pool=\"{}\"}} 1\n",
                worker.worker_id, worker.generation, worker.pid, worker.state,
                pool_label,
            ));
            output.push_str(&format!(
                concat!(
                    "pam_worker_event_loop_lag_seconds{{worker=\"{}\",generation=\"{}\",pid=\"{}\",pool=\"{}\"}} {:.6}\n",
                    "pam_worker_event_loop_lag_max_seconds{{worker=\"{}\",generation=\"{}\",pid=\"{}\",pool=\"{}\"}} {:.6}\n",
                    "pam_worker_event_loop_lag_average_seconds{{worker=\"{}\",generation=\"{}\",pid=\"{}\",pool=\"{}\"}} {:.6}\n",
                ),
                worker.worker_id,
                worker.generation,
                worker.pid,
                pool_label,
                worker.metrics.event_loop_lag_micros as f64 / 1_000_000.0,
                worker.worker_id,
                worker.generation,
                worker.pid,
                pool_label,
                worker.metrics.event_loop_lag_max_micros as f64 / 1_000_000.0,
                worker.worker_id,
                worker.generation,
                worker.pid,
                pool_label,
                event_loop_lag_average_micros(
                    worker.metrics.event_loop_lag_total_micros,
                    worker.metrics.event_loop_lag_samples,
                ) / 1_000_000.0,
            ));
        }
        let mut pools = self
            .workers
            .iter()
            .map(|worker| worker.pool.as_deref().unwrap_or("default"))
            .collect::<Vec<_>>();
        pools.sort_unstable();
        pools.dedup();
        for pool in pools {
            let pool_label = crate::prometheus::label(pool);
            let workers = self
                .workers
                .iter()
                .filter(|worker| worker.pool.as_deref().unwrap_or("default") == pool)
                .collect::<Vec<_>>();
            let sum = |value: fn(&WorkerRuntimeRecord) -> u64| {
                workers
                    .iter()
                    .map(|worker| value(worker))
                    .fold(0_u64, u64::saturating_add)
            };
            output.push_str(&format!(
                concat!(
                    "pam_pool_workers{{pool=\"{}\"}} {}\n",
                    "pam_pool_http_requests_total{{pool=\"{}\"}} {}\n",
                    "pam_pool_http_errors_total{{pool=\"{}\"}} {}\n",
                    "pam_pool_http_active_requests{{pool=\"{}\"}} {}\n",
                    "pam_pool_event_loop_lag_seconds{{pool=\"{}\"}} {:.6}\n",
                    "pam_pool_event_loop_lag_max_seconds{{pool=\"{}\"}} {:.6}\n",
                    "pam_pool_event_loop_lag_average_seconds{{pool=\"{}\"}} {:.6}\n",
                    "pam_pool_process_resident_memory_bytes{{pool=\"{}\"}} {}\n",
                    "pam_pool_php_memory_bytes{{pool=\"{}\"}} {}\n",
                ),
                pool_label,
                workers.len(),
                pool_label,
                sum(|worker| worker.metrics.requests),
                pool_label,
                sum(|worker| worker.metrics.errors),
                pool_label,
                sum(|worker| worker.metrics.active_requests),
                pool_label,
                workers
                    .iter()
                    .map(|worker| worker.metrics.event_loop_lag_micros)
                    .max()
                    .unwrap_or(0) as f64
                    / 1_000_000.0,
                pool_label,
                workers
                    .iter()
                    .map(|worker| worker.metrics.event_loop_lag_max_micros)
                    .max()
                    .unwrap_or(0) as f64
                    / 1_000_000.0,
                pool_label,
                event_loop_lag_average_micros(
                    workers
                        .iter()
                        .map(|worker| worker.metrics.event_loop_lag_total_micros)
                        .fold(0_u64, u64::saturating_add),
                    workers
                        .iter()
                        .map(|worker| worker.metrics.event_loop_lag_samples)
                        .fold(0_u64, u64::saturating_add),
                ) / 1_000_000.0,
                pool_label,
                sum(|worker| worker.metrics.resident_memory_bytes),
                pool_label,
                sum(|worker| worker.metrics.php_memory_bytes),
            ));
        }
        output
    }
}

fn event_loop_lag_average_micros(total_micros: u64, samples: u64) -> f64 {
    if samples == 0 {
        0.0
    } else {
        total_micros as f64 / samples as f64
    }
}

fn event_loop_lag_average_micros_integer(total_micros: u64, samples: u64) -> u64 {
    if samples == 0 {
        0
    } else {
        total_micros / samples
    }
}

pub type SharedClusterSnapshot = Arc<RwLock<ClusterSnapshot>>;

pub struct ControlPlane {
    address: SocketAddr,
    stopped: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

#[derive(Clone, Debug)]
struct AdminToken {
    digest: [u8; 32],
}

impl AdminToken {
    fn from_digest(digest: [u8; 32]) -> Self {
        Self { digest }
    }

    fn authenticates(&self, candidate: Option<&str>) -> bool {
        let Some(candidate) = candidate else {
            return false;
        };
        let candidate: [u8; 32] = Sha256::digest(candidate.as_bytes()).into();
        self.digest
            .iter()
            .zip(candidate)
            .fold(0_u8, |difference, (expected, actual)| {
                difference | (expected ^ actual)
            })
            == 0
    }
}

impl ControlPlane {
    pub fn start(
        address: SocketAddr,
        snapshot: SharedClusterSnapshot,
        token_digest: Option<[u8; 32]>,
    ) -> Result<Self, String> {
        let listener = TcpListener::bind(address)
            .map_err(|error| format!("cannot bind control plane on {address}: {error}"))?;
        let address = listener
            .local_addr()
            .map_err(|error| format!("cannot inspect control plane address: {error}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("cannot configure control plane: {error}"))?;
        let stopped = Arc::new(AtomicBool::new(false));
        let thread_stopped = stopped.clone();
        let token = token_digest.map(AdminToken::from_digest);
        let thread = thread::Builder::new()
            .name("pam-control-plane".to_owned())
            .spawn(move || serve(listener, snapshot, thread_stopped, token))
            .map_err(|error| format!("cannot start control plane: {error}"))?;
        Ok(Self {
            address,
            stopped,
            thread: Some(thread),
        })
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }
}

impl Drop for ControlPlane {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        let _ = TcpStream::connect(self.address);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(
    listener: TcpListener,
    snapshot: SharedClusterSnapshot,
    stopped: Arc<AtomicBool>,
    token: Option<AdminToken>,
) {
    while !stopped.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((mut stream, _)) => respond(&mut stream, &snapshot, token.as_ref()),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20));
            }
            Err(error) => {
                eprintln!("pam: control plane accept failed: {error}");
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

fn respond(stream: &mut TcpStream, snapshot: &SharedClusterSnapshot, token: Option<&AdminToken>) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
    let mut request = [0_u8; 8 * 1024];
    let Ok(length) = stream.read(&mut request) else {
        return;
    };
    let request = std::str::from_utf8(&request[..length]).unwrap_or_default();
    let first_line = request.lines().next().unwrap_or_default();
    let mut fields = first_line.split_whitespace();
    let method = fields.next().unwrap_or_default();
    let path = fields.next().unwrap_or_default();
    let bearer = request.lines().skip(1).find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("authorization")
            .then(|| value.trim().strip_prefix("Bearer "))
            .flatten()
    });
    let current = snapshot
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (status, content_type, body) =
        control_plane_response(method, path, &current, token, bearer);
    let reason = match status {
        200 => "OK",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Service Unavailable",
    };
    let authenticate = if status == 401 {
        "WWW-Authenticate: Bearer realm=\"pam-control-plane\"\r\n"
    } else {
        ""
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\n{authenticate}Connection: close\r\n\r\n{body}",
        body.len(),
    );
    let _ = stream.write_all(response.as_bytes());
}

fn control_plane_response(
    method: &str,
    path: &str,
    current: &ClusterSnapshot,
    token: Option<&AdminToken>,
    bearer: Option<&str>,
) -> (u16, &'static str, String) {
    if token.is_some_and(|token| !token.authenticates(bearer)) {
        (
            401,
            "application/json",
            r#"{"error":"Unauthorized"}"#.to_owned(),
        )
    } else if method != "GET" {
        (
            405,
            "application/json",
            r#"{"error":"Method Not Allowed"}"#.to_owned(),
        )
    } else {
        match path {
            "/live" => {
                let healthy = current.live;
                health_response(current, healthy)
            }
            "/startup" => {
                let healthy = current.live && current.startup_complete;
                health_response(current, healthy)
            }
            "/ready" => {
                let healthy = current.ready();
                health_response(current, healthy)
            }
            "/metrics" => (200, "text/plain; version=0.0.4", current.metrics()),
            "/diagnostics" => match current.diagnostics().and_then(|diagnostics| {
                serde_json::to_string(&diagnostics)
                    .map_err(|error| format!("cannot encode control-plane diagnostics: {error}"))
            }) {
                Ok(body) => (200, "application/json", body),
                Err(_) => (
                    503,
                    "application/json",
                    serde_json::json!({
                        "schemaVersion": CONTROL_PLANE_DIAGNOSTICS_SCHEMA_VERSION,
                        "surfaceCode": CONTROL_PLANE_DIAGNOSTICS_SURFACE_CODE,
                        "resultCode": DiagnosticsResultCode::Unavailable as u8,
                    })
                    .to_string(),
                ),
            },
            _ => (
                404,
                "application/json",
                r#"{"error":"Not Found"}"#.to_owned(),
            ),
        }
    }
}

fn health_response(current: &ClusterSnapshot, healthy: bool) -> (u16, &'static str, String) {
    match current.health(healthy).and_then(|health| {
        serde_json::to_string(&health)
            .map_err(|error| format!("cannot encode control-plane health: {error}"))
    }) {
        Ok(body) => (health_status(healthy), "application/json", body),
        Err(_) => (
            503,
            "application/json",
            serde_json::to_string(&ControlPlaneHealth {
                schema_version: CONTROL_PLANE_HEALTH_SCHEMA_VERSION,
                surface_code: CONTROL_PLANE_HEALTH_SURFACE_CODE,
                result_code: HealthResultCode::Unhealthy as u8,
                healthy: false,
                generation: current.generation,
                desired_workers: current.desired_workers,
                ready_workers: 0,
                workers: Vec::new(),
            })
            .expect("control-plane health fallback is serializable"),
        ),
    }
}

fn health_status(healthy: bool) -> u16 {
    if healthy { 200 } else { 503 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker_state::{WorkerLifecycle, WorkerMetricsSnapshot};

    fn worker(
        id: usize,
        pool: &str,
        lag_micros: u64,
        max_micros: u64,
        total_micros: u64,
        samples: u64,
    ) -> WorkerRuntimeRecord {
        WorkerRuntimeRecord {
            version: 1,
            state: WorkerLifecycle::Ready as u8,
            worker_id: id,
            generation: 1,
            pid: 1_000 + id as u32,
            pool: Some(pool.to_owned()),
            spawned_at_millis: 1,
            started_at_millis: 1,
            updated_at_millis: 2,
            deadline_at_millis: None,
            request_id: None,
            metrics: WorkerMetricsSnapshot {
                event_loop_lag_micros: lag_micros,
                event_loop_lag_max_micros: max_micros,
                event_loop_lag_total_micros: total_micros,
                event_loop_lag_samples: samples,
                ..WorkerMetricsSnapshot::default()
            },
        }
    }

    #[test]
    fn aggregates_event_loop_lag_as_a_maximum() {
        let mut snapshot = ClusterSnapshot {
            live: true,
            startup_complete: true,
            desired_workers: 3,
            generation: 1,
            workers: vec![
                worker(1, "web", 1_250, 8_000, 4_000, 2),
                worker(2, "web", 4_500, 9_000, 12_000, 4),
                worker(3, "jobs", 2_000, 7_000, 5_000, 2),
            ],
        };
        snapshot.workers[0].metrics.client_disconnect_cancellations = 2;
        snapshot.workers[1].metrics.client_disconnect_cancellations = 3;
        snapshot.workers[2].metrics.client_disconnect_cancellations = 1;

        let metrics = snapshot.metrics();
        assert!(metrics.contains("pam_http_client_disconnect_cancellations_total 6\n"));
        assert!(metrics.contains("pam_event_loop_lag_seconds 0.004500\n"));
        assert!(metrics.contains("pam_event_loop_lag_max_seconds 0.009000\n"));
        assert!(metrics.contains("pam_event_loop_lag_average_seconds 0.002625\n"));
        assert!(metrics.contains("pam_pool_event_loop_lag_seconds{pool=\"web\"} 0.004500\n"));
        assert!(metrics.contains("pam_pool_event_loop_lag_max_seconds{pool=\"web\"} 0.009000\n"));
        assert!(
            metrics.contains("pam_pool_event_loop_lag_average_seconds{pool=\"web\"} 0.002667\n")
        );
        assert!(metrics.contains("pam_pool_event_loop_lag_seconds{pool=\"jobs\"} 0.002000\n"));
        assert!(metrics.contains(
            "pam_worker_event_loop_lag_average_seconds{worker=\"1\",generation=\"1\",pid=\"1001\",pool=\"web\"} 0.002000\n"
        ));
    }

    #[test]
    fn hostile_worker_pool_labels_cannot_inject_prometheus_series() {
        let snapshot = ClusterSnapshot {
            live: true,
            startup_complete: true,
            desired_workers: 1,
            generation: 1,
            workers: vec![worker(
                1,
                "web\\blue\"}\npam_injected 1\n#",
                1_000,
                2_000,
                3_000,
                2,
            )],
        };

        let metrics = snapshot.metrics();
        assert!(!metrics.lines().any(|line| line.starts_with("pam_injected")));
        assert!(metrics.contains("pool=\"web\\\\blue\\\"}\\npam_injected 1\\n#\""));
    }

    #[test]
    fn emits_versioned_bounded_worker_diagnostics() {
        let snapshot = ClusterSnapshot {
            live: true,
            startup_complete: true,
            desired_workers: 2,
            generation: 7,
            workers: vec![
                worker(1, "web", 12_500, 20_000, 30_000, 2),
                worker(2, "jobs", 500, 4_000, 9_000, 3),
            ],
        };

        let diagnostics = snapshot.diagnostics().unwrap();
        assert_eq!(diagnostics.schema_version, 1);
        assert_eq!(diagnostics.surface_code, 1);
        assert_eq!(
            diagnostics.result_code,
            DiagnosticsResultCode::Available as u8
        );
        assert_eq!(diagnostics.generation, 7);
        assert_eq!(diagnostics.desired_workers, 2);
        assert_eq!(diagnostics.ready_workers, 2);
        assert_eq!(
            diagnostics.workers[0].lifecycle_code,
            WorkerLifecycle::Ready as u8
        );
        assert_eq!(
            diagnostics.workers[0].result_code,
            WorkerResultCode::Operational as u8
        );
        assert_eq!(diagnostics.workers[0].current_lag_micros, 12_500);
        assert_eq!(diagnostics.workers[0].max_lag_micros, 20_000);
        assert_eq!(diagnostics.workers[0].average_lag_micros, 15_000);
        assert_eq!(diagnostics.workers[0].lag_sample_count, 2);
        diagnostics.validate().unwrap();
    }

    #[test]
    fn rejects_unknown_worker_lifecycle_in_diagnostics() {
        let mut invalid = worker(1, "web", 0, 0, 0, 0);
        invalid.state = 9;
        let snapshot = ClusterSnapshot {
            desired_workers: 1,
            workers: vec![invalid],
            ..ClusterSnapshot::default()
        };

        assert!(snapshot.diagnostics().unwrap_err().contains("lifecycle"));
    }

    #[test]
    fn routes_diagnostics_as_a_versioned_json_contract() {
        let snapshot = ClusterSnapshot {
            live: true,
            startup_complete: true,
            desired_workers: 1,
            generation: 3,
            workers: vec![worker(1, "web", 2_000, 4_000, 6_000, 2)],
        };

        let (status, content_type, body) =
            control_plane_response("GET", "/diagnostics", &snapshot, None, None);
        assert_eq!(status, 200);
        assert_eq!(content_type, "application/json");
        let diagnostics: ControlPlaneDiagnostics = serde_json::from_str(&body).unwrap();
        diagnostics.validate().unwrap();
        assert_eq!(
            diagnostics.result_code,
            DiagnosticsResultCode::Available as u8
        );

        let (status, _, _) = control_plane_response("POST", "/diagnostics", &snapshot, None, None);
        assert_eq!(status, 405);
    }

    #[test]
    fn preserves_health_fields_inside_a_versioned_integer_contract() {
        let snapshot = ClusterSnapshot {
            live: true,
            startup_complete: true,
            desired_workers: 1,
            generation: 9,
            workers: vec![worker(1, "web", 0, 0, 0, 0)],
        };
        let (status, content_type, body) =
            control_plane_response("GET", "/ready", &snapshot, None, None);
        assert_eq!(status, 200);
        assert_eq!(content_type, "application/json");
        let health: ControlPlaneHealth = serde_json::from_str(&body).unwrap();
        assert_eq!(health.schema_version, 1);
        assert_eq!(health.surface_code, 1);
        assert_eq!(health.result_code, HealthResultCode::Healthy as u8);
        assert!(health.healthy);
        assert_eq!(health.generation, 9);
        assert_eq!(health.desired_workers, 1);
        assert_eq!(health.ready_workers, 1);
        assert_eq!(health.workers[0].state, WorkerLifecycle::Ready as u8);

        let value: serde_json::Value = serde_json::from_str(&body).unwrap();
        for legacy in [
            "healthy",
            "generation",
            "desiredWorkers",
            "readyWorkers",
            "workers",
        ] {
            assert!(value.get(legacy).is_some(), "missing legacy field {legacy}");
        }
    }

    #[test]
    fn invalid_worker_state_fails_health_closed_with_a_valid_envelope() {
        let mut invalid = worker(1, "web", 0, 0, 0, 0);
        invalid.state = 9;
        let snapshot = ClusterSnapshot {
            live: true,
            startup_complete: true,
            desired_workers: 1,
            generation: 4,
            workers: vec![invalid],
        };
        let (status, _, body) = control_plane_response("GET", "/live", &snapshot, None, None);
        assert_eq!(status, 503);
        let health: ControlPlaneHealth = serde_json::from_str(&body).unwrap();
        assert_eq!(health.result_code, HealthResultCode::Unhealthy as u8);
        assert!(!health.healthy);
        assert!(health.workers.is_empty());
    }

    #[test]
    fn authenticates_control_plane_tokens_without_storing_plaintext() {
        let snapshot = ClusterSnapshot::default();
        let token =
            AdminToken::from_digest(Sha256::digest(b"0123456789abcdef0123456789abcdef").into());
        assert_eq!(token.digest.len(), 32);

        let (status, _, _) =
            control_plane_response("GET", "/diagnostics", &snapshot, Some(&token), None);
        assert_eq!(status, 401);
        let (status, _, _) = control_plane_response(
            "GET",
            "/diagnostics",
            &snapshot,
            Some(&token),
            Some("wrong-token"),
        );
        assert_eq!(status, 401);
        let (status, _, _) = control_plane_response(
            "GET",
            "/diagnostics",
            &snapshot,
            Some(&token),
            Some("0123456789abcdef0123456789abcdef"),
        );
        assert_eq!(status, 200);
    }
}
