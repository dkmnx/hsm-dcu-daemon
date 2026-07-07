# Phase 4B: End-to-End Integration Tests

## Overview

Full integration tests that verify the complete system: daemon + CLI + mock NCP. Includes CI pipeline setup.

**Effort**: 3-5 days

## Test Structure

```text
tests/
├── integration/
│   ├── daemon_startup.rs      # Daemon starts, creates TUN
│   ├── dcuctl_basic.rs        # dcuctl connects, runs commands
│   ├── network_form.rs        # Form network via mock NCP
│   ├── network_join.rs        # Join network via mock NCP
│   ├── property_roundtrip.rs  # Every property get/set
│   ├── signal_delivery.rs     # D-Bus signals fire
│   ├── data_forwarding.rs     # IP packets forwarded via TUN
│   └── error_handling.rs      # Error scenarios
├── common/
│   ├── mock_ncp.rs            # Shared mock NCP setup
│   ├── dbus_test.rs           # D-Bus test helpers
│   └── daemon_test.rs         # Daemon test helpers
└── snapshots/
    ├── status_output.txt      # Expected status output
    └── get_version.txt        # Expected version output
```

## Test Specs

> **Important**: All E2E tests use **deterministic signal/state-wait patterns**, NOT `tokio::time::sleep()`.
> Tests wait for specific state transitions (`daemon.wait_for_state(Associated, timeout)`) or
> D-Bus signal receipts. This makes tests fast and non-flaky.

### `daemon_startup.rs`

```rust
#[tokio::test]
async fn daemon_starts_and_creates_tun() {
    let mock = setup_mock_ncp().await;
    let config = test_config(&mock);

    let daemon = Daemon::start_with_signal(config).await.unwrap();
    // Use daemon.state_notify() to wait for specific state
    // instead of sleep
    let state = daemon.wait_for_state(NcpState::Offline, Duration::from_secs(5)).await;
    assert_eq!(state, NcpState::Offline);

    // Wait for TUN interface creation via channel signal
    let _iface = daemon.wait_for_interface(Duration::from_secs(5)).await;
    assert!(!_iface.is_empty());

    daemon.shutdown().await.unwrap();
}
```

### `dcuctl_basic.rs`

```rust
#[tokio::test]
async fn dcuctl_status_command() {
    let mock = setup_mock_ncp().await;
    let daemon = Daemon::start(test_config(&mock)).await.unwrap();

    let output = run_dcuctl(&["status"]).await.unwrap();
    assert!(output.contains("NCP:State"));
    assert!(output.contains("Daemon:Version"));

    daemon.shutdown().await.unwrap();
}

#[tokio::test]
async fn dcuctl_get_version() {
    let mock = setup_mock_ncp().await;
    let daemon = Daemon::start(test_config(&mock)).await.unwrap();

    let output = run_dcuctl(&["get", "NCP:Version"]).await.unwrap();
    assert!(output.contains("TIWISUNFAN"));

    daemon.shutdown().await.unwrap();
}

#[tokio::test]
async fn dcuctl_get_all_properties() {
    let mock = setup_mock_ncp().await;
    let daemon = Daemon::start(test_config(&mock)).await.unwrap();

    let props = vec![
        "NCP:ProtocolVersion", "NCP:Version", "NCP:InterfaceType",
        "NCP:HardwareAddress", "NCP:CCAThreshold", "NCP:Region",
        "NCP:ModeID", "unicastchlist", "broadcastchlist",
        "chspacing", "ch0centerfreq", "Network:panid",
        "bcdwellinterval", "ucdwellinterval", "bcinterval",
        "ucchfunction", "bcchfunction", "macfiltermode",
        "Interface:Up", "Stack:Up", "Network:NodeType",
    ];

    for prop in props {
        let output = run_dcuctl(&["get", prop]).await;
        assert!(output.is_ok(), "Failed to get property: {prop}");
    }

    daemon.shutdown().await.unwrap();
}
```

### `network_form.rs`

```rust
#[tokio::test]
async fn form_network_end_to_end() {
    let mock = MockNcpBuilder::new()
        .with_topology(MockTopology::with_nodes(3))
        .build();
    let mock_ncp = setup_mock_ncp_with(mock).await;
    let daemon = Daemon::start(test_config(&mock_ncp)).await.unwrap();

    // Bring interface up
    run_dcuctl(&["set", "interface:up", "true"]).await.unwrap();

    // Wait for form to complete
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Verify state
    let output = run_dcuctl(&["status"]).await.unwrap();
    assert!(output.contains("associated"));

    // Verify connected devices
    let output = run_dcuctl(&["get", "connecteddevices"]).await.unwrap();
    assert!(output.contains("2020:abcd"));

    daemon.shutdown().await.unwrap();
}
```

