# Phase 1A: `wisun-types` — Constants and Enumerations

## Overview

Create the foundational type library that all other crates depend on. Maps every C `#define` and enum to Rust types.

**Replaces**: `src/dcud/NCPConstants.h`, `src/dcud/wpan-properties.h`, `src/dcud/wpan-error.h`, `src/dcud/NCPTypes.*`, `src/ncp-spinel/wisun_config.h`

**Effort**: 2-3 days

## Source Files to Port

| C File                          | LOC  | What to Extract                               |
| ------------------------------- | ---- | --------------------------------------------- |
| `src/dcud/NCPConstants.h`       | ~200 | NCP state enum, command IDs, property keys    |
| `src/dcud/wpan-properties.h`    | ~300 | All `WPANTUND_*` property key constants       |
| `src/dcud/wpan-error.h`         | ~80  | Error code enum (`WPANTUND_STATUS_*`)         |
| `src/dcud/NCPTypes.h`           | ~60  | Type aliases (`CallbackWithStatusArg1`, etc.) |
| `src/dcud/NCPTypes.cpp`         | ~20  | ToString helpers                              |
| `src/ncp-spinel/wisun_config.h` | ~150 | Wi-SUN specific config constants              |

**Total C code**: ~810 LOC

## Crate Structure

```text
wisun-types/
├── Cargo.toml
└── src/
    ├── lib.rs               # Re-exports all public types
    ├── ncp_state.rs         # NcpState enum
    ├── property_key.rs      # Spinel property key constants
    ├── error.rs             # WpanError enum
    ├── network_config.rs    # NetworkName, PANID, XPANID, etc.
    ├── constants.rs         # Version strings, timeouts, buffer sizes
    └── command.rs           # Spinel command IDs
```

## Detailed File Specs

### `ncp_state.rs`

Map the internal NCP states from `src/dcud/NCPTypes.h:34-46` and the D-Bus state strings from `doc/wpan-dbus-protocol.md` (compatibly named to match the D-Bus wire format).

**Rust enum matching C `nl::wpantund::NCPState`** (NCPTypes.h:34-46):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum NcpState {
    Uninitialized,          // UNINITIALIZED
    Fault,                  // FAULT
    Upgrading,              // UPGRADING
    DeepSleep,              // DEEP_SLEEP
    Offline,                // OFFLINE
    Commissioned,           // COMMISSIONED
    Associating,            // ASSOCIATING
    CredentialsNeeded,      // CREDENTIALS_NEEDED
    Associated,             // ASSOCIATED
    Isolated,               // ISOLATED
    NetWakeWaking,          // NET_WAKE_WAKING
    NetWakeAsleep,          // NET_WAKE_ASLEEP
}
```

Must implement:
- `FromStr` — parse from D-Bus string (e.g., `"offline:commissioned"`)
- `Display` — produce D-Bus string (e.g., `"associated"`)
- Mapping from C `NCPState` integer values:
  - `Uninitialized` = 0, `Fault` = 1, `Upgrading` = 2, `DeepSleep` = 3, `Offline` = 4, `Commissioned` = 5, `Associating` = 6, `CredentialsNeeded` = 7, `Associated` = 8, `Isolated` = 9, `NetWakeWaking` = 10, `NetWakeAsleep` = 11
- Helper methods: `is_associated()`, `is_offline()`, `is_fault()`

### `property_key.rs`

Maps every property key from `wpan-properties.h` and TI vendor extensions:

```rust
// Standard Spinel properties
pub const PROP_LAST_STATUS: u32 = 0;
pub const PROP_PROTOCOL_VERSION: u32 = 1;
pub const PROP_NCP_VERSION: u32 = 2;
pub const PROP_INTERFACE_TYPE: u32 = 3;
// ... ~50 more standard props

// TI Wi-SUN vendor properties (from wisun_config.h)
pub const PROP_WISUN_UNICAST_CHANNEL_LIST: u32 = /* vendor-specific */;
pub const PROP_WISUN_BROADCAST_CHANNEL_LIST: u32 = /* vendor-specific */;
// ... all vendor props
```

Must include every property referenced in `ti_wisun_commands.md` (40+ properties).

### `error.rs`

Maps every `WPANTUND_STATUS_*` from `wpan-error.h`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum WpanError {
    Success = 0,
    InvalidArgument = 1,
    ChannelError = 2,
    // ... all ~40 status codes
}
```

Must implement:
- `From<i32>` for conversion from Spinel `LAST_STATUS` values
- `Display` for human-readable messages
- `is_success()`, `is_error()` helpers

