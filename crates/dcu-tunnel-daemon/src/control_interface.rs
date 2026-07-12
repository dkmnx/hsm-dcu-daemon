//! D-Bus command validation.
//!
//! State-dependent guards that reject commands when the NCP is in a
//! state that makes them invalid. Called before command dispatch.

use crate::DaemonError;
use dcu_dbus::commands::Command;
use wisun_types::NcpState;

/// Validate a command against the current NCP state.
///
/// Returns `Ok(())` if the command is allowed, or `Err` with a
/// descriptive message if the NCP state prohibits it.
pub fn validate_command(cmd: &Command, ncp_state: NcpState) -> Result<(), DaemonError> {
    match cmd {
        // Form/Join: must be Offline or Fault (not already associated)
        Command::Form { .. } | Command::Join { .. } => {
            if !matches!(ncp_state, NcpState::Offline | NcpState::Fault) {
                return Err(DaemonError::Ncp(format!(
                    "cannot {} in state {ncp_state}",
                    match cmd {
                        Command::Form { .. } => "form",
                        Command::Join { .. } => "join",
                        _ => unreachable!(),
                    }
                )));
            }
        }

        // Leave: must be in a joined/associated state
        Command::Leave => {
            if !matches!(
                ncp_state,
                NcpState::Associated
                    | NcpState::Isolated
                    | NcpState::NetWakeWaking
                    | NcpState::NetWakeAsleep
                    | NcpState::Commissioned
            ) {
                return Err(DaemonError::Ncp(format!(
                    "cannot leave in state {ncp_state}"
                )));
            }
        }

        // Attach: must be Offline
        Command::Attach => {
            if !matches!(ncp_state, NcpState::Offline) {
                return Err(DaemonError::Ncp(format!(
                    "cannot attach in state {ncp_state}"
                )));
            }
        }

        // BeginLowPower: must be associated
        Command::BeginLowPower => {
            if !ncp_state.is_associated() {
                return Err(DaemonError::Ncp(format!(
                    "cannot enter low-power in state {ncp_state}"
                )));
            }
        }

        // Scan: must not be initializing
        Command::NetScanStart { .. }
        | Command::DiscoverScanStart { .. }
        | Command::EnergyScanStart { .. }
            if ncp_state.is_initializing() =>
        {
            return Err(DaemonError::Ncp(
                "cannot scan while initializing".to_string(),
            ));
        }

        // Reset, HostDidWake, DataPoll, Peek, Poke, Mfg, SetProperty,
        // GetProperty, ConfigGateway, passthrough commands — always allowed
        _ => {}
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_rejected_when_associated() {
        let cmd = Command::Form {
            params: Default::default(),
        };
        assert!(validate_command(&cmd, NcpState::Associated).is_err());
    }

    #[test]
    fn form_allowed_when_offline() {
        let cmd = Command::Form {
            params: Default::default(),
        };
        assert!(validate_command(&cmd, NcpState::Offline).is_ok());
    }

    #[test]
    fn form_allowed_when_fault() {
        let cmd = Command::Form {
            params: Default::default(),
        };
        assert!(validate_command(&cmd, NcpState::Fault).is_ok());
    }

    #[test]
    fn leave_rejected_when_offline() {
        assert!(validate_command(&Command::Leave, NcpState::Offline).is_err());
    }

    #[test]
    fn leave_allowed_when_associated() {
        assert!(validate_command(&Command::Leave, NcpState::Associated).is_ok());
    }

    #[test]
    fn leave_allowed_when_isolated() {
        assert!(validate_command(&Command::Leave, NcpState::Isolated).is_ok());
    }

    #[test]
    fn reset_always_allowed() {
        assert!(validate_command(&Command::Reset, NcpState::Associated).is_ok());
        assert!(validate_command(&Command::Reset, NcpState::Offline).is_ok());
        assert!(validate_command(&Command::Reset, NcpState::Fault).is_ok());
    }

    #[test]
    fn scan_rejected_when_initializing() {
        let cmd = Command::NetScanStart {
            params: Default::default(),
        };
        assert!(validate_command(&cmd, NcpState::Uninitialized).is_err());
    }

    #[test]
    fn scan_allowed_when_offline() {
        let cmd = Command::NetScanStart {
            params: Default::default(),
        };
        assert!(validate_command(&cmd, NcpState::Offline).is_ok());
    }
}
