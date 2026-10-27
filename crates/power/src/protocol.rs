//! The [`PowerControl`] trait and its built-in implementation.

use async_trait::async_trait;

use crate::client::{poll_power_state, probe_power_state_with_confidence, WolSender};
use crate::error::Result;
use crate::types::{EvaluatedPowerState, MacAddress, PowerState};

/// A power-control transport able to wake a TV, sleep, and probe its power state.
#[async_trait]
pub trait PowerControl: Send + Sync {
    /// Wakes the device with `mac` using a Wake-on-LAN magic packet.
    async fn wake_on_lan(&self, mac: &MacAddress) -> Result<()>;

    /// Probes the power state of the TV listening at `addr` (`host:port`).
    async fn poll_power_state(&self, addr: &str) -> Result<PowerState>;

    /// Sends a wake signal to `mac` and returns the evaluated state and confidence score.
    async fn wake(&self, mac: &MacAddress) -> Result<EvaluatedPowerState> {
        self.wake_on_lan(mac).await?;
        Ok(EvaluatedPowerState::new(
            PowerState::On,
            crate::types::ConfidenceScore::Low,
            crate::types::PowerSignalSource::WolPacketSent,
        ))
    }

    /// Requests the device to enter sleep / power off mode.
    async fn sleep(&self) -> Result<EvaluatedPowerState> {
        Err(crate::error::PowerError::Unsupported(
            tv_rs_transport::Unsupported::new(1, "Sleep not supported on this power transport"),
        ))
    }

    /// Probes the target address and returns an evaluated power state with confidence scoring.
    async fn probe(&self, addr: &str) -> Result<EvaluatedPowerState> {
        probe_power_state_with_confidence(addr).await
    }
}

#[async_trait]
impl PowerControl for WolSender {
    async fn wake_on_lan(&self, mac: &MacAddress) -> Result<()> {
        self.wake(mac).await?;
        Ok(())
    }

    async fn poll_power_state(&self, addr: &str) -> Result<PowerState> {
        poll_power_state(addr).await
    }

    async fn wake(&self, mac: &MacAddress) -> Result<EvaluatedPowerState> {
        WolSender::wake(self, mac).await
    }

    async fn sleep(&self) -> Result<EvaluatedPowerState> {
        WolSender::sleep(self).await
    }

    async fn probe(&self, addr: &str) -> Result<EvaluatedPowerState> {
        WolSender::probe(self, addr).await
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

    #[tokio::test]
    async fn power_control_trait_wake_sleep_probe_delegation() {
        let sender = WolSender::new().unwrap();
        let mac: MacAddress = "11:22:33:44:55:66".parse().unwrap();

        let wake_eval = PowerControl::wake(&sender, &mac).await.unwrap();
        assert_eq!(wake_eval.state, PowerState::On);
        assert_eq!(wake_eval.confidence, crate::types::ConfidenceScore::Low);

        let sleep_res = PowerControl::sleep(&sender).await;
        assert!(sleep_res.is_err());

        let probe_err = PowerControl::probe(&sender, "invalid_addr").await;
        assert!(probe_err.is_err());
    }
}
