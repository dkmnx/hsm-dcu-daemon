//! IPv6 packet matcher for firewall/packet classification.
//!
//! Replaces C's `IPv6PacketMatcher.{h,cpp}` from `src/util/`.
//! Provides rule-based matching on IPv6 packets by protocol type,
//! ICMPv6 subtype, ports, and addresses with prefix masks.

use std::collections::BTreeSet;
use std::net::Ipv6Addr;

/// IPv6 header length (fixed).
const IPV6_HEADER_LEN: usize = 40;

// ─── Protocol type constants ─────────────────────────────────────────

/// Next-header / protocol type constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ProtocolType {
    /// Match any protocol.
    All = 0xFF,
    /// Match no protocol (always fails).
    None = 0xFE,
    /// Hop-by-hop options header.
    HopByHop = 0,
    /// TCP.
    Tcp = 6,
    /// UDP.
    Udp = 17,
    /// ICMPv6.
    Icmpv6 = 58,
}

impl ProtocolType {
    /// Convert from a raw next-header byte.
    pub fn from_raw(val: u8) -> Self {
        match val {
            0 => ProtocolType::HopByHop,
            6 => ProtocolType::Tcp,
            17 => ProtocolType::Udp,
            58 => ProtocolType::Icmpv6,
            0xFF => ProtocolType::All,
            0xFE => ProtocolType::None,
            _ => ProtocolType::All, // treat unknown as "match all"
        }
    }
}

/// ICMPv6 subtype constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Icmpv6Subtype {
    /// Match any ICMPv6 type.
    All = 0xFF,
    /// Router Solicitation.
    RouterSol = 133,
    /// Router Advertisement.
    RouterAdv = 134,
    /// Neighbor Solicitation.
    NeighborSol = 135,
    /// Neighbor Advertisement.
    NeighborAdv = 136,
    /// Redirect.
    Redirect = 137,
}

impl Icmpv6Subtype {
    /// Convert from a raw ICMPv6 type byte.
    pub fn from_raw(val: u8) -> Self {
        match val {
            133 => Icmpv6Subtype::RouterSol,
            134 => Icmpv6Subtype::RouterAdv,
            135 => Icmpv6Subtype::NeighborSol,
            136 => Icmpv6Subtype::NeighborAdv,
            137 => Icmpv6Subtype::Redirect,
            _ => Icmpv6Subtype::All,
        }
    }
}

// ─── Packet field extraction helpers ─────────────────────────────────

/// Check if a buffer is a valid IPv6 packet (version == 6, length >= 40).
pub fn is_ipv6_packet(buf: &[u8]) -> bool {
    buf.len() >= IPV6_HEADER_LEN && (buf[0] & 0xF0) == 0x60
}

/// Get the next-header (protocol) field from an IPv6 packet.
pub fn get_next_header(buf: &[u8]) -> u8 {
    if buf.len() >= 7 { buf[6] } else { 0 }
}

/// Get the source port from a TCP/UDP packet (bytes 40-41, big-endian).
pub fn get_src_port(buf: &[u8]) -> u16 {
    if buf.len() >= 42 {
        u16::from_be_bytes([buf[40], buf[41]])
    } else {
        0
    }
}

/// Get the destination port from a TCP/UDP packet (bytes 42-43, big-endian).
pub fn get_dst_port(buf: &[u8]) -> u16 {
    if buf.len() >= 44 {
        u16::from_be_bytes([buf[42], buf[43]])
    } else {
        0
    }
}

/// Get the ICMPv6 type byte (byte 40, first byte after IPv6 header).
pub fn get_icmpv6_type(buf: &[u8]) -> u8 {
    if buf.len() > 40 { buf[40] } else { 0 }
}

/// Get the source IPv6 address from a packet (bytes 8-23).
pub fn get_src_addr(buf: &[u8]) -> Ipv6Addr {
    if buf.len() >= 24 {
        let mut octets = [0u8; 16];
        octets.copy_from_slice(&buf[8..24]);
        Ipv6Addr::from(octets)
    } else {
        Ipv6Addr::UNSPECIFIED
    }
}

