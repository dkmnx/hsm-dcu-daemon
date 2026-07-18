//! PTY transport for mock NCP testing.
//!
//! Reimplements the PTY pair used by the C `SpinelNCPInstance` test harness.
//! The slave end connects to the daemon as if it were a real serial port;
//! the master end is driven by the mock NCP.

use std::os::unix::io::{AsRawFd, RawFd};
use std::pin::Pin;
use std::task::{Context, Poll};

use portable_pty::native_pty_system;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::error::SerialError;
use crate::transport::Transport;

/// A pair of connected PTY endpoints. `slave_path` is a path in the
/// filesystem the daemon can open (e.g. `UartTransport`).
pub struct PtyPair {
    pub master: Box<dyn portable_pty::MasterPty + Send>,
    pub slave_path: String,
}

impl PtyPair {
    /// Create a new PTY pair.
    pub fn open() -> Result<Self, SerialError> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(portable_pty::PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| SerialError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let slave_path = pair.slave_pts_name().unwrap_or_else(|| "/dev/ptmx".into());

        Ok(Self {
            master: pair.master,
            slave_path,
        })
    }

    /// The slave device path for the daemon to open.
    pub fn slave_path(&self) -> &str {
        &self.slave_path
    }
}

/// Async PTY transport wrapping the master end of a PTY.
pub struct PtyTransport {
    reader: Box<dyn portable_pty::MasterPty + Send>,
    writer: Box<dyn portable_pty::MasterPty + Send>,
}

impl PtyTransport {
    /// Wrap a `PtyPair`'s master into a transport suitable for
    /// `FramedTransport`.
    pub fn from_pair(pair: &PtyPair) -> Self {
        Self {
            reader: pair
                .master
                .try_clone_master()
                .expect("PTY try_clone_master for reader failed"),
            writer: pair
                .master
                .try_clone_master()
                .expect("PTY try_clone_master for writer failed"),
        }
    }
}

impl Transport for PtyTransport {
    fn info(&self) -> String {
        "PTY(mock)".to_string()
    }
}

impl AsyncRead for PtyTransport {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // Stub: portable-pty does not expose async I/O natively.
        // The mock NCP should use a raw PTY fd with AsyncFd.
        Poll::Pending
    }
}

impl AsyncWrite for PtyTransport {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Poll::Pending
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
