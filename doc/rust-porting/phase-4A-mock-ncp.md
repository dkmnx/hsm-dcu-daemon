# Phase 4A: `dcu-mock` — Mock NCP

## Overview

Mock NCP for integration testing. Emulates the TI Wi-SUN NCP over a PTY or an
in-memory transport so tests can run without physical hardware.

**Effort**: 3-5 days

## Purpose

- Enable CI testing without physical hardware
- Simulate network operations (scan, form, join)
- Inject failures for error handling tests
- Provide deterministic test scenarios
- Verify the daemon's Spinel/HDLC framing is byte-compatible with the real NCP

## What's already in place

- `dcu-serial` already has a `mock-pty` feature and `PtyPair`/`PtyTransport`
  (`crates/dcu-serial/src/pty.rs`). However, `PtyTransport`'s
  `AsyncRead`/`AsyncWrite` poll methods are currently **stubs** that return
  `Poll::Pending`. The mock crate can either:
  1. Complete the PTY implementation (using the raw PTY fd with `tokio::io::unix::AsyncFd`), or
  2. Use an in-memory `tokio::io::DuplexStream` pair for unit tests and a real PTY for integration tests.
  This phase should start with option 2 for speed, then wire up the real PTY.

- The `spinel` crate provides HDLC framing (`spinel::hdlc`) and frame
  builders (`spinel::property::prop_value_get/set/is`). The mock must speak
  exactly the same wire format as the daemon's `io_task`.

- **Important:** As established in phase 3B, TI Wi-SUN has **no dedicated
  `CMD_FORM`/`CMD_JOIN`/`CMD_SCAN`**. The mock responds to `CMD_RESET`,
  `CMD_NOOP`, `CMD_NET_CLEAR`, `CMD_PEEK`, `CMD_PROP_VALUE_GET`, and
  `CMD_PROP_VALUE_SET` (and their `IS`/`INSERTED`/`REMOVED` responses). Form,
  Join, Scan, Leave are implemented as NCP state-machine transitions driven by
  property sets (`NET_IF_UP`, `NET_STACK_UP`, `MAC_SCAN_STATE`, etc.).

## Crate Structure

```text
dcu-mock/
├── Cargo.toml
└── src/
    ├── lib.rs               # Re-exports
    ├── config.rs            # MockConfig, CapabilitySet
    ├── failure.rs           # FailureRule, MockError
    ├── mock_ncp.rs          # Mock NCP state machine + Spinel command handlers
    ├── pty_transport.rs     # In-memory duplex + PTY integration helpers
    ├── scenarios.rs         # Predefined test scenarios
    ├── topology.rs          # Mock network topology
    └── builder.rs           # MockNcpBuilder
```

`dcu-mock` is a workspace crate. It is a **dev-dependency** of `dcu-tunnel-daemon` and
`spinel` (for integration tests) and an optional runtime dependency if the
daemon ever supports a `--mock-ncp` mode.

## Mock NCP State Machine

The mock mirrors the NCP states defined in `wisun_types::NcpState`. Property
sets drive the transitions. For example:

| Current state | Incoming command | Response | New state |
| --- | --- | --- | --- |
| `Uninitialized` | `CMD_RESET` | `PROP_VALUE_IS(LAST_STATUS, RESET)` | `Uninitialized` (then driver takes it to `Offline`) |
| `Offline` | `PROP_VALUE_SET(NET_IF_UP, true)` | `PROP_VALUE_IS(NET_IF_UP, true)` | `Offline` (interface up, not yet joined) |
| `Offline` | `PROP_VALUE_SET(NET_STACK_UP, true)` | `PROP_VALUE_IS(NET_STACK_UP, true)` + `PROP_VALUE_IS(NET_ROLE, ...)` | `Associating` → `Associated` (after a short delay) |
| `Associated` | `PROP_VALUE_SET(NET_STACK_UP, false)` | `PROP_VALUE_IS(NET_STACK_UP, false)` | `Offline` |
| `Associated` | `CMD_NET_CLEAR` | `PROP_VALUE_IS(LAST_STATUS, OK)` | `Offline` |
| `Offline` | `PROP_VALUE_SET(MAC_SCAN_STATE, SCAN)` | `PROP_VALUE_IS(MAC_SCAN_STATE, SCAN)` + unsolicited `PROP_VALUE_IS(MAC_SCAN_BEACON, ...)` × N + `PROP_VALUE_IS(MAC_SCAN_STATE, IDLE)` | `Offline` |

