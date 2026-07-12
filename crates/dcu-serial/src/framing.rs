//! HDLC-framed transport wrapping any [`Transport`] with Spinel frame
//! encode/decode.
//!
//! Ships Spinel frames over the wire as: `FLAG + escaped(data + CRC) + FLAG`.
//! The HDLC codec (`HdlcEncoder`/`HdlcDecoder`) lives in `spinel::hdlc` and
//! is *not* re-implemented here — this module just adapts it for async I/O.

use spinel::frame::SpinelFrame;
use spinel::hdlc::{HdlcDecoder, HdlcEncoder};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::SerialError;
use crate::transport::Transport;

/// Maximum HDLC frame size the decoder will accept (bytes of raw wire data).
/// Frames exceeding this are discarded to bound memory usage.
const MAX_FRAME_SIZE: usize = 4096;

/// An async HDLC-framed transport around any [`Transport`].
///
/// Wraps the read/write directions with Spinel frame encode/decode using the
/// HDLC codec from the `spinel` crate.
pub struct FramedTransport<T: Transport> {
    inner: T,
    encoder: HdlcEncoder,
    decoder: HdlcDecoder,
    read_buf: Vec<u8>,
}

impl<T: Transport> FramedTransport<T> {
    /// Create a new framed transport wrapping `inner`.
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            encoder: HdlcEncoder::new(),
            decoder: HdlcDecoder::new(),
            read_buf: Vec::with_capacity(256),
        }
    }

    /// Send a Spinel frame, HDLC-encoded, to the NCP.
    ///
    /// Wire format: `FLAG + escaped(header + packed_command + payload + CRC) + FLAG`.
    pub async fn send_frame(&mut self, frame: &SpinelFrame) -> Result<(), SerialError> {
        let wire = self.encoder.encode_frame(frame);
        self.inner.write_all(&wire).await?;
        self.inner.flush().await?;
        Ok(())
    }

    /// Receive a Spinel frame, decoding HDLC framing.
    ///
    /// Reads bytes from the underlying transport and feeds them through the
    /// HDLC decoder until a complete frame (with valid CRC) is produced.
    pub async fn recv_frame(&mut self) -> Result<SpinelFrame, SerialError> {
        loop {
            // If we have data in the buffer, check if the decoder can yield
            // a complete frame before reading more bytes.
            if !self.read_buf.is_empty() {
                // Feed each buffered byte through the decoder.
                let mut pending = std::mem::take(&mut self.read_buf);
                for byte in &pending {
                    if let Some(result) = self.decoder.feed_byte(*byte) {
                        let frame_data = result?;
                        // Put back any remaining bytes (shouldn't happen
                        // with this pattern, but be safe).
                        #[allow(clippy::needless_range_loop)]
                        for i in 1..pending.len() {
                            self.read_buf.push(pending[i]);
                        }
                        pending.clear();
                        return Ok(SpinelFrame::decode(&frame_data)?);
                    }
                }
                // All buffered bytes consumed; decoder still waiting.
                pending.clear();
            }

            // Read more bytes from the transport.
            let mut chunk = [0u8; 256];
            let n = self.inner.read(&mut chunk).await?;
            if n == 0 {
                return Err(SerialError::Framing("connection closed".into()));
            }

            // Guard against runaway growth.
            if self.read_buf.len() + n > MAX_FRAME_SIZE {
                // Discard accumulated bytes and re-sync (look for next FLAG).
                self.read_buf.clear();
                self.decoder = HdlcDecoder::new();
                // Still feed the new bytes in case they contain a frame
                // boundary.
                self.read_buf.extend_from_slice(&chunk[..n]);
                continue;
            }

            self.read_buf.extend_from_slice(&chunk[..n]);
        }
    }

    /// Borrow the underlying transport.
    pub fn inner(&self) -> &T {
        &self.inner
    }

    /// Mutably borrow the underlying transport.
    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Consume self, returning the inner transport.
    pub fn into_inner(self) -> T {
        self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spinel::frame::SpinelFrame;

    /// A minimal in-memory transport for unit tests.
    /// Uses separate read/write buffers to avoid borrow conflicts.
    struct TestTransport {
        read_buf: Vec<u8>,
        write_buf: Vec<u8>,
    }

    impl TestTransport {
        fn new(data: Vec<u8>) -> Self {
            Self {
                read_buf: data,
                write_buf: Vec::new(),
            }
        }
    }

    impl tokio::io::AsyncRead for TestTransport {
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            let this = self.get_mut();
            let n = buf.remaining().min(this.read_buf.len());
            let tail = this.read_buf.split_off(n);
            let consumed = std::mem::replace(&mut this.read_buf, tail);
            buf.put_slice(&consumed);
            std::task::Poll::Ready(Ok(()))
        }
    }

    impl tokio::io::AsyncWrite for TestTransport {
        fn poll_write(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            buf: &[u8],
        ) -> std::task::Poll<std::io::Result<usize>> {
            self.get_mut().write_buf.extend_from_slice(buf);
            std::task::Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn poll_shutdown(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Ok(()))
        }
    }

    impl Transport for TestTransport {
        fn info(&self) -> String {
            "test".into()
        }
    }

    #[tokio::test]
    async fn framing_send_recv_round_trip() {
        let mut framed = FramedTransport::new(TestTransport::new(Vec::new()));
        let frame = SpinelFrame::new(0x06, vec![0x00, 0x01]);
        framed.send_frame(&frame).await.unwrap();

        // Extract the written bytes, then drop framed to free the borrow.
        let wire = std::mem::take(&mut framed.inner_mut().write_buf);
        drop(framed);

        let mut recv = FramedTransport::new(TestTransport::new(wire));
        let received = recv.recv_frame().await.unwrap();
        assert_eq!(received.command_id, 0x06);
        assert_eq!(received.payload, vec![0x00, 0x01]);
    }

    #[tokio::test]
    async fn framing_decode_crc_error() {
        let mut framed = FramedTransport::new(TestTransport::new(Vec::new()));
        let frame = SpinelFrame::new(0x06, vec![0x00, 0x01]);
        framed.send_frame(&frame).await.unwrap();

        let mut wire = std::mem::take(&mut framed.inner_mut().write_buf);
        drop(framed);

        if wire.len() > 3 {
            let corrupt_at = wire.len() - 3;
            wire[corrupt_at] ^= 0xFF;
        }

        let mut recv = FramedTransport::new(TestTransport::new(wire));
        let result = recv.recv_frame().await;
        assert!(result.is_err());
    }
}
