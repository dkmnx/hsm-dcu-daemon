//! `dcu-serial` — async serial/UART/Unix/TCP/PTY transport for the DCU daemon.
//!
//! Provides a [`Transport`] trait and HDLC framing via [`FramedTransport`]
//! wrapping the `spinel::hdlc` codec. Concrete transports:
//!
//! * [`UartTransport`] — serial port via `tokio-serial`.
//! * [`TcpTransport`] — TCP socket via `tokio::net::TcpStream`.
//! * [`UnixSocketTransport`] — Unix domain socket via `tokio::net::UnixStream`.
//! * [`PtyTransport`] — PTY (for mock NCP testing).

pub mod dispatch;
pub mod error;
pub mod framing;
#[cfg(feature = "mock-pty")]
pub mod pty;
pub mod socket;
pub mod system;
pub mod tcp;
pub mod transport;
pub mod uart;

pub use error::SerialError;
pub use framing::FramedTransport;
#[cfg(feature = "mock-pty")]
pub use pty::{PtyPair, PtyTransport};
pub use socket::UnixSocketTransport;
pub use tcp::TcpTransport;
pub use transport::Transport;
pub use uart::{SerialConfig, UartTransport};

#[cfg(test)]
mod tests {
    use crate::SerialConfig;

    #[test]
    fn serial_config_defaults() {
        let config = SerialConfig::default();
        assert_eq!(config.baud_rate, 115200);
        assert_eq!(config.data_bits, 8);
        assert!(!config.flow_control);
        assert!(config.clocal);
        assert!(!config.ixon);
        assert!(!config.ixoff);
        assert!(!config.ixany);
    }
}
