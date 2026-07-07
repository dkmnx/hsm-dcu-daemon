# Phase 1B: `spinel` — Spinel Protocol Library

## Overview

Implement the Spinel binary protocol: frame construction, encoding/decoding, HDLC framing. This is the wire format that talks to the TI NCP hardware.

**Replaces**: `third_party/openthread/` spinel headers, `src/ncp-spinel/spinel-extra.*`, `SpinelNCPTask::SpinelPackData`, `SpinelAppendAny`

**Effort**: 5-7 days

## Source Files to Port

| C/H File                                            | LOC   | What to Extract                                           |
| --------------------------------------------------- | ----- | --------------------------------------------------------- |
| `src/ncp-spinel/spinel-extra.h`                     | ~150  | Spinel pack/unpack function declarations                  |
| `src/ncp-spinel/spinel-extra.c`                     | ~800  | Spinel pack/unpack implementations                        |
| `third_party/openthread/src/spinel/spinel.h`        | ~3000 | Property keys, command IDs, data types                    |
| `third_party/openthread/src/spinel/platform-hdlc.h` | ~80   | HDLC constants                                            |
| `src/ncp-spinel/SpinelNCPInstance-DataPump.cpp`     | ~400  | HDLC framing, CRC-16, escape state machine (lines 65-274) |
| `src/util/Data.cpp`                                 | ~200  | Buffer/data container                                     |

**Total C code**: ~4,630 LOC

**IMPORTANT**: The HDLC codec lives in `SpinelNCPInstance-DataPump.cpp`, NOT in `SocketWrapper.cpp`.

## Crate Structure

```text
spinel/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── frame.rs            # SpinelFrame struct, header parsing
    ├── pack.rs             # Pack format encoder/decoder (LEB128, all types)
    ├── hdlc.rs             # HDLC framing (flag, escape, CRC-16)
    ├── property.rs         # Typed property encode/decode
    ├── command.rs          # Spinel command IDs
    └── vendor.rs           # TI vendor-specific commands
```

## Detailed File Specs

### `frame.rs`

```rust
#[derive(Debug, Clone)]
pub struct SpinelFrame {
    pub header: u8,
    pub command_id: u32,
    pub payload: Vec<u8>,
}

impl SpinelFrame {
    pub fn new(command_id: u32, payload: Vec<u8>) -> Self;

    /// Encode frame to bytes WITHOUT HDLC framing.
    /// Wire format: UINT8(header) + UINT_PACKED(command_id) + payload
    pub fn encode(&self) -> Vec<u8>;

    /// Decode frame from bytes (without HDLC framing).
    pub fn decode(data: &[u8]) -> Result<Self, SpinelError>;

    /// Get IID (Interface ID) from header.
    pub fn iid(&self) -> u8 {
        (self.header & SPINEL_HEADER_IID_MASK) >> SPINEL_HEADER_IID_SHIFT
    }

    /// Get TID (Transaction ID) from header. 0 = no response expected.
    pub fn tid(&self) -> u8 {
        self.header & SPINEL_HEADER_TID_MASK
    }

    /// Check if header has FLAG bit set (bit 7).
    pub fn has_flag(&self) -> bool {
        self.header & SPINEL_HEADER_FLAG != 0
    }
}
```

Header byte layout (from `spinel.h:4479-4493`):

```text
Bit 7:   FLAG (0x80) — always set
Bits 5-4: IID (Interface ID, 0-3)
Bits 3-0: TID (Transaction ID, 0-15; 0 = unsolicited/response)
```

Command and property keys are UINT_PACKED (LEB128), NOT fixed-width.

### `pack.rs`

Reimplements Spinel pack format from `spinel-extra.c` and `spinel.h:4507-4553`.

The Spinel wire format uses these data types (pack format strings):

