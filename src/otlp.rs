use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode, Uri};
use http_body_util::{BodyExt, Full};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde::Serialize;
use tokio::sync::mpsc;

const DEFAULT_QUEUE_SIZE: usize = 2_048;
const DEFAULT_BATCH_SIZE: usize = 512;
const DEFAULT_DELAY_MS: u64 = 5_000;
const DEFAULT_TIMEOUT_MS: u64 = 10_000;
const MAX_RESPONSE_BYTES: usize = 65_536;

#[derive(Clone, Default)]
pub(crate) struct Counters {
    pub exported: Arc<AtomicU64>,
    pub dropped: Arc<AtomicU64>,
    pub errors: Arc<AtomicU64>,
    pub rejected: Arc<AtomicU64>,
}

#[derive(Clone)]
pub(crate) struct Exporter {
    sender: mpsc::Sender<Span>,
    pub counters: Counters,
}

#[derive(Clone)]
pub(crate) struct Span {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub trace_flags: u8,
    pub method: String,
    pub route: Option<String>,
    pub status: u16,
    pub start_unix_nano: u64,
    pub end_unix_nano: u64,
}

struct Config {
    endpoint: Uri,
    headers: HeaderMap,
    service_name: String,
    queue_size: usize,
    batch_size: usize,
    delay: Duration,
    timeout: Duration,
}

impl Exporter {
    pub(crate) fn from_environment() -> Result<Option<Self>, String> {
        let Some(config) = Config::from_environment()? else {
            return Ok(None);
        };
        let counters = Counters::default();
        let (sender, receiver) = mpsc::channel(config.queue_size);
        tokio::spawn(export_loop(config, receiver, counters.clone()));
        Ok(Some(Self { sender, counters }))
    }

    pub(crate) fn export(&self, span: Span) {
        if self.sender.try_send(span).is_err() {
            self.counters.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl Config {
    fn from_environment() -> Result<Option<Self>, String> {
        let traces_endpoint = std::env::var("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT").ok();
        let global_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
        let Some(raw_endpoint) = traces_endpoint.as_ref().or(global_endpoint.as_ref()) else {
            return Ok(None);
        };
        let protocol = std::env::var("OTEL_EXPORTER_OTLP_TRACES_PROTOCOL")
            .or_else(|_| std::env::var("OTEL_EXPORTER_OTLP_PROTOCOL"))
            .unwrap_or_else(|_| "http/protobuf".to_owned());
        if protocol != "http/json" {
            return Err(format!(
                "OTLP endpoint requires OTEL_EXPORTER_OTLP_TRACES_PROTOCOL=http/json (got {protocol:?})"
            ));
        }
        let endpoint = endpoint(raw_endpoint, traces_endpoint.is_none())?;
        let headers = parse_headers(
            std::env::var("OTEL_EXPORTER_OTLP_TRACES_HEADERS")
                .or_else(|_| std::env::var("OTEL_EXPORTER_OTLP_HEADERS"))
                .unwrap_or_default()
                .as_str(),
        )?;
        let queue_size = env_number("OTEL_BSP_MAX_QUEUE_SIZE", DEFAULT_QUEUE_SIZE, 1, 65_536)?;
        let batch_size = env_number(
            "OTEL_BSP_MAX_EXPORT_BATCH_SIZE",
            DEFAULT_BATCH_SIZE.min(queue_size),
            1,
            queue_size,
        )?;
        let delay_ms = env_number_fallback(
            "OTEL_BSP_SCHEDULE_DELAY",
            "OTEL_BSP_SCHEDULE_DELAY",
            DEFAULT_DELAY_MS,
            1,
            60_000,
        )?;
        let timeout_ms = env_number_fallback(
            "OTEL_EXPORTER_OTLP_TRACES_TIMEOUT",
            "OTEL_EXPORTER_OTLP_TIMEOUT",
            DEFAULT_TIMEOUT_MS,
            1,
            120_000,
        )?;
        let service_name = std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "pam".to_owned());
        if service_name.is_empty()
            || service_name.len() > 128
            || service_name.chars().any(char::is_control)
        {
            return Err("OTEL_SERVICE_NAME must contain 1..128 printable characters".to_owned());
        }
        Ok(Some(Self {
            endpoint,
            headers,
            service_name,
            queue_size,
            batch_size,
            delay: Duration::from_millis(delay_ms),
            timeout: Duration::from_millis(timeout_ms),
        }))
    }
}

fn endpoint(raw: &str, append_traces_path: bool) -> Result<Uri, String> {
    let mut value = raw.trim().to_owned();
    if append_traces_path {
        value = format!("{}/v1/traces", value.trim_end_matches('/'));
    }
    let uri: Uri = value
        .parse()
        .map_err(|_| "OTLP endpoint is not a valid URI".to_owned())?;
    let scheme = uri
        .scheme_str()
        .ok_or_else(|| "OTLP endpoint requires a scheme".to_owned())?;
    let authority = uri
        .authority()
        .ok_or_else(|| "OTLP endpoint requires an authority".to_owned())?;
    if authority.as_str().contains('@') {
        return Err("OTLP endpoint must not contain user information".to_owned());
    }
    if scheme != "https" && !(scheme == "http" && is_loopback(authority.host())) {
        return Err(
            "OTLP endpoint must use HTTPS; HTTP is allowed only for loopback collectors".to_owned(),
        );
    }
    Ok(uri)
}

fn is_loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn parse_headers(raw: &str) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    for entry in raw
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
    {
        let (name, value) = entry
            .split_once('=')
            .ok_or_else(|| "OTLP headers must use key=value entries".to_owned())?;
        let name = HeaderName::try_from(name.trim())
            .map_err(|_| "OTLP headers contain an invalid name".to_owned())?;
        let value = percent_decode(value.trim())?;
        let value = HeaderValue::try_from(value)
            .map_err(|_| "OTLP headers contain an invalid value".to_owned())?;
        headers.insert(name, value);
    }
    Ok(headers)
}

fn percent_decode(value: &str) -> Result<String, String> {
    let mut output = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err("OTLP header contains invalid percent encoding".to_owned());
            }
            let high = hex(bytes[index + 1])
                .ok_or_else(|| "OTLP header contains invalid percent encoding".to_owned())?;
            let low = hex(bytes[index + 2])
                .ok_or_else(|| "OTLP header contains invalid percent encoding".to_owned())?;
            output.push(high << 4 | low);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).map_err(|_| "OTLP header is not valid UTF-8".to_owned())
}

fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn env_number(name: &str, default: usize, minimum: usize, maximum: usize) -> Result<usize, String> {
    let Ok(raw) = std::env::var(name) else {
        return Ok(default);
    };
    let value = raw
        .parse::<usize>()
        .map_err(|_| format!("{name} must be an integer"))?;
    if !(minimum..=maximum).contains(&value) {
        return Err(format!("{name} must be between {minimum} and {maximum}"));
    }
    Ok(value)
}

fn env_number_fallback(
    primary: &str,
    fallback: &str,
    default: u64,
    minimum: u64,
    maximum: u64,
) -> Result<u64, String> {
    let raw = std::env::var(primary).or_else(|_| std::env::var(fallback));
    let Ok(raw) = raw else {
        return Ok(default);
    };
    let value = raw
        .parse::<u64>()
        .map_err(|_| format!("{primary} must be an integer"))?;
    if !(minimum..=maximum).contains(&value) {
        return Err(format!("{primary} must be between {minimum} and {maximum}"));
    }
    Ok(value)
}

async fn export_loop(config: Config, mut receiver: mpsc::Receiver<Span>, counters: Counters) {
    let connector = HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .build();
    let client = Client::builder(TokioExecutor::new()).build(connector);
    while let Some(first) = receiver.recv().await {
        let mut batch = Vec::with_capacity(config.batch_size);
        batch.push(first);
        let deadline = tokio::time::sleep(config.delay);
        tokio::pin!(deadline);
        while batch.len() < config.batch_size {
            tokio::select! {
                biased;
                item = receiver.recv() => match item { Some(span) => batch.push(span), None => break },
                () = &mut deadline => break,
            }
        }
        let count = batch.len() as u64;
        let body = match serde_json::to_vec(&Payload::new(&config.service_name, &batch)) {
            Ok(body) => body,
            Err(_) => {
                counters.errors.fetch_add(1, Ordering::Relaxed);
                counters.dropped.fetch_add(count, Ordering::Relaxed);
                continue;
            }
        };
        match send_with_retry(&client, &config, &body).await {
            Ok(rejected) => {
                counters
                    .exported
                    .fetch_add(count.saturating_sub(rejected), Ordering::Relaxed);
                counters.rejected.fetch_add(rejected, Ordering::Relaxed);
            }
            Err(()) => {
                counters.errors.fetch_add(1, Ordering::Relaxed);
                counters.dropped.fetch_add(count, Ordering::Relaxed);
            }
        }
    }
}