The mock must also respond to `CMD_PROP_VALUE_GET` for every property the daemon
reads during init: `NCP_VERSION`, `PROTOCOL_VERSION`, `INTERFACE_TYPE`,
`CAPS`, `HWADDR`, `NET_SAVED`, `NET_ROLE`, `PHY_ENABLED`, `PHY_CHAN`, `MAC_*`,
etc. The exact set is determined by the daemon's init sequence (see the C
`SpinelNCPInstance` protothreads / the phase-3B `leave` task path).

## Detailed File Specs

### `config.rs`

```rust
use std::collections::BTreeSet;
use wisun_types::Eui64;

/// Mock NCP configuration. Kept separate from `MockNcp` so the builder can
/// own it before constructing the mock.
#[derive(Debug, Clone)]
pub struct MockConfig {
    pub ncp_version: String,
    pub hardware_address: Eui64,
    pub auto_respond: bool,
}

impl Default for MockConfig {
    fn default() -> Self {
        Self {
            ncp_version: "TIWISUNFAN 1.0".into(),
            hardware_address: Eui64::default(),
            auto_respond: true,
        }
    }
}

/// Capability set returned by `PROP_CAPS`. The exact representation should
/// mirror the C `mCapabilities` bitmask / enum set.
pub struct CapabilitySet {
    pub bits: BTreeSet<u32>,
}

impl CapabilitySet {
    pub fn empty() -> Self;
    pub fn router() -> Self;   // router-capable NCP
    pub fn sleepy() -> Self;   // supports MCU power state
    pub fn add(&mut self, cap: u32);
    pub fn contains(&self, cap: u32) -> bool;
}
```

### `failure.rs`

```rust
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MockError {
    #[error("serial framing error: {0}")]
    Framing(String),
    #[error("invalid Spinel payload: {0}")]
    InvalidPayload(String),
    #[error("scenario timeout")]
    Timeout,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Failure-injection rules. Applied to outbound frames before they are sent.
pub enum FailureRule {
    /// Drop the next `n` outbound frames.
    DropFrames(u32),
    /// Corrupt the CRC on the `n`th outbound frame.
    CorruptCrc(u32),
    /// Delay the response to the `n`th command by `duration`.
    DelayResponse(u32, Duration),
    /// Reject a property get by returning a LAST_STATUS failure response.
    RejectProperty { prop_key: u32 },
    /// Never respond to the given command ID, causing a daemon timeout.
    DropCommand { command_id: u32 },
}
```

### `mock_ncp.rs`

