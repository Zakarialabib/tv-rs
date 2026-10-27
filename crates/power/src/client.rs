//! Wake-on-LAN sender and TCP power-state probe.

use std::io::ErrorKind;
use std::net::SocketAddr;
use std::time::Duration;

use tokio::net::{TcpStream, UdpSocket};
use tokio::time::timeout;

use crate::error::{PowerError, Result};
use crate::payload::MagicPacket;
use crate::types::{
    ConfidenceScore, EvaluatedPowerState, MacAddress, PowerSignalSource, PowerState,
    WOL_BROADCAST_ADDR, WOL_UDP_PORT,
};

/// Maximum time to wait for a TCP connect probe before declaring the device
/// off.
const POWER_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// Builds a Wake-on-LAN magic packet for `mac`.
///
/// The packet is six `0xFF` sync bytes followed by the MAC address repeated
/// sixteen times (102 bytes total).
pub fn build_magic_packet(mac: &MacAddress) -> MagicPacket {
    MagicPacket::build(mac)
}

/// Sends Wake-on-LAN magic packets to a configurable broadcast address.
#[derive(Debug, Clone)]
pub struct WolSender {
    broadcast_addr: SocketAddr,
}

impl WolSender {
    /// Creates a sender targeting the default broadcast address
    /// (`255.255.255.255:9`).
    pub fn new() -> Result<Self> {
        let default = format!("{WOL_BROADCAST_ADDR}:{WOL_UDP_PORT}");
        let broadcast_addr = default
            .parse::<SocketAddr>()
            .map_err(|_| PowerError::InvalidBroadcastAddress(default))?;
        Ok(Self { broadcast_addr })
    }

    /// Creates a sender that overrides the default broadcast address.
    pub fn with_broadcast_addr(broadcast_addr: SocketAddr) -> Self {
        Self { broadcast_addr }
    }

    /// Returns the configured broadcast address.
    pub fn broadcast_addr(&self) -> SocketAddr {
        self.broadcast_addr
    }

    /// Wakes the device with `mac` by broadcasting a magic packet.
    ///
    /// Binds an ephemeral UDP socket to `0.0.0.0:0`, enables broadcast, and
    /// sends the packet to the configured broadcast address.
    pub async fn wake(&self, mac: &MacAddress) -> Result<EvaluatedPowerState> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.set_broadcast(true)?;
        let packet = build_magic_packet(mac);
        socket
            .send_to(packet.as_bytes(), self.broadcast_addr)
            .await?;
        Ok(EvaluatedPowerState::new(
            PowerState::On,
            ConfidenceScore::Low,
            PowerSignalSource::WolPacketSent,
        ))
    }

    /// Sleep / power off request for Wake-on-LAN sender.
    ///
    /// Sending a Wake-on-LAN magic packet cannot put a device to sleep over network broadcast.
    /// Returns unsupported error.
    pub async fn sleep(&self) -> Result<EvaluatedPowerState> {
        Err(PowerError::Unsupported(tv_rs_transport::Unsupported::new(
            1,
            "Sleep command over Wake-on-LAN broadcast is not supported",
        )))
    }

    /// Probes the device power state using TCP connect probe.
    pub async fn probe(&self, addr: &str) -> Result<EvaluatedPowerState> {
        probe_power_state_with_confidence(addr).await
    }
}

/// Wakes a device by broadcasting a magic packet to the default address.
pub async fn wake_on_lan(mac: &MacAddress) -> Result<()> {
    WolSender::new()?.wake(mac).await?;
    Ok(())
}

/// Probes the power state of a TV listening at `addr` (`host:port`, for
/// example a Tizen remote port or the Roku ECP port) using a TCP connect with
/// a 2 second timeout.
///
/// Semantics:
/// * a successful connect yields [`PowerState::On`];
/// * a refused connection yields [`PowerState::Standby`] — the network
///   interface answers but the application services are down;
/// * a timeout yields [`PowerState::Off`];
/// * any other failure yields [`PowerState::Unknown`].
///
/// # Errors
/// Returns [`PowerError::InvalidBroadcastAddress`] when `addr` is not a
/// syntactically valid `host:port` pair.
pub async fn poll_power_state(addr: &str) -> Result<PowerState> {
    let target = addr.trim();
    validate_probe_target(target)?;

    match timeout(POWER_PROBE_TIMEOUT, TcpStream::connect(target)).await {
        Ok(Ok(_stream)) => Ok(PowerState::On),
        Ok(Err(error)) if error.kind() == ErrorKind::ConnectionRefused => Ok(PowerState::Standby),
        Ok(Err(_other)) => Ok(PowerState::Unknown),
        Err(_elapsed) => Ok(PowerState::Off),
    }
}

