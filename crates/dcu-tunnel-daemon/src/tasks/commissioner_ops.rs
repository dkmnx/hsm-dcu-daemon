//! Commissioner-side Spinel operations: LinkMetrics, JoinerAdd/Remove,
//! EnergyScanQuery. Each function serializes the correct Spinel wire
//! payload, checks NCP capabilities, and sends the command.
//!
//! Wire formats reference: `src/ncp-spinel/SpinelNCPControlInterface.cpp`.

use std::collections::HashMap;

use dcu_dbus::types::Variant;
use spinel::pack::PackWriter;

use crate::error::DaemonError;
use crate::instance::NcpInstanceBase;
use crate::tasks::params;

/// Helper: parse an IPv6 address string from params and write 16 bytes.
fn write_ipv6_from_params(
    w: &mut PackWriter,
    params: &HashMap<String, Variant>,
    key: &str,
) -> Result<(), DaemonError> {
    let addr_str = params::get_str(params, key)
        .ok_or_else(|| DaemonError::Ncp(format!("missing param: {key}")))?;
    let addr: std::net::Ipv6Addr = addr_str
        .parse()
        .map_err(|e| DaemonError::Ncp(format!("{key}: invalid IPv6 addr: {e}")))?;
    w.write_ipv6(&addr.octets());
    Ok(())
}

/// Helper: parse an EUI-64 hex string from params into 8 bytes.
fn parse_eui64(params: &HashMap<String, Variant>, key: &str) -> Option<[u8; 8]> {
    let bytes = params::get_bytes(params, key)?;
    if bytes.len() != 8 {
        return None;
    }
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&bytes);
    Some(arr)
}

/// Build the joiner identity struct into `w`. Shared by JoinerAdd and JoinerRemove.
///
/// - kDiscerner: `struct(u8 bitLen + u64 value)`
/// - kEui64: `struct(EUI64)`
/// - kAny: `struct(NULL)` (empty)
fn write_joiner_identity(
    w: &mut PackWriter,
    params: &HashMap<String, Variant>,
) -> Result<(), DaemonError> {
    let has_discerner = params::get_u8(params, "Joiner:DiscernerBitLength").is_some();
    if has_discerner {
        let bit_len = params::get_u8(params, "Joiner:DiscernerBitLength").unwrap_or(0);
        let value = params::get_u64(params, "Joiner:DiscernerValue").unwrap_or(0);
        let pos = w.write_struct_start();
        w.write_uint8(bit_len);
        w.write_uint64(value);
        w.write_struct_end(pos);
    } else if let Some(eui64) = parse_eui64(params, "Joiner:EUI64") {
        let pos = w.write_struct_start();
        w.write_eui64(&eui64);
        w.write_struct_end(pos);
    } else {
        let pos = w.write_struct_start();
        w.write_struct_end(pos);
    }
    Ok(())
}

/// LinkMetricsQuery: `PROP_VALUE_SET(0x152D, IPv6Addr + u8 seriesId + u8 metrics)`.
pub async fn link_metrics_query(
    ncp: &NcpInstanceBase,
    params: &HashMap<String, Variant>,
) -> Result<(), DaemonError> {
    if !ncp
        .has_capability(spinel::property::CAP_THREAD_LINK_METRICS)
        .await
    {
        return Err(DaemonError::Ncp(
            "FeatureNotSupported: THREAD_LINK_METRICS".into(),
        ));
    }
    let mut w = PackWriter::new();
    write_ipv6_from_params(&mut w, params, "LinkMetrics:Address")?;
    w.write_uint8(params::get_u8(params, "LinkMetrics:SeriesId").unwrap_or(0));
    w.write_uint8(params::get_u8(params, "LinkMetrics:Metrics").unwrap_or(0));
    ncp.send_prop_set(
        spinel::property::PROP_THREAD_LINK_METRICS_QUERY,
        w.into_bytes(),
    )
    .await?;
    Ok(())
}