```text
"b" = bool           — single byte (0 or 1)
"C" = uint8          — 1 byte
"c" = int8           — 1 byte
"S" = uint16         — 2 bytes, little-endian
"s" = int16          — 2 bytes, little-endian
"L" = uint32         — 4 bytes, little-endian
"l" = int32          — 4 bytes, little-endian
"X" = uint64         — 8 bytes, little-endian
"x" = int64          — 8 bytes, little-endian
"i" = uint packed    — LEB128 variable-length (max 3 bytes, max value 2097151)
"6" = IPv6 address   — 16 bytes, big-endian
"E" = EUI-64         — 8 bytes, big-endian
"e" = EUI-48         — 6 bytes, big-endian
"D" = data           — raw bytes, no length prefix
"d" = data w/ length — uint16 (LE) length prefix + raw bytes
"U" = UTF-8 string   — NUL-terminated (NOT length-prefixed)
"t(...)" = struct    — uint16 (LE) length prefix + typed fields (like "d" but with known schema)
"A(...)" = array     — concatenated elements, NO length prefix, length inferred from outer frame
```

**Critical**: The command frame itself is `UINT8(header) + UINT_PACKED(command_id)` — command IDs are LEB128. Property keys are also UINT_PACKED.

#### Packed Unsigned Integer (LEB128)

From `spinel.h:180-199`: values < 127 encode as 1 byte. Larger values use 7-bit chunks with continuation bit, up to 3 bytes (max 2,097,151).

```rust
pub struct PackWriter {
    buf: Vec<u8>,
}

impl PackWriter {
    pub fn new() -> Self;

    // Packed unsigned integer (LEB128) — the dominant integer type
    pub fn write_uint_packed(&mut self, val: u32) {
        assert!(val <= SPINEL_MAX_UINT_PACKED);
        if val < 0x80 {
            self.buf.push(val as u8);
        } else if val < 0x4000 {
            self.buf.push(0x80 | (val & 0x7F) as u8);
            self.buf.push((val >> 7) as u8);
        } else {
            self.buf.push(0x80 | (val & 0x7F) as u8);
            self.buf.push(0x80 | ((val >> 7) & 0x7F) as u8);
            self.buf.push((val >> 14) as u8);
        }
    }

    // Fixed-width integers (little-endian)
    pub fn write_uint8(&mut self, val: u8);
    pub fn write_int8(&mut self, val: i8);
    pub fn write_uint16(&mut self, val: u16);
    pub fn write_int16(&mut self, val: i16);
    pub fn write_uint32(&mut self, val: u32);
    pub fn write_int32(&mut self, val: i32);
    pub fn write_uint64(&mut self, val: u64);
    pub fn write_int64(&mut self, val: i64);

    // Bool
    pub fn write_bool(&mut self, val: bool);

    // Addresses (big-endian, as per spinel.h)
    pub fn write_ipv6(&mut self, addr: &[u8; 16]);
    pub fn write_eui64(&mut self, addr: &[u8; 8]);

    // Data
    pub fn write_bytes(&mut self, data: &[u8]);          // "D" — no length prefix
    pub fn write_data_with_len(&mut self, data: &[u8]);  // "d" — uint16 LE length prefix

    // String (NUL-terminated, NOT length-prefixed)
    pub fn write_utf8(&mut self, s: &str) {
        self.buf.extend_from_slice(s.as_bytes());
        self.buf.push(0); // NUL terminator
    }

    // Struct — write uint16 LE length prefix + fields
    // Use write_struct_start to reserve length, write fields, then patch length
    pub fn write_struct_start(&mut self) -> usize {
        let pos = self.buf.len();
        self.buf.extend_from_slice(&[0, 0]); // placeholder for uint16 length
        pos // return position of length field for later patching
    }

    pub fn write_struct_end(&mut self, start_pos: usize) {
        let total_len = (self.buf.len() - start_pos - 2) as u16;
        self.buf[start_pos..start_pos + 2].copy_from_slice(&total_len.to_le_bytes());
    }

    // Array — just concatenate elements, no count/length prefix.
    // Length is implied by outer frame.
    pub fn write_array_start(&mut self) { /* no-op */ }
    pub fn write_array_end(&mut self)  { /* no-op */ }

    pub fn into_bytes(self) -> Vec<u8>;
}
```

