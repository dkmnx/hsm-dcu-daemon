//! # spinel
//!
//! Spinel binary protocol library for TI Wi-SUN FAN NCP communication.
//!
//! Provides frame construction, pack format encoding/decoding,
//! and HDLC framing with CRC-16/X.25.

pub mod command;
pub mod error;
pub mod frame;
pub mod hdlc;
pub mod pack;
pub mod property;
pub mod vendor;

// Re-exports
pub use command::{CMD_PROP_VALUE_GET, CMD_PROP_VALUE_IS, CMD_PROP_VALUE_SET};
pub use error::SpinelError;
pub use frame::SpinelFrame;
pub use hdlc::{HdlcDecoder, HdlcEncoder};
pub use pack::{PackReader, PackWriter, SPINEL_MAX_UINT_PACKED};
pub use property::{PackFormat, PackFormatError, PackValue};
pub use vendor::{Ch0CenterFreq, ChannelList, ChannelSpacing};
