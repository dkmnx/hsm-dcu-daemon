//! D-Bus type conversions and shared state for `dcu-dbus`.

use std::sync::Arc;

use tokio::sync::RwLock;
use wisun_types::{Eui64, Ipv6Address, NetworkName, PanId, WpanError};

/// D-Bus variant value.
///
/// The original `phase-2A` spec wrote `Variant` throughout, but the real
/// zbus type is `zbus::zvariant::Value`. We alias it here so the rest of the
/// crate reads naturally and matches the spec's intent.
pub type Variant = zbus::zvariant::Value<'static>;

/// Owned variant, used when a value must outlive its message frame
/// (e.g. sent over a oneshot reply channel).
pub type OwnedVariant = zbus::zvariant::OwnedValue;

/// Error type for the D-Bus server and interface handlers.
///
/// Derives `zbus::DBusError` so it can be returned directly from
/// `#[interface]` methods (the generated reply uses `name()`/`description()`
/// derived from the variant and its `Display`).
#[derive(Debug, zbus::DBusError)]
pub enum DbusError {
    /// The requested property key is not known to the daemon.
    UnknownProperty(String),

    /// The requested method/command is not yet implemented.
    NotImplemented(String),

    /// A property value could not be encoded for the D-Bus wire.
    Encoding(String),

    /// A property value could not be decoded from the D-Bus wire.
    Decoding(String),

    /// The underlying transport / connection failed.
    Transport(String),

    /// The interface is not in a state that permits the requested operation.
    InvalidState(String),
}

impl From<zbus::Error> for DbusError {
    fn from(err: zbus::Error) -> Self {
        DbusError::Transport(err.to_string())
    }
}

impl From<zbus::zvariant::Error> for DbusError {
    fn from(err: zbus::zvariant::Error) -> Self {
        DbusError::Encoding(err.to_string())
    }
}

impl From<WpanError> for DbusError {
    fn from(err: WpanError) -> Self {
        DbusError::InvalidState(err.to_string())
    }
}

// ===========================================================================
// Shared daemon state
// ===========================================================================
//
// NOTE: `wisun_types::NcpState` is the *enum* of NCP lifecycle states
// (Uninitialized, Associated, ...). The mutable bag of runtime values the
// D-Bus server reads/writes is `DaemonState` defined here, to avoid
// shadowing the existing type from `wisun-types`.

/// Shared, mutable daemon state backing the D-Bus property handlers.
///
/// Property `get`/`set` operations lock the `RwLock` around this struct.
/// It is the Rust analogue of the C `NCPControlInterface` runtime fields.
#[derive(Debug, Clone)]
pub struct DaemonState {
    // --- NCP ---
    pub ncp_state: wisun_types::NcpState,
    pub ncp_version: String,
    pub ncp_protocol_version: String,
    pub ncp_interface_type: String,
    pub hardware_address: Eui64,
    pub ncp_extended_address: Eui64,
    pub ncp_mac_address: Eui64,
    pub cca_threshold: i8,
    pub tx_power: f64,
    pub region: String,
    pub mode_id: String,
    pub channel: u8,
    pub frequency: u32,
    pub rssi: i32,

    // --- Network ---
    pub network_name: NetworkName,
    pub pan_id: PanId,
    pub xpan_id: Vec<u8>,
    pub node_type: String,
    pub is_commissioned: bool,
    pub is_connected: bool,
    pub network_key: Vec<u8>,

    // --- IPv6 ---
    pub link_local_address: Ipv6Address,
    pub mesh_local_address: Ipv6Address,
    pub mesh_local_prefix: String,

    // --- Interface / stack ---
    pub interface_up: bool,
    pub stack_up: bool,

    // --- Operational dataset (phase 3C) ---
    /// Stringified `Dataset:*` values, refreshed from the NCP's operational
    /// dataset when it arrives. Keyed by D-Bus property key string.
    pub dataset: std::collections::HashMap<String, String>,

    // --- Daemon ---
    pub daemon_enabled: bool,
    pub ready_for_host_sleep: bool,