### `constants.rs`

```rust
pub const DEFAULT_BAUD_RATE: u32 = 115200;
pub const HIGH_THROUGHPUT_BAUD_RATE: u32 = 460800; // src/util/socket-utils.c:699, README.md:217
pub const DEFAULT_INTERFACE_NAME: &str = "wfan0";
pub const DEFAULT_IPV6_PREFIX: &str = "2020:ABCD::/64";
pub const NCP_DEFAULT_COMMAND_RESPONSE_TIMEOUT_MS: u64 = 5000;   // NCPConstants.h:25 = 5s
pub const NCP_DEEP_SLEEP_TICKLE_TIMEOUT_MS: u64 = 4_200_000;      // NCPConstants.h:28 = 60*70 = 4200s
pub const NCP_FORM_TIMEOUT_S: u64 = 60;                            // NCPConstants.h:29
pub const NCP_JOIN_TIMEOUT_S: u64 = 60;                            // NCPConstants.h:30
pub const MAX_CHANNEL_MASK_SIZE: usize = 17; // 129 channels max (Wi-SUN FAN PHY). 129/8 = 16.125 → 17 bytes. See NCPInstanceBase.cpp:2052 comment.
```

### `network_config.rs`

```rust
pub struct NetworkName(pub String);
pub struct PanId(pub u16);
pub struct XPanId(pub u64);
pub struct ChannelMask(pub [u8; 17]); // 129 channels, bitfield
pub struct Eui64(pub [u8; 8]);
pub struct IPv6Address(pub [u8; 16]);
```

Each type must implement:
- `FromStr` (parse from dcuctl output format)
- `Display` (produce dcuctl output format)
- `TryFrom<&[u8]>` (decode from Spinel binary)

## Tests

### Test 1: NCP State Round-Trip

```rust
#[test]
fn ncp_state_from_c_enum() {
    // C enum values from NCPTypes.h:34-46
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
        ("associating:credentials-needed", NcpState::CredentialsNeeded),
        ("associated", NcpState::Associated),
        ("associated:no-parent", NcpState::Isolated),
        ("associated:netwake-waking", NcpState::NetWakeWaking),
        ("associated:netwake-sleeping", NcpState::NetWakeAsleep),
    ];
    for (s, expected) in states {
        let parsed: NcpState = s.parse().unwrap();
        assert_eq!(parsed, expected, "Failed to parse {s}");
        assert_eq!(parsed.to_string(), s);
    }
}
```

### Test 2: Property Key Coverage

```rust
#[test]
fn all_ti_properties_defined() {
    // Extract property names from ti_wisun_commands.md
    // Verify each one has a corresponding constant
    let required = vec![
        "NCP:ProtocolVersion",
        "NCP:Version",
        "NCP:InterfaceType",
        "NCP:HardwareAddress",
        "NCP:CCAThreshold",
        "NCP:Region",
        "NCP:ModeID",
        "unicastchlist",
        "broadcastchlist",
        "asyncchlist",
        "chspacing",
        "ch0centerfreq",
        "Network:panid",
        "bcdwellinterval",
        "ucdwellinterval",
        "bcinterval",
        "ucchfunction",
        "bcchfunction",
        "macfilterlist",
        "macfiltermode",
        "Interface:Up",
        "Stack:Up",
        "Network:NodeType",
        // ... all 40+ from ti_wisun_commands.md
    ];
    for name in required {
        assert!(
            property_key_exists(name),
            "Missing property key: {name}"
        );
    }
}
```

### Test 3: Error Code Completeness

```rust
#[test]
fn error_code_count_matches_c() {
    // C wpan-error.h defines ~40 status codes
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
    assert_eq!(eui.0, [0x00, 0x12, 0x4B, 0x00, 0x14, 0xF7, 0xD2, 0xE6]);
    assert_eq!(eui.to_string(), "00124B0014F7D2E6");
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
    assert_eq!(mask.to_hex_string(), "ff:ff:ff:ff:ff:ff:ff:ff:ff:ff:ff:ff:ff:ff:ff:ff:01");
}
```

## Verification Checklist

- [ ] Every `WPANTUND_STATUS_*` in `wpan-error.h` has a Rust variant
- [ ] Every property in `ti_wisun_commands.md` has a constant
- [ ] Every NCP state in `wpan-dbus-protocol.md` has a Rust variant
- [ ] All numeric constants match C `#define` values (binary comparison)
- [ ] `cargo test` passes
- [ ] `cargo clippy` produces zero warnings
- [ ] No `unsafe` code in this crate
