//! D-Bus signal emission.
//!
//! Signals are emitted on the per-interface object path
//! `/com/nestlabs/WPANTunnelDriver/<iface-name>`:
//!
//! * `NetScanBeacon`  — a discovered network beacon
//! * `EnergyScanResult` — an energy-scan sample
//! * `PropChanged` — a property value changed (key + value)
//!
//! Base-level signals (`InterfaceAdded` / `InterfaceRemoved`) are emitted on
//! the base object path `/com/nestlabs/WPANTunnelDriver` by the server.

use std::collections::HashMap;

use zbus::zvariant::Value;

use crate::types::{DbusError, EnergyScanResultEntry, ScanBeacon, Variant};

/// Build the argument tuple for the `NetScanBeacon` signal.
pub(crate) fn net_scan_beacon_args(beacon: &ScanBeacon) -> HashMap<String, Variant> {
    beacon.to_dict()
}

/// Build the argument tuple for the `EnergyScanResult` signal.
pub(crate) fn energy_scan_result_args(result: &EnergyScanResultEntry) -> HashMap<String, Variant> {
    result.to_dict()
}

/// Build the argument tuple for the `PropChanged` signal.
pub(crate) fn prop_changed_args(key: &str, value: Variant) -> (String, Variant) {
    (key.to_string(), value)
}

/// Emit a `NetScanBeacon` signal on the given connection and path.
pub async fn emit_net_scan_beacon(
    conn: &zbus::Connection,
    path: &str,
    beacon: &ScanBeacon,
) -> Result<(), DbusError> {
    let args = net_scan_beacon_args(beacon);
    let body = (args,);
    conn.emit_signal(
        None::<&str>,
        path,
        crate::server::WPANTUND_DBUS_INTERFACE,
        "NetScanBeacon",
        &body,
    )
    .await?;
    Ok(())
}

/// Emit an `EnergyScanResult` signal.
pub async fn emit_energy_scan_result(
    conn: &zbus::Connection,
    path: &str,
    result: &EnergyScanResultEntry,
) -> Result<(), DbusError> {
    let args = energy_scan_result_args(result);
    let body = (args,);
    conn.emit_signal(
        None::<&str>,
        path,
        crate::server::WPANTUND_DBUS_INTERFACE,
        "EnergyScanResult",
        &body,
    )
    .await?;
    Ok(())
}

/// Emit a `PropChanged` signal.
pub async fn emit_prop_changed(
    conn: &zbus::Connection,
    path: &str,
    key: &str,
    value: Variant,
) -> Result<(), DbusError> {
    let (k, v) = prop_changed_args(key, value);
    let body = (k, v);
    conn.emit_signal(
        None::<&str>,
        path,
        crate::server::WPANTUND_DBUS_INTERFACE,
        "PropChanged",
        &body,
    )
    .await?;
    Ok(())
}

/// Emit a base-level `InterfaceAdded` signal on the base object path.
pub async fn emit_interface_added(
    conn: &zbus::Connection,
    iface_name: &str,
) -> Result<(), DbusError> {
    let body = (iface_name.to_string(),);
    conn.emit_signal(
        None::<&str>,
        crate::server::WPANTUND_BASE_OBJECT_PATH,
        crate::server::WPANTUND_DBUS_INTERFACE,
        "InterfaceAdded",
        &body,
    )
    .await?;
    Ok(())
}

/// Emit a base-level `InterfaceRemoved` signal.
pub async fn emit_interface_removed(
    conn: &zbus::Connection,
    iface_name: &str,
) -> Result<(), DbusError> {
    let body = (iface_name.to_string(),);
    conn.emit_signal(
        None::<&str>,
        crate::server::WPANTUND_BASE_OBJECT_PATH,
        crate::server::WPANTUND_DBUS_INTERFACE,
        "InterfaceRemoved",
        &body,
    )
    .await?;
    Ok(())
}

/// Build a `Value::Dict` (String -> Variant) for the beacon payload, used by
/// tests and external callers that want the raw zvariant form.
#[allow(dead_code)]
pub(crate) fn beacon_dict_value(beacon: &ScanBeacon) -> Value<'static> {
    let dict = beacon.to_dict();
    zbus::zvariant::Value::from(dict)
}

/// Emit a `NetworkTimeUpdate` signal.
pub async fn emit_network_time_update(
    conn: &zbus::Connection,
    path: &str,
    network_time: u64,
    time_sync_status: i8,
) -> Result<(), DbusError> {
    let mut dict: HashMap<String, Variant> = HashMap::new();
    dict.insert("Time".to_string(), Value::U64(network_time));
    dict.insert("Status".to_string(), Value::I16(time_sync_status as i16));
    let body = (dict,);
    conn.emit_signal(
        None::<&str>,
        path,
        crate::server::WPANTUND_DBUS_INTERFACE,
        "NetworkTimeUpdate",
        &body,
    )
    .await?;
    Ok(())
}
