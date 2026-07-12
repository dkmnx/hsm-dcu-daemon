//! Mock NCP state machine.
//!
//! Emulates a TI Wi-SUN NCP responding to Spinel commands over any
//! [`Transport`]. Property sets drive [`NcpState`] transitions; the mock
//! emits the same frame types (`PROP_VALUE_IS`, etc.) that a real NCP would.

use std::collections::HashMap;

use spinel::command::{
    CMD_NET_CLEAR, CMD_NOOP, CMD_PEEK, CMD_PEEK_RET, CMD_PROP_VALUE_GET, CMD_PROP_VALUE_IS,
    CMD_PROP_VALUE_SET, CMD_RESET,
};
use spinel::frame::SpinelFrame;
use spinel::hdlc::HdlcEncoder;
use spinel::pack::{PackReader, PackWriter};
use spinel::property::{
    PROP_CAPS, PROP_HWADDR, PROP_INTERFACE_COUNT, PROP_INTERFACE_TYPE, PROP_LAST_STATUS,
    PROP_MAC_15_4_PANID, PROP_MAC_SCAN_MASK, PROP_MAC_SCAN_STATE, PROP_MCU_POWER_STATE,
    PROP_NCP_VERSION, PROP_NET_IF_UP, PROP_NET_STACK_UP, PROP_PHY_CHAN, PROP_PHY_ENABLED,
    PROP_PROTOCOL_VERSION, SCAN_STATE_BEACON, SCAN_STATE_IDLE, is_vendor_property, prop_value_is,
};
use tokio::io::AsyncWriteExt;
use tokio::time::sleep;
use wisun_types::{DriverState, NcpState};

use crate::config::{CapabilitySet, MockConfig};
use crate::failure::{FailureRule, MockError};
use crate::topology::MockTopology;

/// Maximum number of bytes PEEK will return, preventing wire-driven allocation.
const MAX_PEEK_SIZE: usize = 1024;

/// Internal property store for the mock NCP.
struct PropStore {
    map: HashMap<u32, Vec<u8>>,
}

fn encode_u32(v: u32) -> Vec<u8> {
    let mut w = PackWriter::new();
    w.write_uint32(v);
    w.into_bytes()
}

fn encode_u16(v: u16) -> Vec<u8> {
    let mut w = PackWriter::new();
    w.write_uint16(v);
    w.into_bytes()
}

fn encode_u8(v: u8) -> Vec<u8> {
    let mut w = PackWriter::new();
    w.write_uint8(v);
    w.into_bytes()
}

fn encode_bool(v: bool) -> Vec<u8> {
    let mut w = PackWriter::new();
    w.write_bool(v);
    w.into_bytes()
}

fn encode_u64(v: u64) -> Vec<u8> {
    let mut w = PackWriter::new();
    w.write_uint64(v);
    w.into_bytes()
}

fn encode_str(v: &str) -> Vec<u8> {
    let mut w = PackWriter::new();
    w.write_utf8(v);
    w.into_bytes()
}

impl PropStore {
    fn new(config: &MockConfig, caps: &CapabilitySet) -> Self {
        let mut map: HashMap<u32, Vec<u8>> = HashMap::new();

        map.insert(PROP_PROTOCOL_VERSION, encode_str("1.0"));
        map.insert(PROP_NCP_VERSION, encode_str(&config.ncp_version));
        map.insert(PROP_INTERFACE_TYPE, encode_u32(1));
        map.insert(PROP_INTERFACE_COUNT, encode_u32(1));
        map.insert(
            PROP_HWADDR,
            encode_u64(u64::from_le_bytes(config.hardware_address.0)),
        );
        map.insert(PROP_PHY_ENABLED, encode_bool(true));
        map.insert(PROP_PHY_CHAN, encode_u8(11));
        map.insert(PROP_MAC_15_4_PANID, encode_u16(0xABCD));
        map.insert(PROP_NET_IF_UP, encode_bool(false));
        map.insert(PROP_NET_STACK_UP, encode_bool(false));

        let mut w = PackWriter::new();
        for cap in &caps.bits {
            w.write_uint_packed(*cap);
        }
        map.insert(PROP_CAPS, w.into_bytes());

        Self { map }
    }

    fn get(&self, key: u32) -> Option<&[u8]> {
        self.map.get(&key).map(|v| v.as_slice())
    }

    fn set(&mut self, key: u32, value: Vec<u8>) {
        self.map.insert(key, value);
    }
}

/// Build a `PROP_VALUE_IS(LAST_STATUS, status)` frame.
pub(crate) fn last_status_error(status: u32) -> SpinelFrame {
    let mut w = PackWriter::new();
    w.write_uint_packed(PROP_LAST_STATUS);
    w.write_uint_packed(status);
    SpinelFrame::new(CMD_PROP_VALUE_IS, w.into_bytes())
}

