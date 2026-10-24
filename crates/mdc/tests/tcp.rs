//! Socket-level tests against a scripted MDC display.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
#[cfg(feature = "supporting")]
use tv_rs_core::PanelHours;
use tv_rs_core::{
    Capability, CommandId, DeviceDescriptor, DeviceHandle, DeviceId, DeviceOs, Orientation,
};
use tv_rs_mdc::{
    commands, DisplayId, InputSource, MdcClient, MdcDevice, MdcError, MdcOptions, MdcPower, Status,
};

fn response(display_id: u8, ack: bool, command: u8, data: &[u8]) -> Vec<u8> {
    let ack_byte = if ack { 0x41 } else { 0x4E };
    let mut body = vec![ack_byte, command];
    body.extend_from_slice(data);
    let mut frame = vec![0xAA, 0xFF, display_id, body.len() as u8];
    frame.extend_from_slice(&body);
    let checksum = frame[1..]
        .iter()
        .fold(0u16, |sum, byte| sum + u16::from(*byte))
        & 0xFF;
    frame.push(checksum as u8);
    frame
}

async fn read_request(socket: &mut TcpStream) -> Vec<u8> {
    let mut header = [0u8; 4];
    socket
        .read_exact(&mut header)
        .await
        .expect("request header");
    assert_eq!(header[0], 0xAA);
    let mut tail = vec![0u8; usize::from(header[3]) + 1];
    socket.read_exact(&mut tail).await.expect("request body");
    let mut frame = header.to_vec();
    frame.extend_from_slice(&tail);
    frame
}

async fn listener() -> (TcpListener, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("local address").port();
    (listener, port)
}

#[tokio::test]
async fn power_query_and_set_round_trip_over_tcp() {
    let (listener, port) = listener().await;
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        assert_eq!(
            read_request(&mut socket).await,
            vec![0xAA, 0x11, 0x01, 0x00, 0x12]
        );
        socket
            .write_all(&response(1, true, commands::POWER, &[1]))
            .await
            .expect("ack query");

        assert_eq!(
            read_request(&mut socket).await,
            vec![0xAA, 0x11, 0x01, 0x01, 0x00, 0x13]
        );
        socket
            .write_all(&response(1, true, commands::POWER, &[0]))
            .await
            .expect("ack set");
    });

    let client = MdcClient::with_options(
        "127.0.0.1",
        MdcOptions {
            port,
            timeout: Duration::from_secs(2),
            ..MdcOptions::default()
        },
    )
    .display(DisplayId::new(1).expect("display id"));

    assert_eq!(client.power().await.expect("power query"), MdcPower::On);
    client.set_power(MdcPower::Off).await.expect("power set");
    server.await.expect("server");
}

#[tokio::test]
async fn mdc_device_satisfies_conformance_suite_and_feature_gates_lfd_surfaces() {
    let client = MdcClient::new("127.0.0.1").display(DisplayId::new(1).expect("display id"));
    let descriptor = DeviceDescriptor::new(
        DeviceId::new("conformance-panel").expect("device id"),
        DeviceOs::SamsungMdc,
        "127.0.0.1".to_string(),
    );
    let device = MdcDevice::new(client, descriptor);

    let violations = tv_rs_core::conformance::run(&device).await;
    assert!(
        violations.is_empty(),
        "MDC device conformance violations: {violations:#?}"
    );

    #[cfg(feature = "supporting")]
    {
        assert!(
            device.capabilities().contains(Capability::LampOrPanelHours),
            "LampOrPanelHours must be advertised when supporting feature is enabled"
        );
        assert!(
            device.as_panel_hours().is_some(),
            "PanelHoursProvider must be present when supporting feature is enabled"
        );
        assert!(
            !device.capabilities().contains(Capability::ScreenCapture),
            "ScreenCapture must NEVER be advertised (no MDC command exists)"
        );
        assert!(
            device.as_screen_capturer().is_none(),
            "ScreenCapturer must NEVER be present for MDC"
        );
    }
}

