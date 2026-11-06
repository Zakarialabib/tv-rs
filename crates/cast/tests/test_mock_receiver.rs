//! Integration tests verifying CastClient transport and media controls against a mock receiver.

use bytes::{BufMut, BytesMut};
use prost::Message;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;
use tv_rs_cast::proto::cast_channel::{cast_message, CastMessage};
use tv_rs_cast::CastClient;

fn generate_mock_tls_config() -> ServerConfig {
    let key_pair = rcgen::KeyPair::generate().expect("key pair");
    let params = rcgen::CertificateParams::new(vec!["localhost".into(), "127.0.0.1".into()])
        .expect("cert params");
    let cert = params.self_signed(&key_pair).expect("cert");

    let cert_der = CertificateDer::from(cert.der().to_vec());
    let key_der = PrivateKeyDer::Pkcs8(key_pair.serialize_der().into());

    ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der)
        .expect("server config")
}

async fn read_framed_message<R: AsyncReadExt + Unpin>(reader: &mut R) -> CastMessage {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await.expect("read len");
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload).await.expect("read payload");
    CastMessage::decode(&payload[..]).expect("decode cast message")
}

async fn write_framed_message<W: AsyncWriteExt + Unpin>(writer: &mut W, msg: &CastMessage) {
    let mut payload = BytesMut::new();
    msg.encode(&mut payload).expect("encode");
    let mut buf = BytesMut::with_capacity(4 + payload.len());
    buf.put_u32(payload.len() as u32);
    buf.put_slice(&payload);
    writer.write_all(&buf).await.expect("write");
    writer.flush().await.expect("flush");
}

#[tokio::test]
async fn test_cast_client_mock_receiver_media_controls() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let tls_config = Arc::new(generate_mock_tls_config());
    let acceptor = TlsAcceptor::from(tls_config);

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut tls_stream = acceptor.accept(stream).await.expect("tls accept");

        // 1. Initial connect message
        let connect_msg = read_framed_message(&mut tls_stream).await;
        assert_eq!(
            connect_msg.namespace,
            "urn:x-cast:com.google.cast.tp.connection"
        );

        // 2. Client calls media_status() -> GET_STATUS
        let status_req_msg = read_framed_message(&mut tls_stream).await;
        assert_eq!(status_req_msg.namespace, "urn:x-cast:com.google.cast.media");
        let req_json: serde_json::Value =
            serde_json::from_str(&status_req_msg.payload_utf8.unwrap()).unwrap();
        assert_eq!(req_json["type"], "GET_STATUS");

        // Send MEDIA_STATUS reply
        let media_status_reply = serde_json::json!({
            "type": "MEDIA_STATUS",
            "requestId": req_json["requestId"],
            "status": [{
                "mediaSessionId": 42,
                "playerState": "PLAYING",
                "currentTime": 15.5,
                "media": {
                    "contentId": "https://cdn.tv-rs.test/trailer.mp4",
                    "contentType": "video/mp4",
                    "streamType": "BUFFERED",
                    "duration": 180.0
                }
            }]
        });

        let reply_msg = CastMessage {
            protocol_version: cast_message::ProtocolVersion::Castv210 as i32,
            source_id: "receiver-0".to_string(),
            destination_id: status_req_msg.source_id.clone(),
            namespace: "urn:x-cast:com.google.cast.media".to_string(),
            payload_type: cast_message::PayloadType::String as i32,
            payload_utf8: Some(media_status_reply.to_string()),
            payload_binary: None,
        };
        write_framed_message(&mut tls_stream, &reply_msg).await;

        // 3. Client calls pause()
        let pause_req_msg = read_framed_message(&mut tls_stream).await;
        let pause_json: serde_json::Value =
            serde_json::from_str(&pause_req_msg.payload_utf8.unwrap()).unwrap();
        assert_eq!(pause_json["type"], "PAUSE");
        assert_eq!(pause_json["mediaSessionId"], 42);

        // 4. Client calls play()
        let play_req_msg = read_framed_message(&mut tls_stream).await;
        let play_json: serde_json::Value =
            serde_json::from_str(&play_req_msg.payload_utf8.unwrap()).unwrap();
        assert_eq!(play_json["type"], "PLAY");
        assert_eq!(play_json["mediaSessionId"], 42);

        // 5. Client calls seek(42, 60.0)
        let seek_req_msg = read_framed_message(&mut tls_stream).await;
        let seek_json: serde_json::Value =
            serde_json::from_str(&seek_req_msg.payload_utf8.unwrap()).unwrap();
        assert_eq!(seek_json["type"], "SEEK");
        assert_eq!(seek_json["mediaSessionId"], 42);
        assert_eq!(seek_json["currentTime"], 60.0);

        // 6. Client calls set_volume(0.8)
        let vol_req_msg = read_framed_message(&mut tls_stream).await;
        assert_eq!(vol_req_msg.namespace, "urn:x-cast:com.google.cast.receiver");
        let vol_json: serde_json::Value =
            serde_json::from_str(&vol_req_msg.payload_utf8.unwrap()).unwrap();
        assert_eq!(vol_json["type"], "SET_VOLUME");
        assert_eq!(vol_json["volume"]["level"], 0.8);
    });

    let mut client = CastClient::connect_port("127.0.0.1", port)
        .await
        .expect("client connect");

    let status = client
        .media_status("transport-session-1")
        .await
        .expect("media status");
    assert_eq!(status.state, "PLAYING");
    assert_eq!(status.position, 15.5);
    assert_eq!(status.duration, Some(180.0));
    assert_eq!(
        status.track.as_deref(),
        Some("https://cdn.tv-rs.test/trailer.mp4")
    );

    client
        .pause("transport-session-1", 42)
        .await
        .expect("pause");
    client.play("transport-session-1", 42).await.expect("play");
    client
        .seek("transport-session-1", 42, 60.0)
        .await
        .expect("seek");
    client.set_volume(0.8).await.expect("set volume");

    server_task.await.expect("server task finished cleanly");
}
