//! Core constants and value types for `tv-rs-power`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::PowerError;

/// Wake-on-LAN magic packets are delivered to UDP port 9 (the discard
/// protocol).
pub const WOL_UDP_PORT: u16 = 9;

/// Default IPv4 broadcast address used when the caller supplies none.
pub const WOL_BROADCAST_ADDR: &str = "255.255.255.255";

/// Total length of a magic packet in bytes: six sync bytes plus the MAC
/// address repeated sixteen times (`6 + 6 * 16`).
pub const MAGIC_PACKET_LEN: usize = 102;

/// Length of an Ethernet MAC address in bytes.
pub const MAC_ADDRESS_LEN: usize = 6;

/// A six-octet Ethernet MAC address.
///
/// Parsing accepts both colon-separated (`AA:BB:CC:DD:EE:FF`) and
/// hyphen-separated (`AA-BB-CC-DD-EE-FF`) forms in any case. [`Display`] and
/// serde always emit the canonical uppercase colon-separated form.
///
/// [`Display`]: fmt::Display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MacAddress([u8; MAC_ADDRESS_LEN]);

impl MacAddress {
    /// Borrows the raw six octets of the address.
    pub fn as_bytes(&self) -> &[u8; MAC_ADDRESS_LEN] {
        &self.0
    }

    /// Returns the raw six octets of the address by value.
    pub fn octets(&self) -> [u8; MAC_ADDRESS_LEN] {
        self.0
    }
}

impl fmt::Display for MacAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

impl FromStr for MacAddress {
    type Err = PowerError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let normalized = input.trim().replace('-', ":");
        let parts: Vec<&str> = normalized.split(':').collect();
        if parts.len() != MAC_ADDRESS_LEN {
            return Err(PowerError::InvalidMacAddress(input.to_string()));
        }

        let mut octets = [0u8; MAC_ADDRESS_LEN];
        for (index, part) in parts.iter().enumerate() {
            if part.len() != 2 || !part.chars().all(|digit| digit.is_ascii_hexdigit()) {
                return Err(PowerError::InvalidMacAddress(input.to_string()));
            }
            octets[index] = u8::from_str_radix(part, 16)
                .map_err(|_| PowerError::InvalidMacAddress(input.to_string()))?;
        }

        Ok(Self(octets))
    }
}

impl Serialize for MacAddress {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for MacAddress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

/// Observed power state of a TV, as inferred by [`crate::poll_power_state`].
///
/// The semantics implemented by the TCP connect probe are:
/// * [`PowerState::On`] — the connect succeeded, so the application stack is up.
/// * [`PowerState::Standby`] — the connection was refused: the network interface
///   answers but the application services are down (network-only standby, the
///   typical state for Wake-on-Wireless-LAN capable devices).
/// * [`PowerState::Off`] — the connect attempt timed out and nothing answered.
/// * [`PowerState::Unknown`] — the probe failed for any other reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerState {
    /// The main CPU and application services are awake.
    On,
    /// The network interface answers, but application services are down.
    Standby,
    /// Nothing answered within the probe timeout window.
    Off,
    /// The power state could not be determined.
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_str_accepts_colon_and_hyphen_forms_and_mixed_case() {
        let expected = MacAddress([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);

        assert_eq!("AA:BB:CC:DD:EE:FF".parse::<MacAddress>().unwrap(), expected);
        assert_eq!("aa:bb:cc:dd:ee:ff".parse::<MacAddress>().unwrap(), expected);
        assert_eq!("Aa-Bb-Cc-Dd-Ee-Ff".parse::<MacAddress>().unwrap(), expected);
        assert_eq!("AA-BB-CC-DD-EE-FF".parse::<MacAddress>().unwrap(), expected);
    }

    #[test]
    fn from_str_rejects_malformed_input() {
        assert!("AA:BB:CC:DD:EE".parse::<MacAddress>().is_err());
        assert!("GG:BB:CC:DD:EE:FF".parse::<MacAddress>().is_err());
        assert!("AABBCCDDEEFF".parse::<MacAddress>().is_err());
        assert!("AA:BB:CC:DD:EE:FG".parse::<MacAddress>().is_err());
        assert!("AA.BB.CC.DD.EE.FF".parse::<MacAddress>().is_err());
    }

    #[test]
    fn display_from_str_round_trip_is_canonical_uppercase() {
        let parsed: MacAddress = "aa-bb-cc-dd-ee-ff".parse().unwrap();
        let rendered = parsed.to_string();

        assert_eq!(rendered, "AA:BB:CC:DD:EE:FF");
        assert_eq!(rendered.parse::<MacAddress>().unwrap(), parsed);
    }
}
