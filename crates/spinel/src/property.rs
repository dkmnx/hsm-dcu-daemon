//! Spinel property IDs and the typed pack/unpack engine.
//!
//! The Spinel wire protocol encodes a property value as a sequence of typed
//! fields described by a "pack format string" (the same vocabulary as
//! `SPINEL_DATATYPE_*` in `spinel.h:4507-4553`). This module provides:
//!
//! * Numeric Spinel property ID constants (mirroring `spinel.h` range).
//! * A generic [`PackFormat`] interpreter that encodes/decodes a property
//!   payload from a format string such as `"Cc"` or `"LUD"` or `"t(CC)"`.
//! * Convenience builders for the common property command frames
//!   (`PROP_VALUE_GET`, `PROP_VALUE_SET`, `PROP_VALUE_IS`).
//!
//! The TI Wi-SUN vendor properties live in [`crate::vendor`] and are also
//! described by format strings.

use crate::error::SpinelError;
use crate::pack::{PackReader, PackWriter};

// ---------------------------------------------------------------------------
// Property ID constants (from third_party/openthread/src/ncp/spinel.h)
// ---------------------------------------------------------------------------

pub const PROP_LAST_STATUS: u32 = 0;
pub const PROP_PROTOCOL_VERSION: u32 = 1;
pub const PROP_NCP_VERSION: u32 = 2;
pub const PROP_INTERFACE_TYPE: u32 = 3;
pub const PROP_VENDOR_ID: u32 = 4;
pub const PROP_CAPS: u32 = 5;
pub const PROP_INTERFACE_COUNT: u32 = 6;
pub const PROP_POWER_STATE: u32 = 7;
pub const PROP_HWADDR: u32 = 8;
pub const PROP_LOCK: u32 = 9;
pub const PROP_HOST_POWER_STATE: u32 = 12;
pub const PROP_MCU_POWER_STATE: u32 = 13;

// PHY
pub const PROP_PHY_ENABLED: u32 = 0x20;
pub const PROP_PHY_CHAN: u32 = 0x21;
pub const PROP_PHY_CHAN_SUPPORTED: u32 = 0x22;
pub const PROP_PHY_FREQ: u32 = 0x23;
pub const PROP_PHY_CCA_THRESHOLD: u32 = 0x24;
pub const PROP_PHY_TX_POWER: u32 = 0x25;
pub const PROP_PHY_RSSI: u32 = 0x26;
pub const PROP_PHY_RX_SENSITIVITY: u32 = 0x27;

// MAC
pub const PROP_MAC_SCAN_STATE: u32 = 0x30;
pub const PROP_MAC_SCAN_MASK: u32 = 0x31;
pub const PROP_MAC_SCAN_PERIOD: u32 = 0x32;
pub const PROP_MAC_15_4_LADDR: u32 = 0x34;
pub const PROP_MAC_15_4_SADDR: u32 = 0x35;
pub const PROP_MAC_15_4_PANID: u32 = 0x36;
pub const PROP_MAC_PROMISCUOUS_MODE: u32 = 0x38;
pub const PROP_MAC_DATA_POLL_PERIOD: u32 = 0x3A;

// MAC extended
pub const PROP_MAC_ALLOWLIST: u32 = 0x1300;
pub const PROP_MAC_ALLOWLIST_ENABLED: u32 = 0x1301;
pub const PROP_MAC_EXTENDED_ADDR: u32 = 0x1302;
pub const PROP_MAC_DENYLIST: u32 = 0x1306;
pub const PROP_MAC_DENYLIST_ENABLED: u32 = 0x1307;
pub const PROP_MAC_FIXED_RSS: u32 = 0x1308;

// Vendor range
pub const PROP_VENDOR__BEGIN: u32 = 0x3C00;
pub const PROP_VENDOR__END: u32 = 0x4000;

