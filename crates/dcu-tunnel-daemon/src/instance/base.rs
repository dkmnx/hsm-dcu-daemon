//! Core NCP instance — async state machine, response table, event loop.
//!
//! Owns the transport handles and response table. Commands from D-Bus
//! arrive via `command_rx`. Incoming frames arrive from the I/O task
//! via `frame_rx`. Outbound frames go to the I/O task via `outbound_tx`.
//! The I/O task owns the serial transport and does HDLC encode/decode.

use std::collections::HashSet;
use std::sync::Arc;

use spinel::frame::SpinelFrame;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::sync::{Notify, RwLock, mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use wisun_types::DriverState;
use wisun_types::NcpState;

use crate::DaemonError;
use crate::config::Config;

const READ_BUF_SIZE: usize = 4096;

/// Returns `true` if `prop` is a dataset *container* property whose
/// `PROP_VALUE_IS` payload is the full operational dataset (`A(t(iD))` blob).
///
/// The inner dataset field keys (DATASET_*, NET_*, PHY_*, ...) are decoded
/// *from inside* that blob — they are not themselves dataset containers.
fn is_dataset_prop(prop: u32) -> bool {
    matches!(
        prop,
        spinel::property::PROP_THREAD_ACTIVE_DATASET
            | spinel::property::PROP_THREAD_PENDING_DATASET
    )
}

/// Allocate a TID wrapping 1..=15 (matching `SPINEL_GET_NEXT_TID`).
fn alloc_tid() -> u8 {
    static NEXT_TID: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
    NEXT_TID.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 15 + 1
}

/// Static variant of dataset decoding for use in the frame-processing task
/// (operates on cloned `Arc<RwLock<...>>` handles, not `&self`).
async fn handle_dataset_frame_static(
    frame: &SpinelFrame,
    active_dataset: &Arc<RwLock<crate::dataset::OperationalDataset>>,
    pending_dataset: &Arc<RwLock<crate::dataset::OperationalDataset>>,
    shared_state: &Arc<RwLock<dcu_dbus::DaemonState>>,
) -> bool {
    // Only intercept unsolicited (TID=0) dataset frames. TID-matched
    // responses must flow through the response table so the awaiting
    // task receives them.
    if frame.command_id != spinel::command::CMD_PROP_VALUE_IS || frame.tid() != 0 {
        return false;
    }
    let mut r = spinel::pack::PackReader::new(&frame.payload);
    let prop = match r.read_uint_packed() {
        Ok(p) => p,
        Err(_) => return false,
    };
    if !is_dataset_prop(prop) {
        return false;
    }
    let value = match r.read_bytes(r.remaining()) {
        Ok(v) => v,
        Err(_) => return false,
    };
    match crate::dataset::OperationalDataset::from_spinel_frame(value) {
        Ok(ds) => {
            let target = if prop == spinel::property::PROP_THREAD_PENDING_DATASET {
                pending_dataset
            } else {
                active_dataset
            };
            *target.write().await = ds;
            sync_dataset_to_state_static(active_dataset, pending_dataset, shared_state).await;
            true
        }
        Err(e) => {
            tracing::warn!("Failed to decode operational dataset frame: {e}");
            // Clear only the dataset being decoded (matches C per-instance clear).
            let target = if prop == spinel::property::PROP_THREAD_PENDING_DATASET {
                pending_dataset
            } else {
                active_dataset
            };
            *target.write().await = Default::default();
            sync_dataset_to_state_static(active_dataset, pending_dataset, shared_state).await;
            true
        }
    }
}

/// Static variant that mirrors active/pending datasets into DaemonState.
async fn sync_dataset_to_state_static(
    active_dataset: &Arc<RwLock<crate::dataset::OperationalDataset>>,
    pending_dataset: &Arc<RwLock<crate::dataset::OperationalDataset>>,
    shared_state: &Arc<RwLock<dcu_dbus::DaemonState>>,
) {
    let active = active_dataset.read().await;
    let pending = pending_dataset.read().await;
    let mut guard = shared_state.write().await;
    guard.dataset.clear();
    // Active dataset fields (Dataset:* keys)
    for key in crate::dataset::DATASET_PROPERTY_KEYS {
        if let Some(v) = active.property_string(key) {
            guard.dataset.insert(key.to_string(), v);
        }
    }
    // Pending dataset fields (Thread:PendingDataset:Dataset:* keys)
    for key in crate::dataset::DATASET_PROPERTY_KEYS {
        if let Some(v) = pending.property_string(key) {
            let pending_key = format!("Thread:PendingDataset:{key}");
            guard.dataset.insert(pending_key, v);
        }
    }
}

/// Response table — maps TID to a oneshot sender for the awaiting task.
#[derive(Default)]
pub struct ResponseTable {
    pending: std::sync::Mutex<Vec<(u8, oneshot::Sender<SpinelFrame>)>>,
}

impl ResponseTable {
    pub fn register(&self, tid: u8, sender: oneshot::Sender<SpinelFrame>) {
        self.pending
            .lock()
            .expect("response table mutex poisoned")
            .push((tid, sender));
    }

    /// Deliver a frame to the task waiting on its TID. Returns `true` if delivered.
    pub fn deliver(&self, frame: &SpinelFrame) -> bool {
        let tid = frame.tid();
        if tid == 0 {
            return false;
        }
        let mut map = self.pending.lock().expect("response table mutex poisoned");
        if let Some(pos) = map.iter().position(|(t, _)| *t == tid) {
            let (_, sender) = map.remove(pos);
            let _ = sender.send(frame.clone());
            true
        } else {
            false
        }
    }

    pub fn unregister(&self, tid: u8) {
        self.pending
            .lock()
            .expect("response table mutex poisoned")
            .retain(|(t, _)| *t != tid);
    }
}

/// Combined I/O task: owns the serial transport, reads bytes → HDLC decode →
/// `frame_tx`, and receives outbound frames from `outbound_rx` → HDLC encode → write.
async fn io_task<T: dcu_serial::Transport + Unpin>(
    mut transport: T,
    mut outbound_rx: mpsc::UnboundedReceiver<SpinelFrame>,
    frame_tx: mpsc::UnboundedSender<SpinelFrame>,
    cancel: CancellationToken,
) {
    let mut decoder = spinel::hdlc::HdlcDecoder::new();
    let mut encoder = spinel::hdlc::HdlcEncoder::new();
    let mut read_buf = [0u8; READ_BUF_SIZE];

    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => break,
            result = outbound_rx.recv() => {
                if let Some(frame) = result {
                    let wire = encoder.encode_frame(&frame);
                    if transport.write_all(&wire).await.is_err() {
                        break;
                    }
                } else { break; }
            }
            result = AsyncReadExt::read(&mut transport, &mut read_buf) => {
                match result {
                    Ok(0) => { tracing::warn!("Transport closed"); break; }
                    Ok(n) => {
                        for &byte in &read_buf[..n] {
                            if let Some(r) = decoder.feed_byte(byte) {
                                match r {
                                    Ok(frame_data) => {
                                        match SpinelFrame::decode(&frame_data) {
                                            Ok(frame) => { if frame_tx.send(frame).is_err() { return; } }
                                            Err(e) => tracing::error!("Frame decode: {e}"),
                                        }
                                    }
                                    Err(e) => tracing::warn!("HDLC error: {e}"),
                                }
                            }
                        }
                    }
                    Err(e) => { tracing::error!("Read error: {e}"); break; }
                }
            }
        }
    }
}

