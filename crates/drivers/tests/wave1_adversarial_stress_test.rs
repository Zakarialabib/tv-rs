//! Wave 1 Adversarial Challenge & Stress Test Suite
//!
//! Thoroughly exercises protocol edge cases, checksum corruptions, packet framing,
//! dispatch mechanics, and error mappings for MDC, SICP, IR, and Roku.

#![cfg(all(feature = "mdc", feature = "sicp", feature = "ir", feature = "roku"))]

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use tv_rs_core::capability::{Capability, Key};
use tv_rs_core::command::Command;
use tv_rs_core::descriptor::DiscoveredTv;
use tv_rs_core::driver::Driver;
use tv_rs_core::error::{AuthError, ProtocolError, TransportError, TvError};
use tv_rs_core::types::TvVendor;

// MDC imports
use tv_rs_drivers::mdc::commands as mdc_commands;
use tv_rs_drivers::mdc::driver::MdcDriver;
use tv_rs_drivers::mdc::protocol::{Frame as MdcFrame, Response as MdcResponse};
use tv_rs_drivers::mdc::types::DisplayId as MdcDisplayId;
use tv_rs_drivers::mdc::MdcError;

// SICP imports
use tv_rs_drivers::sicp::commands as sicp_commands;
use tv_rs_drivers::sicp::control_types as sicp_controls;
use tv_rs_drivers::sicp::error::SicpError;
use tv_rs_drivers::sicp::protocol::{Frame as SicpFrame, Response as SicpResponse};
use tv_rs_drivers::sicp::types::DisplayId as SicpDisplayId;

// IR imports
use tv_rs_drivers::ir::driver::IrDriver;
use tv_rs_drivers::ir::emitter::{IrEmitter, MockIrEmitter};
use tv_rs_drivers::ir::error::IrError;
use tv_rs_drivers::ir::protocol::{IrProtocol, IrSignal};

// Roku imports
use tv_rs_drivers::roku::{
    keypress_path, launch_path, map_core_key_to_roku, RokuClient, RokuError, RokuKey,
};

// =============================================================================
// 1. MDC PROTOCOL STRESS TESTS
// =============================================================================

#[test]
fn test_mdc_video_wall_opcode_0x84_and_packet_construction() {
    // Assert constant opcode is exactly 0x84
    assert_eq!(mdc_commands::VIDEO_WALL_CONTROL, 0x84);

    let display = MdcDisplayId::new(1).expect("valid display id 1");
    // Video wall parameters: enable=1, div_x=2, div_y=2, pos=3
    let payload = vec![1, 2, 2, 3];
    let frame = MdcFrame::new(mdc_commands::VIDEO_WALL_CONTROL, display, payload);

    // Checksum formula: low byte of sum(command + display_id + len + data...)
    // 0x84 (132) + 0x01 (1) + 4 + 1 + 2 + 2 + 3 = 145 = 0x91
    assert_eq!(frame.checksum(), 0x91);

    let encoded = frame.encode().expect("frame encoding succeeds");
    assert_eq!(
        encoded,
        vec![0xAA, 0x84, 0x01, 0x04, 0x01, 0x02, 0x02, 0x03, 0x91]
    );
}

#[test]
fn test_mdc_checksum_tampering_and_framing_rejection() {
    let display = MdcDisplayId::new(5).expect("valid display id 5");
    let frame = MdcFrame::new(mdc_commands::POWER, display, vec![1]);
    let mut encoded = frame.encode().expect("encoded");

    // Intentionally corrupt the checksum byte
    let last_idx = encoded.len() - 1;
    encoded[last_idx] ^= 0xFF;

    // Response::parse expects response format: [0xAA, 0xFF, display_id, length, ACK/NAK, cmd, data..., checksum]
    let mut resp_bytes = vec![0xAA, 0xFF, 0x05, 0x03, 0x41, mdc_commands::POWER, 0x01];
    let correct_chk = resp_bytes[1..]
        .iter()
        .fold(0u16, |acc, &b| acc + u16::from(b))
        & 0xFF;
    resp_bytes.push(correct_chk as u8);

    // Valid response passes
    let parsed = MdcResponse::parse(&resp_bytes).expect("valid response parses");
    assert!(parsed.acknowledged);
    assert_eq!(parsed.display_id.value(), 5);

    // Corrupt checksum fails
    let mut corrupted_chk = resp_bytes.clone();
    let chk_pos = corrupted_chk.len() - 1;
    corrupted_chk[chk_pos] = corrupted_chk[chk_pos].wrapping_add(1);
    let err = MdcResponse::parse(&corrupted_chk).expect_err("tampered checksum must fail");
    assert!(matches!(err, MdcError::Protocol(_)));

    // Corrupt header (not 0xAA) fails
    let mut corrupted_hdr = resp_bytes.clone();
    corrupted_hdr[0] = 0xBB;
    let err_hdr = MdcResponse::parse(&corrupted_hdr).expect_err("invalid header must fail");
    assert!(matches!(err_hdr, MdcError::Protocol(_)));

    // Truncated buffer fails
    let err_trunc = MdcResponse::parse(&resp_bytes[..4]).expect_err("truncated frame must fail");
    assert!(matches!(err_trunc, MdcError::Protocol(_)));
}

