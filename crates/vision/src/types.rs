//! Data types for vision events, zones, detections, poses, and configurations.

use serde::{Deserialize, Serialize};

/// Bounding box represented by normalized coordinates `[x_min, y_min, x_max, y_max]` in `[0.0, 1.0]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundingBox {
    /// Minimum X coordinate (left edge), range `[0.0, 1.0]`.
    pub x_min: f32,
    /// Minimum Y coordinate (top edge), range `[0.0, 1.0]`.
    pub y_min: f32,
    /// Maximum X coordinate (right edge), range `[0.0, 1.0]`.
    pub x_max: f32,
    /// Maximum Y coordinate (bottom edge), range `[0.0, 1.0]`.
    pub y_max: f32,
}

impl BoundingBox {
    /// Creates a new bounding box.
    pub fn new(x_min: f32, y_min: f32, x_max: f32, y_max: f32) -> Self {
        Self {
            x_min,
            y_min,
            x_max,
            y_max,
        }
    }

    /// Returns the center coordinate `(x, y)` of the bounding box.
    pub fn center(&self) -> (f32, f32) {
        (
            (self.x_min + self.x_max) / 2.0,
            (self.y_min + self.y_max) / 2.0,
        )
    }

    /// Checks if this bounding box overlaps with another bounding box.
    pub fn intersects(&self, other: &BoundingBox) -> bool {
        self.x_min < other.x_max
            && self.x_max > other.x_min
            && self.y_min < other.y_max
            && self.y_max > other.y_min
    }
}

/// A detection produced by an object detection model (e.g., YOLO).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Detection {
    /// Label of the detected object (e.g., "person", "car", "bottle").
    pub label: String,
    /// Numeric class index.
    pub class_id: usize,
    /// Detection confidence score in `[0.0, 1.0]`.
    pub confidence: f32,
    /// Bounding box in normalized coordinates.
    pub bounding_box: BoundingBox,
    /// Optional tracking ID assigned across consecutive frames.
    pub track_id: Option<u64>,
}

/// Gesture types detected by MediaPipe gesture analysis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GestureType {
    /// Hand waving gesture.
    HandWave,
    /// Thumbs up gesture.
    ThumbsUp,
    /// Raised hand gesture.
    RaiseHand,
    /// Standing pose.
    Standing,
    /// Sitting pose.
    Sitting,
    /// Other gesture specified by label.
    Other(String),
}

/// Individual keypoint landmark in a pose estimation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PoseLandmark {
    /// Landmark index or joint ID.
    pub id: usize,
    /// Landmark name (e.g., "left_wrist", "right_shoulder").
    pub name: String,
    /// Normalized X coordinate `[0.0, 1.0]`.
    pub x: f32,
    /// Normalized Y coordinate `[0.0, 1.0]`.
    pub y: f32,
    /// Depth or Z coordinate if available.
    pub z: f32,
    /// Keypoint visibility confidence score.
    pub confidence: f32,
}

/// Pose and gesture estimate produced by MediaPipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pose {
    /// List of keypoint landmarks.
    pub landmarks: Vec<PoseLandmark>,
    /// Detected gesture associated with this pose.
    pub gesture: Option<GestureType>,
    /// Overall pose confidence score.
    pub confidence: f32,
    /// Bounding box enclosing the person's pose.
    pub bounding_box: BoundingBox,
}

/// Defined zone region within the camera frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Zone {
    /// Unique identifier / label for the zone (e.g., "entrance", "checkout", "display_area").
    pub id: String,
    /// Bounding box defining the zone boundaries in normalized coordinates.
    pub bounds: BoundingBox,
}

impl Zone {
    /// Creates a new named zone with boundary coordinates.
    pub fn new(id: impl Into<String>, x_min: f32, y_min: f32, x_max: f32, y_max: f32) -> Self {
        Self {
            id: id.into(),
            bounds: BoundingBox::new(x_min, y_min, x_max, y_max),
        }
    }

    /// Checks whether a detection bounding box center falls within this zone.
    pub fn contains_detection(&self, detection: &Detection) -> bool {
        let (cx, cy) = detection.bounding_box.center();
        cx >= self.bounds.x_min
            && cx <= self.bounds.x_max
            && cy >= self.bounds.y_min
            && cy <= self.bounds.y_max
    }
}

/// Structured JSON event emitted by the vision pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum VisionEvent {
    /// Person detected event specifying zone and current count.
    ///
    /// JSON output: `{"event": "person_detected", "zone": "entrance", "count": 3}`
    PersonDetected {
        /// Zone ID where persons were detected.
        zone: String,
        /// Total count of persons detected in the zone.
        count: usize,
        /// Optional UNIX timestamp in seconds.
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<u64>,
    },

    /// Gesture detected event in a specific zone.
    ///
    /// JSON output: `{"event": "gesture_detected", "zone": "display_area", "gesture": "hand_wave", "confidence": 0.95}`
    GestureDetected {
        /// Zone ID where gesture occurred.
        zone: String,
        /// Recognized gesture.
        gesture: GestureType,
        /// Confidence score of gesture.
        confidence: f32,
        /// Optional UNIX timestamp in seconds.
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<u64>,
    },

    /// General object detected event.
    ///
    /// JSON output: `{"event": "object_detected", "zone": "entrance", "label": "car", "count": 1, "confidence": 0.92}`
    ObjectDetected {
        /// Zone ID where object was detected.
        zone: String,
        /// Label of the object.
        label: String,
        /// Count of matching objects.
        count: usize,
        /// Highest confidence score.
        confidence: f32,
        /// Optional UNIX timestamp in seconds.
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<u64>,
    },

    /// Zone occupancy summary update event.
    ///
    /// JSON output: `{"event": "zone_occupancy", "zone": "entrance", "count": 3}`
    ZoneOccupancy {
        /// Zone ID.
        zone: String,
        /// Current occupancy count.
        count: usize,
        /// Optional UNIX timestamp in seconds.
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<u64>,
    },
}

/// Inference engine backend type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceBackend {
    /// ONNX Runtime engine.
    OnnxRuntime,
    /// OpenCV DNN engine.
    OpenCv,
    /// Edge AI simulated mock engine (for testing and lightweight deployments).
    Mock,
}

/// RTSP camera stream connection configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RtspStreamConfig {
    /// RTSP URL (e.g., `"rtsp://192.168.1.100:554/stream1"`).
    pub url: String,
    /// Maximum frame rate target FPS (defaults to 30).
    pub fps_limit: u32,
    /// Automatic reconnect interval in seconds.
    pub reconnect_interval_secs: u64,
}

impl Default for RtspStreamConfig {
    fn default() -> Self {
        Self {
            url: "rtsp://127.0.0.1:554/live".to_string(),
            fps_limit: 30,
            reconnect_interval_secs: 5,
        }
    }
}

/// Configuration for the vision pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionConfig {
    /// RTSP stream configuration.
    pub stream: RtspStreamConfig,
    /// List of analytics zones.
    pub zones: Vec<Zone>,
    /// Minimum confidence threshold for detections `[0.0, 1.0]`.
    pub confidence_threshold: f32,
    /// Chosen AI inference backend.
    pub backend: InferenceBackend,
}

impl Default for VisionConfig {
    fn default() -> Self {
        Self {
            stream: RtspStreamConfig::default(),
            zones: vec![Zone::new("entrance", 0.0, 0.0, 1.0, 1.0)],
            confidence_threshold: 0.5,
            backend: InferenceBackend::Mock,
        }
    }
}
