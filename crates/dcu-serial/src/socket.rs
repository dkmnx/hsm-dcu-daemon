//! Unix domain socket transport.

use std::os::unix::io::{AsRawFd, RawFd};
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::UnixStream;

use crate::error::SerialError;
use crate::transport::Transport;

/// A Unix socket transport matching the C `SuperSocket`/`UnixSocket` path
/// (used for `system:` prefixes in `Config:NCP:SocketPath`).
pub struct UnixSocketTransport {
    inner: UnixStream,
    path: String,
}

impl UnixSocketTransport {
    /// Connect to a Unix domain socket at the given path.
    pub async fn connect(path: impl Into<String>) -> Result<Self, SerialError> {
        let path = path.into();
        let inner = UnixStream::connect(&path).await?;
        Ok(Self { inner, path })
    }
}

impl Transport for UnixSocketTransport {
    fn raw_fd(&self) -> Option<RawFd> {
        Some(self.inner.as_raw_fd())
    }

    fn info(&self) -> String {
        format!("UnixSocket:{}", self.path)
    }
}

impl AsyncRead for UnixSocketTransport {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for UnixSocketTransport {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}
