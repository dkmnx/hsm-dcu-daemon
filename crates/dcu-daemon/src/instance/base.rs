//! Core NCP instance — async state machine, response table, event loop.
//!
//! Owns the transport handles and response table. Commands from D-Bus
//! arrive via `command_rx`. Incoming frames arrive from the I/O task
//! via `frame_rx`. Outbound frames go to the I/O task via `outbound_tx`.
//! The I/O task owns the serial transport and does HDLC encode/decode.

use std::sync::Arc;

use spinel::frame::SpinelFrame;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, oneshot, Notify, RwLock};
use tokio_util::sync::CancellationToken;
use wisun_types::NcpState;

use crate::config::Config;
use crate::DaemonError;

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

    pub(crate) response_table: Arc<ResponseTable>,

    command_rx: mpsc::Receiver<dcu_dbus::commands::Command>,

    frame_rx: mpsc::UnboundedReceiver<SpinelFrame>,
    frame_tx: mpsc::UnboundedSender<SpinelFrame>,
    outbound_tx: mpsc::UnboundedSender<SpinelFrame>,

    /// Cancellation token for the I/O task. Created by start_pumps(), consumed by stop().
    io_cancel: Option<CancellationToken>,

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
            response_table: Arc::new(ResponseTable::default()),
            command_rx,
            frame_rx,
            frame_tx,
            outbound_tx: mpsc::unbounded_channel().0, // stub — replaced in start_pumps
            io_cancel: None,
            config,
        })
    }

    pub fn interface_name(&self) -> &str { &self.interface_name }

    /// Borrow the inbound frame sender (for the I/O task).
    pub fn frame_tx(&self) -> &mpsc::UnboundedSender<SpinelFrame> { &self.frame_tx }

    /// Borrow the outbound channel (for the I/O task).
    pub fn outbound_tx(&self) -> &mpsc::UnboundedSender<SpinelFrame> { &self.outbound_tx }

    pub fn response_table(&self) -> &Arc<ResponseTable> { &self.response_table }

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
                                tracing::trace!("Unsolicited: cmd={}", frame.command_id);
                            }
                        }
                        None => break,
                    }
                }
            }
        }
    }

    /// Send a command frame and await the matching response by TID.
    pub async fn send_command(&self, command_id: u32, payload: Vec<u8>) -> Result<SpinelFrame, DaemonError> {
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
            Ok(Ok(resp)) => Ok(resp),
            Ok(Err(_)) => { self.response_table.unregister(tid); Err(DaemonError::Cancelled) }
            Err(_) => { self.response_table.unregister(tid); Err(DaemonError::Ncp("timeout".into())) }
        }
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
        tracing::info!("Opening serial: {}@{}", config.nc_socket_path, config.nc_socket_baud);
        let serial_cfg = dcu_serial::SerialConfig {
            path: config.nc_socket_path.clone(),
            baud_rate: config.nc_socket_baud,
            data_bits: 8,
            stop_bits: 1,
            flow_control: true,
        };
        let transport = dcu_serial::UartTransport::open(serial_cfg)?;

        tokio::spawn(io_task(transport, outbound_rx, self.frame_tx.clone(), cancel));
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

    pub async fn get_ncp_state(&self) -> NcpState { *self.ncp_state.read().await }

    pub async fn handle_command(&mut self, cmd: dcu_dbus::commands::Command) -> Result<String, DaemonError> {
        match cmd {
            dcu_dbus::commands::Command::Reset => Ok(format!("NCP:State: {}", self.get_ncp_state().await)),
            dcu_dbus::commands::Command::Leave => { self.set_ncp_state(NcpState::Offline).await; Ok("Left network".into()) }
            other => { tracing::warn!("Unhandled command: {other:?}"); Ok("unhandled".into()) }
        }
    }
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
        for &tid in &seen { assert!((1..=15).contains(&tid), "TID out of range: {tid}"); }
        let unique: std::collections::HashSet<u8> = seen.iter().copied().collect();
        assert!(unique.len() <= 15, "expected ≤15 unique TIDs, got {}", unique.len());
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
