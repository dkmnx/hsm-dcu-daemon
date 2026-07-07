//! Property key constants for wpantund NCP properties.
//!
//! Two groups of constants:
//! 1. **String constants** — the D-Bus property key strings from `wpan-properties.h`.
//! 2. **Spinel numeric property IDs** — packed integer identifiers used in the Spinel protocol.
//!
//! The TI Wi-SUN vendor range in Spinel is `0x3C00..0x4000` (see `spinel.h:4420`).

// ---------------------------------------------------------------------------
// String property key constants (kWPANTUNDProperty_* from wpan-properties.h)
// ---------------------------------------------------------------------------
//
// Each key is declared as a `pub const` AND added to `ALL_PROPERTY_KEYS` so
// the list stays in sync with the declarations.

macro_rules! declare_property_keys {
    ($(($name:ident, $value:expr)),* $(,)?) => {
        $(
            pub const $name: &str = $value;
        )*
        /// All known property key strings, for validation and lookup.
        pub static ALL_PROPERTY_KEYS: &[&str] = &[$($name),*];
    };
}

declare_property_keys![
    // --- Core ---
    (PROP_NCP_PROTOCOL_VERSION, "NCP:ProtocolVersion"),
    (PROP_NCP_VERSION, "NCP:Version"),
    (PROP_NCP_INTERFACE_TYPE, "NCP:InterfaceType"),
    (PROP_NCP_HARDWARE_ADDRESS, "NCP:HardwareAddress"),
    // --- PHY ---
    (PROP_NCP_CCA_THRESHOLD, "NCP:CCAThreshold"),
    (PROP_NCP_TX_POWER, "NCP:TXPower"),
    // --- TI Wi-SUN PHY ---
    (PROP_NCP_PHY_REGION, "NCP:Region"),
    (PROP_NCP_MODE_ID, "NCP:ModeID"),
    (PROP_UNICAST_CH_LIST, "UnicastChList"),
    (PROP_BROADCAST_CH_LIST, "BroadcastChList"),
    (PROP_ASYNC_CH_LIST, "AsyncChList"),
    (PROP_REGULATION_CH_LIST, "RegulationChList"),
    (PROP_CH_SPACING, "ChSpacing"),
    (PROP_CH0_CENTER_FREQ, "Ch0CenterFreq"),
    (PROP_OPERATING_CLASS, "OperatingClass"),
    (PROP_NUM_CHANNELS, "NumChannels"),
    // --- MAC ---
    (PROP_NETWORK_PAN_ID, "Network:PANID"),
    // --- TI Wi-SUN MAC ---
    (PROP_UC_DWELL_INTERVAL, "UCDwellInterval"),
    (PROP_BC_DWELL_INTERVAL, "BCDwellInterval"),
    (PROP_BC_INTERVAL, "BCInterval"),
    (PROP_UC_CH_FUNCTION, "UCChFunction"),
    (PROP_BC_CH_FUNCTION, "BCChFunction"),
    (PROP_MAC_FILTER_LIST, "MacFilterList"),
    (PROP_MAC_FILTER_MODE, "MacFilterMode"),
    (
        PROP_EXTERNAL_DHCP_SERVER_ENABLED,
        "ExternalDHCPServerEnabled"
    ),
    (PROP_EXTERNAL_DHCP_SERVER_ADDR, "ExternalDHCPServerAddr"),
    (
        PROP_EXTERNAL_AUTH_SERVER_ENABLED,
        "ExternalAuthServerEnabled"
    ),
    (PROP_EXTERNAL_AUTH_SERVER_ADDR, "ExternalAuthServerAddr"),
    // --- NET ---
    (PROP_INTERFACE_UP, "Interface:Up"),
    (PROP_STACK_UP, "Stack:Up"),
    (PROP_NETWORK_ROLE, "Network:Role"),
    (PROP_NETWORK_NAME, "Network:Name"),
    // --- TI Wi-SUN NET ---
    (PROP_DODAG_ROUTE_DEST, "DodagRouteDest"),
    (PROP_DODAG_ROUTE, "DodagRoute"),
    (PROP_NUM_CONNECTED_DEVICES, "NumConnected"),
    (PROP_CONNECTED_DEVICES, "ConnectedDevices"),
    (PROP_IPV6_ALL_ADDRESSES, "IPv6:AllAddresses"),
    // --- NCP ---
    (PROP_NCP_STATE, "NCP:State"),
    (PROP_NCP_EXTENDED_ADDRESS, "NCP:ExtendedAddress"),
    (PROP_NCP_MAC_ADDRESS, "NCP:MACAddress"),
    (PROP_NCP_CHANNEL, "NCP:Channel"),
    (PROP_NCP_FREQUENCY, "NCP:Frequency"),
    (PROP_NCP_TX_POWER_LIMIT, "NCP:TXPowerLimit"),
    (PROP_NCP_CHANNEL_MASK, "NCP:ChannelMask"),
    (PROP_NCP_PREFERRED_CHANNEL_MASK, "NCP:PreferredChannelMask"),
    (PROP_NCP_SLEEPY_POLL_INTERVAL, "NCP:SleepyPollInterval"),
    (PROP_NCP_RSSI, "NCP:RSSI"),
    (PROP_NCP_CCA_FAILURE_RATE, "NCP:CCAFailureRate"),
    (PROP_NCP_MCU_POWER_STATE, "NCP:MCUPowerState"),
    (PROP_NCP_CAPABILITIES, "NCP:Capabilities"),
    // --- Network ---
    (PROP_NETWORK_XPANID, "Network:XPANID"),
    (PROP_NETWORK_NODE_TYPE, "Network:NodeType"),
    (PROP_NETWORK_KEY, "Network:Key"),
    (PROP_NETWORK_KEY_INDEX, "Network:KeyIndex"),
    (
        PROP_NETWORK_KEY_SWITCH_GUARD_TIME,
        "Network:KeySwitchGuardTime"
    ),
    (PROP_NETWORK_IS_COMMISSIONED, "Network:IsCommissioned"),
    (PROP_NETWORK_IS_CONNECTED, "Network:IsConnected"),
    (PROP_NETWORK_PSKC, "Network:PSKc"),
    (PROP_NETWORK_PARTITION_ID, "Network:PartitionId"),
    // --- IPv6 ---
    (
        PROP_IPV6_WFANTUND_GLOBAL_ADDRESS,
        "IPv6:WfantundGlobalAddress"
    ),
    (PROP_IPV6_LINK_LOCAL_ADDRESS, "IPv6:LinkLocalAddress"),
    (PROP_IPV6_MESH_LOCAL_ADDRESS, "IPv6:MeshLocalAddress"),
    (PROP_IPV6_MESH_LOCAL_PREFIX, "IPv6:MeshLocalPrefix"),
    (PROP_IPV6_MULTICAST_ADDRESSES, "IPv6:MulticastAddresses"),
    (PROP_IPV6_INTERFACE_ROUTES, "IPv6:Routes"),
    (
        PROP_IPV6_SET_SLAAC_FOR_AUTO_ADDED_PREFIX,
        "IPv6:SetSLAACForAutoAddedPrefix"
    ),
    // --- Config ---
    (PROP_CONFIG_NCP_SOCKET_PATH, "Config:NCP:SocketPath"),
    (PROP_CONFIG_NCP_SOCKET_BAUD, "Config:NCP:SocketBaud"),
    (PROP_CONFIG_NCP_DRIVER_NAME, "Config:NCP:DriverName"),
    (PROP_CONFIG_NCP_HARD_RESET_PATH, "Config:NCP:HardResetPath"),
    (PROP_CONFIG_NCP_POWER_PATH, "Config:NCP:PowerPath"),
    (
        PROP_CONFIG_NCP_RELIABILITY_LAYER,
        "Config:NCP:ReliabilityLayer"
    ),
    (
        PROP_CONFIG_NCP_FIRMWARE_CHECK_COMMAND,
        "Config:NCP:FirmwareCheckCommand"
    ),
    (
        PROP_CONFIG_NCP_FIRMWARE_UPGRADE_COMMAND,
        "Config:NCP:FirmwareUpgradeCommand"
    ),
    (PROP_CONFIG_TUN_INTERFACE_NAME, "Config:TUN:InterfaceName"),
    (PROP_CONFIG_DAEMON_PID_FILE, "Config:Daemon:PIDFile"),
    (
        PROP_CONFIG_DAEMON_PRIV_DROP_TO_USER,
        "Config:Daemon:PrivDropToUser"
    ),
    (PROP_CONFIG_DAEMON_CHROOT, "Config:Daemon:Chroot"),
    (
        PROP_CONFIG_DAEMON_NETWORK_RETAIN_COMMAND,
        "Config:Daemon:NetworkRetainCommand"
    ),
    // --- Daemon ---
    (PROP_DAEMON_VERSION, "Daemon:Version"),
    (PROP_DAEMON_ENABLED, "Daemon:Enabled"),
    (PROP_DAEMON_SYSLOG_MASK, "Daemon:SyslogMask"),
    (PROP_DAEMON_TERMINATE_ON_FAULT, "Daemon:TerminateOnFault"),
    (PROP_DAEMON_READY_FOR_HOST_SLEEP, "Daemon:ReadyForHostSleep"),
    (
        PROP_DAEMON_AUTO_ASSOCIATE_AFTER_RESET,
        "Daemon:AutoAssociateAfterReset"
    ),
    (
        PROP_DAEMON_AUTO_FIRMWARE_UPDATE,
        "Daemon:AutoFirmwareUpdate"
    ),
    (PROP_DAEMON_AUTO_DEEP_SLEEP, "Daemon:AutoDeepSleep"),
    (PROP_DAEMON_FAULT_REASON, "Daemon:FaultReason"),
    (
        PROP_DAEMON_TICKLE_ON_HOST_DID_WAKE,
        "Daemon:TickleOnHostDidWake"
    ),
    // --- Thread (included for completeness, not all are Wi-SUN relevant) ---
    (PROP_THREAD_RLOC16, "Thread:RLOC16"),
    (PROP_THREAD_ROUTER_ID, "Thread:RouterID"),
    (PROP_THREAD_CHILD_TABLE, "Thread:ChildTable"),
    (PROP_THREAD_NEIGHBOR_TABLE, "Thread:NeighborTable"),
    (PROP_THREAD_NETWORK_DATA, "Thread:NetworkData"),
    (PROP_THREAD_ACTIVE_DATASET, "Thread:ActiveDataset"),
    (PROP_THREAD_PENDING_DATASET, "Thread:PendingDataset"),
    // --- Statistics ---
    (PROP_STAT_RX, "Stat:RX"),
    (PROP_STAT_TX, "Stat:TX"),
    (PROP_STAT_NCP, "Stat:NCP"),
    (PROP_STAT_SHORT, "Stat:Short"),
    (PROP_STAT_LONG, "Stat:Long"),
];

