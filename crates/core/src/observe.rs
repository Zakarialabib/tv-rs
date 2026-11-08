//! Observation stream and push state event vocabulary.
//!
//! Runtime-free and tokio-free: defines data types used across driver boundaries.
//! Strictly `no_std` + `alloc` compliant.

use alloc::boxed::Box;
use alloc::string::String;
use async_trait::async_trait;
use core::fmt;

use crate::capability::Input;
use crate::descriptor::PowerState;
use crate::state::ObservedState;

/// An event emitted by a driver's observation channel.
#[derive(Debug, Clone, PartialEq)]
pub enum TvEvent {
    /// Device state has been updated or verified.
    StateChanged(ObservedState),
    /// Device connection status changed.
    ConnectionChanged(bool),
    /// Device power state changed.
    PowerChanged(PowerState),
    /// Volume level or mute status changed.
    VolumeChanged {
        /// Absolute volume level (0-100).
        level: u8,
        /// Audio mute state.
        muted: bool,
    },
    /// Active foreground application changed.
    AppChanged(String),
    /// Active input source changed.
    InputChanged(Input),
    /// Media playback status changed (e.g., "PLAYING", "PAUSED", "IDLE").
    MediaPlaybackChanged(String),
    /// Device was disconnected.
    Disconnected,
}

/// Errors occurring during stream consumption.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ObservationError {
    /// Backpressure triggered: receiver lagged behind and dropped oldest events.
    #[error("observation receiver lagged behind by {0} messages")]
    Lagged(u64),
    /// The observation stream has closed.
    #[error("observation channel closed")]
    Closed,
    /// Protocol or transport error.
    #[error("observation error: {0}")]
    Custom(String),
}

/// Erased event receiver trait (implemented in `std` crates via tokio broadcast).
#[async_trait]
pub trait EventReceiver: Send + Sync {
    /// Asynchronously receives the next event from the stream.
    async fn recv(&mut self) -> Result<TvEvent, ObservationError>;
}

/// Receiver stub for empty or unsupported observation streams.
#[derive(Debug, Default, Clone, Copy)]
struct EmptyReceiver;

#[async_trait]
impl EventReceiver for EmptyReceiver {
    async fn recv(&mut self) -> Result<TvEvent, ObservationError> {
        Err(ObservationError::Closed)
    }
}

/// An observation event stream returned by [`Driver::observe`](crate::driver::Driver::observe).
pub struct ObservationStream {
    inner: Box<dyn EventReceiver>,
}

impl ObservationStream {
    /// Creates an empty observation stream (immediately returns `Closed`).
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Box::new(EmptyReceiver),
        }
    }

    /// Creates an observation stream backed by an [`EventReceiver`].
    pub fn from_receiver<R: EventReceiver + 'static>(receiver: R) -> Self {
        Self {
            inner: Box::new(receiver),
        }
    }

    /// Creates an observation stream from a boxed [`EventReceiver`].
    #[must_use]
    pub fn from_boxed(inner: Box<dyn EventReceiver>) -> Self {
        Self { inner }
    }

    /// Receives the next observation event asynchronously.
    pub async fn recv(&mut self) -> Result<TvEvent, ObservationError> {
        self.inner.recv().await
    }
}

impl Default for ObservationStream {
    fn default() -> Self {
        Self::new()
    }
}

/// Preserves compatibility with existing object-safety test assertions.
impl fmt::Debug for ObservationStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ObservationStream")
            .field("_private", &())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_stream_and_events() {
        let stream = ObservationStream::new();
        assert_eq!(format!("{stream:?}"), "ObservationStream { _private: () }");

        let event = TvEvent::ConnectionChanged(true);
        assert!(matches!(event, TvEvent::ConnectionChanged(true)));

        let pwr = TvEvent::PowerChanged(PowerState::On);
        assert!(matches!(pwr, TvEvent::PowerChanged(PowerState::On)));

        let vol = TvEvent::VolumeChanged {
            level: 42,
            muted: false,
        };
        assert!(matches!(
            vol,
            TvEvent::VolumeChanged {
                level: 42,
                muted: false
            }
        ));
    }

    #[test]
    fn empty_receiver_returns_closed() {
        futures::executor::block_on(async {
            let mut stream = ObservationStream::new();
            assert_eq!(stream.recv().await, Err(ObservationError::Closed));
        });
    }
}