### `network_join.rs`

```rust
#[tokio::test]
async fn join_network_end_to_end() {
    let mock = MockNcpBuilder::new()
        .with_topology(MockTopology::with_nodes(1))
        .build();
    let mock_ncp = setup_mock_ncp_with(mock).await;
    let daemon = Daemon::start(test_config(&mock_ncp)).await.unwrap();

    // Bring interface up
    run_dcuctl(&["set", "interface:up", "true"]).await.unwrap();

    // Wait for join
    tokio::time::sleep(Duration::from_secs(5)).await;

    let output = run_dcuctl(&["status"]).await.unwrap();
    assert!(output.contains("associated"));

    daemon.shutdown().await.unwrap();
}
```

### `property_roundtrip.rs`

```rust
#[tokio::test]
async fn property_set_get_roundtrip() {
    let mock = setup_mock_ncp().await;
    let daemon = Daemon::start(test_config(&mock)).await.unwrap();

    // Test each writable property
    let writable = vec![
        ("Network:Name", "TestNetwork"),
        ("Network:panid", "0xABCD"),
        ("NCP:CCAThreshold", "-60"),
    ];

    for (prop, value) in writable {
        run_dcuctl(&["set", prop, value]).await.unwrap();
        let output = run_dcuctl(&["get", prop]).await.unwrap();
        assert!(output.contains(value), "Property {prop} roundtrip failed");
    }

    daemon.shutdown().await.unwrap();
}
```

### `signal_delivery.rs`

```rust
#[tokio::test]
async fn dbus_signal_on_state_change() {
    let mock = setup_mock_ncp().await;
    let daemon = Daemon::start(test_config(&mock)).await.unwrap();

    let mut signal_rx = subscribe_dbus_signal("NetScanBeacon").await;

    // Trigger scan
    run_dcuctl(&["scan"]).await.unwrap();

    // Wait for signal
    let signal = tokio::time::timeout(Duration::from_secs(10), signal_rx.recv()).await;
    assert!(signal.is_ok(), "Scan beacon signal not received");

    daemon.shutdown().await.unwrap();
}
```

### `data_forwarding.rs`

```rust
#[tokio::test]
async fn ip_packet_forwarding() {
    let mock = setup_mock_ncp().await;
    let daemon = Daemon::start(test_config(&mock)).await.unwrap();

    // Bring interface up
    run_dcuctl(&["set", "interface:up", "true"]).await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Send ICMPv6 ping via TUN interface
    let tun_name = daemon.interface_name();
    let ping_result = Command::new("ping6")
        .args(["-I", tun_name, "-c", "1", "2020:abcd::1"])
        .output()
        .await;

    // Note: This test verifies the forwarding path, not the ICMP response
    // (mock NCP may not respond to ICMP)
    assert!(ping_result.is_ok());

    daemon.shutdown().await.unwrap();
}
```

### `error_handling.rs`

```rust
#[tokio::test]
async fn daemon_handles_ncp_disconnect() {
    let mock = setup_mock_ncp().await;
    let daemon = Daemon::start(test_config(&mock)).await.unwrap();

    // Simulate NCP disconnect
    mock.disconnect().await;

    // Wait for daemon to detect
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Verify daemon reports fault state
    let state = daemon.ncp_state();
    assert!(state.is_fault() || state == NcpState::Offline);

    daemon.shutdown().await.unwrap();
}

#[tokio::test]
async fn daemon_recovers_from_ncp_restart() {
    let mock = setup_mock_ncp().await;
    let daemon = Daemon::start(test_config(&mock)).await.unwrap();

    // Bring up
    run_dcuctl(&["set", "interface:up", "true"]).await.unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Simulate NCP restart
    mock.restart().await;
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Verify daemon reconnects
    let output = run_dcuctl(&["status"]).await.unwrap();
    assert!(output.contains("NCP:State"));

    daemon.shutdown().await.unwrap();
}
```

## CI Pipeline

### `.github/workflows/rust.yml`

> **CI design notes**:
> - Hardware-requiring tests (TUN, serial) are gated behind `#[cfg_attr(not(feature = "privileged"), ignore)]`
> - `cargo test --workspace` runs **without** privileged tests — passes on plain ubuntu-latest
> - A separate `privileged_test` job runs tests with `--features privileged -- --ignored`
> - Binary strip=true is in `Cargo.toml` under `[profile.release]` to meet size targets
> - Fuzz corpus is persisted across runs as an artifact

