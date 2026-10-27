//! Spatial audio routing manager, 3D distance/gain attenuation calculations, and SpatialAudioProvider implementation.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use tv_rs_core::{CommandId, SpatialAudioProvider, TvError};

use crate::backend::{AudioBackend, MockAudioBackend};
use crate::error::SpatialAudioError;
use crate::sink::{AudioSink, AudioSinkKind, AudioStream};
use crate::types::{ListenerPosition, Position3D, RoutingDecision, Zone};

/// Engine that routes audio streams to Bluetooth A2DP sinks, sound domes, or ceiling speakers based on 3D listener position.
#[derive(Clone)]
pub struct SpatialAudioRouter {
    backend: Arc<dyn AudioBackend>,
    sinks: Arc<Mutex<HashMap<String, AudioSink>>>,
    streams: Arc<Mutex<HashMap<String, AudioStream>>>,
    zones: Arc<Mutex<Vec<Zone>>>,
    listener: Arc<Mutex<Option<ListenerPosition>>>,
}

impl SpatialAudioRouter {
    /// Creates a new spatial audio router using a specific audio backend.
    #[must_use]
    pub fn new(backend: Arc<dyn AudioBackend>) -> Self {
        Self {
            backend,
            sinks: Arc::new(Mutex::new(HashMap::new())),
            streams: Arc::new(Mutex::new(HashMap::new())),
            zones: Arc::new(Mutex::new(Vec::new())),
            listener: Arc::new(Mutex::new(None)),
        }
    }

    /// Creates a default router backed by `MockAudioBackend`.
    #[must_use]
    pub fn with_mock_backend() -> Self {
        Self::new(Arc::new(MockAudioBackend::new()))
    }

    /// Registers an audio sink (Bluetooth dome, ceiling speaker, PulseAudio node).
    pub async fn register_sink(&self, sink: AudioSink) {
        let mut sinks = self.sinks.lock().await;
        sinks.insert(sink.id.clone(), sink);
    }

    /// Registers a 3D audio zone.
    pub async fn register_zone(&self, zone: Zone) {
        let mut zones = self.zones.lock().await;
        zones.push(zone);
    }

    /// Registers an active audio stream.
    pub async fn register_stream(&self, stream: AudioStream) {
        let mut streams = self.streams.lock().await;
        streams.insert(stream.stream_id.clone(), stream);
    }

    /// Explicitly routes an audio stream to a sink ID.
    pub async fn route_stream(
        &self,
        stream_id: &str,
        sink_id: &str,
    ) -> Result<(), SpatialAudioError> {
        let sinks = self.sinks.lock().await;
        if !sinks.contains_key(sink_id) {
            return Err(SpatialAudioError::SinkNotFound(sink_id.to_string()));
        }

        // Auto-connect if sink is a Bluetooth A2DP device
        if let Some(sink) = sinks.get(sink_id) {
            if let AudioSinkKind::BluetoothA2dp { mac_address, .. } = &sink.kind {
                self.backend.connect_bluetooth_a2dp(mac_address).await?;
            }
        }
        drop(sinks);

        self.backend.route_stream(stream_id, sink_id).await?;

        let mut streams = self.streams.lock().await;
        if let Some(stream) = streams.get_mut(stream_id) {
            stream.active_sink_id = Some(sink_id.to_string());
        }

        info!(
            stream = stream_id,
            sink = sink_id,
            "spatial audio stream routed"
        );
        Ok(())
    }