#[tokio::test]
async fn wall_handles_share_one_socket() {
    let (listener, port) = listener().await;
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("first accept");

        assert_eq!(
            read_request(&mut socket).await,
            vec![0xAA, 0x11, 0x01, 0x00, 0x12]
        );
        socket
            .write_all(&response(1, true, commands::POWER, &[1]))
            .await
            .expect("ack power");

        assert_eq!(
            read_request(&mut socket).await,
            vec![0xAA, 0x12, 0x02, 0x00, 0x14]
        );
        socket
            .write_all(&response(2, true, commands::VOLUME, &[50]))
            .await
            .expect("ack volume");

        assert!(
            timeout(Duration::from_millis(250), listener.accept())
                .await
                .is_err(),
            "a second display opened a second connection"
        );
    });

    let wall = MdcClient::with_options(
        "127.0.0.1",
        MdcOptions {
            port,
            timeout: Duration::from_secs(2),
            ..MdcOptions::default()
        },
    );
    let left = wall.display(DisplayId::new(1).expect("left id"));
    let right = wall.display(DisplayId::new(2).expect("right id"));

    assert_eq!(left.power().await.expect("left power"), MdcPower::On);
    assert_eq!(right.volume().await.expect("right volume"), Some(50));
    server.await.expect("server");
}

#[tokio::test]
async fn status_and_brightness_are_typed() {
    let (listener, port) = listener().await;
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");

        assert_eq!(
            read_request(&mut socket).await,
            vec![0xAA, 0x00, 0x01, 0x00, 0x01]
        );
        socket
            .write_all(&response(
                1,
                true,
                commands::STATUS,
                &[1, 50, 0, 0x23, 1, 0, 0],
            ))
            .await
            .expect("ack status");

        assert_eq!(
            read_request(&mut socket).await,
            vec![0xAA, 0x25, 0x01, 0x00, 0x26]
        );
        socket
            .write_all(&response(1, true, commands::BRIGHTNESS, &[70]))
            .await
            .expect("ack brightness query");

        assert_eq!(
            read_request(&mut socket).await,
            vec![0xAA, 0x25, 0x01, 0x01, 0x46, 0x6D]
        );
        socket
            .write_all(&response(1, true, commands::BRIGHTNESS, &[70]))
            .await
            .expect("ack brightness set");
    });

    let client = MdcClient::with_options(
        "127.0.0.1",
        MdcOptions {
            port,
            timeout: Duration::from_secs(2),
            ..MdcOptions::default()
        },
    )
    .display(DisplayId::new(1).expect("display id"));

    let status: Status = client.status().await.expect("status");
    assert_eq!(status.power, MdcPower::On);
    assert_eq!(status.volume, Some(50));
    assert_eq!(status.muted, Some(false));
    assert_eq!(status.input, Some(InputSource::Hdmi2));

    assert_eq!(client.brightness().await.expect("brightness"), Some(70));
    client.set_brightness(70).await.expect("set brightness");
    server.await.expect("server");
}

#[tokio::test]
async fn a_nak_is_returned_and_keeps_the_connection_open() {
    let (listener, port) = listener().await;
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        assert_eq!(
            read_request(&mut socket).await,
            vec![0xAA, 0x11, 0x01, 0x00, 0x12]
        );
        socket
            .write_all(&response(1, false, commands::POWER, &[0x02]))
            .await
            .expect("nak");

        // The second request can only arrive if the client kept the socket.
        assert_eq!(
            read_request(&mut socket).await,
            vec![0xAA, 0x11, 0x01, 0x00, 0x12]
        );
        socket
            .write_all(&response(1, true, commands::POWER, &[1]))
            .await
            .expect("ack");
    });

    let client = MdcClient::with_options(
        "127.0.0.1",
        MdcOptions {
            port,
            timeout: Duration::from_secs(2),
            ..MdcOptions::default()
        },
    )
    .display(DisplayId::new(1).expect("display id"));

    let error = client.power().await.expect_err("expected NAK");
    assert!(matches!(
        error,
        MdcError::Nak {
            command: commands::POWER,
            code: 0x02
        }
    ));
    assert_eq!(client.power().await.expect("second query"), MdcPower::On);
    server.await.expect("server");
}

#[tokio::test]
async fn broadcast_requires_send_only() {
    let client = MdcClient::new("127.0.0.1").broadcast();
    let error = client.power().await.expect_err("broadcast must not wait");
    assert!(matches!(error, MdcError::BroadcastRequiresSendOnly));
}

