# Phase 4A: `dcu-mock` — Mock NCP

## Overview

Mock NCP for integration testing. Emulates the TI NCP over a PTY, responding to Spinel commands without hardware.

**Effort**: 3-5 days

## Purpose

- Enable CI testing without physical hardware
- Simulate network operations (scan, form, join)
- Inject failures for error handling tests
- Provide deterministic test scenarios

## Crate Structure

```text
dcu-mock/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── mock_ncp.rs          # Mock NCP: respond to Spinel commands
    ├── pty_transport.rs     # PTY-based transport (fake UART)
    ├── scenarios.rs         # Predefined test scenarios
    ├── topology.rs          # Mock network topology
    └── builder.rs           # Builder for configuring mock NCP behavior
```

## Detailed File Specs

### `mock_ncp.rs`

```rust
pub struct MockNcp {
    serial: FramedTransport<PtyTransport>,
    state: MockNcpState,
    topology: MockTopology,
    config: MockConfig,
}

#[derive(Debug, Clone)]
pub struct MockConfig {
    pub ncp_version: String,
    pub hardware_address: Eui64,
    pub auto_respond: bool,
    pub failure_injection: Vec<FailureRule>,
}

pub enum FailureRule {
    DropFrames(u32),           // Drop next N frames
    CorruptCrc(u32),           // Corrupt CRC on frame N
    DelayResponse(u32, Duration), // Delay response to command N
    RejectCommand(u32),        // Reject command with error
}

impl MockNcp {
    pub fn new(config: MockConfig) -> Result<Self, MockError>;

    /// Run the mock NCP event loop.
    pub async fn run(&mut self) -> Result<(), MockError>;

    /// Handle a single Spinel command.
    async fn handle_command(&mut self, frame: SpinelFrame) -> Result<(), MockError>;

    // Command handlers
    async fn handle_prop_get(&mut self, prop_key: u32) -> Result<SpinelFrame, MockError>;
    async fn handle_prop_set(&mut self, prop_key: u32, value: &[u8]) -> Result<SpinelFrame, MockError>;
    async fn handle_form(&mut self, payload: &[u8]) -> Result<SpinelFrame, MockError>;
    async fn handle_join(&mut self, payload: &[u8]) -> Result<SpinelFrame, MockError>;
    async fn handle_leave(&mut self) -> Result<SpinelFrame, MockError>;
    async fn handle_scan(&mut self, payload: &[u8]) -> Result<(), MockError>;
}
```

### `pty_transport.rs`

```rust
pub struct MockPtyPair {
    pub daemon_side: PtyTransport,  // Connect daemon here
    pub mock_side: PtyTransport,    // Mock NCP reads/writes here
}

impl MockPtyPair {
    /// Create a PTY pair for testing.
    pub fn create() -> Result<Self, MockError> {
        // Use portable-pty or nix to create PTY pair
        // Returns connected pair
    }
}
```

### `topology.rs`

```rust
pub struct MockTopology {
    nodes: Vec<MockNode>,
}

pub struct MockNode {
    pub eui64: Eui64,
    pub ipv6_address: Ipv6Addr,
    pub rssi: i8,          // Signal strength
    pub hop_count: u8,     // Distance from BR
    pub is_router: bool,
}

impl MockTopology {
    pub fn new() -> Self;
    pub fn add_node(&mut self, node: MockNode);
    pub fn remove_node(&mut self, eui64: &Eui64);
    pub fn get_scan_results(&self) -> Vec<ScanResult>;
    pub fn get_topology(&self) -> Vec<TopologyEntry>;
    pub fn simulate_node_join(&mut self) -> MockNode;
}
```

### `scenarios.rs`

Predefined test scenarios:

```rust
pub enum Scenario {
    /// Power on, scan, form network
    FormNetwork {
        network_name: String,
        pan_id: u16,
        node_count: usize,
    },
    /// Power on, scan, join existing network
    JoinNetwork {
        network_name: String,
        pan_id: u16,
    },
    /// Deep sleep → wake cycle
    SleepWake,
    /// Multiple nodes join and leave
    DynamicTopology {
        join_count: usize,
        leave_count: usize,
    },
    /// Error scenario: NCP crashes and recovers
    NcpCrash,
    /// Error scenario: command timeout
    CommandTimeout {
        command_id: u32,
    },
}

impl Scenario {
    pub async fn execute(self, mock: &mut MockNcp) -> Result<(), MockError>;
}
```

