//! Property get/set dispatch.
//!
//! Maps every D-Bus property key string (e.g. `"NCP:State"`,
//! `"Network:PANID"`) to a read or write against [`DaemonState`].
//!
//! The shared state is passed as `&RwLock<DaemonState>`; callers are
//! responsible for acquiring the lock in the correct mode. (The spec's
//! original `handle_get_property(&NcpState)` signature would not thread
//! through the `Arc<RwLock<..>>` provided by `DbusServer::start`, so it is
//! corrected here.)

use std::str::FromStr;
use std::sync::Arc;

use tokio::sync::RwLock;
use wisun_types::{NetworkName, PanId};
use zbus::zvariant::Value;

use crate::types::{DaemonState, DbusError, Variant};

/// Shared handle to the daemon state (same alias used by the server).
pub type SharedState = Arc<RwLock<DaemonState>>;

/// Read a single property by key.
pub async fn handle_get_property(name: &str, state: &SharedState) -> Result<Variant, DbusError> {
    let guard = state.read().await;
    get_property_locked(name, &guard)
}

/// Write a single property by key.
pub async fn handle_set_property(
    name: &str,
    value: Variant,
    state: &SharedState,
) -> Result<(), DbusError> {
    let mut guard = state.write().await;
    set_property_locked(name, value, &mut guard)
}

/// Synchronous read variant that assumes the lock is already held.
pub fn get_property_locked(name: &str, state: &DaemonState) -> Result<Variant, DbusError> {
    let v: Value<'static> = match name {
        // --- Core / NCP ---
        "NCP:State" => Value::from(state.ncp_state.to_string()),
        "NCP:Version" => Value::from(state.ncp_version.clone()),
        "NCP:ProtocolVersion" => Value::from(state.ncp_protocol_version.clone()),
        "NCP:InterfaceType" => Value::from(state.ncp_interface_type.clone()),
        "NCP:HardwareAddress" => Value::from(state.hardware_address.to_string()),
        "NCP:ExtendedAddress" => Value::from(state.ncp_extended_address.to_string()),
        "NCP:MACAddress" => Value::from(state.ncp_mac_address.to_string()),
        "NCP:CCAThreshold" => Value::from(state.cca_threshold),
        "NCP:TXPower" => Value::from(state.tx_power),
        "NCP:Region" => Value::from(state.region.clone()),
        "NCP:ModeID" => Value::from(state.mode_id.clone()),
        "NCP:Channel" => Value::from(state.channel),
        "NCP:Frequency" => Value::from(state.frequency),
        "NCP:RSSI" => Value::from(state.rssi),

        // --- Network ---
        "Network:Name" => Value::from(state.network_name.to_string()),
        "Network:PANID" => Value::from(state.pan_id.to_string()),
        "Network:XPANID" => Value::from(format!("{:016X}", xpanid_u64(&state.xpan_id))),
        "Network:NodeType" => Value::from(state.node_type.clone()),
        "Network:IsCommissioned" => Value::from(state.is_commissioned),
        "Network:IsConnected" => Value::from(state.is_connected),
        "Network:Key" => Value::from(hex_bytes(&state.network_key)),

        // --- IPv6 ---
        "IPv6:LinkLocalAddress" => Value::from(state.link_local_address.to_string()),
        "IPv6:MeshLocalAddress" => Value::from(state.mesh_local_address.to_string()),
        "IPv6:MeshLocalPrefix" => Value::from(state.mesh_local_prefix.clone()),

        // --- Interface / Stack ---
        "Interface:Up" => Value::from(state.interface_up),
        "Stack:Up" => Value::from(state.stack_up),

        // --- Daemon ---
        "Daemon:Version" => Value::from(env!("CARGO_PKG_VERSION")),
        "Daemon:Enabled" => Value::from(state.daemon_enabled),
        "Daemon:ReadyForHostSleep" => Value::from(state.ready_for_host_sleep),

        _ => return Err(DbusError::UnknownProperty(name.to_string())),
    };
    Ok(v)
}

