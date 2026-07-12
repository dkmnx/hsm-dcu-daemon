//! Mock network topology — simulated nodes and scan beacons.
//!
//! The mock uses [`MockTopology`] to produce scan-beacon and topology-entry
//! responses. Beacons are converted to the Spinel pack format
//! (`"Cct(ESSC)t(iCUd)"` per `SpinelNCPTaskScan.cpp:221-237`) before
//! being sent over the wire.

use std::net::Ipv6Addr;
use std::sync::atomic::{AtomicU64, Ordering};

use spinel::frame::SpinelFrame;
use spinel::pack::PackWriter;
use spinel::property::PROP_MAC_SCAN_BEACON;
use spinel::property::prop_value_is;
use wisun_types::Eui64;

/// A simulated node in the mock network.
#[derive(Debug, Clone)]
pub struct MockNode {
    pub eui64: Eui64,
    pub channel: u8,
    pub ipv6_address: Ipv6Addr,
    pub rssi: i8,
    pub hop_count: u8,
    pub is_router: bool,
}

/// Beacon record for scan responses. Converted to the Spinel wire format
/// `"Cct(ESSC)t(iCUd)"` before emission.
#[derive(Debug, Clone)]
pub struct MockBeacon {
    pub channel: u8,
    pub rssi: i8,
    pub laddr: Eui64,
    pub saddr: u16,
    pub pan_id: u16,
    pub lqi: u8,
    pub protocol: u8,
    pub flags: u8,
    pub network_name: String,
    pub xpan_id: Vec<u8>,
}

impl MockBeacon {
    /// Encode this beacon as a `PROP_VALUE_IS(MAC_SCAN_BEACON, ...)` frame
    /// using the Spinel pack format `"Cct(ESSC)t(iCUd)"`.
    pub fn to_spinel_frame(&self) -> SpinelFrame {
        let mut w = PackWriter::new();
        w.write_uint8(self.channel);
        w.write_int8(self.rssi);

        // Struct t(ESSC): eui64, uint16, uint16, uint8.
        let start1 = w.write_struct_start();
        w.write_eui64(&self.laddr.0);
        w.write_uint16(self.saddr);
        w.write_uint16(self.pan_id);
        w.write_uint8(self.lqi);
        w.write_struct_end(start1);

        // Struct t(iCUd): packed, uint8, utf8, data-with-len.
        let start2 = w.write_struct_start();
        w.write_uint_packed(self.protocol as u32);
        w.write_uint8(self.flags);
        w.write_utf8(&self.network_name);
        w.write_data_with_len(&self.xpan_id);
        w.write_struct_end(start2);

        let payload = w.into_bytes();
        prop_value_is(PROP_MAC_SCAN_BEACON, payload)
    }
}

/// Monotonic counter for `simulate_node_join` / `with_nodes`.
/// Ensures unique EUI-64 bytes even after `remove_node` shrinks the list.
static NEXT_EUI_BYTE: AtomicU64 = AtomicU64::new(1);

/// Return a clamped RSSI `i8` that never overflows, computed from a
/// zero-based index.
fn clamped_rssi(i: usize) -> i8 {
    let raw = -(20i32 + i as i32);
    raw.clamp(-128, -1) as i8
}

/// Simulated network topology for the mock NCP.
#[derive(Debug, Clone)]
pub struct MockTopology {
    nodes: Vec<MockNode>,
}

impl MockTopology {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    pub fn add_node(&mut self, node: MockNode) {
        self.nodes.push(node);
    }

    pub fn remove_node(&mut self, eui64: &Eui64) {
        self.nodes.retain(|n| n.eui64 != *eui64);
    }

