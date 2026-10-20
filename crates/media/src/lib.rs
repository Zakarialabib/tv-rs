//! `tv-rs-media` — a small HTTP media server for televisions.
//!
//! TVs do not play raw files out of a Rust process; they fetch them over HTTP
//! and they demand two behaviours:
//!
//! 1. **`206 Partial Content`.** When a viewer scrubs, the player sends
//!    `Range: bytes=N-` and refuses to seek unless the response is a `206` with
//!    a matching `Content-Range`.
//! 2. **HLS.** Native decoders prefer a `.m3u8` manifest of TS segments over a
//!    single MP4 because the decoder then owns chunking and bitrate choice.
//!
//! This crate is deliberately **separate from the protocol crates**: it pulls
//! `axum`/`tokio-util`, which no protocol crate should ever inherit. Only
//! binaries that actually serve media depend on it.
//!
//! # Example
//!
//! ```no_run
//! # async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//! use tv_rs_media::{serve, MediaServerConfig};
//!
//! let config = MediaServerConfig::new("/srv/media");
//! serve(config, "0.0.0.0:8088".parse()?).await?;
//! # Ok(())
//! # }
//! ```
//!
//! Serving a `206` slice by hand:
//!
//! ```
//! use tv_rs_media::parse_range;
//!
//! let range = parse_range("bytes=52428800-", 104_857_600).unwrap();
//! assert_eq!(range.start, 52_428_800);
//! assert_eq!(range.content_range(104_857_600), "bytes 52428800-104857599/104857600");
//! ```

#![forbid(unsafe_code)]
pub mod hls;
pub mod range;
pub mod server;

pub use hls::{split_duration, MediaPlaylist, Segment};
pub use range::{parse_range, ByteRange, RangeError};
pub use server::{router, serve, MediaServerConfig};
