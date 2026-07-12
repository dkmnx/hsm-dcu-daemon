//! In-memory duplex transport and PTY transport pair for testing.
//!
//! `DuplexTransport` wraps `tokio::io::DuplexStream` to implement the local
//! [`Transport`] trait (needed because coherence prevents implementing a local
//! trait for a foreign type). `MockTransportPair` creates the daemon-side and
//! mock-side pair.

use dcu_serial::transport::Transport;

/// Default buffer size for the in-memory duplex stream (4 KB = one large
/// Spinel frame of ~2.5 KB plus overhead). Shared by the builder and any
/// future `MockTransportPair`.
pub const DUPLEX_BUFFER_SIZE: usize = 4096;
use tokio::io::{AsyncRead, AsyncWrite};
use std::pin::Pin;
use std::task::{Context, Poll};

/// Newtype wrapper around `tokio::io::DuplexStream` so it can implement the
/// local `Transport` trait (coherence).
#[derive(Debug)]
pub struct DuplexTransport(pub tokio::io::DuplexStream);

impl AsyncRead for DuplexTransport {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_read(cx, buf)
    }
}

impl AsyncWrite for DuplexTransport {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.get_mut().0).poll_write(cx, buf)
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_shutdown(cx)
    }
}

impl Transport for DuplexTransport {
    fn info(&self) -> String {
        "duplex-mock".to_string()
    }
}

/// Optional PTY-backed transport for integration tests.
#[cfg(feature = "pty")]
pub struct PtyTransportPair {
    pub daemon_slave_path: String,
    pub mock_side: dcu_serial::FramedTransport<dcu_serial::PtyTransport>,
}

#[cfg(feature = "pty")]
impl PtyTransportPair {
    pub fn create() -> Result<Self, crate::failure::MockError> {
        let pair = dcu_serial::PtyPair::open()?;
        Ok(Self {
            daemon_slave_path: pair.slave_path().to_string(),
            mock_side: dcu_serial::FramedTransport::new(dcu_serial::PtyTransport::from_pair(&pair)),
        })
    }
}
