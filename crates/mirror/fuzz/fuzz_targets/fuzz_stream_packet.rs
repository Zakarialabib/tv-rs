#![no_main]

use libfuzzer_sys::fuzz_target;
use tv_rs_mirror::StreamPacket;

fuzz_target!(|data: &[u8]| {
    if let Ok(packet) = StreamPacket::decode(data) {
        // Ensure that encoding the decoded packet does not panic
        let _ = packet.encode();
    }
});
