# Phase 1A: `wisun-types` — Constants and Enumerations

## Overview

Create the foundational type library that all other crates depend on.
Maps every C `#define` and enum to Rust types.

**Replaces**:

- `src/dcud/NCPConstants.h`
- `src/dcud/wpan-properties.h`
- `src/dcud/wpan-error.h`
- `src/dcud/NCPTypes.*`
- `src/ncp-spinel/wisun_config.h`

**Effort**: 2-3 days

**Status**: COMPLETE

## Source Files to Port

| C File                          | LOC  | What to Extract         |
| ------------------------------- | ---- | ----------------------- |
| `src/dcud/NCPConstants.h`       | ~200 | NCP state, command IDs  |
| `src/dcud/wpan-properties.h`    | ~300 | Property key constants  |
| `src/dcud/wpan-error.h`         | ~80  | Error code enum         |
| `src/dcud/NCPTypes.h`           | ~60  | Type aliases            |
| `src/dcud/NCPTypes.cpp`         | ~20  | ToString helpers        |
| `src/ncp-spinel/wisun_config.h` | ~150 | Wi-SUN config constants |

**Total C code**: ~810 LOC

## Crate Structure

```text
wisun-types/
├── .gitignore          # /target/, /Cargo.lock
├── Cargo.toml          # edition 2024, rust-version 1.85
└── src/
    ├── lib.rs          # Re-exports all public types
    ├── ncp_state.rs    # NcpState enum
    ├── property_key.rs # declare_property_keys! macro
    ├── error.rs        # WpanError enum + NCP error consts
    ├── network_config.rs # NetworkName, PanId, etc.
    ├── constants.rs    # Timeouts, buffer sizes
    └── command.rs      # Spinel command IDs
```

## Detailed File Specs

### `ncp_state.rs`

Map the internal NCP states from
`src/dcud/NCPTypes.h:34-46` and the D-Bus state strings
from `doc/wpan-dbus-protocol.md`.

**Rust enum matching C `nl::wpantund::NCPState`**
(NCPTypes.h:34-46):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum NcpState {
    Uninitialized = 0,
    Fault = 1,
    Upgrading = 2,
    DeepSleep = 3,
    Offline = 4,
    Commissioned = 5,
    Associating = 6,
    CredentialsNeeded = 7,
    Associated = 8,
    Isolated = 9,
    NetWakeWaking = 10,
    NetWakeAsleep = 11,
}
```

Implements:

- `FromStr` — parse D-Bus string (e.g.,
  `"offline:commissioned"`)
- `Display` — produce D-Bus string
- `TryFrom<u32>` — C integer values
- `Error` for `ParseNcpStateError`
- `is_associated(&self)`, `is_offline(&self)`,
  `is_fault(&self)`

### `property_key.rs`

Maps every property key from `wpan-properties.h` and TI
vendor extensions using the `declare_property_keys!`
macro. The macro generates both the `pub const` and the
`ALL_PROPERTY_KEYS` static array in a single invocation
to prevent drift.

Two groups:

1. **String constants** — D-Bus property key strings
   (e.g., `PROP_NCP_PROTOCOL_VERSION = "NCP:..."`)
2. **Spinel numeric property IDs** — packed integer
   identifiers (e.g., `SPINEL_PROP_LAST_STATUS = 0`)

`property_key_exists(name)` performs **case-sensitive**
exact match.

### `error.rs`

Maps every `WPANTUND_STATUS_*` from `wpan-error.h`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum WpanError {
    Ok = 0,
    Failure = 1,
    InvalidArgument = 2,
    // ... through JoinerFailedUnknown = 32
    Reserved33..Reserved39 = 33..39,
    NcpError = 0xEA0000,
}
```

Standalone NCP error constants:

- `NCP_ERROR_BASE: i32 = 0xEA0000`
- `NCP_ERROR_END: i32 = 0xEAFFFF`
- `NCP_ERROR_MASK: i32 = 0xFFFF`

Implements:

- `From<i32>` — maps 0-39 explicitly, 0xEA0000+ to
  `NcpError` (lossy for sub-codes)
- `Display` for human-readable messages
- `is_success()`, `is_error()`, `raw_code()`
- `is_ncp_error(code)` static method

### `constants.rs`

```rust
pub const DEFAULT_BAUD_RATE: u32 = 115200;
pub const HIGH_THROUGHPUT_BAUD_RATE: u32 = 460800;
pub const DEFAULT_INTERFACE_NAME: &str = "wfan0";
pub const NCP_DEFAULT_COMMAND_RESPONSE_TIMEOUT_MS: u64 = 5000;
pub const NCP_DEEP_SLEEP_TICKLE_TIMEOUT_MS: u64 = 4_200_000;
pub const NCP_FORM_TIMEOUT_S: u64 = 60;
pub const NCP_JOIN_TIMEOUT_S: u64 = 60;
pub const MAX_CHANNEL_MASK_SIZE: usize = 17;
// Ch0 center freq: _MHZ_PART and _KHZ_PART
pub const WISUN_DEFAULT_CH0_CENTER_FREQ_MHZ_PART: u32 = 902;
pub const WISUN_DEFAULT_CH0_CENTER_FREQ_KHZ_PART: u32 = 200;
```

