//! Operational dataset codec (phase 3C).
//!
//! Ports `SpinelNCPThreadDataset` (C++ `ThreadDataset`) to idiomatic Rust.
//! Fields are `Option<T>` because only set fields travel in the Spinel frame;
//! Rust's built-in `Option` replaces the C++ `Optional<T>` wrapper.

use std::net::Ipv6Addr;

use ipnet::Ipv6Net;
use spinel::error::SpinelError;
use spinel::pack::{PackReader, PackWriter};
use spinel::property;

/// Canonical list of all dataset field keys in the order they appear
/// in the D-Bus property table. Used by `to_string_list`, `property_string`,
/// and `sync_dataset_to_state_static` to avoid duplication.
pub const DATASET_PROPERTY_KEYS: &[&str] = &[
    wisun_types::PROP_DATASET_ACTIVE_TIMESTAMP,
    wisun_types::PROP_DATASET_PENDING_TIMESTAMP,
    wisun_types::PROP_DATASET_MASTER_KEY,
    wisun_types::PROP_DATASET_NETWORK_NAME,
    wisun_types::PROP_DATASET_EXTENDED_PAN_ID,
    wisun_types::PROP_DATASET_MESH_LOCAL_PREFIX,
    wisun_types::PROP_DATASET_DELAY,
    wisun_types::PROP_DATASET_PAN_ID,
    wisun_types::PROP_DATASET_CHANNEL,
    wisun_types::PROP_DATASET_PSKC,
    wisun_types::PROP_DATASET_CHANNEL_MASK_PAGE0,
    wisun_types::PROP_DATASET_SEC_POLICY_KEY_ROTATION,
    wisun_types::PROP_DATASET_SEC_POLICY_FLAGS,
    wisun_types::PROP_DATASET_RAW_TLVS,
    wisun_types::PROP_DATASET_DEST_IP_ADDRESS,
];

/// Security policy for the operational dataset.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SecurityPolicy {
    pub key_rotation_time: u16,
    pub flags: u8,
}

/// Operational dataset — mirrors C++ `ThreadDataset`.
///
/// Fields are `Option<T>` because only set fields are present in the
/// Spinel frame. `None` means the field was absent in the wire payload.
#[derive(Debug, Clone, Default)]
pub struct OperationalDataset {
    pub active_timestamp: Option<u64>,
    pub pending_timestamp: Option<u64>,
    pub master_key: Option<Vec<u8>>,
    pub network_name: Option<String>,
    pub extended_pan_id: Option<Vec<u8>>,
    pub mesh_local_prefix: Option<Ipv6Net>,
    pub delay: Option<u32>,
    pub pan_id: Option<u16>,
    pub channel: Option<u8>,
    pub pskc: Option<Vec<u8>>,
    pub channel_mask_page0: Option<u32>,
    pub security_policy: Option<SecurityPolicy>,
    pub raw_tlvs: Option<Vec<u8>>,
    pub dest_ip_address: Option<Ipv6Addr>,
}

impl OperationalDataset {
    /// Decode a Spinel frame payload into a new dataset.
    ///
    /// Wire format: `A(t(iD))` — concatenated length-prefixed structs
    /// (each `UINT_PACKED` key + length-prefixed value).
    pub fn from_spinel_frame(data: &[u8]) -> Result<Self, SpinelError> {
        let mut dataset = Self::default();
        let mut reader = PackReader::new(data);
        while !reader.is_empty() {
            let entry = reader.read_struct()?;
            let mut sub = PackReader::new(entry);
            let prop_key = sub.read_uint_packed()?;
            let value = sub.read_bytes(sub.remaining())?;
            dataset.parse_entry(prop_key, value)?;
        }
        Ok(dataset)
    }

