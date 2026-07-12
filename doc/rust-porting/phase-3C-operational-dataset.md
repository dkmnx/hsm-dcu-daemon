# Phase 3C: `dcu-daemon` — Operational Dataset + Vendor Extension Point

## Overview

Port the Thread/Wi-SUN operational dataset codec
(`SpinelNCPThreadDataset`) and document the vendor-custom extension
point (`SpinelNCPVendorCustom`).

**Replaces**:

- `src/ncp-spinel/SpinelNCPThreadDataset.cpp` (~713 LOC)
- `src/ncp-spinel/SpinelNCPThreadDataset.h` (~95 LOC)
- `src/ncp-spinel/SpinelNCPVendorCustom.cpp` (~139 LOC, stub)
- `src/ncp-spinel/SpinelNCPVendorCustom.h` (~74 LOC, stub)

**Total C/C++ code**: ~1,021 LOC (808 real + 213 stub)

**Effort**: 3-4 days

**Status**: Not started

## Rationale

`SpinelNCPThreadDataset` is the single largest unmentioned file in
the porting plan. It serves **14+ `Dataset:*` D-Bus properties**
(`wpan-properties.h:202-220`) that are reachable from `dcuctl get`
today. Without this codec, `dcuctl get Dataset:MasterKey` (etc.)
silently fails, and the daemon cannot persist or replay network
parameters across restarts.

`SpinelNCPVendorCustom` is the Nest extension point for TI vendor
properties. In this build it is a **no-op stub**:
`setup_property_supported_by_class()` returns `false`
(line 55), `mSupportedProperties` is never populated (line 61-63),
and only a placeholder `"__CustomKeyHere__"` is handled (lines
87-126). It is documented here as an extension point, not a blocker.

## Prerequisites (before starting this phase)

1. **Add DATASET_* constants to `crates/spinel/src/property.rs`.**
   The following constants from `spinel.h` are NOT in the spinel crate
   yet and MUST be added before implementing the dataset codec:

| Constant                            | Value    | spinel.h reference               |
| ----------------------------------- | -------- | -------------------------------- |
| `PROP_NET_XPANID`                   | `0x45`   | NET__BEGIN + 5                   |
| `PROP_NET_PSKC`                     | `0x4B`   | NET__BEGIN + 11                  |
| `PROP_DATASET_ACTIVE_TIMESTAMP`     | `0x151C` | THREAD_EXT__BEGIN + 28           |
| `PROP_DATASET_PENDING_TIMESTAMP`    | `0x151D` | THREAD_EXT__BEGIN + 29           |
| `PROP_DATASET_DELAY_TIMER`          | `0x151E` | THREAD_EXT__BEGIN + 30           |
| `PROP_DATASET_SECURITY_POLICY`      | `0x151F` | THREAD_EXT__BEGIN + 31           |
| `PROP_DATASET_RAW_TLVS`             | `0x1520` | THREAD_EXT__BEGIN + 32           |
| `PROP_DATASET_DEST_ADDRESS`         | `0x1527` | THREAD_EXT__BEGIN + 39           |

   Already in the spinel crate (import from there):
   `PROP_PHY_CHAN` (0x21), `PROP_PHY_CHAN_SUPPORTED` (0x22),
   `PROP_MAC_15_4_PANID` (0x36), `PROP_NET_NETWORK_NAME` (0x44),
   `PROP_NET_MASTER_KEY` (0x46), `PROP_IPV6_ML_PREFIX` (0x62).

1. **Add `Dataset:*` property keys to `wisun-types/src/property_key.rs`.**
   The 14 `kWPANTUNDProperty_Dataset*` string keys from
   `wpan-properties.h:202-220` must be registered in the
   `declare_property_keys!` macro invocation so
   `property_key_exists("Dataset:MasterKey")` returns true.

## Source Files to Port

| C File                                      | LOC  | What to Extract                                      |
| ------------------------------------------- | ---- | ---------------------------------------------------- |
| `src/ncp-spinel/SpinelNCPThreadDataset.cpp` | 713  | TLV codec: parse/serialize operational dataset       |
| `src/ncp-spinel/SpinelNCPThreadDataset.h`   | 95   | Dataset struct, Optional<T> template, SecurityPolicy |
| `src/ncp-spinel/SpinelNCPVendorCustom.cpp`  | 139  | Vendor extension point (no-op stub)                  |
| `src/ncp-spinel/SpinelNCPVendorCustom.h`    | 74   | VendorCustom class definition                        |