#[tokio::test]
async fn test_mdc_display_multiplexing_and_video_wall_dispatch() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind listener");
    let port = listener.local_addr().expect("port").port();

    let server_handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept client");

        // 1. First command: initial default display_id is DIRECT (0).
        // Request format: [0xAA, 0x84, 0x00, 0x04, 0x01, 0x02, 0x02, 0x03, 0x90]
        let mut buf = [0u8; 9];
        socket.read_exact(&mut buf).await.expect("read video wall request");
        assert_eq!(buf, [0xAA, 0x84, 0x00, 0x04, 0x01, 0x02, 0x02, 0x03, 0x90]);

        // Send ACK for display 0
        // [0xAA, 0xFF, 0x00, 0x03, 0x41, 0x84, 0x00, checksum]
        let mut ack1 = vec![0xAA, 0xFF, 0x00, 0x03, 0x41, 0x84, 0x00];
        let chk1 = (ack1[1..].iter().fold(0u16, |s, &b| s + u16::from(b)) & 0xFF) as u8;
        ack1.push(chk1);
        socket.write_all(&ack1).await.expect("send ack1");

        // 2. Second command: after driver switches display_id to 2 on SAME socket (multiplexed):
        // power_on() sends: [0xAA, 0x11, 0x02, 0x01, 0x01, 0x15]
        let mut pwr_buf = [0u8; 6];
        socket.read_exact(&mut pwr_buf).await.expect("read display 2 power request");
        assert_eq!(pwr_buf, [0xAA, 0x11, 0x02, 0x01, 0x01, 0x15]);

        // Send ACK for display 2 power_on
        let mut ack2 = vec![0xAA, 0xFF, 0x02, 0x03, 0x41, 0x11, 0x01];
        let chk2 = (ack2[1..].iter().fold(0u16, |s, &b| s + u16::from(b)) & 0xFF) as u8;
        ack2.push(chk2);
        socket.write_all(&ack2).await.expect("send ack2");
    });

    let mut driver = MdcDriver::new();
    let target = DiscoveredTv::probed(TvVendor::Mdc, format!("127.0.0.1:{port}"));
    let _handle = driver.connect(&target).await.expect("driver connects");

    // Send custom video_wall command to display 0
    let ack_vw = driver
        .send(Command::Custom {
            name: "video_wall".to_string(),
            payload: "1,2,2,3".to_string(),
        })
        .await
        .expect("video wall command succeeds");
    assert!(ack_vw.id.is_some());

    // Switch display_id to 2 on same multiplexed TCP connection
    let ack_switch = driver
        .send(Command::Custom {
            name: "display_id".to_string(),
            payload: "2".to_string(),
        })
        .await
        .expect("switch display_id succeeds");
    assert!(ack_switch.id.is_some());

    // Send power command to display 2
    let ack_pwr = driver.send(Command::Power(true)).await.expect("power query succeeds");
    assert!(ack_pwr.id.is_some());

    server_handle.await.expect("server completed cleanly");
}

#[tokio::test]
async fn test_mdc_video_wall_invalid_payload_error() {
    let mut driver = MdcDriver::new();
    let target = DiscoveredTv::probed(TvVendor::Mdc, "127.0.0.1");
    let _handle = driver.connect(&target).await.expect("connect");

    // Invalid payload formats (not 4 comma-separated values)
    let res = driver
        .send(Command::Custom {
            name: "video_wall".to_string(),
            payload: "1,2".to_string(),
        })
        .await;
    assert!(res.is_err());
    if let Err(TvError::Protocol(ProtocolError::Malformed(msg))) = res {
        assert!(msg.contains("video_wall payload format: enable,div_x,div_y,pos"));
    } else {
        panic!("Expected Malformed protocol error, got: {:?}", res);
    }
}

// =============================================================================
// 2. SICP PROTOCOL STRESS TESTS
// =============================================================================

