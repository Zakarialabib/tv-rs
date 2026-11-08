//! Internal transport abstraction over TCP, RS-232 serial and test duplex streams.

use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;

#[cfg(feature = "serial")]
use tokio_serial::SerialStream;

pub(crate) enum Transport {
    Tcp(TcpStream),
    #[cfg(feature = "serial")]
    Serial(Box<SerialStream>),
    #[cfg(test)]
    Duplex(Box<tokio::io::DuplexStream>),
}

impl Transport {
    pub(crate) async fn shutdown(&mut self) -> std::io::Result<()> {
        match self {
            Self::Tcp(socket) => AsyncWriteExt::shutdown(socket).await,
            #[cfg(feature = "serial")]
            Self::Serial(stream) => AsyncWriteExt::shutdown(stream.as_mut()).await,
            #[cfg(test)]
            Self::Duplex(stream) => AsyncWriteExt::shutdown(stream.as_mut()).await,
        }
    }
}

use tokio::io::AsyncWriteExt;

impl AsyncRead for Transport {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match &mut *self {
            Self::Tcp(socket) => Pin::new(socket).poll_read(cx, buf),
            #[cfg(feature = "serial")]
            Self::Serial(stream) => Pin::new(stream.as_mut()).poll_read(cx, buf),
            #[cfg(test)]
            Self::Duplex(stream) => Pin::new(stream.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Transport {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match &mut *self {
            Self::Tcp(socket) => Pin::new(socket).poll_write(cx, buf),
            #[cfg(feature = "serial")]
            Self::Serial(stream) => Pin::new(stream.as_mut()).poll_write(cx, buf),
            #[cfg(test)]
            Self::Duplex(stream) => Pin::new(stream.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match &mut *self {
            Self::Tcp(socket) => Pin::new(socket).poll_flush(cx),
            #[cfg(feature = "serial")]
            Self::Serial(stream) => Pin::new(stream.as_mut()).poll_flush(cx),
            #[cfg(test)]
            Self::Duplex(stream) => Pin::new(stream.as_mut()).poll_flush(cx),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match &mut *self {
            Self::Tcp(socket) => Pin::new(socket).poll_shutdown(cx),
            #[cfg(feature = "serial")]
            Self::Serial(stream) => Pin::new(stream.as_mut()).poll_shutdown(cx),
            #[cfg(test)]
            Self::Duplex(stream) => Pin::new(stream.as_mut()).poll_shutdown(cx),
        }
    }
}