// NET properties (SPINEL_PROP_NET__BEGIN = 0x40)
pub const PROP_NET_SAVED: u32 = 0x40;
pub const PROP_NET_IF_UP: u32 = 0x41;
pub const PROP_NET_STACK_UP: u32 = 0x42;
pub const PROP_NET_ROLE: u32 = 0x43;
pub const PROP_NET_NETWORK_NAME: u32 = 0x44;
pub const PROP_NET_XPANID: u32 = 0x45;
pub const PROP_NET_MASTER_KEY: u32 = 0x46;
pub const PROP_NET_PSKC: u32 = 0x4B;
pub const PROP_NET_KEY_SEQUENCE_COUNTER: u32 = 0x47;
pub const PROP_NET_PARTITION_ID: u32 = 0x48;
pub const PROP_NET_REQUIRE_JOIN_EXISTING: u32 = 0x49;

// IPV6 properties (SPINEL_PROP_IPV6__BEGIN = 0x60)
pub const PROP_IPV6_ML_PREFIX: u32 = 0x62;

// THREAD properties (SPINEL_PROP_THREAD__BEGIN = 0x50)
pub const PROP_THREAD_MODE: u32 = 0x5E;

// THREAD extended properties (SPINEL_PROP_THREAD_EXT__BEGIN = 0x1500)
pub const PROP_THREAD_ROUTER_ROLE_ENABLED: u32 = 0x1507;
pub const PROP_THREAD_NEIGHBOR_TABLE: u32 = 0x150B;
pub const PROP_THREAD_ROUTER_TABLE: u32 = 0x1517;
pub const PROP_THREAD_CHILD_TABLE_ADDRESSES: u32 = 0x1521;
pub const PROP_THREAD_NEIGHBOR_TABLE_ERROR_RATES: u32 = 0x1522;

// THREAD dataset container properties (SPINEL_PROP_THREAD_EXT__BEGIN + 24..=25)
// These carry the full operational dataset (`A(t(iD))` blob) as their value.
// The inner dataset field IDs (DATASET_*) are decoded from inside that blob.
pub const PROP_THREAD_ACTIVE_DATASET: u32 = 0x1518;
pub const PROP_THREAD_PENDING_DATASET: u32 = 0x1519;

// THREAD dataset properties (SPINEL_PROP_THREAD_EXT__BEGIN + 28..=39)
pub const PROP_DATASET_ACTIVE_TIMESTAMP: u32 = 0x151C;
pub const PROP_DATASET_PENDING_TIMESTAMP: u32 = 0x151D;
pub const PROP_DATASET_DELAY_TIMER: u32 = 0x151E;
pub const PROP_DATASET_SECURITY_POLICY: u32 = 0x151F;
pub const PROP_DATASET_RAW_TLVS: u32 = 0x1520;
pub const PROP_DATASET_DEST_ADDRESS: u32 = 0x1527;

// THREAD MLR/BBR properties (SPINEL_PROP_THREAD_EXT__BEGIN + 52..=53)
pub const PROP_THREAD_MLR_REQUEST: u32 = 0x1534;
pub const PROP_THREAD_MLR_RESPONSE: u32 = 0x1535;

// THREAD LinkMetrics properties (SPINEL_PROP_THREAD_EXT__BEGIN + 45..=50)
pub const PROP_THREAD_LINK_METRICS_QUERY: u32 = 0x152D;
pub const PROP_THREAD_LINK_METRICS_QUERY_RESULT: u32 = 0x152E;
pub const PROP_THREAD_LINK_METRICS_PROBE: u32 = 0x152F;
pub const PROP_THREAD_LINK_METRICS_MGMT_ENH_ACK: u32 = 0x1530;
pub const PROP_THREAD_LINK_METRICS_MGMT_FORWARD: u32 = 0x1532;

// THREAD BBR properties
pub const PROP_THREAD_BBR_STATE: u32 = 0x1538;
pub const PROP_THREAD_BBR_REGISTRATION_JITTER: u32 = 0x1539;
pub const PROP_THREAD_BBR_MLR_TIMEOUT: u32 = 0x153A;

// MESHCOP commissioner properties — base range (SPINEL_PROP_MESHCOP__BEGIN = 0x80)
pub const PROP_MESHCOP_COMMISSIONER_JOINERS: u32 = 0x83;

