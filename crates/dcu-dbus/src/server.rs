//! D-Bus server lifecycle.
//!
//! Wraps a `zbus::Connection`, registers the per-interface
//! `com.nestlabs.WPANTunnelDriver` object, and provides signal emitters.
//!
//! Corrected from the spec: the duplicate `emit_property_changed` /
//! `emit_prop_changed` pair is collapsed into `emit_prop_changed` only, and
//! the bus name is the well-known `com.nestlabs.WPANTunnelDriver` (not
//! `org.wpantund` from the upstream protocol doc, which the spec itself
//! notes is not used by this daemon).

use std::sync::Arc;

use tokio::sync::{RwLock, mpsc};
use zbus::Connection;

use crate::commands::Command;
use crate::interface;
use crate::signals;
use crate::types::{DaemonState, DbusError, EnergyScanResultEntry, ScanBeacon, Variant};

/// Well-known D-Bus name and interface for this daemon (from
/// `src/ipc-dbus/wpan-dbus.h`).
pub const WPANTUND_DBUS_NAME: &str = "com.nestlabs.WPANTunnelDriver";
pub const WPANTUND_DBUS_INTERFACE: &str = "com.nestlabs.WPANTunnelDriver";
pub const WPANTUND_BASE_OBJECT_PATH: &str = "/com/nestlabs/WPANTunnelDriver";

/// The D-Bus server handle.
#[derive(Debug)]
pub struct DbusServer {
    conn: Connection,
    /// Per-interface object path, e.g.
    /// `/com/nestlabs/WPANTunnelDriver/wfan0`.
    iface_path: String,
    iface_name: String,
    /// Well-known bus name claimed by this server instance.
    bus_name: String,
}

impl DbusServer {
    /// Build the per-interface object path from an interface name.
    pub fn iface_object_path(iface_name: &str) -> String {
        format!("{WPANTUND_BASE_OBJECT_PATH}/{iface_name}")
    }

    /// Start the D-Bus server on a fresh session bus connection and claim
    /// the well-known name.
    pub async fn start(
        iface_name: String,
        state: Arc<RwLock<DaemonState>>,
        command_tx: mpsc::Sender<Command>,
    ) -> Result<Self, DbusError> {
        let conn = Connection::session().await?;
        Self::start_on(
            conn,
            iface_name,
            state,
            command_tx,
            WPANTUND_DBUS_NAME.to_string(),
        )
        .await
    }

    /// Start the D-Bus server on an *existing* connection (used by tests
    /// that build a dedicated bus). Registers the interface object and emits
    /// `InterfaceAdded`.
    ///
    /// `bus_name` is the well-known name to claim. Production callers pass
    /// [`WPANTUND_DBUS_NAME`]; tests should pass a unique name so parallel
    /// test cases do not contend for the canonical name.
    pub async fn start_on(
        conn: Connection,
        iface_name: String,
        state: Arc<RwLock<DaemonState>>,
        command_tx: mpsc::Sender<Command>,
        bus_name: String,
    ) -> Result<Self, DbusError> {
        // Request the well-known name so clients can find us. Allow
        // replacement so multiple server instances (e.g. test cases on a
        // shared bus) do not fail to acquire the name.
        conn.request_name_with_flags(
            bus_name.as_str(),
            zbus::fdo::RequestNameFlags::ReplaceExisting
                | zbus::fdo::RequestNameFlags::AllowReplacement,
        )
        .await?;

        let iface_path = Self::iface_object_path(&iface_name);
        let iface = interface::new(iface_name.clone(), state, command_tx);
        conn.object_server().at(iface_path.clone(), iface).await?;

        // Announce the new interface on the base object path.
        signals::emit_interface_added(&conn, &iface_name).await?;

        Ok(DbusServer {
            conn,
            iface_path,
            iface_name,
            bus_name,
        })
    }

    /// The well-known bus name this server claims.
    pub fn bus_name(&self) -> &str {
        &self.bus_name
    }

    /// This server connection's unique bus name (e.g. `:1.42`). Useful for
    /// routing D-Bus calls directly to this instance without contending for
    /// the well-known name.
    pub fn unique_name(&self) -> Option<&zbus::names::UniqueName<'_>> {
        self.conn.unique_name().map(|v| &**v)
    }

    /// Borrow the underlying connection (used by tests to build proxies and
    /// signal subscriptions).
    pub fn conn_ref(&self) -> &Connection {
        &self.conn
    }

    /// The per-interface object path.
    pub fn iface_object_path_str(&self) -> &str {
        &self.iface_path
    }

    /// The NCP interface name (e.g. `wfan0`).
    pub fn iface_name(&self) -> &str {
        &self.iface_name
    }

    /// Stop the server: emit `InterfaceRemoved`, release the name, and
    /// close the underlying connection so the bus daemon removes it
    /// promptly (otherwise a lingering connection can interfere with
    /// subsequent tests/clients on the same bus).
    pub async fn stop(self) -> Result<(), DbusError> {
        signals::emit_interface_removed(&self.conn, &self.iface_name).await?;
        self.conn.release_name(self.bus_name.as_str()).await?;
        let _ = self.conn.close().await;
        Ok(())
    }

    /// Emit a `NetScanBeacon` signal.
    pub async fn emit_scan_beacon(&self, beacon: ScanBeacon) -> Result<(), DbusError> {
        signals::emit_net_scan_beacon(&self.conn, &self.iface_path, &beacon).await
    }

    /// Emit an `EnergyScanResult` signal.
    pub async fn emit_energy_scan_result(
        &self,
        result: EnergyScanResultEntry,
    ) -> Result<(), DbusError> {
        signals::emit_energy_scan_result(&self.conn, &self.iface_path, &result).await
    }

    /// Emit a `PropChanged` signal (single, canonical emitter; the spec's
    /// duplicate `emit_property_changed` was removed).
    ///
    /// The signal is emitted on `<iface_path>/Property/<key_as_path>` to
    /// match the C daemon, which transforms the property key into a
    /// D-Bus-compatible object path (`':'` → `'/'`, `'.'` → `'_'`) and emits
    /// the `PropChanged` signal there (see `DBusIPCAPI::property_changed`,
    /// DBusIPCAPI.cpp:442). Clients subscribe per-property at that path.
    pub async fn emit_prop_changed(&self, key: &str, value: Variant) -> Result<(), DbusError> {
        let prop_path = format!("{}/Property/{}", self.iface_path, key_to_path(key));
        signals::emit_prop_changed(&self.conn, &prop_path, key, value).await
    }
}

/// Transform a property key into the D-Bus path segment used by the C
/// daemon for per-property `PropChanged` signals:
/// `NCP:State` → `NCP/State`, `Network:Name` → `Network/Name`, etc.
/// (alphanumerics and `'_'` kept; `':'` → `'/'`; `'.'` → `'_'`.)
fn key_to_path(key: &str) -> String {
    let mut out = String::with_capacity(key.len() + 8);
    for c in key.chars() {
        if c.is_alphanumeric() || c == '_' {
            out.push(c);
        } else if c == ':' {
            out.push('/');
        } else if c == '.' {
            out.push('_');
        }
        // other punctuation is dropped, matching the C implementation.
    }
    out
}