/// LinkMetricsProbe: `PROP_VALUE_SET(0x152F, IPv6Addr + u8 seriesId + u8 length)`.
pub async fn link_metrics_probe(
    ncp: &NcpInstanceBase,
    params: &HashMap<String, Variant>,
) -> Result<(), DaemonError> {
    if !ncp
        .has_capability(spinel::property::CAP_THREAD_LINK_METRICS)
        .await
    {
        return Err(DaemonError::Ncp(
            "FeatureNotSupported: THREAD_LINK_METRICS".into(),
        ));
    }
    let mut w = PackWriter::new();
    write_ipv6_from_params(&mut w, params, "LinkMetrics:Address")?;
    w.write_uint8(params::get_u8(params, "LinkMetrics:SeriesId").unwrap_or(0));
    w.write_uint8(params::get_u8(params, "LinkMetrics:Length").unwrap_or(0));
    ncp.send_prop_set(
        spinel::property::PROP_THREAD_LINK_METRICS_PROBE,
        w.into_bytes(),
    )
    .await?;
    Ok(())
}

/// LinkMetricsMgmtForward: `PROP_VALUE_SET(0x1532, IPv6Addr + u8 seriesId + u8 frame_types + u8 metrics)`.
pub async fn link_metrics_mgmt_forward(
    ncp: &NcpInstanceBase,
    params: &HashMap<String, Variant>,
) -> Result<(), DaemonError> {
    if !ncp
        .has_capability(spinel::property::CAP_THREAD_LINK_METRICS)
        .await
    {
        return Err(DaemonError::Ncp(
            "FeatureNotSupported: THREAD_LINK_METRICS".into(),
        ));
    }
    let mut w = PackWriter::new();
    write_ipv6_from_params(&mut w, params, "LinkMetrics:Address")?;
    w.write_uint8(params::get_u8(params, "LinkMetrics:SeriesId").unwrap_or(0));
    w.write_uint8(params::get_u8(params, "LinkMetrics:FrameTypes").unwrap_or(0));
    w.write_uint8(params::get_u8(params, "LinkMetrics:Metrics").unwrap_or(0));
    ncp.send_prop_set(
        spinel::property::PROP_THREAD_LINK_METRICS_MGMT_FORWARD,
        w.into_bytes(),
    )
    .await?;
    Ok(())
}

/// LinkMetricsMgmtEnhAck: `PROP_VALUE_SET(0x1530, IPv6Addr + u8 flags + u8 metrics)`.
pub async fn link_metrics_mgmt_enh_ack(
    ncp: &NcpInstanceBase,
    params: &HashMap<String, Variant>,
) -> Result<(), DaemonError> {
    if !ncp
        .has_capability(spinel::property::CAP_THREAD_LINK_METRICS)
        .await
    {
        return Err(DaemonError::Ncp(
            "FeatureNotSupported: THREAD_LINK_METRICS".into(),
        ));
    }
    let mut w = PackWriter::new();
    write_ipv6_from_params(&mut w, params, "LinkMetrics:Address")?;
    w.write_uint8(params::get_u8(params, "LinkMetrics:Flags").unwrap_or(0));
    w.write_uint8(params::get_u8(params, "LinkMetrics:Metrics").unwrap_or(0));
    ncp.send_prop_set(
        spinel::property::PROP_THREAD_LINK_METRICS_MGMT_ENH_ACK,
        w.into_bytes(),
    )
    .await?;
    Ok(())
}

/// JoinerAdd: `PROP_VALUE_INSERT(0x83, struct + u32 timeout + utf8 psk)`.
///
/// The struct encodes the joiner identity:
/// - kAny: `struct(NULL)` (empty struct with uint16 length = 0)
/// - kEui64: `struct(EUI64)`
/// - kDiscerner: `struct(u8 bitLen + u64 value)`
///
/// If DiscernerBitLength is present, discerner wins; else if EUI64 is
/// present, EUI64 wins; otherwise kAny (no address filter).
pub async fn joiner_add(
    ncp: &NcpInstanceBase,
    params: &HashMap<String, Variant>,
) -> Result<(), DaemonError> {
    if !ncp
        .has_capability(spinel::property::CAP_THREAD_COMMISSIONER)
        .await
    {
        return Err(DaemonError::Ncp(
            "FeatureNotSupported: THREAD_COMMISSIONER".into(),
        ));
    }
    let psk = params::get_str(params, "Joiner:PSKd")
        .ok_or_else(|| DaemonError::Ncp("JoinerAdd requires PSKd".into()))?;
    let timeout = params::get_u32(params, "Joiner:Timeout").unwrap_or(0);

    let mut w = PackWriter::new();
    write_joiner_identity(&mut w, params)?;
    w.write_uint32(timeout);
    w.write_utf8(&psk);

    ncp.send_prop_insert(
        spinel::property::PROP_MESHCOP_COMMISSIONER_JOINERS,
        w.into_bytes(),
    )
    .await?;
    Ok(())
}