### How the dataset is used in SpinelNCPInstance.cpp

The dataset is NOT a standalone component — it is embedded in
`SpinelNCPInstance` and accessed through its property handlers.

**Loading** (NCP -> daemon):
`SpinelNCPInstance.cpp:2421` calls
`mLocalDataset.set_from_spinel_frame(data_in, data_len)`.

**Serializing** (daemon -> NCP):
`SpinelNCPInstance.cpp:2439-2470` calls
`mLocalDataset.convert_to_spinel_frame(frame, include_values)`,
then `SpinelNCPTaskSendCommand` wraps it in `CMD_PROP_VALUE_SET`.

**D-Bus property reads** (`SpinelNCPInstance.cpp:3674-3804`):
one block per field — checks `mLocalDataset.m<field>.has_value()`,
then calls `cb(kWPANTUNDStatus_Ok, boost::any(value))`.

**Dataset:AllFields / Dataset:AsValMap**
(`SpinelNCPInstance.cpp:1942-1953`):
a local `ThreadDataset dataset` is filled from a Spinel frame,
then `convert_to_valuemap(map)` / `convert_to_string_list(list)`
are called.

### D-Bus properties served

All 14 keys from `wpan-properties.h:202-220`:

| D-Bus key                          | Spinel prop ID                              | C type      | Rust type            |
| ---------------------------------- | ------------------------------------------- | ----------- | -------------------- |
| `Dataset:ActiveTimestamp`          | `SPINEL_PROP_DATASET_ACTIVE_TIMESTAMP`      | `uint64`    | `u64`                |
| `Dataset:PendingTimestamp`         | `SPINEL_PROP_DATASET_PENDING_TIMESTAMP`     | `uint64`    | `u64`                |
| `Dataset:MasterKey`                | `SPINEL_PROP_NET_MASTER_KEY`                | `Data`      | `Vec<u8>` (16 bytes) |
| `Dataset:NetworkName`              | `SPINEL_PROP_NET_NETWORK_NAME`              | `UTF-8`     | `String`             |
| `Dataset:ExtendedPanId`            | `SPINEL_PROP_NET_XPANID`                    | `Data`      | `Vec<u8>` (8 bytes)  |
| `Dataset:MeshLocalPrefix`          | `SPINEL_PROP_IPV6_ML_PREFIX`                | `IPv6+u8`   | `Ipv6Net`            |
| `Dataset:Delay`                    | `SPINEL_PROP_DATASET_DELAY_TIMER`           | `uint32`    | `u32`                |
| `Dataset:PanId`                    | `SPINEL_PROP_MAC_15_4_PANID`                | `uint16`    | `u16`                |
| `Dataset:Channel`                  | `SPINEL_PROP_PHY_CHAN`                      | `uint8`     | `u8`                 |
| `Dataset:PSKc`                     | `SPINEL_PROP_NET_PSKC`                      | `Data`      | `Vec<u8>` (16 bytes) |
| `Dataset:ChannelMaskPage0`         | `SPINEL_PROP_PHY_CHAN_SUPPORTED`            | `Data->u32` | `u32` bitmask        |
| `Dataset:SecPolicy:KeyRotation`    | `SPINEL_PROP_DATASET_SECURITY_POLICY`       | `uint16`    | `u16`                |
| `Dataset:SecPolicy:Flags`          | `SPINEL_PROP_DATASET_SECURITY_POLICY`       | `uint8`     | `u8`                 |
| `Dataset:RawTlvs`                  | `SPINEL_PROP_DATASET_RAW_TLVS`              | `Data`      | `Vec<u8>`            |
| `Dataset:DestIpAddress`            | `SPINEL_PROP_DATASET_DEST_ADDRESS`          | `IPv6`      | `Ipv6Addr`           |

Plus the composite D-Bus keys:
- `Dataset:AllFields` / `Dataset` -> `convert_to_string_list()`
- `Dataset:AllFields_AltString` -> same as above
- `Dataset:AsValMap` / `Thread:ActiveDataset:AsValMap` -> `convert_to_valuemap()`
- `Thread:PendingDataset:AsValMap` -> same (pending dataset path)

### Wire format (Spinel TLV)

