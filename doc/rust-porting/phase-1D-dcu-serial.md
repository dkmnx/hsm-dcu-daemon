# Phase 1D: `dcu-serial` — UART Transport

## Overview

Async serial/UART transport layer with HDLC framing. Communicates with the TI NCP hardware over UART (or PTY for testing).

**Replaces**: `src/util/SocketWrapper.*`, `src/util/SuperSocket.*`, `src/util/SocketAdapter.*`, `src/util/UnixSocket.*`

**Effort**: 3-4 days

**NOTE**: HDLC framing is in `spinel` crate (Phase 1B), sourced from `SpinelNCPInstance-DataPump.cpp:65-274`. This crate provides the transport layer (UART, PTY, Unix socket) and wraps the `spinel::hdlc::FramedTransport`.

## Source Files to Port

| C/C++ File                    | LOC  | What to Extract                               |
| ----------------------------- | ---- | --------------------------------------------- |
| `src/util/SocketWrapper.cpp`  | ~120 | Serial read/write, socket management          |
| `src/util/SocketWrapper.h`    | 40   | Socket wrapper class definition               |
| `src/util/SuperSocket.cpp`    | ~300 | Super socket abstraction                      |
| `src/util/SuperSocket.h`      | ~50  | SuperSocket class                             |
| `src/util/SocketAdapter.cpp`  | ~100 | Socket adapter interface                      |
| `src/util/SocketAdapter.h`    | ~30  | Adapter trait definition                      |
| `src/util/UnixSocket.cpp`     | ~200 | Unix domain socket transport                  |

**Total C/C++ code**: ~848 LOC

> **socket-utils.c transport dispatch (1,031 LOC).** The above files
> cover the individual socket types. `src/util/socket-utils.c` is the
> **transport dispatcher** that decides which socket to open based on
> the `Config:NCP:SocketPath` prefix. It implements three paths
> beyond raw device (covered by `uart.rs`):
>
> 1. **`system:` prefix** (`open_system_socket_forkpty`, line 418):
>    spawns the NCP as a **child process** behind a PTY via
>    double-fork + `forkpty`. This is how the daemon talks to a
>    software/mock NCP binary. Phase 1D's `pty.rs` covers *test*
>    PTYs; the production `system:` spawn path needs its own module
>    (e.g. `system.rs` using `nix::pty::openpty` + `tokio::process::Command`).
> 2. **`host:port` TCP** (`lookup_sockaddr_from_host_and_port`, line
>    290): TCP NCP transport for remote/networked NCPs. Add a
>    `tcp.rs` module using `tokio::net::TcpStream`.
> 3. **`socket_name_is_device` raw `/dev/tty*`** — covered by `uart.rs`.
>
> Also note: `sec-random.c` (60 LOC, `src/util/sec-random.c`) is used
> by `socket-utils.c` for entropy. In Rust, map to `ring::rand` or
> `OsRng`.

## Crate Structure

```text
dcu-serial/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── transport.rs        # Transport trait definition
    ├── uart.rs             # UART implementation (tokio-serial)
    ├── socket.rs           # Unix socket implementation
    ├── pty.rs              # PTY implementation (for mock NCP)
    └── framing.rs          # HDLC framing layer
```

## Detailed File Specs

### `transport.rs`

