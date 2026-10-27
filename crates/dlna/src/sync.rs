//! Synchronized media delivery and playlist awareness for DLNA renderers.
//!
//! Provides playlist tracking, media timing synchronization across multiple
//! renderers, drift measurement, and coordinated playback commands (`play`,
//! `stop`, `seek`, `volume`).

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::error::DlnaError;
use crate::soap::{AvTransport, RenderingControl};

/// An item in a media playlist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaylistItem {
    /// Item identifier.
    pub id: String,
    /// Title of the media item.
    pub title: String,
    /// Stream or media URL.
    pub url: String,
    /// Expected duration if known.
    pub duration: Option<Duration>,
}

impl PlaylistItem {
    /// Create a new playlist item.
    pub fn new(id: impl Into<String>, title: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            url: url.into(),
            duration: None,
        }
    }

    /// Set duration.
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }
}

/// A playlist holding an ordered list of media items and maintaining active track index.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playlist {
    /// List of media items.
    pub items: Vec<PlaylistItem>,
    /// Currently active item index in `items`.
    pub current_index: usize,
    /// Whether to loop the playlist upon reaching the end.
    pub loop_playlist: bool,
}

impl Playlist {
    /// Create a new empty playlist.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a playlist pre-populated with items.
    pub fn with_items(items: Vec<PlaylistItem>) -> Self {
        Self {
            items,
            current_index: 0,
            loop_playlist: false,
        }
    }

    /// Add an item to the playlist.
    pub fn add(&mut self, item: PlaylistItem) {
        self.items.push(item);
    }

    /// Retrieve the current playlist item.
    pub fn current_item(&self) -> Option<&PlaylistItem> {
        self.items.get(self.current_index)
    }

    /// Advance to the next item, returning `Some(&PlaylistItem)` if successful.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<&PlaylistItem> {
        if self.items.is_empty() {
            return None;
        }
        if self.current_index + 1 < self.items.len() {
            self.current_index += 1;
            Some(&self.items[self.current_index])
        } else if self.loop_playlist {
            self.current_index = 0;
            Some(&self.items[0])
        } else {
            None
        }
    }

    /// Move to the previous item.
    pub fn previous(&mut self) -> Option<&PlaylistItem> {
        if self.items.is_empty() {
            return None;
        }
        if self.current_index > 0 {
            self.current_index -= 1;
            Some(&self.items[self.current_index])
        } else if self.loop_playlist {
            self.current_index = self.items.len() - 1;
            Some(&self.items[self.current_index])
        } else {
            None
        }
    }

    /// Jump to a specific index in the playlist.
    pub fn jump_to(&mut self, index: usize) -> Option<&PlaylistItem> {
        if index < self.items.len() {
            self.current_index = index;
            Some(&self.items[self.current_index])
        } else {
            None
        }
    }
}

/// Shared reference clock state for tracking media playback position and drift.
#[derive(Debug, Clone)]
pub struct SyncClock {
    /// Wall-clock system time when reference playback started or resynced.
    pub reference_time: SystemTime,
    /// Target playback position at `reference_time`.
    pub start_position: Duration,
    /// Playback rate multiplier (1.0 = normal speed).
    pub speed_rate: f64,
}

impl Default for SyncClock {
    fn default() -> Self {
        Self {
            reference_time: SystemTime::now(),
            start_position: Duration::ZERO,
            speed_rate: 1.0,
        }
    }
}

impl SyncClock {
    /// Create a new sync clock initialized at a given reference time and position.
    pub fn new(reference_time: SystemTime, start_position: Duration) -> Self {
        Self {
            reference_time,
            start_position,
            speed_rate: 1.0,
        }
    }

    /// Calculate expected playback position at `now`.
    pub fn expected_position_at(&self, now: SystemTime) -> Duration {
        match now.duration_since(self.reference_time) {
            Ok(elapsed) => {
                let scaled_secs = elapsed.as_secs_f64() * self.speed_rate;
                self.start_position + Duration::from_secs_f64(scaled_secs)
            }
            Err(_) => self.start_position,
        }
    }

    /// Compute drift (signed) between reported position and expected position.
    /// Positive duration means reported position is ahead; negative means behind.
    pub fn calculate_drift(
        &self,
        reported_position: Duration,
        now: SystemTime,
    ) -> (Duration, bool) {
        let expected = self.expected_position_at(now);
        if reported_position >= expected {
            (reported_position - expected, true) // ahead
        } else {
            (expected - reported_position, false) // behind
        }
    }
}