Each dataset entry is a Spinel `t(iD)` struct — a `uint16 LE` length
prefix, followed by a `UINT_PACKED` property key, followed by a
`DATA_S` (length-prefixed) value. The dataset is an `A(t(iD))` — a
concatenated array of these structs with no outer length prefix
(length implied by the Spinel frame payload boundary).

`set_from_spinel_frame` (line 214-250): reads `SPINEL_DATATYPE_DATA_WLEN_S`
chunks (struct with length prefix), then `parse_dataset_entry` (line
252-480) reads each `UINT_PACKED + DATA_S` pair and dispatches on the
property key via a `switch` statement.

`convert_to_spinel_frame` (line 482-713): the reverse — for each set
field, packs a `SPINEL_DATATYPE_STRUCT_S(UINT_PACKED_S + <type>)` and
appends it to a `Data` buffer.

### `ChannelMaskPage0` encoding note

The C code encodes the channel mask as an **array of channel numbers**
(one byte per set channel), NOT a bitmask:

Parsing (line 410-424):
```c
while (value_len > 0) {
    uint8_t channel = *value_data;
    require_action(channel <= 31, bail, ret = kWPANTUNDStatus_Failure);
    channel_mask |= (1U << channel);
    value_data += sizeof(uint8_t);
    value_len -= sizeof(uint8_t);
}
mChannelMaskPage0 = channel_mask;  // stored as u32 bitmask
```

Serializing (line 646-668):
```c
for (uint8_t i = 0; i < 32; i++) {
    if (mChannelMaskPage0.get() & (1U << i)) {
        mask_data[mask_len++] = i;  // back to array of channel numbers
    }
}
```

This differs from the vendor-specific `ChannelList` in phase 1B
(`vendor.rs`) which uses a 17-byte bitmask over 129 channels. The
dataset channel mask is page 0 only (channels 0-31), encoded as
individual bytes.

## Crate Structure

This phase adds to the existing `dcu-daemon` crate (no new crate):

```text
dcu-daemon/src/
├── instance/
│   └── base.rs          # +dataset field, +dataset property handlers
├── tasks/
│   └── (existing)
├── dataset.rs            # NEW: OperationalDataset, DatasetField enum
├── vendor_ext.rs         # NEW: VendorExtension trait (extension point)
└── (existing files)
```

## Detailed File Specs

### `dataset.rs`