async fn send_with_retry(
    client: &Client<
        hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
        Full<Bytes>,
    >,
    config: &Config,
    body: &[u8],
) -> Result<u64, ()> {
    for attempt in 0..3_u64 {
        let request = build_request(config, body).map_err(|_| ())?;
        let result = tokio::time::timeout(config.timeout, client.request(request)).await;
        match result {
            Ok(Ok(response)) if response.status().is_success() => {
                return parse_response(response).await;
            }
            Ok(Ok(response)) if retryable(response.status()) => {}
            Ok(Err(_)) | Err(_) => {}
            _ => return Err(()),
        }
        if attempt < 2 {
            tokio::time::sleep(Duration::from_millis(
                100 * (1 << attempt) + body.len() as u64 % 47,
            ))
            .await;
        }
    }
    Err(())
}

fn build_request(config: &Config, body: &[u8]) -> Result<Request<Full<Bytes>>, http::Error> {
    let mut request = Request::builder()
        .method(Method::POST)
        .uri(config.endpoint.clone())
        .header("content-type", "application/json")
        .header("user-agent", concat!("pam/", env!("CARGO_PKG_VERSION")))
        .body(Full::new(Bytes::copy_from_slice(body)))?;
    request.headers_mut().extend(config.headers.clone());
    Ok(request)
}

fn retryable(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::TOO_MANY_REQUESTS
            | StatusCode::BAD_GATEWAY
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT
    )
}

