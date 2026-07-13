//! Address / prefix / route manager (P0-4).
//!
//! Rust-native re-implementation of the observable behavior of C
//! `NCPInstanceBase-Addresses.cpp`. The C daemon keeps typed maps of
//! unicast addresses, multicast addresses, on-mesh prefixes, off-mesh
//! routes, and interface routes, each tagged with an `Origin`
//! (NCP / Interface / User). It diffs full-table `PROP_VALUE_IS` snapshots
//! from the NCP and pushes the resulting address/route deltas to the OS
//! TUN interface.
//!
//! We preserve the *contract*:
//! * NCP-origin entries flow NCP → daemon → OS TUN, never back to the NCP.
//! * User/Interface-origin entries are pushed to the NCP *and* the OS TUN.
//! * `IPv6:AllAddresses` / `IPv6:Routes` / `Thread:OnMeshPrefixes` /
//!   `Thread:OffMeshRoutes` reflect the current view.
//!
//! The manager owns only the typed state and computes deltas. Applying
//! those deltas to the OS TUN (and to the NCP for user-origin changes) is
//! done by the caller (`NcpInstanceBase`), which holds the TUN handle and
//! the Spinel command channel — keeping this module free of I/O.

use std::collections::HashMap;
use std::net::Ipv6Addr;

use dcu_tun::Ipv6Net;

/// Where an address/prefix/route originated, mirroring C `Origin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Reported by the NCP (full-table snapshot).
    Ncp,
    /// Learned from the host OS (netlink on the TUN).
    Interface,
    /// Set by the user via PropInsert/PropRemove.
    User,
}

impl Origin {
    /// User/Interface entries are also pushed to the NCP; NCP entries are not.
    pub fn pushes_to_ncp(self) -> bool {
        self != Origin::Ncp
    }
}

#[derive(Debug, Clone)]
pub struct UnicastEntry {
    pub origin: Origin,
    pub prefix_len: u8,
}

#[derive(Debug, Clone)]
pub struct OnMeshEntry {
    pub origin: Origin,
    /// Whether the prefix is on-mesh (SLAAC/default-route flags omitted for
    /// the Wi-SUN subset; the prefix itself is the contract).
    pub stable: bool,
}

#[derive(Debug, Clone)]
pub struct OffMeshEntry {
    pub origin: Origin,
    /// Linux route metric derived from preference (High=1, Medium=256, Low=512).
    pub metric: u32,
}

/// A TUN-side operation the caller must apply after a manager update.
#[derive(Debug, Clone)]
pub enum TunOp {
    AddAddress(Ipv6Addr, u8),
    RemoveAddress(Ipv6Addr, u8),
    AddRoute(Ipv6Net, u32),
    RemoveRoute(Ipv6Net, u32),
    JoinMulticast(Ipv6Addr),
    LeaveMulticast(Ipv6Addr),
}

/// The address/prefix/route manager.
#[derive(Debug, Default)]
pub struct AddressManager {
    unicast: HashMap<Ipv6Addr, UnicastEntry>,
    multicast: HashMap<Ipv6Addr, Origin>,
    on_mesh: HashMap<Ipv6Net, OnMeshEntry>,
    off_mesh: HashMap<Ipv6Net, OffMeshEntry>,
    /// OS-side interface routes (what the daemon programmed on the TUN).
    interface_routes: HashMap<Ipv6Net, u32>,
}