// MESHCOP commissioner properties — ext range (SPINEL_PROP_MESHCOP_EXT__BEGIN = 0x1800)
pub const PROP_MESHCOP_COMMISSIONER_ANNOUNCE_BEGIN: u32 = 0x1800;
pub const PROP_MESHCOP_COMMISSIONER_ENERGY_SCAN: u32 = 0x1801;
pub const PROP_MESHCOP_COMMISSIONER_PAN_ID_QUERY: u32 = 0x1803;
pub const PROP_MESHCOP_COMMISSIONER_GENERATE_PSKC: u32 = 0x1807;

// OpenThread-specific properties (SPINEL_PROP_OPENTHREAD__BEGIN = 0x1900)
pub const PROP_THREAD_NETWORK_TIME: u32 = 0x1907;

// THREAD route/service properties
pub const PROP_THREAD_ON_MESH_NETS: u32 = 0x1510;
pub const PROP_THREAD_OFF_MESH_ROUTES: u32 = 0x1511;
pub const PROP_THREAD_SERVICE: u32 = 0x1513;

// MESHCOP properties (SPINEL_PROP_MESHCOP__BEGIN = 0x80)
pub const PROP_MESHCOP_JOINER_COMMISSIONING: u32 = 0x81;
pub const PROP_MESHCOP_JOINER_DISCERNER: u32 = 0x86;

// MAC scan beacon (SPINEL_PROP_MAC__BEGIN + 3 = 0x33)
pub const PROP_MAC_SCAN_BEACON: u32 = 0x33;

// STREAM properties (SPINEL_PROP_STREAM__BEGIN = 0x70)
pub const PROP_STREAM_DEBUG: u32 = 0x70;
pub const PROP_STREAM_RAW: u32 = 0x71;
pub const PROP_STREAM_NET: u32 = 0x72;
pub const PROP_STREAM_NET_INSECURE: u32 = 0x73;

// MAC raw stream enable (SPINEL_PROP_MAC__BEGIN + 7 = 0x30 + 7 = 0x37)
pub const PROP_MAC_RAW_STREAM_ENABLED: u32 = 0x37;

// IPv6 address/multicast tables (SPINEL_PROP_IPV6__BEGIN + 3 / +1)
pub const PROP_IPV6_ADDRESS_TABLE: u32 = 0x63;
pub const PROP_IPV6_MULTICAST_ADDRESS_TABLE: u32 = 0x61;

// MSG buffer counters (SPINEL_PROP_CNTR__BEGIN + 400 = 0x6A4)
pub const PROP_MSG_BUFFER_COUNTERS: u32 = 0x6A4;

// Scan state enum (payload of PROP_MAC_SCAN_STATE)
pub const SCAN_STATE_IDLE: u8 = 0;
pub const SCAN_STATE_BEACON: u8 = 1;
pub const SCAN_STATE_ENERGY: u8 = 2;
pub const SCAN_STATE_DISCOVER: u8 = 3;

// MCU power state enum (payload of PROP_MCU_POWER_STATE)
pub const MCU_POWER_STATE_ON: u8 = 0;
pub const MCU_POWER_STATE_LOW_POWER: u8 = 1;

// JOIN status range (used by Join/JoinerCommissioning task final waits)
pub const STATUS_JOIN__BEGIN: i32 = 104;
pub const STATUS_JOIN__END: i32 = 112;

/// Spinel status code: returned in `PROP_VALUE_IS(LAST_STATUS, …)` when a
/// received command payload failed to parse (SPINEL_STATUS_PARSE_ERROR = 9).
/// Used as the default status when a LAST_STATUS response payload itself
/// fails to decode — mirrors the C `SPINEL_STATUS_PARSE_ERROR` fallback.
pub const SPINEL_STATUS_PARSE_ERROR: u32 = 9;

/// Command is not recognized.
pub const STATUS_INVALID_COMMAND: u32 = 5;
/// Property is not recognized.
pub const STATUS_PROP_NOT_FOUND: u32 = 13;