/// The `DeviceHandle` adapter must speak the same wire as the client it wraps.
///
/// Conformance proves the traits agree with the capability set; this proves the bytes
/// are right — brightness read/set, reboot (power with the Reboot code) and the probe
/// (a status round trip) all over a real socket.
#[tokio::test]
async fn device_adapters_speak_the_same_wire() {
    let (listener, port) = listener().await;
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");

        // brightness()
        assert_eq!(
            read_request(&mut socket).await,
            vec![0xAA, commands::BRIGHTNESS, 0x01, 0x00, 0x26]
        );
        socket
            .write_all(&response(1, true, commands::BRIGHTNESS, &[42]))
            .await
            .expect("ack brightness query");

        // set_brightness(55) — 0x37 in the payload, checksum carries it.
        assert_eq!(
            read_request(&mut socket).await,
            vec![0xAA, commands::BRIGHTNESS, 0x01, 0x01, 0x37, 0x5E]
        );
        socket
            .write_all(&response(1, true, commands::BRIGHTNESS, &[55]))
            .await
            .expect("ack brightness set");

        // reboot() is power with the Reboot code, not a separate command.
        assert_eq!(
            read_request(&mut socket).await,
            vec![
                0xAA,
                commands::POWER,
                0x01,
                0x01,
                MdcPower::Reboot.code(),
                0x15
            ]
        );
        socket
            .write_all(&response(
                1,
                true,
                commands::POWER,
                &[MdcPower::Reboot.code()],
            ))
            .await
            .expect("ack reboot");

        // ping() is a status round trip.
        assert_eq!(
            read_request(&mut socket).await,
            vec![0xAA, commands::STATUS, 0x01, 0x00, 0x01]
        );
        socket
            .write_all(&response(
                1,
                true,
                commands::STATUS,
                &[1, 50, 0, 0x23, 1, 0, 0],
            ))
            .await
            .expect("ack status");
    });

    let client = MdcClient::with_options(
        "127.0.0.1",
        MdcOptions {
            port,
            timeout: Duration::from_secs(2),
            ..MdcOptions::default()
        },
    )
    .display(DisplayId::new(1).expect("display id"));

    let descriptor = DeviceDescriptor::new(
        DeviceId::new("lobby-panel").expect("device id"),
        DeviceOs::SamsungMdc,
        "127.0.0.1".to_string(),
    );
    let device = MdcDevice::new(client, descriptor);

    assert!(device.capabilities().contains(Capability::Brightness));
    assert!(device.capabilities().contains(Capability::Reboot));
    assert!(
        !device.capabilities().contains(Capability::Telemetry),
        "the aggregate telemetry trait stays unadvertised; MDC's health data is exposed as panel health instead"
    );
    assert!(
        device.as_telemetry().is_none(),
        "a backend that cannot measure must not claim a provider"
    );

    let brightness = device.as_brightness().expect("brightness adapter");
    assert_eq!(
        brightness.brightness().await.expect("read brightness"),
        Some(42)
    );
    brightness
        .set_brightness(55, CommandId::new())
        .await
        .expect("set brightness");

    device
        .as_reboot()
        .expect("reboot adapter")
        .reboot(CommandId::new())
        .await
        .expect("reboot");

    let rtt = device
        .as_probe()
        .expect("probe adapter")
        .ping()
        .await
        .expect("ping");
    assert!(
        rtt > Duration::ZERO,
        "a round trip that took no time is not a measurement"
    );

    server.await.expect("server");
}

