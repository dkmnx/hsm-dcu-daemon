//! Task module declarations for `dcu-daemon`.
//!
//! Three sub-modules:
//! - `backoff` — RunawayResetBackoffManager (windowed quadratic delay)
//! - `queue` — FIFO task queue over [`EventDrivenTask`]
//! - `send_command` — SendCommandTask for single-command + TID matching

pub mod backoff;
pub mod queue;
pub mod send_command;