impl AddressManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply a full NCP `IPV6_ADDRESS_TABLE` snapshot (origin = Ncp).
    ///
    /// Diffs against current state and returns the TUN ops needed to
    /// converge; NCP-origin adds are NOT pushed back to the NCP.
    pub fn apply_ncp_address_table(&mut self, addrs: &[(Ipv6Addr, u8)]) -> Vec<TunOp> {
        let mut ops = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (addr, prefix_len) in addrs {
            seen.insert(*addr);
            match self.unicast.get(addr) {
                Some(e) if e.prefix_len == *prefix_len => continue,
                Some(_) => {
                    ops.push(TunOp::RemoveAddress(*addr, self.unicast[addr].prefix_len));
                    ops.push(TunOp::AddAddress(*addr, *prefix_len));
                    self.unicast.insert(
                        *addr,
                        UnicastEntry {
                            origin: Origin::Ncp,
                            prefix_len: *prefix_len,
                        },
                    );
                }
                None => {
                    ops.push(TunOp::AddAddress(*addr, *prefix_len));
                    self.unicast.insert(
                        *addr,
                        UnicastEntry {
                            origin: Origin::Ncp,
                            prefix_len: *prefix_len,
                        },
                    );
                }
            }
        }
        // Remove addresses no longer reported by the NCP.
        let stale: Vec<_> = self
            .unicast
            .iter()
            .filter(|(a, e)| e.origin == Origin::Ncp && !seen.contains(a))
            .map(|(a, e)| (*a, e.prefix_len))
            .collect();
        for (addr, prefix_len) in stale {
            ops.push(TunOp::RemoveAddress(addr, prefix_len));
            self.unicast.remove(&addr);
        }
        ops
    }

    /// Apply a full NCP `IPV6_MULTICAST_ADDRESS_TABLE` snapshot.
    pub fn apply_ncp_multicast_table(&mut self, addrs: &[Ipv6Addr]) -> Vec<TunOp> {
        let mut ops = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for addr in addrs {
            seen.insert(*addr);
            if !self.multicast.contains_key(addr) {
                ops.push(TunOp::JoinMulticast(*addr));
                self.multicast.insert(*addr, Origin::Ncp);
            }
        }
        let stale: Vec<_> = self
            .multicast
            .iter()
            .filter(|(a, o)| *o == &Origin::Ncp && !seen.contains(a))
            .map(|(a, _)| *a)
            .collect();
        for addr in stale {
            ops.push(TunOp::LeaveMulticast(addr));
            self.multicast.remove(&addr);
        }
        ops
    }

    /// Apply a full NCP `THREAD_ON_MESH_NETS` snapshot.
    pub fn apply_ncp_on_mesh_table(&mut self, nets: &[(Ipv6Net, bool)]) -> Vec<TunOp> {
        let mut ops = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (net, stable) in nets {
            seen.insert(*net);
            let entry = self.on_mesh.get(net);
            if entry.map(|e| e.stable) != Some(*stable) {
                self.on_mesh.insert(
                    *net,
                    OnMeshEntry {
                        origin: Origin::Ncp,
                        stable: *stable,
                    },
                );
                // On-mesh metric is always 256; use Occupied/Vacant
                // pattern consistent with off_mesh (P1 route leak fix).
                // If route exists, no stale-metric case (metric is constant).
                if let std::collections::hash_map::Entry::Vacant(e) =
                    self.interface_routes.entry(*net)
                {
                    e.insert(256);
                    ops.push(TunOp::AddRoute(*net, 256));
                }
            }
        }
        let stale: Vec<_> = self
            .on_mesh
            .iter()
            .filter(|(n, e)| e.origin == Origin::Ncp && !seen.contains(n))
            .map(|(n, _)| *n)
            .collect();
        for net in stale {
            self.on_mesh.remove(&net);
            if let Some(metric) = self.interface_routes.remove(&net) {
                ops.push(TunOp::RemoveRoute(net, metric));
            }
        }
        ops
    }

    /// Apply a full NCP `THREAD_OFF_MESH_ROUTES` snapshot.
    pub fn apply_ncp_off_mesh_table(&mut self, nets: &[(Ipv6Net, u32)]) -> Vec<TunOp> {
        let mut ops = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (net, metric) in nets {
            seen.insert(*net);
            let entry = self.off_mesh.get(net);
            if entry.map(|e| e.metric) != Some(*metric) {
                self.off_mesh.insert(
                    *net,
                    OffMeshEntry {
                        origin: Origin::Ncp,
                        metric: *metric,
                    },
                );
                // If route already exists with a different metric, we must
                // remove the old one first; or_insert_with alone would
                // silently keep the stale metric (P1 route leak fix).
                match self.interface_routes.entry(*net) {
                    std::collections::hash_map::Entry::Occupied(mut e) => {
                        if *e.get() != *metric {
                            let old_metric = *e.get();
                            e.insert(*metric);
                            ops.push(TunOp::RemoveRoute(*net, old_metric));
                            ops.push(TunOp::AddRoute(*net, *metric));
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(e) => {
                        e.insert(*metric);
                        ops.push(TunOp::AddRoute(*net, *metric));
                    }
                }
            }
        }
        let stale: Vec<_> = self
            .off_mesh
            .iter()
            .filter(|(n, e)| e.origin == Origin::Ncp && !seen.contains(n))
            .map(|(n, _)| *n)
            .collect();
        for net in stale {
            self.off_mesh.remove(&net);
            if let Some(metric) = self.interface_routes.remove(&net) {
                ops.push(TunOp::RemoveRoute(net, metric));
            }
        }
        ops
    }

    /// User/Interface insert of an on-mesh prefix. Returns true if the entry
    /// is new/changed (caller should push to NCP). Returns TUN ops to apply.
    pub fn insert_on_mesh(&mut self, net: Ipv6Net, stable: bool) -> (bool, Vec<TunOp>) {
        let is_new = self
            .on_mesh
            .get(&net)
            .map(|e| e.stable)
            .map(|s| s != stable)
            .unwrap_or(true);
        let mut ops = Vec::new();
        if is_new {
            self.on_mesh.insert(
                net,
                OnMeshEntry {
                    origin: Origin::User,
                    stable,
                },
            );
            if let std::collections::hash_map::Entry::Vacant(e) = self.interface_routes.entry(net) {
                e.insert(256);
                ops.push(TunOp::AddRoute(net, 256));
            }
        }
        (is_new, ops)
    }

    /// User/Interface remove of an on-mesh prefix. Returns true if removed
    /// (caller should push removal to NCP). Returns TUN ops to apply.
    pub fn remove_on_mesh(&mut self, net: Ipv6Net) -> (bool, Vec<TunOp>) {
        if let Some(entry) = self.on_mesh.remove(&net) {
            let mut ops = Vec::new();
            if let Some(metric) = self.interface_routes.remove(&net) {
                ops.push(TunOp::RemoveRoute(net, metric));
            }
            (entry.origin != Origin::Ncp, ops)
        } else {
            (false, Vec::new())
        }
    }

    /// User/Interface insert of an off-mesh route. Returns (changed, ops).
    pub fn insert_off_mesh(&mut self, net: Ipv6Net, metric: u32) -> (bool, Vec<TunOp>) {
        let is_new = self
            .off_mesh
            .get(&net)
            .map(|e| e.metric)
            .map(|m| m != metric)
            .unwrap_or(true);
        let mut ops = Vec::new();
        if is_new {
            self.off_mesh.insert(
                net,
                OffMeshEntry {
                    origin: Origin::User,
                    metric,
                },
            );
            // If route already exists with a different metric, remove
            // stale one and add new one (P1 route leak fix).
            match self.interface_routes.entry(net) {
                std::collections::hash_map::Entry::Occupied(mut e) => {
                    if *e.get() != metric {
                        let old = *e.get();
                        e.insert(metric);
                        ops.push(TunOp::RemoveRoute(net, old));
                        ops.push(TunOp::AddRoute(net, metric));
                    }
                }
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(metric);
                    ops.push(TunOp::AddRoute(net, metric));
                }
            }
        }
        (is_new, ops)
    }

    /// User/Interface remove of an off-mesh route. Returns (push_to_ncp, ops).
    pub fn remove_off_mesh(&mut self, net: Ipv6Net) -> (bool, Vec<TunOp>) {
        if let Some(entry) = self.off_mesh.remove(&net) {
            let mut ops = Vec::new();
            if let Some(metric) = self.interface_routes.remove(&net) {
                ops.push(TunOp::RemoveRoute(net, metric));
            }
            (entry.origin != Origin::Ncp, ops)
        } else {
            (false, Vec::new())
        }
    }

    // ----- read views for D-Bus property serialization -----

    /// `IPv6:AllAddresses` — all unicast addresses, formatted like C
    /// `UnicastAddressEntry::get_description`: `<addr>/<prefix_len>`.
    pub fn all_addresses(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .unicast
            .iter()
            .map(|(a, e)| format!("{a}/{}/{}", e.prefix_len, origin_tag(e.origin)))
            .collect();
        v.sort();
        v
    }

    /// `IPv6:Routes` — interface (OS) routes.
    pub fn routes(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .interface_routes
            .iter()
            .map(|(n, m)| format!("{n} metric:{m}"))
            .collect();
        v.sort();
        v
    }

    /// `Thread:OnMeshPrefixes` — on-mesh prefixes.
    pub fn on_mesh_prefixes(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .on_mesh
            .iter()
            .map(|(n, e)| format!("{n} stable:{}/{}", e.stable, origin_tag(e.origin)))
            .collect();
        v.sort();
        v
    }

    /// `Thread:OffMeshRoutes` — off-mesh routes.
    pub fn off_mesh_routes(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .off_mesh
            .iter()
            .map(|(n, e)| format!("{n} metric:{}/{}", e.metric, origin_tag(e.origin)))
            .collect();
        v.sort();
        v
    }
}

fn origin_tag(o: Origin) -> &'static str {
    match o {
        Origin::Ncp => "NCP",
        Origin::Interface => "IFA",
        Origin::User => "USER",
    }
}
