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
    pub started_at_millis: u64,
    pub updated_at_millis: u64,
    pub deadline_at_millis: Option<u64>,
    pub request_id: Option<String>,
    pub metrics: WorkerMetricsSnapshot,
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
    started_at_millis: u64,
    pending: Mutex<PendingRecord>,
    background_writer: AtomicBool,
}

#[derive(Clone, Debug, Default)]
struct PendingRecord {
    lifecycle: Option<WorkerLifecycle>,
    deadline_at_millis: Option<u64>,
    request_id: Option<String>,
    metrics: WorkerMetricsSnapshot,
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
            started_at_millis: epoch_millis(),
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
            started_at_millis: self.started_at_millis,
            updated_at_millis: epoch_millis(),
            deadline_at_millis: pending.deadline_at_millis,
            request_id: pending.request_id.clone(),
            metrics: pending.metrics.clone(),
        })
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