    /// Create a topology with `n` default nodes spread across channels.
    pub fn with_nodes(n: usize) -> Self {
        let mut topo = Self::new();
        for i in 0..n {
            let id = NEXT_EUI_BYTE.fetch_add(1, Ordering::Relaxed);
            let mut eui = [0u8; 8];
            eui[7] = id as u8;
            let addr = Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0, id as u16);
            topo.add_node(MockNode {
                eui64: Eui64(eui),
                channel: (11u8 + (i as u8) % 26).min(128),
                ipv6_address: addr,
                rssi: clamped_rssi(i),
                hop_count: 1,
                is_router: true,
            });
        }
        topo
    }

    pub fn nodes(&self) -> &[MockNode] {
        &self.nodes
    }

    /// Produce beacon records for a scan response. `channel_mask` is the list
    /// of channel indices the daemon requested via `MAC_SCAN_MASK`.
    pub fn get_scan_beacons(&self, channel_mask: &[u8]) -> Vec<MockBeacon> {
        let channels: std::collections::HashSet<u8> = channel_mask.iter().copied().collect();
        self.nodes
            .iter()
            .filter(|n| channels.contains(&n.channel))
            .enumerate()
            .map(|(i, n)| {
                let mut xpan = vec![0u8; 8];
                xpan[0] = 0xAB;
                xpan[1] = 0xCD;
                xpan[2] = 0xEF;
                xpan[3] = 0x00;
                xpan[4] = 0x00;
                xpan[5] = (i + 1) as u8;
                xpan[6] = 0x00;
                xpan[7] = 0x00;
                MockBeacon {
                    channel: n.channel,
                    rssi: n.rssi,
                    laddr: n.eui64,
                    saddr: 0x8000 | (i as u16),
                    pan_id: 0xABCD,
                    lqi: 200u8.saturating_sub(i as u8),
                    protocol: 0,
                    flags: 1,
                    network_name: "MockNet".into(),
                    xpan_id: xpan,
                }
            })
            .collect()
    }

    /// Simulate a new node joining the network using a monotonic ID counter
    /// so EUIs are unique even after `remove_node`.
    pub fn simulate_node_join(&mut self) -> MockNode {
        let id = NEXT_EUI_BYTE.fetch_add(1, Ordering::Relaxed);
        let mut eui = [0u8; 8];
        eui[7] = id as u8;
        let addr = Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0, id as u16);
        let idx = self.nodes.len();
        let node = MockNode {
            eui64: Eui64(eui),
            channel: (11u8 + (idx as u8) % 26).min(128),
            ipv6_address: addr,
            rssi: clamped_rssi(idx),
            hop_count: 1,
            is_router: true,
        };
        self.add_node(node.clone());
        node
    }
}

impl Default for MockTopology {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beacon_encodes_without_panic() {
        let xpan = vec![0xAB, 0xCD, 0xEF, 0x01, 0x02, 0x03, 0x04, 0x05];
        let beacon = MockBeacon {
            channel: 11,
            rssi: -45,
            laddr: Eui64([0; 8]),
            saddr: 0x8001,
            pan_id: 0xABCD,
            lqi: 200,
            protocol: 0,
            flags: 1,
            network_name: "TestNet".into(),
            xpan_id: xpan.clone(),
        };
        let frame = beacon.to_spinel_frame();
        assert_eq!(frame.command_id, spinel::command::CMD_PROP_VALUE_IS);

        // Re-decode the beacon payload from the frame.
        let mut r = spinel::pack::PackReader::new(&frame.payload);
        let prop = r.read_uint_packed().unwrap();
        assert_eq!(prop, PROP_MAC_SCAN_BEACON);
        assert_eq!(r.read_uint8().unwrap(), 11);
        assert_eq!(r.read_int8().unwrap(), -45);
        // Skip the t(ESSC) and t(iCUd) struct contents — just verify they
        // round-trip through encode/decode.
        let struct1 = r.read_struct().unwrap();
        assert!(!struct1.is_empty());
        let struct2 = r.read_struct().unwrap();
        assert!(!struct2.is_empty());
    }

    #[test]
    fn topology_with_nodes() {
        let topo = MockTopology::with_nodes(5);
        assert_eq!(topo.nodes().len(), 5);
    }

    #[test]
    fn scan_beacons_from_topology() {
        let topo = MockTopology::with_nodes(3);
        let mask = vec![11u8, 12, 13];
        let beacons = topo.get_scan_beacons(&mask);
        // All 3 nodes are on channels 11-13 (with_nodes uses 11+i), so all match.
        assert_eq!(beacons.len(), 3);
        assert_eq!(beacons[0].channel, 11);
    }

    #[test]
    fn simulate_node_join() {
        let mut topo = MockTopology::new();
        let node = topo.simulate_node_join();
        assert_eq!(topo.nodes().len(), 1);
        assert_eq!(node.eui64, topo.nodes()[0].eui64);
    }
}
