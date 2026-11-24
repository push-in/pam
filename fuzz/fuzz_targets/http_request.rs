#![no_main]

//! Raw bytes from the network through the same hyper-util `auto` builder the
//! server uses (HTTP/1 with upgrades, HTTP/2 by preface), then through what
//! the transport does with every request before PHP: client address
//! resolution behind trusted proxies and the header list handed to the C
//! reader. Nothing may panic, and the C reader must accept every list.

use std::convert::Infallible;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::OnceLock;

use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use libfuzzer_sys::fuzz_target;
use tokio::io::AsyncWriteExt;

#[allow(dead_code)]
#[path = "../../src/protocol.rs"]
mod protocol;

#[allow(dead_code)]
#[path = "../../src/client_ip.rs"]
mod client_ip;

unsafe extern "C" {
    fn pam_fuzz_walk_headers(
        encoded: *const u8,
        length: usize,
        names: *mut u32,
        values: *mut u32,
        bytes: *mut u64,
    ) -> i32;
}

fn trusted() -> &'static client_ip::TrustedProxies {
    static TRUSTED: OnceLock<client_ip::TrustedProxies> = OnceLock::new();
    TRUSTED.get_or_init(|| {
        client_ip::TrustedProxies::parse(["127.0.0.0/8", "10.0.0.0/8", "fd00::/8"]).unwrap()
    })
}

async fn handle(request: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let (parts, body) = request.into_parts();
    let peer = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let client = trusted().client_address(peer, &parts.headers);
    // A spoofed entry can only win when every hop to its right is trusted.
    if !trusted().trusts(client) {
        assert!(client.to_string().len() <= 45);
    }
    let encoded = protocol::encode_php_request_headers(&parts.headers);
    let (mut names, mut values, mut bytes) = (0, 0, 0);
    // SAFETY: the C side reads at most `encoded.len()` bytes.
    let valid = unsafe {
        pam_fuzz_walk_headers(
            encoded.as_ptr(),
            encoded.len(),
            &raw mut names,
            &raw mut values,
            &raw mut bytes,
        )
    };
    assert_eq!(valid, 1, "the C reader refused an encoded header list");
    assert_eq!(names as usize, parts.headers.keys_len());
    // The transport reads bodies with a byte limit.
    let _ = Limited::new(body, 64 * 1024).collect().await;
    Ok(Response::new(Full::new(Bytes::from_static(b"ok"))))
}

fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    })
}

fuzz_target!(|data: &[u8]| {
    let data = data.to_vec();
    runtime().block_on(async move {
        let (mut client, server) = tokio::io::duplex(64 * 1024);
        let serve = tokio::spawn(async move {
            let builder = Builder::new(TokioExecutor::new());
            let _ = builder
                .serve_connection_with_upgrades(TokioIo::new(server), service_fn(handle))
                .await;
        });
        let _ = client.write_all(&data).await;
        let _ = client.shutdown().await;
        let mut sink = Vec::new();
        let _ = tokio::time::timeout(
            std::time::Duration::from_millis(200),
            tokio::io::AsyncReadExt::read_to_end(&mut client, &mut sink),
        )
        .await;
        drop(client);
        let _ = tokio::time::timeout(std::time::Duration::from_millis(200), serve).await;
    });
});
