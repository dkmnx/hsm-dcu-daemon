//! Transport abstraction for NCP communication.

use std::os::unix::io::RawFd;
use tokio::io::{AsyncRead, AsyncWrite};

/// A transport layer that the daemon can read/write framed Spinel frames
/// over. Implementations: UART, TCP, system (forkpty), Unix socket.
pub trait Transport: AsyncRead + AsyncWrite + Send + Unpin + 'static {
    /// Get the underlying file descriptor, if available (for event-loop
    /// integration / `AsyncFd` registration).
    fn raw_fd(&self) -> Option<RawFd> {
        None
    }

    /// Human-readable identifier for logging (e.g. `"UART:/dev/ttyUSB0@115200"`).
    fn info(&self) -> String;
}

/// Blanket impl: `Box<dyn Transport>` can be used wherever `T: Transport`
/// is required. This allows `dispatch::open_transport()` to return a boxed
/// trait object that works with `start_pumps_impl<T: Transport>`.
impl Transport for Box<dyn Transport> {
    fn raw_fd(&self) -> Option<RawFd> {
        self.as_ref().raw_fd()
    }

    fn info(&self) -> String {
        self.as_ref().info()
    }
}
