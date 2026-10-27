//! Framing abstractions and codecs for serial byte streams.

use std::error::Error;
use std::fmt;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tv_rs_core::{ProtocolError, TvError};

/// Trait for encoding and decoding frames over a serial byte stream.
pub trait FrameCodec: Send + Sync {
    /// The decoded frame type.
    type Frame: Send + Sync + 'static;
    /// Error type produced during encoding or decoding.
    type Error: Error + Send + Sync + 'static;

    /// Encodes a frame into raw bytes and appends them to `buf`.
    fn encode(&self, frame: &Self::Frame, buf: &mut Vec<u8>) -> Result<(), Self::Error>;

    /// Decodes a single frame from `buf`.
    ///
    /// If a complete frame is decoded, removes the frame bytes from `buf` and returns `Ok(Some(frame))`.
    /// If more bytes are needed, leaves `buf` unmodified (or partially consumed if valid) and returns `Ok(None)`.
    fn decode(&self, buf: &mut Vec<u8>) -> Result<Option<Self::Frame>, Self::Error>;
}

/// Error type for line-based codec operations.
#[derive(Debug, thiserror::Error)]
pub enum LineCodecError {
    /// UTF-8 decoding error when parsing string lines.
    #[error("invalid utf-8 line: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
    /// Exceeded maximum configured frame line length.
    #[error("line length {length} exceeded maximum allowed length {max}")]
    LineTooLong {
        /// Actual line length.
        length: usize,
        /// Maximum allowed line length.
        max: usize,
    },
}

/// A codec that splits byte streams into line string frames using a delimiter (e.g., `\n` or `\r\n`).
#[derive(Debug, Clone)]
pub struct LineCodec {
    delimiter: u8,
    strip_carriage_return: bool,
    max_length: Option<usize>,
}

impl LineCodec {
    /// Creates a line codec with default `\n` delimiter and `\r` stripping enabled.
    #[must_use]
    pub fn new() -> Self {
        Self {
            delimiter: b'\n',
            strip_carriage_return: true,
            max_length: None,
        }
    }

    /// Customizes the delimiter byte (e.g., `b'\n'` or `b'\r'`).
    #[must_use]
    pub fn with_delimiter(mut self, delimiter: u8) -> Self {
        self.delimiter = delimiter;
        self
    }

    /// Sets whether to strip preceding `\r` carriage returns from lines.
    #[must_use]
    pub fn with_strip_carriage_return(mut self, strip: bool) -> Self {
        self.strip_carriage_return = strip;
        self
    }

    /// Sets an optional maximum byte limit per line.
    #[must_use]
    pub fn with_max_length(mut self, max: usize) -> Self {
        self.max_length = Some(max);
        self
    }
}

impl Default for LineCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameCodec for LineCodec {
    type Frame = String;
    type Error = LineCodecError;

    fn encode(&self, frame: &Self::Frame, buf: &mut Vec<u8>) -> Result<(), Self::Error> {
        buf.extend_from_slice(frame.as_bytes());
        if self.strip_carriage_return && self.delimiter == b'\n' {
            buf.push(b'\r');
        }
        buf.push(self.delimiter);
        Ok(())
    }

    fn decode(&self, buf: &mut Vec<u8>) -> Result<Option<Self::Frame>, Self::Error> {
        if let Some(pos) = buf.iter().position(|&b| b == self.delimiter) {
            if let Some(max) = self.max_length {
                if pos > max {
                    return Err(LineCodecError::LineTooLong { length: pos, max });
                }
            }

            let line_bytes = buf.drain(..=pos).collect::<Vec<u8>>();
            let mut line_slice = &line_bytes[..pos]; // Exclude delimiter

            if self.strip_carriage_return && line_slice.ends_with(b"\r") {
                line_slice = &line_slice[..line_slice.len() - 1];
            }

            let text = String::from_utf8(line_slice.to_vec())?;
            Ok(Some(text))
        } else {
            if let Some(max) = self.max_length {
                if buf.len() > max {
                    return Err(LineCodecError::LineTooLong {
                        length: buf.len(),
                        max,
                    });
                }
            }
            Ok(None)
        }
    }
}