### `decode.rs`

```rust
pub struct PackReader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> PackReader<'a> {
    pub fn new(data: &'a [u8]) -> Self;

    pub fn read_uint_packed(&mut self) -> Result<u32, SpinelError>;
    pub fn read_bool(&mut self) -> Result<bool, SpinelError>;
    pub fn read_uint8(&mut self) -> Result<u8, SpinelError>;
    pub fn read_int8(&mut self) -> Result<i8, SpinelError>;
    pub fn read_uint16(&mut self) -> Result<u16, SpinelError>;
    pub fn read_int16(&mut self) -> Result<i16, SpinelError>;
    pub fn read_uint32(&mut self) -> Result<u32, SpinelError>;
    pub fn read_int32(&mut self) -> Result<i32, SpinelError>;
    pub fn read_uint64(&mut self) -> Result<u64, SpinelError>;
    pub fn read_int64(&mut self) -> Result<i64, SpinelError>;
    pub fn read_ipv6(&mut self) -> Result<[u8; 16], SpinelError>;
    pub fn read_eui64(&mut self) -> Result<[u8; 8], SpinelError>;
    pub fn read_bytes(&mut self, len: usize) -> Result<&'a [u8], SpinelError>;
    pub fn read_data_with_len(&mut self) -> Result<&'a [u8], SpinelError> {
        let len = self.read_uint16()? as usize; // uint16 LE length prefix
        self.read_bytes(len)
    }

    /// Read a struct: uint16 LE length prefix + fields (read/write defer to callers).
    pub fn read_struct(&mut self) -> Result<&'a [u8], SpinelError> {
        // Same as data_with_len — uint16 LE length prefix then content
        self.read_data_with_len()
    }

    /// Read UTF-8 string (NUL-terminated).
    pub fn read_utf8(&mut self) -> Result<String, SpinelError> {
        let start = self.offset;
        while self.offset < self.data.len() && self.data[self.offset] != 0 {
            self.offset += 1;
        }
        if self.offset >= self.data.len() {
            return Err(SpinelError::Underflow);
        }
        let s = std::str::from_utf8(&data[start..self.offset])
            .map_err(|_| SpinelError::InvalidUtf8)?;
        self.offset += 1; // skip NUL
        Ok(s.to_string())
    }

    pub fn remaining(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}
```

### `hdlc.rs`

HDLC framing from `SpinelNCPInstance-DataPump.cpp:65-274`:

```rust
pub const FLAG_BYTE: u8 = 0x7E;
pub const ESCAPE_BYTE: u8 = 0x7D;
pub const XON: u8 = 0x11;
pub const XOFF: u8 = 0x13;
pub const SPECIAL_BYTE: u8 = 0xF8;
pub const ESCAPE_XFORM: u8 = 0x20;

pub struct HdlcEncoder {
    crc: u16,
}

impl HdlcEncoder {
    pub fn new() -> Self {
        Self { crc: 0xFFFF } // init = 0xFFFF
    }

    /// Encode a SpinelFrame with HDLC framing.
    /// Output: FLAG + escaped_data + CRC(2 bytes, little-endian) + FLAG
    pub fn encode_frame(&mut self, frame: &SpinelFrame) -> Vec<u8>;

    pub fn encode_bytes(&mut self, data: &[u8]) -> Vec<u8>;
}

pub struct HdlcDecoder {
    state: HdlcState,
    buffer: Vec<u8>,
    crc: u16,
}

impl HdlcDecoder {
    pub fn new() -> Self {
        Self {
            state: HdlcState::SeekingFlag,
            buffer: Vec::new(),
            crc: 0xFFFF, // init = 0xFFFF
        }
    }

    /// Feed one byte. Returns a complete frame when one is decoded.
    pub fn feed_byte(&mut self, byte: u8) -> Option<Result<Vec<u8>, SpinelError>>;
}
```

