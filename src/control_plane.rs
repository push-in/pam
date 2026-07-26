use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::worker_state::{WorkerLifecycle, WorkerRuntimeRecord, epoch_millis};

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

    fn json(&self, healthy: bool) -> String {
        serde_json::json!({
            "healthy": healthy,
            "generation": self.generation,
            "desiredWorkers": self.desired_workers,
            "readyWorkers": self.workers.iter().filter(|worker| {
                matches!(worker.lifecycle(), Ok(WorkerLifecycle::Ready | WorkerLifecycle::Busy))
                    && !worker.deadline_exceeded(epoch_millis())
            }).count(),
            "workers": self.workers.iter().map(|worker| serde_json::json!({
                "id": worker.worker_id,
                "generation": worker.generation,
                "pid": worker.pid,
                "state": worker.state,
            })).collect::<Vec<_>>(),
        })
        .to_string()
    }

    fn metrics(&self) -> String {
        let mut output = String::from(concat!(
            "# TYPE pam_cluster_ready gauge\n",
            "# TYPE pam_cluster_workers gauge\n",
            "# TYPE pam_cluster_worker_info gauge\n",
            "# TYPE pam_http_requests_total counter\n",
            "# TYPE pam_http_errors_total counter\n",
            "# TYPE pam_http_active_requests gauge\n",
            "# TYPE pam_http_request_duration_seconds counter\n",
            "# TYPE pam_http_request_bytes_total counter\n",
            "# TYPE pam_http_response_bytes_total counter\n",
            "# TYPE pam_websocket_connections gauge\n",
            "# TYPE pam_websocket_messages_total counter\n",
            "# TYPE pam_websocket_backpressure_total counter\n",
            "# TYPE pam_process_resident_memory_bytes gauge\n",
            "# TYPE pam_php_memory_bytes gauge\n",
            "# TYPE pam_php_peak_memory_bytes gauge\n",
            "# TYPE pam_php_fibers gauge\n",
        ));
        let sum =
            |value: fn(&WorkerRuntimeRecord) -> u64| self.workers.iter().map(value).sum::<u64>();
        output.push_str(&format!(
            concat!(
                "pam_cluster_ready {}\n",
                "pam_cluster_workers {}\n",
                "pam_http_requests_total {}\n",
                "pam_http_errors_total {}\n",
                "pam_http_active_requests {}\n",
                "pam_http_request_duration_seconds {:.6}\n",
                "pam_http_request_bytes_total {}\n",
                "pam_http_response_bytes_total {}\n",
                "pam_websocket_connections {}\n",
                "pam_websocket_messages_total {}\n",
                "pam_websocket_backpressure_total {}\n",
                "pam_process_resident_memory_bytes {}\n",
                "pam_php_memory_bytes {}\n",
                "pam_php_peak_memory_bytes {}\n",
                "pam_php_fibers {}\n",
            ),
            u8::from(self.ready()),
            self.workers.len(),
            sum(|worker| worker.metrics.requests),
            sum(|worker| worker.metrics.errors),
            sum(|worker| worker.metrics.active_requests),
            sum(|worker| worker.metrics.request_duration_micros) as f64 / 1_000_000.0,
            sum(|worker| worker.metrics.request_bytes),
            sum(|worker| worker.metrics.response_bytes),
            sum(|worker| worker.metrics.websocket_connections),
            sum(|worker| worker.metrics.websocket_messages),
            sum(|worker| worker.metrics.websocket_backpressure),
            sum(|worker| worker.metrics.resident_memory_bytes),
            sum(|worker| worker.metrics.php_memory_bytes),
            sum(|worker| worker.metrics.php_peak_memory_bytes),
            sum(|worker| worker.metrics.php_fibers),
        ));
        for worker in &self.workers {
            output.push_str(&format!(
                "pam_cluster_worker_info{{worker=\"{}\",generation=\"{}\",pid=\"{}\",state=\"{}\"}} 1\n",
                worker.worker_id, worker.generation, worker.pid, worker.state,
            ));
        }
        output
    }
}