/// A handle for multi-renderer sync management.
pub struct MultiRendererSync<T: AvTransport + ?Sized, R: RenderingControl + ?Sized> {
    /// Collection of renderers managed by this controller.
    renderers: Vec<(Arc<T>, Option<Arc<R>>)>,
    /// Playlist managed by the controller.
    pub playlist: Playlist,
    /// Shared reference sync clock.
    pub clock: SyncClock,
    /// Maximum acceptable drift before issuing an explicit seek correction.
    pub drift_tolerance: Duration,
}

impl<T: AvTransport + ?Sized, R: RenderingControl + ?Sized> MultiRendererSync<T, R> {
    /// Create a new controller given a list of renderers (AVTransport and optional RenderingControl).
    pub fn new(renderers: Vec<(Arc<T>, Option<Arc<R>>)>) -> Self {
        Self {
            renderers,
            playlist: Playlist::new(),
            clock: SyncClock::default(),
            drift_tolerance: Duration::from_millis(500),
        }
    }

    /// Add a renderer pair to the sync session.
    pub fn add_renderer(&mut self, transport: Arc<T>, control: Option<Arc<R>>) {
        self.renderers.push((transport, control));
    }

    /// Synchronized play call across all renderers.
    pub async fn play_sync(&mut self) -> Result<(), DlnaError> {
        let now = SystemTime::now();
        self.clock.reference_time = now;
        for (transport, _) in &self.renderers {
            transport.play().await?;
        }
        Ok(())
    }

    /// Synchronized stop call across all renderers.
    pub async fn stop_sync(&self) -> Result<(), DlnaError> {
        for (transport, _) in &self.renderers {
            transport.stop().await?;
        }
        Ok(())
    }

    /// Synchronized seek call across all renderers.
    pub async fn seek_sync(&mut self, target_position: Duration) -> Result<(), DlnaError> {
        self.clock.reference_time = SystemTime::now();
        self.clock.start_position = target_position;
        for (transport, _) in &self.renderers {
            transport.seek(target_position).await?;
        }
        Ok(())
    }

    /// Synchronized volume control across all renderers with volume support.
    pub async fn set_volume_sync(&self, volume: u8) -> Result<(), DlnaError> {
        for (_, rendering_control) in &self.renderers {
            if let Some(rc) = rendering_control {
                rc.set_volume(volume).await?;
            }
        }
        Ok(())
    }

    /// Load and play current playlist item across all renderers.
    pub async fn cast_playlist_current(&mut self) -> Result<(), DlnaError> {
        let Some(item) = self.playlist.current_item().cloned() else {
            return Err(DlnaError::SoapError("Playlist is empty".to_string()));
        };
        let didl = crate::metadata::build_didl_lite(&item.title, &item.url);
        self.clock.reference_time = SystemTime::now();
        self.clock.start_position = Duration::ZERO;

        for (transport, _) in &self.renderers {
            transport.set_uri(&item.url, &didl).await?;
            transport.play().await?;
        }
        Ok(())
    }

    /// Advance to next track in playlist and cast to all renderers.
    pub async fn next_track_sync(&mut self) -> Result<(), DlnaError> {
        if self.playlist.next().is_some() {
            self.cast_playlist_current().await
        } else {
            Err(DlnaError::SoapError("No next item in playlist".to_string()))
        }
    }

