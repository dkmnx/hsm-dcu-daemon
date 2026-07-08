//! Spinel command IDs.
//!
//! These mirror the `SPINEL_CMD_*` enum in
//! `third_party/openthread/src/ncp/spinel.h` (lines 787-1095). Command IDs
//! are encoded on the wire as UINT_PACKED (LEB128), see [`crate::pack`].
//!
//! The vendor and experimental ranges are bounded but the individual TI/NCP
//! vendor command codes are not fixed in OpenThread; TI extensions are sent as
//! property GET/SET commands against vendor-range property IDs (see
//! [`crate::vendor`] and [`crate::property`]).

/// No operation.
pub const CMD_NOOP: u32 = 0;
/// Reset the NCP.
pub const CMD_RESET: u32 = 1;
/// Get a property value.
pub const CMD_PROP_VALUE_GET: u32 = 2;
/// Set a property value.
pub const CMD_PROP_VALUE_SET: u32 = 3;
/// Insert an item into a property collection.
pub const CMD_PROP_VALUE_INSERT: u32 = 4;
/// Remove an item from a property collection.
pub const CMD_PROP_VALUE_REMOVE: u32 = 5;
/// Property value is (unsolicited or response).
pub const CMD_PROP_VALUE_IS: u32 = 6;
/// Item inserted into property collection.
pub const CMD_PROP_VALUE_INSERTED: u32 = 7;
/// Item removed from property collection.
pub const CMD_PROP_VALUE_REMOVED: u32 = 8;
/// Save network settings (deprecated).
pub const CMD_NET_SAVE: u32 = 9;
/// Clear network settings.
pub const CMD_NET_CLEAR: u32 = 10;
/// Recall network settings (deprecated).
pub const CMD_NET_RECALL: u32 = 11;
/// Host-bound offload.
pub const CMD_HBO_OFFLOAD: u32 = 12;
/// Host-bound reclaim.
pub const CMD_HBO_RECLAIM: u32 = 13;
/// Host-bound drop.
pub const CMD_HBO_DROP: u32 = 14;
/// Host-bound offloaded.
pub const CMD_HBO_OFFLOADED: u32 = 15;
/// Host-bound reclaimed.
pub const CMD_HBO_RECLAIMED: u32 = 16;
/// Host-bound dropped.
pub const CMD_HBO_DROPPED: u32 = 17;
/// Peek memory.
pub const CMD_PEEK: u32 = 18;
/// Peek return.
pub const CMD_PEEK_RET: u32 = 19;
/// Poke memory.
pub const CMD_POKE: u32 = 20;
/// Get multiple property values.
pub const CMD_PROP_VALUE_MULTI_GET: u32 = 21;
/// Set multiple property values.
pub const CMD_PROP_VALUE_MULTI_SET: u32 = 22;
/// Multiple property values are.
pub const CMD_PROP_VALUES_ARE: u32 = 23;

/// Start of the NEST vendor command range.
pub const CMD_NEST__BEGIN: u32 = 15296;
/// End of the NEST vendor command range.
pub const CMD_NEST__END: u32 = 15360;
/// Start of the vendor command range.
pub const CMD_VENDOR__BEGIN: u32 = 15360;
/// End of the vendor command range.
pub const CMD_VENDOR__END: u32 = 16384;
/// Start of the experimental command range.
pub const CMD_EXPERIMENTAL__BEGIN: u32 = 2_000_000;
/// End of the experimental command range.
pub const CMD_EXPERIMENTAL__END: u32 = 2_097_152;

/// Returns `true` if `cmd` is a vendor-range command (`CMD_VENDOR__BEGIN..CMD_VENDOR__END`).
pub fn is_vendor_command(cmd: u32) -> bool {
    (CMD_VENDOR__BEGIN..CMD_VENDOR__END).contains(&cmd)
}

/// Returns `true` if `cmd` is an experimental-range command.
pub fn is_experimental_command(cmd: u32) -> bool {
    (CMD_EXPERIMENTAL__BEGIN..=CMD_EXPERIMENTAL__END).contains(&cmd)
}

/// Returns the canonical name of a standard (non-vendor) command, if known.
#[must_use]
pub fn command_name(cmd: u32) -> Option<&'static str> {
    Some(match cmd {
        CMD_NOOP => "NOOP",
        CMD_RESET => "RESET",
        CMD_PROP_VALUE_GET => "PROP_VALUE_GET",
        CMD_PROP_VALUE_SET => "PROP_VALUE_SET",
        CMD_PROP_VALUE_INSERT => "PROP_VALUE_INSERT",
        CMD_PROP_VALUE_REMOVE => "PROP_VALUE_REMOVE",
        CMD_PROP_VALUE_IS => "PROP_VALUE_IS",
        CMD_PROP_VALUE_INSERTED => "PROP_VALUE_INSERTED",
        CMD_PROP_VALUE_REMOVED => "PROP_VALUE_REMOVED",
        CMD_NET_SAVE => "NET_SAVE",
        CMD_NET_CLEAR => "NET_CLEAR",
        CMD_NET_RECALL => "NET_RECALL",
        CMD_HBO_OFFLOAD => "HBO_OFFLOAD",
        CMD_HBO_RECLAIM => "HBO_RECLAIM",
        CMD_HBO_DROP => "HBO_DROP",
        CMD_HBO_OFFLOADED => "HBO_OFFLOADED",
        CMD_HBO_RECLAIMED => "HBO_RECLAIMED",
        CMD_HBO_DROPPED => "HBO_DROPPED",
        CMD_PEEK => "PEEK",
        CMD_PEEK_RET => "PEEK_RET",
        CMD_POKE => "POKE",
        CMD_PROP_VALUE_MULTI_GET => "PROP_VALUE_MULTI_GET",
        CMD_PROP_VALUE_MULTI_SET => "PROP_VALUE_MULTI_SET",
        CMD_PROP_VALUES_ARE => "PROP_VALUES_ARE",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_commands_match_spec() {
        assert_eq!(CMD_PROP_VALUE_IS, 6);
        assert_eq!(CMD_PROP_VALUE_GET, 2);
        assert_eq!(CMD_PROP_VALUE_SET, 3);
    }

    #[test]
    fn vendor_range_bounds() {
        assert!(is_vendor_command(15360));
        assert!(!is_vendor_command(6));
        assert!(!is_vendor_command(16384));
    }

    #[test]
    fn known_names() {
        assert_eq!(command_name(CMD_RESET), Some("RESET"));
        assert_eq!(command_name(CMD_PROP_VALUE_IS), Some("PROP_VALUE_IS"));
        assert_eq!(command_name(9999), None);
    }
}
