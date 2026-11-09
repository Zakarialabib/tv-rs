//! Tokio broadcast adapter connecting drivers to `tv-rs-core::observe::ObservationStream`.

use async_trait::async_trait;
use tokio::sync::broadcast;
use tv_rs_core::observe::{EventReceiver, ObservationError, ObservationStream, TvEvent};

/// Default capacity for driver broadcast channels (drop-oldest ring buffer).
pub const DEFAULT_OBSERVE_CHANNEL_CAPACITY: usize = 64;

/// Adapter implementing `EventReceiver` over a `tokio::sync::broadcast::Receiver`.
pub struct BroadcastReceiver {
    receiver: broadcast::Receiver<TvEvent>,
    _sender: Option<broadcast::Sender<TvEvent>>,
}

impl BroadcastReceiver {
    /// Wraps a tokio broadcast receiver.
    #[must_use]
    pub fn new(receiver: broadcast::Receiver<TvEvent>) -> Self {
        Self {
            receiver,
            _sender: None,
        }
    }

    /// Wraps a tokio broadcast receiver and retains a sender reference to keep the channel open.
    #[must_use]
    pub fn with_sender(
        receiver: broadcast::Receiver<TvEvent>,
        sender: broadcast::Sender<TvEvent>,
    ) -> Self {
        Self {
            receiver,
            _sender: Some(sender),
        }
    }

    /// Converts this receiver into an erased `ObservationStream`.
    #[must_use]
    pub fn into_stream(self) -> ObservationStream {
        ObservationStream::from_receiver(self)
    }
}

#[async_trait]
impl EventReceiver for BroadcastReceiver {
    async fn recv(&mut self) -> Result<TvEvent, ObservationError> {
        match self.receiver.recv().await {
            Ok(event) => Ok(event),
            Err(broadcast::error::RecvError::Lagged(count)) => Err(ObservationError::Lagged(count)),
            Err(broadcast::error::RecvError::Closed) => Err(ObservationError::Closed),
        }
    }
}
