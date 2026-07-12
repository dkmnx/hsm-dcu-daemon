//! `get_topology`, `get_buffer_counters` — NCP introspection queries.
//!
//! Ports of `SpinelNCPTaskGetNetworkTopology.cpp` and
//! `SpinelNCPTaskGetMsgBufferCounters.cpp`.

use spinel::pack::PackReader;
use spinel::property::PROP_MSG_BUFFER_COUNTERS;

use crate::DaemonError;
use crate::instance::NcpInstanceBase;

/// A router-table entry (a subset of the C `TableEntry`, sufficient for the
/// Rust port). Decoded from the `PROP_THREAD_ROUTER_TABLE` value, whose per-row
/// format is `EUI64, uint16, uint8, uint8, uint8, uint8, uint8, uint8, bool`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouterEntry {
    pub ext_address: [u8; 8],
    pub rloc16: u16,
    pub router_id: u8,
    pub next_hop: u8,
    pub path_cost: u8,
    pub link_quality_in: u8,
    pub link_quality_out: u8,
    pub age: u8,
    pub link_established: bool,
}

/// Message buffer counters (16 `uint16` fields, format `"SSSSSSSSSSSSSSSS"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MsgBufferCounters {
    pub total_buffers: u16,
    pub free_buffers: u16,
    pub lo_send_messages: u16,
    pub lo_send_buffers: u16,
    pub lo_reassembly_messages: u16,
    pub lo_reassembly_buffers: u16,
    pub ip6_messages: u16,
    pub ip6_buffers: u16,
    pub mpl_messages: u16,
    pub mpl_buffers: u16,
    pub mle_messages: u16,
    pub mle_buffers: u16,
    pub arp_messages: u16,
    pub arp_buffers: u16,
    pub coap_client_messages: u16,
    pub coap_client_buffers: u16,
}

impl MsgBufferCounters {
    fn from_reader(r: &mut PackReader<'_>) -> Option<MsgBufferCounters> {
        Some(MsgBufferCounters {
            total_buffers: r.read_uint16().ok()?,
            free_buffers: r.read_uint16().ok()?,
            lo_send_messages: r.read_uint16().ok()?,
            lo_send_buffers: r.read_uint16().ok()?,
            lo_reassembly_messages: r.read_uint16().ok()?,
            lo_reassembly_buffers: r.read_uint16().ok()?,
            ip6_messages: r.read_uint16().ok()?,
            ip6_buffers: r.read_uint16().ok()?,
            mpl_messages: r.read_uint16().ok()?,
            mpl_buffers: r.read_uint16().ok()?,
            mle_messages: r.read_uint16().ok()?,
            mle_buffers: r.read_uint16().ok()?,
            arp_messages: r.read_uint16().ok()?,
            arp_buffers: r.read_uint16().ok()?,
            coap_client_messages: r.read_uint16().ok()?,
            coap_client_buffers: r.read_uint16().ok()?,
        })
    }
}

/// Query the NCP message buffer counters.
pub async fn get_buffer_counters(ncp: &NcpInstanceBase) -> Result<MsgBufferCounters, DaemonError> {
    let resp = ncp.send_prop_get(PROP_MSG_BUFFER_COUNTERS).await?;
    let mut r = PackReader::new(&resp.payload);
    // Skip the property-key prefix (packed uint) of the PROP_VALUE_IS frame.
    let _ = r.read_uint_packed();
    MsgBufferCounters::from_reader(&mut r)
        .ok_or_else(|| DaemonError::Ncp("malformed MSG_BUFFER_COUNTERS payload".into()))
}

/// Query the NCP thread router table.
///
/// Only the router-table layout (`PROP_THREAD_ROUTER_TABLE`) is decoded in
/// this port; other table shapes (neighbor/child) have different field
/// layouts and must fail loudly rather than mis-decode. Use
/// [`get_router_table`] for the common case.
pub async fn get_topology(
    ncp: &NcpInstanceBase,
    prop: u32,
) -> Result<Vec<RouterEntry>, DaemonError> {
    if prop != spinel::property::PROP_THREAD_ROUTER_TABLE {
        return Err(DaemonError::Ncp(format!(
            "unsupported topology property 0x{prop:04X} (only router table decoded)"
        )));
    }
    let resp = ncp.send_prop_get(prop).await?;
    let mut r = PackReader::new(&resp.payload);
    let _ = r.read_uint_packed(); // property key prefix
    let mut out = Vec::new();
    while r.remaining() > 0 {
        // Each entry is a length-prefixed struct ("d" / DataWithLen wrapper).
        let entry = match r.read_data_with_len() {
            Ok(e) => e,
            Err(_) => break,
        };
        let mut er = PackReader::new(entry);
        let ext_address = match er.read_eui64() {
            Ok(a) => a,
            Err(_) => break,
        };
        let entry = RouterEntry {
            ext_address,
            rloc16: er.read_uint16().unwrap_or(0),
            router_id: er.read_uint8().unwrap_or(0),
            next_hop: er.read_uint8().unwrap_or(0),
            path_cost: er.read_uint8().unwrap_or(0),
            link_quality_in: er.read_uint8().unwrap_or(0),
            link_quality_out: er.read_uint8().unwrap_or(0),
            age: er.read_uint8().unwrap_or(0),
            link_established: er.read_bool().unwrap_or(false),
        };
        out.push(entry);
    }
    Ok(out)
}

/// Convenience: query the router table specifically.
pub async fn get_router_table(ncp: &NcpInstanceBase) -> Result<Vec<RouterEntry>, DaemonError> {
    get_topology(ncp, spinel::property::PROP_THREAD_ROUTER_TABLE).await
}