### `builder.rs`

```rust
pub struct MockNcpBuilder {
    config: MockConfig,
    topology: MockTopology,
    failure_rules: Vec<FailureRule>,
}

impl MockNcpBuilder {
    pub fn new() -> Self;

    pub fn with_version(mut self, version: &str) -> Self;
    pub fn with_address(mut self, eui64: Eui64) -> Self;
    pub fn with_topology(mut self, topology: MockTopology) -> Self;
    pub fn with_failure(mut self, rule: FailureRule) -> Self;

    pub fn build(self) -> MockNcp;
}
```

## Tests

### Test 1: Mock NCP Responds to Prop Get

```rust
#[tokio::test]
async fn mock_responds_to_prop_get() {
    let pty = MockPtyPair::create().unwrap();
    let mut mock = MockNcp::new(MockConfig::default()).unwrap();

    let ncp_handle = tokio::spawn(async move {
        mock.run_with_transport(pty.mock_side).await.unwrap();
    });

    let mut framed = FramedTransport::new(pty.daemon_side);
    let frame = SpinelFrame::new(CMD_PROP_VALUE_GET, encode_prop_key(PROP_NCP_VERSION));
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
    let scenario = Scenario::FormNetwork {
        network_name: "TestNet".into(),
        pan_id: 0xABCD,
        node_count: 3,
    };

    let mut mock = MockNcpBuilder::new()
        .with_topology(MockTopology::with_nodes(3))
        .build();

    scenario.execute(&mut mock).await.unwrap();
    assert!(mock.is_associated());
}
```

### Test 3: Failure Injection

```rust
#[tokio::test]
async fn mock_injects_timeout() {
    let mut mock = MockNcpBuilder::new()
        .with_failure(FailureRule::RejectCommand(
            CMD_PROP_VALUE_GET,
        ))
        .build();

    // Send command that will be rejected
    // Verify error handling
}
```

### Test 4: Node Join Simulation

```rust
#[tokio::test]
async fn mock_simulates_node_join() {
    let mut topology = MockTopology::new();
    let node = topology.simulate_node_join();
    assert!(topology.nodes().len() == 1);

    let scan_results = topology.get_scan_results();
    assert_eq!(scan_results.len(), 1);
    assert_eq!(scan_results[0].eui64, node.eui64);
}
```

### Test 5: Full Scenario Execution

```rust
#[tokio::test]
async fn full_form_scenario() {
    let scenario = Scenario::FormNetwork {
        network_name: "TestNet".into(),
        pan_id: 0xABCD,
        node_count: 5,
    };

    let pty = MockPtyPair::create().unwrap();
    let mut mock = MockNcpBuilder::new()
        .with_topology(MockTopology::with_nodes(5))
        .build();

    let ncp_handle = tokio::spawn(async move {
        mock.run_with_transport(pty.mock_side).await.unwrap();
    });

    // Connect daemon to mock
    let mut daemon = setup_daemon_with_transport(pty.daemon_side).await;
    daemon.start().await.unwrap();

    // Wait for form to complete
    tokio::time::sleep(Duration::from_secs(5)).await;

    let state = daemon.ncp_state();
    assert_eq!(state, NcpState::Associated);

    ncp_handle.await.unwrap();
}
```

## Dependencies

```toml
[dependencies]
spinel = { path = "../spinel" }
wisun-types = { path = "../wisun-types" }
dcu-serial = { path = "../dcu-serial" }
tokio = { version = "1", features = ["full"] }
portable-pty = "0.8"
tracing = "0.1"
```

## Verification Checklist

- [ ] Mock NCP responds to every known Spinel command
- [ ] PTY transport connects to daemon correctly
- [ ] All predefined scenarios execute successfully
- [ ] Failure injection works (timeout, corrupt, reject)
- [ ] Topology simulation produces correct scan results
- [ ] Node join/leave simulation works
- [ ] `cargo test` passes
- [ ] `cargo clippy` produces zero warnings
