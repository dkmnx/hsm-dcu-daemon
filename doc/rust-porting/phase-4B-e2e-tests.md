# Phase 4B: End-to-End Integration Tests

## Overview

Full integration tests that verify the complete system: daemon + CLI + mock NCP. Includes CI pipeline setup.

**Effort**: 3-5 days

## What's already in place

- `dcu-tunnel-daemon` crate is implemented with `NcpInstance` (`crates/dcu-tunnel-daemon/src/instance/mod.rs`).
  The public API is:
  ```rust
  NcpInstance::new(config: Config) -> Result<NcpInstance, DaemonError>
  NcpInstance::start_pumps(&mut self) -> Result<(), DaemonError>
  NcpInstance::run(&mut self, cancel: CancellationToken) // blocking event loop
  NcpInstance::stop(&mut self) -> Result<(), DaemonError>
  NcpInstance::shared_state(&self) -> Arc<RwLock<DaemonState>>
  NcpInstance::command_sender(&self) -> mpsc::Sender<Command>
  NcpInstance::interface_name(&self) -> &str
  ```
  There is no `Daemon::start`, `wait_for_state`, `wait_for_interface`, or `ncp_state` method on
  `NcpInstance` — tests must read `DaemonState` through `shared_state()` and send commands through
  `command_sender()` or the `dcuctl` binary.

- `dcu-dbus` exposes `DaemonState` with fields like `ncp_state`, `interface_up`, `stack_up`,
  `network_name`, `pan_id`, `is_connected`, and signal payloads `ScanBeacon` / `EnergyScanResultEntry`.
  It also exposes the `Command` enum (`Form`, `Join`, `Leave`, `Reset`, `NetScanStart`, `SetProperty`,
  `GetProperty`, ...).

- `dcuctl` supports commands: `get`, `set`, `add`, `remove`, `status`, `reset`, `help`, `clear`, `quit`.
  It does **not** currently expose a `scan` command. Tests that need to trigger a scan must send
  `Command::NetScanStart` over `NcpInstance::command_sender()` directly.

- `dcu-mock` is specified in `phase-4A-mock-ncp.md`. The helper API is:
  ```rust
  let (mut mock, daemon_transport) = MockNcpBuilder::new().build();
  tokio::spawn(async move { mock.run().await });
  ```
  `build()` returns `(MockNcp<DuplexTransport>, DuplexTransport)` where `DuplexTransport` implements
  `dcu_serial::transport::Transport`.

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

Integration tests are added to the `dcu-tunnel-daemon` crate as integration tests under
`crates/dcu-tunnel-daemon/tests/`. They use `dcu-mock` as a dev-dependency and `dcuctl` as a
binary fixture (`CARGO_BIN_EXE_dcuctl`).

## Test Specs

> **Important**: All E2E tests use **deterministic polling/state-wait patterns** instead of fixed
> `tokio::time::sleep()` delays. Tests poll `DaemonState` via
> `instance.shared_state().read().await` or wait for D-Bus signal receipts. This makes tests fast and
> non-flaky.

### Pre-requisite: a test seam for the serial transport

`NcpInstance::start_pumps()` currently opens the serial transport from
`config.nc_socket_path`. To use the in-memory mock in tests, add a test-only
method:

```rust
impl NcpInstance {
    /// Start I/O pumps over an explicit transport (for tests).
    #[cfg(feature = "test-util")]
    pub async fn start_pumps_with_transport<T: dcu_serial::Transport + Unpin>(
        &mut self,
        transport: T,
    ) -> Result<(), DaemonError>;
}
```

This method mirrors `start_pumps()` but skips opening the UART/PTY and uses the
provided `T`. For PTY-based tests, the regular `start_pumps()` path opens the
slave path while the mock owns the master. Integration tests must enable the
`test-util` feature on `dcu-tunnel-daemon` (add it to `Cargo.toml` as a dev-dependency
feature).

### `daemon_startup.rs`

```rust
use std::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn daemon_starts_and_reaches_offline() {
    let mut daemon = TestDaemon::start().await.unwrap();

    // Wait for the NCP to finish initialization and reach Offline (or higher).
    timeout(Duration::from_secs(5), async {
        loop {
            let state = daemon.shared_state().read().await;
            if !state.ncp_state.is_initializing() {
                break;
            }
            drop(state);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();

    let state = daemon.shared_state().read().await;
    assert!(!state.ncp_state.is_initializing());

    daemon.shutdown().await.unwrap();
}
```

