#![no_main]

//! The streaming `multipart/form-data` parser (`src/upload.rs`), the most
//! exposed parser the transport owns. The input picks the boundary, how the
//! body is split into chunks (boundaries and part headers crossing chunk
//! edges) and the body itself. Nothing may panic, every limit must hold and
//! no temporary file may outlive the parsed body.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use axum::body::Body;
use libfuzzer_sys::fuzz_target;

#[allow(dead_code)]
#[path = "../../src/upload.rs"]
mod upload;

fn directory() -> &'static PathBuf {
    static DIRECTORY: OnceLock<PathBuf> = OnceLock::new();
    DIRECTORY.get_or_init(|| {
        let directory =
            std::env::temp_dir().join(format!("pam-fuzz-multipart-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        directory
    })
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
    if data.len() < 2 {
        return;
    }
    let chunk_size = usize::from(data[0]).max(1);
    let rest = &data[1..];
    let split = rest
        .iter()
        .position(|byte| *byte == b'\n')
        .unwrap_or(rest.len());
    let boundary = String::from_utf8_lossy(&rest[..split]).into_owned();
    let body = rest.get(split + 1..).unwrap_or_default().to_vec();
    let content_type = format!("multipart/form-data; boundary={boundary}");
    let limits = upload::UploadLimits {
        enabled: true,
        max_file_bytes: 4 * 1024,
        max_post_bytes: 64 * 1024,
        max_files: 3,
        max_parts: 16,
        temporary_directory: Some(directory().to_string_lossy().into_owned()),
    };
    let chunks: Vec<Result<bytes::Bytes, std::io::Error>> = body
        .chunks(chunk_size)
        .map(|chunk| Ok(bytes::Bytes::copy_from_slice(chunk)))
        .collect();
    let length = body.len() as u64;
    let parsed = runtime().block_on(upload::read(
        Body::from_stream(futures_util::stream::iter(chunks)),
        &content_type,
        Some(length),
        &limits,
        Duration::from_secs(1),
    ));
    if let Ok(parsed) = parsed {
        // Empty file inputs do not count as uploads, but every part counts.
        assert!(parsed.files.len() <= limits.max_parts);
        let uploaded = parsed
            .files
            .iter()
            .filter(|file| file.size > 0 || !file.tmp_name.is_empty())
            .count();
        assert!(uploaded <= limits.max_files);
        assert!(parsed.fields.len() as u64 <= limits.max_post_bytes.max(length) * 3);
        drop(parsed);
    }
    // Dropping an unregistered body deletes its files.
    let left = std::fs::read_dir(directory())
        .map(|entries| entries.count())
        .unwrap_or(0);
    assert_eq!(left, 0, "temporary upload files outlived the parsed body");
});
