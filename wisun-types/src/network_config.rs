use std::fmt;
use std::str::FromStr;

// ---------------------------------------------------------------------------
// NetworkName
// ---------------------------------------------------------------------------

/// A Wi-SUN network name (UTF-8, max 32 bytes). Newtype over `String`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NetworkName(pub String);

impl FromStr for NetworkName {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() > 32 {
            return Err("NetworkName too long (max 32 bytes)");
        }
        Ok(NetworkName(s.to_string()))
    }
}

impl fmt::Display for NetworkName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ---------------------------------------------------------------------------
// PanId
// ---------------------------------------------------------------------------

/// A 16-bit PAN ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PanId(pub u16);

impl PanId {
    /// The default PAN ID from `wisun_config.h:24` (`0xABCD`).
    pub const DEFAULT: PanId = PanId(0xABCD);
}

impl FromStr for PanId {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let without_prefix =
            if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                hex
            } else {
                s
            };
        u16::from_str_radix(without_prefix, 16)
            .map(PanId)
            .map_err(|_| "invalid PAN ID hex string")
    }
}

impl fmt::Display for PanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:04X}", self.0)
    }
}

impl TryFrom<&[u8]> for PanId {
    type Error = &'static str;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() < 2 {
            return Err("PanId requires at least 2 bytes");
        }
        Ok(PanId(u16::from_le_bytes([bytes[0], bytes[1]])))
    }
}

// ---------------------------------------------------------------------------
// XPanId
// ---------------------------------------------------------------------------

/// A 64-bit extended PAN ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct XPanId(pub u64);

impl FromStr for XPanId {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let without_prefix =
            if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                hex
            } else {
                s
            };
        if without_prefix.len() > 16 {
            return Err("XPANID hex string too long");
        }
        u64::from_str_radix(without_prefix, 16)
            .map(XPanId)
            .map_err(|_| "invalid XPANID hex string")
    }
}

impl fmt::Display for XPanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016X}", self.0)
    }
}

impl TryFrom<&[u8]> for XPanId {
    type Error = &'static str;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() < 8 {
            return Err("XPanId requires at least 8 bytes");
        }
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&bytes[..8]);
        Ok(XPanId(u64::from_le_bytes(buf)))
    }
}

// ---------------------------------------------------------------------------
// ChannelMask
// ---------------------------------------------------------------------------

/// A bitfield representing up to 129 Wi-SUN channels (17 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelMask(pub [u8; 17]);

impl ChannelMask {
    /// Creates an empty (all-zero) channel mask.
    pub fn empty() -> Self {
        ChannelMask([0u8; 17])
    }

    /// Creates a channel mask with all 129 bits set.
    pub fn all() -> Self {
        let mut mask = [0u8; 17];
        mask[..16].fill(0xFF);
        mask[16] = 0x01; // bit 128 (bit 0 of byte 16)
        ChannelMask(mask)
    }

    /// Set the bit for the given channel number (0..=128).
    ///
    /// # Panics
    ///
    /// Panics in debug builds if `channel > 128`.
    pub fn set_channel(&mut self, channel: u8) {
        debug_assert!(
            channel <= 128,
            "channel {channel} out of valid range 0..=128"
        );
        let (byte, bit) = (channel as usize / 8, channel as usize % 8);
        self.0[byte] |= 1 << bit;
    }

    /// Returns `true` if the given channel is set.
    ///
    /// Returns `false` for channels outside the valid range (0..=128).
    pub fn is_channel_set(&self, channel: u8) -> bool {
        if channel > 128 {
            return false;
        }
        let (byte, bit) = (channel as usize / 8, channel as usize % 8);
        (self.0[byte] & (1 << bit)) != 0
    }

    /// Returns the hex-string representation (e.g. `"ff:ff:...:01"`).
    pub fn to_hex_string(&self) -> String {
        self.0
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(":")
    }
}

impl FromStr for ChannelMask {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 17 {
            return Err("ChannelMask requires exactly 17 hex bytes");
        }
        let mut mask = [0u8; 17];
        for (i, part) in parts.iter().enumerate() {
            mask[i] =
                u8::from_str_radix(part, 16).map_err(|_| "invalid hex byte in ChannelMask")?;
        }
        Ok(ChannelMask(mask))
    }
}

impl fmt::Display for ChannelMask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex_string())
    }
}

// ---------------------------------------------------------------------------
// Eui64
// ---------------------------------------------------------------------------

