//! Shared D-Bus parameter decoders for Form/Join/JoinerCommissioning tasks.
//!
//! D-Bus commands carry `HashMap<String, Variant>` overrides keyed by the
//! canonical property strings (e.g. `"Network:PANID"`). These helpers pull
//! typed values out of that map, accepting the common wire representations.

use dcu_dbus::types::Variant;
use std::collections::HashMap;
use zbus::zvariant::Value;

/// Read a string-typed parameter (falls back to the variant's textual form).
pub fn get_str(params: &HashMap<String, Variant>, key: &str) -> Option<String> {
    match params.get(key)? {
        Value::Str(s) => Some(s.to_string()),
        other => Some(other.to_string()),
    }
}

/// Read a `u8` parameter.
pub fn get_u8(params: &HashMap<String, Variant>, key: &str) -> Option<u8> {
    match params.get(key)? {
        Value::U8(n) => Some(*n),
        Value::U16(n) => Some(*n as u8),
        _ => None,
    }
}

/// Read a `u16` parameter.
pub fn get_u16(params: &HashMap<String, Variant>, key: &str) -> Option<u16> {
    match params.get(key)? {
        Value::U16(n) => Some(*n),
        Value::U8(n) => Some(*n as u16),
        Value::U32(n) => Some(*n as u16),
        _ => None,
    }
}

/// Read a `u32` parameter (decimal or hex string).
pub fn get_u32(params: &HashMap<String, Variant>, key: &str) -> Option<u32> {
    match params.get(key)? {
        Value::U32(n) => Some(*n),
        Value::U16(n) => Some(*n as u32),
        Value::U8(n) => Some(*n as u32),
        Value::Str(s) => {
            let s = s.trim();
            let s = s
                .strip_prefix("0x")
                .or_else(|| s.strip_prefix("0X"))
                .unwrap_or(s);
            u32::from_str_radix(s, 16).ok().or_else(|| s.parse().ok())
        }
        _ => None,
    }
}

/// Read a hex-string parameter as raw bytes (e.g. `"DEADBEEFCAFEBABE"`,
/// optionally `0x`-prefixed). Used for keys and extended addresses.
pub fn get_bytes(params: &HashMap<String, Variant>, key: &str) -> Option<Vec<u8>> {
    match params.get(key)? {
        Value::Str(s) => {
            let s = s.trim_start_matches("0x").trim_start_matches("0X");
            (0..s.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
                .collect::<Result<Vec<_>, _>>()
                .ok()
        }
        _ => None,
    }
}
