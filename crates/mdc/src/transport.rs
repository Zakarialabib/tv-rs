//! The byte channel MDC runs over.
//!
//! MDC is one frame format on two wires: TCP 1515 on networked panels, RS-232 on
//! wired video walls. The framing, the checksum, the display-id addressing and the
//! request/response loop are identical on both — a video wall wired over serial is
//! the same protocol with a different carrier — so they live once in
//! [`crate::protocol`] and [`crate::client`], and the only thing that differs is this
//! enum. That is the "one codec, two carriers" shape: nothing is copied, and a fix to
//! the framing cannot land on one path and miss the other.
//!
//! `serial` is off by default. A cloud agent that only ever talks to networked panels
//! should not compile a serial stack, and a deployment that wires RS-232 turns the
//! feature on and gets the same client.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf};

/// One open MDC channel.
pub(crate) enum Transport {
    /// A TCP connection to a panel's control port.
    Tcp(tokio::net::TcpStream),
    /// An RS-232 line.
    #[cfg(feature = "serial")]
    Serial(Box<tokio_serial::SerialStream>),
    /// An in-memory duplex, used by this crate's tests to prove the request loop is
    /// not TCP-specific: the same frames, the same parser, no socket.
    #[cfg(test)]
    Duplex(Box<tokio::io::DuplexStream>),
}

impl Transport {
    /// Closes the channel.
    pub(crate) async fn shutdown(&mut self) -> io::Result<()> {
        match self {
            Self::Tcp(stream) => stream.shutdown().await,
            #[cfg(feature = "serial")]
            Self::Serial(stream) => stream.shutdown().await,
            #[cfg(test)]
            Self::Duplex(stream) => stream.shutdown().await,
        }
    }
}

impl AsyncRead for Transport {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Tcp(stream) => Pin::new(stream).poll_read(cx, buf),
            #[cfg(feature = "serial")]
            Self::Serial(stream) => Pin::new(stream.as_mut()).poll_read(cx, buf),
            #[cfg(test)]
            Self::Duplex(stream) => Pin::new(stream.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Transport {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Tcp(stream) => Pin::new(stream).poll_write(cx, buf),
            #[cfg(feature = "serial")]
            Self::Serial(stream) => Pin::new(stream.as_mut()).poll_write(cx, buf),
            #[cfg(test)]
            Self::Duplex(stream) => Pin::new(stream.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Tcp(stream) => Pin::new(stream).poll_flush(cx),
            #[cfg(feature = "serial")]
            Self::Serial(stream) => Pin::new(stream.as_mut()).poll_flush(cx),
            #[cfg(test)]
            Self::Duplex(stream) => Pin::new(stream.as_mut()).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Tcp(stream) => Pin::new(stream).poll_shutdown(cx),
            #[cfg(feature = "serial")]
            Self::Serial(stream) => Pin::new(stream.as_mut()).poll_shutdown(cx),
            #[cfg(test)]
            Self::Duplex(stream) => Pin::new(stream.as_mut()).poll_shutdown(cx),
        }
    }
}
