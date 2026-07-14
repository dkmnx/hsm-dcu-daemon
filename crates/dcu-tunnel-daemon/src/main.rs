//! Daemon entry point.
//!
//! Reimplements `src/dcud/wpantund.cpp`. Wires the config parser, NCP
//! instance, D-Bus server, and signal handlers together.

use std::sync::Arc;

use clap::Parser;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use dcu_dbus::server::BusType;
use dcu_dbus::{DaemonState, DbusServer};
use dcu_tunnel_daemon::NcpInstance;
use dcu_tunnel_daemon::config::Config;

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

    // Apply daemon lifecycle (PID file, chroot, priv-drop) after all
    // privileged setup completes. We need the config after it's moved
    // into NcpInstance, so clone the lifecycle-relevant parts now.
    let lifecycle_config = config.clone();

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
    // Production uses the system bus; tests pass DCU_DBUS_BUS=session.
    let bus = match std::env::var("DCU_DBUS_BUS")
        .map(|v| v.to_lowercase())
        .as_deref()
    {
        Ok("session") => BusType::Session,
        _ => BusType::System,
    };
    let dbus_server = DbusServer::start_with_bus(
        instance.interface_name().to_string(),
        daemon_state,
        command_tx,
        bus,
    )
    .await?;

    // Start I/O pumps (NCP <-> driver <-> TUN).
    instance.start_pumps().await?;

    // Take the NetworkTimeUpdate receiver and spawn a task that emits the
    // D-Bus signal whenever the NCP pushes a time update.
    let time_update_rx = instance.take_time_update_rx();
    let dbus_conn = dbus_server.conn_ref().clone();
    let iface_path = dbus_server.iface_object_path_str().to_string();
    tokio::spawn(async move {
        let mut rx = time_update_rx;
        while let Some((network_time, time_sync_status)) = rx.recv().await {
            if let Err(e) = dcu_dbus::signals::emit_network_time_update(
                &dbus_conn,
                &iface_path,
                network_time,
                time_sync_status,
            )
            .await
            {
                tracing::warn!("Failed to emit NetworkTimeUpdate: {e}");
            }
        }
    });

    // Apply lifecycle: PID file → chroot → priv-drop.
    // Must happen after serial/TUN/D-Bus are open (privileged setup).
    let _pid_guard = dcu_tunnel_daemon::lifecycle::apply_lifecycle(&lifecycle_config)?;

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
