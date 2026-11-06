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

#[test]
fn test_media_status_dto_roundtrip() {
    use tv_rs_cast::MediaStatusDto;

    let dto = MediaStatusDto {
        state: "PLAYING".to_string(),
        position: 42.5,
        duration: Some(120.0),
        track: Some("https://example.com/video.mp4".to_string()),
    };

    let serialized = serde_json::to_string(&dto).unwrap();
    let deserialized: MediaStatusDto = serde_json::from_str(&serialized).unwrap();
    assert_eq!(dto, deserialized);
}

#[test]
fn test_media_response_to_dto_mapping() {
    use tv_rs_cast::payload::MediaResponse;
    use tv_rs_cast::MediaStatusDto;

    let json = r#"{
        "type": "MEDIA_STATUS",
        "requestId": 99,
        "status": [
            {
                "mediaSessionId": 1,
                "playerState": "BUFFERING",
                "currentTime": 10.0,
                "media": {
                    "contentId": "urn:content:sample-stream",
                    "contentType": "video/mp4",
                    "streamType": "BUFFERED",
                    "duration": 360.5
                }
            }
        ]
    }"#;

    let parsed: MediaResponse = serde_json::from_str(json).unwrap();
    match parsed {
        MediaResponse::MediaStatus { request_id, status } => {
            assert_eq!(request_id, 99);
            assert_eq!(status.len(), 1);
            let dto = MediaStatusDto::from(&status[0]);
            assert_eq!(dto.state, "BUFFERING");
            assert_eq!(dto.position, 10.0);
            assert_eq!(dto.duration, Some(360.5));
            assert_eq!(dto.track, Some("urn:content:sample-stream".to_string()));
        }
    }
}
