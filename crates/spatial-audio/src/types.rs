//! 3D coordinates, zones, listener positions, and routing decisions.

use std::fmt;

/// A 3D coordinate point in meters relative to the display origin.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Position3D {
    /// X axis coordinate in meters (lateral displacement).
    pub x: f64,
    /// Y axis coordinate in meters (depth from screen).
    pub y: f64,
    /// Z axis coordinate in meters (height).
    pub z: f64,
}

impl Position3D {
    /// Creates a new 3D coordinate point.
    #[must_use]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    /// Calculates Euclidean 3D distance between this position and another position in meters.
    #[must_use]
    pub fn distance_to(&self, other: &Self) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

impl fmt::Display for Position3D {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({:.2}m, {:.2}m, {:.2}m)", self.x, self.y, self.z)
    }
}

/// A 3D bounding box zone for ceiling speakers or sound domes.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Zone {
    /// Unique identifier for the zone.
    pub id: String,
    /// Human readable name (e.g. "Front Left Viewing Station").
    pub name: String,
    /// Minimum corner bound.
    pub min_bound: Position3D,
    /// Maximum corner bound.
    pub max_bound: Position3D,
}

impl Zone {
    /// Creates a new 3D spatial zone.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        min_bound: Position3D,
        max_bound: Position3D,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            min_bound,
            max_bound,
        }
    }

    /// Checks if a 3D position falls within this bounding box zone.
    #[must_use]
    pub fn contains(&self, pos: &Position3D) -> bool {
        pos.x >= self.min_bound.x
            && pos.x <= self.max_bound.x
            && pos.y >= self.min_bound.y
            && pos.y <= self.max_bound.y
            && pos.z >= self.min_bound.z
            && pos.z <= self.max_bound.z
    }
}

/// Active listener / customer position in 3D space.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ListenerPosition {
    /// Physical 3D coordinates.
    pub position: Position3D,
    /// Facing orientation angle in degrees (0-360), if known.
    pub heading: Option<f64>,
    /// Confidence factor of position measurement (0.0 to 1.0).
    pub confidence: f32,
}

impl ListenerPosition {
    /// Creates a listener position from coordinates.
    #[must_use]
    pub const fn new(position: Position3D) -> Self {
        Self {
            position,
            heading: None,
            confidence: 1.0,
        }
    }

    /// Sets facing heading angle in degrees.
    #[must_use]
    pub const fn with_heading(mut self, heading: f64) -> Self {
        self.heading = Some(heading);
        self
    }

    /// Sets position confidence factor.
    #[must_use]
    pub const fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence;
        self
    }
}

/// Result of a spatial audio routing calculation.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RoutingDecision {
    /// Target sink ID selected as primary audio output.
    pub primary_sink_id: String,
    /// Secondary sinks and their calculated gain weights (0.0 to 1.0).
    pub secondary_sinks: Vec<(String, f32)>,
    /// Calculated distance from listener to primary sink in meters.
    pub calculated_distance_meters: f64,
    /// Calculated signal attenuation in dB.
    pub attenuation_db: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_calculation_is_accurate() {
        let p1 = Position3D::new(0.0, 0.0, 0.0);
        let p2 = Position3D::new(3.0, 4.0, 0.0);
        assert!((p1.distance_to(&p2) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn zone_contains_position() {
        let zone = Zone::new(
            "z1",
            "Zone 1",
            Position3D::new(0.0, 0.0, 0.0),
            Position3D::new(5.0, 5.0, 3.0),
        );
        assert!(zone.contains(&Position3D::new(2.5, 2.5, 1.5)));
        assert!(!zone.contains(&Position3D::new(6.0, 2.5, 1.5)));
    }
}