/// The LFD subset must speak the same wire the handoff documented.
///
/// Exact frames for `0x5F` (panel lock read/set), `0xC8`/`0x82` (orientation
/// read/set), and the `0x0D` health parse including the `0x02` invalid-selection
/// case. Screen capture is asserted *absent* — no MDC command exists for it.
#[tokio::test]
async fn the_lfd_subset_speaks_the_documented_wire() {
    let (listener, port) = listener().await;
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");

        // panel_lock() — query 0x5F. Checksum = command + id + length.
        assert_eq!(
            read_request(&mut socket).await,
            vec![0xAA, 0x5F, 0x01, 0x00, 0x60]
        );
        socket
            .write_all(&response(1, true, 0x5F, &[0x00]))
            .await
            .expect("ack panel lock query");

        // set_panel_lock(true).
        assert_eq!(
            read_request(&mut socket).await,
            vec![0xAA, 0x5F, 0x01, 0x01, 0x01, 0x62]
        );
        socket
            .write_all(&response(1, true, 0x5F, &[0x01]))
            .await
            .expect("ack panel lock set");

        // orientation() — 0xC8 subcommand 0x82.
        assert_eq!(
            read_request(&mut socket).await,
            vec![0xAA, 0xC8, 0x01, 0x01, 0x82, 0x4C]
        );
        socket
            .write_all(&response(1, true, 0xC8, &[0x03]))
            .await
            .expect("ack orientation query");

        // set_orientation(Portrait) — code 0x03.
        assert_eq!(
            read_request(&mut socket).await,
            vec![0xAA, 0xC8, 0x01, 0x02, 0x82, 0x03, 0x50]
        );
        socket
            .write_all(&response(1, true, 0xC8, &[0x82, 0x03]))
            .await
            .expect("ack orientation set");

        // panel_health() — 0x0D: lamp ok, temperature ok, input-source ok,
        // brightness-sensor slot, temperature 38 °C, fan ok.
        assert_eq!(
            read_request(&mut socket).await,
            vec![0xAA, 0x0D, 0x01, 0x00, 0x0E]
        );
        socket
            .write_all(&response(
                1,
                true,
                0x0D,
                &[0x00, 0x00, 0x00, 0x00, 38, 0x00],
            ))
            .await
            .expect("ack health ok");

        // Second query: lamp fault and input-source 0x02 (invalid selection),
        // which is a signal problem — nothing is being displayed.
        assert_eq!(
            read_request(&mut socket).await,
            vec![0xAA, 0x0D, 0x01, 0x00, 0x0E]
        );
        socket
            .write_all(&response(
                1,
                true,
                0x0D,
                &[0x01, 0x00, 0x00, 0x02, 41, 0x00],
            ))
            .await
            .expect("ack health faulted");
    });

    let client = MdcClient::with_options(
        "127.0.0.1",
        MdcOptions {
            port,
            timeout: Duration::from_secs(2),
            ..MdcOptions::default()
        },
    )
    .display(DisplayId::new(1).expect("display id"));
    let descriptor = DeviceDescriptor::new(
        DeviceId::new("gallery-panel").expect("device id"),
        DeviceOs::SamsungMdc,
        "127.0.0.1".to_string(),
    );
    let device = MdcDevice::new(client, descriptor);

    assert!(device.capabilities().contains(Capability::PanelLock));
    assert!(device.capabilities().contains(Capability::Orientation));
    assert!(device.capabilities().contains(Capability::NoSignalStatus));
    #[cfg(feature = "supporting")]
    assert!(
        !device.capabilities().contains(Capability::ScreenCapture),
        "no MDC command exists for screen capture; advertising it would be a lie"
    );

    let lock = device.as_panel_lock().expect("panel lock adapter");
    assert_eq!(
        lock.panel_lock().await.expect("read panel lock"),
        Some(false)
    );
    lock.set_panel_lock(true, CommandId::new())
        .await
        .expect("set panel lock");

    let orientation = device.as_orientation().expect("orientation adapter");
    assert_eq!(
        orientation.orientation().await.expect("read orientation"),
        Some(Orientation::Portrait)
    );
    assert!(Orientation::Portrait.is_portrait());
    orientation
        .set_orientation(Orientation::Portrait, CommandId::new())
        .await
        .expect("set orientation");

    let health = device.as_panel_health().expect("health adapter");
    assert_eq!(
        health.panel_health().await.expect("read health"),
        Some(tv_rs_core::PanelHealth {
            signal_present: true,
            temperature_celsius: Some(38),
            lamp_error: false,
        })
    );
    assert_eq!(
        health.panel_health().await.expect("read health again"),
        Some(tv_rs_core::PanelHealth {
            signal_present: false,
            temperature_celsius: Some(41),
            lamp_error: true,
        })
    );

    server.await.expect("server");
}

/// `0x83` reports panel on-time in units of ten minutes; the hours conversion
/// belongs to the adapter, not to the caller. Gated behind `supporting`.
#[cfg(feature = "supporting")]
#[tokio::test]
async fn panel_hours_are_converted_from_ten_minute_units() {
    let (listener, port) = listener().await;
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        // 0x83 on-time query. Checksum = command + id + length.
        assert_eq!(
            read_request(&mut socket).await,
            vec![0xAA, 0x83, 0x01, 0x00, 0x84]
        );
        // 600 ten-minute units = 100 hours, encoded big-endian on the wire.
        socket
            .write_all(&response(1, true, 0x83, &[0x02, 0x58]))
            .await
            .expect("ack panel on time");
    });

    let client = MdcClient::with_options(
        "127.0.0.1",
        MdcOptions {
            port,
            timeout: Duration::from_secs(2),
            ..MdcOptions::default()
        },
    )
    .display(DisplayId::new(1).expect("display id"));
    let descriptor = DeviceDescriptor::new(
        DeviceId::new("depot-panel").expect("device id"),
        DeviceOs::SamsungMdc,
        "127.0.0.1".to_string(),
    );
    let device = MdcDevice::new(client, descriptor);

    assert!(device.capabilities().contains(Capability::LampOrPanelHours));
    assert_eq!(
        device
            .as_panel_hours()
            .expect("panel hours adapter")
            .panel_hours()
            .await
            .expect("read panel hours"),
        Some(PanelHours {
            panel_hours: 100,
            lamp_hours: None,
        })
    );

    server.await.expect("server");
}
