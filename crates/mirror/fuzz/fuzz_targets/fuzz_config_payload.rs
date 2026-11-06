#![no_main]

use libfuzzer_sys::fuzz_target;
use tv_rs_mirror::{AudioConfigPayload, VideoConfigPayload};

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = serde_json::from_str::<VideoConfigPayload>(s);
        let _ = serde_json::from_str::<AudioConfigPayload>(s);
    }
});
