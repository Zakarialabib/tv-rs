//! Tests for screen mirroring session contract, control messages, and framing.

use tv_rs_mirror::*;

#[test]
fn test_default_session_behavior() {
    // Missing session field must deserialize to session 0
    let json = r#"{"max_size": 1920, "fps": 60, "bitrate": 8000000}"#;
    let start: MirrorStart = serde_json::from_str(json).unwrap();
    assert_eq!(start.session, MirrorSessionId::SCREEN);
    assert_eq!(start.session.as_u32(), 0);
    assert!(start.session.is_screen());
    assert!(!start.session.is_app_window());

    // Explicit session n
    let json_app = r#"{"max_size": 1280, "fps": 30, "bitrate": 4000000, "session": 42, "app": "org.mozilla.firefox"}"#;
    let start_app: MirrorStart = serde_json::from_str(json_app).unwrap();
    assert_eq!(start_app.session, MirrorSessionId::new(42));
    assert!(start_app.session.is_app_window());
    assert_eq!(start_app.app.as_deref(), Some("org.mozilla.firefox"));
}

#[test]
fn test_start_options_and_validation() {
    // Full options on session 0
    let start_full = MirrorStart::builder(3840, 60, 20_000_000)
        .with_audio(true)
        .with_stay_awake(true)
        .with_screen_off(true)
        .build();
    assert!(start_full.validate().is_ok());
    assert_eq!(start_full.stay_awake, Some(true));
    assert_eq!(start_full.screen_off, Some(true));
    assert!(start_full.audio);

    // Watch mode on session 0
    let start_watch = MirrorStart::builder(1920, 60, 10_000_000)
        .with_watch("player_123")
        .build();
    assert!(start_watch.validate().is_ok());
    assert_eq!(start_watch.watch.as_deref(), Some("player_123"));

    // Sound only requires session 0, audio true, and cannot have app/watch/stay_awake/screen_off
    let valid_sound_only = MirrorStart::builder(1920, 60, 8_000_000)
        .with_sound_only()
        .build();
    assert!(valid_sound_only.validate().is_ok());

    let mut invalid_sound = valid_sound_only.clone();
    invalid_sound.app = Some("com.example.app".into());
    assert!(invalid_sound.validate().is_err());

    let mut invalid_sound_awake = valid_sound_only.clone();
    invalid_sound_awake.stay_awake = Some(true);
    assert!(invalid_sound_awake.validate().is_err());

    let mut invalid_sound_off = valid_sound_only.clone();
    invalid_sound_off.screen_off = Some(true);
    assert!(invalid_sound_off.validate().is_err());

    let mut invalid_sound_watch = valid_sound_only.clone();
    invalid_sound_watch.watch = Some("p1".into());
    assert!(invalid_sound_watch.validate().is_err());

    let mut invalid_sound_no_audio = valid_sound_only.clone();
    invalid_sound_no_audio.audio = false;
    assert!(invalid_sound_no_audio.validate().is_err());

    // App window session requires app parameter
    let app_without_pkg = MirrorStart::builder(1920, 60, 8_000_000)
        .with_session(MirrorSessionId::new(5))
        .build();
    assert!(app_without_pkg.validate().is_err());

    // App window cannot be combined with watch
    let mut app_with_watch = MirrorStart::builder(1920, 60, 8_000_000)
        .with_app(MirrorSessionId::new(5), "pkg")
        .build();
    app_with_watch.watch = Some("watch_mode".into());
    assert!(app_with_watch.validate().is_err());

    // App window on session 0 is invalid
    let app_on_session_zero = MirrorStart {
        max_size: 1920,
        fps: 60,
        bitrate: 8_000_000,
        audio: false,
        session: MirrorSessionId::SCREEN,
        app: Some("com.android.chrome".into()),
        stay_awake: None,
        screen_off: None,
        watch: None,
        sound_only: None,
    };
    assert!(app_on_session_zero.validate().is_err());
}

#[test]
fn test_stop_and_keyframe_default_session() {
    let json_stop = r#"{}"#;
    let stop: MirrorStop = serde_json::from_str(json_stop).unwrap();
    assert_eq!(stop.session, MirrorSessionId::SCREEN);

    let json_stop_app = r#"{"session": 3}"#;
    let stop_app: MirrorStop = serde_json::from_str(json_stop_app).unwrap();
    assert_eq!(stop_app.session, MirrorSessionId::new(3));

    let json_kf = r#"{}"#;
    let kf: MirrorKeyframe = serde_json::from_str(json_kf).unwrap();
    assert_eq!(kf.session, MirrorSessionId::SCREEN);
}

#[test]
fn test_resize_session_enforcement() {
    assert!(MirrorResize::new(MirrorSessionId::SCREEN, 1080, 1920).is_err());
    let resize = MirrorResize::new(MirrorSessionId::new(2), 1080, 1920).unwrap();
    assert_eq!(resize.session, MirrorSessionId::new(2));
    assert_eq!(resize.width, 1080);
    assert_eq!(resize.height, 1920);

    let serialized = serde_json::to_string(&resize).unwrap();
    let deserialized: MirrorResize = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, resize);
}

#[test]
fn test_power_and_sound_messages() {
    let power = MirrorPower {
        stay_awake: Some(true),
        screen_off: Some(false),
    };
    let json_power = serde_json::to_string(&power).unwrap();
    let decoded_power: MirrorPower = serde_json::from_str(&json_power).unwrap();
    assert_eq!(decoded_power, power);

    let sound = MirrorSound { on: true };
    let json_sound = serde_json::to_string(&sound).unwrap();
    let decoded_sound: MirrorSound = serde_json::from_str(&json_sound).unwrap();
    assert!(decoded_sound.on);
}

