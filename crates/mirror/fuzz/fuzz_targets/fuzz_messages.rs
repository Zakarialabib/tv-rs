#![no_main]

use libfuzzer_sys::fuzz_target;
use tv_rs_mirror::input::MirrorInputMessage;
use tv_rs_mirror::{
    AppListPayload, MirrorAppsResponse, MirrorKeyframe, MirrorPower, MirrorResize, MirrorSound,
    MirrorStart, MirrorStop,
};

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(start) = serde_json::from_str::<MirrorStart>(s) {
            let _ = start.validate();
        }
        let _ = serde_json::from_str::<MirrorStop>(s);
        let _ = serde_json::from_str::<MirrorPower>(s);
        let _ = serde_json::from_str::<MirrorKeyframe>(s);
        let _ = serde_json::from_str::<MirrorResize>(s);
        let _ = serde_json::from_str::<MirrorSound>(s);
        let _ = serde_json::from_str::<AppListPayload>(s);
        let _ = serde_json::from_str::<MirrorAppsResponse>(s);
        let _ = serde_json::from_str::<MirrorInputMessage>(s);
    }
});