#[test]
fn test_sicp_framing_and_xor_checksum_validation() {
    let display = SicpDisplayId::new(1).expect("display 1");
    let frame = SicpFrame::new(
        display,
        sicp_controls::SET,
        sicp_commands::POWER,
        vec![0x01],
    );

    // Frame size: 5 + 1 = 6
    // Checksum = 6 ^ 1 ^ 0x00 ^ 0x18 ^ 0x01 = 0x1E
    assert_eq!(frame.checksum(), 0x1E);

    let encoded = frame.encode().expect("encode");
    assert_eq!(encoded, vec![0x06, 0x01, 0x00, 0x18, 0x01, 0x1E]);

    // Parse valid response
    let valid_resp = vec![0x06, 0x01, sicp_controls::REPLY, sicp_commands::POWER, 0x01, 0x1C];
    let parsed = SicpResponse::parse(&valid_resp).expect("valid sicp response parses");
    assert!(parsed.acknowledged);
    assert_eq!(parsed.display_id.value(), 1);
    assert_eq!(parsed.command, sicp_commands::POWER);
    assert_eq!(parsed.data, vec![0x01]);

    // Corrupt XOR checksum
    let mut corrupted = valid_resp.clone();
    let chk_idx = corrupted.len() - 1;
    corrupted[chk_idx] ^= 0x55;
    let err = SicpResponse::parse(&corrupted).expect_err("corrupted checksum must fail");
    assert!(matches!(err, SicpError::Protocol(_)));

    // Truncated buffer
    let err_len = SicpResponse::parse(&valid_resp[..3]).expect_err("too short buffer fails");
    assert!(matches!(err_len, SicpError::Protocol(_)));

    // Declared size mismatch
    let mut size_mismatch = valid_resp.clone();
    size_mismatch[0] = 0x0A;
    let err_size = SicpResponse::parse(&size_mismatch).expect_err("declared size mismatch fails");
    assert!(matches!(err_size, SicpError::Protocol(_)));
}

#[test]
fn test_sicp_nak_error_mapping_to_tv_error() {
    // 6 ^ 1 ^ 0x03 (NAK) ^ 0x18 (POWER) ^ 0x02 = 0x1E
    let nak_resp = vec![0x06, 0x01, sicp_controls::NAK, sicp_commands::POWER, 0x02, 0x1E];
    let parsed = SicpResponse::parse(&nak_resp).expect("nak response parses");
    assert!(!parsed.acknowledged);
    assert_eq!(parsed.control_type, sicp_controls::NAK);

    let sicp_err = SicpError::Nak {
        command: parsed.command,
        code: parsed.data[0],
    };
    let mapped = tv_rs_drivers::map::sicp::to_tv_error(&sicp_err);

    match mapped {
        TvError::Protocol(ProtocolError::Unexpected(msg)) => {
            assert!(msg.contains("display returned NAK 0x02 for command 0x18"));
        }
        other => panic!("Expected ProtocolError::Unexpected, got: {:?}", other),
    }
}

// =============================================================================
// 3. IR PROTOCOL & DISPATCH STRESS TESTS
// =============================================================================

#[tokio::test]
async fn test_ir_emitter_arc_dyn_dispatch_and_error_mapping() {
    let mock = Arc::new(MockIrEmitter::new());
    let dyn_emitter: Arc<dyn IrEmitter> = mock.clone();

    // Verify IrEmitter trait implementation for Arc<dyn IrEmitter>
    let test_signal = IrSignal::encode(IrProtocol::Nec, 0x07, 0x02, None).expect("signal encode");
    dyn_emitter.emit(&test_signal).await.expect("emit via Arc<dyn IrEmitter>");

    assert_eq!(mock.emitted_signals().len(), 1);
    assert_eq!(mock.last_emitted().expect("last emitted"), test_signal);

    // Test IrDriver with injected Arc<dyn IrEmitter>
    let mut driver = IrDriver::with_emitter(dyn_emitter);
    let mut target = DiscoveredTv::probed(TvVendor::Ir, "192.168.1.100");
    target.friendly_name = Some("Bravia-IR".to_string());
    let _handle = driver.connect(&target).await.expect("driver connects");

    // Send power on key
    let ack = driver.send(Command::Power(true)).await.expect("send power succeeds");
    assert!(ack.id.is_some());
    assert_eq!(mock.emitted_signals().len(), 2);

    // Verify IR error mappings
    let err_emitter = IrError::Emitter("hardware disconnected".to_string());
    assert!(matches!(
        tv_rs_drivers::map::ir::to_tv_error(&err_emitter),
        TvError::Transport(TransportError::Unreachable(_))
    ));

    let err_proto = IrError::Protocol("bad carrier frequency".to_string());
    assert!(matches!(
        tv_rs_drivers::map::ir::to_tv_error(&err_proto),
        TvError::Protocol(ProtocolError::Malformed(_))
    ));

    let err_scan = IrError::ScanFailed("no match found".to_string());
    assert!(matches!(
        tv_rs_drivers::map::ir::to_tv_error(&err_scan),
        TvError::Protocol(ProtocolError::Unexpected(_))
    ));
}