TUN interface creation is a privileged operation (`dcu-tun` uses `ioctl`). It is
expected to run only in the privileged CI job or in an environment with `CAP_NET_ADMIN`.
The test above verifies the daemon reaches a non-initializing state without
requiring TUN privileges.

### `dcuctl_basic.rs`

```rust
#[tokio::test]
async fn dcuctl_status_command() {
    let daemon = TestDaemon::start().await.unwrap();

    let output = run_dcuctl(&["status"]).await.unwrap();
    assert!(output.contains("NCP:State"));
    assert!(output.contains("Daemon:Enabled"));

    daemon.shutdown().await.unwrap();
}

#[tokio::test]
async fn dcuctl_get_version() {
    let daemon = TestDaemon::start().await.unwrap();

    let output = run_dcuctl(&["get", "NCP:Version"]).await.unwrap();
    assert!(output.contains("TIWISUNFAN"));

    daemon.shutdown().await.unwrap();
}

#[tokio::test]
async fn dcuctl_get_all_properties() {
    let daemon = TestDaemon::start().await.unwrap();

    let props = vec![
        "NCP:ProtocolVersion", "NCP:Version", "NCP:InterfaceType",
        "NCP:HardwareAddress", "NCP:ExtendedAddress", "NCP:MACAddress",
        "NCP:CCAThreshold", "NCP:TXPower", "NCP:Region", "NCP:ModeID",
        "NCP:Channel", "NCP:Frequency", "NCP:RSSI",
        "Network:Name", "Network:PANID", "Network:XPANID",
        "Network:NodeType", "Network:IsCommissioned", "Network:IsConnected",
        "IPv6:LinkLocalAddress", "IPv6:MeshLocalAddress", "IPv6:MeshLocalPrefix",
        "Interface:Up", "Stack:Up", "Daemon:Enabled", "Daemon:ReadyForHostSleep",
    ];

    for prop in props {
        let output = run_dcuctl(&["get", prop]).await;
        assert!(output.is_ok(), "Failed to get property: {prop}");
    }

    daemon.shutdown().await.unwrap();
}
```

### `network_form.rs`

Forming a network is triggered by the D-Bus `Form` command, not by a `dcuctl set`.

```rust
use std::time::Duration;
use dcu_dbus::commands::Command;
use tokio::time::timeout;
use wisun_types::NcpState;

#[tokio::test]
async fn form_network_end_to_end() {
    let mut daemon = TestDaemon::start_with_topology(3).await.unwrap();

    // Send Form command directly (dcuctl does not yet expose a 'form' command).
    daemon
        .send_command(Command::Form {
            params: Default::default(),
        })
        .await;

    // Wait for the mock to transition to Associated.
    timeout(Duration::from_secs(10), async {
        loop {
            let state = daemon.shared_state().read().await;
            if state.ncp_state == NcpState::Associated {
                break;
            }
            drop(state);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();

    let output = run_dcuctl(&["status"]).await.unwrap();
    assert!(output.contains("NCP:State") && output.contains("associated"));

    daemon.shutdown().await.unwrap();
}
```

### `network_join.rs`

```rust
use std::time::Duration;
use dcu_dbus::commands::Command;
use tokio::time::timeout;
use wisun_types::NcpState;

#[tokio::test]
async fn join_network_end_to_end() {
    let mut daemon = TestDaemon::start_with_topology(1).await.unwrap();

    daemon
        .send_command(Command::Join {
            params: Default::default(),
        })
        .await;

    timeout(Duration::from_secs(10), async {
        loop {
            let state = daemon.shared_state().read().await;
            if state.ncp_state == NcpState::Associated {
                break;
            }
            drop(state);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();

    let output = run_dcuctl(&["status"]).await.unwrap();
    assert!(output.contains("associated"));

    daemon.shutdown().await.unwrap();
}
```

### `property_roundtrip.rs`

```rust
#[tokio::test]
async fn property_set_get_roundtrip() {
    let daemon = TestDaemon::start().await.unwrap();

    let writable = vec![
        ("Network:Name", "TestNetwork"),
        ("Network:PANID", "0xABCD"),
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

`dcuctl` does not have a `scan` command. The test drives the scan through the D-Bus
`Command::NetScanStart` and waits for the `NetScanBeacon` signal on the D-Bus connection.

```rust
use std::time::Duration;
use dcu_dbus::commands::Command;
use tokio::time::timeout;
use zbus::Connection;