/// Mock NCP. Generic over any [`dcu_serial::Transport`].
pub struct MockNcp<T: dcu_serial::transport::Transport + Unpin> {
    framed: dcu_serial::FramedTransport<T>,
    ncp_state: NcpState,
    driver_state: DriverState,
    topology: MockTopology,
    config: MockConfig,
    caps: CapabilitySet,
    failure_rules: Vec<FailureRule>,
    frame_counter: u32,
    /// Remaining frames to drop (consumed by DropFrames rules).
    remaining_drops: u32,
    props: PropStore,
}

impl<T: dcu_serial::transport::Transport + Unpin> MockNcp<T> {
    pub fn new(
        transport: T,
        config: MockConfig,
        topology: MockTopology,
        caps: CapabilitySet,
        failure_rules: Vec<FailureRule>,
        initial_ncp_state: NcpState,
        initial_driver_state: DriverState,
    ) -> Result<Self, MockError> {
        let props = PropStore::new(&config, &caps);
        // Initialize remaining_drops from the first DropFrames rule (if any).
        let remaining_drops = failure_rules
            .iter()
            .find_map(|r| {
                if let FailureRule::DropFrames(n) = r {
                    Some(*n)
                } else {
                    None
                }
            })
            .unwrap_or(0);
        Ok(Self {
            framed: dcu_serial::FramedTransport::new(transport),
            ncp_state: initial_ncp_state,
            driver_state: initial_driver_state,
            topology,
            config,
            caps,
            failure_rules,
            frame_counter: 0,
            remaining_drops,
            props,
        })
    }

    /// Run the mock NCP event loop: read frames, respond, emit unsolicited
    /// frames (state changes, scan beacons, etc.).
    pub async fn run(&mut self) -> Result<(), MockError> {
        loop {
            let frame = match self.framed.recv_frame().await {
                Ok(f) => f,
                Err(e) => {
                    tracing::debug!("Mock NCP transport closed: {e}");
                    return Ok(());
                }
            };

            self.frame_counter += 1;

            // DropFrames: consume the drop budget before processing.
            if self.remaining_drops > 0 {
                self.remaining_drops -= 1;
                continue;
            }

            // DropCommand: silently ignore specific command IDs.
            let mut dropped = false;
            for rule in &self.failure_rules {
                if let FailureRule::DropCommand { command_id } = rule {
                    if *command_id == frame.command_id {
                        dropped = true;
                        break;
                    }
                }
            }
            if dropped {
                continue;
            }

            // DelayResponse: sleep before responding.
            for rule in &self.failure_rules {
                if let FailureRule::DelayResponse(n, dur) = rule {
                    if self.frame_counter == *n {
                        sleep(*dur).await;
                    }
                }
            }

            let responses = match self.handle_frame(frame).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!("Mock NCP handler error: {e}");
                    // Send an error response rather than hanging.
                    vec![last_status_error(1)]
                }
            };