    fn parse_entry(&mut self, key: u32, value: &[u8]) -> Result<(), SpinelError> {
        let mut r = PackReader::new(value);
        match key {
            property::DATASET_ACTIVE_TIMESTAMP => {
                self.active_timestamp = Some(r.read_uint64()?);
            }
            property::DATASET_PENDING_TIMESTAMP => {
                self.pending_timestamp = Some(r.read_uint64()?);
            }
            property::NET_MASTER_KEY => {
                self.master_key = Some(value.to_vec());
            }
            property::NET_NETWORK_NAME => {
                self.network_name = Some(r.read_utf8()?);
            }
            property::NET_XPANID => {
                self.extended_pan_id = Some(value.to_vec());
            }
            property::IPV6_ML_PREFIX => {
                let addr = Ipv6Addr::from(r.read_ipv6()?);
                let prefix_len = r.read_uint8()?;
                if prefix_len != 64 {
                    return Err(SpinelError::InvalidValue);
                }
                let net = Ipv6Net::new(addr, prefix_len).map_err(|_| SpinelError::InvalidValue)?;
                self.mesh_local_prefix = Some(net);
            }
            property::MAC_15_4_PANID => {
                self.pan_id = Some(r.read_uint16()?);
            }
            property::PHY_CHAN => {
                self.channel = Some(r.read_uint8()?);
            }
            property::NET_PSKC => {
                self.pskc = Some(value.to_vec());
            }
            property::PHY_CHAN_SUPPORTED => {
                let mut mask: u32 = 0;
                for &ch in value {
                    if ch <= 31 {
                        mask |= 1u32 << ch;
                    }
                }
                self.channel_mask_page0 = Some(mask);
            }
            property::DATASET_SECURITY_POLICY => {
                self.security_policy = Some(SecurityPolicy {
                    key_rotation_time: r.read_uint16()?,
                    flags: r.read_uint8()?,
                });
            }
            property::DATASET_RAW_TLVS => {
                self.raw_tlvs = Some(value.to_vec());
            }
            property::DATASET_DELAY_TIMER => {
                self.delay = Some(r.read_uint32()?);
            }
            property::DATASET_DEST_ADDRESS => {
                self.dest_ip_address = Some(Ipv6Addr::from(r.read_ipv6()?));
            }
            // Unknown key — skip silently (matches C behavior).
            _ => {}
        }
        Ok(())
    }