/// Returns `true` if `name` is a known property key (exact match).
///
/// Property keys are case-sensitive — they match the D-Bus wire format
/// defined in `wpan-properties.h`.
pub fn property_key_exists(name: &str) -> bool {
    ALL_PROPERTY_KEYS.contains(&name)
}

// ---------------------------------------------------------------------------
// Spinel numeric property IDs  (from third_party/openthread/src/ncp/spinel.h)
// ---------------------------------------------------------------------------

pub const SPINEL_PROP_LAST_STATUS: u32 = 0;
pub const SPINEL_PROP_PROTOCOL_VERSION: u32 = 1;
pub const SPINEL_PROP_NCP_VERSION: u32 = 2;
pub const SPINEL_PROP_INTERFACE_TYPE: u32 = 3;
pub const SPINEL_PROP_VENDOR_ID: u32 = 4;
pub const SPINEL_PROP_CAPS: u32 = 5;
pub const SPINEL_PROP_INTERFACE_COUNT: u32 = 6;
pub const SPINEL_PROP_POWER_STATE: u32 = 7;
pub const SPINEL_PROP_HWADDR: u32 = 8;
pub const SPINEL_PROP_LOCK: u32 = 9;
pub const SPINEL_PROP_HOST_POWER_STATE: u32 = 12;
pub const SPINEL_PROP_MCU_POWER_STATE: u32 = 13;

