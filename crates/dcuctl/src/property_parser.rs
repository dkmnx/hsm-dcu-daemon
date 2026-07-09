use zbus::zvariant::Value;

#[derive(Debug)]
pub enum ParseError {
    InvalidValue(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidValue(msg) => write!(f, "invalid value: {msg}"),
        }
    }
}

impl std::error::Error for ParseError {}

pub fn parse_property_value(name: &str, input: &str) -> Result<Value<'static>, ParseError> {
    match name {
        "Network:PANID" => parse_hex_u16(input).map(Value::from),
        "NCP:CCAThreshold" => parse_i8(input).map(Value::from),
        "UnicastChList" | "BroadcastChList" | "AsyncChList" | "RegulationChList" => {
            parse_channel_mask(input).map(Value::from)
        }
        "Interface:Up"
        | "Stack:Up"
        | "Daemon:Enabled"
        | "Network:IsCommissioned"
        | "Network:IsConnected" => parse_bool(input).map(Value::from),
        "NCP:HardwareAddress" | "NCP:ExtendedAddress" | "NCP:MACAddress" => {
            Ok(Value::Str(input.to_string().into()))
        }
        "IPv6:LinkLocalAddress" | "IPv6:MeshLocalAddress" | "IPv6:MeshLocalPrefix" => {
            Ok(Value::Str(input.to_string().into()))
        }
        "Network:XPANID" => parse_hex_u64(input).map(Value::from),
        "Network:NodeType" => parse_node_type(input).map(|s| Value::Str(s.into())),
        "NCP:Channel" => parse_u8(input).map(Value::from),
        "NCP:Frequency" => parse_u32(input).map(Value::from),
        "NCP:RSSI" => parse_i32(input).map(Value::from),
        "NCP:TXPower" => parse_f64(input).map(Value::from),
        "Network:Name" => Ok(Value::Str(input.to_string().into())),
        _ => Ok(Value::Str(input.to_string().into())),
    }
}

fn parse_hex_u16(input: &str) -> Result<u16, ParseError> {
    let s = strip_hex_prefix(input)?;
    u16::from_str_radix(s, 16)
        .map_err(|_| ParseError::InvalidValue(format!("cannot parse \"{input}\" as hex u16")))
}

fn parse_hex_u64(input: &str) -> Result<u64, ParseError> {
    let s = strip_hex_prefix(input)?;
    u64::from_str_radix(s, 16)
        .map_err(|_| ParseError::InvalidValue(format!("cannot parse \"{input}\" as hex u64")))
}

fn parse_i8(input: &str) -> Result<i8, ParseError> {
    input
        .parse::<i8>()
        .map_err(|_| ParseError::InvalidValue(format!("cannot parse \"{input}\" as i8")))
}

fn parse_u8(input: &str) -> Result<u8, ParseError> {
    let s = input.trim();
    if s.starts_with("0x") || s.starts_with("0X") {
        let hex = &s[2..];
        u8::from_str_radix(hex, 16)
            .map_err(|_| ParseError::InvalidValue(format!("cannot parse \"{input}\" as u8")))
    } else {
        s.parse::<u8>()
            .map_err(|_| ParseError::InvalidValue(format!("cannot parse \"{input}\" as u8")))
    }
}

fn parse_u32(input: &str) -> Result<u32, ParseError> {
    let s = input.trim();
    if s.starts_with("0x") || s.starts_with("0X") {
        let hex = &s[2..];
        u32::from_str_radix(hex, 16)
            .map_err(|_| ParseError::InvalidValue(format!("cannot parse \"{input}\" as u32")))
    } else {
        s.parse::<u32>()
            .map_err(|_| ParseError::InvalidValue(format!("cannot parse \"{input}\" as u32")))
    }
}

fn parse_i32(input: &str) -> Result<i32, ParseError> {
    input
        .parse::<i32>()
        .map_err(|_| ParseError::InvalidValue(format!("cannot parse \"{input}\" as i32")))
}

fn parse_f64(input: &str) -> Result<f64, ParseError> {
    input
        .parse::<f64>()
        .map_err(|_| ParseError::InvalidValue(format!("cannot parse \"{input}\" as f64")))
}

fn parse_bool(input: &str) -> Result<bool, ParseError> {
    match input.to_lowercase().as_str() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        _ => Err(ParseError::InvalidValue(format!(
            "cannot parse \"{input}\" as bool (expected true/false/1/0)"
        ))),
    }
}

