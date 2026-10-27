//! AI model inference engine wrappers for YOLO object detection and MediaPipe pose/gesture tracking.

use crate::error::Result;
use crate::stream::VideoFrame;
use crate::types::{BoundingBox, Detection, GestureType, InferenceBackend, Pose, PoseLandmark};
use async_trait::async_trait;
use tracing::debug;

/// Combined results of an AI inference pass over a video frame.
#[derive(Debug, Clone, Default)]
pub struct InferenceResult {
    /// List of detected objects (e.g., YOLO detections).
    pub detections: Vec<Detection>,
    /// List of detected human poses and gestures (e.g., MediaPipe pose tracking).
    pub poses: Vec<Pose>,
}

/// Abstract async trait for AI model inference backends.
#[async_trait]
pub trait VisionEngine: Send + Sync {
    /// Runs AI model inference on the provided video frame.
    async fn infer(&mut self, frame: &VideoFrame) -> Result<InferenceResult>;

    /// Returns the name of the active inference backend.
    fn backend_name(&self) -> &'static str;
}

/// YOLO object detection model wrapper (supporting ONNX Runtime / OpenCV / Mock).
pub struct YoloDetector {
    backend: InferenceBackend,
    confidence_threshold: f32,
    model_path: String,
}

impl YoloDetector {
    /// Creates a new YOLO object detector with specified parameters.
    pub fn new(
        backend: InferenceBackend,
        confidence_threshold: f32,
        model_path: impl Into<String>,
    ) -> Self {
        Self {
            backend,
            confidence_threshold,
            model_path: model_path.into(),
        }
    }

    /// Returns the model file path.
    pub fn model_path(&self) -> &str {
        &self.model_path
    }

    /// Returns the configured confidence threshold.
    pub fn confidence_threshold(&self) -> f32 {
        self.confidence_threshold
    }

    /// Performs Non-Maximum Suppression (NMS) to filter overlapping bounding boxes.
    pub fn non_maximum_suppression(
        detections: Vec<Detection>,
        iou_threshold: f32,
    ) -> Vec<Detection> {
        let mut sorted = detections;
        sorted.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));

        let mut kept: Vec<Detection> = Vec::new();
        for det in sorted {
            let mut keep = true;
            for existing in &kept {
                if det.class_id == existing.class_id {
                    let iou = Self::calculate_iou(&det.bounding_box, &existing.bounding_box);
                    if iou > iou_threshold {
                        keep = false;
                        break;
                    }
                }
            }
            if keep {
                kept.push(det);
            }
        }
        kept
    }

    fn calculate_iou(box_a: &BoundingBox, box_b: &BoundingBox) -> f32 {
        let x1 = box_a.x_min.max(box_b.x_min);
        let y1 = box_a.y_min.max(box_b.y_min);
        let x2 = box_a.x_max.min(box_b.x_max);
        let y2 = box_a.y_max.min(box_b.y_max);

        let intersection_area = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
        let area_a = (box_a.x_max - box_a.x_min) * (box_a.y_max - box_a.y_min);
        let area_b = (box_b.x_max - box_b.x_min) * (box_b.y_max - box_b.y_min);
        let union_area = area_a + area_b - intersection_area;

        if union_area <= 0.0 {
            0.0
        } else {
            intersection_area / union_area
        }
    }
}

#[async_trait]
impl VisionEngine for YoloDetector {
    async fn infer(&mut self, frame: &VideoFrame) -> Result<InferenceResult> {
        debug!(
            "Running YOLO detection on frame {} via {:?}",
            frame.frame_id, self.backend
        );

        // Simulation / ONNX Runtime placeholder detections for standard frame
        let raw_detections = vec![Detection {
            label: "person".to_string(),
            class_id: 0,
            confidence: 0.92,
            bounding_box: BoundingBox::new(0.1, 0.1, 0.4, 0.8),
            track_id: Some(1),
        }];

        let filtered = raw_detections
            .into_iter()
            .filter(|d| d.confidence >= self.confidence_threshold)
            .collect();

        Ok(InferenceResult {
            detections: filtered,
            poses: vec![],
        })
    }

    fn backend_name(&self) -> &'static str {
        match self.backend {
            InferenceBackend::OnnxRuntime => "ONNX Runtime (YOLO)",
            InferenceBackend::OpenCv => "OpenCV (YOLO)",
            InferenceBackend::Mock => "Mock YOLO Engine",
        }
    }
}

