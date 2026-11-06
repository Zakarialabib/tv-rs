//! Binary stream packet framing for video and audio data.

use crate::error::{MirrorError, Result};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use serde::{Deserialize, Serialize};

/// Maximum packet size (8 MiB = 8,388,608 bytes).
pub const MAX_PACKET_SIZE: usize = 8 * 1024 * 1024;

/// Packet kind discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum PacketKind {
    /// Format configuration (codec, width, height, or audio sample rate/channels).
    Config = 0,
    /// Inter-coded frame (P/B slice) depending on previous pictures.
    Frame = 1,
    /// Intra-coded keyframe (IDR/IRAP) starting a new sequence with SPS/PPS.
    Keyframe = 2,
}

impl PacketKind {
    /// Parses a byte into a `PacketKind`.
    pub fn from_u8(val: u8) -> Result<Self> {
        match val {
            0 => Ok(Self::Config),
            1 => Ok(Self::Frame),
            2 => Ok(Self::Keyframe),
            other => Err(MirrorError::UnknownPacketKind(other)),
        }
    }

    /// Returns the discriminator byte.
    #[inline]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Video codec identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodec {
    /// H.264 / AVC (Annex B).
    H264,
    /// H.265 / HEVC (Annex B).
    H265,
}

/// Video stream configuration metadata payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoConfigPayload {
    /// Video codec in use.
    pub codec: VideoCodec,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Associated session ID, present for app window sessions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<u32>,
}

/// Audio format identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioFormat {
    /// 16-bit little-endian interleaved linear PCM.
    PcmS16le,
}

/// Audio stream configuration metadata payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioConfigPayload {
    /// Audio codec/format.
    pub codec: AudioFormat,
    /// Sample rate in Hz (8,000 to 96,000 Hz).
    pub rate: u32,
    /// Channels (1 = mono, 2 = stereo).
    pub channels: u32,
}

/// Binary packet framing for mirroring video and audio streams.
///
/// Wire layout:
/// ```text
/// u32 length (big-endian; counts everything after it: 1 byte kind + 8 bytes time + data)
/// u8  kind   (0 = config, 1 = frame, 2 = keyframe)
/// u64 time   (capture timestamp in microseconds, big-endian)
/// bytes data
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamPacket {
    /// Packet kind: Config (0), Frame (1), or Keyframe (2).
    pub kind: PacketKind,
    /// Capture time in microseconds (big-endian 64-bit integer).
    pub time_us: u64,
    /// Raw payload bytes (configuration CBOR/JSON, Annex B video access unit, or PCM frame).
    pub data: Bytes,
}

impl StreamPacket {
    /// Header overhead in bytes: 1 byte kind + 8 bytes timestamp = 9 bytes.
    pub const HEADER_LEN: usize = 1 + 8;

    /// Creates a new `StreamPacket`.
    pub fn new(kind: PacketKind, time_us: u64, data: impl Into<Bytes>) -> Self {
        Self {
            kind,
            time_us,
            data: data.into(),
        }
    }

    /// Encodes the packet into binary wire format with length prefix.
    pub fn encode(&self) -> Result<Bytes> {
        let payload_len = Self::HEADER_LEN + self.data.len();
        if payload_len > MAX_PACKET_SIZE {
            return Err(MirrorError::PacketTooLarge {
                size: payload_len,
                limit: MAX_PACKET_SIZE,
            });
        }

        let total_size = 4 + payload_len;
        let mut buf = BytesMut::with_capacity(total_size);
        buf.put_u32(payload_len as u32);
        buf.put_u8(self.kind.as_u8());
        buf.put_u64(self.time_us);
        buf.put_slice(&self.data);

        Ok(buf.freeze())
    }

    /// Decodes a packet from a byte slice containing the length prefix and packet body.
    pub fn decode(mut src: &[u8]) -> Result<Self> {
        if src.len() < 4 {
            return Err(MirrorError::BufferTooShort {
                expected: 4,
                actual: src.len(),
            });
        }

        let length = (&src[..4]).get_u32() as usize;
        if length > MAX_PACKET_SIZE {
            return Err(MirrorError::PacketTooLarge {
                size: length,
                limit: MAX_PACKET_SIZE,
            });
        }

        if src.len() < 4 + length {
            return Err(MirrorError::BufferTooShort {
                expected: 4 + length,
                actual: src.len(),
            });
        }

        src = &src[4..4 + length];
        if src.len() < Self::HEADER_LEN {
            return Err(MirrorError::BufferTooShort {
                expected: Self::HEADER_LEN,
                actual: src.len(),
            });
        }

        let kind = PacketKind::from_u8(src[0])?;
        let time_us = (&src[1..9]).get_u64();
        let data = Bytes::copy_from_slice(&src[9..]);

        Ok(Self {
            kind,
            time_us,
            data,
        })
    }
}