// NCP capability bits (used by task capability guards)
pub const CAP_MCU_POWER_STATE: u32 = 13;
pub const CAP_CONFIG_FTD: u32 = 32;
pub const CAP_ROLE_ROUTER: u32 = 48;
pub const CAP_ROLE_SLEEPY: u32 = 49;
pub const CAP_NEST_LEGACY_INTERFACE: u32 = 15296;

// Thread-specific capabilities (SPINEL_CAP_THREAD__BEGIN = 1024)
pub const CAP_THREAD_COMMISSIONER: u32 = 1024;
pub const CAP_THREAD_LINK_METRICS: u32 = 1031;

/// Returns `true` if `prop` is in the TI/NCP vendor range.
#[must_use]
pub fn is_vendor_property(prop: u32) -> bool {
    (PROP_VENDOR__BEGIN..PROP_VENDOR__END).contains(&prop)
}

// ---------------------------------------------------------------------------
// Pack format string engine
// ---------------------------------------------------------------------------

/// A single Spinel data type token from a pack format string.
///
/// Mirrors `SPINEL_DATATYPE_*` in `spinel.h`. Composite types `t(...)` and
/// `A(...)` are modelled by [`PackFormatToken::Struct`] / [`PackFormatToken::Array`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackFormatToken {
    /// `b` — bool
    Bool,
    /// `C` — uint8
    Uint8,
    /// `c` — int8
    Int8,
    /// `S` — uint16
    Uint16,
    /// `s` — int16
    Int16,
    /// `L` — uint32
    Uint32,
    /// `l` — int32
    Int32,
    /// `X` — uint64
    Uint64,
    /// `x` — int64
    Int64,
    /// `i` — packed uint (LEB128)
    UintPacked,
    /// `6` — IPv6 address (16 bytes, big-endian)
    Ipv6,
    /// `E` — EUI-64 (8 bytes, big-endian)
    Eui64,
    /// `e` — EUI-48 (6 bytes, big-endian)
    Eui48,
    /// `D` — raw data, no length prefix
    Data,
    /// `d` — data with uint16 LE length prefix
    DataWithLen,
    /// `U` — NUL-terminated UTF-8 string
    Utf8,
    /// `t(...)` — struct with uint16 LE length prefix + inner format
    Struct(Vec<PackFormatToken>),
    /// `A(...)` — array of inner format, no count prefix
    Array(Vec<PackFormatToken>),
}

/// Errors from parsing or applying a pack format string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackFormatError {
    /// Unrecognized format character.
    UnknownToken(char),
    /// Mismatched `)` or unbalanced `(` in a composite format.
    UnbalancedBrackets,
    /// Underflow while decoding a value from the payload.
    Underflow,
}

impl std::fmt::Display for PackFormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackFormatError::UnknownToken(c) => write!(f, "unknown pack format token: {c:?}"),
            PackFormatError::UnbalancedBrackets => {
                write!(f, "unbalanced brackets in format string")
            }
            PackFormatError::Underflow => write!(f, "underflow decoding value"),
        }
    }
}

impl std::error::Error for PackFormatError {}

impl From<SpinelError> for PackFormatError {
    fn from(e: SpinelError) -> Self {
        match e {
            SpinelError::Underflow => PackFormatError::Underflow,
            _ => PackFormatError::Underflow,
        }
    }
}

/// A parsed Spinel pack format string, e.g. `"Cc"` or `"t(CC)"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackFormat {
    tokens: Vec<PackFormatToken>,
}

impl PackFormat {
    /// Parse a format string into a [`PackFormat`].
    pub fn parse(fmt: &str) -> Result<Self, PackFormatError> {
        let tokens = parse_tokens(fmt.as_bytes(), &mut 0)?;
        Ok(Self { tokens })
    }