pub type SharedClusterSnapshot = Arc<RwLock<ClusterSnapshot>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ControlAction {
    Reload = 1,
    Drain = 2,
}

pub struct ControlPlane {
    address: SocketAddr,
    stopped: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ControlPlane {
    pub fn start(
        address: SocketAddr,
        snapshot: SharedClusterSnapshot,
        token: Option<String>,
    ) -> Result<(Self, Receiver<ControlAction>), String> {
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
        let (actions, receiver) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("pam-control-plane".to_owned())
            .spawn(move || serve(listener, snapshot, thread_stopped, token, actions))
            .map_err(|error| format!("cannot start control plane: {error}"))?;
        Ok((
            Self {
                address,
                stopped,
                thread: Some(thread),
            },
            receiver,
        ))
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
    token: Option<String>,
    actions: Sender<ControlAction>,
) {
    while !stopped.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((mut stream, _)) => respond(&mut stream, &snapshot, token.as_deref(), &actions),
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

fn respond(
    stream: &mut TcpStream,
    snapshot: &SharedClusterSnapshot,
    token: Option<&str>,
    actions: &Sender<ControlAction>,
) {
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
    let authorization = request.lines().skip(1).find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("authorization")
            .then(|| value.trim())
    });
    let (status, content_type, body) = if method == "POST" {
        mutation_response(path, authorization, token, actions)
    } else if method != "GET" {
        json_response(405, "Method Not Allowed")
    } else {
        let current = snapshot
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match path {
            "/live" => {
                let healthy = current.live;
                (
                    health_status(healthy),
                    "application/json",
                    current.json(healthy),
                )
            }
            "/startup" => {
                let healthy = current.live && current.startup_complete;
                (
                    health_status(healthy),
                    "application/json",
                    current.json(healthy),
                )
            }
            "/ready" => {
                let healthy = current.ready();
                (
                    health_status(healthy),
                    "application/json",
                    current.json(healthy),
                )
            }
            "/metrics" => (200, "text/plain; version=0.0.4", current.metrics()),
            _ => json_response(404, "Not Found"),
        }
    };
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        503 => "Service Unavailable",
        _ => "Service Unavailable",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len(),
    );
    let _ = stream.write_all(response.as_bytes());
}

fn mutation_response(
    path: &str,
    authorization: Option<&str>,
    token: Option<&str>,
    actions: &Sender<ControlAction>,
) -> (u16, &'static str, String) {
    let Some(expected) = token else {
        return json_response(503, "Administrative mutations are disabled");
    };
    let supplied = authorization
        .and_then(|value| value.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, token)| token)
        .unwrap_or_default();
    if !constant_time_equal(supplied.as_bytes(), expected.as_bytes()) {
        return json_response(401, "Unauthorized");
    }
    let action = match path {
        "/reload" => ControlAction::Reload,
        "/drain" => ControlAction::Drain,
        _ => return json_response(404, "Not Found"),
    };
    if actions.send(action).is_err() {
        return json_response(503, "Cluster supervisor is unavailable");
    }
    (
        202,
        "application/json",
        serde_json::json!({"accepted": true, "action": action as u8}).to_string(),
    )
}

fn json_response(status: u16, message: &str) -> (u16, &'static str, String) {
    (
        status,
        "application/json",
        serde_json::json!({"error": message}).to_string(),
    )
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let length = left.len().max(right.len());
    for index in 0..length {
        let left = left.get(index).copied().unwrap_or(0);
        let right = right.get(index).copied().unwrap_or(0);
        difference |= usize::from(left ^ right);
    }
    difference == 0
}

fn health_status(healthy: bool) -> u16 {
    if healthy { 200 } else { 503 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn administrative_tokens_are_compared_without_prefix_matches() {
        assert!(constant_time_equal(b"secret", b"secret"));
        assert!(!constant_time_equal(b"secret", b"secret-extra"));
        assert!(!constant_time_equal(b"", b"secret"));
    }
}