/// MediaPipe pose & gesture tracking model wrapper.
pub struct MediaPipeTracker {
    backend: InferenceBackend,
    confidence_threshold: f32,
}

impl MediaPipeTracker {
    /// Creates a new MediaPipe pose and gesture tracker.
    pub fn new(backend: InferenceBackend, confidence_threshold: f32) -> Self {
        Self {
            backend,
            confidence_threshold,
        }
    }

    /// Returns the configured confidence threshold.
    pub fn confidence_threshold(&self) -> f32 {
        self.confidence_threshold
    }

    /// Classifies a gesture from keypoint landmarks (e.g., detecting hand wave).
    pub fn classify_gesture(landmarks: &[PoseLandmark]) -> Option<GestureType> {
        let left_wrist = landmarks.iter().find(|l| l.name == "left_wrist");
        let left_shoulder = landmarks.iter().find(|l| l.name == "left_shoulder");

        if let (Some(wrist), Some(shoulder)) = (left_wrist, left_shoulder) {
            if wrist.y < shoulder.y && wrist.confidence > 0.6 {
                return Some(GestureType::HandWave);
            }
        }
        Some(GestureType::Standing)
    }
}

#[async_trait]
impl VisionEngine for MediaPipeTracker {
    async fn infer(&mut self, frame: &VideoFrame) -> Result<InferenceResult> {
        debug!(
            "Running MediaPipe pose estimation on frame {}",
            frame.frame_id
        );

        let landmarks = vec![
            PoseLandmark {
                id: 11,
                name: "left_shoulder".to_string(),
                x: 0.25,
                y: 0.30,
                z: 0.0,
                confidence: 0.95,
            },
            PoseLandmark {
                id: 15,
                name: "left_wrist".to_string(),
                x: 0.24,
                y: 0.20,
                z: 0.0,
                confidence: 0.90,
            },
        ];

        let gesture = Self::classify_gesture(&landmarks);
        let pose = Pose {
            landmarks,
            gesture,
            confidence: 0.93,
            bounding_box: BoundingBox::new(0.1, 0.1, 0.4, 0.8),
        };

        let poses = if pose.confidence >= self.confidence_threshold {
            vec![pose]
        } else {
            vec![]
        };

        Ok(InferenceResult {
            detections: vec![],
            poses,
        })
    }

    fn backend_name(&self) -> &'static str {
        match self.backend {
            InferenceBackend::OnnxRuntime => "ONNX Runtime (MediaPipe)",
            InferenceBackend::OpenCv => "OpenCV (MediaPipe)",
            InferenceBackend::Mock => "Mock MediaPipe Engine",
        }
    }
}

/// Simulated mock engine for edge testing without external C library dependencies.
pub struct MockEngine {
    preset_detections: Vec<Detection>,
    preset_poses: Vec<Pose>,
}

impl MockEngine {
    /// Creates a new mock vision engine with default test detections.
    pub fn new() -> Self {
        Self {
            preset_detections: vec![
                Detection {
                    label: "person".to_string(),
                    class_id: 0,
                    confidence: 0.95,
                    bounding_box: BoundingBox::new(0.05, 0.05, 0.35, 0.75),
                    track_id: Some(101),
                },
                Detection {
                    label: "person".to_string(),
                    class_id: 0,
                    confidence: 0.88,
                    bounding_box: BoundingBox::new(0.10, 0.10, 0.30, 0.70),
                    track_id: Some(102),
                },
                Detection {
                    label: "person".to_string(),
                    class_id: 0,
                    confidence: 0.91,
                    bounding_box: BoundingBox::new(0.15, 0.15, 0.40, 0.80),
                    track_id: Some(103),
                },
            ],
            preset_poses: vec![],
        }
    }

    /// Sets custom preset detections to return on inference.
    pub fn with_detections(mut self, detections: Vec<Detection>) -> Self {
        self.preset_detections = detections;
        self
    }

    /// Sets custom preset poses to return on inference.
    pub fn with_poses(mut self, poses: Vec<Pose>) -> Self {
        self.preset_poses = poses;
        self
    }
}

impl Default for MockEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VisionEngine for MockEngine {
    async fn infer(&mut self, _frame: &VideoFrame) -> Result<InferenceResult> {
        Ok(InferenceResult {
            detections: self.preset_detections.clone(),
            poses: self.preset_poses.clone(),
        })
    }

    fn backend_name(&self) -> &'static str {
        "Mock AI Engine"
    }
}