/// Get the destination IPv6 address from a packet (bytes 24-39).
pub fn get_dst_addr(buf: &[u8]) -> Ipv6Addr {
    if buf.len() >= 40 {
        let mut octets = [0u8; 16];
        octets.copy_from_slice(&buf[24..40]);
        Ipv6Addr::from(octets)
    } else {
        Ipv6Addr::UNSPECIFIED
    }
}

/// Apply a prefix-length mask to an IPv6 address.
/// Equivalent to C's `in6_addr_apply_mask()` from `IPv6Helpers.h`.
pub fn apply_mask(addr: Ipv6Addr, prefix_len: u8) -> Ipv6Addr {
    if prefix_len == 0 {
        return Ipv6Addr::UNSPECIFIED;
    }
    let mut octets = addr.octets();
    let full_bytes = (prefix_len / 8) as usize;
    let remaining_bits = prefix_len % 8;

    // Mask the partial byte (if any) BEFORE zeroing the rest
    if remaining_bits > 0 && full_bytes < 16 {
        octets[full_bytes] &= 0xFF << (8 - remaining_bits);
        // Zero bytes strictly after the partial byte
        for byte in octets.iter_mut().skip(full_bytes + 1) {
            *byte = 0;
        }
    } else {
        // No partial byte: zero everything from full_bytes onward
        for byte in octets.iter_mut().skip(full_bytes) {
            *byte = 0;
        }
    }
    Ipv6Addr::from(octets)
}

// ─── PacketMatcherRule ──────────────────────────────────────────────

/// A single packet matching rule.
///
/// Mirrors C's `IPv6PacketMatcherRule` struct. Fields use `Option` for
/// port/address matching (None = don't match) and explicit `bool` flags
/// to match the C semantics of `local_port_match` / `remote_port_match`.
#[derive(Debug, Clone)]
pub struct PacketMatcherRule {
    pub protocol: ProtocolType,
    pub subtype: Icmpv6Subtype,
    pub local_port: u16,
    pub local_port_match: bool,
    pub local_address: Ipv6Addr,
    pub local_match_mask: u8,
    pub remote_port: u16,
    pub remote_port_match: bool,
    pub remote_address: Ipv6Addr,
    pub remote_match_mask: u8,
}

impl Default for PacketMatcherRule {
    fn default() -> Self {
        Self {
            protocol: ProtocolType::All,
            subtype: Icmpv6Subtype::All,
            local_port: 0,
            local_port_match: false,
            local_address: Ipv6Addr::UNSPECIFIED,
            local_match_mask: 0,
            remote_port: 0,
            remote_port_match: false,
            remote_address: Ipv6Addr::UNSPECIFIED,
            remote_match_mask: 0,
        }
    }
}

impl PartialEq for PacketMatcherRule {
    fn eq(&self, other: &Self) -> bool {
        self.protocol == other.protocol
            && self.subtype == other.subtype
            && self.local_port == other.local_port
            && self.local_port_match == other.local_port_match
            && self.local_match_mask == other.local_match_mask
            && self.local_address == other.local_address
            && self.remote_port == other.remote_port
            && self.remote_port_match == other.remote_port_match
            && self.remote_match_mask == other.remote_match_mask
            && self.remote_address == other.remote_address
    }
}

impl Eq for PacketMatcherRule {}

impl PartialOrd for PacketMatcherRule {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PacketMatcherRule {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.protocol
            .cmp(&other.protocol)
            .then_with(|| self.subtype.cmp(&other.subtype))
            .then_with(|| self.local_port.cmp(&other.local_port))
            .then_with(|| self.local_port_match.cmp(&other.local_port_match))
            .then_with(|| self.local_match_mask.cmp(&other.local_match_mask))
            .then_with(|| {
                self.local_address
                    .octets()
                    .cmp(&other.local_address.octets())
            })
            .then_with(|| self.remote_port.cmp(&other.remote_port))
            .then_with(|| self.remote_port_match.cmp(&other.remote_port_match))
            .then_with(|| self.remote_match_mask.cmp(&other.remote_match_mask))
            .then_with(|| {
                self.remote_address
                    .octets()
                    .cmp(&other.remote_address.octets())
            })
    }
}

