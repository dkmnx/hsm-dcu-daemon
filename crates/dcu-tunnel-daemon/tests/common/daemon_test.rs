//! Test helper that wires a mock NCP, an `NcpInstance`, and optionally a
//! D-Bus server together for integration tests.
//!
//! ## Ownership model
//!
//! `NcpInstance::run(&mut self, cancel)` runs until the token fires. It must
//! be spawned on a tokio task, which moves the instance. To keep the
//! shared-state and command handles accessible from the test, we clone the
//! `Arc<RwLock<DaemonState>>` and `mpsc::Sender<Command>` *before* the move.
//!
//! `tear_down()` cancels the token and waits for the task to finish.

#![allow(dead_code)]

use std::sync::Arc;

use tokio::sync::{RwLock, mpsc};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use dcu_tunnel_daemon::config::Config;
use dcu_tunnel_daemon::instance::NcpInstance;
use dcu_dbus::DaemonState;
use dcu_dbus::commands::Command;
use dcu_mock::builder::MockNcpBuilder;
use dcu_mock::failure::FailureRule;
use dcu_mock::topology::MockTopology;

/// Error type for test helpers.
#[derive(Debug)]
pub enum TestError {
    Daemon(dcu_tunnel_daemon::DaemonError),
    Timeout,
}

impl From<dcu_tunnel_daemon::DaemonError> for TestError {
    fn from(e: dcu_tunnel_daemon::DaemonError) -> Self {
        TestError::Daemon(e)
    }
}

/// A running daemon backed by a mock NCP, ready for integration tests.
pub struct TestDaemon {
    cancel: CancellationToken,
    handle: JoinHandle<()>,
    shared_state: Arc<RwLock<DaemonState>>,
    ncp_state: Arc<tokio::sync::RwLock<wisun_types::NcpState>>,
    command_tx: mpsc::Sender<Command>,
}

impl TestDaemon {
    /// Start the daemon with `node_count` mock nodes in the topology.
    pub async fn start_with_topology(node_count: usize) -> Result<Self, TestError> {
        Self::builder().with_topology(node_count).start().await
    }

    /// Start the daemon with default configuration (no mock nodes).
    pub async fn start() -> Result<Self, TestError> {
        Self::builder().start().await
    }

    pub fn builder() -> TestDaemonBuilder {
        TestDaemonBuilder::new()
    }

    /// Send a command to the daemon (non-blocking).
    pub async fn send_command(&self, cmd: Command) {
        let _ = self.command_tx.send(cmd).await;
    }

    /// Clone the shared daemon state handle.
    pub fn shared_state(&self) -> Arc<RwLock<DaemonState>> {
        Arc::clone(&self.shared_state)
    }

    /// Read the NCP state directly from the inner instance.
    pub async fn get_ncp_state(&self) -> wisun_types::NcpState {
        *self.ncp_state.read().await
    }

    /// Cancel the daemon event loop and wait for the task to finish.
    pub async fn tear_down(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
    }
}

pub struct TestDaemonBuilder {
    node_count: usize,
    failure_rules: Vec<FailureRule>,
    interface_name: String,
}

impl TestDaemonBuilder {
    pub fn new() -> Self {
        Self {
            node_count: 0,
            failure_rules: Vec::new(),
            interface_name: "test_wfan0".into(),
        }
    }

    pub fn with_topology(mut self, node_count: usize) -> Self {
        self.node_count = node_count;
        self
    }

    pub fn with_failure(mut self, rule: FailureRule) -> Self {
        self.failure_rules.push(rule);
        self
    }

    pub async fn start(self) -> Result<TestDaemon, TestError> {
        // 1. Build mock NCP + duplex transport pair.
        let mut builder = MockNcpBuilder::new();
        if self.node_count > 0 {
            builder = builder.with_topology(MockTopology::with_nodes(self.node_count));
        }
        for rule in self.failure_rules {
            builder = builder.with_failure(rule);
        }
        let (mut mock, daemon_transport) = builder.build();

        // 2. Spawn mock NCP event loop.
        tokio::spawn(async move {
            if let Err(e) = mock.run().await {
                tracing::error!("Mock NCP error: {e}");
            }
        });

        // 3. Create NcpInstance with test config.
        let config = test_config(&self.interface_name);
        let mut instance = NcpInstance::new(config).await?;

        // 4. Extract cloneable handles BEFORE moving instance.
        let shared_state = instance.shared_state();
        let ncp_state = instance.ncp_state_handle();
        let command_tx = instance.command_sender();

        // 5. Start I/O pumps over the mock transport.
        instance
            .start_pumps_with_transport(daemon_transport)
            .await?;

        // 6. Spawn the daemon event loop (moves `instance`).
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(async move {
            instance.run(cancel_clone).await;
        });

        Ok(TestDaemon {
            cancel,
            handle,
            shared_state,
            ncp_state,
            command_tx,
        })
    }
}

/// Build a `Config` suitable for test runs (does not touch real hardware).
fn test_config(interface_name: &str) -> Config {
    Config {
        nc_socket_path: "mock".into(),
        tun_interface_name: interface_name.into(),
        ..Default::default()
    }
}

/// Poll `shared_state().ncp_state` until `pred(state)` returns true, or
/// time out.
pub async fn wait_for_state<F>(
    daemon: &TestDaemon,
    pred: F,
    timeout: std::time::Duration,
) -> Result<(), TestError>
where
    F: Fn(wisun_types::NcpState) -> bool,
{
    tokio::time::timeout(timeout, async {
        loop {
            let state = daemon.get_ncp_state().await;
            if pred(state) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| TestError::Timeout)
}
