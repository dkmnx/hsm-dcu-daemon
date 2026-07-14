//! NetworkRetain — save/recall/erase network info across NCP resets.
//!
//! Port of `src/wfantund/NetworkRetain.cpp`. On NCP state transitions
//! the daemon invokes an external helper command with a single-char
//! argument:
//!
//! - `R` (recall) when transitioning from initializing to Offline
//! - `E` (erase) when transitioning from commissioned to Offline
//!
//! The command is spawned fresh per transition via `tokio::process::Command`.
//! This is behaviorally equivalent to C's persistent forked child (which
//! re-invokes `system()` per char) while avoiding unsafe `fork()` in a
//! multi-threaded tokio runtime.

use wisun_types::NcpState;

/// The action NetworkRetain will perform on a state transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetainAction {
    Recall,
    Erase,
}

impl RetainAction {
    /// Single-char argument passed to the retain helper command.
    pub fn as_arg(self) -> &'static str {
        match self {
            RetainAction::Recall => "R",
            RetainAction::Erase => "E",
        }
    }
}

/// Determine which retain action (if any) to perform for a given NCP
/// state transition.
///
/// Matches the logic in `NetworkRetain.cpp:58-79`.
pub fn action_for_transition(old: NcpState, new: NcpState) -> Option<RetainAction> {
    // Initializing -> Offline  (recall)
    if old.is_initializing() && new == NcpState::Offline {
        return Some(RetainAction::Recall);
    }

    // Commissioned -> Offline  (erase)
    if old.is_commissioned() && new == NcpState::Offline {
        return Some(RetainAction::Erase);
    }

    None
}

/// NetworkRetain handler. Stores the external helper command and spawns
/// it on NCP state transitions.
pub struct NetworkRetain {
    command: Option<String>,
}

impl NetworkRetain {
    pub fn new(command: Option<String>) -> Self {
        Self { command }
    }

    /// Handle an NCP state change. If a retain action is triggered and
    /// a command is configured, spawns `<command> <arg>` (S/R/E).
    /// Returns a JoinHandle the caller can await to ensure the retain
    /// command completes before proceeding (e.g. before AutoAssociate).
    ///
    /// **Post-chroot note:** The command path must exist inside the chroot
    /// and be runnable by the dropped uid. Spawn failures are logged at
    /// `error` level because a failed recall means networks won't survive
    /// NCP resets.
    pub fn handle_state_change(
        &self,
        old: NcpState,
        new: NcpState,
    ) -> Option<tokio::task::JoinHandle<()>> {
        let cmd = self.command.as_ref()?;
        let action = action_for_transition(old, new)?;
        tracing::info!("NetworkRetain - {:?} network info...", action);
        let cmd = cmd.clone();
        let arg = action.as_arg().to_string();
        Some(tokio::spawn(async move {
            match tokio::process::Command::new(&cmd).arg(&arg).output().await {
                Ok(out) if out.status.success() => {
                    tracing::debug!("NetworkRetain: {cmd} {arg} succeeded");
                }
                Ok(out) => {
                    tracing::error!(
                        "NetworkRetain: {cmd} {arg} exited with {} — networks may not survive NCP reset",
                        out.status
                    );
                }
                Err(e) => {
                    tracing::error!(
                        "NetworkRetain: failed to spawn {cmd}: {e} — networks may not survive NCP reset"
                    );
                }
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recall_on_initializing_to_offline() {
        assert_eq!(
            action_for_transition(NcpState::Uninitialized, NcpState::Offline),
            Some(RetainAction::Recall)
        );
        assert_eq!(
            action_for_transition(NcpState::Upgrading, NcpState::Offline),
            Some(RetainAction::Recall)
        );
    }

    #[test]
    fn erase_on_commissioned_to_offline() {
        assert_eq!(
            action_for_transition(NcpState::Commissioned, NcpState::Offline),
            Some(RetainAction::Erase)
        );
    }

    #[test]
    fn no_action_for_same_state() {
        assert_eq!(
            action_for_transition(NcpState::Offline, NcpState::Offline),
            None
        );
        assert_eq!(
            action_for_transition(NcpState::Associated, NcpState::Associated),
            None
        );
    }

    #[test]
    fn no_action_for_non_matching_transitions() {
        assert_eq!(
            action_for_transition(NcpState::Offline, NcpState::Fault),
            None
        );
        assert_eq!(
            action_for_transition(NcpState::Associated, NcpState::Offline),
            // Associated -> Offline: is_commissioned=true but new is Offline,
            // however old.is_initializing()=false — check: is_commissioned=true
            // and new=Offline → Erase? Yes! Commissioned/Associated -> Offline = Erase.
            Some(RetainAction::Erase)
        );
        assert_eq!(
            action_for_transition(NcpState::Associated, NcpState::Uninitialized),
            None
        );
    }
}