    /// Serialize the dataset to a Spinel frame payload.
    ///
    /// Wire format: `A(t(iD))`. With `include_values = false`, only the
    /// property keys are written (no values), matching the C "key-only"
    /// variant used for partial GETs.
    pub fn to_spinel_frame(&self, include_values: bool) -> Vec<u8> {
        let mut frame = PackWriter::new();

        if let Some(v) = self.active_timestamp {
            let start = frame.write_struct_start();
            frame.write_uint_packed(property::DATASET_ACTIVE_TIMESTAMP);
            if include_values {
                frame.write_uint64(v);
            }
            frame.write_struct_end(start);
        }
        if let Some(v) = self.pending_timestamp {
            let start = frame.write_struct_start();
            frame.write_uint_packed(property::DATASET_PENDING_TIMESTAMP);
            if include_values {
                frame.write_uint64(v);
            }
            frame.write_struct_end(start);
        }
        if let Some(v) = &self.master_key {
            let start = frame.write_struct_start();
            frame.write_uint_packed(property::NET_MASTER_KEY);
            if include_values {
                frame.write_bytes(v);
            }
            frame.write_struct_end(start);
        }
        if let Some(v) = &self.network_name {
            let start = frame.write_struct_start();
            frame.write_uint_packed(property::NET_NETWORK_NAME);
            if include_values {
                frame.write_utf8(v);
            }
            frame.write_struct_end(start);
        }
        if let Some(v) = &self.extended_pan_id {
            let start = frame.write_struct_start();
            frame.write_uint_packed(property::NET_XPANID);
            if include_values {
                frame.write_bytes(v);
            }
            frame.write_struct_end(start);
        }
        if let Some(v) = &self.mesh_local_prefix {
            let start = frame.write_struct_start();
            frame.write_uint_packed(property::IPV6_ML_PREFIX);
            if include_values {
                frame.write_ipv6(&v.addr().octets());
                frame.write_uint8(v.prefix_len());
            }
            frame.write_struct_end(start);
        }
        if let Some(v) = self.delay {
            let start = frame.write_struct_start();
            frame.write_uint_packed(property::DATASET_DELAY_TIMER);
            if include_values {
                frame.write_uint32(v);
            }
            frame.write_struct_end(start);
        }
        if let Some(v) = self.pan_id {
            let start = frame.write_struct_start();
            frame.write_uint_packed(property::MAC_15_4_PANID);
            if include_values {
                frame.write_uint16(v);
            }
            frame.write_struct_end(start);
        }
        if let Some(v) = self.channel {
            let start = frame.write_struct_start();
            frame.write_uint_packed(property::PHY_CHAN);
            if include_values {
                frame.write_uint8(v);
            }
            frame.write_struct_end(start);
        }
        if let Some(v) = &self.pskc {
            let start = frame.write_struct_start();
            frame.write_uint_packed(property::NET_PSKC);
            if include_values {
                frame.write_bytes(v);
            }
            frame.write_struct_end(start);
        }
        if let Some(v) = self.channel_mask_page0 {
            let start = frame.write_struct_start();
            frame.write_uint_packed(property::PHY_CHAN_SUPPORTED);
            if include_values {
                for ch in 0..32 {
                    if v & (1u32 << ch) != 0 {
                        frame.write_uint8(ch as u8);
                    }
                }
            }
            frame.write_struct_end(start);
        }
        if let Some(v) = &self.security_policy {
            let start = frame.write_struct_start();
            frame.write_uint_packed(property::DATASET_SECURITY_POLICY);
            if include_values {
                frame.write_uint16(v.key_rotation_time);
                frame.write_uint8(v.flags);
            }
            frame.write_struct_end(start);
        }
        if let Some(v) = &self.raw_tlvs {
            let start = frame.write_struct_start();
            frame.write_uint_packed(property::DATASET_RAW_TLVS);
            if include_values {
                frame.write_bytes(v);
            }
            frame.write_struct_end(start);
        }
        if let Some(v) = self.dest_ip_address {
            let start = frame.write_struct_start();
            frame.write_uint_packed(property::DATASET_DEST_ADDRESS);
            if include_values {
                frame.write_ipv6(&v.octets());
            }
            frame.write_struct_end(start);
        }

        frame.into_bytes()
    }

    /// Convert to string list (`Dataset:AllFields`), matching C formatting.
    pub fn to_string_list(&self) -> Vec<String> {
        DATASET_PROPERTY_KEYS
            .iter()
            .filter_map(|key| {
                self.field_to_string(key).map(|v| format!("{key} = {v}"))
            })
            .collect()
    }

    /// Resolve a `Dataset:*` D-Bus property key to its stringified value.
    ///
    /// Returns `None` if the field is not present in the dataset (the C
    /// code would report "property not found" for absent fields). The
    /// composite keys `Dataset:AllFields` / `Dataset:AsValMap` are handled
    /// by the caller via [`to_string_list`].
    pub fn property_string(&self, key: &str) -> Option<String> {
        self.field_to_string(key)
    }