```rust
use std::os::unix::io::RawFd;
use tokio::io::{AsyncRead, AsyncWrite};

/// Transport abstraction for NCP communication.
/// Implementations: UART, Unix socket, PTY.
///
/// Note: native async traits (Rust 1.75+) — no `async_trait` crate
/// needed. The trait methods are all synchronous; async behavior comes
/// from the `AsyncRead`/`AsyncWrite` supertraits.
pub trait Transport: AsyncRead + AsyncWrite + Send + Unpin + 'static {
    /// Get the underlying file descriptor, if available (for event-loop
    /// integration / `AsyncFd` registration).
    fn raw_fd(&self) -> Option<RawFd> {
        None
    }

    /// Human-readable identifier for logging (e.g. "UART:/dev/ttyUSB0@115200").
    fn info(&self) -> String;
}

/// Configuration for serial transport.
#[derive(Debug, Clone)]
pub struct SerialConfig {
    pub path: String,            // e.g., "/dev/ttyUSB0"
    pub baud_rate: u32,          // default: 115200
    pub data_bits: u8,           // default: 8
    pub stop_bits: u8,           // default: 1
    pub flow_control: bool,      // default: true (RTS/CTS)
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            path: "/dev/ttyUSB0".into(),
            baud_rate: 115200,
            data_bits: 8,
            stop_bits: 1,
            flow_control: true,
        }
    }
}
```

### `uart.rs`

```rust
use tokio_serial::{SerialPortBuilderExt, SerialStream};

pub struct UartTransport {
    inner: SerialStream,
    config: SerialConfig,
}

impl UartTransport {
    pub fn open(config: SerialConfig) -> Result<Self, SerialError>;
}

impl Transport for UartTransport {
    fn raw_fd(&self) -> Option<RawFd> {
        Some(self.inner.as_raw_fd())
    }

    fn info(&self) -> String {
        format!("UART:{}@{}", self.config.path, self.config.baud_rate)
    }
}

impl AsyncRead for UartTransport { ... }
impl AsyncWrite for UartTransport { ... }
```

### `pty.rs`

For mock NCP testing:

```rust
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

pub struct PtyPair {
    pub master: Box<dyn portable_pty::MasterPty + Send>,
    pub slave_path: String,
}

impl PtyPair {
    /// Create a PTY pair. Returns master + slave path.
    pub fn open() -> Result<Self, SerialError>;

    /// Get the slave path (connect daemon to this).
    pub fn slave_path(&self) -> &str;
}

pub struct PtyTransport {
    reader: Box<dyn std::io::Read + Send>,
    writer: Box<dyn std::io::Write + Send>,
}

impl PtyTransport {
    pub fn from_pair(pair: &PtyPair) -> Result<Self, SerialError>;
}
```

### `framing.rs`

HDLC framing layer wrapping any Transport. HDLC codec is in `spinel::hdlc` (sourced from `SpinelNCPInstance-DataPump.cpp:65-274`).

```rust
use crate::transport::Transport;
use spinel::hdlc::{HdlcEncoder, HdlcDecoder};
use spinel::frame::SpinelFrame;

pub struct FramedTransport<T: Transport> {
    transport: T,
    encoder: HdlcEncoder,
    decoder: HdlcDecoder,
    read_buf: Vec<u8>,
}

impl<T: Transport> FramedTransport<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            encoder: HdlcEncoder::new(),
            decoder: HdlcDecoder::new(),
            read_buf: Vec::new(),
        }
    }

    /// Send a Spinel frame (HDLC encode + write).
    /// Wire: FLAG + escaped(header + packed_command + payload) + CRC(LE) + FLAG
    pub async fn send_frame(&mut self, frame: &SpinelFrame) -> Result<(), SerialError>;

    /// Receive a Spinel frame (read + HDLC decode).
    pub async fn recv_frame(&mut self) -> Result<SpinelFrame, SerialError>;

    /// Get reference to underlying transport.
    pub fn inner(&self) -> &T;
}
```

The HDLC framing from `SpinelNCPInstance-DataPump.cpp`:
1. `send_frame`: escape data, compute CRC-16 (init 0xFFFF), append CRC LE, wrap with FLAGs
2. `recv_frame`: strip FLAGs, unescape, verify CRC (init 0xFFFF, final XOR 0xFFFF)
3. CRC-16/X.25 (HDLC FCS-16): polynomial 0x1021, init 0xFFFF, refin=true, refout=true, xorout 0xFFFF — see phase-1B for crate compatibility warning.

## Tests

### Test 1: HDLC Encode/Decode Round-Trip