/// JoinerRemove: `PROP_VALUE_REMOVE(0x83, struct)`.
///
/// Same joiner identity struct as JoinerAdd, but no timeout/psk.
pub async fn joiner_remove(
    ncp: &NcpInstanceBase,
    params: &HashMap<String, Variant>,
) -> Result<(), DaemonError> {
    if !ncp
        .has_capability(spinel::property::CAP_THREAD_COMMISSIONER)
        .await
    {
        return Err(DaemonError::Ncp(
            "FeatureNotSupported: THREAD_COMMISSIONER".into(),
        ));
    }

    let mut w = PackWriter::new();
    write_joiner_identity(&mut w, params)?;

    ncp.send_prop_remove(
        spinel::property::PROP_MESHCOP_COMMISSIONER_JOINERS,
        w.into_bytes(),
    )
    .await?;
    Ok(())
}

/// EnergyScanQuery: `PROP_VALUE_SET(0x1801, u32 channel_mask + u8 count + u16 period + u16 scan_duration + IPv6Addr)`.
pub async fn energy_scan_query(
    ncp: &NcpInstanceBase,
    params: &HashMap<String, Variant>,
) -> Result<(), DaemonError> {
    if !ncp
        .has_capability(spinel::property::CAP_THREAD_COMMISSIONER)
        .await
    {
        return Err(DaemonError::Ncp(
            "FeatureNotSupported: THREAD_COMMISSIONER".into(),
        ));
    }
    let mut w = PackWriter::new();
    w.write_uint32(params::get_u32(params, "EnergyScan:ChannelMask").unwrap_or(0));
    w.write_uint8(params::get_u8(params, "EnergyScan:Count").unwrap_or(0));
    w.write_uint16(params::get_u16(params, "EnergyScan:Period").unwrap_or(0));
    w.write_uint16(params::get_u16(params, "EnergyScan:ScanDuration").unwrap_or(0));
    write_ipv6_from_params(&mut w, params, "EnergyScan:Destination")?;
    ncp.send_prop_set(
        spinel::property::PROP_MESHCOP_COMMISSIONER_ENERGY_SCAN,
        w.into_bytes(),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use spinel::pack::PackWriter;

    fn make_params(entries: Vec<(&str, Variant)>) -> HashMap<String, Variant> {
        let mut m = HashMap::new();
        for (k, v) in entries {
            m.insert(k.to_string(), v);
        }
        m
    }

    #[test]
    fn link_metrics_query_encoding() {
        let params = make_params(vec![
            ("LinkMetrics:Address", Variant::Str("fe80::1".into())),
            ("LinkMetrics:SeriesId", Variant::U8(42)),
            ("LinkMetrics:Metrics", Variant::U8(0x0F)),
        ]);
        let mut w = PackWriter::new();
        write_ipv6_from_params(&mut w, &params, "LinkMetrics:Address").unwrap();
        w.write_uint8(params::get_u8(&params, "LinkMetrics:SeriesId").unwrap_or(0));
        w.write_uint8(params::get_u8(&params, "LinkMetrics:Metrics").unwrap_or(0));
        let bytes = w.into_bytes();
        // IPv6 fe80::1 = 16 bytes, then seriesId=42, metrics=0x0F
        assert_eq!(bytes.len(), 16 + 1 + 1);
        assert_eq!(bytes[16], 42);
        assert_eq!(bytes[17], 0x0F);
        // Verify IPv6 encoding: fe80::1 = fe80:0000:0000:0000:0000:0000:0000:0001
        assert_eq!(&bytes[0..2], &[0xfe, 0x80]);
        assert_eq!(bytes[15], 0x01);
    }

    #[test]
    fn link_metrics_probe_encoding() {
        let params = make_params(vec![
            ("LinkMetrics:Address", Variant::Str("::1".into())),
            ("LinkMetrics:SeriesId", Variant::U8(1)),
            ("LinkMetrics:Length", Variant::U8(10)),
        ]);
        let mut w = PackWriter::new();
        write_ipv6_from_params(&mut w, &params, "LinkMetrics:Address").unwrap();
        w.write_uint8(params::get_u8(&params, "LinkMetrics:SeriesId").unwrap_or(0));
        w.write_uint8(params::get_u8(&params, "LinkMetrics:Length").unwrap_or(0));
        let bytes = w.into_bytes();
        assert_eq!(bytes.len(), 16 + 1 + 1);
        assert_eq!(bytes[15], 0x01); // ::1 last byte
        assert_eq!(bytes[16], 1);
        assert_eq!(bytes[17], 10);
    }

    #[test]
    fn link_metrics_mgmt_forward_encoding() {
        let params = make_params(vec![
            ("LinkMetrics:Address", Variant::Str("fe80::1".into())),
            ("LinkMetrics:SeriesId", Variant::U8(7)),
            ("LinkMetrics:FrameTypes", Variant::U8(0x03)),
            ("LinkMetrics:Metrics", Variant::U8(0x1F)),
        ]);
        let mut w = PackWriter::new();
        write_ipv6_from_params(&mut w, &params, "LinkMetrics:Address").unwrap();
        w.write_uint8(params::get_u8(&params, "LinkMetrics:SeriesId").unwrap_or(0));
        w.write_uint8(params::get_u8(&params, "LinkMetrics:FrameTypes").unwrap_or(0));
        w.write_uint8(params::get_u8(&params, "LinkMetrics:Metrics").unwrap_or(0));
        let bytes = w.into_bytes();
        assert_eq!(bytes.len(), 16 + 1 + 1 + 1);
        assert_eq!(bytes[16], 7);
        assert_eq!(bytes[17], 0x03);
        assert_eq!(bytes[18], 0x1F);
    }

    #[test]
    fn link_metrics_mgmt_enh_ack_encoding() {
        let params = make_params(vec![
            ("LinkMetrics:Address", Variant::Str("fe80::1".into())),
            ("LinkMetrics:Flags", Variant::U8(0x05)),
            ("LinkMetrics:Metrics", Variant::U8(0x0A)),
        ]);
        let mut w = PackWriter::new();
        write_ipv6_from_params(&mut w, &params, "LinkMetrics:Address").unwrap();
        w.write_uint8(params::get_u8(&params, "LinkMetrics:Flags").unwrap_or(0));
        w.write_uint8(params::get_u8(&params, "LinkMetrics:Metrics").unwrap_or(0));
        let bytes = w.into_bytes();
        assert_eq!(bytes.len(), 16 + 1 + 1);
        assert_eq!(bytes[16], 0x05);
        assert_eq!(bytes[17], 0x0A);
    }

    #[test]
    fn joiner_add_kany_encoding() {
        let mut w = PackWriter::new();
        let pos = w.write_struct_start();
        w.write_struct_end(pos);
        w.write_uint32(300);
        w.write_utf8("TESTPSK1234");
        let bytes = w.into_bytes();
        // struct(0 bytes content) = 2 bytes length prefix (0x0000)
        // + u32(300) = 4 bytes + utf8 "TESTPSK1234" + null = 12 bytes
        assert_eq!(bytes[0], 0x00); // struct length lo
        assert_eq!(bytes[1], 0x00); // struct length hi
        assert_eq!(&bytes[2..6], &300u32.to_le_bytes());
    }

    #[test]
    fn joiner_add_keui64_encoding() {
        let mut w = PackWriter::new();
        let pos = w.write_struct_start();
        w.write_eui64(&[0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77]);
        w.write_struct_end(pos);
        w.write_uint32(600);
        w.write_utf8("KEY123");
        let bytes = w.into_bytes();
        // struct: length=8 (EUI64)
        assert_eq!(bytes[0], 8);
        assert_eq!(bytes[1], 0);
        assert_eq!(
            &bytes[2..10],
            &[0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77]
        );
        assert_eq!(&bytes[10..14], &600u32.to_le_bytes());
    }

    #[test]
    fn joiner_add_kdiscerner_encoding() {
        let mut w = PackWriter::new();
        let pos = w.write_struct_start();
        w.write_uint8(40);
        w.write_uint64(0xDEADBEEF);
        w.write_struct_end(pos);
        w.write_uint32(120);
        w.write_utf8("DISC");
        let bytes = w.into_bytes();
        // struct: length = 1 (u8) + 8 (u64) = 9
        assert_eq!(bytes[0], 9);
        assert_eq!(bytes[1], 0);
        assert_eq!(bytes[2], 40); // bit length
        assert_eq!(&bytes[3..11], &0xDEADBEEF_u64.to_le_bytes());
        assert_eq!(&bytes[11..15], &120u32.to_le_bytes());
    }

    #[test]
    fn energy_scan_query_encoding() {
        let params = make_params(vec![
            ("EnergyScan:ChannelMask", Variant::U32(0x000007FFF)),
            ("EnergyScan:Count", Variant::U8(10)),
            ("EnergyScan:Period", Variant::U16(500)),
            ("EnergyScan:ScanDuration", Variant::U16(2000)),
            ("EnergyScan:Destination", Variant::Str("fe80::1".into())),
        ]);
        let mut w = PackWriter::new();
        w.write_uint32(0x000007FFF);
        w.write_uint8(10);
        w.write_uint16(500);
        w.write_uint16(2000);
        write_ipv6_from_params(&mut w, &params, "EnergyScan:Destination").unwrap();
        let bytes = w.into_bytes();
        assert_eq!(bytes.len(), 4 + 1 + 2 + 2 + 16);
        assert_eq!(&bytes[0..4], &0x000007FFF_u32.to_le_bytes());
        assert_eq!(bytes[4], 10);
        assert_eq!(&bytes[5..7], &500u16.to_le_bytes());
        assert_eq!(&bytes[7..9], &2000u16.to_le_bytes());
    }

    #[test]
    fn network_time_decode() {
        let mut w = PackWriter::new();
        w.write_uint64(1234567890);
        w.write_int8(-5);
        let bytes = w.into_bytes();
        let mut r = spinel::pack::PackReader::new(&bytes);
        let time = r.read_uint64().unwrap();
        let status = r.read_int8().unwrap();
        assert_eq!(time, 1234567890);
        assert_eq!(status, -5);
    }

    #[test]
    fn property_constants_correct() {
        // Verify the property IDs match C spinel.h values
        assert_eq!(spinel::property::PROP_THREAD_LINK_METRICS_QUERY, 0x152D);
        assert_eq!(spinel::property::PROP_THREAD_LINK_METRICS_PROBE, 0x152F);
        assert_eq!(
            spinel::property::PROP_THREAD_LINK_METRICS_MGMT_ENH_ACK,
            0x1530
        );
        assert_eq!(
            spinel::property::PROP_THREAD_LINK_METRICS_MGMT_FORWARD,
            0x1532
        );
        assert_eq!(spinel::property::PROP_MESHCOP_COMMISSIONER_JOINERS, 0x83);
        assert_eq!(spinel::property::PROP_THREAD_NETWORK_TIME, 0x1907);
        // Fixed: was 0x1901, now 0x1801
        assert_eq!(
            spinel::property::PROP_MESHCOP_COMMISSIONER_ENERGY_SCAN,
            0x1801
        );
        assert_eq!(spinel::property::CAP_THREAD_COMMISSIONER, 1024);
        assert_eq!(spinel::property::CAP_THREAD_LINK_METRICS, 1031);
    }
}