#### CRC-16 Parameters

From `SpinelNCPInstance-DataPump.cpp:88-93,242,274`:

```text
CRC-16/X.25 (HDLC FCS-16, RFC-1662)  — NOT CRC-16/CCITT-FALSE
Polynomial:  0x1021 (reflected)
Init value:  0xFFFF  (line 242: mInboundFrameHDLCCRC = 0xffff)
Final XOR:   0xFFFF  (line 274: mInboundFrameHDLCCRC ^= 0xFFFF)
Refin:       true
Refout:      true
```

> **IMPORTANT**: The `crc` crate's `CRC_16_CCITT_FALSE` constant uses init=0xFFFF but refin=false, which will NOT match this NCP. Use `CRC_16_X25` (init=0xFFFF, refin=true, refout=true, xorout=0xFFFF, also known as CRC-16/IBM-SDLC, CRC-16/ISO-HDLC, or CRC-16/CCITT-TRUE).

The CRC is computed over the unescaped frame bytes (excluding FLAG and CRC bytes themselves). The CRC is appended as 2 bytes in little-endian order.

### `vendor.rs`

TI vendor-specific Spinel commands (from `wisun_config.h`):

```rust
// Vendor command IDs
pub const VENDOR_CMD_GET_UNICAST_CHANNEL_LIST: u32 = /* ... */;
pub const VENDOR_CMD_SET_UNICAST_CHANNEL_LIST: u32 = /* ... */;
pub const VENDOR_CMD_GET_BROADCAST_CHANNEL_LIST: u32 = /* ... */;
// ... all vendor commands from wisun_config.h

// Vendor property types
pub struct UnicastChannelList(pub [u8; 17]);
pub struct BroadcastChannelList(pub [u8; 17]);
pub struct ChannelSpacing(pub u32);  // kHz
pub struct Ch0CenterFreq { pub mhz: u16, pub khz: u16 }
```

## Tests

### Test 1: Packed Uint (LEB128) Round-Trip

```rust
#[test]
fn packed_uint_single_byte() {
    let mut writer = PackWriter::new();
    writer.write_uint_packed(42);
    let data = writer.into_bytes();
    assert_eq!(data, vec![42]);
    let mut reader = PackReader::new(&data);
    assert_eq!(reader.read_uint_packed().unwrap(), 42);
}

#[test]
fn packed_uint_two_bytes() {
    let mut writer = PackWriter::new();
    writer.write_uint_packed(200);
    let data = writer.into_bytes();
    assert_eq!(data, vec![0xC8, 0x01]); // 200 = 0x80 | 0x48, 0x01
    let mut reader = PackReader::new(&data);
    assert_eq!(reader.read_uint_packed().unwrap(), 200);
}

#[test]
fn packed_uint_max_value() {
    let mut writer = PackWriter::new();
    writer.write_uint_packed(2097151); // SPINEL_MAX_UINT_PACKED
    let data = writer.into_bytes();
    assert_eq!(data.len(), 3);
    let mut reader = PackReader::new(&data);
    assert_eq!(reader.read_uint_packed().unwrap(), 2097151);
}

#[test]
fn packed_command_id() {
    // CMD_PROP_VALUE_IS = 6, should be single byte
    let mut writer = PackWriter::new();
    writer.write_uint_packed(6);
    let data = writer.into_bytes();
    assert_eq!(data, vec![0x06]);
}
```

### Test 2: UTF-8 String is NUL-Terminated

