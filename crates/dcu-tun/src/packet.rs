//! Minimal IPv6 packet inspection for the TUN data path.
//!
//! Covers only what the daemon needs at this layer: detecting an IPv6
//! packet and parsing the fixed 40-byte IPv6 header. The heavier
//! `IPv6PacketMatcher` firewall logic from `src/util/IPv6PacketMatcher.cpp`
//! is handled by the async daemon.

use std::net::Ipv6Addr;

use crate::error::TunError;

/// Returns `true` if `buf` begins with a valid IPv6 header (>= 40 bytes and
/// version field == 6).
pub fn is_ipv6_packet(buf: &[u8]) -> bool {
    buf.len() >= 40 && (buf[0] >> 4) == 6
}

/// Returns the IPv6 payload slice from a raw TUN frame. TUN frames are
/// already IPv6 (any 4-byte protocol-info header is stripped in
/// [`crate::interface::TunnelIPv6Interface::read_packet`]), so this returns
/// `buf` unchanged.
pub fn get_ipv6_payload(buf: &[u8]) -> &[u8] {
    buf
}

/// Parsed fixed fields of an IPv6 header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IPv6Header {
    /// IP version (always 6 for these frames).
    pub version: u8,
    /// Traffic Class (DSCP + ECN).
    pub traffic_class: u8,
    /// Flow Label.
    pub flow_label: u32,
    /// Payload length in bytes (excluding the 40-byte header).
    pub payload_length: u16,
    /// Next Header (upper-layer / extension header type).
    pub next_header: u8,
    /// Hop Limit.
    pub hop_limit: u8,
    /// Source address.
    pub source: Ipv6Addr,
    /// Destination address.
    pub destination: Ipv6Addr,
}

/// Parse the fixed 40-byte IPv6 header from `buf`.
///
/// Layout (RFC 8200):
/// - `buf[0]`: version (4 bits) << 4 | traffic class (high 4 bits)
/// - `buf[1]`: traffic class (low 4 bits) << 4 | flow label (high 4 bits)
/// - `buf[2..4]`: flow label (low 16 bits)
/// - `buf[4..6]`: payload length
/// - `buf[6]`: next header
/// - `buf[7]`: hop limit
/// - `buf[8..24]`: source
/// - `buf[24..40]`: destination
pub fn parse_ipv6_header(buf: &[u8]) -> Result<IPv6Header, TunError> {
    if buf.len() < 40 {
        return Err(TunError::InvalidConfig(format!(
            "ipv6 header too short: {} bytes, need 40",
            buf.len()
        )));
    }

    let version = buf[0] >> 4;
    let traffic_class = ((buf[0] & 0x0F) << 4) | (buf[1] >> 4);
    let flow_label = (((buf[1] & 0x0F) as u32) << 16) | ((buf[2] as u32) << 8) | (buf[3] as u32);
    let payload_length = u16::from_be_bytes([buf[4], buf[5]]);
    let next_header = buf[6];
    let hop_limit = buf[7];

    let source = Ipv6Addr::from(<[u8; 16]>::try_from(&buf[8..24]).unwrap());
    let destination = Ipv6Addr::from(<[u8; 16]>::try_from(&buf[24..40]).unwrap());

    Ok(IPv6Header {
        version,
        traffic_class,
        flow_label,
        payload_length,
        next_header,
        hop_limit,
        source,
        destination,
    })
}
