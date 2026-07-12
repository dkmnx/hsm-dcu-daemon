//! Predefined test scenarios that drive a mock NCP through network
//! operations (form, join, sleep/wake, error injection) by sending the same
//! Spinel frames the daemon would write.

use dcu_serial::FramedTransport;
use spinel::pack::PackWriter;
use spinel::property::prop_value_set;
use spinel::property::{
    PROP_MAC_15_4_PANID, PROP_MAC_PROMISCUOUS_MODE, PROP_NET_IF_UP, PROP_NET_NETWORK_NAME,
    PROP_NET_STACK_UP, PROP_NET_XPANID, PROP_PHY_CHAN,
};

use crate::failure::MockError;
use crate::pty_transport::DuplexTransport;

/// High-level test scenarios executed against a mock NCP transport.
#[derive(Debug, Clone)]
pub enum Scenario {
    /// Power on, form network by setting properties and bringing up interface + stack.
    FormNetwork {
        network_name: String,
        pan_id: u16,
        _node_count: usize,
    },
    /// Power on, join an existing network.
    JoinNetwork { network_name: String, pan_id: u16 },
    /// Deep sleep → wake cycle via `MCU_POWER_STATE`.
    SleepWake,
    /// Multiple nodes join and leave (topology simulation).
    DynamicTopology {
        _join_count: usize,
        _leave_count: usize,
    },
    /// Send `CMD_RESET` and observe the mock reset.
    NcpReset,
    /// Never respond to a specific command, causing a daemon timeout.
    CommandTimeout { _command_id: u32 },
    /// Reject a specific property get with `LAST_STATUS_FAILURE`.
    RejectProperty { _prop_key: u32 },
}

impl Scenario {
    /// Execute the scenario by driving a framed transport connected to a mock
    /// NCP. The mock's `run()` must be spawned by the caller.
    pub async fn execute(
        self,
        frame_tx: &mut FramedTransport<DuplexTransport>,
    ) -> Result<(), MockError> {
        match self {
            Scenario::FormNetwork {
                network_name,
                pan_id,
                _node_count: _,
            } => {
                // Clear any previous network settings via NET_CLEAR.
                let clear_frame =
                    spinel::frame::SpinelFrame::new(spinel::command::CMD_NET_CLEAR, Vec::new());
                frame_tx.send_frame(&clear_frame).await?;
                let _ = frame_tx.recv_frame().await?;

                // Set channel to 11.
                let ch_payload = {
                    let mut w = PackWriter::new();
                    w.write_uint8(11);
                    w.into_bytes()
                };
                frame_tx
                    .send_frame(&prop_value_set(PROP_PHY_CHAN, ch_payload))
                    .await?;
                let _ = frame_tx.recv_frame().await?;

                // Promiscuous off.
                frame_tx
                    .send_frame(&prop_value_set(PROP_MAC_PROMISCUOUS_MODE, vec![0]))
                    .await?;
                let _ = frame_tx.recv_frame().await?;

                // Set PAN ID.
                let mut w = PackWriter::new();
                w.write_uint16(pan_id);
                frame_tx
                    .send_frame(&prop_value_set(PROP_MAC_15_4_PANID, w.into_bytes()))
                    .await?;
                let _ = frame_tx.recv_frame().await?;

                // Set network name.
                let mut w = PackWriter::new();
                w.write_utf8(&network_name);
                frame_tx
                    .send_frame(&prop_value_set(PROP_NET_NETWORK_NAME, w.into_bytes()))
                    .await?;
                let _ = frame_tx.recv_frame().await?;

                // Set XPANID.
                let xpanid = {
                    let mut x = vec![0u8; 8];
                    x[0..6].copy_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE]);
                    x
                };
                frame_tx
                    .send_frame(&prop_value_set(PROP_NET_XPANID, xpanid))
                    .await?;
                let _ = frame_tx.recv_frame().await?;

                // Bring interface up.
                let mut w = PackWriter::new();
                w.write_bool(true);
                frame_tx
                    .send_frame(&prop_value_set(PROP_NET_IF_UP, w.into_bytes()))
                    .await?;
                let _ = frame_tx.recv_frame().await?;

                // Bring stack up.
                let mut w = PackWriter::new();
                w.write_bool(true);
                frame_tx
                    .send_frame(&prop_value_set(PROP_NET_STACK_UP, w.into_bytes()))
                    .await?;
                let _ = frame_tx.recv_frame().await?;

                Ok(())
            }

            Scenario::JoinNetwork { .. } => {
                let mut w = PackWriter::new();
                w.write_bool(true);
                frame_tx
                    .send_frame(&prop_value_set(PROP_NET_IF_UP, w.into_bytes()))
                    .await?;
                let _ = frame_tx.recv_frame().await?;

                let mut w = PackWriter::new();
                w.write_bool(true);
                frame_tx
                    .send_frame(&prop_value_set(PROP_NET_STACK_UP, w.into_bytes()))
                    .await?;
                let _ = frame_tx.recv_frame().await?;

                Ok(())
            }

            Scenario::SleepWake => Err(MockError::InvalidPayload(
                "SleepWake scenario not implemented".into(),
            )),

            Scenario::DynamicTopology { .. } => Err(MockError::InvalidPayload(
                "DynamicTopology scenario not implemented".into(),
            )),

            Scenario::NcpReset => {
                let reset_frame =
                    spinel::frame::SpinelFrame::new(spinel::command::CMD_RESET, Vec::new());
                frame_tx.send_frame(&reset_frame).await?;
                let _ = frame_tx.recv_frame().await?;
                Ok(())
            }

            Scenario::CommandTimeout { .. } => Err(MockError::InvalidPayload(
                "CommandTimeout scenario not implemented".into(),
            )),

            Scenario::RejectProperty { .. } => Err(MockError::InvalidPayload(
                "RejectProperty scenario not implemented — use with_failure".into(),
            )),
        }
    }
}
