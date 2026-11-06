//! `tv-rs-mirror` — a video session vocabulary, not a video pipeline.
//!
//! As stated in `NON_GOALS.md` and the workspace README, this crate does not
//! decode video, render anything, or transcode. Instead, it provides the pure
//! session vocabulary and framing contracts adopted from NectarLink's
//! `docs/protocol/mirror.md`.
//!
//! This enables a phone or edge box to act as a session source for a display
//! panel that lacks an integrated network media player.
//!
//! # Core Concepts
//!
//! - **Numbered Sessions**: Session `0` designates the full screen; any `n > 0`
//!   designates an isolated app window. A missing session field defaults to `0`,
//!   ensuring older peers work seamlessly.
//! - **Control Messages**: Strongly typed messages for `mirror.start`, `mirror.stop`,
//!   `mirror.power`, `mirror.keyframe`, `mirror.resize`, `mirror.apps`,
//!   `mirror.sound`, and `mirror.input`.
//! - **Packet Framing**: Exact binary wire framing for video and audio packets
//!   with 4-byte big-endian payload length, 1-byte packet kind (`config`, `frame`,
//!   `keyframe`), and 8-byte big-endian capture timestamp in microseconds.
//!
//! # Example
//!
//! ```
//! use tv_rs_mirror::{MirrorStart, MirrorSessionId, PacketKind, StreamPacket};
//!
//! // Create a mirror.start request targeting the main screen (session 0).
//! let start = MirrorStart::builder(1920, 60, 8_000_000)
//!     .with_audio(true)
//!     .with_stay_awake(true)
//!     .build();
//!
//! assert_eq!(start.session, MirrorSessionId::SCREEN);
//!
//! // Construct a keyframe packet.
//! let raw_keyframe = vec![0x00, 0x00, 0x00, 0x01, 0x67]; // Annex B SPS/IDR snippet
//! let packet = StreamPacket::new(PacketKind::Keyframe, 1_000_000, raw_keyframe);
//! let encoded = packet.encode().expect("packet within 8 MiB limit");
//!
//! let decoded = StreamPacket::decode(&encoded).expect("valid stream packet");
//! assert_eq!(decoded.kind, PacketKind::Keyframe);
//! assert_eq!(decoded.time_us, 1_000_000);
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod error;
pub mod input;
pub mod message;
pub mod packet;
pub mod session;

pub use error::{MirrorError, Result};
pub use input::{GamepadInput, InputEvent, KeyAction, ScrollInput, TouchAction, TouchInput};
pub use message::{
    AppInfo, AppListPayload, MirrorAppsQuery, MirrorAppsResponse, MirrorKeyframe, MirrorPower,
    MirrorResize, MirrorSound, MirrorStart, MirrorStartBuilder, MirrorStop,
};
pub use packet::{
    AudioConfigPayload, AudioFormat, PacketKind, StreamPacket, VideoCodec, VideoConfigPayload,
    MAX_PACKET_SIZE,
};
pub use session::{MirrorSession, MirrorSessionId, SessionState};