/// Probes the power state of a TV listening at `addr` (`host:port`) and returns
/// an [`EvaluatedPowerState`] with confidence scoring.
///
/// Semantics:
/// * `PowerState::On` -> `ConfidenceScore::Medium` (TCP port answered)
/// * `PowerState::Standby` -> `ConfidenceScore::Medium` (TCP connect refused by host)
/// * `PowerState::Off` -> `ConfidenceScore::Low` (connection timed out)
/// * `PowerState::Unknown` -> `ConfidenceScore::Unknown`
pub async fn probe_power_state_with_confidence(addr: &str) -> Result<EvaluatedPowerState> {
    let state = poll_power_state(addr).await?;
    let confidence = match state {
        PowerState::On | PowerState::Standby => ConfidenceScore::Medium,
        PowerState::Off => ConfidenceScore::Low,
        PowerState::Unknown => ConfidenceScore::Unknown,
    };
    Ok(EvaluatedPowerState::new(
        state,
        confidence,
        PowerSignalSource::TcpProbe,
    ))
}

/// Validates the syntactic shape of a `host:port` probe target.
fn validate_probe_target(addr: &str) -> Result<()> {
    let Some((host, port)) = addr.rsplit_once(':') else {
        return Err(PowerError::InvalidBroadcastAddress(addr.to_string()));
    };
    if host.is_empty() || port.parse::<u16>().is_err() {
        return Err(PowerError::InvalidBroadcastAddress(addr.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MAC_ADDRESS_LEN, MAGIC_PACKET_LEN};

    #[test]
    fn build_magic_packet_has_sync_prefix_and_repeated_mac() {
        let mac: MacAddress = "AA:BB:CC:DD:EE:FF".parse().unwrap();
        let packet = build_magic_packet(&mac);
        let bytes = packet.as_bytes();

        assert_eq!(bytes.len(), MAGIC_PACKET_LEN);
        assert_eq!(&bytes[..6], &[0xFF; 6]);

        let mac_bytes = mac.as_bytes();
        let mut blocks = 0;
        for chunk in bytes[6..].as_chunks::<MAC_ADDRESS_LEN>().0.iter() {
            assert_eq!(chunk, mac_bytes);
            blocks += 1;
        }
        assert_eq!(blocks, 16);
    }

    #[test]
    fn magic_packet_to_vec_len_matches_constant() {
        let mac: MacAddress = "AA:BB:CC:DD:EE:FF".parse().unwrap();
        let packet = build_magic_packet(&mac);

        assert_eq!(packet.to_vec().len(), MAGIC_PACKET_LEN);
    }

    #[test]
    fn wol_sender_defaults_to_global_broadcast_port_nine() {
        let sender = WolSender::new().unwrap();

        assert_eq!(
            sender.broadcast_addr(),
            "255.255.255.255:9".parse::<SocketAddr>().unwrap()
        );
    }

    #[test]
    fn wol_sender_with_broadcast_addr_overrides_default() {
        let custom: SocketAddr = "192.168.1.255:7".parse().unwrap();
        let sender = WolSender::with_broadcast_addr(custom);

        assert_eq!(sender.broadcast_addr(), custom);
    }

    #[test]
    fn validate_probe_target_accepts_host_port_and_rejects_junk() {
        assert!(validate_probe_target("192.168.1.10:8002").is_ok());
        assert!(validate_probe_target("roku.local:8060").is_ok());
        assert!(validate_probe_target("192.168.1.10").is_err());
        assert!(validate_probe_target("192.168.1.10:notaport").is_err());
        assert!(validate_probe_target(":8002").is_err());
    }

    #[tokio::test]
    async fn wol_sender_sleep_returns_unsupported() {
        let sender = WolSender::new().unwrap();
        assert!(sender.sleep().await.is_err());
    }

    #[tokio::test]
    async fn wol_sender_wake_returns_low_confidence_evaluated_state() {
        let sender = WolSender::new().unwrap();
        let mac: MacAddress = "AA:BB:CC:DD:EE:FF".parse().unwrap();
        let evaluated = sender.wake(&mac).await.unwrap();

        assert_eq!(evaluated.state, PowerState::On);
        assert_eq!(evaluated.confidence, ConfidenceScore::Low);
        assert_eq!(evaluated.source, PowerSignalSource::WolPacketSent);
    }
}