    // --- Address/prefix/route views (P0-4) ----
    /// Stringified `IPv6:AllAddresses` entries, refreshed from the
    /// AddressManager after each NCP table snapshot.
    pub ipv6_all_addresses: Vec<String>,
    /// Stringified `IPv6:Routes` entries (OS-side interface routes only,
    /// matching C `mInterfaceRoutes` / `get_prop_IPv6InterfaceRoutes`).
    pub ipv6_routes: Vec<String>,
    /// Stringified `Thread:OnMeshPrefixes` entries.
    pub on_mesh_prefixes: Vec<String>,
    /// Stringified `Thread:OffMeshRoutes` entries.
    pub off_mesh_routes: Vec<String>,
}

impl Default for DaemonState {
    fn default() -> Self {
        DaemonState {
            ncp_state: wisun_types::NcpState::Uninitialized,
            ncp_version: String::new(),
            ncp_protocol_version: String::new(),
            ncp_interface_type: String::new(),
            hardware_address: Eui64([0u8; 8]),
            ncp_extended_address: Eui64([0u8; 8]),
            ncp_mac_address: Eui64([0u8; 8]),
            cca_threshold: 0,
            tx_power: 0.0,
            region: String::new(),
            mode_id: String::new(),
            channel: 0,
            frequency: 0,
            rssi: 0,
            network_name: NetworkName(String::new()),
            pan_id: PanId(0),
            xpan_id: Vec::new(),
            node_type: String::new(),
            is_commissioned: false,
            is_connected: false,
            network_key: Vec::new(),
            link_local_address: Ipv6Address([0u8; 16]),
            mesh_local_address: Ipv6Address([0u8; 16]),
            mesh_local_prefix: String::new(),
            interface_up: false,
            stack_up: false,
            dataset: std::collections::HashMap::new(),
            daemon_enabled: false,
            ready_for_host_sleep: false,
            ipv6_all_addresses: Vec::new(),
            ipv6_routes: Vec::new(),
            on_mesh_prefixes: Vec::new(),
            off_mesh_routes: Vec::new(),
        }
    }
}

// ===========================================================================
// Signal payloads
// ===========================================================================

/// Payload for the `NetScanBeacon` signal.
///
/// Mirrors the C `mOnNetScanBeacon` beacon dict
/// (`DBusIPCAPI::received_beacon`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanBeacon {
    pub network_name: String,
    pub pan_id: u16,
    pub channel: u8,
    pub xpan_id: Vec<u8>,
    pub rssi: i32,
    pub lqi: u8,
    pub permit_joining: bool,
}

/// Payload for the `EnergyScanResult` signal.
///
/// Mirrors the C `mOnEnergyScanResult` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnergyScanResultEntry {
    pub channel: u8,
    pub max_rssi: i32,
}

/// Shared handle around the daemon state, used by interface handlers and
/// the server.
pub type SharedState = Arc<RwLock<DaemonState>>;

impl ScanBeacon {
    /// Serialize into the D-Bus dict form the C daemon emits:
    /// `{ "Network:Name", "PANID", "Channel", "XPANID", "RSSI", "LQI", "Joinable" }`.
    pub fn to_dict(&self) -> std::collections::HashMap<String, Variant> {
        use zbus::zvariant::Value;
        let mut m = std::collections::HashMap::new();
        m.insert(
            "Network:Name".into(),
            Value::from(self.network_name.clone()),
        );
        m.insert("PANID".into(), Value::from(self.pan_id));
        m.insert("Channel".into(), Value::from(self.channel));
        m.insert("XPANID".into(), Value::from(self.xpan_id.clone()));
        m.insert("RSSI".into(), Value::from(self.rssi));
        m.insert("LQI".into(), Value::from(self.lqi));
        m.insert("Joinable".into(), Value::from(self.permit_joining));
        m
    }
}

impl EnergyScanResultEntry {
    /// Serialize into the D-Bus dict form: `{ "Channel", "MaxRssi" }`.
    pub fn to_dict(&self) -> std::collections::HashMap<String, Variant> {
        use zbus::zvariant::Value;
        let mut m = std::collections::HashMap::new();
        m.insert("Channel".into(), Value::from(self.channel));
        m.insert("MaxRssi".into(), Value::from(self.max_rssi));
        m
    }
}