/// The base NCP instance.
pub struct NcpInstanceBase {
    pub(crate) ncp_state: Arc<RwLock<NcpState>>,
    interface_name: String,
    state_changed: Arc<Notify>,

    /// Driver-side readiness (Rust analogue of `mDriverState`).
    driver_state: Arc<RwLock<DriverState>>,

    /// NCP capability bit set, populated from `PROP_CAPS` during init.
    capabilities: Arc<RwLock<HashSet<u32>>>,

    pub(crate) response_table: Arc<ResponseTable>,

    command_rx: mpsc::Receiver<dcu_dbus::commands::Command>,

    frame_rx: mpsc::UnboundedReceiver<SpinelFrame>,
    frame_tx: mpsc::UnboundedSender<SpinelFrame>,
    outbound_tx: mpsc::UnboundedSender<SpinelFrame>,

    /// Cancellation token for the I/O task. Created by start_pumps(), consumed by stop().
    io_cancel: Option<CancellationToken>,

    /// Join handle for the frame-processing task spawned in `run()`.
    /// Cancelled/aborted via stop() and on every run() exit path so it
    /// never outlives the instance.
    frame_task: Option<tokio::task::JoinHandle<()>>,

    /// Active scan collector. Only one scan runs at a time, so a single slot.
    /// Set by `register_scan_collector`, cleared by `unregister_scan_collector`
    /// (also cleared when `dispatch_unsolicited` fails to forward a frame to a
    /// dropped sender).
    scan_collector: Arc<RwLock<Option<mpsc::UnboundedSender<SpinelFrame>>>>,

    /// Active operational dataset (phase 3C). Updated from
    /// `PROP_THREAD_ACTIVE_DATASET` Spinel frames.
    active_dataset: Arc<RwLock<crate::dataset::OperationalDataset>>,

    /// Pending operational dataset (phase 3C). Updated from
    /// `PROP_THREAD_PENDING_DATASET` Spinel frames.
    pending_dataset: Arc<RwLock<crate::dataset::OperationalDataset>>,

    /// Shared D-Bus daemon state. The frame task mirrors the live operational
    /// dataset into `DaemonState.dataset` so `Dataset:*` property reads stay
    /// current without the D-Bus layer depending on `dcu-tunnel-daemon` internals.
    shared_state: Arc<RwLock<dcu_dbus::DaemonState>>,

    /// Windowed backoff manager for unexpected NCP resets.
    /// Wired into the reset handler when unexpected-reset detection is added.
    #[allow(dead_code)]
    backoff: crate::tasks::backoff::BackoffManager,

    /// Notified when `driver_state` changes (used by `wait_for_driver_ready`).
    driver_state_changed: Arc<Notify>,

    config: Config,
}

impl NcpInstanceBase {
    pub async fn new(
        config: Config,
        shared_state: Arc<RwLock<dcu_dbus::DaemonState>>,
        command_rx: mpsc::Receiver<dcu_dbus::commands::Command>,
    ) -> Result<Self, DaemonError> {
        // Channels are created in start_pumps() alongside the I/O task.
        // Until then, build stub channels so run() doesn't panic.
        let (frame_tx, frame_rx) = mpsc::unbounded_channel();

        Ok(Self {
            ncp_state: Arc::new(RwLock::new(NcpState::Uninitialized)),
            interface_name: config.tun_interface_name.clone(),
            state_changed: Arc::new(Notify::new()),
            driver_state: Arc::new(RwLock::new(DriverState::Initializing)),
            capabilities: Arc::new(RwLock::new(HashSet::new())),
            response_table: Arc::new(ResponseTable::default()),
            command_rx,
            frame_rx,
            frame_tx,
            outbound_tx: mpsc::unbounded_channel().0, // stub — replaced in start_pumps
            io_cancel: None,
            scan_collector: Arc::new(RwLock::new(None)),
            frame_task: None,
            active_dataset: Arc::new(RwLock::new(crate::dataset::OperationalDataset::default())),
            pending_dataset: Arc::new(RwLock::new(crate::dataset::OperationalDataset::default())),
            shared_state,
            backoff: crate::tasks::backoff::BackoffManager::new(),
            driver_state_changed: Arc::new(Notify::new()),
            config,
        })
    }

    pub fn interface_name(&self) -> &str {
        &self.interface_name
    }

    /// Borrow the inbound frame sender (for the I/O task).
    pub fn frame_tx(&self) -> &mpsc::UnboundedSender<SpinelFrame> {
        &self.frame_tx
    }

    /// Borrow the outbound channel (for the I/O task).
    pub fn outbound_tx(&self) -> &mpsc::UnboundedSender<SpinelFrame> {
        &self.outbound_tx
    }

    pub fn response_table(&self) -> &Arc<ResponseTable> {
        &self.response_table
    }

