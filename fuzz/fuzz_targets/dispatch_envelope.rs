#![no_main]

use libfuzzer_sys::fuzz_target;

#[allow(dead_code)]
#[path = "../../src/protocol.rs"]
mod protocol;

fuzz_target!(|payload: &[u8]| {
    let _ = protocol::parse_dispatch_envelope(payload);
});