async fn parse_response(response: hyper::Response<hyper::body::Incoming>) -> Result<u64, ()> {
    let bytes = http_body_util::Limited::new(response.into_body(), MAX_RESPONSE_BYTES)
        .collect()
        .await
        .map_err(|_| ())?
        .to_bytes();
    if bytes.is_empty() {
        return Ok(0);
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| ())?;
    Ok(value
        .pointer("/partialSuccess/rejectedSpans")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Payload<'a> {
    resource_spans: [ResourceSpans<'a>; 1],
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResourceSpans<'a> {
    resource: Resource<'a>,
    scope_spans: [ScopeSpans<'a>; 1],
}
#[derive(Serialize)]
struct Resource<'a> {
    attributes: [Attribute<'a>; 2],
}
#[derive(Serialize)]
struct Attribute<'a> {
    key: &'a str,
    value: StringValue<'a>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StringValue<'a> {
    string_value: &'a str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScopeSpans<'a> {
    scope: Scope<'a>,
    spans: Vec<JsonSpan<'a>>,
}
#[derive(Serialize)]
struct Scope<'a> {
    name: &'a str,
    version: &'a str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonSpan<'a> {
    trace_id: &'a str,
    span_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_span_id: Option<&'a str>,
    name: String,
    kind: u8,
    start_time_unix_nano: String,
    end_time_unix_nano: String,
    attributes: Vec<OwnedAttribute>,
    status: SpanStatus,
    flags: u8,
}
#[derive(Serialize)]
struct OwnedAttribute {
    key: &'static str,
    value: OwnedValue,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OwnedValue {
    #[serde(skip_serializing_if = "Option::is_none")]
    string_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    int_value: Option<String>,
}
#[derive(Serialize)]
struct SpanStatus {
    code: u8,
}

impl<'a> Payload<'a> {
    fn new(service: &'a str, spans: &'a [Span]) -> Self {
        let spans = spans
            .iter()
            .map(|span| JsonSpan {
                trace_id: &span.trace_id,
                span_id: &span.span_id,
                parent_span_id: span.parent_span_id.as_deref(),
                name: format!("HTTP {}", span.method),
                kind: 2,
                start_time_unix_nano: span.start_unix_nano.to_string(),
                end_time_unix_nano: span.end_unix_nano.to_string(),
                attributes: {
                    let mut attributes = vec![
                        OwnedAttribute {
                            key: "http.request.method",
                            value: OwnedValue {
                                string_value: Some(span.method.clone()),
                                int_value: None,
                            },
                        },
                        OwnedAttribute {
                            key: "http.response.status_code",
                            value: OwnedValue {
                                string_value: None,
                                int_value: Some(span.status.to_string()),
                            },
                        },
                    ];
                    if let Some(route) = &span.route {
                        attributes.push(OwnedAttribute {
                            key: "http.route",
                            value: OwnedValue {
                                string_value: Some(route.clone()),
                                int_value: None,
                            },
                        });
                    }
                    attributes
                },
                status: SpanStatus {
                    code: if span.status >= 500 { 2 } else { 0 },
                },
                flags: span.trace_flags,
            })
            .collect();
        Self {
            resource_spans: [ResourceSpans {
                resource: Resource {
                    attributes: [
                        Attribute {
                            key: "service.name",
                            value: StringValue {
                                string_value: service,
                            },
                        },
                        Attribute {
                            key: "service.version",
                            value: StringValue {
                                string_value: env!("CARGO_PKG_VERSION"),
                            },
                        },
                    ],
                },
                scope_spans: [ScopeSpans {
                    scope: Scope {
                        name: "pam.runtime",
                        version: env!("CARGO_PKG_VERSION"),
                    },
                    spans,
                }],
            }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn endpoint_requires_secure_remote_transport() {
        assert!(endpoint("http://collector.example/v1/traces", false).is_err());
        assert!(
            endpoint("http://127.0.0.1:4318", true)
                .unwrap()
                .to_string()
                .ends_with("/v1/traces")
        );
        assert!(endpoint("https://collector.example/v1/traces", false).is_ok());
    }

    #[test]
    fn headers_decode_percent_encoding() {
        let headers = parse_headers("authorization=Bearer%20secret,x-tenant=pam").unwrap();
        assert_eq!(headers["authorization"], "Bearer secret");
        assert_eq!(headers["x-tenant"], "pam");
    }

    #[test]
    fn payload_omits_request_targets() {
        let span = Span {
            trace_id: "01".repeat(16),
            span_id: "02".repeat(8),
            parent_span_id: None,
            trace_flags: 1,
            method: "GET".to_owned(),
            route: Some("/users/{id}".to_owned()),
            status: 200,
            start_unix_nano: 1,
            end_unix_nano: 2,
        };
        let json = serde_json::to_string(&Payload::new("pam", &[span])).unwrap();
        assert!(json.contains("/users/{id}"));
        assert!(!json.contains("requestId"));
        assert!(!json.contains("url.full"));
    }

    #[tokio::test]
    async fn exporter_posts_otlp_json_with_configured_headers() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let collector = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 4_096];
            loop {
                let read = stream.read(&mut buffer).unwrap();
                request.extend_from_slice(&buffer[..read]);
                let header_end = request
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .map(|position| position + 4);
                if let Some(header_end) = header_end {
                    let headers = String::from_utf8_lossy(&request[..header_end]);
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|value| value.parse::<usize>().ok())
                        })
                        .unwrap();
                    if request.len() >= header_end + content_length {
                        break;
                    }
                }
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}")
                .unwrap();
            String::from_utf8(request).unwrap()
        });
        let config = Config {
            endpoint: format!("http://{address}/v1/traces").parse().unwrap(),
            headers: parse_headers("authorization=Bearer%20collector-token").unwrap(),
            service_name: "pam-test".to_owned(),
            queue_size: 1,
            batch_size: 1,
            delay: Duration::from_millis(1),
            timeout: Duration::from_secs(5),
        };
        let span = Span {
            trace_id: "01".repeat(16),
            span_id: "02".repeat(8),
            parent_span_id: Some("03".repeat(8)),
            trace_flags: 1,
            method: "POST".to_owned(),
            route: None,
            status: 201,
            start_unix_nano: 1,
            end_unix_nano: 2,
        };
        let body = serde_json::to_vec(&Payload::new(&config.service_name, &[span])).unwrap();

        assert_eq!(
            send_with_retry(
                &Client::builder(TokioExecutor::new()).build(
                    HttpsConnectorBuilder::new()
                        .with_webpki_roots()
                        .https_or_http()
                        .enable_http1()
                        .build()
                ),
                &config,
                &body
            )
            .await,
            Ok(0)
        );
        let request = collector.join().unwrap();
        assert!(request.starts_with("POST /v1/traces HTTP/1.1\r\n"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer collector-token\r\n")
        );
        assert!(request.contains("\"parentSpanId\":\"0303030303030303\""));
    }
}
