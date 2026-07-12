//! Shared Spinel payload encoders and the common "configure network"
//! sequence used by `form` and `join`.
//!
//! The two tasks differ only in the final bring-up (join requires
//! `NET_REQUIRE_JOIN_EXISTING` and uses a join timeout); the credential/
//! channel/PANID/XPANID/name/key/ml-prefix setup is identical, so it lives
//! here to avoid divergence.

use std::time::Duration;

use dcu_dbus::types::Variant;
use spinel::command::CMD_NET_CLEAR;
use spinel::pack::PackWriter;
use spinel::property::{
    PROP_IPV6_ML_PREFIX, PROP_MAC_15_4_PANID, PROP_MAC_PROMISCUOUS_MODE,
    PROP_NET_KEY_SEQUENCE_COUNTER, PROP_NET_MASTER_KEY, PROP_NET_NETWORK_NAME, PROP_NET_XPANID,
    PROP_PHY_CHAN,
};
use std::collections::HashMap;

use crate::DaemonError;
use crate::instance::NcpInstanceBase;
use crate::tasks::params;

/// Command response timeout (`NCP_DEFAULT_COMMAND_RESPONSE_TIMEOUT`).
pub const CMD_TIMEOUT: Duration = Duration::from_secs(5);

/// Encode a Spinel `"b"` bool payload (1 byte: 0x00 / 0x01).
pub fn bool_payload(v: bool) -> Vec<u8> {
    let mut w = PackWriter::new();
    w.write_bool(v);
    w.into_bytes()
}

/// Encode a Spinel `"C"` uint8 payload.
pub fn u8_payload(v: u8) -> Vec<u8> {
    let mut w = PackWriter::new();
    w.write_uint8(v);
    w.into_bytes()
}

/// Encode a Spinel `"S"` uint16 payload.
pub fn u16_payload(v: u16) -> Vec<u8> {
    let mut w = PackWriter::new();
    w.write_uint16(v);
    w.into_bytes()
}

/// Encode a Spinel `"L"` uint32 payload.
pub fn u32_payload(v: u32) -> Vec<u8> {
    let mut w = PackWriter::new();
    w.write_uint32(v);
    w.into_bytes()
}

/// Encode a Spinel `"U"` NUL-terminated UTF-8 payload.
pub fn utf8_payload(s: &str) -> Vec<u8> {
    let mut w = PackWriter::new();
    w.write_utf8(s);
    w.into_bytes()
}

/// Encode a Spinel `"D"` raw-bytes payload.
pub fn bytes_payload(b: &[u8]) -> Vec<u8> {
    let mut w = PackWriter::new();
    w.write_bytes(b);
    w.into_bytes()
}

/// Build the mesh-local prefix payload: a 16-byte IPv6 address followed by
/// the prefix length (uint8). `prefix` is raw bytes (e.g. hex-decoded); only
/// the first 16 are used. Length defaults to 64.
pub fn mesh_local_prefix_payload(prefix: &[u8], prefix_len: u8) -> Vec<u8> {
    let mut w = PackWriter::new();
    let mut addr = [0u8; 16];
    let n = prefix.len().min(16);
    addr[..n].copy_from_slice(&prefix[..n]);
    w.write_ipv6(&addr);
    w.write_uint8(prefix_len);
    w.into_bytes()
}

/// Clear saved settings and apply the common network configuration from
/// `params` (channel, promiscuous-off, PANID, XPANID, name, key, key index,
/// mesh-local prefix). Used by both `form` and `join`.
pub async fn configure_network(
    ncp: &NcpInstanceBase,
    params: &HashMap<String, Variant>,
) -> Result<(), DaemonError> {
    // Clear any previously saved network settings.
    ncp.send_command(CMD_NET_CLEAR, Vec::new()).await?;

    // Channel.
    if let Some(ch) = params::get_u8(params, "NCP:Channel") {
        ncp.send_prop_set(PROP_PHY_CHAN, u8_payload(ch)).await?;
    }

    // Promiscuous mode off.
    ncp.send_prop_set(PROP_MAC_PROMISCUOUS_MODE, u8_payload(0))
        .await?;

    if let Some(panid) = params::get_u16(params, "Network:PANID") {
        ncp.send_prop_set(PROP_MAC_15_4_PANID, u16_payload(panid))
            .await?;
    }

    if let Some(xpanid) = params::get_bytes(params, "Network:XPANID") {
        ncp.send_prop_set(PROP_NET_XPANID, bytes_payload(&xpanid))
            .await?;
    }

    if let Some(name) = params::get_str(params, "Network:Name") {
        ncp.send_prop_set(PROP_NET_NETWORK_NAME, utf8_payload(&name))
            .await?;
    }

    if let Some(key) = params::get_bytes(params, "Network:Key") {
        ncp.send_prop_set(PROP_NET_MASTER_KEY, bytes_payload(&key))
            .await?;
    }

    if let Some(idx) = params::get_u16(params, "Network:KeyIndex") {
        ncp.send_prop_set(PROP_NET_KEY_SEQUENCE_COUNTER, u32_payload(idx as u32))
            .await?;
    }

    if let Some(prefix) = params::get_bytes(params, "IPv6:MeshLocalPrefix") {
        let prefix_len = params::get_u8(params, "IPv6:MeshLocalPrefixLen").unwrap_or(64);
        ncp.send_prop_set(
            PROP_IPV6_ML_PREFIX,
            mesh_local_prefix_payload(&prefix, prefix_len),
        )
        .await?;
    }

    Ok(())
}
