use std::collections::HashMap;
use std::os::fd::RawFd;

use bytes::Bytes;
use serde::Deserialize;

const MAX_NATIVE_HEADER_NAMES: usize = 256;
const MAX_NATIVE_HEADER_VALUES: usize = 256;
const MAX_NATIVE_RESPONSE_CHUNKS: usize = 4_096;

#[repr(u8)]
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(try_from = "u8")]
pub(crate) enum PhpDispatchState {
    Complete = 1,
    Suspended = 2,
}

impl TryFrom<u8> for PhpDispatchState {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Complete),
            2 => Ok(Self::Suspended),
            _ => Err(format!("unknown PHP dispatch state {value}")),
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(try_from = "u8")]
pub(crate) enum PhpOperationKind {
    Timer = 1,
    Stream = 2,
    Dns = 3,
    FileRead = 4,
    FileWrite = 5,
    Process = 6,
    Signal = 7,
    ResponseChunk = 8,
}

impl TryFrom<u8> for PhpOperationKind {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Timer),
            2 => Ok(Self::Stream),
            3 => Ok(Self::Dns),
            4 => Ok(Self::FileRead),
            5 => Ok(Self::FileWrite),
            6 => Ok(Self::Process),
            7 => Ok(Self::Signal),
            8 => Ok(Self::ResponseChunk),
            _ => Err(format!("unknown PHP async operation kind {value}")),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PhpOperation {
    pub(crate) kind: PhpOperationKind,
    pub(crate) delay_micros: u64,
    #[serde(default)]
    pub(crate) read_file_descriptors: Vec<RawFd>,
    #[serde(default)]
    pub(crate) write_file_descriptors: Vec<RawFd>,
    #[serde(default)]
    pub(crate) host: Option<String>,
    #[serde(default)]
    pub(crate) path: Option<String>,
    #[serde(default)]
    pub(crate) data: Option<String>,
    #[serde(default)]
    pub(crate) atomic: bool,
    #[serde(default)]
    pub(crate) arguments: Vec<String>,
    #[serde(default)]
    pub(crate) stdin: Option<String>,
    #[serde(default = "default_native_output_limit")]
    pub(crate) max_output_bytes: usize,
    #[serde(default)]
    pub(crate) signal: Option<i32>,
    #[serde(default)]
    pub(crate) response: Option<JsonPhpHttpResponse>,
    #[serde(default)]
    pub(crate) chunk: Option<String>,
}

fn default_native_output_limit() -> usize {
    8 * 1024 * 1024
}

#[derive(Debug)]
pub(crate) struct PhpDispatchEnvelope {
    pub(crate) state: PhpDispatchState,
    pub(crate) response: Option<PhpHttpResponse>,
    pub(crate) operation: Option<PhpOperation>,
}

