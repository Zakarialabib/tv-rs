//! Observation stream and push state event vocabulary.
//!
//! Runtime-free and tokio-free: defines data types used across driver boundaries.

use crate::state::ObservedState;

/// An event emitted by a driver's observation channel.
#[derive(Debug, Clone, PartialEq)]
pub enum TvEvent {
    /// Device state has been updated or verified.
    StateChanged(ObservedState),
    /// Device connection status changed.
    ConnectionChanged(bool),
}

/// An observation event stream returned by [`Driver::observe`](crate::driver::Driver::observe).
#[derive(Debug, Default)]
pub struct ObservationStream {
    _private: (),
}

impl ObservationStream {
    /// Creates a new observation stream container.
    #[must_use]
    pub fn new() -> Self {
        Self { _private: () }
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
    }
}