    /// Encode a sequence of [`PackValue`]s into bytes following this format.
    pub fn encode(&self, values: &[PackValue]) -> Result<Vec<u8>, PackFormatError> {
        if values.len() != self.tokens.len() {
            return Err(PackFormatError::UnknownToken('?'));
        }
        let mut writer = PackWriter::new();
        for (tok, val) in self.tokens.iter().zip(values.iter()) {
            encode_token(&mut writer, tok, val)?;
        }
        Ok(writer.into_bytes())
    }

    /// Decode a payload into a vector of [`PackValue`]s following this format.
    pub fn decode(&self, data: &[u8]) -> Result<Vec<PackValue>, PackFormatError> {
        let mut reader = PackReader::new(data);
        let mut out = Vec::with_capacity(self.tokens.len());
        for tok in &self.tokens {
            out.push(decode_token(&mut reader, tok)?);
        }
        Ok(out)
    }
}

fn parse_tokens(bytes: &[u8], pos: &mut usize) -> Result<Vec<PackFormatToken>, PackFormatError> {
    let mut tokens = Vec::new();
    while *pos < bytes.len() {
        let c = bytes[*pos] as char;
        match c {
            ')' => return Ok(tokens), // end of a composite
            '(' => {
                *pos += 1;
                let inner = parse_tokens(bytes, pos)?;
                // consume the closing ')'
                if *pos >= bytes.len() || bytes[*pos] as char != ')' {
                    return Err(PackFormatError::UnbalancedBrackets);
                }
                *pos += 1;
                // Peek the enclosing composite tag ('t' or 'A') by inspecting
                // the token that preceded this group is not possible here, so
                // we infer: a plain group `(` is treated as a struct body and
                // the leading tag is handled by the caller via peek.
                tokens.push(PackFormatToken::Struct(inner));
            }
            'b' => tokens.push(PackFormatToken::Bool),
            'C' => tokens.push(PackFormatToken::Uint8),
            'c' => tokens.push(PackFormatToken::Int8),
            'S' => tokens.push(PackFormatToken::Uint16),
            's' => tokens.push(PackFormatToken::Int16),
            'L' => tokens.push(PackFormatToken::Uint32),
            'l' => tokens.push(PackFormatToken::Int32),
            'X' => tokens.push(PackFormatToken::Uint64),
            'x' => tokens.push(PackFormatToken::Int64),
            'i' => tokens.push(PackFormatToken::UintPacked),
            '6' => tokens.push(PackFormatToken::Ipv6),
            'E' => tokens.push(PackFormatToken::Eui64),
            'e' => tokens.push(PackFormatToken::Eui48),
            'D' => tokens.push(PackFormatToken::Data),
            'd' => tokens.push(PackFormatToken::DataWithLen),
            'U' => tokens.push(PackFormatToken::Utf8),
            't' | 'A' => {
                // Composite: tag followed by `(...)`.
                *pos += 1;
                if *pos >= bytes.len() || bytes[*pos] as char != '(' {
                    return Err(PackFormatError::UnbalancedBrackets);
                }
                *pos += 1;
                let inner = parse_tokens(bytes, pos)?;
                if *pos >= bytes.len() || bytes[*pos] as char != ')' {
                    return Err(PackFormatError::UnbalancedBrackets);
                }
                *pos += 1;
                tokens.push(if c == 't' {
                    PackFormatToken::Struct(inner)
                } else {
                    PackFormatToken::Array(inner)
                });
            }
            other => return Err(PackFormatError::UnknownToken(other)),
        }
        *pos += 1;
    }
    Ok(tokens)
}

/// A decoded/encoded value for a single pack format token.
///
/// This is a generic carrier used by [`PackFormat`]. For typed access prefer
/// the dedicated helpers in [`crate::vendor`] or build frames directly with
/// [`crate::pack::PackWriter`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackValue {
    Bool(bool),
    Uint8(u8),
    Int8(i8),
    Uint16(u16),
    Int16(i16),
    Uint32(u32),
    Int32(i32),
    Uint64(u64),
    Int64(i64),
    UintPacked(u32),
    Ipv6([u8; 16]),
    Eui64([u8; 8]),
    Eui48([u8; 6]),
    Data(Vec<u8>),
    Utf8(String),
}

