//! Integration tests for canned stream framing in `tv-rs-serial`.

#![allow(missing_docs)]

use tokio::io::AsyncWriteExt;
use tv_rs_serial::{FramedTransport, LineCodec, RawLineCodec};

#[tokio::test]
async fn test_canned_duplex_stream_line_codec() {
    let (client_stream, mut server_stream) = tokio::io::duplex(1024);
    let mut transport = FramedTransport::new(client_stream, LineCodec::new());

    // Client sends command line
    transport
        .send(&"POWER_ON".to_string())
        .await
        .expect("sending frame should succeed");

    // Server receives command over wire
    let mut read_buf = vec![0u8; 64];
    let n = tokio::io::AsyncReadExt::read(&mut server_stream, &mut read_buf)
        .await
        .expect("server read should succeed");
    assert_eq!(&read_buf[..n], b"POWER_ON\r\n");

    // Server sends response
    server_stream
        .write_all(b"POWER_ON_ACK\r\n")
        .await
        .expect("server write should succeed");

    // Client parses response frame
    let frame = transport
        .next_frame()
        .await
        .expect("next_frame should succeed")
        .expect("frame should be Some");

    assert_eq!(frame, "POWER_ON_ACK");
}

#[tokio::test]
async fn test_canned_duplex_stream_partial_frame_assembly() {
    let (client_stream, mut server_stream) = tokio::io::duplex(1024);
    let mut transport = FramedTransport::new(client_stream, LineCodec::new());

    // Server writes partial frame 1
    server_stream.write_all(b"PARTIAL_").await.unwrap();

    // Spawn client next_frame task
    let task = tokio::spawn(async move { transport.next_frame().await });

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    // Server finishes frame
    server_stream.write_all(b"FRAME\r\n").await.unwrap();

    let frame = task.await.unwrap().unwrap().unwrap();
    assert_eq!(frame, "PARTIAL_FRAME");
}

#[tokio::test]
async fn test_canned_duplex_stream_raw_binary_codec() {
    let (client_stream, mut server_stream) = tokio::io::duplex(1024);
    let mut transport = FramedTransport::new(client_stream, RawLineCodec::new());

    let payload = vec![0xAA, 0x11, 0x22, 0x33, 0xFF];
    transport.send(&payload).await.unwrap();

    let mut raw_wire = vec![0u8; 64];
    let n = tokio::io::AsyncReadExt::read(&mut server_stream, &mut raw_wire)
        .await
        .unwrap();
    assert_eq!(&raw_wire[..n], b"\xAA\x11\x22\x33\xFF\r\n");

    server_stream.write_all(b"\x00\x01\x02\n").await.unwrap();

    let frame = transport.next_frame().await.unwrap().unwrap();
    assert_eq!(frame, vec![0x00, 0x01, 0x02]);
}

#[tokio::test]
async fn test_custom_delimiter_framing() {
    let (client_stream, mut server_stream) = tokio::io::duplex(1024);
    let codec = LineCodec::new().with_delimiter(b';');
    let mut transport = FramedTransport::new(client_stream, codec);

    server_stream
        .write_all(b"COMMAND1;COMMAND2;")
        .await
        .unwrap();

    let f1 = transport.next_frame().await.unwrap().unwrap();
    let f2 = transport.next_frame().await.unwrap().unwrap();

    assert_eq!(f1, "COMMAND1");
    assert_eq!(f2, "COMMAND2");
}
