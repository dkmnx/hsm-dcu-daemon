//! End-to-end integration tests for the daemon + mock NCP stack.
//!
//! All tests share a single compilation unit so `common::daemon_test`
//! helpers are always reachable — no dead-code warnings across separate
//! test binaries.

mod common;

use std::time::Duration;

use common::daemon_test::{TestDaemon, wait_for_state};
use dcu_dbus::commands::Command;
use dcu_mock::failure::FailureRule;
use wisun_types::NcpState;

// ---------------------------------------------------------------------------
// daemon_startup
// ---------------------------------------------------------------------------

#[tokio::test]
async fn daemon_starts_and_reaches_offline() {
    let daemon = TestDaemon::start().await.unwrap();

    wait_for_state(&daemon, |s| !s.is_initializing(), Duration::from_secs(5))
        .await
        .unwrap();

    let state = daemon.get_ncp_state().await;
    assert!(
        !state.is_initializing(),
        "expected non-initializing state, got {:?}",
        state
    );

    daemon.tear_down().await;
}

// ---------------------------------------------------------------------------
// network_form
// ---------------------------------------------------------------------------

#[tokio::test]
async fn form_network_end_to_end() {
    let daemon = TestDaemon::start_with_topology(3).await.unwrap();

    wait_for_state(&daemon, |s| !s.is_initializing(), Duration::from_secs(5))
        .await
        .unwrap();

    daemon
        .send_command(Command::Form {
            params: Default::default(),
        })
        .await;

    wait_for_state(
        &daemon,
        |s| s == NcpState::Associated,
        Duration::from_secs(10),
    )
    .await
    .unwrap();

    let state = daemon.get_ncp_state().await;
    assert_eq!(state, NcpState::Associated);

    daemon.tear_down().await;
}

// ---------------------------------------------------------------------------
// network_join
// ---------------------------------------------------------------------------

#[tokio::test]
async fn join_network_end_to_end() {
    let daemon = TestDaemon::start_with_topology(1).await.unwrap();

    wait_for_state(&daemon, |s| !s.is_initializing(), Duration::from_secs(5))
        .await
        .unwrap();

    daemon
        .send_command(Command::Join {
            params: Default::default(),
        })
        .await;

    wait_for_state(
        &daemon,
        |s| s == NcpState::Associated,
        Duration::from_secs(10),
    )
    .await
    .unwrap();

    let state = daemon.get_ncp_state().await;
    assert_eq!(state, NcpState::Associated);

    daemon.tear_down().await;
}

// ---------------------------------------------------------------------------
// error_handling
// ---------------------------------------------------------------------------

#[tokio::test]
async fn daemon_handles_ncp_timeout() {
    let daemon = TestDaemon::builder()
        .with_failure(FailureRule::DropCommand {
            command_id: spinel::command::CMD_PROP_VALUE_GET,
        })
        .start()
        .await
        .unwrap();

    wait_for_state(
        &daemon,
        |s| s == NcpState::Offline || s.is_fault(),
        Duration::from_secs(10),
    )
    .await
    .unwrap();

    daemon.tear_down().await;
}