```yaml
name: Rust CI

on:
  push:
    branches: [main, rust-port]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: -D warnings

jobs:
  check:
    name: Check (fmt + clippy + audit)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --check
      - run: cargo clippy --workspace --all-targets
      - run: cargo install cargo-audit && cargo audit

  test:
    name: Test (unprivileged)
    runs-on: ubuntu-latest
    needs: check
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --workspace

  privileged_test:
    name: Test (privileged — TUN + serial)
    runs-on: ubuntu-latest
    needs: check
    if: github.repository_owner == 'main'
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: sudo apt-get update && sudo apt-get install -y iproute2
      - run: cargo test --workspace --features privileged -- --ignored

  fuzz:
    name: Fuzz (60s each)
    runs-on: ubuntu-latest
    needs: check
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo install cargo-fuzz
      - run: cargo fuzz run spinel_frame -- -max_total_time=60
      - run: cargo fuzz run spinel_hdlc -- -max_total_time=60
      - name: Upload fuzz corpus
        uses: actions/upload-artifact@v4
        with:
          name: fuzz-corpus
          path: fuzz/corpus/
          retention-days: 30

  build:
    name: Build Release (strip + size check)
    runs-on: ubuntu-latest
    needs: test
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo build --release --workspace
      - name: Check binary size
        run: |
          SIZE=$(stat -c%s target/release/dcud)
          echo "dcud binary size: $SIZE bytes"
          if [ "$SIZE" -gt 6291456 ]; then
            echo "ERROR: Binary too large (>6 MB, may need LTO or strip)"
            exit 1
          fi
      - uses: actions/upload-artifact@v4
        with:
          name: binaries
          path: |
            target/release/dcud
            target/release/dcuctl
```

## Test Helpers

### `common/daemon_test.rs`

```rust
pub struct TestDaemon {
    daemon: Daemon,
    mock_ncp: MockNcp,
    pty: MockPtyPair,
}

impl TestDaemon {
    pub async fn start() -> Result<Self, TestError> {
        let pty = MockPtyPair::create()?;
        let mock = MockNcp::new(MockConfig::default())?;
        let config = test_config(&pty);

        let daemon = Daemon::start(config).await?;

        // Spawn mock NCP in background
        let mock_handle = tokio::spawn(async move {
            mock.run_with_transport(pty.mock_side).await.unwrap();
        });

        Ok(Self { daemon, mock_ncp: mock, pty })
    }

    pub async fn shutdown(mut self) {
        self.daemon.shutdown().await.unwrap();
    }
}

pub fn test_config(pty: &MockPtyPair) -> Config {
    Config {
        ncSocketPath: pty.slave_path().into(),
        tunInterfaceName: "test_wfan0".into(),
        ..Default::default()
    }
}
```

### `common/dbus_test.rs`

```rust
pub async fn run_dcuctl(args: &[&str]) -> Result<String, TestError> {
    let output = Command::new(env!("CARGO_BIN_EXE_dcuctl"))
        .args(args)
        .output()
        .await?;

    if !output.status.success() {
        return Err(TestError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).into(),
        ));
    }

    Ok(String::from_utf8(output.stdout)?)
}

pub async fn dbus_get_version() -> Result<String, TestError> {
    let output = run_dcuctl(&["get", "Daemon:Version"]).await?;
    Ok(output.trim().into())
}
```

## Verification Checklist

- [ ] All integration tests pass
- [ ] Daemon starts and creates TUN interface
- [ ] dcuctl connects and runs all commands
- [ ] Network form/join works end-to-end
- [ ] All properties round-trip correctly
- [ ] D-Bus signals fire on state changes
- [ ] IP packet forwarding works
- [ ] Error handling: disconnect, restart, timeout
- [ ] CI pipeline runs on every push/PR
- [ ] Fuzz targets run for 60+ seconds
- [ ] `Cargo.toml` `[profile.release]` has `strip = true` and `lto = "thin"` to meet size targets
- [ ] CI has separate `privileged_test` job for TUN/ioctl tests
- [ ] All E2E tests use deterministic state-wait patterns, not sleep()
- [ ] Binary size < 6 MB (tokio+zbus+clap unstripped ~5 MB; stripped ~3 MB)
- [ ] `cargo clippy` zero warnings in CI
- [ ] `cargo fmt --check` passes in CI