```rust
#[test]
fn utf8_string_nul_terminated() {
    let mut writer = PackWriter::new();
    writer.write_utf8("Hello");
    let data = writer.into_bytes();
    assert_eq!(data, b"Hello\0");

    let mut reader = PackReader::new(&data);
    let s = reader.read_utf8().unwrap();
    assert_eq!(s, "Hello");
}
```

### Test 3: Struct Has uint16 Length Prefix

```rust
#[test]
fn struct_has_uint16_length_prefix() {
    // Spinel structs have a uint16 LE length prefix before the fields
    let mut writer = PackWriter::new();
    let start = writer.write_struct_start();
    writer.write_uint8(0xAA);
    writer.write_uint16(0xBBCC);
    writer.write_struct_end(start);
    let data = writer.into_bytes();

    // Length prefix (2 bytes LE) = 3 (0xAA + 0xCC + 0xBB = 3 bytes)
    assert_eq!(data, vec![0x03, 0x00, 0xAA, 0xCC, 0xBB]);

    let mut reader = PackReader::new(&data);
    let content = reader.read_struct().unwrap();
    assert_eq!(content.len(), 3);

    let mut sub_reader = PackReader::new(content);
    assert_eq!(sub_reader.read_uint8().unwrap(), 0xAA);
    assert_eq!(sub_reader.read_uint16().unwrap(), 0xBBCC);
}
```

### Test 4: HDLC CRC Matches C Implementation (Golden Vector)

```rust
#[test]
fn hdlc_crc_matches_c_path() {
    // Golden vector: bytes [0x80, 0x06] fed into hdlc_crc16() with init=0xFFFF
    // then final XOR 0xFFFF. Computed by running DataPump.cpp hdlc_crc16 on
    // these bytes and extracting the 2-byte CRC from mInboundFrame at position
    // [mInboundFrameSize-2] after the ^=0xFFFF step.
    let mut encoder = HdlcEncoder::new();
    let data = vec![0x80, 0x06]; // FLAG header + CMD_PROP_VALUE_IS (packed)
    let encoded = encoder.encode_bytes(&data);

    // Expected CRC-16/X.25 of [0x80, 0x06] with init=0xFFFF, xorout=0xFFFF.
    // Verified = 0xE6BD (computed with the crc crate CRC_16_X25; matches
    // the reflected 0x1021 table + init/xorout 0xFFFF in DataPump.cpp:95-274).
    let expected_crc = 0xE6BDu16.to_le_bytes();

    let frame_end = encoded.len() - 2; // skip FLAG delimiters
    assert_eq!(encoded[frame_end..frame_end + 2], expected_crc,
        "CRC does not match C path. Run DataPump.cpp test case first to get golden value.");

    // Full round-trip decode
    let mut decoder = HdlcDecoder::new();
    let mut result = None;
    for byte in &encoded {
        result = decoder.feed_byte(*byte);
    }
    let frame_data = result.unwrap().unwrap();
    assert_eq!(frame_data, data);
}
```

### Test 5: HDLC Escape Handling

```rust
#[test]
fn hdlc_escape_special_bytes() {
    let mut encoder = HdlcEncoder::new();
    let data = vec![FLAG_BYTE, ESCAPE_BYTE, XON, XOFF, SPECIAL_BYTE];
    let encoded = encoder.encode_bytes(&data);

    // Verify no unescaped FLAG bytes in the middle
    // (FLAG only appears as frame delimiters)
    let inner = &encoded[1..encoded.len()-1];
    assert!(!inner.contains(&0x7E));

    // Decode and verify round-trip
    let mut decoder = HdlcDecoder::new();
    let mut decoded = None;
    for byte in encoded {
        decoded = decoder.feed_byte(byte);
    }
    assert_eq!(decoded.unwrap().unwrap(), data);
}
```

### Test 6: Frame Header FLAG|IID|TID

