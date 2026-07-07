//! Spinel protocol command IDs, from `third_party/openthread/src/ncp/spinel.h:787-1095`.

/// No-op command (ping / liveliness check).
pub const CMD_NOOP: u32 = 0;

/// Reset NCP command.
pub const CMD_RESET: u32 = 1;

/// Get property value command.
pub const CMD_PROP_VALUE_GET: u32 = 2;

/// Set property value command.
pub const CMD_PROP_VALUE_SET: u32 = 3;

/// Insert value into property command.
pub const CMD_PROP_VALUE_INSERT: u32 = 4;

/// Remove value from property command.
pub const CMD_PROP_VALUE_REMOVE: u32 = 5;

/// Property value notification / response (NCP → Host).
pub const CMD_PROP_VALUE_IS: u32 = 6;

/// Value inserted notification (NCP → Host).
pub const CMD_PROP_VALUE_INSERTED: u32 = 7;

/// Value removed notification (NCP → Host).
pub const CMD_PROP_VALUE_REMOVED: u32 = 8;

/// Save network settings to non-volatile storage.
pub const CMD_NET_SAVE: u32 = 9;

/// Clear network settings.
pub const CMD_NET_CLEAR: u32 = 10;

/// Recall network settings from non-volatile storage.
pub const CMD_NET_RECALL: u32 = 11;

/// Host-side buffer offload requests (Host → NCP).
pub const CMD_HBO_OFFLOAD: u32 = 12;
pub const CMD_HBO_RECLAIM: u32 = 13;
pub const CMD_HBO_DROP: u32 = 14;

/// Offload acknowledgement (NCP → Host).
pub const CMD_HBO_OFFLOADED: u32 = 15;
/// Reclaim acknowledgement (NCP → Host).
pub const CMD_HBO_RECLAIMED: u32 = 16;
/// Drop acknowledgement (NCP → Host).
pub const CMD_HBO_DROPPED: u32 = 17;

/// Peek / poke (debug / diagnostics).
pub const CMD_PEEK: u32 = 18;
pub const CMD_PEEK_RET: u32 = 19;
pub const CMD_POKE: u32 = 20;

/// Multi-get / multi-set (Host → NCP).
pub const CMD_PROP_VALUE_MULTI_GET: u32 = 21;
pub const CMD_PROP_VALUE_MULTI_SET: u32 = 22;

/// Multi-property value notification / response (NCP → Host).
pub const CMD_PROP_VALUES_ARE: u32 = 23;

/// Nest command range.
pub const CMD_NEST__BEGIN: u32 = 15296;
pub const CMD_NEST__END: u32 = 15360;

/// Vendor command range start.
pub const CMD_VENDOR__BEGIN: u32 = 15360;
/// Vendor command range end.
pub const CMD_VENDOR__END: u32 = 16384;

/// Experimental command range.
pub const CMD_EXPERIMENTAL__BEGIN: u32 = 2_000_000;
pub const CMD_EXPERIMENTAL__END: u32 = 2_097_152;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_command_ids() {
        assert_eq!(CMD_NOOP, 0);
        assert_eq!(CMD_RESET, 1);
        assert_eq!(CMD_PROP_VALUE_GET, 2);
        assert_eq!(CMD_PROP_VALUE_SET, 3);
        assert_eq!(CMD_NET_CLEAR, 10);
    }

    #[test]
    fn command_range_values() {
        // Range bounds must match spinel.h exactly; NEST__END abuts VENDOR__BEGIN.
        assert_eq!(CMD_NEST__BEGIN, 15296);
        assert_eq!(CMD_NEST__END, 15360);
        assert_eq!(CMD_VENDOR__BEGIN, 15360);
        assert_eq!(CMD_VENDOR__END, 16384);
        assert_eq!(CMD_EXPERIMENTAL__BEGIN, 2_000_000);
        assert_eq!(CMD_EXPERIMENTAL__END, 2_097_152);
    }

    #[test]
    fn notification_command_values() {
        assert_eq!(CMD_HBO_OFFLOADED, 15);
        assert_eq!(CMD_HBO_RECLAIMED, 16);
        assert_eq!(CMD_HBO_DROPPED, 17);
        assert_eq!(CMD_PROP_VALUES_ARE, 23);
    }
}
