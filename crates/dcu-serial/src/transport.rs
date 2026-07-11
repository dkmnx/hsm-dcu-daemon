//! Transport abstraction for NCP communication.

use std::os::unix::io::RawFd;
use tokio::io::{AsyncRead, AsyncWrite};

/// A transport layer that the daemon can read/write framed Spinel frames
/// over. Implementations: UART, Unix socket, PTY.
pub trait Transport: AsyncRead + AsyncWrite + Send + Unpin + 'static {
    /// Get the underlying file descriptor, if available (for event-loop
    /// integration / `AsyncFd` registration).
    fn raw_fd(&self) -> Option<RawFd> {
        None
    }

    /// Human-readable identifier for logging (e.g. `"UART:/dev/ttyUSB0@115200"`).
    fn info(&self) -> String;
}