```rust
use std::time::Duration;
use tokio::time::sleep; // used in transition delays (e.g. Associating → Associated)
use dcu_serial::framing::FramedTransport;
use dcu_serial::transport::Transport;
use spinel::command::{
    CMD_PROP_VALUE_GET, CMD_PROP_VALUE_SET, CMD_PROP_VALUE_IS,
    CMD_PROP_VALUE_INSERTED, CMD_PROP_VALUE_REMOVED, CMD_NET_CLEAR, CMD_RESET, CMD_NOOP, CMD_PEEK,
};
use spinel::frame::SpinelFrame;
use spinel::pack::{PackReader, PackWriter};
use wisun_types::{DriverState, NcpState};
use crate::failure::{FailureRule, MockError};
use crate::topology::MockTopology;
use crate::config::{MockConfig, CapabilitySet};

/// Mock NCP. Generic over any [`Transport`].
pub struct MockNcp<T: Transport + Unpin> {
    framed: FramedTransport<T>,
    ncp_state: NcpState,
    /// Mirrors the daemon's `DriverState` (which mirrors the C `mDriverState`).
    driver_state: DriverState,
    topology: MockTopology,
    config: MockConfig,
    caps: CapabilitySet,
    failure_rules: Vec<FailureRule>,
    frame_counter: u32,
}

impl<T: Transport + Unpin> MockNcp<T> {
    /// Create a mock NCP over any `Transport`.
    pub fn new(transport: T, config: MockConfig) -> Result<Self, MockError>;

    /// Run the mock NCP event loop: read frames, respond, emit unsolicited
    /// frames (state changes, scan beacons, etc.).
    pub async fn run(&mut self) -> Result<(), MockError>;

    /// Current NCP state visible to the daemon.
    pub fn ncp_state(&self) -> NcpState;

    /// Current driver state (for final-wait checks like `mDriverState == NormalOperation`).
    pub fn driver_state(&self) -> DriverState;

    /// Convenience helper used by tests/scenarios.
    pub fn is_associated(&self) -> bool { self.ncp_state == NcpState::Associated }

    /// Handle a single decoded Spinel frame.
    async fn handle_frame(&mut self, frame: SpinelFrame) -> Result<(), MockError>;

    // Individual command handlers. There are no dedicated form/join/scan
    // command handlers — those operations are state transitions driven by
    // property sets.
    async fn handle_prop_value_get(&mut self, payload: &[u8]) -> Result<Vec<SpinelFrame>, MockError>;
    async fn handle_prop_value_set(&mut self, payload: &[u8]) -> Result<Vec<SpinelFrame>, MockError>;
    async fn handle_net_clear(&mut self) -> Result<Vec<SpinelFrame>, MockError>;
    async fn handle_reset(&mut self) -> Result<Vec<SpinelFrame>, MockError>;
    async fn handle_noop(&mut self) -> Result<Vec<SpinelFrame>, MockError>;
    async fn handle_peek(&mut self, payload: &[u8]) -> Result<Vec<SpinelFrame>, MockError>;

    // State transitions. These are helpers, not command handlers.
    async fn transition_to(&mut self, state: NcpState) -> Vec<SpinelFrame>;
    async fn on_net_stack_up(&mut self, value: bool) -> Vec<SpinelFrame>;
    async fn on_net_if_up(&mut self, value: bool) -> Vec<SpinelFrame>;
    async fn on_scan_start(&mut self) -> Vec<SpinelFrame>;
}
```

