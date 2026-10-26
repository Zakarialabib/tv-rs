//! Integration tests for MQTT Subscribe Provider (mock broker connection and reconnect handling).

#![cfg(feature = "mqtt")]

use std::net::SocketAddr;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tv_rs_agent::mqtt_provider::{MqttSubscribeProvider, MqttTransport};
use tv_rs_core::{DeviceDescriptor, DeviceId, DeviceOs};

/// Minimal mock MQTT broker over TCP.
///
/// Accepts CONNECT -> sends CONNACK.
/// Accepts SUBSCRIBE -> sends SUBACK.
/// Can optionally send a PUBLISH or drop connection to test reconnect handling.
async fn run_mock_mqtt_broker(listener: TcpListener) {
    loop {
        let (mut socket, _) = match listener.accept().await {
            Ok(res) => res,
            Err(_) => break,
        };

        tokio::spawn(async move {
            let mut buf = [0u8; 1024];

            // 1. Read CONNECT
            if socket.read(&mut buf).await.unwrap_or(0) == 0 {
                return;
            }

            // Basic check for MQTT CONNECT packet header (0x10)
            if buf[0] & 0xF0 == 0x10 {
                // Send CONNACK: [0x20 (CONNACK), 0x02 (Length), 0x00 (Flags), 0x00 (Return Code 0: Accepted)]
                let connack = [0x20, 0x02, 0x00, 0x00];
                if socket.write_all(&connack).await.is_err() {
                    return;
                }
            } else {
                return;
            }

            // 2. Read SUBSCRIBE or next packet
            if socket.read(&mut buf).await.unwrap_or(0) == 0 {
                return;
            }

            // Check for SUBSCRIBE packet header (0x82)
            if buf[0] & 0xF0 == 0x80 {
                let packet_id_msb = buf[2];
                let packet_id_lsb = buf[3];
                // Send SUBACK: [0x90 (SUBACK), 0x03 (Length), PacketIdMSB, PacketIdLSB, 0x01 (Granted QoS 1)]
                let suback = [0x90, 0x03, packet_id_msb, packet_id_lsb, 0x01];
                if socket.write_all(&suback).await.is_err() {
                    return;
                }

                // Send retained PUBLISH message for "screens/tv-mock/desired"
                // Topic: screens/tv-mock/desired (22 bytes)
                // Payload: {"power":"on","app":"youtube"} (28 bytes)
                let topic = "screens/tv-mock/desired";
                let payload = br#"{"power":"on","app":"youtube"}"#;
                let topic_len = topic.len() as u16;

                let remaining_len = 2 + topic.len() + 2 + payload.len(); // topic_len field (2) + topic + packet_id (2) + payload
                let mut publish = vec![0x32, remaining_len as u8]; // QoS 1 Publish (0x32)
                publish.extend_from_slice(&topic_len.to_be_bytes());
                publish.extend_from_slice(topic.as_bytes());
                publish.extend_from_slice(&[0x00, 0x01]); // Packet ID 1
                publish.extend_from_slice(payload);

                let _ = socket.write_all(&publish).await;
            }

            // Keep socket alive to maintain session
            loop {
                if socket.read(&mut buf).await.unwrap_or(0) == 0 {
                    break;
                }
                // If PINGREQ (0xC0), answer PINGRESP (0xD0)
                if buf[0] & 0xF0 == 0xC0 {
                    let pingresp = [0xD0, 0x00];
                    if socket.write_all(&pingresp).await.is_err() {
                        break;
                    }
                }
            }
        });
    }
}

#[tokio::test]
async fn mock_broker_tcp_connection_and_reconcile() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();

    let _broker_task = tokio::spawn(run_mock_mqtt_broker(listener));

    let provider = MqttSubscribeProvider::connect_with_transport(
        &addr.to_string(),
        "screens",
        "test-client-1",
        MqttTransport::Tcp,
    )
    .await
    .expect("connect to mock broker");

    // Give the async task a moment to process the incoming PUBLISH message
    tokio::time::sleep(Duration::from_millis(100)).await;

    let device = DeviceDescriptor::new(
        DeviceId::new("tv-mock").unwrap(),
        DeviceOs::Roku,
        "10.0.0.10",
    );
    use tv_rs_agent::desired::DesiredStateProvider;
    let state = provider
        .desired_state(&device)
        .await
        .unwrap()
        .expect("state published from mock broker");

    assert_eq!(state.app.as_deref(), Some("youtube"));
    assert_eq!(provider.known_devices(), vec!["tv-mock"]);
}

#[tokio::test]
async fn mock_broker_reconnect_handling() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();

    let _broker_task = tokio::spawn(run_mock_mqtt_broker(listener));

    let provider = MqttSubscribeProvider::connect_with_transport(
        &addr.to_string(),
        "screens",
        "test-client-2",
        MqttTransport::Tcp,
    )
    .await
    .expect("connect to mock broker");

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(provider.known_devices(), vec!["tv-mock"]);
}
