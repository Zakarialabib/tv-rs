//! The [`PowerControl`] trait and its built-in implementation.

use async_trait::async_trait;

use crate::client::{poll_power_state, WolSender};
use crate::error::Result;
use crate::types::{MacAddress, PowerState};

/// A power-control transport able to wake a TV and probe its power state.
#[async_trait]
pub trait PowerControl: Send + Sync {
    /// Wakes the device with `mac` using a Wake-on-LAN magic packet.
    async fn wake_on_lan(&self, mac: &MacAddress) -> Result<()>;

    /// Probes the power state of the TV listening at `addr` (`host:port`).
    async fn poll_power_state(&self, addr: &str) -> Result<PowerState>;
}

#[async_trait]
impl PowerControl for WolSender {
    async fn wake_on_lan(&self, mac: &MacAddress) -> Result<()> {
        self.wake(mac).await
    }

    async fn poll_power_state(&self, addr: &str) -> Result<PowerState> {
        poll_power_state(addr).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn wol_sender_implements_power_control_probe_validation() {
        let sender = WolSender::new().unwrap();
        // Malformed input must surface a typed error rather than panicking.
        assert!(sender.poll_power_state("192.168.1.10").await.is_err());
    }
}
