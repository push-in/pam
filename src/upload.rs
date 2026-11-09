//! Streams `multipart/form-data` request bodies to temporary files.
//!
//! PHP's own SAPIs never hold an upload in memory: the rfc1867 handler writes
//! every file part to a temporary file in `upload_tmp_dir` while the body is
//! read, applies `upload_max_filesize`, `post_max_size`, `max_file_uploads`
//! and the `MAX_FILE_SIZE` form field, and leaves `php://input` empty. This
//! module does the same in the transport, so a 1 GB upload costs a bounded
//! buffer of memory regardless of PHP's `memory_limit`. PHP then receives the
//! form fields and a manifest of the files, registers the files with
//! `is_uploaded_file()`, and the files are deleted when the request ends
//! unless the application moved them.

use std::fs::OpenOptions;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use axum::body::{Body, HttpBody};
use axum::http::StatusCode;
use bytes::{Buf, BytesMut};
use http_body_util::BodyExt;
use memchr::memmem;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

const UPLOAD_ERR_OK: u8 = 0;
const UPLOAD_ERR_INI_SIZE: u8 = 1;
const UPLOAD_ERR_FORM_SIZE: u8 = 2;
const UPLOAD_ERR_PARTIAL: u8 = 3;
const UPLOAD_ERR_NO_FILE: u8 = 4;
const UPLOAD_ERR_CANT_WRITE: u8 = 7;

/// Part headers larger than this are rejected (PHP uses a similar bound).
const MAX_PART_HEADER_BYTES: usize = 16 * 1024;
/// A body rejected for its size is still read (and dropped) up to this many
/// bytes, so the client sees the response; beyond it the connection closes.
const DISCARD_LIMIT_BYTES: u64 = 64 * 1024 * 1024;
/// File data is written in slices of this size.
const WRITE_SLICE_BYTES: usize = 1024 * 1024;

/// PHP's upload settings, read from the INI when the server starts.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UploadLimits {
    /// `file_uploads`.
    pub enabled: bool,
    /// `upload_max_filesize` in bytes; 0 disables the limit.
    pub max_file_bytes: u64,
    /// `post_max_size` in bytes; 0 disables the limit.
    pub max_post_bytes: u64,
    /// `max_file_uploads`.
    pub max_files: usize,
    /// `upload_tmp_dir`, or the system temporary directory.
    pub temporary_directory: Option<String>,
}

impl Default for UploadLimits {
    fn default() -> Self {
        Self {
            enabled: true,
            max_file_bytes: 2 * 1024 * 1024,
            max_post_bytes: 8 * 1024 * 1024,
            max_files: 20,
            temporary_directory: None,
        }
    }
}

impl UploadLimits {
    fn directory(&self) -> PathBuf {
        self.temporary_directory
            .as_deref()
            .filter(|directory| !directory.is_empty())
            .map_or_else(std::env::temp_dir, PathBuf::from)
    }
}

/// One `$_FILES` entry.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadedFile {
    /// The form field name, with PHP's bracket syntax left intact.
    pub field: String,
    pub name: String,
    pub full_path: String,
    #[serde(rename = "type")]
    pub media_type: String,
    pub tmp_name: String,
    pub error: u8,
    pub size: u64,
}

/// A parsed multipart body: URL-encoded fields (PHP applies `parse_str()`
/// rules to them) and the uploaded files.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MultipartBody {
    pub fields: String,
    pub files: Vec<UploadedFile>,
    /// The body exceeded `post_max_size`; like PHP, nothing was parsed.
    pub post_limit_exceeded: bool,
    #[serde(skip)]
    pub received_bytes: u64,
    #[serde(skip)]
    registered: AtomicBool,
}

impl MultipartBody {
    /// Paths of the temporary files that exist.
    pub fn temporary_files(&self) -> impl Iterator<Item = &str> {
        self.files
            .iter()
            .map(|file| file.tmp_name.as_str())
            .filter(|path| !path.is_empty())
    }

    /// Marks the files as owned by the PHP request (see [`php::release_upload`]).
    pub fn mark_registered(&self) {
        self.registered.store(true, Ordering::Release);
    }
}