```rust
use std::net::Ipv6Addr;
use ipnet::Ipv6Net;
use spinel::pack::{PackWriter, PackReader};
use spinel::error::SpinelError;

/// Spinel property IDs used by the dataset codec.
///
/// Most are already defined in `spinel::property` (import from there).
/// The DATASET_* constants are NOT in the spinel crate yet — they must
/// be added to `crates/spinel/src/property.rs` as a prerequisite for
/// this phase. Values are from `third_party/openthread/src/ncp/spinel.h`.
///
/// **CRITICAL**: These values are computed from `spinel.h` base constants:
/// - PHY__BEGIN = 0x20, MAC__BEGIN = 0x30, NET__BEGIN = 0x40
/// - IPV6__BEGIN = 0x60, THREAD_EXT__BEGIN = 0x1500
///
/// Do NOT hardcode different values — the wire protocol depends on them.
///
/// **Implementation note:** For production, import the 6 existing constants
/// from `spinel::property` via `pub use`. They are defined locally below so
/// this spec is self-contained; remove the local definitions once imported.
pub mod prop {
    // --- Already in spinel::property (defined locally here for self-containedness) ---
    pub const PHY_CHAN: u32            = 0x21;  // PHY__BEGIN + 1
    pub const PHY_CHAN_SUPPORTED: u32  = 0x22;  // PHY__BEGIN + 2
    pub const MAC_15_4_PANID: u32     = 0x36;  // MAC__BEGIN + 6
    pub const NET_NETWORK_NAME: u32   = 0x44;  // NET__BEGIN + 4
    pub const NET_MASTER_KEY: u32     = 0x46;  // NET__BEGIN + 6
    pub const IPV6_ML_PREFIX: u32     = 0x62;  // IPV6__BEGIN + 2

    // --- Must be added to spinel/src/property.rs ---
    pub const NET_XPANID: u32         = 0x45;  // NET__BEGIN + 5
    pub const NET_PSKC: u32           = 0x4B;  // NET__BEGIN + 11

    // DATASET_* — all in THREAD_EXT range (0x1500+)
    // These MUST be added to spinel/src/property.rs before this phase.
    pub const DATASET_ACTIVE_TIMESTAMP: u32   = 0x151C; // THREAD_EXT + 28
    pub const DATASET_PENDING_TIMESTAMP: u32  = 0x151D; // THREAD_EXT + 29
    pub const DATASET_DELAY_TIMER: u32        = 0x151E; // THREAD_EXT + 30
    pub const DATASET_SECURITY_POLICY: u32    = 0x151F; // THREAD_EXT + 31
    pub const DATASET_RAW_TLVS: u32           = 0x1520; // THREAD_EXT + 32
    pub const DATASET_DEST_ADDRESS: u32       = 0x1527; // THREAD_EXT + 39
}

/// Security policy for the operational dataset.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SecurityPolicy {
    pub key_rotation_time: u16,
    pub flags: u8,
}

/// Operational dataset — mirrors C++ ThreadDataset.
///
/// Fields are `Option<T>` because only set fields are present in the
/// Spinel frame. Rust's built-in `Option<T>` replaces the C++
/// `ThreadDataset::Optional<T>` wrapper — no custom type is needed.
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
    /// Wire format: A(t(iD)) — concatenated length-prefixed structs.
    ///
    /// Rust idiom: associated `from_*` constructor (replaces C++
    /// `clear()` + `set_from_spinel_frame()`).
    pub fn from_spinel_frame(data: &[u8])
        -> Result<Self, SpinelError>
    {
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

    fn parse_entry(&mut self, key: u32, value: &[u8])
        -> Result<(), SpinelError>
    {
        let mut r = PackReader::new(value);
        match key {
            prop::DATASET_ACTIVE_TIMESTAMP => {
                self.active_timestamp = Some(r.read_uint64()?);
            }
            prop::NET_MASTER_KEY => {
                self.master_key = Some(value.to_vec());
            }
            prop::NET_NETWORK_NAME => {
                self.network_name = Some(r.read_utf8()?);
            }
            prop::NET_XPANID => {
                self.extended_pan_id = Some(value.to_vec());
            }
            prop::IPV6_ML_PREFIX => {
                let addr = Ipv6Addr::from(r.read_ipv6()?);
                let prefix_len = r.read_uint8()?;
                let net = Ipv6Net::new(addr, prefix_len)?;
                self.mesh_local_prefix = Some(net);
            }
            prop::MAC_15_4_PANID => {
                self.pan_id = Some(r.read_uint16()?);
            }
            prop::PHY_CHAN => {
                self.channel = Some(r.read_uint8()?);
            }
            prop::NET_PSKC => {
                self.pskc = Some(value.to_vec());
            }
            prop::PHY_CHAN_SUPPORTED => {
                let mut mask: u32 = 0;
                for &ch in value {
                    if ch <= 31 { mask |= 1u32 << ch; }
                }
                self.channel_mask_page0 = Some(mask);
            }
            prop::DATASET_SECURITY_POLICY => {
                self.security_policy = Some(SecurityPolicy {
                    key_rotation_time: r.read_uint16()?,
                    flags: r.read_uint8()?,
                });
            }
            prop::DATASET_RAW_TLVS => {
                self.raw_tlvs = Some(value.to_vec());
            }
            prop::DATASET_DELAY_TIMER => {
                self.delay = Some(r.read_uint32()?);
            }
            prop::DATASET_DEST_ADDRESS => {
                self.dest_ip_address =
                    Some(Ipv6Addr::from(r.read_ipv6()?));
            }
            _ => { /* unknown key — log and skip */ }
        }
        Ok(())
    }

    /// Serialize the dataset to a Spinel frame payload.
    /// Wire format: A(t(iD)).
    ///
    /// Rust idiom: `to_*` conversion method (replaces C++
    /// `convert_to_spinel_frame()`). Use `include_values = false`
    /// to write only the property keys (no values).
    pub fn to_spinel_frame(&self, include_values: bool)
        -> Vec<u8>
    {
        let mut frame = PackWriter::new();
        // For each Some(field), write a struct entry:
        //   let start = frame.write_struct_start();
        //   frame.write_uint_packed(prop_key);
        //   if include_values { write field value }
        //   frame.write_struct_end(start);
        frame.into_bytes()
    }

    /// Convert to D-Bus ValueMap (Dataset:AsValMap).
    pub fn to_valuemap(&self) -> HashMap<String, Variant> {
        let mut map = HashMap::new();
        // One entry per Some(field)
        map
    }

    /// Convert to string list (Dataset:AllFields).
    pub fn to_string_list(&self) -> Vec<String> {
        let mut list = Vec::new();
        // One entry per Some(field), matching C formatting:
        //   "%-32s =  0x%08X%08X" for timestamps
        //   "%-32s =  [%s]" for byte arrays
        //   "%-32s =  %d" for integers
        //   "%-32s =  %s/64" for mesh-local prefix
        list
    }
}
```