fn encode_token(
    w: &mut PackWriter,
    tok: &PackFormatToken,
    val: &PackValue,
) -> Result<(), PackFormatError> {
    match tok {
        PackFormatToken::Bool => {
            if let PackValue::Bool(v) = val {
                w.write_bool(*v)
            } else {
                return Err(PackFormatError::UnknownToken('b'));
            }
        }
        PackFormatToken::Uint8 => {
            if let PackValue::Uint8(v) = val {
                w.write_uint8(*v)
            } else {
                return Err(PackFormatError::UnknownToken('C'));
            }
        }
        PackFormatToken::Int8 => {
            if let PackValue::Int8(v) = val {
                w.write_int8(*v)
            } else {
                return Err(PackFormatError::UnknownToken('c'));
            }
        }
        PackFormatToken::Uint16 => {
            if let PackValue::Uint16(v) = val {
                w.write_uint16(*v)
            } else {
                return Err(PackFormatError::UnknownToken('S'));
            }
        }
        PackFormatToken::Int16 => {
            if let PackValue::Int16(v) = val {
                w.write_int16(*v)
            } else {
                return Err(PackFormatError::UnknownToken('s'));
            }
        }
        PackFormatToken::Uint32 => {
            if let PackValue::Uint32(v) = val {
                w.write_uint32(*v)
            } else {
                return Err(PackFormatError::UnknownToken('L'));
            }
        }
        PackFormatToken::Int32 => {
            if let PackValue::Int32(v) = val {
                w.write_int32(*v)
            } else {
                return Err(PackFormatError::UnknownToken('l'));
            }
        }
        PackFormatToken::Uint64 => {
            if let PackValue::Uint64(v) = val {
                w.write_uint64(*v)
            } else {
                return Err(PackFormatError::UnknownToken('X'));
            }
        }
        PackFormatToken::Int64 => {
            if let PackValue::Int64(v) = val {
                w.write_int64(*v)
            } else {
                return Err(PackFormatError::UnknownToken('x'));
            }
        }
        PackFormatToken::UintPacked => {
            if let PackValue::UintPacked(v) = val {
                w.write_uint_packed(*v)
            } else {
                return Err(PackFormatError::UnknownToken('i'));
            }
        }
        PackFormatToken::Ipv6 => {
            if let PackValue::Ipv6(v) = val {
                w.write_ipv6(v)
            } else {
                return Err(PackFormatError::UnknownToken('6'));
            }
        }
        PackFormatToken::Eui64 => {
            if let PackValue::Eui64(v) = val {
                w.write_eui64(v)
            } else {
                return Err(PackFormatError::UnknownToken('E'));
            }
        }
        PackFormatToken::Eui48 => {
            if let PackValue::Eui48(v) = val {
                w.write_eui48(v)
            } else {
                return Err(PackFormatError::UnknownToken('e'));
            }
        }
        PackFormatToken::Data => {
            if let PackValue::Data(v) = val {
                w.write_bytes(v)
            } else {
                return Err(PackFormatError::UnknownToken('D'));
            }
        }
        PackFormatToken::DataWithLen => {
            if let PackValue::Data(v) = val {
                w.write_data_with_len(v)
            } else {
                return Err(PackFormatError::UnknownToken('d'));
            }
        }
        PackFormatToken::Utf8 => {
            if let PackValue::Utf8(v) = val {
                w.write_utf8(v)
            } else {
                return Err(PackFormatError::UnknownToken('U'));
            }
        }
        PackFormatToken::Struct(_) => {
            // t(...) — uint16 LE length prefix wraps the inner payload.
            // The inner payload is supplied verbatim as PackValue::Data.
            let PackValue::Data(inner_bytes) = val else {
                return Err(PackFormatError::UnknownToken('t'));
            };
            w.write_data_with_len(inner_bytes);
        }
        PackFormatToken::Array(_) => {
            if let PackValue::Data(v) = val {
                w.write_bytes(v)
            } else {
                return Err(PackFormatError::UnknownToken('A'));
            }
        }
    }
    Ok(())
}

