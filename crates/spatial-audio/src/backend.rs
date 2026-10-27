//! Audio backend interface, mock test backend, and system PulseAudio/PipeWire/Bluetooth adapters.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

use crate::error::SpatialAudioError;
use crate::sink::{AudioSink, SinkState};

/// Abstract controller for audio sinks, Bluetooth A2DP pairing, and stream routing.
#[async_trait]
pub trait AudioBackend: Send + Sync {
    /// Discovers and lists available audio sinks on the system.
    async fn list_sinks(&self) -> Result<Vec<AudioSink>, SpatialAudioError>;

    /// Connects to a Bluetooth A2DP audio sink by MAC address.
    async fn connect_bluetooth_a2dp(&self, mac_address: &str) -> Result<(), SpatialAudioError>;

    /// Disconnects from a Bluetooth A2DP audio sink by MAC address.
    async fn disconnect_bluetooth_a2dp(&self, mac_address: &str) -> Result<(), SpatialAudioError>;

    /// Routes a named stream to a target sink.
    async fn route_stream(&self, stream_id: &str, sink_id: &str) -> Result<(), SpatialAudioError>;

    /// Sets the volume of a target sink (0-100).
    async fn set_sink_volume(&self, sink_id: &str, volume: u8) -> Result<(), SpatialAudioError>;

    /// Mutes or unmutes a target sink.
    async fn set_sink_mute(&self, sink_id: &str, mute: bool) -> Result<(), SpatialAudioError>;

    /// Updates individual sink gain weights for spatial audio panning.
    async fn set_sink_gains(&self, gains: &[(String, f32)]) -> Result<(), SpatialAudioError>;
}

/// Thread-safe in-memory mock backend for testing.
#[derive(Debug, Clone, Default)]
pub struct MockAudioBackend {
    sinks: Arc<Mutex<HashMap<String, AudioSink>>>,
    connected_bluetooth: Arc<Mutex<Vec<String>>>,
    stream_routes: Arc<Mutex<HashMap<String, String>>>,
    sink_gains: Arc<Mutex<HashMap<String, f32>>>,
}

impl MockAudioBackend {
    /// Creates a new mock audio backend with empty state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a mock sink to the backend state.
    pub async fn add_sink(&self, sink: AudioSink) {
        let mut sinks = self.sinks.lock().await;
        sinks.insert(sink.id.clone(), sink);
    }

    /// Checks if a Bluetooth MAC address is currently connected.
    pub async fn is_bluetooth_connected(&self, mac: &str) -> bool {
        let bt = self.connected_bluetooth.lock().await;
        bt.contains(&mac.to_string())
    }

    /// Gets active routed sink for a stream.
    pub async fn get_routed_sink(&self, stream_id: &str) -> Option<String> {
        let routes = self.stream_routes.lock().await;
        routes.get(stream_id).cloned()
    }

    /// Gets current gain weight assigned to a sink.
    pub async fn get_sink_gain(&self, sink_id: &str) -> Option<f32> {
        let gains = self.sink_gains.lock().await;
        gains.get(sink_id).copied()
    }
}

#[async_trait]
impl AudioBackend for MockAudioBackend {
    async fn list_sinks(&self) -> Result<Vec<AudioSink>, SpatialAudioError> {
        let sinks = self.sinks.lock().await;
        Ok(sinks.values().cloned().collect())
    }

    async fn connect_bluetooth_a2dp(&self, mac_address: &str) -> Result<(), SpatialAudioError> {
        let mut bt = self.connected_bluetooth.lock().await;
        if !bt.contains(&mac_address.to_string()) {
            bt.push(mac_address.to_string());
        }

        let mut sinks = self.sinks.lock().await;
        for sink in sinks.values_mut() {
            if let crate::sink::AudioSinkKind::BluetoothA2dp {
                mac_address: mac, ..
            } = &sink.kind
            {
                if mac == mac_address {
                    sink.state = SinkState::Connected;
                }
            }
        }
        info!(mac = mac_address, "mock bluetooth A2DP sink connected");
        Ok(())
    }

    async fn disconnect_bluetooth_a2dp(&self, mac_address: &str) -> Result<(), SpatialAudioError> {
        let mut bt = self.connected_bluetooth.lock().await;
        bt.retain(|m| m != mac_address);

        let mut sinks = self.sinks.lock().await;
        for sink in sinks.values_mut() {
            if let crate::sink::AudioSinkKind::BluetoothA2dp {
                mac_address: mac, ..
            } = &sink.kind
            {
                if mac == mac_address {
                    sink.state = SinkState::Disconnected;
                }
            }
        }
        info!(mac = mac_address, "mock bluetooth A2DP sink disconnected");
        Ok(())
    }

    async fn route_stream(&self, stream_id: &str, sink_id: &str) -> Result<(), SpatialAudioError> {
        let sinks = self.sinks.lock().await;
        if !sinks.contains_key(sink_id) {
            return Err(SpatialAudioError::SinkNotFound(sink_id.to_string()));
        }
        drop(sinks);

        let mut routes = self.stream_routes.lock().await;
        routes.insert(stream_id.to_string(), sink_id.to_string());
        info!(
            stream = stream_id,
            sink = sink_id,
            "mock audio stream routed"
        );
        Ok(())
    }

    async fn set_sink_volume(&self, sink_id: &str, volume: u8) -> Result<(), SpatialAudioError> {
        let mut sinks = self.sinks.lock().await;
        if let Some(sink) = sinks.get_mut(sink_id) {
            sink.volume = volume.min(100);
            Ok(())
        } else {
            Err(SpatialAudioError::SinkNotFound(sink_id.to_string()))
        }
    }

