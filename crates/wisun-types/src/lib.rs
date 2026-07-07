//! # wisun-types
//!
//! Foundational type library for the Wi-SUN FAN daemon Rust port.
//!
//! Maps every C `#define` and enum from the original `wpantund` daemon
//! into Rust types, constants, and enumerations. All downstream crates
//! in the workspace depend on this crate.

mod constants;
mod error;
mod ncp_state;
mod network_config;
mod property_key;

/// Spinel protocol command IDs.
pub mod command;

// Re-exports
pub use error::{NCP_ERROR_BASE, NCP_ERROR_END, NCP_ERROR_MASK, WpanError};
pub use ncp_state::NcpState;
pub use ncp_state::ParseNcpStateError;
pub use network_config::ChannelMask;
pub use network_config::Eui64;
pub use network_config::Ipv6Address;
pub use network_config::NetworkName;
pub use network_config::PanId;
pub use network_config::XPanId;
pub use property_key::*;

// Re-export constants at crate level for convenience.
pub use constants::*;