### `vendor_ext.rs`

Document the extension point. In the C code, `SpinelNCPVendorCustom`
is the class `SpinelNCPInstance` delegates unknown property keys to.
In this build it is a no-op stub (returns
`kWPANTUNDStatus_FeatureNotSupported` for all keys).

```rust
/// Vendor extension point for future TI property handlers.
///
/// In the C codebase, `SpinelNCPVendorCustom` is the class that
/// `SpinelNCPInstance` delegates unknown vendor property keys to.
/// In this build it is a no-op stub (empty `mSupportedProperties`,
/// only placeholder `"__CustomKeyHere__"` handled).
pub trait VendorExtension {
    fn is_property_key_supported(&self, key: &str) -> bool;
    fn property_get_value(&self, key: &str)
        -> Result<String, VendorError>;
    fn property_set_value(&self, key: &str, value: &str)
        -> Result<(), VendorError>;
    fn property_insert_value(&self, key: &str, value: &str)
        -> Result<(), VendorError>;
    fn property_remove_value(&self, key: &str, value: &str)
        -> Result<(), VendorError>;
    fn get_ms_to_next_event(&self) -> Option<std::time::Duration>;
    fn process(&mut self);
}

/// Default no-op implementation (matches C SpinelNCPVendorCustom).
pub struct NoOpVendorExtension;

impl VendorExtension for NoOpVendorExtension {
    fn is_property_key_supported(&self, _: &str) -> bool { false }
    fn property_get_value(&self, key: &str)
        -> Result<String, VendorError>
    {
        Err(VendorError::FeatureNotSupported(key.into()))
    }
    // ... all methods return FeatureNotSupported / no-op
}
```

### `instance/base.rs` integration

The `NcpInstanceBase` holds the dataset and routes `Dataset:*`
property reads.

```rust
pub struct NcpInstanceBase {
    // ... existing fields ...
    dataset: OperationalDataset,
    vendor_ext: Box<dyn VendorExtension>,
}
```

In the property handler dispatch, `Dataset:*` keys are matched
before the generic property lookup:

```rust
// Inside handle_get_property or the properties module:
match name {
    "Dataset:ActiveTimestamp" if self.dataset.active_timestamp.is_some() =>
        Ok(format!("0x{:08X}{:08X}", ...)),
    "Dataset:MasterKey" if self.dataset.master_key.is_some() =>
        Ok(format!("[{}]",
            hex::encode(self.dataset.master_key.as_ref().unwrap()))),
    "Dataset:AllFields" | "Dataset" =>
        Ok(self.dataset.to_string_list().join("\n")),
    "Dataset:AsValMap" | "Thread:ActiveDataset:AsValMap" =>
        Ok(serde_json::to_string(&self.dataset.to_valuemap())?),
    _ if self.vendor_ext.is_property_key_supported(name) =>
        self.vendor_ext.property_get_value(name)?,
    _ => Err(DaemonError::UnknownProperty(name.into())),
}
```

The `Dataset:*` properties are **read-only** from D-Bus. The dataset
is updated only by Spinel frames arriving from the NCP (during
`PROP_VALUE_IS` unsolicited frames or as part of `GET` responses).
There is no `PropSet` handler for `Dataset:*` keys — the C code does
not allow setting individual dataset fields through D-Bus.

## Tests

### Test 1: Dataset TLV round-trip

```rust
#[test]
fn dataset_round_trip() {
    let mut ds = OperationalDataset::default();
    ds.pan_id = Some(0xABCD);
    ds.channel = Some(1);
    ds.network_name = Some("TestNet".into());
    ds.master_key = Some(vec![0u8; 16]);

    let frame = ds.to_spinel_frame(true);
    let decoded = OperationalDataset::from_spinel_frame(&frame).unwrap();

    assert_eq!(decoded.pan_id, Some(0xABCD));
    assert_eq!(decoded.channel, Some(1));
    assert_eq!(decoded.network_name.as_deref(), Some("TestNet"));
    assert_eq!(decoded.master_key.as_deref(), Some(&vec![0u8; 16][..]));
}
```