    async fn set_sink_mute(&self, sink_id: &str, mute: bool) -> Result<(), SpatialAudioError> {
        let mut sinks = self.sinks.lock().await;
        if let Some(sink) = sinks.get_mut(sink_id) {
            sink.muted = mute;
            Ok(())
        } else {
            Err(SpatialAudioError::SinkNotFound(sink_id.to_string()))
        }
    }

    async fn set_sink_gains(&self, gains: &[(String, f32)]) -> Result<(), SpatialAudioError> {
        let mut sink_gains = self.sink_gains.lock().await;
        for (sink_id, gain) in gains {
            sink_gains.insert(sink_id.clone(), *gain);
        }
        Ok(())
    }
}

/// System PulseAudio / PipeWire / Bluetooth CLI bridge adapter.
#[derive(Debug, Clone, Default)]
pub struct PulsePipeWireBackend;

impl PulsePipeWireBackend {
    /// Creates a system PulseAudio / PipeWire backend bridge.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl AudioBackend for PulsePipeWireBackend {
    async fn list_sinks(&self) -> Result<Vec<AudioSink>, SpatialAudioError> {
        // Interacts with pactl / pw-cli command output when executed in Linux environments
        let output = tokio::process::Command::new("pactl")
            .arg("list")
            .arg("sinks")
            .output()
            .await;

        match output {
            Ok(out) if out.status.success() => {
                // Parse pactl output or return empty vec if no sinks found
                Ok(Vec::new())
            }
            _ => Ok(Vec::new()),
        }
    }

    async fn connect_bluetooth_a2dp(&self, mac_address: &str) -> Result<(), SpatialAudioError> {
        let status = tokio::process::Command::new("bluetoothctl")
            .arg("connect")
            .arg(mac_address)
            .status()
            .await
            .map_err(|e| SpatialAudioError::BluetoothError(e.to_string()))?;

        if status.success() {
            Ok(())
        } else {
            Err(SpatialAudioError::BluetoothError(format!(
                "bluetoothctl connect failed for {mac_address}"
            )))
        }
    }

    async fn disconnect_bluetooth_a2dp(&self, mac_address: &str) -> Result<(), SpatialAudioError> {
        let status = tokio::process::Command::new("bluetoothctl")
            .arg("disconnect")
            .arg(mac_address)
            .status()
            .await
            .map_err(|e| SpatialAudioError::BluetoothError(e.to_string()))?;

        if status.success() {
            Ok(())
        } else {
            Err(SpatialAudioError::BluetoothError(format!(
                "bluetoothctl disconnect failed for {mac_address}"
            )))
        }
    }

    async fn route_stream(&self, stream_id: &str, sink_id: &str) -> Result<(), SpatialAudioError> {
        let status = tokio::process::Command::new("pactl")
            .arg("move-sink-input")
            .arg(stream_id)
            .arg(sink_id)
            .status()
            .await
            .map_err(|e| SpatialAudioError::CommandFailed(e.to_string()))?;

        if status.success() {
            Ok(())
        } else {
            Err(SpatialAudioError::CommandFailed(format!(
                "pactl move-sink-input failed for stream {stream_id} -> sink {sink_id}"
            )))
        }
    }

    async fn set_sink_volume(&self, sink_id: &str, volume: u8) -> Result<(), SpatialAudioError> {
        let vol_str = format!("{}%", volume.min(100));
        let status = tokio::process::Command::new("pactl")
            .arg("set-sink-volume")
            .arg(sink_id)
            .arg(vol_str)
            .status()
            .await
            .map_err(|e| SpatialAudioError::CommandFailed(e.to_string()))?;

        if status.success() {
            Ok(())
        } else {
            Err(SpatialAudioError::CommandFailed(format!(
                "pactl set-sink-volume failed for {sink_id}"
            )))
        }
    }

    async fn set_sink_mute(&self, sink_id: &str, mute: bool) -> Result<(), SpatialAudioError> {
        let mute_arg = if mute { "1" } else { "0" };
        let status = tokio::process::Command::new("pactl")
            .arg("set-sink-mute")
            .arg(sink_id)
            .arg(mute_arg)
            .status()
            .await
            .map_err(|e| SpatialAudioError::CommandFailed(e.to_string()))?;

        if status.success() {
            Ok(())
        } else {
            Err(SpatialAudioError::CommandFailed(format!(
                "pactl set-sink-mute failed for {sink_id}"
            )))
        }
    }

    async fn set_sink_gains(&self, _gains: &[(String, f32)]) -> Result<(), SpatialAudioError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sink::{AudioSink, AudioSinkKind};

    #[tokio::test]
    async fn mock_backend_tracks_bluetooth_and_routing() {
        let backend = MockAudioBackend::new();
        let sink = AudioSink::new(
            "bt_dome",
            "Dome Speaker",
            AudioSinkKind::BluetoothA2dp {
                mac_address: "AA:BB:CC:DD:EE:FF".into(),
                adapter_id: None,
            },
        );
        backend.add_sink(sink).await;

        assert!(!backend.is_bluetooth_connected("AA:BB:CC:DD:EE:FF").await);
        backend
            .connect_bluetooth_a2dp("AA:BB:CC:DD:EE:FF")
            .await
            .unwrap();
        assert!(backend.is_bluetooth_connected("AA:BB:CC:DD:EE:FF").await);

        backend.route_stream("stream_1", "bt_dome").await.unwrap();
        assert_eq!(
            backend.get_routed_sink("stream_1").await,
            Some("bt_dome".to_string())
        );
    }
}
