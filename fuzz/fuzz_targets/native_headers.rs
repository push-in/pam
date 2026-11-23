#![no_main]

//! The boundary between the Rust request header encoder and the C reader in
//! `native/pam_header_reader.h` (what `pam_build_request_headers` uses):
//! arbitrary bytes must never make the reader leave its buffer, and every
//! list the encoder produces must read back with the same names and values.

use http::{HeaderMap, HeaderName, HeaderValue};
use libfuzzer_sys::fuzz_target;

#[allow(dead_code)]
#[path = "../../src/protocol.rs"]
mod protocol;

unsafe extern "C" {
    fn pam_fuzz_walk_headers(
        encoded: *const u8,
        length: usize,
        names: *mut u32,
        values: *mut u32,
        bytes: *mut u64,
    ) -> i32;
}

fn walk(encoded: &[u8]) -> Option<(u32, u32, u64)> {
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
    (valid == 1).then_some((names, values, bytes))
}

fuzz_target!(|data: &[u8]| {
    // 1. Raw bytes straight into the reader.
    let _ = walk(data);

    // 2. Header maps built from the input must round-trip.
    let mut headers = HeaderMap::new();
    for line in data.split(|byte| *byte == b'\n').take(64) {
        let Some(colon) = line.iter().position(|byte| *byte == b':') else {
            continue;
        };
        let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(&line[..colon]),
            HeaderValue::from_bytes(&line[colon + 1..]),
        ) else {
            continue;
        };
        headers.append(name, value);
    }
    let encoded = protocol::encode_php_request_headers(&headers);
    let (names, values, bytes) =
        walk(&encoded).expect("the encoder produced a list the C reader refused");
    assert_eq!(names as usize, headers.keys_len());
    let expected_values: Vec<&HeaderValue> = headers
        .values()
        .filter(|value| value.to_str().is_ok())
        .collect();
    assert_eq!(values as usize, expected_values.len());
    let expected_bytes: usize = headers
        .keys()
        .map(|name| name.as_str().len())
        .sum::<usize>()
        + expected_values
            .iter()
            .map(|value| value.len())
            .sum::<usize>();
    assert_eq!(bytes as usize, expected_bytes);
});
