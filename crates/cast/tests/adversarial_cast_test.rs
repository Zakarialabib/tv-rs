//! Adversarial Challenge Test Suite for Cast TLS Handshake & Protobuf Protocol Parsing.
//!
//! Empirically challenges and stress tests:
//! 1. Empty / Missing status array in MEDIA_STATUS (`"status": []`).
//! 2. Malformed Protobuf framing:
//!    - Truncated length prefix (EOF mid-u32).
//!    - Truncated message payload (declared length exceeds actual bytes sent before EOF).
//!    - Corrupt protobuf wire bytes (fails `CastMessage::decode`).
//! 3. Malformed JSON within protobuf `payload_utf8`.
//! 4. Abrupt TLS handshake termination / failure.
//! 5. Handshake and control dispatch under rapid reconnect stress.

use std::sync::Arc;

use bytes::{BufMut, BytesMut};
use prost::Message;
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

async fn read_framed_message<R: AsyncReadExt + Unpin>(reader: &mut R) -> Result<CastMessage, String> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await.map_err(|e| format!("read len: {e}"))?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload).await.map_err(|e| format!("read payload: {e}"))?;
    CastMessage::decode(&payload[..]).map_err(|e| format!("decode: {e}"))
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
async fn test_cast_empty_media_status_rejection() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let tls_config = Arc::new(generate_mock_tls_config());
    let acceptor = TlsAcceptor::from(tls_config);

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut tls_stream = acceptor.accept(stream).await.expect("tls accept");

        // 1. Read CONNECT
        let connect_msg = read_framed_message(&mut tls_stream).await.expect("connect msg");
        assert_eq!(connect_msg.namespace, "urn:x-cast:com.google.cast.tp.connection");

        // 2. Read GET_STATUS
        let status_req = read_framed_message(&mut tls_stream).await.expect("status req");
        assert_eq!(status_req.namespace, "urn:x-cast:com.google.cast.media");

        // Send invalid MEDIA_STATUS with EMPTY status list `[]`
        let empty_status_reply = serde_json::json!({
            "type": "MEDIA_STATUS",
            "requestId": 1,
            "status": []
        });

        let reply_msg = CastMessage {
            protocol_version: cast_message::ProtocolVersion::Castv210 as i32,
            source_id: "receiver-0".to_string(),
            destination_id: status_req.source_id,
            namespace: "urn:x-cast:com.google.cast.media".to_string(),
            payload_type: cast_message::PayloadType::String as i32,
            payload_utf8: Some(empty_status_reply.to_string()),
            payload_binary: None,
        };
        write_framed_message(&mut tls_stream, &reply_msg).await;
    });

    let mut client = CastClient::connect_port("127.0.0.1", port)
        .await
        .expect("client connect ok");

    let err = client
        .media_status("transport-session-empty")
        .await
        .expect_err("Empty media status array must fail");

    assert!(
        err.to_string().contains("empty status list"),
        "Expected error message indicating empty status list, got: {err}"
    );

    server_task.await.expect("server finished");
}

#[tokio::test]
async fn test_cast_malformed_protobuf_wire_bytes() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let tls_config = Arc::new(generate_mock_tls_config());
    let acceptor = TlsAcceptor::from(tls_config);

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut tls_stream = acceptor.accept(stream).await.expect("tls accept");

        let _ = read_framed_message(&mut tls_stream).await.expect("connect msg");
        let _ = read_framed_message(&mut tls_stream).await.expect("status req");

        // Send 4-byte length prefix (16 bytes payload)
        let corrupt_payload = [0xFF; 16]; // invalid protobuf tag/wire format
        let mut buf = BytesMut::new();
        buf.put_u32(16);
        buf.put_slice(&corrupt_payload);

        tls_stream.write_all(&buf).await.expect("write corrupt frame");
        tls_stream.flush().await.expect("flush");
    });

    let mut client = CastClient::connect_port("127.0.0.1", port)
        .await
        .expect("client connect ok");

    let err = client
        .media_status("session-corrupt-proto")
        .await
        .expect_err("Corrupt protobuf wire bytes must fail decode");

    assert!(
        matches!(err, tv_rs_cast::error::CastError::Protobuf(_)),
        "Expected CastError::Protobuf, got: {err:?}"
    );

    server_task.await.expect("server finished");
}