### `network_config.rs`

```rust
pub struct NetworkName(pub String);
pub struct PanId(pub u16);
pub struct XPanId(pub u64);
pub struct ChannelMask(pub [u8; 17]); // 129 channels
pub struct Eui64(pub [u8; 8]);
pub struct Ipv6Address(pub [u8; 16]);
```

Each type implements:

- `FromStr` — parse dcuctl output format
- `Display` — produce dcuctl output format
- `TryFrom<&[u8]>` — decode from Spinel binary

Additional features:

- `Eui64` accepts both bare hex and colon-separated
  (`00:12:4B:00:14:F7:D2:E6`)
- `ChannelMask::set_channel()` has `debug_assert!`
  for channel > 128
- `Ipv6Address` has `From<Ipv6Addr>` conversions
- `PanId::DEFAULT = PanId(0xABCD)`

## Tests

41 tests total. All pass, clippy clean, fmt clean.

### Test 1: NCP State Round-Trip

```rust
#[test]
fn ncp_state_from_c_enum() {
    let c_to_rust = vec![
        (0, NcpState::Uninitialized),
        (1, NcpState::Fault),
        (2, NcpState::Upgrading),
        (3, NcpState::DeepSleep),
        (4, NcpState::Offline),
        (5, NcpState::Commissioned),
        (6, NcpState::Associating),
        (7, NcpState::CredentialsNeeded),
        (8, NcpState::Associated),
        (9, NcpState::Isolated),
        (10, NcpState::NetWakeWaking),
        (11, NcpState::NetWakeAsleep),
    ];
    for (c_val, rust_state) in c_to_rust {
        assert_eq!(rust_state as u32, c_val);
    }
}

#[test]
fn ncp_state_dbus_string_round_trip() {
    let states = vec![
        ("uninitialized", NcpState::Uninitialized),
        ("uninitialized:fault", NcpState::Fault),
        ("uninitialized:upgrading", NcpState::Upgrading),
        ("offline:deep-sleep", NcpState::DeepSleep),
        ("offline", NcpState::Offline),
        ("offline:commissioned", NcpState::Commissioned),
        ("associating", NcpState::Associating),
        ("associating:credentials-needed",
            NcpState::CredentialsNeeded),
        ("associated", NcpState::Associated),
        ("associated:no-parent", NcpState::Isolated),
        ("associated:netwake-waking",
            NcpState::NetWakeWaking),
        ("associated:netwake-asleep",
            NcpState::NetWakeAsleep),
    ];
    for (s, expected) in states {
        let parsed: NcpState = s.parse().unwrap();
        assert_eq!(parsed, expected);
        assert_eq!(parsed.to_string(), s);
    }
}
```

### Test 2: Property Key Coverage

```rust
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
    ];
    for name in required {
        assert!(property_key_exists(name));
    }
}
```

### Test 3: Error Code Completeness

```rust
#[test]
fn error_code_count_matches_c() {
    assert_eq!(WpanError::VARIANTS.len(), 40);
}
```

### Test 4: Error Code Bidirectional Mapping

```rust
#[test]
fn error_code_c_to_rust_round_trip() {
    for code in 0..40 {
        let err = WpanError::from(code);
        let back: i32 = err.into();
        assert_eq!(back, code);
    }
}
```

### Test 5: EUI-64 Parsing

```rust
#[test]
fn eui64_from_hex_string() {
    let eui: Eui64 = "00124B0014F7D2E6".parse().unwrap();
    assert_eq!(eui.0,
        [0x00, 0x12, 0x4B, 0x00, 0x14, 0xF7, 0xD2, 0xE6]);
    assert_eq!(eui.to_string(), "00124B0014F7D2E6");
}

#[test]
fn eui64_from_colon_separated() {
    let eui: Eui64 = "00:12:4B:00:14:F7:D2:E6"
        .parse().unwrap();
    assert_eq!(eui.0,
        [0x00, 0x12, 0x4B, 0x00, 0x14, 0xF7, 0xD2, 0xE6]);
}
```

### Test 6: Channel Mask Operations

```rust
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
        "01:00:00:00:00:00:00:00:00:00:00:00:\
         00:00:00:00:01"
    );
}
```

## Verification Checklist

- [x] Every `WPANTUND_STATUS_*` has a Rust variant
- [x] Every property in `ti_wisun_commands.md` has
  a constant
- [x] Every NCP state has a Rust variant
- [x] All numeric constants match C `#define` values
- [x] `cargo test` passes (41 tests)
- [x] `cargo clippy` produces zero warnings
- [x] No `unsafe` code in this crate
- [x] `cargo fmt --check` clean
- [x] `[lints]` deny all warnings in Cargo.toml
- [x] `.gitignore` excludes `target/` and `Cargo.lock`
