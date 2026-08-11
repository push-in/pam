use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderName, HeaderValue, Request, Response, StatusCode, Uri};
use http_body_util::BodyExt;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::sync::watch;

#[derive(Clone, Debug)]
pub struct Route {
    pub name: String,
    pub address: SocketAddr,
    pub prefixes: Vec<String>,
}

#[derive(Clone)]
struct IngressState {
    routes: Arc<Vec<Route>>,
    client: Client<HttpConnector, Body>,
}

pub struct Ingress {
    shutdown: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}

impl Ingress {
    pub async fn start(address: SocketAddr, routes: Vec<Route>) -> Result<Self, String> {
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .map_err(|error| format!("cannot bind PAM pool ingress {address}: {error}"))?;
        let state = IngressState {
            routes: Arc::new(routes),
            client: Client::builder(TokioExecutor::new()).build(HttpConnector::new()),
        };
        let app = Router::new().fallback(proxy).with_state(state);
        let (shutdown, mut receiver) = watch::channel(false);
        let task = tokio::spawn(async move {
            if let Err(error) = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async move {
                while !*receiver.borrow() && receiver.changed().await.is_ok() {}
            })
            .await
            {
                eprintln!("pam: pool ingress stopped unexpectedly: {error}");
            }
        });
        Ok(Self { shutdown, task })
    }

    pub async fn stop(self) {
        let _ = self.shutdown.send(true);
        let _ = self.task.await;
    }
}

async fn proxy(
    State(state): State<IngressState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    mut request: Request<Body>,
) -> Response<Body> {
    let path = request.uri().path();
    let route = state
        .routes
        .iter()
        .flat_map(|route| route.prefixes.iter().map(move |prefix| (route, prefix)))
        .filter(|(_, prefix)| {
            prefix.as_str() == "*"
                || path == prefix.as_str()
                || (path.starts_with(prefix.as_str())
                    && (prefix.ends_with('/') || path.as_bytes().get(prefix.len()) == Some(&b'/')))
        })
        .max_by_key(|(_, prefix)| {
            if prefix.as_str() == "*" {
                0
            } else {
                prefix.len()
            }
        })
        .map(|(route, _)| route);
    let Some(route) = route else {
        return unavailable("no PAM worker pool matches this route");
    };
    let path_and_query = request
        .uri()
        .path_and_query()
        .map_or("/", |value| value.as_str());
    let Ok(uri) = format!("http://{}{}", route.address, path_and_query).parse::<Uri>() else {
        return unavailable("invalid PAM worker pool target");
    };
    *request.uri_mut() = uri;
    request.headers_mut().insert(
        HeaderName::from_static("x-pam-worker-pool"),
        HeaderValue::from_str(&route.name).unwrap_or(HeaderValue::from_static("unknown")),
    );
    request.headers_mut().insert(
        HeaderName::from_static("x-forwarded-for"),
        HeaderValue::from_str(&peer.ip().to_string())
            .unwrap_or(HeaderValue::from_static("unknown")),
    );
    request.headers_mut().insert(
        HeaderName::from_static("x-forwarded-proto"),
        HeaderValue::from_static("http"),
    );
    let client_upgrade = hyper::upgrade::on(&mut request);
    match state.client.request(request).await {
        Ok(mut response) => {
            if response.status() == StatusCode::SWITCHING_PROTOCOLS {
                let backend_upgrade = hyper::upgrade::on(&mut response);
                tokio::spawn(async move {
                    let (Ok(client), Ok(backend)) = (client_upgrade.await, backend_upgrade.await)
                    else {
                        return;
                    };
                    let (mut client, mut backend) = (TokioIo::new(client), TokioIo::new(backend));
                    if let Err(error) =
                        tokio::io::copy_bidirectional(&mut client, &mut backend).await
                    {
                        eprintln!("pam: upgraded pool connection closed: {error}");
                    }
                });
            }
            response.map(|body| Body::new(body.map_err(std::io::Error::other)))
        }
        Err(error) => {
            eprintln!("pam: pool {} unavailable: {error}", route.name);
            unavailable("PAM worker pool unavailable")
        }
    }
}

fn unavailable(message: &'static str) -> Response<Body> {
    Response::builder()
        .status(StatusCode::SERVICE_UNAVAILABLE)
        .header("content-type", "application/json")
        .body(Body::from(format!(r#"{{"error":"{message}"}}"#)))
        .expect("static ingress response is valid")
}