```rust
#[test]
fn hdlc_framing_round_trip() {
    let mut framed = FramedTransport::new(mock_transport());
    let frame = SpinelFrame::new(0x01, vec![0x02, 0x03, 0x04]);

    // Encode with HDLC framing
    let hdlc_encoded = framed.encoder.encode_frame(&frame);

    // Verify starts and ends with FLAG
    assert_eq!(hdlc_encoded[0], FLAG_BYTE);
    assert_eq!(*hdlc_encoded.last().unwrap(), FLAG_BYTE);

    // Decode and verify CRC
    let mut decoder = HdlcDecoder::new();
    let mut result = None;
    for byte in &hdlc_encoded {
        result = decoder.feed_byte(*byte);
    }
    let frame_data = result.unwrap().unwrap();
    let decoded = SpinelFrame::decode(&frame_data).unwrap();
    assert_eq!(decoded.command_id, 0x01);
}
```

### Test 2: PTY Loopback

```rust
#[tokio::test]
async fn pty_loopback() {
    let pty = PtyPair::open().unwrap();
    let mut framed = FramedTransport::new(
        PtyTransport::from_pair(&pty).unwrap()
    );

    let frame = SpinelFrame::new(0x06, vec![0x00, 0x01]);
    framed.send_frame(&frame).unwrap();

    // Read back from master side
    let mut buf = [0u8; 1024];
    let n = pty.master.read(&mut buf).unwrap();
    assert!(n > 0);
}
```

### Test 3: UART Configuration

```rust
#[test]
fn serial_config_defaults() {
    let config = SerialConfig::default();
    assert_eq!(config.baud_rate, 115200);
    assert_eq!(config.data_bits, 8);
    assert!(config.flow_control);
}
```

### Test 4: Escape Sequence Handling

```rust
#[test]
fn hdlc_escape_special_bytes() {
    let data = vec![FLAG_BYTE, ESCAPE_BYTE, XON, XOFF, SPECIAL_BYTE];
    let mut encoder = HdlcEncoder::new();
    let encoded = encoder.encode_bytes(&data);

    // Verify no unescaped FLAG bytes in middle
    let inner = &encoded[1..encoded.len()-1];
    assert!(!inner.contains(&0x7E));

    // Decode and verify round-trip
    let mut decoder = HdlcDecoder::new();
    let mut decoded = None;
    for byte in encoded {
        decoded = decoder.feed_byte(byte);
    }
    assert_eq!(decoded.unwrap().unwrap(), data);
}
```

### Test 5: Async Frame Exchange

```rust
#[tokio::test]
async fn async_frame_exchange() {
    let (master, slave) = create_async_pty_pair();
    let mut framed_send = FramedTransport::new(master);
    let mut framed_recv = FramedTransport::new(slave);

    let frame = SpinelFrame::new(0x10, vec![0xAA, 0xBB]);
    framed_send.send_frame(&frame).await.unwrap();

    let received = framed_recv.recv_frame().await.unwrap();
    assert_eq!(received.command_id, 0x10);
    assert_eq!(received.payload, vec![0xAA, 0xBB]);
}
```

## Dependencies

```toml
[dependencies]
spinel = { path = "../spinel" }
tokio = { version = "1", features = ["io-util", "net", "time"] }
tokio-serial = "5"
tokio-util = { version = "0.7", features = ["io"] }
thiserror = "2"
tracing = "0.1"

[dev-dependencies]
portable-pty = "0.8"
```

## Verification Checklist

- [ ] UART opens at correct baud rate (verify with `stty`)
- [ ] HDLC framing matches `SocketWrapper.cpp` output
- [ ] PTY loopback works for frame exchange
- [ ] Async read integrates with tokio event loop
- [ ] CRC validation catches corrupted frames
- [ ] `cargo test` passes
- [ ] `cargo clippy` produces zero warnings
- [ ] Only `uart.rs` and `pty.rs` contain `unsafe` (via tokio-serial)
