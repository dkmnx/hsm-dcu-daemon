//! Property handler map: D-Bus property key → Spinel property ID.
//!
//! Bridges the D-Bus property namespace (string keys like `"NCP:Channel"`)
//! to Spinel wire property IDs (u32 like `0x21`). Used by
//! `handle_command(SetProperty/GetProperty)` to forward D-Bus operations
//! to the NCP over the serial transport.

use std::collections::HashMap;

/// Access mode for a property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PropAccess {
    /// Read-only from D-Bus (NCP is the source of truth).
    ReadOnly,
    /// Read-write from D-Bus (writes forwarded to NCP).
    ReadWrite,
}

/// A property handler entry: maps a D-Bus key to its Spinel prop ID.
#[derive(Debug, Clone)]
pub(crate) struct PropHandler {
    pub prop_id: u32,
    pub access: PropAccess,
}

/// Build the property handler map.
///
/// Returns a static map of D-Bus property key → Spinel property ID + access
/// mode. Keys not in this map are daemon-local (read from Config or internal
/// state) or unsupported.
pub(crate) fn build_handler_map() -> HashMap<&'static str, PropHandler> {
    let mut m: HashMap<&'static str, PropHandler> = HashMap::new();

    // Helper macro to reduce boilerplate.
    macro_rules! prop {
        ($key:expr, $id:expr, $access:expr) => {
            m.insert(
                $key,
                PropHandler {
                    prop_id: $id,
                    access: $access,
                },
            );
        };
    }

    use PropAccess::{ReadOnly, ReadWrite};

    // --- NCP properties (standard Spinel) ---
    prop!("NCP:ProtocolVersion", 1, ReadOnly);
    prop!("NCP:Version", 2, ReadOnly);
    prop!("NCP:InterfaceType", 3, ReadOnly);
    prop!("NCP:HardwareAddress", 8, ReadOnly);
    prop!("NCP:CCAThreshold", 0x24, ReadWrite);
    prop!("NCP:TXPower", 0x25, ReadWrite);
    prop!("NCP:Channel", 0x21, ReadOnly);
    prop!("NCP:Frequency", 0x23, ReadOnly);
    prop!("NCP:RSSI", 0x26, ReadOnly);
    prop!("NCP:ExtendedAddress", 0x1302, ReadOnly);
    prop!("NCP:MCUPowerState", 13, ReadWrite);

    // --- Network properties ---
    prop!("Network:PANID", 0x36, ReadWrite);
    prop!("Network:Name", 0x44, ReadWrite);
    prop!("Network:XPANID", 0x45, ReadWrite);
    prop!("Network:Key", 0x46, ReadOnly);
    prop!("Network:KeyIndex", 0x47, ReadOnly);
    prop!("Network:PSKc", 0x4B, ReadOnly);
    prop!("Network:PartitionId", 0x48, ReadOnly);
    prop!("Network:NodeType", 0x43, ReadWrite);
    prop!("Network:KeySwitchGuardTime", 0x4A, ReadOnly);

    // --- Interface / Stack ---
    prop!("Interface:Up", 0x41, ReadWrite);
    prop!("Stack:Up", 0x42, ReadWrite);

    // --- IPv6 ---
    prop!("IPv6:MeshLocalPrefix", 0x62, ReadOnly);

    // --- PHY (TI Wi-SUN) ---
    prop!("NCP:Region", 0x50, ReadWrite);
    prop!("NCP:ModeID", 0x51, ReadWrite);
    prop!("UnicastChList", 0x52, ReadWrite);
    prop!("BroadcastChList", 0x53, ReadWrite);
    prop!("AsyncChList", 0x54, ReadWrite);
    prop!("RegulationChList", 0x55, ReadWrite);
    prop!("OperatingClass", 0x56, ReadWrite);
    prop!("NumChannels", 0x57, ReadOnly);
    prop!("ChSpacing", 0x1500, ReadWrite);
    prop!("Ch0CenterFreq", 0x1501, ReadWrite);

    // --- MAC timing (TI Wi-SUN) ---
    prop!("UCDwellInterval", 0x1556, ReadWrite);
    prop!("BCDwellInterval", 0x1557, ReadWrite);
    prop!("BCInterval", 0x1558, ReadWrite);
    prop!("UCChFunction", 0x1559, ReadWrite);
    prop!("BCChFunction", 0x155A, ReadWrite);
    prop!("MacFilterList", 0x155B, ReadWrite);
    prop!("MacFilterMode", 0x155C, ReadWrite);

    m
}

/// Look up a property handler by D-Bus key name.
pub(crate) fn lookup(name: &str) -> Option<&'static PropHandler> {
    use std::sync::LazyLock;
    static HANDLERS: LazyLock<HashMap<&'static str, PropHandler>> =
        LazyLock::new(build_handler_map);
    HANDLERS.get(name)
}
