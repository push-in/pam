#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../src/protocol.rs"]
mod protocol;

fuzz_target!(|data: &[u8]| {
    let maximum = data.len().min(1024 * 1024);
    if let Ok(decoded) = protocol::decode_chunked_body(data, maximum) {
        assert!(decoded.len() <= maximum);
    }
});
