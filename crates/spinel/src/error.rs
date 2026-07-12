use std::fmt;

/// Errors that can occur during Spinel frame encoding/decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpinelError {
    /// Not enough data to decode the requested field.
    Underflow,
    /// Invalid UTF-8 string encountered.
    InvalidUtf8,
    /// Packed uint exceeds maximum value (2,097,151).
    InvalidPackedUint,
    /// Invalid escape sequence in HDLC stream.
    InvalidEscape,
    /// CRC check failed on received frame.
    CrcMismatch,
    /// Frame too large.
    FrameTooLarge,
    /// Invalid header byte (missing FLAG).
    InvalidHeader,
    /// Invalid value (e.g., wrong prefix length).
    InvalidValue,
}

impl fmt::Display for SpinelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpinelError::Underflow => f.write_str("not enough data"),
            SpinelError::InvalidUtf8 => f.write_str("invalid UTF-8 string"),
            SpinelError::InvalidPackedUint => f.write_str("packed uint exceeds max value"),
            SpinelError::InvalidEscape => f.write_str("invalid HDLC escape sequence"),
            SpinelError::CrcMismatch => f.write_str("CRC mismatch"),
            SpinelError::FrameTooLarge => f.write_str("frame too large"),
            SpinelError::InvalidHeader => f.write_str("invalid header byte"),
            SpinelError::InvalidValue => f.write_str("invalid value"),
        }
    }
}

impl std::error::Error for SpinelError {}