/// A 64-bit IEEE EUI-64 address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Eui64(pub [u8; 8]);

impl FromStr for Eui64 {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();

        // Colon-separated: "00:12:4B:00:14:F7:D2:E6"
        if s.contains(':') {
            let parts: Vec<&str> = s.split(':').collect();
            if parts.len() != 8 {
                return Err("EUI-64 colon-separated form requires exactly 8 hex bytes");
            }
            let mut bytes = [0u8; 8];
            for (i, part) in parts.iter().enumerate() {
                bytes[i] = u8::from_str_radix(part, 16)
                    .map_err(|_| "invalid hex byte in colon-separated EUI-64")?;
            }
            return Ok(Eui64(bytes));
        }

        // Bare concatenated hex: "00124B0014F7D2E6"
        if s.len() != 16 {
            return Err("EUI-64 string must be exactly 16 hex digits (or 8 colon-separated bytes)");
        }
        let mut bytes = [0u8; 8];
        for i in 0..8 {
            bytes[i] = u8::from_str_radix(&s[2 * i..2 * i + 2], 16)
                .map_err(|_| "invalid hex byte in EUI-64")?;
        }
        Ok(Eui64(bytes))
    }
}

impl fmt::Display for Eui64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Uppercase without separators (canonical dcuctl output format).
        for byte in &self.0 {
            write!(f, "{byte:02X}")?;
        }
        Ok(())
    }
}

impl TryFrom<&[u8]> for Eui64 {
    type Error = &'static str;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() < 8 {
            return Err("Eui64 requires at least 8 bytes");
        }
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&bytes[..8]);
        Ok(Eui64(buf))
    }
}

// ---------------------------------------------------------------------------
// IPv6Address
// ---------------------------------------------------------------------------

/// A 128-bit IPv6 address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ipv6Address(pub [u8; 16]);

impl FromStr for Ipv6Address {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() {
            return Err("empty IPv6 address string");
        }

        // Handle bracketed notation like [::1]
        let s = s
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
            .unwrap_or(s);

        let addr: std::net::Ipv6Addr = s.parse().map_err(|_| "invalid IPv6 address string")?;
        Ok(Ipv6Address(addr.octets()))
    }
}

impl fmt::Display for Ipv6Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let addr = std::net::Ipv6Addr::from(self.0);
        write!(f, "{addr}")
    }
}

impl From<[u8; 16]> for Ipv6Address {
    fn from(octets: [u8; 16]) -> Self {
        Ipv6Address(octets)
    }
}

impl From<std::net::Ipv6Addr> for Ipv6Address {
    fn from(addr: std::net::Ipv6Addr) -> Self {
        Ipv6Address(addr.octets())
    }
}

impl From<Ipv6Address> for std::net::Ipv6Addr {
    fn from(addr: Ipv6Address) -> Self {
        std::net::Ipv6Addr::from(addr.0)
    }
}