#[derive(Deserialize)]
struct JsonPhpDispatchEnvelope {
    state: PhpDispatchState,
    #[serde(default)]
    response: Option<JsonPhpHttpResponse>,
    #[serde(default)]
    operation: Option<PhpOperation>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct JsonPhpHttpResponse {
    pub(crate) status: u16,
    pub(crate) headers: HashMap<String, Vec<String>>,
    pub(crate) body: String,
    #[serde(default)]
    pub(crate) chunks: Vec<String>,
}

impl JsonPhpHttpResponse {
    pub(crate) fn into_php(self) -> PhpHttpResponse {
        PhpHttpResponse {
            status: self.status,
            headers: self.headers,
            body: Bytes::from(self.body),
            chunks: self.chunks.into_iter().map(Bytes::from).collect(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct PhpHttpResponse {
    pub(crate) status: u16,
    pub(crate) headers: HashMap<String, Vec<String>>,
    pub(crate) body: Bytes,
    pub(crate) chunks: Vec<Bytes>,
}

pub(crate) fn parse_dispatch_envelope(payload: &[u8]) -> Result<PhpDispatchEnvelope, String> {
    if let Some(response) = payload.strip_prefix(&[0x89, b'P', b'A', b'M', 1]) {
        return Ok(PhpDispatchEnvelope {
            state: PhpDispatchState::Complete,
            response: Some(parse_binary_http_response(response)?),
            operation: None,
        });
    }
    let envelope: JsonPhpDispatchEnvelope = serde_json::from_slice(payload)
        .map_err(|error| format!("invalid PHP dispatch envelope: {error}"))?;
    Ok(PhpDispatchEnvelope {
        state: envelope.state,
        response: envelope.response.map(JsonPhpHttpResponse::into_php),
        operation: envelope.operation,
    })
}

fn parse_binary_http_response(payload: &[u8]) -> Result<PhpHttpResponse, String> {
    let mut cursor = BinaryResponseCursor::new(payload);
    let status = cursor.u16()?;
    let header_count = usize::from(cursor.u16()?);
    if header_count > MAX_NATIVE_HEADER_NAMES {
        return Err("native HTTP response contains too many header names".to_owned());
    }
    let mut headers = HashMap::with_capacity(header_count);
    for _ in 0..header_count {
        let name_length = usize::from(cursor.u16()?);
        let name = cursor.utf8(name_length)?.to_owned();
        let value_count = usize::from(cursor.u16()?);
        if value_count > MAX_NATIVE_HEADER_VALUES {
            return Err("native HTTP response header contains too many values".to_owned());
        }
        let mut values = Vec::with_capacity(value_count);
        for _ in 0..value_count {
            let length = usize::try_from(cursor.u32()?)
                .map_err(|_| "native HTTP header length exceeds this platform".to_owned())?;
            values.push(cursor.utf8(length)?.to_owned());
        }
        headers.insert(name, values);
    }
    let body_length = usize::try_from(cursor.u32()?)
        .map_err(|_| "native HTTP body length exceeds this platform".to_owned())?;
    let body = Bytes::copy_from_slice(cursor.take(body_length)?);
    let chunk_count = usize::try_from(cursor.u32()?)
        .map_err(|_| "native HTTP chunk count exceeds this platform".to_owned())?;
    if chunk_count > MAX_NATIVE_RESPONSE_CHUNKS {
        return Err("native HTTP response contains too many chunks".to_owned());
    }
    let mut chunks = Vec::with_capacity(chunk_count);
    for _ in 0..chunk_count {
        let length = usize::try_from(cursor.u32()?)
            .map_err(|_| "native HTTP chunk length exceeds this platform".to_owned())?;
        chunks.push(Bytes::copy_from_slice(cursor.take(length)?));
    }
    if !cursor.is_empty() {
        return Err("native HTTP response contains trailing bytes".to_owned());
    }
    Ok(PhpHttpResponse {
        status,
        headers,
        body,
        chunks,
    })
}

struct BinaryResponseCursor<'a> {
    remaining: &'a [u8],
}

impl<'a> BinaryResponseCursor<'a> {
    fn new(remaining: &'a [u8]) -> Self {
        Self { remaining }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], String> {
        if length > self.remaining.len() {
            return Err("truncated native HTTP response".to_owned());
        }
        let (value, remaining) = self.remaining.split_at(length);
        self.remaining = remaining;
        Ok(value)
    }

    fn u16(&mut self) -> Result<u16, String> {
        let bytes: [u8; 2] = self
            .take(2)?
            .try_into()
            .map_err(|_| "truncated native u16".to_owned())?;
        Ok(u16::from_be_bytes(bytes))
    }

    fn u32(&mut self) -> Result<u32, String> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| "truncated native u32".to_owned())?;
        Ok(u32::from_be_bytes(bytes))
    }

    fn utf8(&mut self, length: usize) -> Result<&'a str, String> {
        std::str::from_utf8(self.take(length)?)
            .map_err(|error| format!("native HTTP metadata is not UTF-8: {error}"))
    }

    fn is_empty(&self) -> bool {
        self.remaining.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_response_parser_accepts_binary_bodies() {
        let mut payload = vec![0x89, b'P', b'A', b'M', 1];
        payload.extend_from_slice(&200_u16.to_be_bytes());
        payload.extend_from_slice(&1_u16.to_be_bytes());
        payload.extend_from_slice(&12_u16.to_be_bytes());
        payload.extend_from_slice(b"content-type");
        payload.extend_from_slice(&1_u16.to_be_bytes());
        payload.extend_from_slice(&24_u32.to_be_bytes());
        payload.extend_from_slice(b"application/octet-stream");
        payload.extend_from_slice(&4_u32.to_be_bytes());
        payload.extend_from_slice(&[0, 0xff, b'P', b'A']);
        payload.extend_from_slice(&0_u32.to_be_bytes());

        let envelope = parse_dispatch_envelope(&payload).unwrap();
        let response = envelope.response.unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body.as_ref(), &[0, 0xff, b'P', b'A']);
    }

    #[test]
    fn malformed_dispatch_payloads_never_panic() {
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        for length in 0..10_000_usize {
            let mut payload = Vec::with_capacity(length % 257);
            for _ in 0..payload.capacity() {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                payload.push(state as u8);
            }
            let _ = parse_dispatch_envelope(&payload);
        }
    }

    #[test]
    fn native_response_parser_rejects_unbounded_chunk_allocation() {
        let mut payload = vec![0x89, b'P', b'A', b'M', 1];
        payload.extend_from_slice(&200_u16.to_be_bytes());
        payload.extend_from_slice(&0_u16.to_be_bytes());
        payload.extend_from_slice(&0_u32.to_be_bytes());
        payload.extend_from_slice(&u32::MAX.to_be_bytes());

        assert_eq!(
            parse_dispatch_envelope(&payload).unwrap_err(),
            "native HTTP response contains too many chunks",
        );
    }
}
