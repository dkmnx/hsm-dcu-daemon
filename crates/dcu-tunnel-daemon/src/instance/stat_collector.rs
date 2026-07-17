use std::collections::VecDeque;
use std::net::Ipv6Addr;
use std::time::Instant;

use wisun_types::NcpState;

const MAX_HISTORY: usize = 64;

/// IPv6 next-header protocol classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Protocol {
    Tcp,
    Udp,
    Icmp,
    Other,
}

fn classify_packet(packet: &[u8]) -> (Ipv6Addr, Ipv6Addr, u16, Protocol) {
    if packet.len() < 40 {
        return (
            Ipv6Addr::UNSPECIFIED,
            Ipv6Addr::UNSPECIFIED,
            0,
            Protocol::Other,
        );
    }
    let payload_len = u16::from_be_bytes([packet[4], packet[5]]);
    let next_header = packet[6];
    let src = Ipv6Addr::from(<[u8; 16]>::try_from(&packet[8..24]).unwrap());
    let dst = Ipv6Addr::from(<[u8; 16]>::try_from(&packet[24..40]).unwrap());
    let proto = match next_header {
        6 => Protocol::Tcp,
        17 => Protocol::Udp,
        58 => Protocol::Icmp,
        _ => Protocol::Other,
    };
    (src, dst, payload_len, proto)
}

/// Information about a single recorded packet.
#[derive(Debug, Clone)]
pub struct PacketInfo {
    pub timestamp: Instant,
    pub payload_len: u16,
    pub src: Ipv6Addr,
    pub dst: Ipv6Addr,
}

/// Information about a recorded NCP state change.
#[derive(Debug, Clone)]
pub struct NcpStateInfo {
    pub timestamp: Instant,
    pub state: NcpState,
}

/// Packet and NCP state statistics collector.
///
/// Mirrors the C `StatCollector` (~1737 LOC) that tracks packet stats,
/// NCP state history, and serves `Stat:*` properties via D-Bus.
pub struct StatCollector {
    start_time: Instant,

    rx_packets_total: u32,
    tx_packets_total: u32,
    rx_bytes_total: u64,
    tx_bytes_total: u64,

    rx_packets_udp: u32,
    rx_packets_tcp: u32,
    rx_packets_icmp: u32,
    tx_packets_udp: u32,
    tx_packets_tcp: u32,
    tx_packets_icmp: u32,

    rx_history: VecDeque<PacketInfo>,
    tx_history: VecDeque<PacketInfo>,
    ncp_state_history: VecDeque<NcpStateInfo>,
}