    /// Main event loop — receives frames from the I/O task and delivers
    /// matching responses via the response table.
    pub async fn run(&mut self, cancel: CancellationToken) {
        tracing::info!("Starting NCP instance event loop");

        // Spawn a dedicated frame-processing task so that send_command()
        // (called from handle_command) can await responses without deadlocking
        // the main loop. The frame task reads frame_rx and delivers TID-matched
        // responses via the response table.
        let response_table = self.response_table.clone();
        let scan_collector = self.scan_collector.clone();
        let active_dataset = self.active_dataset.clone();
        let pending_dataset = self.pending_dataset.clone();
        let shared_state = self.shared_state.clone();
        let frame_rx = std::mem::replace(&mut self.frame_rx, mpsc::unbounded_channel().1);
        let frame_cancel = cancel.clone();
        self.frame_task = Some(tokio::spawn(async move {
            let mut frame_rx = frame_rx;
            loop {
                tokio::select! {
                    _ = frame_cancel.cancelled() => break,
                    frame = frame_rx.recv() => {
                        match frame {
                            Some(frame) => {
                                // Dataset updates are unsolicited (or GET
                                // responses) and not TID-matched; handle them
                                // before the response table.
                                if handle_dataset_frame_static(&frame, &active_dataset, &pending_dataset, &shared_state).await {
                                    continue;
                                }
                                if !response_table.deliver(&frame) {
                                    Self::dispatch_unsolicited_static(&frame, &scan_collector).await;
                                }
                            }
                            None => break,
                        }
                    }
                }
            }
        }));

        // Init: send CMD_RESET, query CAPS. On failure, leave the NCP in
        // a fault state rather than masking it as Offline — a dead/unresponsive
        // NCP must surface to readiness probes.
        if let Err(e) = self
            .send_command(spinel::command::CMD_RESET, Vec::new())
            .await
        {
            tracing::error!("NCP init reset failed: {e}");
            self.set_ncp_state(NcpState::Fault).await;
            self.drain_frame_task().await;
            return;
        }
        // Query capabilities from the NCP.
        match self.send_prop_get(spinel::property::PROP_CAPS).await {
            Ok(resp) => {
                let mut r = spinel::pack::PackReader::new(&resp.payload);
                let _ = r.read_uint_packed(); // skip property key
                let mut caps = HashSet::new();
                while r.remaining() > 0 {
                    if let Ok(cap) = r.read_uint_packed() {
                        caps.insert(cap);
                    }
                }
                self.set_capabilities(caps).await;
                tracing::info!("NCP capabilities: {:?}", self.capabilities.read().await);
            }
            Err(e) => {
                tracing::error!("Failed to query NCP capabilities: {e}");
                self.set_ncp_state(NcpState::Fault).await;
                self.drain_frame_task().await;
                return;
            }
        }
        self.set_ncp_state(NcpState::Offline).await;
        self.set_driver_state(DriverState::NormalOperation).await;

        // Populate DaemonState from NCP by querying essential properties.
        // This must happen before firmware check so the real NCP version is available.
        let init_props = [
            "NCP:Version",
            "NCP:ProtocolVersion",
            "NCP:InterfaceType",
            "NCP:HardwareAddress",
            "NCP:Channel",
            "NCP:Frequency",
            "NCP:CCAThreshold",
            "NCP:TXPower",
            "Network:Name",
            "Network:PANID",
            "Network:NodeType",
        ];
        // Populate DaemonState from NCP by querying essential properties.
        // Bounded by a single aggregate timeout to avoid blocking startup
        // for 5s × num_props on a slow NCP.
        let init_timeout = std::time::Duration::from_secs(10);
        let init_futs: Vec<_> = init_props
            .iter()
            .map(|p| self.query_and_update_daemon_state(p))
            .collect();
        match tokio::time::timeout(init_timeout, futures::future::join_all(init_futs)).await {
            Ok(results) => {
                for (name, result) in init_props.iter().zip(results) {
                    if let Err(e) = result {
                        tracing::debug!("Init query {name} failed: {e}");
                    }
                }
            }
            Err(_) => {
                tracing::warn!("Init property queries timed out after {init_timeout:?}");
            }
        }

        // Firmware check: if auto-update is enabled and a check command is
        // configured, verify NCP firmware version and upgrade if needed.
        if self.config.daemon_auto_firmware_update {
            if let (Some(check), Some(upgrade)) = (
                &self.config.firmware_check_command,
                &self.config.firmware_upgrade_command,
            ) {
                let ncp_version = self.shared_state.read().await.ncp_version.clone();
                if ncp_version.is_empty() {
                    tracing::warn!("Skipping firmware check: NCP version not available");
                } else {
                    match crate::firmware_upgrade::is_firmware_upgrade_required(check, &ncp_version)
                        .await
                    {
                        Ok(true) => {
                            tracing::info!("Firmware upgrade required, starting...");
                            if let Err(e) = crate::firmware_upgrade::upgrade_firmware(upgrade).await
                            {
                                tracing::error!("Firmware upgrade failed: {e}");
                                self.set_ncp_state(NcpState::Fault).await;
                            }
                        }
                        Ok(false) => {
                            tracing::info!("Firmware is up to date");
                        }
                        Err(e) => {
                            tracing::warn!("Firmware check failed: {e}");
                        }
                    }
                }
            }
        }

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                cmd = self.command_rx.recv() => {
                    match cmd {
                        Some(cmd) => { let _ = self.handle_command(cmd).await; }
                        None => break,
                    }
                }
            }
        }

        self.drain_frame_task().await;
    }

    /// Cancel and await the frame-processing task (if any).
    async fn drain_frame_task(&mut self) {
        if let Some(handle) = self.frame_task.take() {
            handle.abort();
            let _ = handle.await;
        }
    }

    /// Route unsolicited frames using Arc'd fields (callable from a spawned task).
    async fn dispatch_unsolicited_static(
        frame: &SpinelFrame,
        scan_collector: &std::sync::Arc<
            tokio::sync::RwLock<Option<mpsc::UnboundedSender<SpinelFrame>>>,
        >,
    ) {
        if frame.command_id != spinel::command::CMD_PROP_VALUE_IS {
            tracing::trace!("Unsolicited cmd={}", frame.command_id);
            return;
        }
        let prop = match spinel::pack::PackReader::new(&frame.payload).read_uint_packed() {
            Ok(p) => p,
            Err(_) => {
                tracing::warn!("Malformed PROP_VALUE_IS frame");
                return;
            }
        };
        if prop == spinel::property::PROP_MAC_SCAN_BEACON
            || prop == spinel::property::PROP_MAC_SCAN_STATE
        {
            if let Some(tx) = scan_collector.read().await.clone() {
                if tx.send(frame.clone()).is_err() {
                    tracing::warn!("Scan collector dropped; clearing slot");
                    *scan_collector.write().await = None;
                }
            }
        } else {
            tracing::trace!("Unsolicited property 0x{prop:04X}");
        }
    }

    /// Send a command frame and await the matching response by TID.
    pub async fn send_command(
        &self,
        command_id: u32,
        payload: Vec<u8>,
    ) -> Result<SpinelFrame, DaemonError> {
        let tid = alloc_tid();
        let header = spinel::frame::make_header(0, tid);
        let frame = SpinelFrame::with_header(header, command_id, payload);
        let (tx, rx) = oneshot::channel();
        self.response_table.register(tid, tx);

        if self.outbound_tx.send(frame).is_err() {
            self.response_table.unregister(tid);
            return Err(DaemonError::Ncp("I/O task not running".into()));
        }

        match tokio::time::timeout(std::time::Duration::from_secs(5), rx).await {
            Ok(Ok(resp)) => {
                // Central status check (M1): a SET/exec response arrives as
                // PROP_VALUE_IS(LAST_STATUS, status). Surface non-OK statuses
                // as errors so every task gets consistent error semantics
                // without re-implementing this per task (mirrors the C
                // vprocess_send_command status handling).
                if resp.command_id == spinel::command::CMD_PROP_VALUE_IS {
                    let mut r = spinel::pack::PackReader::new(&resp.payload);
                    if let Ok(prop) = r.read_uint_packed() {
                        if prop == spinel::property::PROP_LAST_STATUS {
                            // Spinel status is INT_PACKED (LEB128), not a fixed
                            // 4-byte int. A typical non-zero status (e.g. 112)
                            // packs to a single byte; read_uint_packed is the
                            // correct decoder. On a malformed payload, treat
                            // it as a parse error rather than OK (mirrors the
                            // C's SPINEL_STATUS_PARSE_ERROR default).
                            let status = r
                                .read_uint_packed()
                                .unwrap_or(spinel::property::SPINEL_STATUS_PARSE_ERROR)
                                as i32;
                            if status != 0 {
                                self.response_table.unregister(tid);
                                return Err(DaemonError::Ncp(format!(
                                    "NCP status {status}: {}",
                                    wisun_types::WpanError::from(status)
                                )));
                            }
                        }
                    }
                }
                Ok(resp)
            }
            Ok(Err(_)) => {
                self.response_table.unregister(tid);
                Err(DaemonError::Cancelled)
            }
            Err(_) => {
                self.response_table.unregister(tid);
                Err(DaemonError::Ncp("timeout".into()))
            }
        }
    }

    /// Send a `PROP_VALUE_SET` frame for `prop` with the given payload bytes.
    pub async fn send_prop_set(
        &self,
        prop: u32,
        payload: Vec<u8>,
    ) -> Result<SpinelFrame, DaemonError> {
        self.send_command(
            spinel::command::CMD_PROP_VALUE_SET,
            spinel::property::prop_value_set(prop, payload).payload,
        )
        .await
    }

    /// Send a `PROP_VALUE_INSERT` frame for `prop` (list append).
    pub async fn send_prop_insert(
        &self,
        prop: u32,
        payload: Vec<u8>,
    ) -> Result<SpinelFrame, DaemonError> {
        self.send_command(
            spinel::command::CMD_PROP_VALUE_INSERT,
            spinel::property::prop_value_set(prop, payload).payload,
        )
        .await
    }

    /// Send a `PROP_VALUE_REMOVE` frame for `prop` (list remove).
    pub async fn send_prop_remove(
        &self,
        prop: u32,
        payload: Vec<u8>,
    ) -> Result<SpinelFrame, DaemonError> {
        self.send_command(
            spinel::command::CMD_PROP_VALUE_REMOVE,
            spinel::property::prop_value_set(prop, payload).payload,
        )
        .await
    }

    /// Send a `PROP_VALUE_GET` frame for `prop` and await the response.
    pub async fn send_prop_get(&self, prop: u32) -> Result<SpinelFrame, DaemonError> {
        self.send_command(
            spinel::command::CMD_PROP_VALUE_GET,
            spinel::property::prop_value_get(prop).payload,
        )
        .await
    }

    /// Query a property by D-Bus name, returning the raw Spinel response payload.
    ///
    /// Looks up the handler map to find the Spinel prop ID, then sends a GET.
    /// Returns the response frame (caller parses the value bytes).
    pub async fn query_property_by_name(&self, name: &str) -> Result<SpinelFrame, DaemonError> {
        let handler = crate::instance::property_handlers::lookup(name)
            .ok_or_else(|| DaemonError::Ncp(format!("unknown property: {name}")))?;
        self.send_prop_get(handler.prop_id).await
    }

    /// Set a property on the NCP by D-Bus name.
    ///
    /// Serializes the Variant value to Spinel wire format and sends a SET.
    pub async fn set_property_by_name(
        &self,
        name: &str,
        value: dcu_dbus::types::Variant,
    ) -> Result<(), DaemonError> {
        use zbus::zvariant::Value;

        let handler = crate::instance::property_handlers::lookup(name)
            .ok_or_else(|| DaemonError::Ncp(format!("unknown property: {name}")))?;

        if handler.access == crate::instance::property_handlers::PropAccess::ReadOnly {
            return Err(DaemonError::Ncp(format!("property {name} is read-only")));
        }

        let wire_bytes = match &value {
            Value::Str(s) => s.as_bytes().to_vec(),
            Value::U8(n) => vec![*n],
            Value::U16(n) => n.to_le_bytes().to_vec(),
            Value::U32(n) => n.to_le_bytes().to_vec(),
            Value::I16(n) => (*n).to_le_bytes().to_vec(),
            Value::I32(n) => (*n).to_le_bytes().to_vec(),
            Value::Bool(b) => vec![u8::from(*b)],
            other => {
                return Err(DaemonError::Ncp(format!(
                    "unsupported value type for {name}: {other:?}"
                )));
            }
        };

        self.send_prop_set(handler.prop_id, wire_bytes).await?;
        Ok(())
    }

    /// Parse a Spinel GET response payload into a Variant.
    ///
    /// The response payload contains: packed_property_id + value_bytes.
    /// Skips the property ID and decodes the value based on the expected
    /// wire type for the given D-Bus property name.
    pub fn parse_prop_response(
        name: &str,
        payload: &[u8],
    ) -> Result<dcu_dbus::types::Variant, DaemonError> {
        use spinel::pack::PackReader;
        use zbus::zvariant::Value;

        let mut r = PackReader::new(payload);
        let _prop_id = r
            .read_uint_packed()
            .map_err(|e| DaemonError::Ncp(format!("failed to parse property ID: {e}")))?;

        let v: Value<'static> = match name {
            "NCP:Version" | "NCP:ProtocolVersion" | "NCP:InterfaceType" | "Network:Name" => {
                let s = r
                    .read_utf8()
                    .map_err(|e| DaemonError::Ncp(format!("failed to parse string: {e}")))?;
                Value::from(s)
            }
            "NCP:Channel" | "NCP:Region" | "NCP:ModeID" | "NCP:MCUPowerState"
            | "Network:NodeType" | "OperatingClass" | "NumChannels" | "UCDwellInterval"
            | "BCDwellInterval" | "UCChFunction" | "BCChFunction" | "MacFilterMode" => {
                let b = r
                    .read_uint8()
                    .map_err(|e| DaemonError::Ncp(format!("failed to parse u8: {e}")))?;
                Value::from(b)
            }
            "NCP:CCAThreshold" | "NCP:TXPower" | "NCP:RSSI" => {
                let b = r
                    .read_int8()
                    .map_err(|e| DaemonError::Ncp(format!("failed to parse i8: {e}")))?;
                Value::from(b as i16)
            }
            "Network:PANID" | "ChSpacing" => {
                let b = r
                    .read_uint16()
                    .map_err(|e| DaemonError::Ncp(format!("failed to parse u16: {e}")))?;
                Value::from(b)
            }
            "NCP:Frequency"
            | "Network:KeyIndex"
            | "Network:PartitionId"
            | "Network:KeySwitchGuardTime"
            | "BCInterval" => {
                let b = r
                    .read_uint32()
                    .map_err(|e| DaemonError::Ncp(format!("failed to parse u32: {e}")))?;
                Value::from(b)
            }
            "Interface:Up" | "Stack:Up" => {
                let b = r
                    .read_uint8()
                    .map_err(|e| DaemonError::Ncp(format!("failed to parse bool: {e}")))?;
                Value::from(b != 0)
            }
            "NCP:HardwareAddress"
            | "NCP:ExtendedAddress"
            | "Network:XPANID"
            | "Network:Key"
            | "Network:PSKc" => {
                let n = r.remaining();
                let bytes = r
                    .read_bytes(n)
                    .map_err(|e| DaemonError::Ncp(format!("failed to read bytes: {e}")))?;
                let hex: String = bytes.iter().map(|b| format!("{b:02X}")).collect();
                Value::from(hex)
            }
            "IPv6:MeshLocalPrefix" => {
                let n = r.remaining();
                let bytes = r
                    .read_bytes(n)
                    .map_err(|e| DaemonError::Ncp(format!("failed to read prefix: {e}")))?;
                if bytes.len() >= 8 {
                    let mut octets = [0u8; 16];
                    octets[..8].copy_from_slice(&bytes[..8]);
                    let addr = std::net::Ipv6Addr::from(octets);
                    Value::from(format!("{addr}/64"))
                } else {
                    let hex: String = bytes.iter().map(|b| format!("{b:02X}")).collect();
                    Value::from(hex)
                }
            }
            _ => {
                let n = r.remaining();
                let bytes = r
                    .read_bytes(n)
                    .map_err(|e| DaemonError::Ncp(format!("failed to read bytes: {e}")))?;
                let hex: String = bytes.iter().map(|b| format!("{b:02X}")).collect();
                Value::from(hex)
            }
        };

        Ok(v)
    }

    /// Query a property by D-Bus name and update DaemonState.
    ///
    /// Used during init to populate DaemonState from NCP values.
    pub async fn query_and_update_daemon_state(&self, name: &str) -> Result<(), DaemonError> {
        use std::str::FromStr;

        let resp = self.query_property_by_name(name).await?;
        let variant = Self::parse_prop_response(name, &resp.payload)?;

        let mut ds = self.shared_state.write().await;
        match name {
            "NCP:Version" => {
                ds.ncp_version = dcu_dbus::properties::variant_to_string(&variant);
            }
            "NCP:ProtocolVersion" => {
                ds.ncp_protocol_version = dcu_dbus::properties::variant_to_string(&variant);
            }
            "NCP:InterfaceType" => {
                ds.ncp_interface_type = dcu_dbus::properties::variant_to_string(&variant);
            }
            "NCP:HardwareAddress" => {
                let s = dcu_dbus::properties::variant_to_string(&variant);
                if let Ok(eui) = wisun_types::Eui64::from_str(&s) {
                    ds.hardware_address = eui;
                }
            }
            "NCP:Channel" => {
                if let zbus::zvariant::Value::U8(ch) = variant {
                    ds.channel = ch;
                }
            }
            "NCP:Frequency" => {
                if let zbus::zvariant::Value::U32(freq) = variant {
                    ds.frequency = freq;
                }
            }
            "NCP:CCAThreshold" => {
                if let zbus::zvariant::Value::I16(v) = variant {
                    ds.cca_threshold = v as i8;
                }
            }
            "NCP:TXPower" => {
                if let zbus::zvariant::Value::I16(v) = variant {
                    ds.tx_power = v as f64;
                }
            }
            "NCP:RSSI" => {
                if let zbus::zvariant::Value::I16(v) = variant {
                    ds.rssi = v as i32;
                }
            }
            "Network:Name" => {
                let s = dcu_dbus::properties::variant_to_string(&variant);
                if let Ok(name) = wisun_types::NetworkName::from_str(&s) {
                    ds.network_name = name;
                }
            }
            "Network:PANID" => {
                let s = dcu_dbus::properties::variant_to_string(&variant);
                if let Ok(pan) = wisun_types::PanId::from_str(&s) {
                    ds.pan_id = pan;
                }
            }
            "Network:NodeType" => {
                ds.node_type = dcu_dbus::properties::variant_to_string(&variant);
            }
            _ => {}
        }
        Ok(())
    }

    /// Open the serial transport and spawn the I/O task.
    pub async fn start_pumps(&mut self) -> Result<(), DaemonError> {
        let config = &self.config;
        tracing::info!(
            "Opening transport: {}@{}",
            config.nc_socket_path,
            config.nc_socket_baud
        );
        let transport =
            dcu_serial::dispatch::open_transport(&config.nc_socket_path, config.nc_socket_baud)
                .await?;
        tracing::info!("Transport opened: {}", transport.info());
        self.start_pumps_impl(transport).await
    }

    /// Shared pump setup: wire channels, spawn the I/O task over `transport`.
    ///
    /// Both `start_pumps` (production serial) and `start_pumps_with_transport`
    /// (tests) funnel through here so the channel wiring and task spawn
    /// cannot drift between the two paths.
    async fn start_pumps_impl<T: dcu_serial::Transport + Unpin>(
        &mut self,
        transport: T,
    ) -> Result<(), DaemonError> {
        let cancel = CancellationToken::new();
        self.io_cancel = Some(cancel.clone());
        let (frame_tx, frame_rx) = mpsc::unbounded_channel();
        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
        self.frame_rx = frame_rx;
        self.frame_tx = frame_tx;
        self.outbound_tx = outbound_tx;

        tokio::spawn(io_task(
            transport,
            outbound_rx,
            self.frame_tx.clone(),
            cancel,
        ));
        tracing::info!("I/O task spawned");
        Ok(())
    }

    /// Start I/O pumps over an existing transport, skipping the serial open.
    ///
    /// Used by integration tests to inject an in-memory mock NCP transport
    /// without touching the filesystem or hardware.
    #[cfg(feature = "test-util")]
    pub async fn start_pumps_with_transport<T: dcu_serial::transport::Transport + Unpin>(
        &mut self,
        transport: T,
    ) -> Result<(), DaemonError> {
        self.start_pumps_impl(transport).await
    }

    pub async fn stop(&mut self) -> Result<(), DaemonError> {
        tracing::info!("Stopping NCP instance");
        if let Some(cancel) = self.io_cancel.take() {
            cancel.cancel();
        }
        // Abort the frame-processing task so it can't outlive the instance.
        if let Some(handle) = self.frame_task.take() {
            handle.abort();
            let _ = handle.await;
        }
        Ok(())
    }

    pub async fn set_ncp_state(&self, state: NcpState) {
        let old = *self.ncp_state.read().await;
        if old == state {
            return;
        }
        tracing::info!("NCP state: {old} -> {state}");
        *self.ncp_state.write().await = state;

        // DeepSleep side effect must happen before we hold the lock
        // (it does async network I/O).
        if state == NcpState::DeepSleep
            && self
                .has_capability(spinel::property::CAP_MCU_POWER_STATE)
                .await
        {
            if let Err(e) = self
                .send_prop_set(
                    spinel::property::PROP_MCU_POWER_STATE,
                    vec![spinel::property::MCU_POWER_STATE_LOW_POWER],
                )
                .await
            {
                tracing::warn!("Failed to set MCU power state: {e}");
            }
        }

        // Acquire one write lock for all DaemonState updates
        {
            let mut ds = self.shared_state.write().await;
            ds.ncp_state = state;

            match state {
                NcpState::Offline => {
                    ds.is_connected = false;
                    ds.is_commissioned = false;
                }
                NcpState::Associated | NcpState::Isolated => {
                    ds.is_connected = true;
                }
                NcpState::Fault => {
                    tracing::error!("NCP entered FAULT state");
                }
                NcpState::Uninitialized => {
                    ds.is_connected = false;
                    ds.is_commissioned = false;
                    ds.network_name = wisun_types::NetworkName(String::new());
                    ds.pan_id = wisun_types::PanId::DEFAULT;
                    ds.xpan_id.clear();
                    ds.network_key.clear();
                }
                _ => {}
            }
        }
        self.state_changed.notify_waiters();
    }

    pub async fn get_ncp_state(&self) -> NcpState {
        *self.ncp_state.read().await
    }

    pub async fn set_driver_state(&self, state: DriverState) {
        tracing::info!("Driver state: {:?}", state);
        *self.driver_state.write().await = state;
        self.driver_state_changed.notify_waiters();
    }

    pub async fn get_driver_state(&self) -> DriverState {
        *self.driver_state.read().await
    }

    /// Replace the NCP capability set (called during init from `PROP_CAPS`).
    pub async fn set_capabilities(&self, caps: HashSet<u32>) {
        *self.capabilities.write().await = caps;
    }

    /// Returns `true` if the NCP advertises `cap` (e.g. `CAP_MCU_POWER_STATE`).
    pub async fn has_capability(&self, cap: u32) -> bool {
        self.capabilities.read().await.contains(&cap)
    }

    /// Wait until `pred(get_ncp_state())` is true, or time out.
    ///
    /// Replaces the C `EH_REQUIRE_WITHIN(secs, cond, on_error)` state guard.
    /// Uses an absolute deadline (not a per-notification reset) so the total
    /// wait never exceeds `dur`, matching the C semantics.
    pub async fn wait_for_state<F>(
        &self,
        pred: F,
        dur: std::time::Duration,
    ) -> Result<(), DaemonError>
    where
        F: Fn(NcpState) -> bool,
    {
        if pred(self.get_ncp_state().await) {
            return Ok(());
        }
        let deadline = tokio::time::Instant::now() + dur;
        loop {
            let notified = self.state_changed.notified();
            tokio::pin!(notified);
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Err(DaemonError::Ncp("timeout waiting for NCP state".into()));
            }
            let _ = tokio::time::timeout(remaining, &mut notified).await;
            if pred(self.get_ncp_state().await) {
                return Ok(());
            }
        }
    }

    /// Wait until the driver reaches `NormalOperation`, or time out.
    ///
    /// Replaces the C task final-wait clause
    /// `mDriverState == NORMAL_OPERATION`. Called after NCP init completes.
    /// Returns `Ok(())` when ready, or `Err(DaemonError::Cancelled)` on timeout.
    pub async fn wait_for_driver_ready(&self, dur: std::time::Duration) -> Result<(), DaemonError> {
        let deadline = tokio::time::Instant::now() + dur;
        loop {
            if *self.driver_state.read().await == DriverState::NormalOperation {
                return Ok(());
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Err(DaemonError::Cancelled);
            }
            tokio::select! {
                _ = self.driver_state_changed.notified() => {}
                _ = tokio::time::sleep(remaining) => {
                    return Err(DaemonError::Cancelled);
                }
            }
        }
    }

    /// Register the active scan collector channel. Replaces any prior collector.
    pub async fn register_scan_collector(&self, tx: mpsc::UnboundedSender<SpinelFrame>) {
        *self.scan_collector.write().await = Some(tx);
    }

    /// Clear the active scan collector.
    pub async fn unregister_scan_collector(&self) {
        *self.scan_collector.write().await = None;
    }

    pub async fn handle_command(
        &mut self,
        cmd: dcu_dbus::commands::Command,
    ) -> Result<String, DaemonError> {
        use dcu_dbus::commands::Command;

        // Validate command against current NCP state before dispatching.
        let ncp_state = self.get_ncp_state().await;
        crate::control_interface::validate_command(&cmd, ncp_state)?;

        match cmd {
            Command::Reset => {
                // Explicit operator reset — no backoff delay or count.
                // BackoffManager tracks unexpected/NCP-driven resets only.
                self.set_ncp_state(NcpState::Uninitialized).await;
                self.send_command(spinel::command::CMD_RESET, Vec::new())
                    .await?;
                self.wait_for_state(|s| !s.is_initializing(), std::time::Duration::from_secs(5))
                    .await?;
                Ok(format!("NCP:State: {}", self.get_ncp_state().await))
            }
            Command::Leave => {
                crate::tasks::leave::leave(self).await?;
                Ok("Left network".into())
            }
            Command::Form { params } => {
                crate::tasks::form::form(self, &params).await?;
                Ok("Formed network".into())
            }
            Command::Join { params } => {
                crate::tasks::join::join(self, &params).await?;
                Ok("Joined network".into())
            }
            Command::BeginLowPower => {
                crate::tasks::sleep::deep_sleep(self).await?;
                Ok("Entered low-power".into())
            }
            Command::HostDidWake => {
                crate::tasks::sleep::host_did_wake(self, true).await?;
                Ok("Host woke".into())
            }
            Command::Peek { params } => {
                let addr = crate::tasks::params::get_u32(&params, "address")
                    .ok_or_else(|| DaemonError::Ncp("peek requires address".into()))?;
                let count = crate::tasks::params::get_u16(&params, "count")
                    .ok_or_else(|| DaemonError::Ncp("peek requires count".into()))?;
                let data = crate::tasks::peek::peek(self, addr, count).await?;
                Ok(format!("peek({addr:#x}, {count}): {}", hex_string(&data)))
            }
            // --- Scan commands ---
            Command::NetScanStart { params } => {
                let mask = extract_channel_mask(&params)?;
                let results = crate::tasks::scan::scan(self, &mask).await?;
                Ok(format!("Scan complete: {} beacons", results.len()))
            }
            Command::DiscoverScanStart { params } => {
                let mask = extract_channel_mask(&params)?;
                let results = crate::tasks::scan::scan(self, &mask).await?;
                Ok(format!("Discover scan complete: {} beacons", results.len()))
            }
            Command::EnergyScanStart { params } => {
                let mask = extract_channel_mask(&params)?;
                let results = crate::tasks::scan::scan(self, &mask).await?;
                Ok(format!("Energy scan complete: {} beacons", results.len()))
            }
            Command::NetScanStop | Command::DiscoverScanStop | Command::EnergyScanStop => {
                // Scan stop is timeout-based; the scan task will finish on its own.
                tracing::info!("Scan stop requested (timeout-based)");
                Ok("Scan stop acknowledged".into())
            }
            // --- Property operations ---
            Command::SetProperty { name, value } => {
                match self.set_property_by_name(&name, value).await {
                    Ok(()) => Ok(format!("Set {name}")),
                    Err(e) => {
                        tracing::warn!("SetProperty {name} failed: {e}");
                        Err(e)
                    }
                }
            }
            Command::GetProperty { name, reply } => {
                let result = match self.query_property_by_name(&name).await {
                    Ok(resp) => Self::parse_prop_response(&name, &resp.payload),
                    Err(e) => Err(e),
                };
                let _ = reply
                    .send(result.map_err(|e| dcu_dbus::types::DbusError::Transport(e.to_string())));
                Ok("GetProperty dispatched".into())
            }
            Command::DataPoll => {
                // Trigger a data poll by reading the poll timeout property.
                self.send_prop_get(spinel::property::PROP_MAC_DATA_POLL_PERIOD)
                    .await?;
                Ok("Data poll".into())
            }
            // --- Passthrough commands (send Spinel SET to NCP) ---
            Command::MlrRequest { params } => {
                let payload = serialize_params(&params);
                self.send_prop_set(spinel::property::PROP_THREAD_MLR_REQUEST, payload)
                    .await?;
                Ok("MLR request sent".into())
            }
            Command::BackboneRouterConfig { params } => {
                let payload = serialize_params(&params);
                self.send_prop_set(spinel::property::PROP_THREAD_BBR_STATE, payload)
                    .await?;
                Ok("BBR config sent".into())
            }
            Command::AnnounceBegin { params } => {
                let payload = serialize_params(&params);
                self.send_prop_set(
                    spinel::property::PROP_MESHCOP_COMMISSIONER_ANNOUNCE_BEGIN,
                    payload,
                )
                .await?;
                Ok("Announce begin sent".into())
            }
            Command::PanIdQuery { params } => {
                let payload = serialize_params(&params);
                self.send_prop_set(
                    spinel::property::PROP_MESHCOP_COMMISSIONER_PAN_ID_QUERY,
                    payload,
                )
                .await?;
                Ok("PAN ID query sent".into())
            }
            Command::GeneratePSKc { params } => {
                let payload = serialize_params(&params);
                self.send_prop_set(
                    spinel::property::PROP_MESHCOP_COMMISSIONER_GENERATE_PSKC,
                    payload,
                )
                .await?;
                Ok("PSKc generation sent".into())
            }
            Command::RouteAdd { params } => {
                let payload = serialize_params(&params);
                self.send_prop_insert(spinel::property::PROP_THREAD_OFF_MESH_ROUTES, payload)
                    .await?;
                Ok("Route added".into())
            }
            Command::RouteRemove { params } => {
                let payload = serialize_params(&params);
                self.send_prop_remove(spinel::property::PROP_THREAD_OFF_MESH_ROUTES, payload)
                    .await?;
                Ok("Route removed".into())
            }
            Command::ServiceAdd { params } => {
                let payload = serialize_params(&params);
                self.send_prop_insert(spinel::property::PROP_THREAD_SERVICE, payload)
                    .await?;
                Ok("Service added".into())
            }
            Command::ServiceRemove { params } => {
                let payload = serialize_params(&params);
                self.send_prop_remove(spinel::property::PROP_THREAD_SERVICE, payload)
                    .await?;
                Ok("Service removed".into())
            }
            Command::Poke { params } => {
                let addr = crate::tasks::params::get_u32(&params, "address")
                    .ok_or_else(|| DaemonError::Ncp("poke requires address".into()))?;
                let data = crate::tasks::params::get_bytes(&params, "data")
                    .ok_or_else(|| DaemonError::Ncp("poke requires data".into()))?;
                let mut payload = Vec::new();
                payload.extend_from_slice(&addr.to_le_bytes());
                payload.extend_from_slice(&(data.len() as u16).to_le_bytes());
                payload.extend_from_slice(&data);
                self.send_command(spinel::command::CMD_POKE, payload)
                    .await?;
                Ok(format!("poke({addr:#x}, {} bytes)", data.len()))
            }
            // --- Attach ---
            Command::Attach => {
                self.wait_for_state(|s| !s.is_initializing(), std::time::Duration::from_secs(5))
                    .await?;
                self.send_prop_set(spinel::property::PROP_NET_STACK_UP, vec![1u8])
                    .await?;
                Ok("Attached".into())
            }
            // --- ConfigGateway ---
            Command::ConfigGateway { params } => {
                crate::tasks::form::form(self, &params).await?;
                Ok("Gateway configured".into())
            }
            // --- Manufacturing passthrough ---
            Command::Mfg { command } => {
                // Forward the raw command string to the NCP via a
                // vendor-specific property (PROP_VENDOR__BEGIN range).
                let payload = command.as_bytes().to_vec();
                self.send_prop_set(spinel::property::PROP_VENDOR__BEGIN, payload)
                    .await?;
                Ok(format!("Mfg: {command}"))
            }
            // --- List property mutations ---
            Command::PropInsert { name, value } => {
                use zbus::zvariant::Value;
                let handler = crate::instance::property_handlers::lookup(&name)
                    .ok_or_else(|| DaemonError::Ncp(format!("unknown property: {name}")))?;
                let wire_bytes = match &value {
                    Value::Str(s) => s.as_bytes().to_vec(),
                    Value::U8(n) => vec![*n],
                    Value::U16(n) => n.to_le_bytes().to_vec(),
                    Value::U32(n) => n.to_le_bytes().to_vec(),
                    Value::I16(n) => (*n).to_le_bytes().to_vec(),
                    Value::Bool(b) => vec![u8::from(*b)],
                    other => {
                        let s = dcu_dbus::properties::variant_to_string(other);
                        s.into_bytes()
                    }
                };
                self.send_prop_insert(handler.prop_id, wire_bytes).await?;
                Ok(format!("Inserted into {name}"))
            }
            Command::PropRemove { name, value } => {
                use zbus::zvariant::Value;
                let handler = crate::instance::property_handlers::lookup(&name)
                    .ok_or_else(|| DaemonError::Ncp(format!("unknown property: {name}")))?;
                let wire_bytes = match &value {
                    Value::Str(s) => s.as_bytes().to_vec(),
                    Value::U8(n) => vec![*n],
                    Value::U16(n) => n.to_le_bytes().to_vec(),
                    Value::U32(n) => n.to_le_bytes().to_vec(),
                    Value::I16(n) => (*n).to_le_bytes().to_vec(),
                    Value::Bool(b) => vec![u8::from(*b)],
                    other => {
                        let s = dcu_dbus::properties::variant_to_string(other);
                        s.into_bytes()
                    }
                };
                self.send_prop_remove(handler.prop_id, wire_bytes).await?;
                Ok(format!("Removed from {name}"))
            }
        }
    }
}

