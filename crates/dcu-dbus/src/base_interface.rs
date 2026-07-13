//! Base D-Bus object for `com.nestlabs.WPANTunnelDriver`.
//!
//! The C daemon registers a **base object** at
//! `/com/nestlabs/WPANTunnelDriver` (distinct from the per-interface
//! object at `/com/nestlabs/WPANTunnelDriver/<iface>`). The base object
//! serves the interface-enumeration and version methods:
//!
//! * `GetInterfaces() -> aas` — array of `[iface_name, unique_bus_name]`
//!   pairs (C: `DBUSIPCServer.cpp:286`, returns `DBUS_TYPE_ARRAY_AS_STRING
//!   DBUS_TYPE_STRING_AS_STRING`).
//! * `GetVersion() -> u` — daemon version as a uint32
//!   (`WPAN_TUNNEL_DBUS_VERSION = 2`, `wpan-dbus.h:27`).
//!
//! In C `GetVersion` is dispatched from the base `message_handler`, not the
//! per-interface `DBusIPCAPI::message_handler` — so it lives here, not on
//! [`crate::interface::WpanInterface`].

use zbus::interface;

/// The base D-Bus object.
///
/// Holds the interface name and a clone of the connection handle so it can
/// report the daemon's own unique bus name (used as the `unique_bus_name`
/// half of every `GetInterfaces` entry).
pub struct BaseInterface {
    pub iface_name: String,
}

#[interface(name = "com.nestlabs.WPANTunnelDriver")]
impl BaseInterface {
    /// GetInterfaces: list the interfaces this daemon manages.
    ///
    /// Returns `aas` — an array where each element is the 2-string array
    /// `[interface_name, unique_bus_name]`. The C daemon also tracks
    /// *external* daemons learned via `InterfaceAdded`; this port currently
    /// reports only the locally-owned interface (the single `wfan0` instance
    /// this daemon manages).
    #[zbus(name = "GetInterfaces")]
    async fn get_interfaces(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> Vec<Vec<String>> {
        let unique = connection
            .unique_name()
            .map(|n| n.to_string())
            .unwrap_or_default();
        vec![vec![self.iface_name.clone(), unique]]
    }

    /// GetVersion: daemon version as a uint32 (matches C `WPAN_TUNNEL_DBUS_VERSION`).
    #[zbus(name = "GetVersion")]
    async fn get_version(&self) -> u32 {
        2
    }
}
