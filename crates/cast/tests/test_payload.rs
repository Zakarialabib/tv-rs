//! Unit tests for Cast payloads.

use tv_rs_cast::payload::{ConnectionRequest, ReceiverResponse};

#[test]
fn test_connection_request_json() {
    let req = ConnectionRequest::Connect;
    let json = serde_json::to_string(&req).unwrap();
    assert_eq!(json, r#"{"type":"CONNECT"}"#);

    let parsed: ConnectionRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, req);
}

#[test]
fn test_receiver_response_json() {
    let json = r#"{
        "type": "RECEIVER_STATUS",
        "requestId": 123,
        "status": {
            "volume": {
                "level": 0.5,
                "muted": false
            }
        }
    }"#;

    let parsed: ReceiverResponse = serde_json::from_str(json).unwrap();
    match parsed {
        ReceiverResponse::ReceiverStatus { request_id, status } => {
            assert_eq!(request_id, 123);
            let vol = status.volume.unwrap();
            assert_eq!(vol.level, Some(0.5));
            assert_eq!(vol.muted, Some(false));
        }
        _ => panic!("Expected ReceiverStatus"),
    }
}