/// Render bytes as a compact hex string.
fn hex_string(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

/// Extract a `ChannelMask` from scan command params.
///
/// The D-Bus scan methods pass a `ChannelMask` key as a string
/// (e.g. `"1:10,12:20"`) or as a hex uint32 bitmask.
///
/// NOTE: The u32 bitmask form only covers channels 0–31. For Wi-SUN
/// channels above 31 (902–928 MHz band), use the string form.
fn extract_channel_mask(
    params: &std::collections::HashMap<String, dcu_dbus::types::Variant>,
) -> Result<wisun_types::ChannelMask, DaemonError> {
    use zbus::zvariant::Value;
    // Try string channel mask first (e.g. "1:10,12:20")
    if let Some(Value::Str(s)) = params.get("ChannelMask") {
        return s
            .parse::<wisun_types::ChannelMask>()
            .map_err(|e| DaemonError::Ncp(format!("invalid ChannelMask: {e}")));
    }
    // Try uint32 bitmask (each bit = a channel)
    if let Some(Value::U32(bitmask)) = params.get("ChannelMask") {
        let mut mask = wisun_types::ChannelMask::empty();
        for ch in 0..32 {
            if bitmask & (1 << ch) != 0 {
                mask.set_channel(ch);
            }
        }
        return Ok(mask);
    }
    // Default: all Wi-SUN channels
    Ok(wisun_types::ChannelMask::all())
}

/// Serialize a D-Bus params HashMap to a Spinel wire payload.
///
/// Encodes each key-value pair as a packed string + value, suitable
/// for passing to `send_prop_set`. The exact wire format depends on
/// the property; this provides a generic string-based encoding.
fn serialize_params(
    params: &std::collections::HashMap<String, dcu_dbus::types::Variant>,
) -> Vec<u8> {
    use spinel::pack::PackWriter;
    use zbus::zvariant::Value;

    let mut w = PackWriter::new();
    for (key, value) in params {
        w.write_utf8(key);
        match value {
            Value::Str(s) => w.write_utf8(s),
            Value::U8(n) => w.write_uint8(*n),
            Value::U16(n) => w.write_uint16(*n),
            Value::U32(n) => w.write_uint32(*n),
            Value::Bool(b) => w.write_bool(*b),
            _ => {
                let s = dcu_dbus::properties::variant_to_string(value);
                w.write_utf8(&s);
            }
        }
    }
    w.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_table_deliver_match() {
        let table = ResponseTable::default();
        let (tx, mut rx) = oneshot::channel();
        let frame = SpinelFrame::with_header(spinel::frame::make_header(0, 5), 6, vec![]);
        table.register(5, tx);
        assert!(table.deliver(&frame));
        assert_eq!(rx.try_recv().unwrap().command_id, 6);
    }

    #[test]
    fn response_table_deliver_miss() {
        let table = ResponseTable::default();
        let frame = SpinelFrame::with_header(spinel::frame::make_header(0, 5), 6, vec![]);
        assert!(!table.deliver(&frame));
    }

    #[test]
    fn response_table_ignores_tid_zero() {
        let table = ResponseTable::default();
        let frame = SpinelFrame::with_header(spinel::frame::make_header(0, 0), 6, vec![]);
        assert!(!table.deliver(&frame));
    }

    #[test]
    fn tid_wraps_at_15() {
        let seen: Vec<u8> = (0..20).map(|_| alloc_tid()).collect();
        for &tid in &seen {
            assert!((1..=15).contains(&tid), "TID out of range: {tid}");
        }
        let unique: std::collections::HashSet<u8> = seen.iter().copied().collect();
        assert!(
            unique.len() <= 15,
            "expected ≤15 unique TIDs, got {}",
            unique.len()
        );
    }

    #[test]
    fn response_table_deliver_consumes_once() {
        let table = ResponseTable::default();
        let (tx, rx) = oneshot::channel();
        let frame = SpinelFrame::with_header(spinel::frame::make_header(0, 3), 6, vec![]);
        table.register(3, tx);
        assert!(table.deliver(&frame));
        assert!(!table.deliver(&frame));
        drop(rx);
    }
}