### Test 2: Dataset string list format matches C

```rust
#[test]
fn dataset_string_list_matches_c_format() {
    let mut ds = OperationalDataset::default();
    ds.pan_id = Some(0xABCD);
    ds.channel = Some(1);
    ds.network_name = Some("TestNet".into());

    let list = ds.to_string_list();
    assert!(list.iter().any(|s|
        s.contains("Dataset:PanId") && s.contains("0xABCD")
    ));
}
```

### Test 3: Channel mask page 0 byte-array encoding

```rust
#[test]
fn channel_mask_page0_byte_array_encoding() {
    let mut ds = OperationalDataset::default();
    ds.channel_mask_page0 = Some(0b101);  // channels 0 and 2

    let frame = ds.to_spinel_frame(true);
    let decoded = OperationalDataset::from_spinel_frame(&frame).unwrap();

    assert_eq!(decoded.channel_mask_page0, Some(0b101));
}
```

### Test 4: Partial dataset (not all fields present)

```rust
#[test]
fn partial_dataset_only_present_fields_decode() {
    let mut ds = OperationalDataset::default();
    ds.channel = Some(25);

    let frame = ds.to_spinel_frame(true);
    let decoded = OperationalDataset::from_spinel_frame(&frame).unwrap();

    assert_eq!(decoded.channel, Some(25));
    assert_eq!(decoded.pan_id, None);
    assert_eq!(decoded.network_name, None);
}
```

### Test 5: Dataset ValMap matches D-Bus property keys

```rust
#[test]
fn dataset_valmap_keys_match_wpan_properties() {
    let mut ds = OperationalDataset::default();
    ds.pan_id = Some(0xABCD);
    ds.channel = Some(1);

    let map = ds.to_valuemap();
    assert!(map.contains_key("Dataset:PanId"));
    assert!(map.contains_key("Dataset:Channel"));
}
```

### Test 6: NoOp vendor extension

```rust
#[test]
fn noop_vendor_extension_returns_not_supported() {
    let ext = NoOpVendorExtension;
    assert!(!ext.is_property_key_supported("NCP:Region"));
    assert!(ext.property_get_value("NCP:Region").is_err());
    assert_eq!(ext.get_ms_to_next_event(), None);
}
```

## Verification Checklist

- [ ] Every `SPINEL_PROP_DATASET_*` constant is defined in `dataset.rs::prop`
- [ ] Every `kWPANTUNDProperty_Dataset*` maps to a D-Bus property key string
- [ ] `from_spinel_frame` / `to_spinel_frame` round-trip for all 14 fields
- [ ] `ChannelMaskPage0` byte-array encoding matches C (not bitmask)
- [ ] `to_string_list` format matches C exactly (`%-32s =  value`)
- [ ] `to_valuemap` keys match `wpan-properties.h:202-220`
- [ ] Partial datasets (not all fields present) decode correctly
- [ ] `NoOpVendorExtension` is documented as the extension point stub
- [ ] `cargo test` passes (6+ tests)
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` produces zero warnings
- [ ] No `unsafe` code in this module

## Cross-references

This phase complements the existing porting docs:

| Related phase | Connection                                                                                                                                      |
| ------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| Phase 2A      | **Must amend**: add `Dataset:*` (14 keys) to the 29-key property table; the `properties.rs` dispatch must handle `Dataset:ActiveTimestamp` etc. |
| Phase 3B      | `NcpInstanceBase` gains the `dataset` field; form/join tasks may populate it from NCP responses.                                                |
| Phase 4A      | Mock NCP must respond to `CMD_PROP_VALUE_GET` for dataset-related Spinel properties (master key, channel, PAN ID, etc.)                         |

## Out of scope

- **Thread network steering / Commissioner** — `SpinelNCPThreadDataset`
  only holds the local dataset. Commissioner/Joiner flows are a separate
  concern (`SpinelNCPVendorCustom` stub in this build).
- **Dataset persistence to disk** — `NetworkRetain` (phase-3A, deferred)
  handles persistent config; the dataset is held in memory only here.
- **`Thread:PendingDataset` write path** — The C code does not expose
  individual dataset field writes via D-Bus. If needed in the future,
  add a `PropSet` handler for `Thread:PendingDataset:AsValMap`.