impl Drop for MultipartBody {
    fn drop(&mut self) {
        // Files PHP registered are released (and deleted unless moved) by the
        // request on the PHP thread. Anything else is deleted here.
        if self.registered.load(Ordering::Acquire) {
            return;
        }
        for path in self.temporary_files() {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[derive(Debug)]
pub enum UploadError {
    Malformed(&'static str),
    Timeout,
    Aborted,
}

impl UploadError {
    pub fn status(&self) -> StatusCode {
        match self {
            Self::Malformed(_) | Self::Aborted => StatusCode::BAD_REQUEST,
            Self::Timeout => StatusCode::REQUEST_TIMEOUT,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Malformed(reason) => format!("malformed multipart body: {reason}"),
            Self::Timeout => "request body was not received before its timeout".to_owned(),
            Self::Aborted => "request body was interrupted".to_owned(),
        }
    }
}

/// Whether PHP would parse this request body as an upload.
pub fn is_multipart(content_type: &str) -> bool {
    content_type
        .trim_start()
        .get(..19)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("multipart/form-data"))
}

fn boundary(content_type: &str) -> Option<String> {
    for parameter in content_type.split(';').skip(1) {
        let Some((name, value)) = parameter.split_once('=') else {
            continue;
        };
        if !name.trim().eq_ignore_ascii_case("boundary") {
            continue;
        }
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .unwrap_or(value);
        if value.is_empty() || value.len() > 200 {
            return None;
        }
        return Some(value.to_owned());
    }
    None
}

enum State {
    Preamble,
    AfterDelimiter,
    Headers,
    Body,
    Epilogue,
}

enum Sink {
    Field { name: String, value: Vec<u8> },
    File(FileSink),
    Discard,
}

struct FileSink {
    index: usize,
    file: Option<tokio::fs::File>,
    pending: Vec<u8>,
    size: u64,
    limit: Option<(u64, u8)>,
}

/// Reads a multipart body, streaming file parts to temporary files.
pub async fn read(
    mut body: Body,
    content_type: &str,
    content_length: Option<u64>,
    limits: &UploadLimits,
    idle_timeout: Duration,
) -> Result<MultipartBody, UploadError> {
    let mut parsed = MultipartBody::default();
    if limits.max_post_bytes > 0
        && content_length.is_some_and(|length| length > limits.max_post_bytes)
    {
        // PHP does not parse a body over post_max_size and leaves $_POST and
        // $_FILES empty; the application decides (Laravel answers 413).
        parsed.post_limit_exceeded = true;
        parsed.received_bytes = discard(&mut body, idle_timeout).await;
        return Ok(parsed);
    }
    let Some(boundary) = boundary(content_type) else {
        // PHP warns about the missing boundary and parses nothing.
        return Ok(parsed);
    };
    let delimiter = format!("\r\n--{boundary}").into_bytes();
    let finder = memmem::Finder::new(&delimiter);
    let line_end = memmem::Finder::new(b"\r\n");
    let headers_end = memmem::Finder::new(b"\r\n\r\n");
    let directory = limits.directory();

    // The leading CRLF lets the first delimiter match like every other one.
    let mut buffer = BytesMut::from(&b"\r\n"[..]);
    let mut state = State::Preamble;
    let mut sink = Sink::Discard;
    let mut fields = Vec::<u8>::new();
    let mut form_max_file_size: Option<u64> = None;
    let mut ended = body.is_end_stream();

    loop {
        loop {
            match state {
                State::Preamble => match finder.find(&buffer) {
                    Some(index) => {
                        buffer.advance(index + delimiter.len());
                        state = State::AfterDelimiter;
                    }
                    None => {
                        let keep = delimiter.len() - 1;
                        if buffer.len() > keep {
                            buffer.advance(buffer.len() - keep);
                        }
                        break;
                    }
                },
                State::AfterDelimiter => {
                    if buffer.len() < 2 {
                        break;
                    }
                    if buffer.starts_with(b"--") {
                        state = State::Epilogue;
                        continue;
                    }
                    match line_end.find(&buffer) {
                        Some(index) => {
                            // Transport padding after the delimiter is ignored.
                            buffer.advance(index + 2);
                            state = State::Headers;
                        }
                        None if buffer.len() > 256 => {
                            return Err(UploadError::Malformed("invalid delimiter line"));
                        }
                        None => break,
                    }
                }
                State::Headers => {
                    let header_bytes = if buffer.starts_with(b"\r\n") {
                        Some(0)
                    } else {
                        headers_end.find(&buffer).map(|index| index + 2)
                    };
                    let Some(header_bytes) = header_bytes else {
                        if buffer.len() > MAX_PART_HEADER_BYTES {
                            return Err(UploadError::Malformed("part headers are too large"));
                        }
                        break;
                    };
                    let headers = String::from_utf8_lossy(&buffer[..header_bytes]).into_owned();
                    buffer.advance(header_bytes + 2);
                    sink = start_part(
                        &headers,
                        &mut parsed,
                        limits,
                        form_max_file_size,
                        &directory,
                    )
                    .await?;
                    state = State::Body;
                }
                State::Body => match finder.find(&buffer) {
                    Some(index) => {
                        let data = buffer.split_to(index);
                        write_part(&mut sink, &data, &mut parsed).await?;
                        buffer.advance(delimiter.len());
                        finish_part(
                            std::mem::replace(&mut sink, Sink::Discard),
                            &mut parsed,
                            &mut fields,
                            &mut form_max_file_size,
                            false,
                        )
                        .await;
                        state = State::AfterDelimiter;
                    }
                    None => {
                        let keep = delimiter.len() - 1;
                        if buffer.len() > keep {
                            let data = buffer.split_to(buffer.len() - keep);
                            write_part(&mut sink, &data, &mut parsed).await?;
                        }
                        break;
                    }
                },
                State::Epilogue => {
                    buffer.clear();
                    break;
                }
            }
        }
        if ended {
            break;
        }
        let frame = match tokio::time::timeout(idle_timeout, body.frame()).await {
            Err(_) => return Err(UploadError::Timeout),
            Ok(None) => {
                ended = true;
                continue;
            }
            Ok(Some(Err(_))) => return Err(UploadError::Aborted),
            Ok(Some(Ok(frame))) => frame,
        };
        let Ok(data) = frame.into_data() else {
            continue;
        };
        parsed.received_bytes = parsed.received_bytes.saturating_add(data.len() as u64);
        if limits.max_post_bytes > 0 && parsed.received_bytes > limits.max_post_bytes {
            // A body without Content-Length that outgrows post_max_size.
            drop(sink);
            let received = parsed
                .received_bytes
                .saturating_add(discard(&mut body, idle_timeout).await);
            drop(parsed);
            return Ok(MultipartBody {
                post_limit_exceeded: true,
                received_bytes: received,
                fields: String::new(),
                files: Vec::new(),
                registered: AtomicBool::new(false),
            });
        }
        buffer.extend_from_slice(&data);
    }

    if matches!(state, State::Body) {
        // The body ended inside a part: like PHP, keep what was complete and
        // report a partial file.
        finish_part(
            sink,
            &mut parsed,
            &mut fields,
            &mut form_max_file_size,
            true,
        )
        .await;
    }
    parsed.fields = String::from_utf8(fields).unwrap_or_default();
    Ok(parsed)
}

/// Reads and drops the rest of a rejected body (up to [`DISCARD_LIMIT_BYTES`])
/// so the client, which is usually still sending, receives the response
/// instead of a reset.
async fn discard(body: &mut Body, idle_timeout: Duration) -> u64 {
    let mut discarded = 0_u64;
    while discarded < DISCARD_LIMIT_BYTES {
        match tokio::time::timeout(idle_timeout, body.frame()).await {
            Ok(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    discarded = discarded.saturating_add(data.len() as u64);
                }
            }
            _ => break,
        }
    }
    discarded
}

async fn start_part(
    headers: &str,
    parsed: &mut MultipartBody,
    limits: &UploadLimits,
    form_max_file_size: Option<u64>,
    directory: &std::path::Path,
) -> Result<Sink, UploadError> {
    let mut disposition = None;
    let mut media_type = None;
    for line in headers.split("\r\n") {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name.eq_ignore_ascii_case("content-disposition") {
            disposition = Some(value.trim().to_owned());
        } else if name.eq_ignore_ascii_case("content-type") {
            media_type = Some(value.trim().to_owned());
        }
    }
    let Some(disposition) = disposition else {
        return Ok(Sink::Discard);
    };
    let parameters = disposition_parameters(&disposition);
    let Some(field) = parameters
        .iter()
        .find(|(name, _)| name == "name")
        .map(|(_, value)| value.clone())
        .filter(|name| !name.is_empty())
    else {
        return Ok(Sink::Discard);
    };
    let Some(filename) = parameters
        .iter()
        .find(|(name, _)| name == "filename")
        .map(|(_, value)| value.clone())
    else {
        return Ok(Sink::Field {
            name: field,
            value: Vec::new(),
        });
    };
    if !limits.enabled {
        return Ok(Sink::Discard);
    }
    let media_type = media_type.unwrap_or_default();
    let name = filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_owned();
    if filename.is_empty() {
        // A file input left empty.
        parsed.files.push(UploadedFile {
            field,
            name,
            full_path: filename,
            media_type,
            tmp_name: String::new(),
            error: UPLOAD_ERR_NO_FILE,
            size: 0,
        });
        return Ok(Sink::Discard);
    }
    let uploaded = parsed
        .files
        .iter()
        .filter(|file| file.error != UPLOAD_ERR_NO_FILE)
        .count();
    if uploaded >= limits.max_files {
        // PHP skips files beyond max_file_uploads.
        return Ok(Sink::Discard);
    }
    let limit = match (form_max_file_size, limits.max_file_bytes) {
        // PHP checks upload_max_filesize first, then MAX_FILE_SIZE.
        (_, ini) if ini > 0 && form_max_file_size.is_none_or(|form| ini <= form) => {
            Some((ini, UPLOAD_ERR_INI_SIZE))
        }
        (Some(form), _) => Some((form, UPLOAD_ERR_FORM_SIZE)),
        _ => None,
    };
    let (path, file) = match create_temporary(directory).await {
        Ok(created) => created,
        Err(error) => {
            eprintln!("pam: cannot create a temporary upload file: {error}");
            parsed.files.push(UploadedFile {
                field,
                name,
                full_path: filename,
                media_type,
                tmp_name: String::new(),
                error: UPLOAD_ERR_CANT_WRITE,
                size: 0,
            });
            return Ok(Sink::Discard);
        }
    };
    parsed.files.push(UploadedFile {
        field,
        name,
        full_path: filename,
        media_type,
        tmp_name: path,
        error: UPLOAD_ERR_OK,
        size: 0,
    });
    Ok(Sink::File(FileSink {
        index: parsed.files.len() - 1,
        file: Some(file),
        pending: Vec::new(),
        size: 0,
        limit,
    }))
}

async fn write_part(
    sink: &mut Sink,
    data: &[u8],
    parsed: &mut MultipartBody,
) -> Result<(), UploadError> {
    match sink {
        Sink::Discard => Ok(()),
        Sink::Field { value, .. } => {
            value.extend_from_slice(data);
            Ok(())
        }
        Sink::File(file) => {
            if file.file.is_none() {
                return Ok(());
            }
            file.size = file.size.saturating_add(data.len() as u64);
            if let Some((limit, error)) = file.limit
                && file.size > limit
            {
                cancel_file(file, parsed, error).await;
                return Ok(());
            }
            file.pending.extend_from_slice(data);
            if file.pending.len() >= WRITE_SLICE_BYTES {
                flush_pending(file, parsed).await;
            }
            Ok(())
        }
    }
}

async fn flush_pending(file: &mut FileSink, parsed: &mut MultipartBody) {
    let Some(handle) = file.file.as_mut() else {
        return;
    };
    if let Err(error) = handle.write_all(&file.pending).await {
        eprintln!("pam: cannot write an uploaded file: {error}");
        cancel_file(file, parsed, UPLOAD_ERR_CANT_WRITE).await;
        return;
    }
    file.pending.clear();
}

/// Stops storing a file part, deletes what was written and records `error`
/// (the rest of the part is still read and discarded).
async fn cancel_file(file: &mut FileSink, parsed: &mut MultipartBody, error: u8) {
    file.file = None;
    file.pending = Vec::new();
    let entry = &mut parsed.files[file.index];
    let _ = tokio::fs::remove_file(&entry.tmp_name).await;
    entry.tmp_name = String::new();
    entry.error = error;
    entry.size = 0;
}

async fn finish_part(
    sink: Sink,
    parsed: &mut MultipartBody,
    fields: &mut Vec<u8>,
    form_max_file_size: &mut Option<u64>,
    truncated: bool,
) {
    match sink {
        Sink::Discard => {}
        Sink::Field { name, value } => {
            if truncated {
                return;
            }
            if name == "MAX_FILE_SIZE" {
                *form_max_file_size = std::str::from_utf8(&value)
                    .ok()
                    .and_then(|value| value.trim().parse::<u64>().ok())
                    .filter(|size| *size > 0);
            }
            if !fields.is_empty() {
                fields.push(b'&');
            }
            percent_encode(name.as_bytes(), fields);
            fields.push(b'=');
            percent_encode(&value, fields);
        }
        Sink::File(mut file) => {
            if file.file.is_some() {
                flush_pending(&mut file, parsed).await;
            }
            if let Some(mut handle) = file.file.take()
                && let Err(error) = handle.flush().await
            {
                eprintln!("pam: cannot write an uploaded file: {error}");
                file.file = None;
                cancel_file(&mut file, parsed, UPLOAD_ERR_CANT_WRITE).await;
                return;
            }
            let entry = &mut parsed.files[file.index];
            if entry.tmp_name.is_empty() {
                return;
            }
            if truncated {
                let _ = tokio::fs::remove_file(&entry.tmp_name).await;
                entry.tmp_name = String::new();
                entry.error = UPLOAD_ERR_PARTIAL;
                entry.size = 0;
            } else {
                entry.size = file.size;
            }
        }
    }
}

/// Creates `phpXXXXXX`-style files with mode 0600, like PHP's mkstemp().
async fn create_temporary(directory: &std::path::Path) -> io::Result<(String, tokio::fs::File)> {
    for _ in 0..16 {
        let mut random = [0_u8; 8];
        getrandom(&mut random)?;
        let suffix: String = random
            .iter()
            .map(|byte| {
                const ALPHABET: &[u8] =
                    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
                ALPHABET[usize::from(*byte) % ALPHABET.len()] as char
            })
            .collect();
        let path = directory.join(format!("php{suffix}"));
        let Some(text) = path.to_str().map(str::to_owned) else {
            return Err(io::Error::other("the upload directory is not UTF-8"));
        };
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(file) => return Ok((text, tokio::fs::File::from_std(file))),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other("cannot pick a unique temporary file name"))
}

fn getrandom(buffer: &mut [u8]) -> io::Result<()> {
    use std::io::Read;
    std::fs::File::open("/dev/urandom")?.read_exact(buffer)
}

/// `rawurlencode()`: everything but unreserved characters is escaped.
fn percent_encode(input: &[u8], output: &mut Vec<u8>) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &byte in input {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            output.push(byte);
        } else {
            output.extend_from_slice(&[
                b'%',
                HEX[usize::from(byte >> 4)],
                HEX[usize::from(byte & 15)],
            ]);
        }
    }
}

