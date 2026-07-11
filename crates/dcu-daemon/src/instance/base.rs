//! Core NCP state machine.
//!
//! Reimplements `src/dcud/NCPInstanceBase.cpp` and related files as
//! an async state machine driven by tokio.

use std::sync::Arc;

use tokio::sync::{mpsc, Notify, RwLock};
use tokio_util::sync::CancellationToken;
use wisun_types::NcpState;

use crate::config::Config;
use crate::DaemonError;

/// The base NCP instance — state machine, event loop, task queue,
/// and transport handles.
pub struct NcpInstanceBase {
    // State
    ncp_state: Arc<RwLock<NcpState>>,
    interface_name: String,
    state_changed: Arc<Notify>,

    // Channel from the D-Bus server
    command_rx: mpsc::Receiver<dcu_dbus::commands::Command>,

    // Shared daemon state — written by data-pump callbacks during operation.
    #[allow(dead_code)]
    shared_state: Arc<RwLock<dcu_dbus::DaemonState>>,

    // Config — kept for when I/O pumps are implemented and need
    // nc_socket_path, nc_socket_baud, etc.
    #[allow(dead_code)]
    config: Config,
}

impl NcpInstanceBase {
    pub async fn new(
        config: Config,
        shared_state: Arc<RwLock<dcu_dbus::DaemonState>>,
        command_rx: mpsc::Receiver<dcu_dbus::commands::Command>,
    ) -> Result<Self, DaemonError> {
        let interface_name = config.tun_interface_name.clone();

        Ok(Self {
            ncp_state: Arc::new(RwLock::new(NcpState::Uninitialized)),
            interface_name,
            state_changed: Arc::new(Notify::new()),
            command_rx,
            shared_state,
            config,
        })
    }

    pub fn interface_name(&self) -> &str {
        &self.interface_name
    }

    pub async fn run(&mut self, cancel: CancellationToken) {
        tracing::info!("Starting NCP instance event loop");
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!("NCP instance cancelled");
                    break;
                }
                cmd = self.command_rx.recv() => {
                    match cmd {
                        Some(cmd) => {
                            tracing::debug!("Received command: {:?}", cmd);
                            let _ = self.handle_command(cmd).await;
                        }
                        None => {
                            tracing::info!("Command channel closed");
                            break;
                        }
                    }
                }
                _ = self.state_changed.notified() => {
                    tracing::trace!("State change notification");
                }
            }
        }
    }

    pub async fn start_pumps(&mut self) -> Result<(), DaemonError> {
        tracing::info!("Starting I/O pumps (stub)");
        // TODO: open serial transport, spin up NCP↔driver pumps
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<(), DaemonError> {
        tracing::info!("Stopping NCP instance");
        Ok(())
    }

    pub async fn set_ncp_state(&self, state: NcpState) {
        let mut guard = self.ncp_state.write().await;
        *guard = state;
        self.state_changed.notify_waiters();
    }

    pub async fn get_ncp_state(&self) -> NcpState {
        *self.ncp_state.read().await
    }

    /// Handle a single command from the D-Bus server.
    /// Returns a status string (not a D-Bus `Variant` — the string is used
    /// for internal status reporting; D-Bus property replies go through
    /// the shared state).
    pub async fn handle_command(
        &mut self,
        cmd: dcu_dbus::commands::Command,
    ) -> Result<String, DaemonError> {
        match cmd {
            dcu_dbus::commands::Command::Reset => {
                let state = self.get_ncp_state().await;
                Ok(format!("NCP:State: {state}"))
            }
            dcu_dbus::commands::Command::Leave => {
                self.set_ncp_state(NcpState::Offline).await;
                Ok("Left network".into())
            }
            other => {
                tracing::warn!("Unhandled command: {other:?}");
                Ok("unhandled".into())
            }
        }
    }
}