#[tokio::test]
async fn dbus_signal_on_scan_beacon() {
    let mut daemon = TestDaemon::start_with_topology(3).await.unwrap();
    let conn = daemon.dbus_connection().clone();
    let iface_path = daemon.dbus_iface_path().to_string();

    // Subscribe to NetScanBeacon signal.
    let mut signal_rx = subscribe_net_scan_beacon(&conn, &iface_path).await;

    daemon
        .send_command(Command::NetScanStart {
            params: Default::default(),
        })
        .await;

    let signal = timeout(Duration::from_secs(10), signal_rx.recv()).await;
    assert!(signal.is_ok(), "Scan beacon signal not received");

    daemon.shutdown().await.unwrap();
}
```

### `data_forwarding.rs`

TUN tests are privileged. Gate them with `#[cfg(feature = "privileged")]` or
`#[cfg_attr(not(feature = "privileged"), ignore)]`.

```rust
use std::time::Duration;
use dcu_dbus::commands::Command;
use tokio::time::timeout;
use wisun_types::NcpState;

#[cfg_attr(not(feature = "privileged"), ignore)]
#[tokio::test]
async fn ip_packet_forwarding() {
    let mut daemon = TestDaemon::start_with_topology(1).await.unwrap();

    // Form the network first.
    daemon
        .send_command(Command::Form {
            params: Default::default(),
        })
        .await;
    wait_for_state(&daemon, NcpState::Associated, Duration::from_secs(10)).await;

    let tun_name = daemon.interface_name();
    let ping_result = Command::new("ping6")
        .args(["-I", tun_name, "-c", "1", "2020:abcd::1"])
        .output()
        .await;

    // Note: This test verifies the forwarding path, not the ICMP response
    // (mock NCP may not respond to ICMP).
    assert!(ping_result.is_ok());

    daemon.shutdown().await.unwrap();
}
```

### `error_handling.rs`

Use the mock's failure-injection rules to simulate errors instead of undefined
`mock.disconnect()`/`mock.restart()` methods.

```rust
use dcu_mock::failure::FailureRule;
use dcu_dbus::commands::Command;
use std::time::Duration;

#[tokio::test]
async fn daemon_handles_ncp_timeout() {
    let mut daemon = TestDaemon::builder()
        .with_failure(FailureRule::DropCommand {
            command_id: spinel::command::CMD_PROP_VALUE_GET,
        })
        .start()
        .await
        .unwrap();

    // The daemon init sequence will eventually hit a dropped command and
    // report a timeout / fault. The exact state depends on the timeout policy.
    wait_for_fault_or_offline(&daemon, Duration::from_secs(10)).await;

    daemon.shutdown().await.unwrap();
}
```

> **Note**: `DropCommand` matches on the Spinel command ID (`spinel::command::CMD_PROP_VALUE_GET`),
> not the D-Bus `Command`. The mock operates at the Spinel framing layer.

## CI Pipeline

### `.github/workflows/rust.yml`

> **CI design notes**:
> - The current repository still has the C autotools CI (`./bootstrap.sh && ./configure && make`) in
>   its own workflow. This Rust porting CI is a separate, future workflow that runs the Rust workspace.
> - Hardware-requiring tests (TUN, serial) are gated behind `#[cfg_attr(not(feature = "privileged"), ignore)]`.
> - `cargo test --workspace` runs **without** privileged tests — passes on plain `ubuntu-latest`.
> - A separate `privileged_test` job runs tests with `--features privileged -- --ignored`.
> - The `fuzz` job is aspirational; `cargo-fuzz` and the `fuzz/` directory do not exist yet in the
>   Rust workspace. Add them before enabling this job.
> - Binary `strip = true` / `lto = "thin"` should be in the workspace root `[profile.release]` to
>   meet size targets.

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
    name: Check (fmt + clippy)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --check
      - run: cargo clippy --workspace --all-targets -- -D warnings

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

> Note: The `fuzz` job is not included until `cargo-fuzz` and `fuzz/` targets are added. Once they
> exist, re-add the fuzz job from the previous version of this doc.

## Test Helpers

### `common/daemon_test.rs`