/// Synchronous write variant that assumes the lock is already held.
pub fn set_property_locked(
    name: &str,
    value: Variant,
    state: &mut DaemonState,
) -> Result<(), DbusError> {
    // Most runtime properties are read-only from D-Bus; the daemon
    // updates them internally. Writable ones are accepted here.
    match name {
        "Network:Name" => {
            state.network_name = NetworkName::from_str(&string_from(value)?)
                .map_err(|e| DbusError::Decoding(e.to_string()))?;
            Ok(())
        }
        "Network:PANID" => {
            state.pan_id = PanId::from_str(&string_from(value)?)
                .map_err(|e| DbusError::Decoding(e.to_string()))?;
            Ok(())
        }
        "Network:XPANID" => {
            state.xpan_id = parse_xpanid(&string_from(value)?)?;
            Ok(())
        }
        "Interface:Up" => {
            state.interface_up = bool_from(value)?;
            Ok(())
        }
        "Stack:Up" => {
            state.stack_up = bool_from(value)?;
            Ok(())
        }
        "NCP:Region" => {
            state.region = string_from(value)?;
            Ok(())
        }
        "NCP:ModeID" => {
            state.mode_id = string_from(value)?;
            Ok(())
        }
        "NCP:CCAThreshold" => {
            state.cca_threshold = i8_from(value)?;
            Ok(())
        }
        "NCP:TXPower" => {
            state.tx_power = f64_from(value)?;
            Ok(())
        }
        "Daemon:Enabled" => {
            state.daemon_enabled = bool_from(value)?;
            Ok(())
        }
        _ => Err(DbusError::UnknownProperty(name.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn xpanid_u64(bytes: &[u8]) -> u64 {
    let mut buf = [0u8; 8];
    let n = bytes.len().min(8);
    buf[..n].copy_from_slice(&bytes[..n]);
    u64::from_le_bytes(buf)
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

fn parse_xpanid(s: &str) -> Result<Vec<u8>, DbusError> {
    let s = s.trim();
    let without_prefix = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    let without_sep = without_prefix.replace(':', "");
    if without_sep.len() > 16 {
        return Err(DbusError::Decoding("XPANID too long".into()));
    }
    let mut out = Vec::new();
    let mut chars = without_sep.chars();
    while let Some(hi) = chars.next() {
        let lo = chars
            .next()
            .ok_or(DbusError::Decoding("odd XPANID hex length".into()))?;
        let byte = u8::from_str_radix(&format!("{hi}{lo}"), 16)
            .map_err(|e| DbusError::Decoding(e.to_string()))?;
        out.push(byte);
    }
    Ok(out)
}

fn string_from(v: Variant) -> Result<String, DbusError> {
    match v {
        Value::Str(s) => Ok(s.as_str().to_string()),
        Value::U8(n) => Ok(n.to_string()),
        Value::U16(n) => Ok(n.to_string()),
        Value::U32(n) => Ok(n.to_string()),
        Value::U64(n) => Ok(n.to_string()),
        Value::I32(n) => Ok(n.to_string()),
        Value::I64(n) => Ok(n.to_string()),
        other => Err(DbusError::Decoding(format!(
            "expected string-like value, got {other:?}"
        ))),
    }
}

fn bool_from(v: Variant) -> Result<bool, DbusError> {
    match v {
        Value::Bool(b) => Ok(b),
        Value::U8(n) => Ok(n != 0),
        other => Err(DbusError::Decoding(format!("expected bool, got {other:?}"))),
    }
}

fn i8_from(v: Variant) -> Result<i8, DbusError> {
    match v {
        Value::I16(n) => Ok(n as i8),
        Value::I32(n) => Ok(n as i8),
        Value::I64(n) => Ok(n as i8),
        Value::U8(n) => Ok(n as i8),
        other => Err(DbusError::Decoding(format!(
            "expected integer, got {other:?}"
        ))),
    }
}

fn f64_from(v: Variant) -> Result<f64, DbusError> {
    match v {
        Value::F64(f) => Ok(f),
        Value::I32(n) => Ok(n as f64),
        other => Err(DbusError::Decoding(format!(
            "expected number, got {other:?}"
        ))),
    }
}

/// Render a `Variant` as a human-readable string.
///
/// Used by the D-Bus `PropGet`/`Status` methods, which return stringified
/// values. (zbus 4.x cannot serialize a bare `Value`/`OwnedValue` as a
/// method return — the server would never reply — so properties are
/// surfaced as strings over the bus, matching the C daemon's common
/// stringified representation.)
pub fn variant_to_string(v: &Variant) -> String {
    match v {
        Value::U8(n) => n.to_string(),
        Value::U16(n) => n.to_string(),
        Value::U32(n) => n.to_string(),
        Value::U64(n) => n.to_string(),
        Value::I16(n) => n.to_string(),
        Value::I32(n) => n.to_string(),
        Value::I64(n) => n.to_string(),
        Value::F64(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Str(s) => s.to_string(),
        other => format!("{other:?}"),
    }
}

/// Returns every property key this dispatcher recognizes.
pub fn all_property_keys() -> &'static [&'static str] {
    &[
        "NCP:State",
        "NCP:Version",
        "NCP:ProtocolVersion",
        "NCP:InterfaceType",
        "NCP:HardwareAddress",
        "NCP:ExtendedAddress",
        "NCP:MACAddress",
        "NCP:CCAThreshold",
        "NCP:TXPower",
        "NCP:Region",
        "NCP:ModeID",
        "NCP:Channel",
        "NCP:Frequency",
        "NCP:RSSI",
        "Network:Name",
        "Network:PANID",
        "Network:XPANID",
        "Network:NodeType",
        "Network:IsCommissioned",
        "Network:IsConnected",
        "Network:Key",
        "IPv6:LinkLocalAddress",
        "IPv6:MeshLocalAddress",
        "IPv6:MeshLocalPrefix",
        "Interface:Up",
        "Stack:Up",
        "Daemon:Version",
        "Daemon:Enabled",
        "Daemon:ReadyForHostSleep",
    ]
}
