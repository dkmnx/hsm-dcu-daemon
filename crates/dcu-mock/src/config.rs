//! Mock NCP configuration — NCP version, MAC address, capability set.
//!
//! Mirrors the C `SpinelNCPInstance` configuration that the daemon reads
//! from `PROP_CAPS`/`PROP_NCP_VERSION`/`PROP_HWADDR`.

use std::collections::BTreeSet;

use wisun_types::Eui64;

/// Mock NCP configuration. Kept separate from `MockNcp` so the builder can
/// own it before constructing the mock.
#[derive(Debug, Clone)]
pub struct MockConfig {
    pub ncp_version: String,
    pub hardware_address: Eui64,
}

impl Default for MockConfig {
    fn default() -> Self {
        Self {
            ncp_version: "TIWISUNFAN 1.0".into(),
            hardware_address: Eui64([0; 8]),
        }
    }
}

/// Capability bit set returned by `PROP_CAPS`. Mirrors the C
/// `std::set<unsigned int> mCapabilities` on `SpinelNCPInstance`.
#[derive(Debug, Clone)]
pub struct CapabilitySet {
    pub bits: BTreeSet<u32>,
}

impl CapabilitySet {
    pub fn empty() -> Self {
        Self { bits: BTreeSet::new() }
    }

    /// Standard router-capable NCP.
    pub fn router() -> Self {
        let mut c = Self::empty();
        c.add(spinel::property::CAP_ROLE_ROUTER);
        c.add(spinel::property::CAP_CONFIG_FTD);
        c
    }

    /// Supports MCU power-state (for deep sleep / wake).
    pub fn with_mcu_power(mut self) -> Self {
        self.add(spinel::property::CAP_MCU_POWER_STATE);
        self
    }

    pub fn add(&mut self, cap: u32) {
        self.bits.insert(cap);
    }

    pub fn contains(&self, cap: u32) -> bool {
        self.bits.contains(&cap)
    }
}