/// Parameters of a `Content-Disposition` value, with quoted strings and
/// backslash escapes handled like PHP's parser.
fn disposition_parameters(value: &str) -> Vec<(String, String)> {
    let mut parameters = Vec::new();
    let mut rest = value;
    // Skip the disposition type.
    match rest.find(';') {
        Some(index) => rest = &rest[index + 1..],
        None => return parameters,
    }
    loop {
        rest = rest.trim_start_matches([' ', '\t', ';']);
        if rest.is_empty() {
            break;
        }
        let Some(equals) = rest.find('=') else {
            break;
        };
        let name = rest[..equals].trim().to_ascii_lowercase();
        rest = rest[equals + 1..].trim_start();
        let value;
        if let Some(quoted) = rest.strip_prefix('"') {
            let mut unescaped = String::new();
            let mut characters = quoted.char_indices();
            let mut end = quoted.len();
            while let Some((index, character)) = characters.next() {
                match character {
                    '\\' => match characters.clone().next() {
                        Some((_, next @ ('"' | '\\'))) => {
                            unescaped.push(next);
                            characters.next();
                        }
                        _ => unescaped.push('\\'),
                    },
                    '"' => {
                        end = index + 1;
                        break;
                    }
                    other => unescaped.push(other),
                }
            }
            value = unescaped;
            rest = &quoted[end.min(quoted.len())..];
        } else {
            let end = rest.find(';').unwrap_or(rest.len());
            value = rest[..end].trim().to_owned();
            rest = &rest[end..];
        }
        parameters.push((name, value));
    }
    parameters
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits(directory: &std::path::Path) -> UploadLimits {
        UploadLimits {
            enabled: true,
            max_file_bytes: 1024,
            max_post_bytes: 64 * 1024,
            max_files: 2,
            temporary_directory: Some(directory.to_str().unwrap().to_owned()),
        }
    }

    fn chunked(parts: Vec<Vec<u8>>) -> Body {
        let stream = futures_util::stream::iter(
            parts
                .into_iter()
                .map(|part| Ok::<_, std::io::Error>(bytes::Bytes::from(part))),
        );
        Body::from_stream(stream)
    }

    fn scratch(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "pam-upload-test-{name}-{}-{}",
            std::process::id(),
            crate::worker_state::epoch_millis()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn parses_disposition_parameters() {
        let parameters =
            disposition_parameters(r#"form-data; name="files[]"; filename="C:\\dir\\a \"b\".txt""#);
        assert_eq!(parameters[0], ("name".to_owned(), "files[]".to_owned()));
        assert_eq!(
            parameters[1],
            ("filename".to_owned(), r#"C:\dir\a "b".txt"#.to_owned())
        );
        assert!(is_multipart("Multipart/Form-Data; boundary=x"));
        assert_eq!(
            boundary(r#"multipart/form-data; boundary="a b""#).as_deref(),
            Some("a b")
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn streams_fields_and_files_split_across_chunks() {
        let directory = scratch("chunks");
        let body = b"preamble\r\n--XyZ\r\nContent-Disposition: form-data; name=\"tags[]\"\r\n\r\na&b\r\n--XyZ\r\nContent-Disposition: form-data; name=\"doc\"; filename=\"/tmp/x/report.txt\"\r\nContent-Type: text/plain\r\n\r\nhello\r\n--XyY-world\r\n--XyZ\r\nContent-Disposition: form-data; name=\"big\"; filename=\"big.bin\"\r\n\r\n".to_vec();
        let mut body = body;
        body.extend(std::iter::repeat_n(b'z', 2000));
        body.extend_from_slice(b"\r\n--XyZ\r\nContent-Disposition: form-data; name=\"none\"; filename=\"\"\r\n\r\n\r\n--XyZ--\r\nepilogue");
        let chunks = body.chunks(7).map(<[u8]>::to_vec).collect();
        let parsed = read(
            chunked(chunks),
            "multipart/form-data; boundary=XyZ",
            None,
            &limits(&directory),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        assert_eq!(parsed.fields, "tags%5B%5D=a%26b");
        assert_eq!(parsed.files.len(), 3);
        let doc = &parsed.files[0];
        assert_eq!(
            (doc.name.as_str(), doc.full_path.as_str()),
            ("report.txt", "/tmp/x/report.txt")
        );
        assert_eq!(doc.error, UPLOAD_ERR_OK);
        assert_eq!(
            std::fs::read(&doc.tmp_name).unwrap(),
            b"hello\r\n--XyY-world"
        );
        assert_eq!(doc.size, 18);
        let big = &parsed.files[1];
        assert_eq!(
            (big.error, big.size, big.tmp_name.as_str()),
            (UPLOAD_ERR_INI_SIZE, 0, "")
        );
        assert_eq!(parsed.files[2].error, UPLOAD_ERR_NO_FILE);
        let path = doc.tmp_name.clone();
        drop(parsed);
        assert!(
            !std::path::Path::new(&path).exists(),
            "unregistered uploads are deleted"
        );
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn applies_post_max_size_and_max_file_uploads() {
        let directory = scratch("limits");
        let parsed = read(
            Body::empty(),
            "multipart/form-data; boundary=a",
            Some(1 << 20),
            &limits(&directory),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        assert!(parsed.post_limit_exceeded);
        let mut body = Vec::new();
        for index in 0..3 {
            body.extend_from_slice(
                format!("--a\r\nContent-Disposition: form-data; name=\"f{index}\"; filename=\"{index}.txt\"\r\n\r\n{index}\r\n").as_bytes(),
            );
        }
        body.extend_from_slice(b"--a\r\nContent-Disposition: form-data; name=\"partial\"; filename=\"p.txt\"\r\n\r\nunfinished");
        let parsed = read(
            chunked(vec![body]),
            "multipart/form-data; boundary=a",
            None,
            &limits(&directory),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        assert_eq!(parsed.files.len(), 2, "max_file_uploads skips the rest");
        assert!(!parsed.post_limit_exceeded);
        drop(parsed);
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);
        let mut oversized = b"--a\r\nContent-Disposition: form-data; name=\"x\"\r\n\r\n".to_vec();
        oversized.extend(std::iter::repeat_n(b'x', 70 * 1024));
        let parsed = read(
            chunked(oversized.chunks(4096).map(<[u8]>::to_vec).collect()),
            "multipart/form-data; boundary=a",
            None,
            &limits(&directory),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        assert!(parsed.post_limit_exceeded && parsed.fields.is_empty());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