impl PacketMatcherRule {
    /// Reset all fields to defaults (type=ALL, subtype=ALL, no port/address matching).
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Populate this rule from an inbound packet (NCP → host).
    ///
    /// For inbound: local = destination (our side), remote = source.
    pub fn update_from_inbound_packet(&mut self, packet: &[u8]) -> &mut Self {
        self.clear();

        if !is_ipv6_packet(packet) {
            return self;
        }

        self.protocol = ProtocolType::from_raw(get_next_header(packet));
        self.subtype = Icmpv6Subtype::All;

        let proto = self.protocol;
        if proto == ProtocolType::Tcp || proto == ProtocolType::Udp {
            self.remote_port = get_src_port(packet);
            self.remote_port_match = true;
            self.local_port = get_dst_port(packet);
            self.local_port_match = true;
        } else {
            self.remote_port = 0;
            self.remote_port_match = false;
            self.local_port = 0;
            self.local_port_match = false;
            if proto == ProtocolType::Icmpv6 {
                self.subtype = Icmpv6Subtype::from_raw(get_icmpv6_type(packet));
            }
        }

        let dst = get_dst_addr(packet);
        if !dst.is_multicast() {
            self.local_address = dst;
            self.local_match_mask = 128;
        } else {
            self.local_match_mask = 0;
        }

        self.remote_address = get_src_addr(packet);
        self.remote_match_mask = 128;

        self
    }

    /// Direction-independent protocol + ICMPv6 subtype check shared by
    /// `match_inbound` and `match_outbound`. Returns `false` if the rule's
    /// `None` protocol rejects the packet, `true` if the rule is `All`, and
    /// otherwise whether the packet's next-header and (for ICMPv6) type match.
    fn matches_protocol(&self, packet: &[u8]) -> bool {
        if self.protocol == ProtocolType::None {
            return false;
        }
        if self.protocol == ProtocolType::All {
            return true;
        }
        if self.protocol != ProtocolType::from_raw(get_next_header(packet)) {
            return false;
        }
        if self.subtype != Icmpv6Subtype::All
            && self.subtype != Icmpv6Subtype::from_raw(get_icmpv6_type(packet))
        {
            return false;
        }
        true
    }

    /// Test if an inbound packet matches this rule.
    pub fn match_inbound(&self, packet: &[u8]) -> bool {
        if !is_ipv6_packet(packet) || !self.matches_protocol(packet) {
            return false;
        }

        if self.local_port_match && self.local_port != get_dst_port(packet) {
            return false;
        }
        if self.remote_port_match && self.remote_port != get_src_port(packet) {
            return false;
        }

        if self.local_match_mask > 0 {
            let addr = apply_mask(get_dst_addr(packet), self.local_match_mask);
            if addr != self.local_address {
                return false;
            }
        }
        if self.remote_match_mask > 0 {
            let addr = apply_mask(get_src_addr(packet), self.remote_match_mask);
            if addr != self.remote_address {
                return false;
            }
        }

        true
    }

    /// Populate this rule from an outbound packet (host → NCP).
    ///
    /// For outbound: local = source (our side), remote = destination.
    pub fn update_from_outbound_packet(&mut self, packet: &[u8]) -> &mut Self {
        self.clear();

        if !is_ipv6_packet(packet) {
            return self;
        }

        self.protocol = ProtocolType::from_raw(get_next_header(packet));
        self.subtype = Icmpv6Subtype::All;

        let proto = self.protocol;
        if proto == ProtocolType::Tcp || proto == ProtocolType::Udp {
            self.remote_port = get_dst_port(packet);
            self.remote_port_match = true;
            self.local_port = get_src_port(packet);
            self.local_port_match = true;
        } else {
            self.remote_port = 0;
            self.remote_port_match = false;
            self.local_port = 0;
            self.local_port_match = false;
            if proto == ProtocolType::Icmpv6 {
                self.subtype = Icmpv6Subtype::from_raw(get_icmpv6_type(packet));
            }
        }

        self.local_address = get_src_addr(packet);
        self.local_match_mask = 128;

        self.remote_address = get_dst_addr(packet);
        self.remote_match_mask = 128;

        self
    }

