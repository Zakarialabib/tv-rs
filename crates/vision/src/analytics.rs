//! Analytics processor for zone occupancy, detection counting, and event generation.

use crate::engine::InferenceResult;
use crate::types::{Detection, VisionEvent, Zone};
use std::collections::HashMap;
use tracing::debug;

/// Processes inference results against target camera zones to yield structured events.
#[derive(Debug, Default)]
pub struct EventProcessor {
    last_zone_counts: HashMap<String, usize>,
}

impl EventProcessor {
    /// Creates a new event processor.
    pub fn new() -> Self {
        Self {
            last_zone_counts: HashMap::new(),
        }
    }

    /// Evaluates raw inference results against defined zones and generates structured `VisionEvent` items.
    pub fn process(&mut self, result: &InferenceResult, zones: &[Zone]) -> Vec<VisionEvent> {
        let mut events = Vec::new();

        // Get current system timestamp in seconds
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .ok();

        for zone in zones {
            // Find all detections within this zone
            let zone_detections: Vec<&Detection> = result
                .detections
                .iter()
                .filter(|d| zone.contains_detection(d))
                .collect();

            // Count persons in zone
            let person_count = zone_detections
                .iter()
                .filter(|d| d.label == "person")
                .count();

            if person_count > 0 {
                events.push(VisionEvent::PersonDetected {
                    zone: zone.id.clone(),
                    count: person_count,
                    timestamp,
                });
            }

            // Track state change / occupancy
            let previous_count = self.last_zone_counts.get(&zone.id).copied().unwrap_or(0);
            if person_count != previous_count {
                self.last_zone_counts.insert(zone.id.clone(), person_count);
                events.push(VisionEvent::ZoneOccupancy {
                    zone: zone.id.clone(),
                    count: person_count,
                    timestamp,
                });
            }

            // Other object detections in zone
            let mut label_counts: HashMap<String, usize> = HashMap::new();
            let mut label_conf: HashMap<String, f32> = HashMap::new();
            for d in &zone_detections {
                if d.label != "person" {
                    *label_counts.entry(d.label.clone()).or_insert(0) += 1;
                    label_conf
                        .entry(d.label.clone())
                        .and_modify(|c| *c = c.max(d.confidence))
                        .or_insert(d.confidence);
                }
            }

            for (label, count) in label_counts {
                let confidence = label_conf.get(&label).copied().unwrap_or(0.0);
                events.push(VisionEvent::ObjectDetected {
                    zone: zone.id.clone(),
                    label,
                    count,
                    confidence,
                    timestamp,
                });
            }
        }

        // Process pose gestures in zones
        for pose in &result.poses {
            if let Some(gesture) = &pose.gesture {
                for zone in zones {
                    let (cx, cy) = pose.bounding_box.center();
                    if cx >= zone.bounds.x_min
                        && cx <= zone.bounds.x_max
                        && cy >= zone.bounds.y_min
                        && cy <= zone.bounds.y_max
                    {
                        events.push(VisionEvent::GestureDetected {
                            zone: zone.id.clone(),
                            gesture: gesture.clone(),
                            confidence: pose.confidence,
                            timestamp,
                        });
                    }
                }
            }
        }

        debug!("Generated {} vision events across zones", events.len());
        events
    }
}
