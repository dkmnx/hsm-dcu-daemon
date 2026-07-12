//! Failure injection rules for mock NCP testing.

use std::time::Duration;

use thiserror::Error;

/// Errors returned by the mock NCP.
#[derive(Debug, Error)]
pub enum MockError {
    #[error("serial framing error: {0}")]
    Framing(String),
    #[error("invalid Spinel payload: {0}")]
    InvalidPayload(String),
    #[error("scenario timeout")]
    Timeout,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("mock NCP is disabled")]
    Disabled,
}

impl From<dcu_serial::SerialError> for MockError {
    fn from(e: dcu_serial::SerialError) -> Self {
        MockError::Framing(e.to_string())
    }
}

/// Failure injection rules applied to outbound frames before they are sent.
#[derive(Debug, Clone)]
pub enum FailureRule {
    /// Drop the next `n` outbound frames.
    DropFrames(u32),
    /// Corrupt the CRC on the `n`th outbound frame.
    CorruptCrc(u32),
    /// Delay the response to the `n`th command by `duration`.
    DelayResponse(u32, Duration),
    /// Reject a property get by returning a `LAST_STATUS(FAILURE)` response.
    RejectProperty { prop_key: u32 },
    /// Never respond to the given command ID, causing a daemon timeout.
    DropCommand { command_id: u32 },
}
