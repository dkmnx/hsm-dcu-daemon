//! `dcu-daemon` — HSM DCU Border Router daemon (Wi-SUN FAN).
//!
//! Replaces `src/dcud/*` and `src/ncp-spinel/*` with async Rust.
//! This is the **critical/call-in** crate — it wires the NCP state machine,
//! D-Bus server, TUN interface, and serial transport together.

pub mod config;
pub mod control_interface;
pub mod dispatcher;
pub mod error;
pub mod firmware_upgrade;
pub mod instance;
pub mod tasks;

pub use config::Config;
pub use error::DaemonError;
pub use instance::NcpInstance;