impl TryFrom<&[u8]> for Ipv6Address {
    type Error = &'static str;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() < 16 {
            return Err("Ipv6Address requires at least 16 bytes");
        }
        let mut buf = [0u8; 16];
        buf.copy_from_slice(&bytes[..16]);
        Ok(Ipv6Address(buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- EUI-64 ---

    #[test]
    fn eui64_from_hex_string() {
        let eui: Eui64 = "00124B0014F7D2E6".parse().unwrap();
        assert_eq!(eui.0, [0x00, 0x12, 0x4B, 0x00, 0x14, 0xF7, 0xD2, 0xE6]);
        assert_eq!(eui.to_string(), "00124B0014F7D2E6");
    }

    #[test]
    fn eui64_from_colon_separated() {
        let eui: Eui64 = "00:12:4B:00:14:F7:D2:E6".parse().unwrap();
        assert_eq!(eui.0, [0x00, 0x12, 0x4B, 0x00, 0x14, 0xF7, 0xD2, 0xE6]);
    }

    #[test]
    fn eui64_round_trip_colon() {
        let s = "00:12:4B:00:14:F7:D2:E6";
        let eui: Eui64 = s.parse().unwrap();
        // Display always produces uppercase bare hex.
        assert_eq!(eui.to_string(), "00124B0014F7D2E6");
    }

    #[test]
    fn eui64_from_bytes() {
        let bytes = [0x00, 0x12, 0x4B, 0x00, 0x14, 0xF7, 0xD2, 0xE6];
        let eui = Eui64::try_from(&bytes[..]).unwrap();
        assert_eq!(eui.0, bytes);
    }

    #[test]
    fn eui64_invalid_length() {
        assert!("00124B0014F7".parse::<Eui64>().is_err());
        assert!("00124B0014F7D2E6ZZ".parse::<Eui64>().is_err());
        assert!("00:12:4B:00:14:F7:D2".parse::<Eui64>().is_err());
    }

    // --- ChannelMask ---

    #[test]
    fn channel_mask_bit_manipulation() {
        let mut mask = ChannelMask::empty();
        mask.set_channel(0);
        mask.set_channel(128);
        assert!(mask.is_channel_set(0));
        assert!(mask.is_channel_set(128));
        assert!(!mask.is_channel_set(1));
        assert_eq!(
            mask.to_hex_string(),
            "01:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:01"
        );
    }

    #[test]
    fn channel_mask_all() {
        let mask = ChannelMask::all();
        assert!(mask.is_channel_set(0));
        assert!(mask.is_channel_set(127));
        assert!(mask.is_channel_set(128));
        assert!(!mask.is_channel_set(129)); // out of range
    }

    #[test]
    fn channel_mask_parse() {
        let s = "01:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:01";
        let mask: ChannelMask = s.parse().unwrap();
        assert!(mask.is_channel_set(0));
        assert!(mask.is_channel_set(128));
        assert_eq!(mask.to_hex_string(), s);
    }

    #[test]
    fn channel_mask_wrong_len() {
        assert!("ff:ff:ff".parse::<ChannelMask>().is_err());
    }

    #[test]
    #[should_panic = "channel 200 out of valid range"]
    fn channel_mask_set_out_of_range() {
        let mut mask = ChannelMask::empty();
        mask.set_channel(200);
    }

    // --- PAN ID ---

    #[test]
    fn pan_id_parse() {
        let pan: PanId = "0xABCD".parse().unwrap();
        assert_eq!(pan.0, 0xABCD);
        assert_eq!(pan.to_string(), "0xABCD");
    }

    #[test]
    fn pan_id_parse_without_prefix() {
        let pan: PanId = "ABCD".parse().unwrap();
        assert_eq!(pan.0, 0xABCD);
    }

    #[test]
    fn pan_id_default() {
        assert_eq!(PanId::DEFAULT.0, 0xABCD);
    }

    #[test]
    fn pan_id_from_bytes() {
        let pan = PanId::try_from(&[0xCD, 0xAB][..]).unwrap();
        assert_eq!(pan.0, 0xABCD);
    }

    // --- XPANID ---

    #[test]
    fn xpanid_round_trip() {
        let xpan: XPanId = "0xDEADBEEFCAFEBABE".parse().unwrap();
        assert_eq!(xpan.0, 0xDEAD_BEEF_CAFE_BABE);
        let s = xpan.to_string();
        assert_eq!(s, "DEADBEEFCAFEBABE");
    }

    #[test]
    fn xpanid_from_bytes() {
        let xpan = XPanId::try_from(&[0xBE, 0xBA, 0xFE, 0xCA, 0xEF, 0xBE, 0xAD, 0xDE][..]).unwrap();
        assert_eq!(xpan.0, 0xDEAD_BEEF_CAFE_BABE);
    }

    // --- NetworkName ---

    #[test]
    fn network_name_round_trip() {
        let name: NetworkName = "Wi-SUN Network".parse().unwrap();
        assert_eq!(name.to_string(), "Wi-SUN Network");
    }

    #[test]
    fn network_name_too_long() {
        let long = "a".repeat(33);
        assert!(long.parse::<NetworkName>().is_err());
    }

    // --- IPv6 ---

    #[test]
    fn ipv6_address_parse() {
        let addr: Ipv6Address = "2020:abcd::1".parse().unwrap();
        assert_eq!(addr.0[0], 0x20);
        assert_eq!(addr.0[1], 0x20);
        assert_eq!(addr.0[15], 0x01);
    }

    #[test]
    fn ipv6_address_display() {
        let addr = Ipv6Address::from(std::net::Ipv6Addr::new(0x2020, 0xabcd, 0, 0, 0, 0, 0, 1));
        assert_eq!(addr.to_string(), "2020:abcd::1");
    }

    #[test]
    fn ipv6_address_from_bytes() {
        let bytes = [0x20, 0x20, 0xab, 0xcd, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
        let addr = Ipv6Address::try_from(&bytes[..]).unwrap();
        assert_eq!(addr.0, bytes);
    }
}