impl Default for StatCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl StatCollector {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            rx_packets_total: 0,
            tx_packets_total: 0,
            rx_bytes_total: 0,
            tx_bytes_total: 0,
            rx_packets_udp: 0,
            rx_packets_tcp: 0,
            rx_packets_icmp: 0,
            tx_packets_udp: 0,
            tx_packets_tcp: 0,
            tx_packets_icmp: 0,
            rx_history: VecDeque::with_capacity(MAX_HISTORY),
            tx_history: VecDeque::with_capacity(MAX_HISTORY),
            ncp_state_history: VecDeque::with_capacity(MAX_HISTORY),
        }
    }

    /// Record an inbound (NCP \u2192 host) IPv6 packet.
    pub fn record_inbound_packet(&mut self, packet: &[u8]) {
        if packet.len() < 40 {
            return;
        }
        let (src, dst, payload_len, proto) = classify_packet(packet);

        self.rx_packets_total = self.rx_packets_total.saturating_add(1);
        self.rx_bytes_total = self.rx_bytes_total.saturating_add(payload_len as u64);

        match proto {
            Protocol::Udp => self.rx_packets_udp = self.rx_packets_udp.saturating_add(1),
            Protocol::Tcp => self.rx_packets_tcp = self.rx_packets_tcp.saturating_add(1),
            Protocol::Icmp => self.rx_packets_icmp = self.rx_packets_icmp.saturating_add(1),
            Protocol::Other => {}
        }

        let info = PacketInfo {
            timestamp: Instant::now(),
            payload_len,
            src,
            dst,
        };
        if self.rx_history.len() >= MAX_HISTORY {
            self.rx_history.pop_front();
        }
        self.rx_history.push_back(info);
    }

    /// Record an outbound (host \u2192 NCP) IPv6 packet.
    pub fn record_outbound_packet(&mut self, packet: &[u8]) {
        if packet.len() < 40 {
            return;
        }
        let (src, dst, payload_len, proto) = classify_packet(packet);

        self.tx_packets_total = self.tx_packets_total.saturating_add(1);
        self.tx_bytes_total = self.tx_bytes_total.saturating_add(payload_len as u64);

        match proto {
            Protocol::Udp => self.tx_packets_udp = self.tx_packets_udp.saturating_add(1),
            Protocol::Tcp => self.tx_packets_tcp = self.tx_packets_tcp.saturating_add(1),
            Protocol::Icmp => self.tx_packets_icmp = self.tx_packets_icmp.saturating_add(1),
            Protocol::Other => {}
        }

        let info = PacketInfo {
            timestamp: Instant::now(),
            payload_len,
            src,
            dst,
        };
        if self.tx_history.len() >= MAX_HISTORY {
            self.tx_history.pop_front();
        }
        self.tx_history.push_back(info);
    }

    /// Record an NCP state transition.
    pub fn record_ncp_state_change(&mut self, state: NcpState) {
        let info = NcpStateInfo {
            timestamp: Instant::now(),
            state,
        };
        if self.ncp_state_history.len() >= MAX_HISTORY {
            self.ncp_state_history.pop_front();
        }
        self.ncp_state_history.push_back(info);
    }

    /// Format `Stat:RX` — RX summary line.
    pub fn stat_rx(&self) -> String {
        format!(
            "RX: {} total, {} bytes, {} UDP, {} TCP, {} ICMP",
            self.rx_packets_total,
            self.rx_bytes_total,
            self.rx_packets_udp,
            self.rx_packets_tcp,
            self.rx_packets_icmp,
        )
    }

    /// Format `Stat:TX` — TX summary line.
    pub fn stat_tx(&self) -> String {
        format!(
            "TX: {} total, {} bytes, {} UDP, {} TCP, {} ICMP",
            self.tx_packets_total,
            self.tx_bytes_total,
            self.tx_packets_udp,
            self.tx_packets_tcp,
            self.tx_packets_icmp,
        )
    }

    /// Format `Stat:NCP` — full NCP state history.
    pub fn stat_ncp(&self) -> String {
        if self.ncp_state_history.is_empty() {
            return String::new();
        }
        let mut lines = Vec::with_capacity(self.ncp_state_history.len());
        for entry in &self.ncp_state_history {
            let elapsed = entry.timestamp.duration_since(self.start_time);
            lines.push(format!("{:.3} -> {}", elapsed.as_secs_f64(), entry.state));
        }
        lines.join("\n")
    }

    /// Format `Stat:Short` — RX + TX summary + last 10 NCP state entries.
    pub fn stat_short(&self) -> String {
        let mut out = String::with_capacity(256);
        out.push_str(&self.stat_rx());
        out.push('\n');
        out.push_str(&self.stat_tx());

        let ncp_entries: Vec<_> = self.ncp_state_history.iter().rev().take(10).collect();
        if !ncp_entries.is_empty() {
            out.push('\n');
            for entry in ncp_entries.iter().rev() {
                let elapsed = entry.timestamp.duration_since(self.start_time);
                out.push_str(&format!(
                    "{:.3} -> {}\n",
                    elapsed.as_secs_f64(),
                    entry.state
                ));
            }
            // Remove trailing newline
            out.pop();
        }
        out
    }

    /// Format `Stat:Long` — full RX + TX history + full NCP state history.
    pub fn stat_long(&self) -> String {
        let mut out = String::with_capacity(4096);
        out.push_str(&self.stat_rx());
        out.push('\n');
        out.push_str(&self.stat_tx());
        out.push('\n');

        for entry in &self.rx_history {
            let elapsed = entry.timestamp.duration_since(self.start_time);
            out.push_str(&format!(
                "RX {:.3} {} -> {}\n",
                elapsed.as_secs_f64(),
                entry.src,
                entry.dst,
            ));
        }
        for entry in &self.tx_history {
            let elapsed = entry.timestamp.duration_since(self.start_time);
            out.push_str(&format!(
                "TX {:.3} {} -> {}\n",
                elapsed.as_secs_f64(),
                entry.src,
                entry.dst,
            ));
        }

        if !self.ncp_state_history.is_empty() {
            out.push_str("NCP:\n");
            for entry in &self.ncp_state_history {
                let elapsed = entry.timestamp.duration_since(self.start_time);
                out.push_str(&format!(
                    "{:.3} -> {}\n",
                    elapsed.as_secs_f64(),
                    entry.state
                ));
            }
        }

        // Remove trailing newline
        out.pop();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ipv6_packet(src: Ipv6Addr, dst: Ipv6Addr, next_header: u8, payload_len: u16) -> Vec<u8> {
        let mut pkt = vec![0u8; 40 + payload_len as usize];
        // Version 6 + traffic class + flow label
        pkt[0] = 0x60;
        // Payload length (big-endian)
        pkt[4] = (payload_len >> 8) as u8;
        pkt[5] = payload_len as u8;
        pkt[6] = next_header;
        pkt[7] = 64; // hop limit
        pkt[8..24].copy_from_slice(&src.octets());
        pkt[24..40].copy_from_slice(&dst.octets());
        pkt
    }

    #[test]
    fn record_inbound_counts_udp() {
        let mut sc = StatCollector::new();
        let pkt = ipv6_packet(Ipv6Addr::LOCALHOST, Ipv6Addr::LOCALHOST, 17, 100);
        sc.record_inbound_packet(&pkt);
        assert_eq!(sc.rx_packets_total, 1);
        assert_eq!(sc.rx_bytes_total, 100);
        assert_eq!(sc.rx_packets_udp, 1);
        assert_eq!(sc.rx_packets_tcp, 0);
        assert_eq!(sc.rx_packets_icmp, 0);
    }

    #[test]
    fn record_outbound_counts_tcp() {
        let mut sc = StatCollector::new();
        let pkt = ipv6_packet(Ipv6Addr::LOCALHOST, Ipv6Addr::LOCALHOST, 6, 200);
        sc.record_outbound_packet(&pkt);
        assert_eq!(sc.tx_packets_total, 1);
        assert_eq!(sc.tx_bytes_total, 200);
        assert_eq!(sc.tx_packets_tcp, 1);
        assert_eq!(sc.tx_packets_udp, 0);
    }

    #[test]
    fn record_inbound_short_packet_ignored() {
        let mut sc = StatCollector::new();
        sc.record_inbound_packet(&[0u8; 10]);
        assert_eq!(sc.rx_packets_total, 0);
        assert!(sc.rx_history.is_empty());
    }

    #[test]
    fn history_bounded_at_64() {
        let mut sc = StatCollector::new();
        let pkt = ipv6_packet(Ipv6Addr::LOCALHOST, Ipv6Addr::LOCALHOST, 17, 50);
        for _ in 0..70 {
            sc.record_inbound_packet(&pkt);
        }
        assert_eq!(sc.rx_history.len(), 64);
        assert_eq!(sc.rx_packets_total, 70);
    }

    #[test]
    fn ncp_state_history_bounded() {
        let mut sc = StatCollector::new();
        for _ in 0..70 {
            sc.record_ncp_state_change(NcpState::Associated);
        }
        assert_eq!(sc.ncp_state_history.len(), 64);
    }

    #[test]
    fn stat_rx_format() {
        let mut sc = StatCollector::new();
        let pkt = ipv6_packet(Ipv6Addr::LOCALHOST, Ipv6Addr::LOCALHOST, 17, 100);
        sc.record_inbound_packet(&pkt);
        let s = sc.stat_rx();
        assert!(s.starts_with("RX: 1 total, 100 bytes, 1 UDP, 0 TCP, 0 ICMP"));
    }

    #[test]
    fn stat_tx_format() {
        let mut sc = StatCollector::new();
        let pkt = ipv6_packet(Ipv6Addr::LOCALHOST, Ipv6Addr::LOCALHOST, 6, 200);
        sc.record_outbound_packet(&pkt);
        let s = sc.stat_tx();
        assert!(s.starts_with("TX: 1 total, 200 bytes, 0 UDP, 1 TCP, 0 ICMP"));
    }

    #[test]
    fn stat_ncp_empty() {
        let sc = StatCollector::new();
        assert!(sc.stat_ncp().is_empty());
    }

    #[test]
    fn stat_ncp_with_entries() {
        let mut sc = StatCollector::new();
        sc.record_ncp_state_change(NcpState::Offline);
        sc.record_ncp_state_change(NcpState::Associated);
        let s = sc.stat_ncp();
        assert!(s.contains("-> offline"));
        assert!(s.contains("-> associated"));
    }

    #[test]
    fn stat_short_includes_last_10_ncp() {
        let mut sc = StatCollector::new();
        for _ in 0..15 {
            sc.record_ncp_state_change(NcpState::Associated);
        }
        let s = sc.stat_short();
        // Should only have last 10 entries
        assert_eq!(s.matches("-> associated").count(), 10);
    }

    #[test]
    fn stat_long_includes_all_histories() {
        let mut sc = StatCollector::new();
        let pkt = ipv6_packet(Ipv6Addr::LOCALHOST, Ipv6Addr::LOCALHOST, 17, 50);
        sc.record_inbound_packet(&pkt);
        sc.record_outbound_packet(&pkt);
        sc.record_ncp_state_change(NcpState::Offline);
        let s = sc.stat_long();
        assert!(s.contains("RX:"));
        assert!(s.contains("TX:"));
        assert!(s.contains("RX 0"));
        assert!(s.contains("TX 0"));
        assert!(s.contains("NCP:"));
        assert!(s.contains("-> offline"));
    }
}
