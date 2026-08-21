use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub const WORKER_STATE_PATH_ENV: &str = "PAM_WORKER_STATE_PATH";
const WORKER_STATE_VERSION: u8 = 1;

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerLifecycle {
    Starting = 1,
    Ready = 2,
    Busy = 3,
    Draining = 4,
}

impl TryFrom<u8> for WorkerLifecycle {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Starting),
            2 => Ok(Self::Ready),
            3 => Ok(Self::Busy),
            4 => Ok(Self::Draining),
            _ => Err(format!("unknown worker lifecycle {value}")),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerMetricsSnapshot {
    pub requests: u64,
    pub errors: u64,
    #[serde(default)]
    pub client_disconnect_cancellations: u64,
    pub active_requests: u64,
    pub request_duration_micros: u64,
    pub request_duration_buckets: [u64; 11],
    pub request_bytes: u64,
    pub response_bytes: u64,
    pub response_cache_hits: u64,
    pub response_cache_misses: u64,
    pub response_cache_collapsed: u64,
    pub response_cache_stale: u64,
    pub response_cache_purges: u64,
    pub websocket_connections: u64,
    pub websocket_messages: u64,
    pub websocket_backpressure: u64,
    pub event_loop_lag_micros: u64,
    #[serde(default)]
    pub event_loop_lag_max_micros: u64,
    #[serde(default)]
    pub event_loop_lag_total_micros: u64,
    #[serde(default)]
    pub event_loop_lag_samples: u64,
    pub resident_memory_bytes: u64,
    pub php_memory_bytes: u64,
    pub php_peak_memory_bytes: u64,
    pub php_fibers: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerRuntimeRecord {
    pub version: u8,
    pub state: u8,
    pub worker_id: usize,
    pub generation: u64,
    pub pid: u32,
    #[serde(default)]
    pub pool: Option<String>,
    #[serde(default)]
    pub spawned_at_millis: u64,
    #[serde(default)]
    pub startup_phases: Option<WorkerStartupPhases>,
    pub started_at_millis: u64,
    pub updated_at_millis: u64,
    pub deadline_at_millis: Option<u64>,
    pub request_id: Option<String>,
    pub metrics: WorkerMetricsSnapshot,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerStartupPhases {
    pub spawn_to_process_millis: u64,
    pub php_engine_millis: u64,
    pub spawn_to_engine_millis: u64,
    pub composer_millis: u64,
    pub runtime_bootstrap_millis: u64,
    pub application_millis: u64,
}

impl WorkerRuntimeRecord {
    pub fn lifecycle(&self) -> Result<WorkerLifecycle, String> {
        if self.version != WORKER_STATE_VERSION {
            return Err(format!("unsupported worker state version {}", self.version));
        }
        WorkerLifecycle::try_from(self.state)
    }

    pub fn deadline_exceeded(&self, now_millis: u64) -> bool {
        self.lifecycle().ok() == Some(WorkerLifecycle::Busy)
            && self
                .deadline_at_millis
                .is_some_and(|deadline| now_millis >= deadline)
    }
}

#[derive(Clone, Debug)]
pub struct WorkerStateReporter {
    inner: Option<Arc<ReporterInner>>,
}

#[derive(Debug)]
struct ReporterInner {
    path: PathBuf,
    worker_id: usize,
    generation: u64,
    pool: Option<String>,
    spawned_at_millis: u64,
    started_at_millis: u64,
    startup_milestones: Option<crate::php::StartupMilestones>,
    pending: Mutex<PendingRecord>,
    background_writer: AtomicBool,
}

#[derive(Clone, Debug, Default)]
struct PendingRecord {
    lifecycle: Option<WorkerLifecycle>,
    deadline_at_millis: Option<u64>,
    request_id: Option<String>,
    metrics: WorkerMetricsSnapshot,
    startup_phases: Option<WorkerStartupPhases>,
    revision: u64,
}

impl WorkerStateReporter {
    pub fn from_environment() -> Self {
        let Some(path) = std::env::var_os(WORKER_STATE_PATH_ENV).map(PathBuf::from) else {
            return Self { inner: None };
        };
        let inner = Arc::new(ReporterInner {
            path,
            worker_id: std::env::var("PAM_WORKER_ID")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0),
            generation: std::env::var("PAM_WORKER_GENERATION")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(1),
            pool: std::env::var("PAM_WORKER_POOL").ok(),
            spawned_at_millis: std::env::var("PAM_WORKER_SPAWNED_AT_MILLIS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or_else(epoch_millis),
            started_at_millis: epoch_millis(),
            startup_milestones: crate::php::startup_milestones(),
            pending: Mutex::new(PendingRecord::default()),
            background_writer: AtomicBool::new(false),
        });

        let weak = Arc::downgrade(&inner);
        match thread::Builder::new()
            .name("pam-worker-state".to_owned())
            .spawn(move || publish_pending_records(weak))
        {
            Ok(_) => inner.background_writer.store(true, Ordering::Release),
            Err(error) => {
                eprintln!("pam: cannot start worker state publisher: {error}");
            }
        }

        Self { inner: Some(inner) }
    }

    pub fn enabled(&self) -> bool {
        self.inner.is_some()
    }

    pub fn report(
        &self,
        lifecycle: WorkerLifecycle,
        deadline_at_millis: Option<u64>,
        request_id: Option<&str>,
    ) -> Result<(), String> {
        let Some(inner) = &self.inner else {
            return Ok(());
        };
        {
            let mut pending = inner
                .pending
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            pending.lifecycle = Some(lifecycle);
            if lifecycle == WorkerLifecycle::Ready && pending.startup_phases.is_none() {
                pending.startup_phases = inner.startup_phases(epoch_millis());
            }
            pending.deadline_at_millis = deadline_at_millis;
            pending.request_id = request_id.map(str::to_owned);
            pending.revision = pending.revision.wrapping_add(1);
        }
        self.publish_synchronously_if_needed(inner)
    }

    pub fn update_metrics(&self, metrics: WorkerMetricsSnapshot) -> Result<(), String> {
        let Some(inner) = &self.inner else {
            return Ok(());
        };
        {
            let mut pending = inner
                .pending
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            pending.metrics = metrics;
            pending.revision = pending.revision.wrapping_add(1);
        }
        self.publish_synchronously_if_needed(inner)
    }

    fn publish_synchronously_if_needed(&self, inner: &ReporterInner) -> Result<(), String> {
        if inner.background_writer.load(Ordering::Acquire) {
            return Ok(());
        }
        let pending = inner
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let Some(record) = inner.record(&pending) else {
            return Ok(());
        };
        write_atomic(&inner.path, &record)
    }
}

impl ReporterInner {
    fn record(&self, pending: &PendingRecord) -> Option<WorkerRuntimeRecord> {
        let lifecycle = pending.lifecycle?;
        Some(WorkerRuntimeRecord {
            version: WORKER_STATE_VERSION,
            state: lifecycle as u8,
            worker_id: self.worker_id,
            generation: self.generation,
            pid: std::process::id(),
            pool: self.pool.clone(),
            spawned_at_millis: self.spawned_at_millis,
            startup_phases: pending.startup_phases,
            started_at_millis: self.started_at_millis,
            updated_at_millis: epoch_millis(),
            deadline_at_millis: pending.deadline_at_millis,
            request_id: pending.request_id.clone(),
            metrics: pending.metrics.clone(),
        })
    }

    fn startup_phases(&self, ready_at_millis: u64) -> Option<WorkerStartupPhases> {
        let milestones = self.startup_milestones?;
        Some(startup_phases(
            self.spawned_at_millis,
            milestones,
            ready_at_millis,
        ))
    }
}

fn startup_phases(
    spawned_at_millis: u64,
    milestones: crate::php::StartupMilestones,
    ready_at_millis: u64,
) -> WorkerStartupPhases {
    WorkerStartupPhases {
        spawn_to_process_millis: milestones
            .process_started_at_millis
            .saturating_sub(spawned_at_millis),
        php_engine_millis: milestones
            .engine_ready_at_millis
            .saturating_sub(milestones.process_started_at_millis),
        spawn_to_engine_millis: milestones
            .engine_ready_at_millis
            .saturating_sub(spawned_at_millis),
        composer_millis: milestones
            .composer_ready_at_millis
            .saturating_sub(milestones.engine_ready_at_millis),
        runtime_bootstrap_millis: milestones
            .runtime_ready_at_millis
            .saturating_sub(milestones.composer_ready_at_millis),
        application_millis: ready_at_millis.saturating_sub(milestones.runtime_ready_at_millis),
    }
}

fn publish_pending_records(inner: Weak<ReporterInner>) {
    let mut published_revision = 0;
    loop {
        let Some(inner) = inner.upgrade() else {
            return;
        };
        let pending = inner
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if pending.revision != published_revision {
            if let Some(record) = inner.record(&pending)
                && let Err(error) = write_atomic(&inner.path, &record)
            {
                eprintln!("pam: {error}");
            }
            published_revision = pending.revision;
        }
        drop(inner);
        thread::sleep(Duration::from_millis(5));
    }
}

pub fn read(path: &Path) -> Result<WorkerRuntimeRecord, String> {
    let contents = fs::read(path)
        .map_err(|error| format!("cannot read worker state {}: {error}", path.display()))?;
    let record: WorkerRuntimeRecord = serde_json::from_slice(&contents)
        .map_err(|error| format!("invalid worker state {}: {error}", path.display()))?;
    record.lifecycle()?;
    Ok(record)
}

pub fn epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn write_atomic(path: &Path, record: &WorkerRuntimeRecord) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let contents = serde_json::to_vec(record)
        .map_err(|error| format!("cannot serialize worker state: {error}"))?;
    fs::write(&temporary, contents)
        .map_err(|error| format!("cannot write {}: {error}", temporary.display()))?;
    fs::rename(&temporary, path).map_err(|error| {
        format!(
            "cannot publish worker state {} -> {}: {error}",
            temporary.display(),
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_worker_state_defaults_additive_lag_aggregates() {
        let source = serde_json::json!({
            "version": 1,
            "state": 2,
            "workerId": 1,
            "generation": 1,
            "pid": 42,
            "pool": "web",
            "startedAtMillis": 1,
            "updatedAtMillis": 2,
            "deadlineAtMillis": null,
            "requestId": null,
            "metrics": {
                "requests": 0,
                "errors": 0,
                "activeRequests": 0,
                "requestDurationMicros": 0,
                "requestDurationBuckets": [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                "requestBytes": 0,
                "responseBytes": 0,
                "responseCacheHits": 0,
                "responseCacheMisses": 0,
                "responseCacheCollapsed": 0,
                "responseCacheStale": 0,
                "responseCachePurges": 0,
                "websocketConnections": 0,
                "websocketMessages": 0,
                "websocketBackpressure": 0,
                "eventLoopLagMicros": 1250,
                "residentMemoryBytes": 0,
                "phpMemoryBytes": 0,
                "phpPeakMemoryBytes": 0,
                "phpFibers": 0
            }
        });
        let record: WorkerRuntimeRecord = serde_json::from_value(source).unwrap();
        assert_eq!(record.spawned_at_millis, 0);
        assert!(record.startup_phases.is_none());
        assert_eq!(record.metrics.event_loop_lag_micros, 1_250);
        assert_eq!(record.metrics.event_loop_lag_max_micros, 0);
        assert_eq!(record.metrics.event_loop_lag_total_micros, 0);
        assert_eq!(record.metrics.event_loop_lag_samples, 0);
    }

    #[test]
    fn startup_phases_partition_the_worker_critical_path() {
        let phases = startup_phases(
            100,
            crate::php::StartupMilestones {
                process_started_at_millis: 115,
                engine_ready_at_millis: 130,
                composer_ready_at_millis: 145,
                runtime_ready_at_millis: 165,
            },
            210,
        );

        assert_eq!(phases.spawn_to_process_millis, 15);
        assert_eq!(phases.php_engine_millis, 15);
        assert_eq!(phases.spawn_to_engine_millis, 30);
        assert_eq!(phases.composer_millis, 15);
        assert_eq!(phases.runtime_bootstrap_millis, 20);
        assert_eq!(phases.application_millis, 45);
    }
}