    /// Test if an outbound packet matches this rule.
    pub fn match_outbound(&self, packet: &[u8]) -> bool {
        if !is_ipv6_packet(packet) || !self.matches_protocol(packet) {
            return false;
        }

        if self.local_port_match && self.local_port != get_src_port(packet) {
            return false;
        }
        if self.remote_port_match && self.remote_port != get_dst_port(packet) {
            return false;
        }

        if self.local_match_mask > 0 {
            let addr = apply_mask(get_src_addr(packet), self.local_match_mask);
            if addr != self.local_address {
                return false;
            }
        }
        if self.remote_match_mask > 0 {
            let addr = apply_mask(get_dst_addr(packet), self.remote_match_mask);
            if addr != self.remote_address {
                return false;
            }
        }

        true
    }
}

// ─── PacketMatcher (set container) ──────────────────────────────────

/// A set of packet matching rules.
///
/// Mirrors C's `IPv6PacketMatcher` (which extends `std::set`).
/// Uses `BTreeSet` for deterministic ordering.
#[derive(Debug, Clone, Default)]
pub struct PacketMatcher {
    rules: BTreeSet<PacketMatcherRule>,
}

impl PacketMatcher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, rule: PacketMatcherRule) {
        self.rules.insert(rule);
    }

    pub fn remove(&mut self, rule: &PacketMatcherRule) {
        self.rules.remove(rule);
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Find the first rule that matches an outbound packet.
    /// Returns `Some(&rule)` if found, `None` if no rule matches.
    pub fn match_outbound(&self, packet: &[u8]) -> Option<&PacketMatcherRule> {
        self.rules.iter().find(|r| r.match_outbound(packet))
    }

    /// Find the first rule that matches an inbound packet.
    /// Returns `Some(&rule)` if found, `None` if no rule matches.
    pub fn match_inbound(&self, packet: &[u8]) -> Option<&PacketMatcherRule> {
        self.rules.iter().find(|r| r.match_inbound(packet))
    }
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid IPv6+TCP packet for testing.
    fn make_tcp_packet(src: Ipv6Addr, dst: Ipv6Addr, src_port: u16, dst_port: u16) -> Vec<u8> {
        let mut buf = vec![0u8; 60]; // IPv6 header (40) + TCP header (20 min)
        // Version 6, no traffic class/flow label
        buf[0] = 0x60;
        // Payload length = 20 (TCP header)
        buf[4] = 0;
        buf[5] = 20;
        // Next header = TCP (6)
        buf[6] = 6;
        // Hop limit
        buf[7] = 64;
        // Source address
        buf[8..24].copy_from_slice(&src.octets());
        // Destination address
        buf[24..40].copy_from_slice(&dst.octets());
        // Source port (big-endian)
        buf[40..42].copy_from_slice(&src_port.to_be_bytes());
        // Destination port (big-endian)
        buf[42..44].copy_from_slice(&dst_port.to_be_bytes());
        buf
    }

    /// Build a minimal valid IPv6+ICMPv6 packet.
    fn make_icmpv6_packet(src: Ipv6Addr, dst: Ipv6Addr, icmp_type: u8) -> Vec<u8> {
        let mut buf = vec![0u8; 48]; // IPv6 header (40) + ICMPv6 (8 min)
        buf[0] = 0x60;
        buf[4] = 0;
        buf[5] = 8;
        buf[6] = 58; // ICMPv6
        buf[7] = 255;
        buf[8..24].copy_from_slice(&src.octets());
        buf[24..40].copy_from_slice(&dst.octets());
        buf[40] = icmp_type;
        buf
    }

    fn local() -> Ipv6Addr {
        "2001:db8::1".parse().unwrap()
    }

    fn remote() -> Ipv6Addr {
        "2001:db8::2".parse().unwrap()
    }

    // ─── Packet field extraction ────────────────────────────────

    #[test]
    fn extract_next_header() {
        let pkt = make_tcp_packet(local(), remote(), 12345, 80);
        assert_eq!(get_next_header(&pkt), 6);
    }

    #[test]
    fn extract_ports() {
        let pkt = make_tcp_packet(local(), remote(), 12345, 80);
        assert_eq!(get_src_port(&pkt), 12345);
        assert_eq!(get_dst_port(&pkt), 80);
    }

    #[test]
    fn extract_addrs() {
        let pkt = make_tcp_packet(local(), remote(), 80, 443);
        assert_eq!(get_src_addr(&pkt), local());
        assert_eq!(get_dst_addr(&pkt), remote());
    }

    #[test]
    fn extract_icmpv6_type() {
        let pkt = make_icmpv6_packet(local(), remote(), 135);
        assert_eq!(get_icmpv6_type(&pkt), 135);
    }

    // ─── Address masking ────────────────────────────────────────

    #[test]
    fn apply_mask_full() {
        let addr: Ipv6Addr = "2001:db8::1".parse().unwrap();
        assert_eq!(apply_mask(addr, 128), addr);
    }

    #[test]
    fn apply_mask_64() {
        let addr: Ipv6Addr = "2001:db8:1::1".parse().unwrap();
        let masked = apply_mask(addr, 64);
        assert_eq!(masked, "2001:db8:1::".parse::<Ipv6Addr>().unwrap());
    }

    #[test]
    fn apply_mask_zero() {
        let addr: Ipv6Addr = "2001:db8::1".parse().unwrap();
        assert_eq!(apply_mask(addr, 0), Ipv6Addr::UNSPECIFIED);
    }

    #[test]
    fn apply_mask_partial_byte() {
        let addr: Ipv6Addr = "2001:db8::1".parse().unwrap();
        let masked = apply_mask(addr, 120);
        // /120 zeros the last byte
        assert_eq!(masked, "2001:db8::".parse::<Ipv6Addr>().unwrap());
    }

    #[test]
    fn apply_mask_non_byte_aligned() {
        // /124: top 4 bits of last byte preserved, rest zeroed
        // 0x01 = 0b0000_0001 → masked to 0b0000_0000 = 0x00
        let addr: Ipv6Addr = "2001:db8::1".parse().unwrap();
        let masked = apply_mask(addr, 124);
        assert_eq!(masked, "2001:db8::".parse::<Ipv6Addr>().unwrap());

        // /4: top nibble preserved
        // 0x20 = 0b0010_0000 → masked to 0b0010_0000 = 0x20
        let addr2: Ipv6Addr = "2001:db8::1".parse().unwrap();
        let masked2 = apply_mask(addr2, 4);
        assert_eq!(masked2.octets()[0], 0x20);
        // All bytes after first should be zero
        assert_eq!(&masked2.octets()[1..], &[0u8; 15]);
    }

    // ─── Protocol constants ─────────────────────────────────────

    #[test]
    fn protocol_type_from_raw() {
        assert_eq!(ProtocolType::from_raw(6), ProtocolType::Tcp);
        assert_eq!(ProtocolType::from_raw(17), ProtocolType::Udp);
        assert_eq!(ProtocolType::from_raw(58), ProtocolType::Icmpv6);
        assert_eq!(ProtocolType::from_raw(0), ProtocolType::HopByHop);
        assert_eq!(ProtocolType::from_raw(0xFF), ProtocolType::All);
        assert_eq!(ProtocolType::from_raw(0xFE), ProtocolType::None);
    }

    #[test]
    fn icmpv6_subtype_from_raw() {
        assert_eq!(Icmpv6Subtype::from_raw(133), Icmpv6Subtype::RouterSol);
        assert_eq!(Icmpv6Subtype::from_raw(134), Icmpv6Subtype::RouterAdv);
        assert_eq!(Icmpv6Subtype::from_raw(135), Icmpv6Subtype::NeighborSol);
        assert_eq!(Icmpv6Subtype::from_raw(136), Icmpv6Subtype::NeighborAdv);
        assert_eq!(Icmpv6Subtype::from_raw(137), Icmpv6Subtype::Redirect);
    }

    // ─── Rule matching ──────────────────────────────────────────

    #[test]
    fn rule_update_inbound_tcp() {
        let pkt = make_tcp_packet(remote(), local(), 80, 12345);
        let mut rule = PacketMatcherRule::default();
        rule.update_from_inbound_packet(&pkt);
        assert_eq!(rule.protocol, ProtocolType::Tcp);
        assert!(rule.local_port_match);
        assert_eq!(rule.local_port, 12345);
        assert!(rule.remote_port_match);
        assert_eq!(rule.remote_port, 80);
        assert_eq!(rule.local_address, local());
        assert_eq!(rule.remote_address, remote());
    }

    #[test]
    fn rule_update_outbound_tcp() {
        let pkt = make_tcp_packet(local(), remote(), 12345, 80);
        let mut rule = PacketMatcherRule::default();
        rule.update_from_outbound_packet(&pkt);
        assert_eq!(rule.protocol, ProtocolType::Tcp);
        assert!(rule.local_port_match);
        assert_eq!(rule.local_port, 12345);
        assert!(rule.remote_port_match);
        assert_eq!(rule.remote_port, 80);
        assert_eq!(rule.local_address, local());
        assert_eq!(rule.remote_address, remote());
    }

    #[test]
    fn rule_match_inbound_exact() {
        let pkt = make_tcp_packet(remote(), local(), 80, 12345);
        let mut rule = PacketMatcherRule::default();
        rule.update_from_inbound_packet(&pkt);
        assert!(rule.match_inbound(&pkt));
    }

    #[test]
    fn rule_match_outbound_exact() {
        let pkt = make_tcp_packet(local(), remote(), 12345, 80);
        let mut rule = PacketMatcherRule::default();
        rule.update_from_outbound_packet(&pkt);
        assert!(rule.match_outbound(&pkt));
    }

    #[test]
    fn rule_match_inbound_wrong_port() {
        let pkt = make_tcp_packet(remote(), local(), 80, 12345);
        let mut rule = PacketMatcherRule::default();
        rule.update_from_inbound_packet(&pkt);
        // Modify destination port in a copy
        let mut pkt2 = pkt.clone();
        pkt2[43] = 0xFF; // change dst port
        assert!(!rule.match_inbound(&pkt2));
    }

    #[test]
    fn rule_match_inbound_wrong_protocol() {
        let tcp_pkt = make_tcp_packet(remote(), local(), 80, 12345);
        let mut rule = PacketMatcherRule::default();
        rule.update_from_inbound_packet(&tcp_pkt);
        // ICMPv6 packet should not match a TCP rule
        let icmp_pkt = make_icmpv6_packet(remote(), local(), 135);
        assert!(!rule.match_inbound(&icmp_pkt));
    }

    #[test]
    fn rule_type_none_always_fails() {
        let pkt = make_tcp_packet(remote(), local(), 80, 12345);
        let rule = PacketMatcherRule {
            protocol: ProtocolType::None,
            ..Default::default()
        };
        assert!(!rule.match_inbound(&pkt));
        assert!(!rule.match_outbound(&pkt));
    }

    #[test]
    fn rule_type_all_matches_any() {
        let tcp_pkt = make_tcp_packet(remote(), local(), 80, 12345);
        let rule = PacketMatcherRule {
            protocol: ProtocolType::All,
            ..Default::default()
        };
        assert!(rule.match_inbound(&tcp_pkt));

        let icmp_pkt = make_icmpv6_packet(remote(), local(), 135);
        assert!(rule.match_inbound(&icmp_pkt));
    }

    #[test]
    fn rule_icmpv6_subtype_match() {
        let pkt = make_icmpv6_packet(remote(), local(), 135);
        let rule = PacketMatcherRule {
            protocol: ProtocolType::Icmpv6,
            subtype: Icmpv6Subtype::NeighborSol,
            ..Default::default()
        };
        assert!(rule.match_inbound(&pkt));

        // Different subtype should not match
        let rule2 = PacketMatcherRule {
            protocol: ProtocolType::Icmpv6,
            subtype: Icmpv6Subtype::RouterAdv,
            ..Default::default()
        };
        assert!(!rule2.match_inbound(&pkt));
    }

    #[test]
    fn rule_multicast_no_address_match() {
        let multicast = "ff02::1".parse::<Ipv6Addr>().unwrap();
        let pkt = make_tcp_packet(remote(), multicast, 80, 12345);
        let mut rule = PacketMatcherRule::default();
        rule.update_from_inbound_packet(&pkt);
        // Multicast destination: local_match_mask should be 0 (no address match)
        assert_eq!(rule.local_match_mask, 0);
        assert!(rule.match_inbound(&pkt));
    }

    // ─── PacketMatcher container ────────────────────────────────

    #[test]
    fn matcher_insert_and_match() {
        let mut matcher = PacketMatcher::new();
        let pkt = make_tcp_packet(remote(), local(), 80, 12345);
        let mut rule = PacketMatcherRule::default();
        rule.update_from_inbound_packet(&pkt);
        matcher.insert(rule);
        assert!(matcher.match_inbound(&pkt).is_some());
        assert_eq!(matcher.len(), 1);
    }

    #[test]
    fn matcher_no_match() {
        let matcher = PacketMatcher::new();
        let pkt = make_tcp_packet(remote(), local(), 80, 12345);
        assert!(matcher.match_inbound(&pkt).is_none());
    }

    #[test]
    fn matcher_first_match_wins() {
        let mut matcher = PacketMatcher::new();
        let pkt = make_tcp_packet(remote(), local(), 80, 12345);

        // Rule 1: match all
        matcher.insert(PacketMatcherRule {
            protocol: ProtocolType::All,
            ..Default::default()
        });

        // Rule 2: match TCP port 80 specifically
        matcher.insert(PacketMatcherRule {
            protocol: ProtocolType::Tcp,
            local_port_match: true,
            local_port: 12345,
            ..Default::default()
        });

        // First match should be rule1 (type=ALL comes after TCP in ordering,
        // but BTreeSet is sorted, so TCP rule comes first)
        let matched = matcher.match_inbound(&pkt).unwrap();
        assert_eq!(matched.protocol, ProtocolType::Tcp);
    }

    #[test]
    fn matcher_remove() {
        let mut matcher = PacketMatcher::new();
        let rule = PacketMatcherRule {
            protocol: ProtocolType::Tcp,
            ..Default::default()
        };
        matcher.insert(rule.clone());
        assert_eq!(matcher.len(), 1);
        matcher.remove(&rule);
        assert_eq!(matcher.len(), 0);
    }

    // ─── Ordering and equality ──────────────────────────────────

    #[test]
    fn rule_ordering() {
        let r1 = PacketMatcherRule {
            protocol: ProtocolType::Tcp,
            ..Default::default()
        };

        let r2 = PacketMatcherRule {
            protocol: ProtocolType::Udp,
            ..Default::default()
        };

        assert!(r1 < r2);
        assert!(r2 > r1);
    }

    #[test]
    fn rule_equality() {
        let r1 = PacketMatcherRule::default();
        let r2 = PacketMatcherRule::default();
        assert_eq!(r1, r2);
    }

    // ─── Non-IPv6 packets ──────────────────────────────────────

    #[test]
    fn non_ipv6_packet_fails() {
        let pkt = vec![0x00; 60]; // not IPv6
        let rule = PacketMatcherRule::default();
        assert!(!rule.match_inbound(&pkt));
        assert!(!rule.match_outbound(&pkt));
    }

    #[test]
    fn short_packet_fails() {
        let pkt = vec![0x60; 10]; // IPv6 version but too short
        let rule = PacketMatcherRule::default();
        assert!(!rule.match_inbound(&pkt));
    }
}
