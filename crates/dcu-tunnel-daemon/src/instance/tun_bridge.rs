//! TUN ↔ NCP IPv6 packet bridge.
//!
//! Implements the live data plane that the C daemon builds from two
//! protothreads (`ncp_to_driver_pump` / `driver_to_ncp_pump` in
//! `SpinelNCPInstance-DataPump.cpp`). The Rust version uses two independent
//! tokio tasks over the already-async [`dcu_tun::TunnelIPv6Interface`]:
//!
//! * [`ncp_to_tun`] — NCP → host. Reads `SPINEL_PROP_STREAM_NET` /
//!   `STREAM_NET_INSECURE` frames forwarded by `dispatch_unsolicited_static`
//!   and writes the raw IPv6 payload to the TUN. The Spinel value is packed
//!   as `DATA(DATA)` = `[ipv6_packet][metadata]`; the metadata is discarded.
//! * [`tun_to_ncp`] — host → NCP. Reads raw IPv6 packets from the TUN and
//!   wraps them in a `PROP_VALUE_SET(STREAM_NET [0x72] | STREAM_NET_INSECURE
//!   [0x73])` frame (5-byte header + payload, HDLC framing by `io_task`).
//!   When the NCP is not yet interface-up / is joining, the insecure
//!   property is used.
//!
//! Frame wire format (host → NCP), matching the C `driver_to_ncp_pump`:
//! `[0x80][0x01][prop][lenLo][lenHi] <ipv6_packet>` where the 16-bit length
//! is little-endian and counts only the IPv6 payload bytes.

use std::sync::Arc;

use tokio::sync::{RwLock, mpsc};

use dcu_tun::TunnelIPv6Interface;
use spinel::SpinelFrame;
use spinel::property::{PROP_STREAM_NET, PROP_STREAM_NET_INSECURE};

/// NCP → host: deliver IPv6 packets received from the NCP to the TUN.
pub async fn ncp_to_tun(
    tun: TunnelIPv6Interface,
    mut rx: mpsc::UnboundedReceiver<SpinelFrame>,
    cancel: tokio_util::sync::CancellationToken,
) {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            frame = rx.recv() => {
                let Some(frame) = frame else { break };
                match extract_ipv6(&frame.payload) {
                    Some(pkt) => {
                        if let Err(e) = tun.async_write_packet(pkt).await {
                            tracing::warn!("TUN write failed: {e}");
                        }
                    }
                    None => tracing::trace!("STREAM_NET frame with no IPv6 payload"),
                }
            }
        }
    }
    // Drop the receiver so the sender side observes closure.
    drop(rx);
}

/// Host → NCP: read IPv6 packets from the TUN and send them to the NCP.
pub async fn tun_to_ncp(
    tun: TunnelIPv6Interface,
    outbound_tx: mpsc::UnboundedSender<SpinelFrame>,
    ncp_state: Arc<RwLock<wisun_types::NcpState>>,
    cancel: tokio_util::sync::CancellationToken,
) {
    let mut buf = vec![0u8; 2000];
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            read = tun.async_read_packet(&mut buf) => {
                let n = match read {
                    Ok(n) if n > 0 => n,
                    Ok(_) => continue,
                    Err(e) => { tracing::warn!("TUN read failed: {e}"); continue; }
                };
                let packet = &buf[..n];
                // Use the insecure stream while the NCP is not yet associated
                // (mirrors C should_forward_ncpbound_frame: joining/credentials
                // needed frames use STREAM_NET_INSECURE).
                let insecure = {
                    let st = *ncp_state.read().await;
                    !st.is_associated()
                };
                let prop = if insecure {
                    PROP_STREAM_NET_INSECURE
                } else {
                    PROP_STREAM_NET
                };
                let frame = build_stream_net_frame(prop, packet);
                if outbound_tx.send(frame).is_err() {
                    tracing::warn!("TUN→NCP channel closed; stopping TUN read task");
                    break;
                }
            }
        }
    }
}

/// Extract the IPv6 packet from a `PROP_STREAM_NET` value payload.
///
/// Wire layout: packed_prop_id + `DATA(DATA)` — first DATA is
/// length-prefixed (uint16 LE) IPv6 packet, second DATA is opaque metadata
/// (RSSI/channel) which the C daemon discards. We read the first `DATA` only.
fn extract_ipv6(payload: &[u8]) -> Option<&[u8]> {
    let mut r = spinel::pack::PackReader::new(payload);
    if r.read_uint_packed().is_err() {
        return None;
    }
    r.read_data_with_len().ok()
}

/// Build a `PROP_VALUE_SET(prop, DATA(<ipv6_packet>))` frame.
///
/// The C `driver_to_ncp_pump` writes `[header][CMD_PROP_VALUE_SET][prop]
/// [lenLo][lenHi][ipv6_packet]` — the value is Spinel `d()` (uint16 LE
/// length prefix + data), not raw bytes. We replicate that via
/// `write_data_with_len` so the NCP can delimit the packet.
fn build_stream_net_frame(prop: u32, packet: &[u8]) -> SpinelFrame {
    let mut w = spinel::pack::PackWriter::new();
    w.write_uint_packed(prop);
    w.write_data_with_len(packet);
    SpinelFrame::with_header(
        spinel::frame::make_header(0, 0),
        spinel::command::CMD_PROP_VALUE_SET,
        w.into_bytes(),
    )
}