            // Determine if the current frame should be corrupted, before
            // the send loop (avoids borrow conflicts with inner_mut).
            let do_corrupt = self
                .failure_rules
                .iter()
                .any(|rule| matches!(rule, FailureRule::CorruptCrc(n) if *n == self.frame_counter));
            for resp in &responses {
                if do_corrupt {
                    let mut raw = HdlcEncoder::new().encode_frame(resp);
                    let len = raw.len();
                    if len >= 4 {
                        raw[len - 3] ^= 0x01;
                        raw[len - 4] ^= 0x80;
                    }
                    // Write raw corrupted bytes directly, bypassing send_frame
                    // (which would re-encode with a correct CRC).
                    self.framed.inner_mut().write_all(&raw).await?;
                    self.framed.inner_mut().flush().await?;
                } else {
                    self.framed.send_frame(resp).await?;
                }
            }
        }
    }

    async fn handle_frame(&mut self, frame: SpinelFrame) -> Result<Vec<SpinelFrame>, MockError> {
        let tid = frame.tid();
        let mut responses = match frame.command_id {
            CMD_RESET => self.handle_reset(),
            CMD_NOOP => vec![last_status_error(0)],
            CMD_NET_CLEAR => self.handle_net_clear(),
            CMD_PEEK => self.handle_peek(&frame.payload)?,
            CMD_PROP_VALUE_GET => self.handle_prop_value_get(&frame.payload).await?,
            CMD_PROP_VALUE_SET => self.handle_prop_value_set(&frame.payload).await?,
            _ => {
                tracing::warn!("Mock NCP: unknown command {}", frame.command_id);
                vec![last_status_error(spinel::property::STATUS_INVALID_COMMAND)]
            }
        };
        // Echo the request TID on every response frame so the daemon's
        // response_table delivers them to the waiting oneshot.
        for resp in &mut responses {
            resp.header = (resp.header & 0xF0) | (tid & 0x0F);
        }
        Ok(responses)
    }

    // ---- Command handlers ----

    fn handle_reset(&mut self) -> Vec<SpinelFrame> {
        self.ncp_state = NcpState::Uninitialized;
        self.driver_state = DriverState::Initializing;
        vec![last_status_error(0)]
    }

    fn handle_net_clear(&mut self) -> Vec<SpinelFrame> {
        self.ncp_state = NcpState::Offline;
        self.driver_state = DriverState::NormalOperation;
        vec![last_status_error(0)]
    }

    fn handle_peek(&self, payload: &[u8]) -> Result<Vec<SpinelFrame>, MockError> {
        if payload.len() < 6 {
            return Ok(vec![last_status_error(9)]); // SPINEL_STATUS_PARSE_ERROR
        }
        let addr = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
        let count = u16::from_le_bytes([payload[4], payload[5]]);
        let clamped = (count as usize).min(MAX_PEEK_SIZE);

        let mut w = PackWriter::new();
        w.write_uint8(0);
        w.write_uint_packed(0);
        w.write_uint32(addr);
        w.write_uint16(clamped as u16);
        w.write_bytes(&vec![0u8; clamped]);
        Ok(vec![SpinelFrame::new(CMD_PEEK_RET, w.into_bytes())])
    }

    async fn handle_prop_value_get(
        &mut self,
        payload: &[u8],
    ) -> Result<Vec<SpinelFrame>, MockError> {
        let mut r = PackReader::new(payload);
        let prop_key = r
            .read_uint_packed()
            .map_err(|_| MockError::InvalidPayload("missing property key in GET".into()))?;

        for rule in &self.failure_rules {
            if let FailureRule::RejectProperty { prop_key: p } = rule {
                if *p == prop_key {
                    return Ok(vec![last_status_error(1)]);
                }
            }
        }

        let value = if prop_key == PROP_MAC_SCAN_MASK {
            self.props.get(prop_key).unwrap_or(&[0u8; 0]).to_vec()
        } else if prop_key == PROP_MCU_POWER_STATE {
            encode_u8(0)
        } else if let Some(v) = self.props.get(prop_key) {
            v.to_vec()
        } else if is_vendor_property(prop_key) {
            vec![]
        } else {
            tracing::warn!("Mock NCP: unknown GET property 0x{prop_key:04X}");
            return Ok(vec![last_status_error(1)]);
        };

        Ok(vec![prop_value_is(prop_key, value)])
    }

    async fn handle_prop_value_set(
        &mut self,
        payload: &[u8],
    ) -> Result<Vec<SpinelFrame>, MockError> {
        let mut r = PackReader::new(payload);
        let prop_key = r
            .read_uint_packed()
            .map_err(|_| MockError::InvalidPayload("missing property key in SET".into()))?;
        let value_bytes = r
            .read_bytes(r.remaining())
            .map(|b| b.to_vec())
            .unwrap_or_default();

        self.props.set(prop_key, value_bytes.clone());
        let mut frames = vec![prop_value_is(prop_key, value_bytes.clone())];

        match prop_key {
            PROP_NET_IF_UP => {
                let val = value_bytes.first().copied().unwrap_or(0) != 0;
                if !val {
                    self.ncp_state = NcpState::Offline;
                }
            }
            PROP_NET_STACK_UP => {
                let val = value_bytes.first().copied().unwrap_or(0) != 0;
                if val && self.ncp_state != NcpState::Associated && !self.ncp_state.is_fault() {
                    self.driver_state = DriverState::NormalOperation;
                    self.ncp_state = NcpState::Associated;
                } else if !val {
                    self.ncp_state = NcpState::Offline;
                }
            }
            PROP_MAC_SCAN_STATE => {
                let scan_state = value_bytes.first().copied().unwrap_or(0);
                if scan_state == SCAN_STATE_BEACON {
                    let mask = self
                        .props
                        .get(PROP_MAC_SCAN_MASK)
                        .map(|b| b.to_vec())
                        .unwrap_or_default();
                    let beacons = self.topology.get_scan_beacons(&mask);
                    for beacon in &beacons {
                        frames.push(beacon.to_spinel_frame());
                    }
                    let mut w = PackWriter::new();
                    w.write_uint_packed(PROP_MAC_SCAN_STATE);
                    w.write_uint_packed(SCAN_STATE_IDLE as u32);
                    frames.push(SpinelFrame::new(CMD_PROP_VALUE_IS, w.into_bytes()));
                }
            }
            _ => {
                tracing::trace!("Mock NCP: unknown property SET {}", prop_key);
                frames.push(prop_value_is(
                    spinel::property::PROP_LAST_STATUS,
                    spinel::property::STATUS_PROP_NOT_FOUND
                        .to_le_bytes()
                        .to_vec(),
                ));
            }
        }

        Ok(frames)
    }

    // ---- State accessors ----

    pub fn ncp_state(&self) -> NcpState {
        self.ncp_state
    }

    pub fn driver_state(&self) -> DriverState {
        self.driver_state
    }

    pub fn is_associated(&self) -> bool {
        self.ncp_state == NcpState::Associated
    }

    pub fn config(&self) -> &MockConfig {
        &self.config
    }

    pub fn caps(&self) -> &CapabilitySet {
        &self.caps
    }

    pub fn framer(&mut self) -> &mut dcu_serial::FramedTransport<T> {
        &mut self.framed
    }
}
