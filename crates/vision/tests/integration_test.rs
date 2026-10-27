#![allow(missing_docs)]

use tv_rs_vision::{
    BoundingBox, Detection, EventProcessor, GestureType, InferenceBackend, MediaPipeTracker,
    MockEngine, RtspStreamConfig, RtspStreamReader, VisionConfig, VisionEvent, VisionPipeline,
    YoloDetector, Zone,
};

#[test]
fn test_vision_event_json_serialization_exact_format() {
    let person_event = VisionEvent::PersonDetected {
        zone: "entrance".to_string(),
        count: 3,
        timestamp: None,
    };

    let json_output = serde_json::to_string(&person_event).expect("JSON serialization failed");
    assert_eq!(
        json_output,
        r#"{"event":"person_detected","zone":"entrance","count":3}"#
    );

    let gesture_event = VisionEvent::GestureDetected {
        zone: "display_area".to_string(),
        gesture: GestureType::HandWave,
        confidence: 0.95,
        timestamp: None,
    };

    let json_gesture = serde_json::to_string(&gesture_event).expect("JSON serialization failed");
    assert_eq!(
        json_gesture,
        r#"{"event":"gesture_detected","zone":"display_area","gesture":"hand_wave","confidence":0.95}"#
    );
}

#[test]
fn test_zone_detection_containment() {
    let zone = Zone::new("entrance", 0.0, 0.0, 0.5, 0.5);
    let inside_detection = Detection {
        label: "person".to_string(),
        class_id: 0,
        confidence: 0.95,
        bounding_box: BoundingBox::new(0.1, 0.1, 0.3, 0.3),
        track_id: None,
    };
    let outside_detection = Detection {
        label: "person".to_string(),
        class_id: 0,
        confidence: 0.90,
        bounding_box: BoundingBox::new(0.6, 0.6, 0.8, 0.8),
        track_id: None,
    };

    assert!(zone.contains_detection(&inside_detection));
    assert!(!zone.contains_detection(&outside_detection));
}

#[test]
fn test_event_processor() {
    let mut processor = EventProcessor::new();
    let zone = Zone::new("entrance", 0.0, 0.0, 1.0, 1.0);
    let det = Detection {
        label: "person".to_string(),
        class_id: 0,
        confidence: 0.95,
        bounding_box: BoundingBox::new(0.1, 0.1, 0.4, 0.4),
        track_id: None,
    };
    let inf_result = tv_rs_vision::InferenceResult {
        detections: vec![det],
        poses: vec![],
    };

    let events = processor.process(&inf_result, &[zone]);
    assert!(!events.is_empty());
}

#[test]
fn test_yolo_non_maximum_suppression() {
    let det1 = Detection {
        label: "person".to_string(),
        class_id: 0,
        confidence: 0.95,
        bounding_box: BoundingBox::new(0.1, 0.1, 0.4, 0.4),
        track_id: None,
    };
    let det2 = Detection {
        label: "person".to_string(),
        class_id: 0,
        confidence: 0.80,
        bounding_box: BoundingBox::new(0.12, 0.12, 0.38, 0.38),
        track_id: None,
    };

    let suppressed = YoloDetector::non_maximum_suppression(vec![det1, det2], 0.5);
    assert_eq!(suppressed.len(), 1);
    assert_eq!(suppressed[0].confidence, 0.95);
}

#[test]
fn test_mediapipe_gesture_classification() {
    let tracker = MediaPipeTracker::new(InferenceBackend::Mock, 0.5);
    assert_eq!(tracker.confidence_threshold(), 0.5);
}

#[tokio::test]
async fn test_rtsp_stream_reader_synthetic_frames() {
    let config = RtspStreamConfig {
        url: "rtsp://192.168.1.50:554/live".to_string(),
        fps_limit: 100,
        reconnect_interval_secs: 1,
    };

    let mut reader = RtspStreamReader::connect(config)
        .await
        .expect("RTSP reader connection failed");

    assert!(reader.is_connected());
    let frame = reader.next_frame().await.expect("Failed to get frame");
    assert_eq!(frame.width, 1280);
    assert_eq!(frame.height, 720);
    reader.close().await;
    assert!(!reader.is_connected());
}

#[tokio::test]
async fn test_vision_pipeline_execution() {
    let config = VisionConfig {
        stream: RtspStreamConfig {
            url: "rtsp://localhost:554/cam1".to_string(),
            fps_limit: 100,
            reconnect_interval_secs: 1,
        },
        zones: vec![Zone::new("entrance", 0.0, 0.0, 1.0, 1.0)],
        confidence_threshold: 0.5,
        backend: InferenceBackend::Mock,
    };

    let mock_engine = Box::new(MockEngine::new());
    let mut pipeline = VisionPipeline::new(config, mock_engine);
    pipeline.connect().await.expect("Failed pipeline connect");

    let events = pipeline.step().await.expect("Failed pipeline step");
    assert!(!events.is_empty());

    let person_event = events
        .iter()
        .find(|e| matches!(e, VisionEvent::PersonDetected { .. }))
        .expect("Missing PersonDetected event");

    if let VisionEvent::PersonDetected { zone, count, .. } = person_event {
        assert_eq!(zone, "entrance");
        assert_eq!(*count, 3);
    }
}
