//! TI Wi-SUN vendor-specific Spinel properties.
//!
//! TI's CC13xx/CC26xx NCP exposes Wi-SUN configuration through Spinel
//! properties in the vendor range (`0x3C00..0x4000`, see
//! [`crate::property::PROP_VENDOR__BEGIN`]). This module defines typed
//! wrappers for the vendor property *payloads* and their wire encoding.
//!
//! The numeric property IDs themselves are allocated per the wpantund string
//! keys (see `crates/wisun-types/src/property_key.rs`); the wire encoding of
//! each value is described here.

use crate::error::SpinelError;
use crate::pack::{PackReader, PackWriter};

// ---------------------------------------------------------------------------
// Property ID aliases (TI Wi-SUN vendor range, 0x3C00+)
// ---------------------------------------------------------------------------

pub const PROP_UNICAST_CHANNEL_LIST: u32 = 0x3C00;
pub const PROP_BROADCAST_CHANNEL_LIST: u32 = 0x3C01;
pub const PROP_ASYNC_CHANNEL_LIST: u32 = 0x3C02;
pub const PROP_REGULATION_CHANNEL_LIST: u32 = 0x3C03;
pub const PROP_CHANNEL_SPACING: u32 = 0x3C04;
pub const PROP_CH0_CENTER_FREQ: u32 = 0x3C05;
pub const PROP_OPERATING_CLASS: u32 = 0x3C06;
pub const PROP_NUM_CHANNELS: u32 = 0x3C07;
pub const PROP_PHY_REGION: u32 = 0x3C08;
pub const PROP_MODE_ID: u32 = 0x3C09;

// ---------------------------------------------------------------------------
// Channel list types
// ---------------------------------------------------------------------------

/// Number of bytes in a TI Wi-SUN channel bitmask (129 channels → 17 bytes).
pub const CHANNEL_LIST_LEN: usize = 17;

/// A TI Wi-SUN channel bitmask.
///
/// Bit `n` set (where `n` is the channel index, 0..=128) means channel `n` is
/// included. Encoded as 17 bytes, little-endian bit order within each byte
/// (byte 0 holds channels 0..7, LSB-first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChannelList(pub [u8; CHANNEL_LIST_LEN]);

impl ChannelList {
    /// All channels clear.
    pub const EMPTY: ChannelList = ChannelList([0u8; CHANNEL_LIST_LEN]);

    /// Set the given channel bit. No-op if `channel > 127`.
    pub fn set(&mut self, channel: u8) {
        if (channel as usize) < CHANNEL_LIST_LEN * 8 {
            let byte = channel as usize / 8;
            let bit = channel % 8;
            self.0[byte] |= 1 << bit;
        }
    }

    /// Returns `true` if `channel` bit is set.
    pub fn is_set(&self, channel: u8) -> bool {
        if (channel as usize) >= CHANNEL_LIST_LEN * 8 {
            return false;
        }
        let byte = channel as usize / 8;
        let bit = channel % 8;
        (self.0[byte] & (1 << bit)) != 0
    }

    /// Count of set channels.
    pub fn count(&self) -> u32 {
        self.0.iter().map(|b| b.count_ones()).sum()
    }

    /// Encode as 17 raw bytes (no length prefix — this is the `"D"` form).
    pub fn encode(&self) -> Vec<u8> {
        self.0.to_vec()
    }

    /// Decode from exactly 17 raw bytes.
    pub fn decode(data: &[u8]) -> Result<Self, SpinelError> {
        if data.len() < CHANNEL_LIST_LEN {
            return Err(SpinelError::Underflow);
        }
        let mut buf = [0u8; CHANNEL_LIST_LEN];
        buf.copy_from_slice(&data[..CHANNEL_LIST_LEN]);
        Ok(ChannelList(buf))
    }
}

/// Unicast channel list (bitmask).
pub type UnicastChannelList = ChannelList;
/// Broadcast channel list (bitmask).
pub type BroadcastChannelList = ChannelList;
/// Async channel list (bitmask).
pub type AsyncChannelList = ChannelList;
/// Regulation channel list (bitmask).
pub type RegulationChannelList = ChannelList;

// ---------------------------------------------------------------------------
// Channel spacing & center frequency
// ---------------------------------------------------------------------------

/// Wi-SUN channel spacing, in kHz.
///
/// Encoded as a 32-bit unsigned integer (Spinel `"L"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChannelSpacing(pub u32);

impl ChannelSpacing {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = PackWriter::new();
        w.write_uint32(self.0);
        w.into_bytes()
    }
    pub fn decode(data: &[u8]) -> Result<Self, SpinelError> {
        let mut r = PackReader::new(data);
        Ok(ChannelSpacing(r.read_uint32()?))
    }
}

/// Wi-SUN Ch0 center frequency.
///
/// Encoded as two uint16 little-endian values: MHz then kHz fraction
/// (Spinel `"SS"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Ch0CenterFreq {
    /// Integer MHz part.
    pub mhz: u16,
    /// Fractional kHz part.
    pub khz: u16,
}

impl Ch0CenterFreq {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = PackWriter::new();
        w.write_uint16(self.mhz);
        w.write_uint16(self.khz);
        w.into_bytes()
    }
    pub fn decode(data: &[u8]) -> Result<Self, SpinelError> {
        let mut r = PackReader::new(data);
        Ok(Ch0CenterFreq {
            mhz: r.read_uint16()?,
            khz: r.read_uint16()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_list_set_and_count() {
        let mut list = ChannelList::EMPTY;
        list.set(0);
        list.set(7);
        list.set(128);
        assert!(list.is_set(0));
        assert!(list.is_set(7));
        assert!(list.is_set(128));
        assert!(!list.is_set(8));
        assert_eq!(list.count(), 3);
        assert_eq!(list.0[0], 0b1000_0001);
        assert_eq!(list.0[16], 0b0000_0001);
    }

    #[test]
    fn channel_list_round_trip() {
        let mut list = ChannelList::EMPTY;
        list.set(5);
        list.set(42);
        let bytes = list.encode();
        assert_eq!(bytes.len(), CHANNEL_LIST_LEN);
        let decoded = ChannelList::decode(&bytes).unwrap();
        assert_eq!(decoded, list);
        assert!(decoded.is_set(5));
        assert!(decoded.is_set(42));
    }

    #[test]
    fn channel_list_underflow() {
        assert_eq!(ChannelList::decode(&[0u8; 5]), Err(SpinelError::Underflow));
    }

    #[test]
    fn channel_spacing_round_trip() {
        let cs = ChannelSpacing(200);
        let bytes = cs.encode();
        assert_eq!(bytes, vec![200, 0, 0, 0]);
        assert_eq!(ChannelSpacing::decode(&bytes).unwrap(), cs);
    }

    #[test]
    fn ch0_center_freq_round_trip() {
        let freq = Ch0CenterFreq { mhz: 902, khz: 400 };
        let bytes = freq.encode();
        assert_eq!(bytes, vec![0x86, 0x03, 0x90, 0x01]); // 902, 400 LE
        assert_eq!(Ch0CenterFreq::decode(&bytes).unwrap(), freq);
    }
}
