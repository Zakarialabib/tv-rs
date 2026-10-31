//! Unit tests for protobuf encode/decode round-trip.

use prost::Message;
use tv_rs_cast::proto::cast_channel::{cast_message, CastMessage};

#[test]
fn test_protobuf_roundtrip() {
    let msg = CastMessage {
        protocol_version: cast_message::ProtocolVersion::Castv210 as i32,
        source_id: "sender-0".to_string(),
        destination_id: "receiver-0".to_string(),
        namespace: "urn:x-cast:com.google.cast.tp.connection".to_string(),
        payload_type: cast_message::PayloadType::String as i32,
        payload_utf8: Some(r#"{"type":"CONNECT"}"#.to_string()),
        payload_binary: None,
    };

    let mut buf = bytes::BytesMut::new();
    msg.encode(&mut buf).unwrap();

    let decoded = CastMessage::decode(&buf[..]).unwrap();
    assert_eq!(msg.protocol_version, decoded.protocol_version);
    assert_eq!(msg.source_id, decoded.source_id);
    assert_eq!(msg.destination_id, decoded.destination_id);
    assert_eq!(msg.namespace, decoded.namespace);
    assert_eq!(msg.payload_type, decoded.payload_type);
    assert_eq!(msg.payload_utf8, decoded.payload_utf8);
    assert_eq!(msg.payload_binary, decoded.payload_binary);
}
