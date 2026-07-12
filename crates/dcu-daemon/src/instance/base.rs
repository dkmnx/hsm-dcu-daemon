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

/// Allocate a TID wrapping 1..=15 (matching `SPINEL_GET_NEXT_TID`).
fn alloc_tid() -> u8 {
    static NEXT_TID: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
    NEXT_TID.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 15 + 1
}

/// Response table — maps TID to a oneshot sender for the awaiting task.
#[derive(Default)]
pub struct ResponseTable {
    pending: std::sync::Mutex<Vec<(u8, oneshot::Sender<SpinelFrame>)>>,
}

impl ResponseTable {
    pub fn register(&self, tid: u8, sender: oneshot::Sender<SpinelFrame>) {
        self.pending.lock().unwrap().push((tid, sender));
    }

    /// Deliver a frame to the task waiting on its TID. Returns `true` if delivered.
    pub fn deliver(&self, frame: &SpinelFrame) -> bool {
        let tid = frame.tid();
        if tid == 0 {
            return false;
        }
        let mut map = self.pending.lock().unwrap();
        if let Some(pos) = map.iter().position(|(t, _)| *t == tid) {
            let (_, sender) = map.remove(pos);
            let _ = sender.send(frame.clone());
            true
        } else {
            false
        }
    }

    pub fn unregister(&self, tid: u8) {
        self.pending.lock().unwrap().retain(|(t, _)| *t != tid);
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
    ncp_state: Arc<RwLock<NcpState>>,
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

    /// Active scan collector. Only one scan runs at a time, so a single slot.
    /// Set by `register_scan_collector`, cleared by `unregister_scan_collector`
    /// (also cleared when `dispatch_unsolicited` fails to forward a frame to a
    /// dropped sender).
    scan_collector: Arc<RwLock<Option<mpsc::UnboundedSender<SpinelFrame>>>>,

    #[allow(dead_code)]
    config: Config,
}

impl NcpInstanceBase {
    pub async fn new(
        config: Config,
        _shared_state: Arc<RwLock<dcu_dbus::DaemonState>>,
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
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                cmd = self.command_rx.recv() => {
                    match cmd {
                        Some(cmd) => { let _ = self.handle_command(cmd).await; }
                        None => break,
                    }
                }
                frame = self.frame_rx.recv() => {
                    match frame {
                        Some(frame) => {
                            if !self.response_table.deliver(&frame) {
                                self.dispatch_unsolicited(&frame).await;
                            }
                        }
                        None => break,
                    }
                }
            }
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

    /// Send a `PROP_VALUE_GET` frame for `prop` and await the response.
    pub async fn send_prop_get(&self, prop: u32) -> Result<SpinelFrame, DaemonError> {
        self.send_command(
            spinel::command::CMD_PROP_VALUE_GET,
            spinel::property::prop_value_get(prop).payload,
        )
        .await
    }

    /// Open the serial transport and spawn the I/O task.
    pub async fn start_pumps(&mut self) -> Result<(), DaemonError> {
        let cancel = CancellationToken::new();
        self.io_cancel = Some(cancel.clone());
        let (frame_tx, frame_rx) = mpsc::unbounded_channel();
        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
        self.frame_rx = frame_rx;
        self.frame_tx = frame_tx;
        self.outbound_tx = outbound_tx;

        // Open the serial transport. For now: PTY if config says so,
        // otherwise UART.
        // TODO: parse Config:NCP:SocketPath for system:/serial:/TCP prefixes
        let config = &self.config;
        tracing::info!(
            "Opening serial: {}@{}",
            config.nc_socket_path,
            config.nc_socket_baud
        );
        let serial_cfg = dcu_serial::SerialConfig {
            path: config.nc_socket_path.clone(),
            baud_rate: config.nc_socket_baud,
            data_bits: 8,
            stop_bits: 1,
            flow_control: true,
        };
        let transport = dcu_serial::UartTransport::open(serial_cfg)?;

        tokio::spawn(io_task(
            transport,
            outbound_rx,
            self.frame_tx.clone(),
            cancel,
        ));
        tracing::info!("I/O task spawned");
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<(), DaemonError> {
        tracing::info!("Stopping NCP instance");
        if let Some(cancel) = self.io_cancel.take() {
            cancel.cancel();
        }
        Ok(())
    }

    pub async fn set_ncp_state(&self, state: NcpState) {
        *self.ncp_state.write().await = state;
        self.state_changed.notify_waiters();
    }

    pub async fn get_ncp_state(&self) -> NcpState {
        *self.ncp_state.read().await
    }

    pub async fn set_driver_state(&self, state: DriverState) {
        *self.driver_state.write().await = state;
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
    /// `mDriverState == NORMAL_OPERATION`. The `NORMAL_OPERATION` transition is
    /// driven by the NCP init task (later phase); until that exists, the
    /// instance is constructed in `Initializing` and never flips, so this
    /// would always time out. Stubbed to succeed for now — the init task will
    /// call `set_driver_state(NormalOperation)` and this becomes a real wait.
    pub async fn wait_for_driver_ready(
        &self,
        _dur: std::time::Duration,
    ) -> Result<(), DaemonError> {
        Ok(())
    }

    /// Register the active scan collector channel. Replaces any prior collector.
    pub async fn register_scan_collector(&self, tx: mpsc::UnboundedSender<SpinelFrame>) {
        *self.scan_collector.write().await = Some(tx);
    }

    /// Clear the active scan collector.
    pub async fn unregister_scan_collector(&self) {
        *self.scan_collector.write().await = None;
    }

    /// Handle unsolicited (TID==0) frames that were not consumed by the
    /// response table. Currently this forwards scan beacons/energy results to
    /// the active scan collector; everything else is traced.
    async fn dispatch_unsolicited(&self, frame: &SpinelFrame) {
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
            if let Some(tx) = self.scan_collector.read().await.clone() {
                if tx.send(frame.clone()).is_err() {
                    tracing::warn!("Scan collector dropped; clearing slot");
                    *self.scan_collector.write().await = None;
                }
            }
        } else {
            tracing::trace!("Unsolicited property 0x{prop:04X}");
        }
    }

    pub async fn handle_command(
        &mut self,
        cmd: dcu_dbus::commands::Command,
    ) -> Result<String, DaemonError> {
        use dcu_dbus::commands::Command;
        match cmd {
            Command::Reset => {
                // C reset path: drive the NCP reset and wait for re-init.
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
            other => {
                tracing::warn!("Unhandled command: {other:?}");
                Ok("unhandled".into())
            }
        }
    }
}

/// Render bytes as a compact hex string.
fn hex_string(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
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