```rust
#[test]
fn frame_header_layout() {
    // FLAG (0x80) | IID=0 | TID=1 → 0x81
    let frame = SpinelFrame {
        header: 0x81,
        command_id: 6,  // CMD_PROP_VALUE_IS (packed as LEB128)
        payload: vec![0x00, 0x01, 0x02],
    };

    assert!(frame.has_flag());
    assert_eq!(frame.iid(), 0);
    assert_eq!(frame.tid(), 1);

    let encoded = frame.encode();
    let decoded = SpinelFrame::decode(&encoded).unwrap();
    assert_eq!(decoded.header, 0x81);
    assert_eq!(decoded.command_id, 6);
    assert_eq!(decoded.payload, vec![0x00, 0x01, 0x02]);
}
```

### Test 7: Frame Encode/Decode with Packed Command

```rust
#[test]
fn frame_round_trip_packed_command() {
    let frame = SpinelFrame::new(0x06, vec![0x01, 0x02]);
    let encoded = frame.encode();

    // First byte: header (FLAG | IID=0 | TID=0 → 0x80)
    assert_eq!(encoded[0], 0x80);
    // Second byte: command ID 6 as LEB128 → 0x06
    assert_eq!(encoded[1], 0x06);
    // Rest: payload
    assert_eq!(&encoded[2..], &[0x01, 0x02]);
}
```

### Test 8: HDLC Full Round-Trip with CRC

```rust
#[test]
fn hdlc_full_round_trip() {
    let mut encoder = HdlcEncoder::new();
    let frame = SpinelFrame::new(0x06, vec![0x00, 0x01]);
    let hdlc_encoded = encoder.encode_frame(&frame);

    // Verify starts and ends with FLAG
    assert_eq!(hdlc_encoded[0], FLAG_BYTE);
    assert_eq!(*hdlc_encoded.last().unwrap(), FLAG_BYTE);

    // Verify CRC is valid
    let mut decoder = HdlcDecoder::new();
    let mut result = None;
    for byte in &hdlc_encoded {
        result = decoder.feed_byte(byte);
    }
    let frame_data = result.unwrap().unwrap();
    let decoded = SpinelFrame::decode(&frame_data).unwrap();
    assert_eq!(decoded.command_id, 0x06);
}
```

### Test 9: Fuzz Target

```rust
// fuzz_targets/spinel_frame_fuzz.rs
#![no_main]
use libfuzzer_sys::fuzz_target;
use spinel::frame::SpinelFrame;

fuzz_target!(|data: &[u8]| {
    let _ = SpinelFrame::decode(data);
});
```

## Verification Checklist

- [ ] HDLC encode/decode matches `SpinelNCPInstance-DataPump.cpp` output for known inputs
- [ ] Pack format handles all types from `spinel.h:4507-4528` (C/S/L/X/i/E/e/U/D/d/t/A)
- [ ] Packed uint (LEB128) matches `spinel.h:180-199` encoding rules
- [ ] All command IDs from `spinel.h` are defined and encoded as UINT_PACKED
- [ ] All vendor commands from `wisun_config.h` are defined
- [ ] CRC-16 init=0xFFFF, refin=true, refout=true, xorout=0xFFFF matches `DataPump.cpp:242,274` — this is **CRC-16/X.25**, NOT CCITT-FALSE
- [ ] Golden vector CRC test passes: encode known bytes through Rust path, decode through C path (cross-compare)
- [ ] Frame header uses FLAG|IID|TID (not MB|AR|MN|seq)
- [ ] `d` (DATA_WLEN) uses uint16 LE length prefix (not packed uint), per spinel.h:222-224
- [ ] `t(...)` (STRUCT) has uint16 LE length prefix (like `d`), per spinel.h:257-262
- [ ] UTF-8 strings are NUL-terminated, not length-prefixed
- [ ] Fuzz target runs 60+ seconds with no crashes
- [ ] `cargo test` passes
- [ ] `cargo clippy` produces zero warnings
- [ ] No `unsafe` code in this crate