    /// Updates current listener position and recalculates spatial audio routing across sinks.
    pub async fn update_listener_position(
        &self,
        listener: ListenerPosition,
    ) -> Result<Option<RoutingDecision>, SpatialAudioError> {
        let mut stored_listener = self.listener.lock().await;
        *stored_listener = Some(listener.clone());
        drop(stored_listener);

        let decision = self.calculate_best_routing(&listener.position).await;

        if let Some(ref routing) = decision {
            let sinks = self.sinks.lock().await;
            if let Some(target_sink) = sinks.get(&routing.primary_sink_id) {
                // Auto-connect Bluetooth A2DP sink if closest target
                if let AudioSinkKind::BluetoothA2dp { mac_address, .. } = &target_sink.kind {
                    if let Err(e) = self.backend.connect_bluetooth_a2dp(mac_address).await {
                        warn!(mac = mac_address, error = %e, "auto Bluetooth A2DP connect failed");
                    }
                }
            }
            drop(sinks);

            // Apply calculated gain weights across primary and secondary sinks
            let mut gains = vec![(routing.primary_sink_id.clone(), 1.0f32)];
            for (sec_id, weight) in &routing.secondary_sinks {
                gains.push((sec_id.clone(), *weight));
            }
            self.backend.set_sink_gains(&gains).await?;

            // Route all active streams to primary sink
            let streams = self.streams.lock().await;
            let stream_ids: Vec<String> = streams.keys().cloned().collect();
            drop(streams);

            for stream_id in stream_ids {
                let _ = self
                    .route_stream(&stream_id, &routing.primary_sink_id)
                    .await;
            }
        }

        Ok(decision)
    }

    /// Computes closest sink, distance, attenuation, and secondary gain weights based on 3D coordinates.
    pub async fn calculate_best_routing(&self, pos: &Position3D) -> Option<RoutingDecision> {
        let sinks = self.sinks.lock().await;
        if sinks.is_empty() {
            return None;
        }

        let mut closest_sink_id: Option<String> = None;
        let mut min_distance = f64::MAX;
        let mut secondary: Vec<(String, f32)> = Vec::new();

        for (id, sink) in sinks.iter() {
            if let Some(sink_pos) = sink.position {
                let d = pos.distance_to(&sink_pos);
                let radius = sink.coverage_radius.unwrap_or(3.0);

                if d < min_distance {
                    if let Some(prev_closest) = closest_sink_id.take() {
                        let prev_d = min_distance;
                        let weight = (1.0 - (prev_d / radius)).max(0.0) as f32;
                        secondary.push((prev_closest, weight));
                    }
                    min_distance = d;
                    closest_sink_id = Some(id.clone());
                } else {
                    let weight = (1.0 - (d / radius)).max(0.0) as f32;
                    if weight > 0.05 {
                        secondary.push((id.clone(), weight));
                    }
                }
            }
        }

        let primary = closest_sink_id?;
        // Calculate distance attenuation in dB: 20 * log10(max(1.0, distance))
        let attenuation_db = if min_distance <= 1.0 {
            0.0
        } else {
            (20.0 * min_distance.log10()) as f32
        };

        Some(RoutingDecision {
            primary_sink_id: primary,
            secondary_sinks: secondary,
            calculated_distance_meters: min_distance,
            attenuation_db,
        })
    }
}

#[async_trait]
impl SpatialAudioProvider for SpatialAudioRouter {
    async fn route_audio(&self, sink_id: &str, _cmd: CommandId) -> Result<(), TvError> {
        let streams = self.streams.lock().await;
        let first_stream = streams.keys().next().cloned();
        drop(streams);

        let stream_id = first_stream.unwrap_or_else(|| "default_stream".to_string());
        self.route_stream(&stream_id, sink_id)
            .await
            .map_err(Into::into)
    }

    async fn update_listener_position(
        &self,
        x: f64,
        y: f64,
        z: f64,
        _cmd: CommandId,
    ) -> Result<(), TvError> {
        let listener = ListenerPosition::new(Position3D::new(x, y, z));
        self.update_listener_position(listener)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn router_selects_closest_directional_dome() {
        let router = SpatialAudioRouter::with_mock_backend();

        let dome_left = AudioSink::new(
            "dome_left",
            "Left Dome",
            AudioSinkKind::DirectionalDome {
                dome_id: "d_left".into(),
                coordinates: Position3D::new(-2.0, 2.0, 2.5),
                beam_angle_degrees: 30.0,
            },
        );

        let dome_right = AudioSink::new(
            "dome_right",
            "Right Dome",
            AudioSinkKind::DirectionalDome {
                dome_id: "d_right".into(),
                coordinates: Position3D::new(2.0, 2.0, 2.5),
                beam_angle_degrees: 30.0,
            },
        );

        router.register_sink(dome_left).await;
        router.register_sink(dome_right).await;

        // Listener standing near right dome
        let decision = router
            .update_listener_position(ListenerPosition::new(Position3D::new(1.8, 2.0, 1.7)))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(decision.primary_sink_id, "dome_right");
    }
}