    /// Centralized per-field formatting. Used by both `to_string_list`
    /// and `property_string` to prevent drift.
    fn field_to_string(&self, key: &str) -> Option<String> {
        match key {
            wisun_types::PROP_DATASET_ACTIVE_TIMESTAMP => self
                .active_timestamp
                .map(|v| format!("0x{:08X}{:08X}", (v >> 32) as u32, v as u32)),
            wisun_types::PROP_DATASET_PENDING_TIMESTAMP => self
                .pending_timestamp
                .map(|v| format!("0x{:08X}{:08X}", (v >> 32) as u32, v as u32)),
            wisun_types::PROP_DATASET_MASTER_KEY => self
                .master_key
                .as_ref()
                .map(|v| format!("[{}]", hex::encode(v))),
            wisun_types::PROP_DATASET_NETWORK_NAME => self.network_name.as_ref().map(|v| format!("\"{v}\"")),
            wisun_types::PROP_DATASET_EXTENDED_PAN_ID => self
                .extended_pan_id
                .as_ref()
                .map(|v| format!("0x{}", hex::encode(v))),
            wisun_types::PROP_DATASET_MESH_LOCAL_PREFIX => self
                .mesh_local_prefix
                .as_ref()
                .map(|v| format!("{}/{}", v.addr(), v.prefix_len())),
            wisun_types::PROP_DATASET_DELAY => self.delay.map(|v| v.to_string()),
            wisun_types::PROP_DATASET_PAN_ID => self.pan_id.map(|v| format!("0x{v:02X}")),
            wisun_types::PROP_DATASET_CHANNEL => self.channel.map(|v| v.to_string()),
            wisun_types::PROP_DATASET_PSKC => {
                self.pskc.as_ref().map(|v| format!("[{}]", hex::encode(v)))
            }
            wisun_types::PROP_DATASET_CHANNEL_MASK_PAGE0 => {
                self.channel_mask_page0.map(|v| format!("0x{v:08X}"))
            }
            wisun_types::PROP_DATASET_SEC_POLICY_KEY_ROTATION => self
                .security_policy
                .as_ref()
                .map(|v| v.key_rotation_time.to_string()),
            wisun_types::PROP_DATASET_SEC_POLICY_FLAGS => self
                .security_policy
                .as_ref()
                .map(|v| format!("0x{:X}", v.flags)),
            wisun_types::PROP_DATASET_RAW_TLVS => self
                .raw_tlvs
                .as_ref()
                .map(|v| format!("[{}]", hex::encode(v))),
            wisun_types::PROP_DATASET_DEST_IP_ADDRESS => {
                self.dest_ip_address.map(|v| v.to_string())
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> OperationalDataset {
        OperationalDataset {
            pan_id: Some(0xABCD),
            channel: Some(1),
            network_name: Some("TestNet".into()),
            master_key: Some(vec![0u8; 16]),
            ..Default::default()
        }
    }

    #[test]
    fn dataset_round_trip() {
        let ds = sample();
        let frame = ds.to_spinel_frame(true);
        let decoded = OperationalDataset::from_spinel_frame(&frame).unwrap();

        assert_eq!(decoded.pan_id, Some(0xABCD));
        assert_eq!(decoded.channel, Some(1));
        assert_eq!(decoded.network_name.as_deref(), Some("TestNet"));
        assert_eq!(decoded.master_key.as_deref(), Some(&vec![0u8; 16][..]));
    }

    #[test]
    fn dataset_string_list_matches_c_format() {
        let ds = sample();
        let list = ds.to_string_list();
        assert!(
            list.iter()
                .any(|s| s.contains("Dataset:PanId") && s.contains("0xABCD"))
        );
        assert!(
            list.iter()
                .any(|s| s.contains("Dataset:Channel") && s.contains("1"))
        );
    }

    #[test]
    fn channel_mask_page0_byte_array_encoding() {
        let ds = OperationalDataset {
            channel_mask_page0: Some(0b101), // channels 0 and 2
            ..Default::default()
        };

        let frame = ds.to_spinel_frame(true);
        let decoded = OperationalDataset::from_spinel_frame(&frame).unwrap();

        assert_eq!(decoded.channel_mask_page0, Some(0b101));
    }

    #[test]
    fn partial_dataset_only_present_fields_decode() {
        let ds = OperationalDataset {
            channel: Some(25),
            ..Default::default()
        };

        let frame = ds.to_spinel_frame(true);
        let decoded = OperationalDataset::from_spinel_frame(&frame).unwrap();

        assert_eq!(decoded.channel, Some(25));
        assert_eq!(decoded.pan_id, None);
        assert_eq!(decoded.network_name, None);
    }

    #[test]
    fn dataset_property_keys_match_wpan_properties() {
        let ds = sample();
        assert!(ds.property_string(wisun_types::PROP_DATASET_PAN_ID).is_some());
        assert!(ds.property_string(wisun_types::PROP_DATASET_CHANNEL).is_some());
        assert!(ds.property_string(wisun_types::PROP_DATASET_MASTER_KEY).is_some());
    }
}
