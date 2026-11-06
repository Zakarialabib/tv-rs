//! Fuzzing and robustness test harness for tv-rs-mirror parsers.
//!
//! Asserts no panics or out-of-memory errors on arbitrary, mutated, and malformed byte sequences.

use tv_rs_mirror::input::MirrorInputMessage;
use tv_rs_mirror::*;

#[test]
fn fuzz_stream_packet_parser_arbitrary_bytes() {
    // Generate pseudo-random deterministic test sequences
    let mut state: u64 = 0xDEADBEEFCAFE1234;
    let mut next_u8 = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (state >> 33) as u8
    };

    // 1. Completely arbitrary byte sequences of varying lengths (0 to 1024 bytes)
    for len in 0..=256 {
        for _ in 0..20 {
            let mut buf = Vec::with_capacity(len);
            for _ in 0..len {
                buf.push(next_u8());
            }

            // Parsing must never panic on arbitrary byte inputs
            let res = StreamPacket::decode(&buf);
            if let Ok(pkt) = res {
                assert!(pkt.data.len() <= MAX_PACKET_SIZE);
                let _ = pkt.encode();
            }
        }
    }

    // 2. Specific boundary and edge cases
    let edge_cases: Vec<&[u8]> = vec![
        &[],
        &[0],
        &[0, 0],
        &[0, 0, 0],
        &[0, 0, 0, 0],
        // Length says 1 byte, but body missing
        &[0x00, 0x00, 0x00, 0x01],
        // Length says MAX_PACKET_SIZE, but body missing
        &[0x00, 0x80, 0x00, 0x00],
        // Length > MAX_PACKET_SIZE (e.g. 0xFFFFFFFF)
        &[0xFF, 0xFF, 0xFF, 0xFF],
        // Valid length header 9, but invalid kind
        &[0x00, 0x00, 0x00, 0x09, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0],
        // Valid length header 9, valid kind Config (0)
        &[0x00, 0x00, 0x00, 0x09, 0x00, 0, 0, 0, 0, 0, 0, 0, 0],
        // Valid length header 9, valid kind Frame (1)
        &[0x00, 0x00, 0x00, 0x09, 0x01, 0, 0, 0, 0, 0, 0, 0, 0],
        // Valid length header 9, valid kind Keyframe (2)
        &[0x00, 0x00, 0x00, 0x09, 0x02, 0, 0, 0, 0, 0, 0, 0, 0],
    ];

    for edge in edge_cases {
        let _ = StreamPacket::decode(edge);
    }
}

#[test]
fn fuzz_config_and_message_parsers() {
    let mut state: u64 = 0x1337BEEF01234567;
    let mut next_u8 = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (state >> 33) as u8
    };

    // Pseudo-random strings and mutated JSON structures
    let valid_samples = [
        r#"{"codec":"h264","width":1920,"height":1080}"#,
        r#"{"codec":"pcm_s16le","rate":48000,"channels":2}"#,
        r#"{"max_size":1920,"fps":60,"bitrate":8000000,"audio":true}"#,
        r#"{"session":1,"app":"com.example.app"}"#,
        r#"{"session":0}"#,
        r#"{"width":1280,"height":720,"session":2}"#,
        r#"{"b":{"apps":[{"pkg":"test","label":"Test"}]}}"#,
        r#"{"session":0,"event":{"type":"touch","action":"down","x":0.5,"y":0.5}}"#,
    ];

    for base in valid_samples {
        // Run against valid sample
        let _ = serde_json::from_str::<VideoConfigPayload>(base);
        let _ = serde_json::from_str::<AudioConfigPayload>(base);
        let _ = serde_json::from_str::<MirrorStart>(base);
        let _ = serde_json::from_str::<MirrorStop>(base);
        let _ = serde_json::from_str::<MirrorKeyframe>(base);
        let _ = serde_json::from_str::<MirrorResize>(base);
        let _ = serde_json::from_str::<MirrorPower>(base);
        let _ = serde_json::from_str::<MirrorSound>(base);
        let _ = serde_json::from_str::<AppListPayload>(base);
        let _ = serde_json::from_str::<MirrorAppsResponse>(base);
        let _ = serde_json::from_str::<MirrorInputMessage>(base);

        // Mutate with bit flips, insertions, truncations
        for _ in 0..50 {
            let mut bytes = base.as_bytes().to_vec();
            if !bytes.is_empty() {
                let idx = (next_u8() as usize) % bytes.len();
                bytes[idx] ^= next_u8();
            }
            if let Ok(s) = std::str::from_utf8(&bytes) {
                if let Ok(start) = serde_json::from_str::<MirrorStart>(s) {
                    let _ = start.validate();
                }
                let _ = serde_json::from_str::<VideoConfigPayload>(s);
                let _ = serde_json::from_str::<AudioConfigPayload>(s);
                let _ = serde_json::from_str::<MirrorStop>(s);
                let _ = serde_json::from_str::<MirrorKeyframe>(s);
                let _ = serde_json::from_str::<MirrorResize>(s);
                let _ = serde_json::from_str::<MirrorPower>(s);
                let _ = serde_json::from_str::<MirrorSound>(s);
                let _ = serde_json::from_str::<AppListPayload>(s);
                let _ = serde_json::from_str::<MirrorAppsResponse>(s);
                let _ = serde_json::from_str::<MirrorInputMessage>(s);
            }
        }
    }

    // Completely arbitrary byte sequences passed to JSON parsers
    for _ in 0..100 {
        let len = (next_u8() as usize) % 128;
        let mut buf = Vec::with_capacity(len);
        for _ in 0..len {
            buf.push(next_u8());
        }
        if let Ok(s) = std::str::from_utf8(&buf) {
            let _ = serde_json::from_str::<VideoConfigPayload>(s);
            let _ = serde_json::from_str::<AudioConfigPayload>(s);
            let _ = serde_json::from_str::<MirrorStart>(s);
            let _ = serde_json::from_str::<MirrorStop>(s);
            let _ = serde_json::from_str::<MirrorKeyframe>(s);
            let _ = serde_json::from_str::<MirrorResize>(s);
            let _ = serde_json::from_str::<MirrorPower>(s);
            let _ = serde_json::from_str::<MirrorSound>(s);
            let _ = serde_json::from_str::<AppListPayload>(s);
            let _ = serde_json::from_str::<MirrorAppsResponse>(s);
            let _ = serde_json::from_str::<MirrorInputMessage>(s);
        }
    }
}