    /// Adjust renderers that have drifted past `drift_tolerance`.
    pub async fn resync_drift(&self) -> Result<usize, DlnaError> {
        let now = SystemTime::now();
        let expected = self.clock.expected_position_at(now);
        let mut corrected_count = 0;

        for (transport, _) in &self.renderers {
            if let Ok(pos_info) = transport.position_info().await {
                let (drift, _) = self.clock.calculate_drift(pos_info.rel_time, now);
                if drift > self.drift_tolerance {
                    transport.seek(expected).await?;
                    corrected_count += 1;
                }
            }
        }

        Ok(corrected_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soap::{PositionInfo, TransportInfo};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};

    #[test]
    fn playlist_navigation_works() {
        let mut playlist = Playlist::new();
        playlist.add(PlaylistItem::new("1", "Song A", "http://media/a.mp3"));
        playlist.add(PlaylistItem::new("2", "Song B", "http://media/b.mp3"));

        assert_eq!(playlist.current_item().unwrap().id, "1");
        assert_eq!(playlist.next().unwrap().id, "2");
        assert!(playlist.next().is_none()); // no loop by default
        assert_eq!(playlist.previous().unwrap().id, "1");

        playlist.loop_playlist = true;
        assert_eq!(playlist.previous().unwrap().id, "2"); // wraps to end

        // Test jump_to
        assert_eq!(playlist.jump_to(0).unwrap().id, "1");
        assert!(playlist.jump_to(10).is_none());
    }

    #[test]
    fn sync_clock_calculates_position_and_drift() {
        let start_time = SystemTime::now();
        let clock = SyncClock::new(start_time, Duration::from_secs(10));

        let future_time = start_time + Duration::from_secs(5);
        let expected = clock.expected_position_at(future_time);
        assert_eq!(expected, Duration::from_secs(15));

        // Reported 17s -> 2s ahead
        let (drift, ahead) = clock.calculate_drift(Duration::from_secs(17), future_time);
        assert_eq!(drift, Duration::from_secs(2));
        assert!(ahead);

        // Reported 13s -> 2s behind
        let (drift_behind, ahead_behind) =
            clock.calculate_drift(Duration::from_secs(13), future_time);
        assert_eq!(drift_behind, Duration::from_secs(2));
        assert!(!ahead_behind);
    }

    struct MockTransport {
        playing: AtomicBool,
        position_secs: AtomicU64,
        last_uri: std::sync::Mutex<Option<String>>,
    }

    impl MockTransport {
        fn new() -> Self {
            Self {
                playing: AtomicBool::new(false),
                position_secs: AtomicU64::new(0),
                last_uri: std::sync::Mutex::new(None),
            }
        }
    }

    #[async_trait]
    impl AvTransport for MockTransport {
        async fn set_uri(&self, uri: &str, _didl: &str) -> Result<(), DlnaError> {
            *self.last_uri.lock().unwrap() = Some(uri.to_string());
            Ok(())
        }
        async fn play(&self) -> Result<(), DlnaError> {
            self.playing.store(true, Ordering::SeqCst);
            Ok(())
        }
        async fn pause(&self) -> Result<(), DlnaError> {
            self.playing.store(false, Ordering::SeqCst);
            Ok(())
        }
        async fn stop(&self) -> Result<(), DlnaError> {
            self.playing.store(false, Ordering::SeqCst);
            Ok(())
        }
        async fn seek(&self, position: Duration) -> Result<(), DlnaError> {
            self.position_secs
                .store(position.as_secs(), Ordering::SeqCst);
            Ok(())
        }
        async fn transport_info(&self) -> Result<TransportInfo, DlnaError> {
            Ok(TransportInfo {
                state: if self.playing.load(Ordering::SeqCst) {
                    "PLAYING".to_string()
                } else {
                    "STOPPED".to_string()
                },
                status: Some("OK".to_string()),
                speed: Some("1".to_string()),
            })
        }
        async fn position_info(&self) -> Result<PositionInfo, DlnaError> {
            Ok(PositionInfo {
                rel_time: Duration::from_secs(self.position_secs.load(Ordering::SeqCst)),
                track_duration: Some(Duration::from_secs(300)),
                track_uri: self.last_uri.lock().unwrap().clone(),
            })
        }
    }

    struct MockControl {
        vol: AtomicU8,
    }

    #[async_trait]
    impl RenderingControl for MockControl {
        async fn volume(&self) -> Result<u8, DlnaError> {
            Ok(self.vol.load(Ordering::SeqCst))
        }
        async fn set_volume(&self, volume: u8) -> Result<(), DlnaError> {
            self.vol.store(volume, Ordering::SeqCst);
            Ok(())
        }
        async fn muted(&self) -> Result<bool, DlnaError> {
            Ok(false)
        }
        async fn set_muted(&self, _muted: bool) -> Result<(), DlnaError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn multi_renderer_sync_executes_coordinated_commands() {
        let t1 = Arc::new(MockTransport::new());
        let c1 = Arc::new(MockControl {
            vol: AtomicU8::new(20),
        });
        let t2 = Arc::new(MockTransport::new());
        let c2 = Arc::new(MockControl {
            vol: AtomicU8::new(20),
        });

        let mut sync = MultiRendererSync::new(vec![
            (t1.clone(), Some(c1.clone())),
            (t2.clone(), Some(c2.clone())),
        ]);

        sync.playlist.add(PlaylistItem::new(
            "track1",
            "Track 1",
            "http://media/track1.mp3",
        ));
        sync.cast_playlist_current().await.unwrap();

        assert!(t1.playing.load(Ordering::SeqCst));
        assert!(t2.playing.load(Ordering::SeqCst));

        sync.set_volume_sync(45).await.unwrap();
        assert_eq!(c1.vol.load(Ordering::SeqCst), 45);
        assert_eq!(c2.vol.load(Ordering::SeqCst), 45);

        // Simulate drift on t2
        t2.position_secs.store(100, Ordering::SeqCst); // very far off
        let corrected = sync.resync_drift().await.unwrap();
        assert_eq!(corrected, 1);

        sync.stop_sync().await.unwrap();
        assert!(!t1.playing.load(Ordering::SeqCst));
        assert!(!t2.playing.load(Ordering::SeqCst));
    }
}
