//! Data pump — single serial reader that decodes HDLC frames and produces
//! [`NcpEvent`](super::NcpEvent)s for the task dispatcher.

use spinel::hdlc::HdlcDecoder;
use spinel::frame::SpinelFrame;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::DaemonError;

/// Buffer size for reading raw bytes from the serial transport.
const READ_BUF_SIZE: usize = 4096;

/// Reads bytes from a framed transport, decodes HDLC framing, and sends
/// complete [`NcpEvent::FrameReceived`](super::NcpEvent::FrameReceived)
/// events into the channel.
pub struct DataPump {
    decoder: HdlcDecoder,
}

impl Default for DataPump {
    fn default() -> Self {
        Self::new()
    }
}

impl DataPump {
    pub fn new() -> Self {
        Self {
            decoder: HdlcDecoder::new(),
        }
    }

    /// Run the data pump on `transport`, sending frames into `event_tx`.
    /// Cancels on `cancel` or when the transport returns EOF.
    pub async fn run<T: dcu_serial::Transport>(
        &mut self,
        transport: &mut T,
        event_tx: &mpsc::UnboundedSender<super::NcpEvent>,
        cancel: CancellationToken,
    ) -> Result<(), DaemonError> {
        let mut buf = [0u8; READ_BUF_SIZE];

        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => {
                    break;
                }
                result = tokio::io::AsyncReadExt::read(transport, &mut buf) => {
                    let n = result?;
                    if n == 0 {
                        tracing::warn!("Data pump: transport closed");
                        break;
                    }
                    for &byte in &buf[..n] {
                        if let Some(result) = self.decoder.feed_byte(byte) {
                            match result {
                                Ok(frame_data) => {
                                    match SpinelFrame::decode(&frame_data) {
                                        Ok(frame) => {
                                            if event_tx.send(super::NcpEvent::FrameReceived(frame)).is_err() {
                                                break;
                                            }
                                        }
                                        Err(e) => {
                                            tracing::error!("Frame decode error: {e}");
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("HDLC error: {e}");
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