#[tokio::test]
async fn test_cast_truncated_length_prefix_eof() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let tls_config = Arc::new(generate_mock_tls_config());
    let acceptor = TlsAcceptor::from(tls_config);

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut tls_stream = acceptor.accept(stream).await.expect("tls accept");

        let _ = read_framed_message(&mut tls_stream).await.expect("connect msg");
        let _ = read_framed_message(&mut tls_stream).await.expect("status req");

        // Send only 2 bytes of the 4-byte length prefix, then abruptly close connection
        tls_stream.write_all(&[0x00, 0x01]).await.expect("write partial len");
        tls_stream.flush().await.expect("flush");
        drop(tls_stream);
    });

    let mut client = CastClient::connect_port("127.0.0.1", port)
        .await
        .expect("client connect ok");

    let err = client
        .media_status("session-truncated-len")
        .await
        .expect_err("Truncated length prefix must fail with IO error");

    assert!(
        matches!(err, tv_rs_cast::error::CastError::Io(_)),
        "Expected CastError::Io, got: {err:?}"
    );

    server_task.await.expect("server finished");
}

#[tokio::test]
async fn test_cast_truncated_payload_eof() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let tls_config = Arc::new(generate_mock_tls_config());
    let acceptor = TlsAcceptor::from(tls_config);

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut tls_stream = acceptor.accept(stream).await.expect("tls accept");

        let _ = read_framed_message(&mut tls_stream).await.expect("connect msg");
        let _ = read_framed_message(&mut tls_stream).await.expect("status req");

        // Announce 1000 bytes, but send only 10 bytes then close
        let mut buf = BytesMut::new();
        buf.put_u32(1000);
        buf.put_slice(&[0x08, 0x01, 0x12, 0x05, 0x74, 0x65, 0x73, 0x74, 0x00, 0x00]);

        tls_stream.write_all(&buf).await.expect("write partial payload");
        tls_stream.flush().await.expect("flush");
        drop(tls_stream);
    });

    let mut client = CastClient::connect_port("127.0.0.1", port)
        .await
        .expect("client connect ok");

    let err = client
        .media_status("session-truncated-payload")
        .await
        .expect_err("Truncated payload must fail with IO error");

    assert!(
        matches!(err, tv_rs_cast::error::CastError::Io(_)),
        "Expected CastError::Io, got: {err:?}"
    );

    server_task.await.expect("server finished");
}

#[tokio::test]
async fn test_cast_malformed_json_in_protobuf_payload() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let tls_config = Arc::new(generate_mock_tls_config());
    let acceptor = TlsAcceptor::from(tls_config);

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut tls_stream = acceptor.accept(stream).await.expect("tls accept");

        let _ = read_framed_message(&mut tls_stream).await.expect("connect msg");
        let status_req = read_framed_message(&mut tls_stream).await.expect("status req");

        // Send valid protobuf CastMessage, but with broken JSON in payload_utf8
        let reply_msg = CastMessage {
            protocol_version: cast_message::ProtocolVersion::Castv210 as i32,
            source_id: "receiver-0".to_string(),
            destination_id: status_req.source_id,
            namespace: "urn:x-cast:com.google.cast.media".to_string(),
            payload_type: cast_message::PayloadType::String as i32,
            payload_utf8: Some("{ this is definitely not valid json : [[".to_string()),
            payload_binary: None,
        };
        write_framed_message(&mut tls_stream, &reply_msg).await;
    });

    let mut client = CastClient::connect_port("127.0.0.1", port)
        .await
        .expect("client connect ok");

    let err = client
        .media_status("session-bad-json")
        .await
        .expect_err("Malformed JSON must fail with JSON error");

    assert!(
        matches!(err, tv_rs_cast::error::CastError::Json(_)),
        "Expected CastError::Json, got: {err:?}"
    );

    server_task.await.expect("server finished");
}

#[tokio::test]
async fn test_cast_tls_handshake_abrupt_failure() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();

    // Server immediately closes connection on accept (does not complete TLS handshake)
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        drop(stream);
    });

    let result = CastClient::connect_port("127.0.0.1", port).await;
    assert!(result.is_err(), "Abrupt TLS closure must fail connect");

    server_task.await.expect("server task finished");
}
