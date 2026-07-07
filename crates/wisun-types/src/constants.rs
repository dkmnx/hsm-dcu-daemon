//! Numeric constants from NCPConstants.h and Wi-SUN defaults.

// ---------------------------------------------------------------------------
// Baud rates
// ---------------------------------------------------------------------------

pub const DEFAULT_BAUD_RATE: u32 = 115200;
pub const HIGH_THROUGHPUT_BAUD_RATE: u32 = 460800;

// ---------------------------------------------------------------------------
// Default interface / network identifiers
// ---------------------------------------------------------------------------

pub const DEFAULT_INTERFACE_NAME: &str = "wfan0";
pub const DEFAULT_IPV6_PREFIX: &str = "2020:ABCD::/64";

// ---------------------------------------------------------------------------
// Timeouts (milliseconds)
// ---------------------------------------------------------------------------

/// Default command response timeout (`NCP_DEFAULT_COMMAND_RESPONSE_TIMEOUT` = 5 s).
pub const NCP_DEFAULT_COMMAND_RESPONSE_TIMEOUT_MS: u64 = 5000;
/// Default command send timeout (`NCP_DEFAULT_COMMAND_SEND_TIMEOUT` = 5 s).
pub const NCP_DEFAULT_COMMAND_SEND_TIMEOUT_MS: u64 = 5000;
/// Tickle timeout (`NCP_TICKLE_TIMEOUT` = 60 s).
pub const NCP_TICKLE_TIMEOUT_MS: u64 = 60_000;
/// Deep sleep tickle timeout (`NCP_DEEP_SLEEP_TICKLE_TIMEOUT` = 60*70 = 4200 s).
pub const NCP_DEEP_SLEEP_TICKLE_TIMEOUT_MS: u64 = 4_200_000;
/// Form timeout (`NCP_FORM_TIMEOUT` = 60 s).
pub const NCP_FORM_TIMEOUT_S: u64 = 60;
/// Join timeout (`NCP_JOIN_TIMEOUT` = 60 s).
pub const NCP_JOIN_TIMEOUT_S: u64 = 60;
/// Joiner timeout (`NCP_JOINER_TIMEOUT` = 60 s).
pub const NCP_JOINER_TIMEOUT_S: u64 = 60;
/// Reset timeout (`NCP_RESET_TIMEOUT` = 10 s with libudev, 0 without).
pub const NCP_RESET_TIMEOUT_S: u64 = 10;
/// Combined default reset response timeout.
pub const NCP_DEFAULT_RESET_RESPONSE_TIMEOUT_MS: u64 =
    NCP_DEFAULT_COMMAND_RESPONSE_TIMEOUT_MS + NCP_RESET_TIMEOUT_S * 1000;

// ---------------------------------------------------------------------------
// Buffer / size limits
// ---------------------------------------------------------------------------

/// Network key size in bytes (`NCP_NETWORK_KEY_SIZE`).
pub const NCP_NETWORK_KEY_SIZE: usize = 16;
/// Extended PAN ID size in bytes (`NCP_XPANID_SIZE`).
pub const NCP_XPANID_SIZE: usize = 8;
/// EUI-64 size in bytes (`NCP_EUI64_SIZE`).
pub const NCP_EUI64_SIZE: usize = 8;
/// Maximum debug line length (`NCP_DEBUG_LINE_LENGTH_MAX`).
pub const NCP_DEBUG_LINE_LENGTH_MAX: usize = 400;
/// MAC filter list size in entries (`MAC_FILTER_LIST_SIZE` from NCPTypes.h:29).
pub const MAC_FILTER_LIST_SIZE: usize = 10;

// ---------------------------------------------------------------------------
// Channel mask
// ---------------------------------------------------------------------------

/// Maximum number of Wi-SUN FAN channels (129).
pub const MAX_CHANNELS: usize = 129;
/// Size of the channel mask bitfield in bytes (129 bits → 17 bytes).
pub const MAX_CHANNEL_MASK_SIZE: usize = 17;

// ---------------------------------------------------------------------------
// Busy / insomnia
// ---------------------------------------------------------------------------

/// Busy debounce time in ms (`BUSY_DEBOUNCE_TIME_IN_MS`).
pub const BUSY_DEBOUNCE_TIME_IN_MS: u64 = 200;
/// Maximum insomnia time in ms (`MAX_INSOMNIA_TIME_IN_MS` = 60s).
pub const MAX_INSOMNIA_TIME_IN_MS: u64 = 60_000;

// ---------------------------------------------------------------------------
// Wi-SUN default configuration (from wisun_config.h)
// ---------------------------------------------------------------------------

pub const WISUN_DEFAULT_PANID: u16 = 0xABCD;
pub const WISUN_DEFAULT_NETWORK_NAME: &str = "Wi-SUN Network";
pub const WISUN_DEFAULT_CCA_THRESHOLD: i32 = -60;
pub const WISUN_DEFAULT_TX_POWER: i32 = 10;
pub const WISUN_DEFAULT_UC_CH_FUNCTION: u8 = 2;
pub const WISUN_DEFAULT_BC_CH_FUNCTION: u8 = 2;
pub const WISUN_DEFAULT_UC_DWELL_INTERVAL: u32 = 100;
pub const WISUN_DEFAULT_BC_DWELL_INTERVAL: u32 = 250;
pub const WISUN_DEFAULT_BC_INTERVAL: u32 = 1000;
pub const WISUN_DEFAULT_REGION: u8 = 1;
pub const WISUN_DEFAULT_CH_SPACING_KHZ: u32 = 200;
/// Ch0 center frequency: integer MHz part (902.2 MHz → 902).
pub const WISUN_DEFAULT_CH0_CENTER_FREQ_MHZ_PART: u32 = 902;
/// Ch0 center frequency: fractional kHz part (902.2 MHz → 200 kHz).
pub const WISUN_DEFAULT_CH0_CENTER_FREQ_KHZ_PART: u32 = 200;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_match_c_defines() {
        // NCPConstants.h values
        assert_eq!(NCP_DEFAULT_COMMAND_RESPONSE_TIMEOUT_MS, 5000);
        assert_eq!(NCP_DEEP_SLEEP_TICKLE_TIMEOUT_MS, 4_200_000);
        assert_eq!(NCP_FORM_TIMEOUT_S, 60);
        assert_eq!(NCP_JOIN_TIMEOUT_S, 60);
        assert_eq!(NCP_NETWORK_KEY_SIZE, 16);
        assert_eq!(NCP_XPANID_SIZE, 8);
        assert_eq!(NCP_EUI64_SIZE, 8);
    }

    #[test]
    fn wisun_defaults() {
        assert_eq!(WISUN_DEFAULT_PANID, 0xABCD);
        assert_eq!(WISUN_DEFAULT_NETWORK_NAME, "Wi-SUN Network");
        assert_eq!(WISUN_DEFAULT_CCA_THRESHOLD, -60);
        assert_eq!(WISUN_DEFAULT_TX_POWER, 10);
    }

    #[test]
    fn channel_mask_sizes() {
        assert_eq!(MAX_CHANNELS, 129);
        assert_eq!(MAX_CHANNEL_MASK_SIZE, 17);
    }
}
