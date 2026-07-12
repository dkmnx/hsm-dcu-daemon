//! NCP instance management — wraps [`NcpInstanceBase`] with the D-Bus handles.
//!
//! `NcpInstance` is what `main.rs` constructs: it owns the inner base
//! instance, shared daemon state, and command channel that the D-Bus server
//! receives.

use std::sync::Arc;

use tokio::sync::{RwLock, mpsc};

use crate::DaemonError;
use crate::config::Config;

pub mod base;
pub use base::NcpInstanceBase;

/// The public daemon instance, wrapping the base state machine.
pub struct NcpInstance {
    inner: NcpInstanceBase,
    shared_state: Arc<RwLock<dcu_dbus::DaemonState>>,
    command_tx: mpsc::Sender<dcu_dbus::commands::Command>,
}

impl NcpInstance {
    pub async fn new(config: Config) -> Result<Self, DaemonError> {
        let shared_state = Arc::new(RwLock::new(dcu_dbus::DaemonState::default()));
        let (command_tx, command_rx) = mpsc::channel(64);
        let inner = NcpInstanceBase::new(config, shared_state.clone(), command_rx).await?;

        Ok(Self {
            inner,
            shared_state,
            command_tx,
        })
    }

    pub async fn run(&mut self, cancel: tokio_util::sync::CancellationToken) {
        self.inner.run(cancel).await;
    }

    pub fn shared_state(&self) -> Arc<RwLock<dcu_dbus::DaemonState>> {
        self.shared_state.clone()
    }

    pub fn command_sender(&self) -> mpsc::Sender<dcu_dbus::commands::Command> {
        self.command_tx.clone()
    }

    pub fn interface_name(&self) -> &str {
        self.inner.interface_name()
    }

    /// Read the NCP state from the inner instance (not DaemonState).
    pub async fn get_ncp_state(&self) -> wisun_types::NcpState {
        self.inner.get_ncp_state().await
    }

    /// Clone the Arc<NcpState> handle for use outside the instance.
    pub fn ncp_state_handle(&self) -> std::sync::Arc<tokio::sync::RwLock<wisun_types::NcpState>> {
        self.inner.ncp_state.clone()
    }

    pub async fn start_pumps(&mut self) -> Result<(), DaemonError> {
        self.inner.start_pumps().await
    }

    /// Start I/O pumps over an existing transport (for tests).
    #[cfg(feature = "test-util")]
    pub async fn start_pumps_with_transport<T: dcu_serial::transport::Transport + Unpin>(
        &mut self,
        transport: T,
    ) -> Result<(), DaemonError> {
        self.inner.start_pumps_with_transport(transport).await
    }

    pub async fn stop(&mut self) -> Result<(), DaemonError> {
        self.inner.stop().await
    }
}