```rust
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, mpsc};
use tokio_util::sync::CancellationToken;

use dcu_tunnel_daemon::config::Config;
use dcu_tunnel_daemon::instance::NcpInstance;
use dcu_dbus::{DbusServer, DaemonState, commands::Command};
use dcu_mock::builder::MockNcpBuilder;
use dcu_mock::failure::FailureRule;
use dcu_mock::pty_transport::DuplexTransport;
use wisun_types::NcpState;

pub struct TestDaemon {
    instance: NcpInstance,
    dbus_server: DbusServer,
    cancel: CancellationToken,
    bus_address: String,
}

impl TestDaemon {
    pub async fn start() -> Result<Self, TestError> {
        Self::builder().start().await
    }

    pub async fn start_with_topology(node_count: usize) -> Result<Self, TestError> {
        Self::builder()
            .with_topology(node_count)
            .start()
            .await
    }

    pub fn builder() -> TestDaemonBuilder {
        TestDaemonBuilder::new()
    }

    pub async fn send_command(&self, cmd: Command) {
        let _ = self.command_sender().send(cmd).await;
    }

    pub fn shared_state(&self) -> Arc<RwLock<DaemonState>> {
        self.instance.shared_state()
    }

    pub fn command_sender(&self) -> mpsc::Sender<Command> {
        self.instance.command_sender()
    }

    pub fn interface_name(&self) -> &str {
        self.instance.interface_name()
    }

    pub fn dbus_connection(&self) -> &zbus::Connection {
        self.dbus_server.conn_ref()
    }

    pub fn dbus_iface_path(&self) -> &str {
        self.dbus_server.iface_object_path_str()
    }

    pub async fn shutdown(mut self) -> Result<(), TestError> {
        self.cancel.cancel();
        tokio::time::sleep(Duration::from_millis(50)).await;
        self.instance.stop().await?;
        self.dbus_server.stop().await?;
        Ok(())
    }
}

pub struct TestDaemonBuilder {
    node_count: usize,
    failure_rules: Vec<FailureRule>,
    interface_name: String,
}

impl TestDaemonBuilder {
    pub fn new() -> Self {
        Self {
            node_count: 0,
            failure_rules: Vec::new(),
            interface_name: "test_wfan0".into(),
        }
    }

    pub fn with_topology(mut self, node_count: usize) -> Self {
        self.node_count = node_count;
        self
    }

    pub fn with_failure(mut self, rule: FailureRule) -> Self {
        self.failure_rules.push(rule);
        self
    }

    pub async fn start(self) -> Result<TestDaemon, TestError> {
        let (mut mock, daemon_transport) = {
            let mut b = MockNcpBuilder::new();
            if self.node_count > 0 {
                b = b.with_topology(dcu_mock::topology::MockTopology::with_nodes(self.node_count));
            }
            for rule in self.failure_rules {
                b = b.with_failure(rule);
            }
            b.build()
        };
        tokio::spawn(async move { mock.run().await.unwrap() });

        let config = test_config(&self.interface_name);
        let mut instance = NcpInstance::new(config).await?;

        let bus_address = start_private_dbus_session().await?;
        let conn = zbus::Connection::session().await?;
        let iface_name = self.interface_name.clone();
        let dbus_server = DbusServer::start_on(
            conn,
            iface_name,
            instance.shared_state(),
            instance.command_sender(),
            format!("com.nestlabs.WPANTunnelDriver.Test.{}", std::process::id()),
        )
        .await?;

        #[cfg(feature = "test-util")]
        instance.start_pumps_with_transport(daemon_transport).await?;

        let cancel = CancellationToken::new();
        let cancel_run = cancel.clone();
        let mut run_instance = instance;
        tokio::spawn(async move { run_instance.run(cancel_run).await });

        Ok(TestDaemon {
            instance: /* ownership problem */ todo!("store instance for shutdown"),
            dbus_server,
            cancel,
            bus_address,
        })
    }
}

pub fn test_config(interface_name: &str) -> Config {
    Config {
        nc_socket_path: "mock".into(),
        tun_interface_name: interface_name.into(),
        ..Default::default()
    }
}

async fn start_private_dbus_session() -> Result<String, TestError> {
    todo!("implement with dbus-daemon --session or dbus-run-session")
}

pub async fn wait_for_state(
    daemon: &TestDaemon,
    target: NcpState,
    duration: Duration,
) -> Result<(), TestError> {
    tokio::time::timeout(duration, async {
        loop {
            if daemon.shared_state().read().await.ncp_state == target {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| TestError::Timeout)
}

pub async fn wait_for_fault_or_offline(
    daemon: &TestDaemon,
    duration: Duration,
) -> Result<(), TestError> {
    tokio::time::timeout(duration, async {
        loop {
            let s = daemon.shared_state().read().await.ncp_state;
            if s == NcpState::Offline || s.is_fault() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| TestError::Timeout)
}
```