#[test]
fn test_apps_listing_message() {
    let apps_res = MirrorAppsResponse {
        b: AppListPayload {
            apps: vec![
                AppInfo {
                    pkg: "com.android.chrome".into(),
                    label: "Chrome".into(),
                    icon: Some(vec![0x89, 0x50, 0x4E, 0x47]),
                },
                AppInfo {
                    pkg: "org.videolan.vlc".into(),
                    label: "VLC".into(),
                    icon: None,
                },
            ],
        },
    };

    let serialized = serde_json::to_string(&apps_res).unwrap();
    let deserialized: MirrorAppsResponse = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.b.apps.len(), 2);
    assert_eq!(deserialized.b.apps[0].pkg, "com.android.chrome");
    assert_eq!(deserialized.b.apps[1].label, "VLC");
}

#[test]
fn test_input_events_serialization() {
    use tv_rs_mirror::input::*;

    // Touch
    let touch = MirrorInputMessage {
        session: MirrorSessionId::SCREEN,
        event: InputEvent::Touch(TouchInput {
            action: TouchAction::Down,
            x: 0.5,
            y: 0.5,
        }),
    };
    let json_touch = serde_json::to_string(&touch).unwrap();
    let decoded_touch: MirrorInputMessage = serde_json::from_str(&json_touch).unwrap();
    assert_eq!(decoded_touch, touch);

    // Scroll
    let scroll = MirrorInputMessage {
        session: MirrorSessionId::new(2),
        event: InputEvent::Scroll(ScrollInput {
            x: 0.1,
            y: 0.2,
            dx: 0.0,
            dy: -1.0,
        }),
    };
    let json_scroll = serde_json::to_string(&scroll).unwrap();
    let decoded_scroll: MirrorInputMessage = serde_json::from_str(&json_scroll).unwrap();
    assert_eq!(decoded_scroll, scroll);

    // Key
    let key = MirrorInputMessage {
        session: MirrorSessionId::SCREEN,
        event: InputEvent::Key {
            key: KeyAction::Back,
        },
    };
    let json_key = serde_json::to_string(&key).unwrap();
    let decoded_key: MirrorInputMessage = serde_json::from_str(&json_key).unwrap();
    assert_eq!(decoded_key, key);

    // Text
    let text = MirrorInputMessage {
        session: MirrorSessionId::SCREEN,
        event: InputEvent::Text {
            text: "Hello Digital Signage!".into(),
        },
    };
    let json_text = serde_json::to_string(&text).unwrap();
    let decoded_text: MirrorInputMessage = serde_json::from_str(&json_text).unwrap();
    assert_eq!(decoded_text, text);

    // Gamepad
    let gamepad = MirrorInputMessage {
        session: MirrorSessionId::SCREEN,
        event: InputEvent::Gamepad(GamepadInput {
            buttons: 0x1000,
            lx: 100,
            ly: -200,
            rx: 0,
            ry: 0,
            lt: 128,
            rt: 255,
        }),
    };
    let json_gamepad = serde_json::to_string(&gamepad).unwrap();
    let decoded_gamepad: MirrorInputMessage = serde_json::from_str(&json_gamepad).unwrap();
    assert_eq!(decoded_gamepad, gamepad);
}

#[test]
fn test_stream_packet_roundtrip() {
    let raw_data = vec![1, 2, 3, 4, 5, 6, 7, 8];
    let packet = StreamPacket::new(PacketKind::Keyframe, 123_456_789, raw_data.clone());
    let encoded = packet.encode().unwrap();

    assert_eq!(encoded.len(), 4 + 9 + 8);
    let decoded = StreamPacket::decode(&encoded).unwrap();
    assert_eq!(decoded.kind, PacketKind::Keyframe);
    assert_eq!(decoded.time_us, 123_456_789);
    assert_eq!(decoded.data.as_ref(), raw_data.as_slice());
}

#[test]
fn test_stream_packet_config_payloads() {
    // Video config
    let v_config = VideoConfigPayload {
        codec: VideoCodec::H265,
        width: 3840,
        height: 2160,
        session: Some(1),
    };
    let json_v = serde_json::to_string(&v_config).unwrap();
    let decoded_v: VideoConfigPayload = serde_json::from_str(&json_v).unwrap();
    assert_eq!(decoded_v, v_config);

    // Audio config
    let a_config = AudioConfigPayload {
        codec: AudioFormat::PcmS16le,
        rate: 48000,
        channels: 2,
    };
    let json_a = serde_json::to_string(&a_config).unwrap();
    let decoded_a: AudioConfigPayload = serde_json::from_str(&json_a).unwrap();
    assert_eq!(decoded_a, a_config);
}

#[test]
fn test_stream_packet_size_limit_and_errors() {
    let too_large = vec![0u8; MAX_PACKET_SIZE + 1];
    let packet = StreamPacket::new(PacketKind::Frame, 100, too_large);
    assert!(packet.encode().is_err());

    // Short buffer decode
    assert!(StreamPacket::decode(&[0x00, 0x00]).is_err());
    // Truncated body decode
    assert!(StreamPacket::decode(&[0x00, 0x00, 0x00, 0x05, 0x01]).is_err());
    // Unknown kind
    assert!(StreamPacket::decode(&[0x00, 0x00, 0x00, 0x09, 0xFF, 0, 0, 0, 0, 0, 0, 0, 1]).is_err());
}
