//! `scan` — active network scan collecting unsolicited beacons.
//!
//! Port of `src/ncp-spinel/SpinelNCPTaskScan.cpp`. Unlike the other tasks,
//! scan issues a `PROP_VALUE_SET(MAC_SCAN_STATE, SCAN)` and then collects a
//! *stream* of unsolicited `PROP_VALUE_IS(MAC_SCAN_BEACON)` frames until
//! `PROP_VALUE_IS(MAC_SCAN_STATE) == IDLE` arrives or the timeout fires.
//!
//! The beacons are routed by `NcpInstanceBase::run()` into the active scan
//! collector channel registered here (see `register_scan_collector`).

use std::time::Duration;

use spinel::command::CMD_PROP_VALUE_SET;
use spinel::pack::{PackReader, PackWriter};
use spinel::property::prop_value_set;
use spinel::property::{
    PROP_MAC_SCAN_BEACON, PROP_MAC_SCAN_MASK, PROP_MAC_SCAN_STATE, SCAN_STATE_BEACON,
    SCAN_STATE_IDLE,
};
use tokio::sync::mpsc;
use wisun_types::ChannelMask;

use crate::DaemonError;
use crate::instance::NcpInstanceBase;

/// Entry-guard timeout. The C uses NCP_DEFAULT_COMMAND_RESPONSE_TIMEOUT (5s);
/// this is set higher to avoid races during scan setup.
const TIMEOUT: Duration = Duration::from_secs(15);
/// Total scan beacon-collection duration before timeout.
const SCAN_TIMEOUT: Duration = Duration::from_secs(15);

/// A single discovered network from a scan beacon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanResult {
    pub channel: u8,
    pub rssi: i8,
    pub laddr: [u8; 8],
    pub saddr: u16,
    pub panid: u16,
    pub lqi: u8,
    pub proto: u32,
    pub flags: u8,
    pub network_name: String,
    pub xpanid: Vec<u8>,
}

impl ScanResult {
    /// Decode a `PROP_VALUE_IS(MAC_SCAN_BEACON)` frame.
    ///
    /// Beacon payload format (C: `"Cct(ESSC)t(iCUd)"`):
    /// `C` channel, `c` rssi, then a struct `(ESSC)` = laddr(eui64) saddr(u16)
    /// panid(u16) lqi(u8), then a struct `(iCUd)` = proto(packed) flags(u8)
    /// networkid(utf8) xpanid(data).
    fn from_beacon(frame: &spinel::frame::SpinelFrame) -> Option<ScanResult> {
        // Skip the property-key prefix (packed uint) of the PROP_VALUE_IS frame.
        let mut reader = PackReader::new(&frame.payload);
        let _ = reader.read_uint_packed().ok()?;

        let channel = reader.read_uint8().ok()?;
        let rssi = reader.read_int8().ok()?;

        // Inner struct (ESSC): eui64, uint16, uint16, uint8.
        let laddr_bytes = reader.read_eui64().ok()?;
        let saddr = reader.read_uint16().ok()?;
        let panid = reader.read_uint16().ok()?;
        let lqi = reader.read_uint8().ok()?;

        // Inner struct (iCUd): packed, uint8, utf8, data.
        let proto = reader.read_uint_packed().ok()?;
        let flags = reader.read_uint8().ok()?;
        let network_name = reader.read_utf8().unwrap_or_default();
        let xpanid = reader
            .read_data_with_len()
            .map(|b| b.to_vec())
            .unwrap_or_default();

        Some(ScanResult {
            channel,
            rssi,
            laddr: laddr_bytes,
            saddr,
            panid,
            lqi,
            proto,
            flags,
            network_name,
            xpanid,
        })
    }
}

/// Run an active scan. `channel_mask` is a `wisun_types::ChannelMask`
/// bitfield (matching the D-Bus `NetScanStart` param shape). The C expands
/// the bitfield into a **list of channel indices** and sends that list as the
/// `MAC_SCAN_MASK` payload (raw bytes, not a bitmask) — this does the same.
/// So `channel_mask` is NOT the wire encoding; it is the channel set, and this
/// function builds the index list the NCP expects.
pub async fn scan(
    ncp: &NcpInstanceBase,
    channel_mask: &ChannelMask,
) -> Result<Vec<ScanResult>, DaemonError> {
    // C guards: !is_initializing && state ∉ {ASSOCIATING, CREDENTIALS_NEEDED}.
    ncp.wait_for_state(
        |s| {
            !s.is_initializing()
                && !matches!(
                    s,
                    wisun_types::NcpState::Associating | wisun_types::NcpState::CredentialsNeeded
                )
        },
        TIMEOUT,
    )
    .await?;

    let (beacon_tx, mut beacon_rx) = mpsc::unbounded_channel();
    ncp.register_scan_collector(beacon_tx).await;

    // Expand the bitfield into the channel-index list the NCP expects.
    let mut mask_bytes = Vec::new();
    for ch in 0u8..=128 {
        if channel_mask.is_channel_set(ch) {
            mask_bytes.push(ch);
        }
    }

    // Set the channel mask.
    ncp.send_command(
        CMD_PROP_VALUE_SET,
        prop_value_set(PROP_MAC_SCAN_MASK, mask_bytes).payload,
    )
    .await?;

    // Start the scan.
    let mut w = PackWriter::new();
    w.write_uint8(SCAN_STATE_BEACON);
    ncp.send_command(
        CMD_PROP_VALUE_SET,
        prop_value_set(PROP_MAC_SCAN_STATE, w.into_bytes()).payload,
    )
    .await?;

    // Collect beacons until the scan state reports IDLE or we time out.
    let mut results = Vec::new();
    let deadline = tokio::time::sleep(SCAN_TIMEOUT);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            frame = beacon_rx.recv() => {
                let frame = match frame {
                    Some(f) => f,
                    None => break, // collector dropped
                };
                let prop = PackReader::new(&frame.payload)
                    .read_uint_packed()
                    .unwrap_or(0);
                if prop == PROP_MAC_SCAN_STATE {
                    // Payload after the prop key: a packed-int scan state ("i").
                    let mut r = PackReader::new(&frame.payload);
                    let _ = r.read_uint_packed();
                    if r.read_uint_packed().unwrap_or(SCAN_STATE_IDLE as u32) == SCAN_STATE_IDLE as u32 {
                        break;
                    }
                } else if prop == PROP_MAC_SCAN_BEACON {
                    if let Some(result) = ScanResult::from_beacon(&frame) {
                        results.push(result);
                    }
                }
            }
            _ = &mut deadline => {
                ncp.unregister_scan_collector().await;
                return Err(DaemonError::Ncp("scan timed out".into()));
            }
        }
    }

    ncp.unregister_scan_collector().await;
    Ok(results)
}
