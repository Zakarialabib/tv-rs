//! Audio sink types, Bluetooth A2DP sinks, PulseAudio/PipeWire nodes, and directional sound domes.

use crate::types::Position3D;

/// Classification of audio output sink devices.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AudioSinkKind {
    /// Bluetooth A2DP sink (e.g., Bluetooth speaker or directional sound dome).
    BluetoothA2dp {
        /// Bluetooth device MAC address.
        mac_address: String,
        /// Host Bluetooth adapter interface ID (e.g. "hci0").
        adapter_id: Option<String>,
    },
    /// PulseAudio sink node.
    PulseAudio {
        /// System sink name in PulseAudio daemon.
        sink_name: String,
        /// Channel map string (e.g. "front-left,front-right").
        channel_map: Option<String>,
    },
    /// PipeWire audio node.
    PipeWire {
        /// PipeWire node numerical ID.
        node_id: u32,
        /// PipeWire node name identifier.
        node_name: String,
    },
    /// Directional sound dome mounted over a specific viewing position.
    DirectionalDome {
        /// Sound dome identifier.
        dome_id: String,
        /// Physical 3D mounting position.
        coordinates: Position3D,
        /// Sound beam projection angle in degrees.
        beam_angle_degrees: f32,
    },
    /// Ceiling speaker array unit.
    CeilingSpeaker {
        /// Speaker ID.
        speaker_id: String,
        /// Physical 3D location on ceiling grid.
        coordinates: Position3D,
        /// Assigned audio zone ID.
        zone: String,
    },
}

/// Operational state of an audio output sink.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SinkState {
    /// Sink is disconnected or offline.
    Disconnected,
    /// Connection handshake in progress.
    Connecting,
    /// Connected and ready to receive stream audio.
    Connected,
    /// Currently receiving active routed audio stream.
    ActiveRouting,
    /// Sink error state with error description.
    Error(String),
}

/// Represented audio output sink device or audio node.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AudioSink {
    /// Unique identifier for this sink.
    pub id: String,
    /// Human readable descriptor name.
    pub name: String,
    /// Specific hardware/software type of sink.
    pub kind: AudioSinkKind,
    /// Current volume level (0 to 100).
    pub volume: u8,
    /// Mute status.
    pub muted: bool,
    /// Connection state.
    pub state: SinkState,
    /// 3D physical coordinate position of the sink speaker/dome, if localized.
    pub position: Option<Position3D>,
    /// Coverage radius in meters for spatial attenuation calculation.
    pub coverage_radius: Option<f64>,
    /// Digital gain adjustment in dB.
    pub gain_db: f32,
}

impl AudioSink {
    /// Creates a new audio sink definition.
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>, kind: AudioSinkKind) -> Self {
        let position = match &kind {
            AudioSinkKind::DirectionalDome { coordinates, .. } => Some(*coordinates),
            AudioSinkKind::CeilingSpeaker { coordinates, .. } => Some(*coordinates),
            _ => None,
        };

        Self {
            id: id.into(),
            name: name.into(),
            kind,
            volume: 80,
            muted: false,
            state: SinkState::Disconnected,
            position,
            coverage_radius: Some(3.0),
            gain_db: 0.0,
        }
    }

    /// Sets the physical 3D position of the sink.
    #[must_use]
    pub const fn with_position(mut self, pos: Position3D) -> Self {
        self.position = Some(pos);
        self
    }

    /// Sets the spatial coverage radius in meters.
    #[must_use]
    pub const fn with_coverage_radius(mut self, radius: f64) -> Self {
        self.coverage_radius = Some(radius);
        self
    }
}

/// An active audio stream emitted by media player or system source.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AudioStream {
    /// Unique stream identifier.
    pub stream_id: String,
    /// Stream name or title.
    pub name: String,
    /// Source PulseAudio/PipeWire node name.
    pub source_node: String,
    /// Currently assigned target sink ID, if routed.
    pub active_sink_id: Option<String>,
}

impl AudioStream {
    /// Creates a new audio stream reference.
    #[must_use]
    pub fn new(stream_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            stream_id: stream_id.into(),
            name: name.into(),
            source_node: "default_source".to_string(),
            active_sink_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sink_position_extracted_from_directional_dome() {
        let coords = Position3D::new(1.0, 2.0, 3.0);
        let sink = AudioSink::new(
            "dome1",
            "Dome 1",
            AudioSinkKind::DirectionalDome {
                dome_id: "d1".into(),
                coordinates: coords,
                beam_angle_degrees: 45.0,
            },
        );
        assert_eq!(sink.position, Some(coords));
    }
}