`MockNcp` is generic over any `Transport` so unit tests can use an in-memory
`DuplexStream` while integration tests can use `dcu_serial::PtyTransport`. The
mock owns an `FramedTransport<T>` (the same adapter the daemon's `io_task` uses),
so it speaks the same HDLC-framed Spinel bytes.

### `pty_transport.rs`

```rust
use dcu_serial::framing::FramedTransport;
use dcu_serial::transport::Transport;

/// Newtype wrapper around `tokio::io::DuplexStream` so it can implement the
/// local `Transport` trait (coherence rules prevent implementing `Transport`
/// for a foreign type directly).
pub struct DuplexTransport(pub tokio::io::DuplexStream);

impl std::fmt::Debug for DuplexTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DuplexTransport").finish()
    }
}

impl tokio::io::AsyncRead for DuplexTransport {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().0).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for DuplexTransport {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.get_mut().0).poll_write(cx, buf)
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().0).poll_flush(cx)
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().0).poll_shutdown(cx)
    }
}

impl Transport for DuplexTransport {
    fn info(&self) -> String {
        "duplex-mock".to_string()
    }
}

/// A connected pair of byte transports for testing. The daemon owns one side
/// and the mock NCP owns the other; both sides see the same HDLC-framed bytes.
///
/// In phase 4A-v1 use a `tokio::io::DuplexStream` pair (no real PTY, no
/// blocking). Later, integration tests can switch to `PtyTransportPair` backed by
/// `dcu_serial::PtyPair` once `PtyTransport` is fully async (or once the test
/// code uses the raw PTY fd with `AsyncFd`).
pub struct MockTransportPair {
    pub daemon_side: FramedTransport<DuplexTransport>,
    pub mock_side: FramedTransport<DuplexTransport>,
}

impl MockTransportPair {
    /// Create a pair of connected in-memory transports with a default buffer size.
    pub fn create() -> MockTransportPair {
        let (daemon_raw, mock_raw) = tokio::io::duplex(4096);
        MockTransportPair {
            daemon_side: FramedTransport::new(DuplexTransport(daemon_raw)),
            mock_side: FramedTransport::new(DuplexTransport(mock_raw)),
        }
    }
}

/// Optional PTY-backed transport for integration tests.
/// Wraps `dcu_serial::PtyPair` and exposes both sides as `FramedTransport`.
///
/// Note: this requires `dcu_serial::PtyTransport` to implement functional
/// `AsyncRead`/`AsyncWrite`. Until then, use `MockTransportPair::create()`.
#[cfg(feature = "pty")]
pub struct PtyTransportPair {
    pub daemon_slave_path: String,
    pub mock_side: FramedTransport<dcu_serial::PtyTransport>,
}

#[cfg(feature = "pty")]
impl PtyTransportPair {
    pub fn create() -> Result<Self, MockError> {
        let pair = dcu_serial::PtyPair::open()?;
        Ok(Self {
            daemon_slave_path: pair.slave_path().to_string(),
            mock_side: FramedTransport::new(dcu_serial::PtyTransport::from_pair(&pair)),
        })
    }
}
```

For CI and unit tests, `MockTransportPair` (in-memory duplex) is sufficient. The
PTY path is for tests that want to exercise the exact serial-port setup the
daemon uses.

### `topology.rs`

```rust
use std::net::Ipv6Addr;
use wisun_types::Eui64;

pub struct MockTopology {
    nodes: Vec<MockNode>,
}

pub struct MockNode {
    pub eui64: Eui64,
    pub channel: u8,       // Channel the node is operating on
    pub ipv6_address: Ipv6Addr,
    pub rssi: i8,          // Signal strength
    pub hop_count: u8,     // Distance from BR
    pub is_router: bool,
}

/// Beacon used internally for the scan response. Converted to the actual Spinel
/// pack format ("Cct(ESSC)t(iCUd)") before being sent on the wire.
pub struct MockBeacon {
    pub channel: u8,
    pub rssi: i8,
    pub laddr: Eui64,      // Long address (EUI-64) of the beacon source
    pub saddr: u16,        // Short address
    pub pan_id: u16,
    pub lqi: u8,
    pub protocol: u8,      // Beacon protocol ID (e.g. 0 for Wi-SUN)
    pub flags: u8,
    pub network_name: String,
    pub xpan_id: Vec<u8>,  // Extended PAN ID (variable length, up to 8 bytes)
}

impl MockTopology {
    pub fn new() -> Self;
    pub fn add_node(&mut self, node: MockNode);
    pub fn remove_node(&mut self, eui64: &Eui64);

    /// Create a topology with `n` default nodes spread across channels.
    pub fn with_nodes(n: usize) -> Self;

    /// Access the current node list.
    pub fn nodes(&self) -> &[MockNode];

    /// Produce beacon records for a scan response. `channel_mask` is the set of
    /// channels the daemon requested via `MAC_SCAN_MASK`.
    pub fn get_scan_beacons(&self, channel_mask: &[u8]) -> Vec<MockBeacon>;

    /// Produce topology entries for `CMD_NET_TOPOLOGY_GET` responses.
    pub fn get_topology_entries(&self) -> Vec<MockNode>;

    /// Simulate a new node joining the network and return it.
    pub fn simulate_node_join(&mut self) -> MockNode;
}
```

`MockBeacon` is converted to the Spinel pack format when the mock emits the
unsolicited `PROP_VALUE_IS(MAC_SCAN_BEACON)` frames. The exact format is given
in `SpinelNCPTaskScan.cpp:221-237` ("Cct(ESSC)t(iCUd)"). Do not use
`dcu_dbus::ScanBeacon` inside the mock — that is a D-Bus signal type, not a
Spinel wire type.

### `scenarios.rs`

Scenarios are high-level test scripts that drive the mock by writing the same
Spinel frames the daemon would write. They are **not** special commands handled
by the mock; they exercise the property-set/state-transition path.

```rust
use crate::{MockError, MockNcp};
use crate::topology::MockTopology;

pub enum Scenario {
    /// Power on, scan, form network by setting NET_IF_UP + NET_STACK_UP.
    FormNetwork {
        network_name: String,
        pan_id: u16,
        node_count: usize,
    },
    /// Power on, scan, join an existing network.
    JoinNetwork {
        network_name: String,
        pan_id: u16,
    },
    /// Deep sleep → wake cycle via MCU_POWER_STATE or NOOP.
    SleepWake,
    /// Multiple nodes join and leave.
    DynamicTopology {
        join_count: usize,
        leave_count: usize,
    },
    /// Error scenario: NCP resets and recovers (emits LAST_STATUS_RESET).
    NcpReset,
    /// Error scenario: never respond to a specific command, causing a daemon
    /// timeout. `command_id` is the Spinel command to drop (e.g. CMD_PROP_VALUE_GET).
    CommandTimeout {
        command_id: u32,
    },
    /// Error scenario: reject a specific property get with LAST_STATUS_FAILURE.
    RejectProperty {
        prop_key: u32,
    },
}

impl Scenario {
    /// Execute the scenario by driving a transport connected to a mock NCP.
    /// The mock's `run()` loop must be spawned by the caller before calling
    /// `execute()`.
    pub async fn execute<T>(self, transport: T) -> Result<(), MockError>
    where
        T: dcu_serial::transport::Transport + Unpin;
}
```

`Scenario::execute` is a thin async script. For example, `FormNetwork` will:
1. Send `CMD_PROP_VALUE_SET(NET_IF_UP, true)` and await the `IS` response.
2. Send `CMD_PROP_VALUE_SET(NET_STACK_UP, true)` and await the `IS` response.
3. Wait for the mock to transition to `Associating` then `Associated`
   (the mock emits unsolicited `PROP_VALUE_IS(NET_ROLE, ...)` and
   `PROP_VALUE_IS(NET_STATE, ...)` if the daemon subscribes to state changes).
4. Verify `mock.ncp_state() == NcpState::Associated`.

### `builder.rs`

```rust
use wisun_types::{DriverState, Eui64};
use crate::config::{MockConfig, CapabilitySet};
use crate::failure::FailureRule;
use crate::topology::MockTopology;
use crate::mock_ncp::MockNcp;
use crate::pty_transport::DuplexTransport;

pub struct MockNcpBuilder<T: dcu_serial::transport::Transport + Unpin> {
    config: MockConfig,
    topology: MockTopology,
    failure_rules: Vec<FailureRule>,
    initial_ncp_state: wisun_types::NcpState,
    initial_driver_state: DriverState,
    transport: Option<T>,
}

impl MockNcpBuilder<DuplexTransport> {
    /// Default builder for the common in-memory test case.
    pub fn new() -> Self;

    pub fn with_version(mut self, version: &str) -> Self;
    pub fn with_address(mut self, eui64: Eui64) -> Self;
    pub fn with_topology(mut self, topology: MockTopology) -> Self;
    pub fn with_failure(mut self, rule: FailureRule) -> Self;
    pub fn with_capabilities(mut self, caps: CapabilitySet) -> Self;

    /// Build a `MockNcp` using an internal `DuplexTransport` pair.
    /// Returns the mock and the daemon-side transport.
    pub fn build(self) -> (MockNcp<DuplexTransport>, DuplexTransport);
}

impl<T: dcu_serial::transport::Transport + Unpin> MockNcpBuilder<T> {
    /// Build a mock over an existing transport.
    pub fn build_with_transport(self, transport: T) -> MockNcp<T>;
}
```

The builder keeps the public API stable for phase-4B tests (`MockNcpBuilder::new()`,
`with_topology()`, `build()`), but it now returns the mock **and** the daemon-side
transport so tests can connect both ends.

## Tests

All tests use the in-memory `tokio::io::DuplexStream` pair by default. The mock
is spawned on a task; the test side uses the daemon-side half as if it were the
NCP serial transport.

### Test 1: Mock NCP Responds to Prop Get

```rust
#[tokio::test]
async fn mock_responds_to_prop_get() {
    use spinel::command::CMD_PROP_VALUE_IS;
    use spinel::property::prop_value_get;
    use spinel::property::PROP_NCP_VERSION;
    use dcu_serial::framing::FramedTransport;

    let (mut mock, daemon_raw) = MockNcpBuilder::new().build();
    let ncp_handle = tokio::spawn(async move { mock.run().await.unwrap() });

    let mut framed = FramedTransport::new(daemon_raw);
    let frame = prop_value_get(PROP_NCP_VERSION);
    framed.send_frame(&frame).await.unwrap();

    let response = framed.recv_frame().await.unwrap();
    assert_eq!(response.command_id, CMD_PROP_VALUE_IS);
    ncp_handle.await.unwrap();
}
```

### Test 2: Mock Form Network

```rust
#[tokio::test]
async fn mock_form_network() {
    use spinel::command::CMD_PROP_VALUE_IS;
    use spinel::property::{PROP_NET_IF_UP, PROP_NET_STACK_UP};
    use spinel::property::prop_value_set;
    use spinel::pack::PackWriter;
    use dcu_serial::framing::FramedTransport;
    use wisun_types::NcpState;

    let (mut mock, daemon_raw) = MockNcpBuilder::new()
        .with_topology(MockTopology::with_nodes(3))
        .build();
    let ncp_handle = tokio::spawn(async move { mock.run().await.unwrap() });

    let mut framed = FramedTransport::new(daemon_raw);

    // Bring interface up
    let mut w = PackWriter::new();
    w.write_bool(true);
    framed.send_frame(&prop_value_set(PROP_NET_IF_UP, w.into_bytes()))
        .await
        .unwrap();
    let _ = framed.recv_frame().await.unwrap(); // IS(NET_IF_UP, true)

    // Bring stack up — mock transitions to Associated after a short delay
    let mut w = PackWriter::new();
    w.write_bool(true);
    framed.send_frame(&prop_value_set(PROP_NET_STACK_UP, w.into_bytes()))
        .await
        .unwrap();
    let _ = framed.recv_frame().await.unwrap(); // IS(NET_STACK_UP, true)

    // Wait for association (test helper, not sleep)
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let frame = framed.recv_frame().await.unwrap();
            // Alternatively, the mock can push an unsolicited IS(NET_STATE, Associated).
            if is_associated_state_frame(&frame) { break; }
        }
    })
    .await
    .unwrap();

    ncp_handle.await.unwrap();
}

fn is_associated_state_frame(frame: &SpinelFrame) -> bool {
    // Parse the frame to detect the mock signalling Associated state.
    // Implementation depends on how the mock encodes state changes.
    false
}
```

### Test 3: Failure Injection — Reject a Property Get

```rust
#[tokio::test]
async fn mock_rejects_prop_get() {
    use spinel::property::PROP_NCP_VERSION;
    use spinel::property::prop_value_get;
    use dcu_serial::framing::FramedTransport;

    let (mut mock, daemon_raw) = MockNcpBuilder::new()
        .with_failure(FailureRule::RejectProperty {
            prop_key: PROP_NCP_VERSION,
        })
        .build();
    let ncp_handle = tokio::spawn(async move { mock.run().await.unwrap() });

    let mut framed = FramedTransport::new(daemon_raw);
    framed.send_frame(&prop_value_get(PROP_NCP_VERSION))
        .await
        .unwrap();

    let response = framed.recv_frame().await.unwrap();
    // Response should be a PROP_VALUE_IS(LAST_STATUS, FAILURE) or similar error.
    assert!(is_last_status_failure(&response));

    ncp_handle.await.unwrap();
}
```

### Test 4: Node Join Simulation

```rust
#[tokio::test]
async fn mock_simulates_node_join() {
    let mut topology = MockTopology::new();
    let node = topology.simulate_node_join();
    assert_eq!(topology.nodes().len(), 1);

    let beacons = topology.get_scan_beacons(&[1u8 << node.channel]);
    assert_eq!(beacons.len(), 1);
    assert_eq!(beacons[0].laddr, node.eui64);
}
```

### Test 5: Full Scenario Execution

```rust
#[tokio::test]
async fn full_form_scenario() {
    use dcu_serial::framing::FramedTransport;
    use spinel::property::{PROP_NET_IF_UP, PROP_NET_STACK_UP};
    use spinel::property::prop_value_set;
    use spinel::pack::PackWriter;
    use wisun_types::NcpState;

    let scenario = Scenario::FormNetwork {
        network_name: "TestNet".into(),
        pan_id: 0xABCD,
        node_count: 5,
    };

    let (mut mock, daemon_raw) = MockNcpBuilder::new()
        .with_topology(MockTopology::with_nodes(5))
        .build();
    let ncp_handle = tokio::spawn(async move { mock.run().await.unwrap() });

    scenario.execute(daemon_raw).await.unwrap();
    // `execute` consumes the transport. The final NCP state can be observed
    // by the test helper reading frames from the mock, or by querying the mock
    // handle if it exposes a state channel.

    ncp_handle.await.unwrap();
}
```

## Dependencies

```toml
[package]
name = "dcu-mock"
version = "0.1.0"
edition = "2024"
rust-version = "1.85"

[features]
# Enable real-PTY integration tests. Requires `dcu-serial` mock-pty feature
# and a functional `PtyTransport` AsyncRead/AsyncWrite implementation.
pty = ["dcu-serial/mock-pty"]

[dependencies]
spinel = { path = "../spinel" }
wisun-types = { path = "../wisun-types" }
dcu-serial = { path = "../dcu-serial" }
dcu-dbus = { path = "../dcu-dbus" }          # For D-Bus signal types in scenario assertions
tokio = { workspace = true, features = ["full"] }
tracing = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
dcu-tunnel-daemon = { path = "../dcu-tunnel-daemon" }     # For full-stack integration tests

[lints]
workspace = true
```

## Implementation notes (read before implementing)

1. **This is a spec, not code.** The `dcu-mock` crate does not exist yet. The
   file structure, type signatures, and test examples are the intended design.
   An implementer will create the actual crate files and fill in the function
   bodies.

2. **The mock must be byte-compatible with the real NCP.** Every response
   frame — command IDs, property IDs, pack format strings, byte order — must
   match what a TI Wi-SUN NCP would emit. The summary table is the happy path;
   for the exact init property list, capability bits, scan-beacon format
   ("Cct(ESSC)t(iCUd)" per `SpinelNCPTaskScan.cpp:221-237`), and error
   responses, read the C `SpinelNCPTask*.cpp` and `SpinelNCPInstance*.cpp` files.

3. **`DuplexTransport` is a stopgap.** It exists only because `tokio::io::DuplexStream`
   is a foreign type and cannot implement the local `Transport` trait directly
   (coherence). Once `dcu_serial::PtyTransport` has functional `AsyncRead`/`AsyncWrite`
   (or the test harness switches to raw PTY fds with `AsyncFd`), the integration
   tests should move to the `pty` feature and a real pseudo-terminal.

4. **Capability set must match the daemon's expectations.** The daemon's init
   flow reads `PROP_CAPS` and refuses to form/join if the NCP does not report
   the required capability bits (e.g. router-capable, MCU power state, TI
   vendor-specific caps). The `CapabilitySet` type should mirror the C
   `mCapabilities` structure; do not invent a simpler set unless it matches the
   C bitmask format.

5. **State-change notifications need a channel or polling.** The `wait_for_state`
   helper in `dcu-tunnel-daemon` (phase 3B) uses `tokio::sync::Notify`. The mock can
   either emit unsolicited `PROP_VALUE_IS(NET_STATE, ...)` frames and let the
   daemon's `run()` loop call `state_changed.notify_waiters()`, or the test can
   poll frames directly. The test helpers in this doc poll frames for simplicity;
   production integration tests should rely on the daemon's `wait_for_state`.

## Verification Checklist

- [ ] Mock NCP responds correctly to `CMD_PROP_VALUE_GET`/`CMD_PROP_VALUE_SET` for all properties used during daemon init
- [ ] Form/Join/Scan/Leave are driven by property sets, not dedicated commands
- [ ] In-memory `DuplexStream` transport works for unit tests; PTY integration builds behind the `pty` feature
- [ ] `MockNcp::ncp_state()` and `driver_state()` reflect the correct transitions
- [ ] Failure injection works: drop frames, corrupt CRC, delay responses, reject properties, cause daemon timeout
- [ ] Topology simulation produces valid Spinel beacons ("Cct(ESSC)t(iCUd)") and topology entries
- [ ] Node join/leave simulation updates topology and scan beacons
- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` produces zero warnings
