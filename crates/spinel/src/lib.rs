//! # spinel
//!
//! Spinel binary protocol library for TI Wi-SUN FAN NCP communication.
//!
//! Provides frame construction, pack format encoding/decoding,
//! and HDLC framing with CRC-16/X.25.

pub mod error;
pub mod frame;
pub mod hdlc;
pub mod pack;

// Re-exports
pub use error::SpinelError;
pub use frame::SpinelFrame;
pub use hdlc::{HdlcDecoder, HdlcEncoder};
pub use pack::{PackReader, PackWriter, SPINEL_MAX_UINT_PACKED};