// PHY properties
pub const SPINEL_PROP_PHY_ENABLED: u32 = 0x20;
pub const SPINEL_PROP_PHY_CHAN: u32 = 0x21;
pub const SPINEL_PROP_PHY_CHAN_SUPPORTED: u32 = 0x22;
pub const SPINEL_PROP_PHY_FREQ: u32 = 0x23;
pub const SPINEL_PROP_PHY_CCA_THRESHOLD: u32 = 0x24;
pub const SPINEL_PROP_PHY_TX_POWER: u32 = 0x25;
pub const SPINEL_PROP_PHY_RSSI: u32 = 0x26;
pub const SPINEL_PROP_PHY_RX_SENSITIVITY: u32 = 0x27;

// MAC properties
pub const SPINEL_PROP_MAC_SCAN_STATE: u32 = 0x30;
pub const SPINEL_PROP_MAC_SCAN_MASK: u32 = 0x31;
pub const SPINEL_PROP_MAC_SCAN_PERIOD: u32 = 0x32;
pub const SPINEL_PROP_MAC_15_4_LADDR: u32 = 0x34;
pub const SPINEL_PROP_MAC_15_4_SADDR: u32 = 0x35;
pub const SPINEL_PROP_MAC_15_4_PANID: u32 = 0x36;
pub const SPINEL_PROP_MAC_PROMISCUOUS_MODE: u32 = 0x38;
pub const SPINEL_PROP_MAC_DATA_POLL_PERIOD: u32 = 0x3A;

