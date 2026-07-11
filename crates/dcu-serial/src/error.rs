//! Error type for the `dcu-serial` crate.

/// Errors from the serial/UART transport layer.
#[derive(Debug, thiserror::Error)]
pub enum SerialError {
    /// Serial port open or configuration failed.
    #[error("serial port: {0}")]
    Open(#[from] tokio_serial::Error),
    /// Underlying I/O error.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// Spinel protocol error (from the codec).
    #[error("spinel: {0}")]
    Spinel(#[from] spinel::SpinelError),
    /// HDLC or transport framing violation.
    #[error("framing: {0}")]
    Framing(String),
    /// Invalid configuration.
    #[error("invalid config: {0}")]
    InvalidConfig(String),
}