/// A codec that splits byte streams into raw byte vector frames (`Vec<u8>`) using a delimiter.
#[derive(Debug, Clone)]
pub struct RawLineCodec {
    delimiter: u8,
    strip_carriage_return: bool,
    max_length: Option<usize>,
}

impl RawLineCodec {
    /// Creates a raw line codec with default `\n` delimiter and `\r` stripping enabled.
    #[must_use]
    pub fn new() -> Self {
        Self {
            delimiter: b'\n',
            strip_carriage_return: true,
            max_length: None,
        }
    }

    /// Customizes the delimiter byte (e.g., `b'\n'` or `b'\r'`).
    #[must_use]
    pub fn with_delimiter(mut self, delimiter: u8) -> Self {
        self.delimiter = delimiter;
        self
    }

    /// Sets whether to strip preceding `\r` carriage returns.
    #[must_use]
    pub fn with_strip_carriage_return(mut self, strip: bool) -> Self {
        self.strip_carriage_return = strip;
        self
    }

    /// Sets an optional maximum byte limit per frame.
    #[must_use]
    pub fn with_max_length(mut self, max: usize) -> Self {
        self.max_length = Some(max);
        self
    }
}

impl Default for RawLineCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameCodec for RawLineCodec {
    type Frame = Vec<u8>;
    type Error = LineCodecError;

    fn encode(&self, frame: &Self::Frame, buf: &mut Vec<u8>) -> Result<(), Self::Error> {
        buf.extend_from_slice(frame);
        if self.strip_carriage_return && self.delimiter == b'\n' {
            buf.push(b'\r');
        }
        buf.push(self.delimiter);
        Ok(())
    }

    fn decode(&self, buf: &mut Vec<u8>) -> Result<Option<Self::Frame>, Self::Error> {
        if let Some(pos) = buf.iter().position(|&b| b == self.delimiter) {
            if let Some(max) = self.max_length {
                if pos > max {
                    return Err(LineCodecError::LineTooLong { length: pos, max });
                }
            }

            let line_bytes = buf.drain(..=pos).collect::<Vec<u8>>();
            let mut line_slice = &line_bytes[..pos];

            if self.strip_carriage_return && line_slice.ends_with(b"\r") {
                line_slice = &line_slice[..line_slice.len() - 1];
            }

            Ok(Some(line_slice.to_vec()))
        } else {
            if let Some(max) = self.max_length {
                if buf.len() > max {
                    return Err(LineCodecError::LineTooLong {
                        length: buf.len(),
                        max,
                    });
                }
            }
            Ok(None)
        }
    }
}

/// A framed transport wrapping an underlying async stream `S` using a [`FrameCodec`].
pub struct FramedTransport<S, C> {
    stream: S,
    codec: C,
    read_buf: Vec<u8>,
    write_buf: Vec<u8>,
    read_chunk_size: usize,
}

impl<S, C> fmt::Debug for FramedTransport<S, C>
where
    S: fmt::Debug,
    C: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FramedTransport")
            .field("stream", &self.stream)
            .field("codec", &self.codec)
            .field("read_buf_len", &self.read_buf.len())
            .field("write_buf_len", &self.write_buf.len())
            .finish()
    }
}

