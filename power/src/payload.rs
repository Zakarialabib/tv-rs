//! Request payloads emitted by the power protocols.

use crate::types::{MacAddress, MAGIC_PACKET_LEN, MAC_ADDRESS_LEN};

/// Number of leading `0xFF` synchronisation bytes in a magic packet.
const SYNC_PREFIX_LEN: usize = 6;

/// A Wake-on-LAN magic packet: six `0xFF` sync bytes followed by the target
/// MAC address repeated sixteen times, for a total of [`MAGIC_PACKET_LEN`]
/// bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MagicPacket([u8; MAGIC_PACKET_LEN]);

impl MagicPacket {
    /// Builds a magic packet targeting `mac`.
    pub fn build(mac: &MacAddress) -> Self {
        let mut bytes = [0u8; MAGIC_PACKET_LEN];
        bytes[..SYNC_PREFIX_LEN].fill(0xFF);

        let mac_bytes = mac.as_bytes();
        for chunk in bytes[SYNC_PREFIX_LEN..].chunks_exact_mut(MAC_ADDRESS_LEN) {
            chunk.copy_from_slice(mac_bytes);
        }

        Self(bytes)
    }

    /// Borrows the raw packet bytes.
    pub fn as_bytes(&self) -> &[u8; MAGIC_PACKET_LEN] {
        &self.0
    }

    /// Copies the packet into an owned byte vector.
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }
}
