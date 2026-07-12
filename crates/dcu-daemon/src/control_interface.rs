//! D-Bus command dispatch — maps `dcu_dbus::commands::Command` to
//! `NcpInstanceBase` operations.
//!
//! The D-Bus server only *produces* commands (D-Bus serialization stays in
//! `dcu-dbus`); this module *executes* them against the instance.
//! The primary entry point is [`NcpInstanceBase::handle_command`] in
//! `instance/base.rs`; this module exists as an extension point for complex
//! multi-step command flows.

use crate::DaemonError;
use dcu_dbus::commands::Command;

/// Validate and prepare a command before dispatching it to the instance.
/// Returns `Err` if the command cannot be run in the current state.
pub fn validate_command(_cmd: &Command) -> Result<(), DaemonError> {
    // TODO: state-dependent validation (e.g. reject Form if already
    // associated).
    Ok(())
}