// =============================================================================
// 4. ROKU PROTOCOL & ENDPOINT STRESS TESTS
// =============================================================================

#[test]
fn test_roku_dpad_key_mapping_and_unsupported_rejection() {
    // D-pad translation assertions
    assert_eq!(map_core_key_to_roku(Key::Up), Some(RokuKey::Up));
    assert_eq!(map_core_key_to_roku(Key::Down), Some(RokuKey::Down));
    assert_eq!(map_core_key_to_roku(Key::Left), Some(RokuKey::Left));
    assert_eq!(map_core_key_to_roku(Key::Right), Some(RokuKey::Right));
    assert_eq!(map_core_key_to_roku(Key::Select), Some(RokuKey::Select));
    assert_eq!(map_core_key_to_roku(Key::Enter), Some(RokuKey::Enter));
    assert_eq!(map_core_key_to_roku(Key::Back), Some(RokuKey::Back));
    assert_eq!(map_core_key_to_roku(Key::Home), Some(RokuKey::Home));
    assert_eq!(map_core_key_to_roku(Key::Play), Some(RokuKey::Play));
    assert_eq!(map_core_key_to_roku(Key::Pause), Some(RokuKey::Play));
    assert_eq!(map_core_key_to_roku(Key::VolumeUp), Some(RokuKey::VolumeUp));
    assert_eq!(map_core_key_to_roku(Key::VolumeDown), Some(RokuKey::VolumeDown));
    assert_eq!(map_core_key_to_roku(Key::Mute), Some(RokuKey::VolumeMute));
    assert_eq!(map_core_key_to_roku(Key::PowerOff), Some(RokuKey::PowerOff));
    assert_eq!(map_core_key_to_roku(Key::Power), Some(RokuKey::PowerOn));

    // Unsupported key variants return None
    assert_eq!(map_core_key_to_roku(Key::Stop), None);
    assert_eq!(map_core_key_to_roku(Key::Next), None);
    assert_eq!(map_core_key_to_roku(Key::Digit(5)), None);
}

#[test]
fn test_roku_url_endpoint_paths_and_error_mapping() {
    assert_eq!(keypress_path(RokuKey::Home), "/keypress/Home");
    assert_eq!(keypress_path(RokuKey::Select), "/keypress/Select");
    assert_eq!(keypress_path(RokuKey::Up), "/keypress/Up");
    assert_eq!(keypress_path(RokuKey::PowerOff), "/keypress/PowerOff");
    assert_eq!(launch_path("12345"), "/launch/12345");
    assert_eq!(launch_path("tvinput.hdmi1"), "/launch/tvinput.hdmi1");

    let client = RokuClient::new("192.168.1.50");
    assert_eq!(client.base_url(), "http://192.168.1.50:8060");

    let client_custom = RokuClient::with_base_url("http://192.168.1.50:8060/");
    assert_eq!(client_custom.base_url(), "http://192.168.1.50:8060/");

    // Roku error mappings
    let err_unsupported = RokuError::Unsupported(tv_rs_transport::Unsupported::new(2, "REA gap"));
    assert!(matches!(
        tv_rs_drivers::map::roku::to_tv_error(&err_unsupported, Capability::Key),
        TvError::Unsupported(Capability::Key)
    ));

    let err_perm = RokuError::NotPermitted {
        remedy: "Enable Control by Mobile Apps in Settings".to_string(),
    };
    match tv_rs_drivers::map::roku::to_tv_error(&err_perm, Capability::Key) {
        TvError::Auth(AuthError::NotPermitted { remedy }) => {
            assert!(remedy.contains("Enable Control by Mobile Apps"));
        }
        other => panic!("Expected AuthError::NotPermitted, got: {:?}", other),
    }

    let err_xml = RokuError::Xml("Unexpected closing tag".to_string());
    assert!(matches!(
        tv_rs_drivers::map::roku::to_tv_error(&err_xml, Capability::Key),
        TvError::Protocol(ProtocolError::Malformed(_))
    ));
}