// MAC extended properties
pub const SPINEL_PROP_MAC_ALLOWLIST: u32 = 0x1300;
pub const SPINEL_PROP_MAC_ALLOWLIST_ENABLED: u32 = 0x1301;
pub const SPINEL_PROP_MAC_EXTENDED_ADDR: u32 = 0x1302;
pub const SPINEL_PROP_MAC_DENYLIST: u32 = 0x1306;
pub const SPINEL_PROP_MAC_DENYLIST_ENABLED: u32 = 0x1307;
pub const SPINEL_PROP_MAC_FIXED_RSS: u32 = 0x1308;

// Vendor range (TI Wi-SUN vendor-specific properties use 0x3C00+)
pub const SPINEL_PROP_VENDOR__BEGIN: u32 = 0x3C00;
pub const SPINEL_PROP_VENDOR__END: u32 = 0x4000;

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify all TI Wi-SUN properties referenced in `ti_wisun_commands.md`
    /// have a corresponding constant (using the canonical case-sensitive form).
    #[test]
    fn all_ti_properties_defined() {
        let required = vec![
            "NCP:ProtocolVersion",
            "NCP:Version",
            "NCP:InterfaceType",
            "NCP:HardwareAddress",
            "NCP:CCAThreshold",
            "NCP:Region",
            "NCP:ModeID",
            "UnicastChList",
            "BroadcastChList",
            "AsyncChList",
            "ChSpacing",
            "Ch0CenterFreq",
            "Network:PANID",
            "BCDwellInterval",
            "UCDwellInterval",
            "BCInterval",
            "UCChFunction",
            "BCChFunction",
            "MacFilterList",
            "MacFilterMode",
            "Interface:Up",
            "Stack:Up",
            "Network:NodeType",
            "Network:Name",
            "DodagRouteDest",
            "DodagRoute",
            "NumConnected",
            "ConnectedDevices",
            "IPv6:AllAddresses",
            "ExternalDHCPServerEnabled",
            "ExternalDHCPServerAddr",
            "ExternalAuthServerEnabled",
            "ExternalAuthServerAddr",
            "RegulationChList",
            "OperatingClass",
            "NumChannels",
        ];
        for name in required {
            assert!(property_key_exists(name), "Missing property key: {name}");
        }
    }

    #[test]
    fn known_property_keys() {
        assert!(property_key_exists("NCP:State"));
        assert!(property_key_exists("Network:XPANID"));
        assert!(property_key_exists("Daemon:Version"));
        assert!(!property_key_exists("NonExistent:Property"));
    }

    /// Verify case sensitivity: only the canonical form should match.
    #[test]
    fn property_key_case_sensitive() {
        assert!(property_key_exists("UnicastChList"));
        assert!(!property_key_exists("unicastchlist"));
        assert!(property_key_exists("Network:PANID"));
        assert!(!property_key_exists("Network:panid"));
    }
}
