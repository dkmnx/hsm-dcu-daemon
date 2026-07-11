//! Daemon entry point.
//!
//! Reimplements `src/dcud/wpantund.cpp`. Wires the config parser, NCP
//! instance, D-Bus server, and signal handlers together.

use std::sync::Arc;

use clap::Parser;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use dcu_daemon::config::Config;
use dcu_daemon::NcpInstance;
use dcu_dbus::{DaemonState, DbusServer};

#[derive(Parser)]
#[command(name = "dcud", about = "HSM DCU Wi-SUN FAN Border Router")]
struct Args {
    /// Path to wpantund.conf-style configuration file.
    #[arg(short = 'c', long = "config", default_value = "/etc/wpantund.conf")]
    config_path: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let config = Config::load(&args.config_path)?;

    // Graceful stop: SIGINT or SIGTERM -> cancel.
    let cancel = CancellationToken::new();

    let cancel_int = cancel.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        cancel_int.cancel();
    });

    let cancel_term = cancel.clone();
    tokio::spawn(async move {
        let mut sigterm = signal(SignalKind::terminate()).expect("SIGTERM handler");
        sigterm.recv().await;
        cancel_term.cancel();
    });

    // Build the NCP instance (owns the shared state and command channel).
    let mut instance = NcpInstance::new(config).await?;
    let daemon_state: Arc<RwLock<DaemonState>> = instance.shared_state();
    let command_tx = instance.command_sender();

    // Start the D-Bus server (claims com.nestlabs.WPANTunnelDriver).
    let dbus_server = DbusServer::start(
        instance.interface_name().to_string(),
        daemon_state,
        command_tx,
    )
    .await?;

    // Start I/O pumps (NCP <-> driver <-> TUN).
    instance.start_pumps().await?;

    // Main event loop.
    instance.run(cancel.clone()).await;

    if cancel.is_cancelled() {
        tracing::info!("Stopping daemon...");
    }

    // Cleanup: stop D-Bus, close NCP/TUN, drop bus name.
    instance.stop().await?;
    dbus_server.stop().await?;

    Ok(())
}