fn decode_token(
    r: &mut PackReader<'_>,
    tok: &PackFormatToken,
) -> Result<PackValue, PackFormatError> {
    Ok(match tok {
        PackFormatToken::Bool => PackValue::Bool(r.read_bool()?),
        PackFormatToken::Uint8 => PackValue::Uint8(r.read_uint8()?),
        PackFormatToken::Int8 => PackValue::Int8(r.read_int8()?),
        PackFormatToken::Uint16 => PackValue::Uint16(r.read_uint16()?),
        PackFormatToken::Int16 => PackValue::Int16(r.read_int16()?),
        PackFormatToken::Uint32 => PackValue::Uint32(r.read_uint32()?),
        PackFormatToken::Int32 => PackValue::Int32(r.read_int32()?),
        PackFormatToken::Uint64 => PackValue::Uint64(r.read_uint64()?),
        PackFormatToken::Int64 => PackValue::Int64(r.read_int64()?),
        PackFormatToken::UintPacked => PackValue::UintPacked(r.read_uint_packed()?),
        PackFormatToken::Ipv6 => PackValue::Ipv6(r.read_ipv6()?),
        PackFormatToken::Eui64 => PackValue::Eui64(r.read_eui64()?),
        PackFormatToken::Eui48 => PackValue::Eui48(r.read_eui48()?),
        PackFormatToken::Data => {
            let len = r.remaining();
            PackValue::Data(r.read_bytes(len)?.to_vec())
        }
        PackFormatToken::DataWithLen => PackValue::Data(r.read_data_with_len()?.to_vec()),
        PackFormatToken::Utf8 => PackValue::Utf8(r.read_utf8()?),
        PackFormatToken::Struct(_) => {
            // t(...) — uint16 LE length prefix wraps the inner payload.
            PackValue::Data(r.read_struct()?.to_vec())
        }
        PackFormatToken::Array(_) => {
            let len = r.remaining();
            PackValue::Data(r.read_bytes(len)?.to_vec())
        }
    })
}

// ---------------------------------------------------------------------------
// Property command frame builders
// ---------------------------------------------------------------------------

use crate::frame::SpinelFrame;

/// Build a `PROP_VALUE_GET` frame for the given property ID.
pub fn prop_value_get(prop: u32) -> SpinelFrame {
    let mut w = PackWriter::new();
    w.write_uint_packed(prop);
    SpinelFrame::new(crate::command::CMD_PROP_VALUE_GET, w.into_bytes())
}

/// Build a `PROP_VALUE_SET` frame for the given property ID and payload.
pub fn prop_value_set(prop: u32, payload: Vec<u8>) -> SpinelFrame {
    let mut w = PackWriter::new();
    w.write_uint_packed(prop);
    w.write_bytes(&payload);
    SpinelFrame::new(crate::command::CMD_PROP_VALUE_SET, w.into_bytes())
}

/// Build a `PROP_VALUE_IS` frame for the given property ID and payload.
pub fn prop_value_is(prop: u32, payload: Vec<u8>) -> SpinelFrame {
    let mut w = PackWriter::new();
    w.write_uint_packed(prop);
    w.write_bytes(&payload);
    SpinelFrame::new(crate::command::CMD_PROP_VALUE_IS, w.into_bytes())
}

// ---------------------------------------------------------------------------
// Unprefixed aliases (phase 3C)
// ---------------------------------------------------------------------------
//
// The porting docs reference the dataset/NET constants without the `PROP_`
// prefix (e.g. `prop::DATASET_ACTIVE_TIMESTAMP`). These re-exports provide
// those names without duplicating the magic numbers — a single source of
// truth lives in the `PROP_*` constants above.