impl<S, C> FramedTransport<S, C>
where
    S: AsyncRead + AsyncWrite + Unpin,
    C: FrameCodec,
{
    /// Creates a new framed transport around `stream` using `codec`.
    pub fn new(stream: S, codec: C) -> Self {
        Self {
            stream,
            codec,
            read_buf: Vec::with_capacity(1024),
            write_buf: Vec::with_capacity(1024),
            read_chunk_size: 512,
        }
    }

    /// Returns a mutable reference to the underlying stream.
    pub fn stream_mut(&mut self) -> &mut S {
        &mut self.stream
    }

    /// Consumes the transport, returning the underlying stream.
    pub fn into_inner(self) -> S {
        self.stream
    }

    /// Encodes and sends a frame over the stream.
    ///
    /// # Errors
    ///
    /// Returns [`TvError::Protocol`] if codec encoding fails, or [`TvError::Transport`] on IO failure.
    pub async fn send(&mut self, frame: &C::Frame) -> Result<(), TvError> {
        self.write_buf.clear();
        self.codec
            .encode(frame, &mut self.write_buf)
            .map_err(|e| TvError::Protocol(ProtocolError::Malformed(e.to_string())))?;

        self.stream
            .write_all(&self.write_buf)
            .await
            .map_err(|e| TvError::Transport(tv_rs_core::TransportError::from_io(&e)))?;

        self.stream
            .flush()
            .await
            .map_err(|e| TvError::Transport(tv_rs_core::TransportError::from_io(&e)))?;

        Ok(())
    }

    /// Reads and decodes the next frame from the stream.
    ///
    /// Returns `Ok(Some(frame))` when a complete frame is available, or `Ok(None)` on clean EOF.
    ///
    /// # Errors
    ///
    /// Returns [`TvError::Protocol`] if codec decoding fails, or [`TvError::Transport`] on IO failure.
    pub async fn next_frame(&mut self) -> Result<Option<C::Frame>, TvError> {
        loop {
            if let Some(frame) = self
                .codec
                .decode(&mut self.read_buf)
                .map_err(|e| TvError::Protocol(ProtocolError::Malformed(e.to_string())))?
            {
                return Ok(Some(frame));
            }

            let mut tmp = vec![0u8; self.read_chunk_size];
            let n = self
                .stream
                .read(&mut tmp)
                .await
                .map_err(|e| TvError::Transport(tv_rs_core::TransportError::from_io(&e)))?;

            if n == 0 {
                // Stream reached EOF. Try one last decode attempt.
                if let Some(frame) = self
                    .codec
                    .decode(&mut self.read_buf)
                    .map_err(|e| TvError::Protocol(ProtocolError::Malformed(e.to_string())))?
                {
                    return Ok(Some(frame));
                }
                return Ok(None);
            }

            self.read_buf.extend_from_slice(&tmp[..n]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_codec_encodes_and_decodes_cleanly() {
        let codec = LineCodec::new();
        let mut buf = Vec::new();

        codec.encode(&"hello world".to_string(), &mut buf).unwrap();
        assert_eq!(buf, b"hello world\r\n");

        let frame = codec.decode(&mut buf).unwrap();
        assert_eq!(frame, Some("hello world".to_string()));
        assert!(buf.is_empty());
    }

    #[test]
    fn line_codec_handles_partial_data() {
        let codec = LineCodec::new();
        let mut buf = Vec::from(&b"hello "[..]);

        let frame = codec.decode(&mut buf).unwrap();
        assert_eq!(frame, None);
        assert_eq!(buf, b"hello ");

        buf.extend_from_slice(b"world\n");
        let frame = codec.decode(&mut buf).unwrap();
        assert_eq!(frame, Some("hello world".to_string()));
        assert!(buf.is_empty());
    }

    #[test]
    fn raw_line_codec_encodes_and_decodes_cleanly() {
        let codec = RawLineCodec::new();
        let mut buf = Vec::new();

        codec
            .encode(&b"raw binary frame".to_vec(), &mut buf)
            .unwrap();
        assert_eq!(buf, b"raw binary frame\r\n");

        let frame = codec.decode(&mut buf).unwrap();
        assert_eq!(frame, Some(b"raw binary frame".to_vec()));
        assert!(buf.is_empty());
    }
}
