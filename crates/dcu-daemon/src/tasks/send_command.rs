//! Command dispatch — send a Spinel frame and await the response by TID.
//!
//! This module provides the [`send_command`] helper used by async NCP tasks.
//! The TID lifecycle (allocate → register → send → deliver → unregister)
//! is handled by [`NcpInstanceBase::send_command`], which lives on the
//! instance for ergonomic access from task functions.
//!
//! Standalone utility: if you need to send a command outside the instance
//! event loop, use [`ResponseTable`] directly.

/// Status code for successful completion (matches `SPINEL_STATUS_OK = 0`).
pub const STATUS_OK: i32 = 0;
/// Status code for timeout.
pub const STATUS_TIMEOUT: i32 = -1;