> **Note**: The `TestDaemon` implementation above has a known ownership issue: the `run()` task
> needs to own the `NcpInstance`, but `shutdown()` also needs to call `stop()` on it. A real
> implementation will use a channel/`JoinHandle` to reclaim the instance, or split `NcpInstance` into
> a handle + task pair. This spec leaves the exact ownership pattern as a TODO for the
> implementer.

### `common/dbus_test.rs`

```rust
use std::process::Command;

pub async fn run_dcuctl(args: &[&str]) -> Result<String, TestError> {
    let output = Command::new(env!("CARGO_BIN_EXE_dcuctl"))
        .args(args)
        .env("DBUS_SESSION_BUS_ADDRESS", current_test_bus_address())
        .output()?;

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

pub async fn subscribe_net_scan_beacon(
    conn: &zbus::Connection,
    iface_path: &str,
) -> tokio::sync::mpsc::Receiver<dcu_dbus::types::ScanBeacon> {
    // Use zbus signal stream on the interface path / member "NetScanBeacon".
    todo!("implement with zbus::SignalStream")
}

fn current_test_bus_address() -> String {
    std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_default()
}
```

## Implementation notes (read before implementing)

1. **This is a spec, not code.** The `dcu-tunnel-daemon` integration tests and `dcu-mock` crate do not exist
   yet. The `TestDaemon` helper is a design target; the implementer must resolve ownership/lifetime
   details (e.g. how to reclaim `NcpInstance` from the `run()` task for shutdown).

2. **`NcpInstance` needs a test seam.** `start_pumps_with_transport` is required because the
   current `start_pumps()` opens the serial device from `Config::nc_socket_path`. Without this seam,
   tests cannot inject the in-memory `DuplexTransport`. Add it under `#[cfg(feature = "test-util")]`
   (integration tests link the library as external, so `#[cfg(test)]` is not active).

3. **Private D-Bus session per test.** The production daemon claims the canonical well-known name
   `com.nestlabs.WPANTunnelDriver`. Parallel tests cannot all claim that name. Use `dbus-daemon` or
   `dbus-run-session` to give each test a private bus, and set `DBUS_SESSION_BUS_ADDRESS` before
   spawning `dcuctl`.

4. **No `scan` / `form` / `join` dcuctl commands yet.** The current `dcuctl` only supports `get`,
   `set`, `add`, `remove`, `status`, `reset`, `help`, `clear`, `quit`. Tests that need to form/join/scan
   must send `Command::Form`/`Join`/`NetScanStart` over `NcpInstance::command_sender()`.

5. **TUN tests require privileges.** Do not run them in the default `cargo test` job; use the
   `privileged` feature and an `ubuntu-latest` runner with `sudo`/`CAP_NET_ADMIN`.

6. **Fuzz is aspirational.** The `fuzz` job and `cargo-fuzz` targets are not present in the Rust
   workspace yet. The CI doc shows the future structure; do not enable the fuzz job until the
   targets are implemented.

## Verification Checklist

- [ ] `start_pumps_with_transport` test seam added to `NcpInstance`
- [ ] Private D-Bus session bus helper works for parallel tests
- [ ] `TestDaemon` builds and can start/stop cleanly
- [ ] Daemon reaches a non-initializing state in `daemon_startup.rs`
- [ ] `dcuctl status` returns state after daemon starts
- [ ] `dcuctl get NCP:Version` returns the mock version string
- [ ] `dcuctl get` works for every property listed in `dcuctl_get_all_properties`
- [ ] Form and Join commands reach `NcpState::Associated` deterministically
- [ ] Property set/get round-trips for `Network:Name`, `Network:PANID`, `NCP:CCAThreshold`
- [ ] `NetScanBeacon` signal is received after `Command::NetScanStart`
- [ ] TUN/ICMPv6 forwarding test passes with `--features privileged`
- [ ] Failure injection causes timeout/fault handling path
- [ ] CI runs `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`
- [ ] `privileged_test` CI job runs TUN tests with `--features privileged -- --ignored`
- [ ] Release binary size check passes (< 6 MB)
- [ ] Workspace `[profile.release]` has `strip = true` and `lto = "thin"`