pub const PHY_CHAN: u32 = PROP_PHY_CHAN;
pub const PHY_CHAN_SUPPORTED: u32 = PROP_PHY_CHAN_SUPPORTED;
pub const MAC_15_4_PANID: u32 = PROP_MAC_15_4_PANID;
pub const NET_NETWORK_NAME: u32 = PROP_NET_NETWORK_NAME;
pub const NET_MASTER_KEY: u32 = PROP_NET_MASTER_KEY;
pub const IPV6_ML_PREFIX: u32 = PROP_IPV6_ML_PREFIX;
pub const NET_XPANID: u32 = PROP_NET_XPANID;
pub const NET_PSKC: u32 = PROP_NET_PSKC;
pub const DATASET_ACTIVE_TIMESTAMP: u32 = PROP_DATASET_ACTIVE_TIMESTAMP;
pub const DATASET_PENDING_TIMESTAMP: u32 = PROP_DATASET_PENDING_TIMESTAMP;
pub const DATASET_DELAY_TIMER: u32 = PROP_DATASET_DELAY_TIMER;
pub const DATASET_SECURITY_POLICY: u32 = PROP_DATASET_SECURITY_POLICY;
pub const DATASET_RAW_TLVS: u32 = PROP_DATASET_RAW_TLVS;
pub const DATASET_DEST_ADDRESS: u32 = PROP_DATASET_DEST_ADDRESS;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_parse_basic() {
        let fmt = PackFormat::parse("CcSL").unwrap();
        assert_eq!(
            fmt.tokens,
            vec![
                PackFormatToken::Uint8,
                PackFormatToken::Int8,
                PackFormatToken::Uint16,
                PackFormatToken::Uint32,
            ]
        );
    }

    #[test]
    fn format_parse_struct() {
        let fmt = PackFormat::parse("t(CC)").unwrap();
        assert_eq!(
            fmt.tokens,
            vec![PackFormatToken::Struct(vec![
                PackFormatToken::Uint8,
                PackFormatToken::Uint8,
            ])]
        );
    }

    #[test]
    fn format_round_trip_struct() {
        let fmt = PackFormat::parse("t(CL)").unwrap();
        let bytes = fmt
            .encode(&[PackValue::Data({
                let mut w = PackWriter::new();
                w.write_uint8(0xAA);
                w.write_uint32(0xBBCCDDEE);
                w.into_bytes()
            })])
            .unwrap();
        // length prefix (2 LE) = 5
        assert_eq!(bytes[0..2], [0x05, 0x00]);
        let decoded = fmt.decode(&bytes).unwrap();
        assert_eq!(decoded.len(), 1);
        if let PackValue::Data(inner) = &decoded[0] {
            assert_eq!(inner, &[0xAA, 0xEE, 0xDD, 0xCC, 0xBB]);
        } else {
            panic!("expected Data");
        }
    }

    #[test]
    fn format_unknown_token() {
        assert_eq!(
            PackFormat::parse("Q"),
            Err(PackFormatError::UnknownToken('Q'))
        );
    }

    #[test]
    fn format_unbalanced() {
        assert_eq!(
            PackFormat::parse("t(CC"),
            Err(PackFormatError::UnbalancedBrackets)
        );
    }

    #[test]
    fn prop_value_get_frame() {
        let frame = prop_value_get(PROP_PHY_CHAN);
        assert_eq!(frame.command_id, crate::command::CMD_PROP_VALUE_GET);
        let mut r = PackReader::new(&frame.payload);
        assert_eq!(r.read_uint_packed().unwrap(), PROP_PHY_CHAN);
    }

    #[test]
    fn prop_value_set_frame() {
        let payload = vec![0x05, 0x00];
        let frame = prop_value_set(PROP_PHY_CHAN, payload.clone());
        assert_eq!(frame.command_id, crate::command::CMD_PROP_VALUE_SET);
        let mut r = PackReader::new(&frame.payload);
        assert_eq!(r.read_uint_packed().unwrap(), PROP_PHY_CHAN);
        assert_eq!(r.read_bytes(2).unwrap(), &payload[..]);
    }

    #[test]
    fn vendor_property_range() {
        assert!(is_vendor_property(0x3C00));
        assert!(!is_vendor_property(0x21));
    }
}