fn parse_channel_mask(input: &str) -> Result<Vec<u8>, ParseError> {
    let s = strip_hex_prefix(input)?;
    let hex: String = s.chars().filter(|c| *c != ':' && *c != '-').collect();
    if hex.len() % 2 != 0 {
        return Err(ParseError::InvalidValue(
            "channel mask must have even number of hex digits".into(),
        ));
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for chunk in hex.as_bytes().chunks(2) {
        let byte_str = std::str::from_utf8(chunk)
            .map_err(|_| ParseError::InvalidValue("invalid hex in channel mask".into()))?;
        let byte = u8::from_str_radix(byte_str, 16)
            .map_err(|_| ParseError::InvalidValue(format!("invalid hex byte: {byte_str}")))?;
        bytes.push(byte);
    }
    Ok(bytes)
}

fn parse_node_type(input: &str) -> Result<String, ParseError> {
    match input.to_lowercase().as_str() {
        "router" | "r" | "2" => Ok("router".into()),
        "end-device" | "enddevice" | "end" | "ed" | "e" | "3" => Ok("end-device".into()),
        "sleepy-end-device" | "sleepy" | "sed" | "s" | "4" => Ok("sleepy-end-device".into()),
        "lurker" | "nl-lurker" | "l" | "6" => Ok("nl-lurker".into()),
        _ => Err(ParseError::InvalidValue(format!(
            "unknown node type \"{input}\" (expected: router, end-device, sleepy-end-device, lurker)"
        ))),
    }
}

fn strip_hex_prefix(input: &str) -> Result<&str, ParseError> {
    let s = input.trim();
    Ok(s.strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_panid_hex() {
        let v = parse_property_value("Network:PANID", "0xABCD").unwrap();
        assert_eq!(v, Value::U16(0xABCD));
    }

    #[test]
    fn parse_bool_variants() {
        assert_eq!(
            parse_property_value("Interface:Up", "true").unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            parse_property_value("Interface:Up", "1").unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            parse_property_value("Interface:Up", "false").unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            parse_property_value("Interface:Up", "0").unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn parse_channel_mask() {
        let v = parse_property_value("UnicastChList", "FF:FF:01").unwrap();
        let expected: Value<'static> =
            Value::Array(zbus::zvariant::Array::from(vec![0xFF_u8, 0xFF, 0x01]));
        assert_eq!(v, expected);
    }

    #[test]
    fn parse_node_type_router() {
        for input in &["router", "r", "2"] {
            assert_eq!(parse_node_type(input).unwrap(), "router");
        }
    }

    #[test]
    fn parse_node_type_end_device() {
        for input in &["end-device", "enddevice", "end", "ed", "e", "3"] {
            assert_eq!(parse_node_type(input).unwrap(), "end-device");
        }
    }

    #[test]
    fn parse_node_type_sleepy() {
        for input in &["sleepy-end-device", "sleepy", "sed", "s", "4"] {
            assert_eq!(parse_node_type(input).unwrap(), "sleepy-end-device");
        }
    }

    #[test]
    fn parse_node_type_lurker() {
        for input in &["lurker", "nl-lurker", "l", "6"] {
            assert_eq!(parse_node_type(input).unwrap(), "nl-lurker");
        }
    }

    #[test]
    fn hex_u16() {
        assert_eq!(parse_hex_u16("0xABCD").unwrap(), 0xABCD);
        assert_eq!(parse_hex_u16("ABCD").unwrap(), 0xABCD);
    }

    #[test]
    fn hex_u64() {
        assert_eq!(
            parse_hex_u64("0x0011223344556677").unwrap(),
            0x0011223344556677
        );
    }

    #[test]
    fn u8_parse() {
        assert_eq!(parse_u8("255").unwrap(), 255);
        assert_eq!(parse_u8("0xFF").unwrap(), 255);
    }

    #[test]
    fn u32_parse() {
        assert_eq!(parse_u32("100").unwrap(), 100);
        assert_eq!(parse_u32("0x64").unwrap(), 100);
    }

    #[test]
    fn f64_parse() {
        assert_eq!(parse_f64("1.5").unwrap(), 1.5);
        assert_eq!(parse_f64("-20.0").unwrap(), -20.0);
    }

    #[test]
    fn parse_passthrough_string() {
        let v = parse_property_value("Network:Name", "MyNet").unwrap();
        assert_eq!(v, Value::Str("MyNet".into()));
    }
}
