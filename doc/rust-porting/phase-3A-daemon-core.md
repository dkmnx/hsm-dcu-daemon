# Phase 3A: `dcu-daemon` — Core State Machine

## Overview

The main daemon process. Manages NCP lifecycle, state machine, event loop, task queue, config parsing. This is where protothreads get replaced with async/await.

**Replaces**: `src/dcud/*` (core daemon files)

**Effort**: 14-21 days — this is the **critical/hardest path** in the entire port. Contains protothread→async conversion, signal handling,
event loop with select/poll→tokio migration, fork()→Command rewrite, and config file parsing.

## Source Files to Port

| C/C++ File                                  | LOC   | What to Extract                                    |
| ------------------------------------------- | ----- | -------------------------------------------------- |
| `src/dcud/wpantund.cpp`                     | 1033  | Entry point, main(), signal handling, config       |
| `src/dcud/NCPInstanceBase.cpp`              | 2162  | Base NCP state machine, task queue                 |
| `src/dcud/NCPInstanceBase-Addresses.cpp`    | 1332  | IPv6 address management                            |
| `src/dcud/NCPInstanceBase-AsyncIO.cpp`      | ~800  | Async I/O pumps (NCP ↔ driver)                     |
| `src/dcud/NCPInstanceBase-NetInterface.cpp` | ~700  | TUN interface lifecycle                            |
| `src/dcud/NCPInstance.cpp`                  | ~400  | NCP instance creation, dispatch                    |
| `src/dcud/NCPInstance.h`                    | ~100  | Instance class definition                          |
| `src/dcud/NCPControlInterface.cpp`          | ~600  | Abstract control interface                         |
| `src/dcud/FirmwareUpgrade.cpp`              | ~400  | Firmware update (fork + exec)                      |
| `src/dcud/StatCollector.cpp`                | 1737  | Network statistics collection                      |
| `src/dcud/NetworkRetain.cpp`                | ~300  | Persistent network config                          |
| `src/dcud/Pcap.cpp`                         | ~200  | Packet capture                                     |
| `src/dcud/RunawayResetBackoffManager.cpp`   | ~150  | Reset backoff logic                                |
| `src/dcud/NCPTypes.cpp`                     | ~20   | ToString helpers                                   |
| `src/dcud/wpan-error.c`                     | ~80   | Error code strings                                 |
| `src/dcud/NCPTypes.h`                       | ~60   | Type aliases                                       |
| `src/dcud/wpantund.h`                       | ~40   | Global declarations                                |
| `src/dcud/IPCServer.h`                      | ~30   | IPC interface                                      |
| `src/dcud/NetworkInstance.h`                | ~40   | Network instance definition                        |

**Total C/C++ code**: ~10,244 LOC

## Crate Structure

```text
dcu-daemon/
├── Cargo.toml
└── src/
    ├── main.rs                 # Entry point, signal handling
    ├── config.rs               # Config file parser
    ├── instance/
    │   ├── mod.rs
    │   ├── base.rs             # NCPInstanceBase: state machine, task queue
    │   ├── addresses.rs        # IPv6 address management
    │   ├── net_interface.rs    # TUN interface lifecycle
    │   └── async_io.rs         # Async I/O pumps
    ├── tasks/
    │   ├── mod.rs              # Task trait + queue
    │   ├── queue.rs            # Task queue management
    │   └── backoff.rs          # RunawayResetBackoffManager
    ├── stat_collector.rs       # Network statistics
    ├── network_retain.rs       # Persistent network config
    ├── pcap.rs                 # Packet capture
    ├── firmware_upgrade.rs     # NCP firmware update
    ├── event.rs                # Event system (replaces protothreads)
    └── control_interface.rs    # Control interface dispatch
```

## Protothread → Async Conversion

### Pattern: Basic Wait

**C (protothread)**:
```c
int vprocess_disabled(int event, va_list args) {
    EH_BEGIN_SUB(&mSubPT);
    while (!mEnabled) {
        EH_WAIT_UNTIL_WITH_TIMEOUT(
            NCP_DEFAULT_COMMAND_RESPONSE_TIMEOUT,
            mEnabled || !is_busy()
        );
        if (mEnabled) break;
        // handle timeout
    }
    EH_END();
}
```

**Rust (async)**:
```rust
async fn process_disabled(&self, cancel: &CancellationToken) {
    while !self.enabled.load(Ordering::Relaxed) {
        let timeout = Duration::from_millis(NCP_DEFAULT_COMMAND_RESPONSE_TIMEOUT_MS);
        tokio::select! {
            _ = self.state_changed.notified() => {},
            _ = tokio::time::sleep(timeout) => {
                tracing::warn!("Timeout in disabled state");
                return;
            }
            _ = cancel.cancelled() => return,
        }
        if self.enabled.load(Ordering::Relaxed) {
            break;
        }
    }
}
```

### Pattern: Spawn Child Task

**C (protothread)**:
```c
EH_SPAWN(child_pt, child_task_process());
```

**Rust (async)**:
```rust
tokio::spawn(async move {
    child_task.run().await;
});
```

### Pattern: Wait for I/O

**C (protothread)**:
```c
NLPT_WAIT_UNTIL_READABLE_OR_COND(pt, fd, condition);
```

**Rust (async)**:
```rust
tokio::select! {
    _ = readable_fd.recv() => {},
    _ = tokio::time::sleep(timeout) => {},
}
```

## Detailed File Specs

### `main.rs`

Reimplements `wpantund.cpp`:

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::load(args.config_path)?;

    // Setup signal handlers (SIGINT, SIGTERM → graceful shutdown)
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        cancel_clone.cancel();
    });

    // Initialize NCP instance
    let mut instance = NcpInstance::new(config).await?;

    // Start D-Bus server
    let dbus_server = DbusServer::start(
        instance.interface_name().into(),
        instance.state_handle(),
        instance.command_sender(),
    ).await?;

    // Start I/O pumps
    instance.start_pumps().await?;

    // Main event loop
    tokio::select! {
        _ = instance.run() => {},
        _ = cancel.cancelled() => {
            tracing::info!("Shutting down...");
        }
    }

    // Cleanup
    instance.shutdown().await?;
    Ok(())
}
```

### `config.rs`

Parse `wpantund.conf` format — **NOT** TOML. The C parser in `src/util/config-file.c:49-177` uses a custom line-based format:
- Lines: `Config:TUN:InterfaceName "wfan0"` (whitespace-separated key + value)
- Values support shell-style quoting (single, double)
- Comments start with `#`
- Keys use colon-delimited namespaces (e.g., `Config:NCP:SocketPath`)

```rust
pub struct Config {
    pub nc_socket_path: String,      // Config:NCP:SocketPath
    pub nc_socket_baud: u32,       // Config:NCP:SocketBaud
    pub nc_driver_name: String,     // Config:NCP:DriverName
    pub tun_interface_name: String, // Config:TUN:InterfaceName
    pub daemon_pid_file: Option<String>,
    pub daemon_priv_drop_to_user: Option<String>,
    pub daemon_chroot: Option<String>,
    pub daemon_terminate_on_fault: bool,           // Config:Daemon:TerminateOnFault
    pub daemon_auto_associate_after_reset: bool,   // Config:Daemon:AutoAssociateAfterReset
    pub daemon_auto_firmware_update: bool,         // Config:Daemon:AutoFirmwareUpdate
    pub daemon_auto_deep_sleep: bool,              // Config:Daemon:AutoDeepSleep
    pub ipv6_wfantund_global_address: Option<String>, // IPv6:WfantundGlobalAddress
    // ... all config keys from wpantund.conf
}

impl Config {
    /// Parse wpantund.conf format.
    /// NOT TOML. Uses shell-style quoting and whitespace separation.
    pub fn parse(content: &str) -> Result<Self, ConfigError>;
    pub fn load(path: &str) -> Result<Self, ConfigError>;
    pub fn from_args(args: &Args) -> Result<Self, ConfigError>;
}
```

### `instance/base.rs`

The core state machine:

```rust
pub struct NcpInstanceBase {
    // State
    ncp_state: Arc<RwLock<NcpState>>,
    enabled: Arc<AtomicBool>,
    interface_name: String,

    // Transport
    serial: FramedTransport<UartTransport>,

    // Task management
    task_queue: TaskQueue,
    current_task: Option<Box<dyn SpinelTask>>,

    // I/O
    tun: Option<TunnelIPv6Interface>,

    // Channels
    command_tx: mpsc::Sender<Command>,
    command_rx: mpsc::Receiver<Command>,
    state_changed: Notify,

    // Config
    config: Config,
}

impl NcpInstanceBase {
    pub async fn new(config: Config) -> Result<Self, DaemonError>;
    pub async fn run(&mut self, cancel: CancellationToken);
    pub async fn start_pumps(&mut self) -> Result<(), DaemonError>;
    pub async fn shutdown(&mut self) -> Result<(), DaemonError>;

    // State management
    pub fn set_ncp_state(&mut self, state: NcpState);
    pub fn get_ncp_state(&self) -> NcpState;

    // Task management
    pub fn start_new_task(&mut self, task: Box<dyn SpinelTask>);
    pub fn reset_tasks(&mut self, status: WpanError);

    // Command handling
    async fn handle_command(&mut self, cmd: Command) -> Result<Variant, DaemonError>;

    // Data pump
    async fn pump_ncp_to_driver(&mut self) -> Result<(), DaemonError>;
    async fn pump_driver_to_ncp(&mut self) -> Result<(), DaemonError>;
}
```

### `instance/addresses.rs`

From `NCPInstanceBase-Addresses.cpp`:

```rust
impl NcpInstanceBase {
    /// Add an IPv6 address to the TUN interface.
    pub async fn add_address(&mut self, addr: Ipv6Addr, prefix_len: u8) -> Result<(), DaemonError>;

    /// Remove an IPv6 address from the TUN interface.
    pub async fn remove_address(&mut self, addr: Ipv6Addr, prefix_len: u8) -> Result<(), DaemonError>;

    /// Refresh all addresses from NCP state.
    pub async fn refresh_addresses(&mut self) -> Result<(), DaemonError>;

    /// Handle NCP reporting a new address.
    pub fn on_ncp_address_added(&mut self, addr: Ipv6Addr, prefix_len: u8);

    /// Handle NCP reporting address removal.
    pub fn on_ncp_address_removed(&mut self, addr: Ipv6Addr, prefix_len: u8);
}
```

### `tasks/queue.rs`

```rust
pub struct TaskQueue {
    pending: VecDeque<Box<dyn SpinelTask>>,
    max_concurrent: usize,
}

impl TaskQueue {
    pub fn new() -> Self;
    pub fn push(&mut self, task: Box<dyn SpinelTask>);
    pub fn pop(&mut self) -> Option<Box<dyn SpinelTask>>;
    pub fn cancel_all(&mut self, status: WpanError);
    pub fn is_empty(&self) -> bool;
}
```

### `tasks/backoff.rs`

From `RunawayResetBackoffManager.cpp`:

```rust
pub struct BackoffManager {
    reset_count: u32,
    base_interval: Duration,
    max_interval: Duration,
    current_interval: Duration,
}

impl BackoffManager {
    pub fn new() -> Self;
    pub fn record_reset(&mut self);
    pub fn next_interval(&self) -> Duration;
    pub fn should_block(&self) -> bool;
    pub fn reset(&mut self);
}
```

### `firmware_upgrade.rs`

From `FirmwareUpgrade.cpp` — uses `fork()`:

```rust
pub async fn upgrade_firmware(
    ncp_path: &str,
    firmware_path: &str,
) -> Result<(), FirmwareError> {
    // In Rust, we'd use tokio::process::Command instead of fork()
    let status = Command::new(firmware_command)
        .arg(ncp_path)
        .arg(firmware_path)
        .status()
        .await?;

    if status.success() {
        Ok(())
    } else {
        Err(FirmwareError::UpgradeFailed(status.code().unwrap_or(-1)))
    }
}
```

## Tests

### Test 1: State Machine Transitions

```rust
#[test]
fn ncp_state_transitions() {
    let mut state = NcpState::Uninitialized;
    state.transition(NcpEvent::NcpReady);
    assert_eq!(state, NcpState::Offline);

    state.transition(NcpEvent::FormStarted);
    assert_eq!(state, NcpState::Associating);

    state.transition(NcpEvent::NetworkJoined);
    assert_eq!(state, NcpState::Associated);
}
```

### Test 2: Task Queue Ordering

```rust
#[test]
fn task_queue_fifo() {
    let mut queue = TaskQueue::new();
    queue.push(Box::new(MockTask::new("first")));
    queue.push(Box::new(MockTask::new("second")));

    assert_eq!(queue.pop().unwrap().name(), "first");
    assert_eq!(queue.pop().unwrap().name(), "second");
    assert!(queue.is_empty());
}
```

### Test 3: Config File Parsing (wpantund.conf format, NOT TOML)

```rust
#[test]
fn parse_wpantund_conf() {
    let content = r#"
# This is a comment
Config:TUN:InterfaceName "wfan0"
Config:NCP:SocketPath "/dev/ttyUSB0"
Config:NCP:DriverName "spinel"
Config:NCP:SocketBaud 115200
"#;
    let config = Config::parse(content).unwrap();
    assert_eq!(config.tun_interface_name, "wfan0");
    assert_eq!(config.nc_socket_path, "/dev/ttyUSB0");
    assert_eq!(config.nc_driver_name, "spinel");
    assert_eq!(config.nc_socket_baud, 115200);
}
```

### Test 4: Backoff Manager

```rust
#[test]
fn exponential_backoff() {
    let mut mgr = BackoffManager::new();
    assert_eq!(mgr.next_interval(), Duration::from_secs(1));

    mgr.record_reset();
    let interval = mgr.next_interval();
    assert!(interval > Duration::from_secs(1));

    mgr.record_reset();
    let interval2 = mgr.next_interval();
    assert!(interval2 > interval);
}
```

### Test 5: Address Management

```rust
#[tokio::test]
async fn address_add_remove() {
    let mut instance = setup_mock_instance().await;
    let addr: Ipv6Addr = "2020:abcd::1".parse().unwrap();

    instance.add_address(addr, 64).await.unwrap();
    let addrs = instance.list_addresses().await.unwrap();
    assert!(addrs.contains(&addr));

    instance.remove_address(addr, 64).await.unwrap();
    let addrs = instance.list_addresses().await.unwrap();
    assert!(!addrs.contains(&addr));
}
```

### Test 6: Daemon Startup with Mock Serial

```rust
#[tokio::test]
async fn daemon_starts_with_mock() {
    let pty = PtyPair::open().unwrap();
    let config = Config::mock(pty.slave_path());
    let mut instance = NcpInstanceBase::new(config).await.unwrap();

    let cancel = CancellationToken::new();
    let handle = tokio::spawn(async move {
        tokio::select! {
            _ = instance.run(&cancel) => {},
            _ = tokio::time::sleep(Duration::from_secs(2)) => {},
        }
    });
    handle.await.unwrap();
}
```

### Test 7: Command Handling

```rust
#[tokio::test]
async fn status_command() {
    let mut instance = setup_mock_instance().await;
    let result = instance.handle_command(Command::Status).await.unwrap();
    assert!(result.as_str().unwrap().contains("NCP:State"));
}
```

## Dependencies

```toml
[dependencies]
wisun-types = { path = "../wisun-types" }
spinel = { path = "../spinel" }
dcu-tun = { path = "../dcu-tun" }
dcu-serial = { path = "../dcu-serial" }
dcu-dbus = { path = "../dcu-dbus" }
tokio = { version = "1", features = ["full"] }
tokio-util = { version = "0.7", features = ["cancellation", "time"] }
bytes = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
thiserror = "2"
serde = { version = "1", features = ["derive"] }
nix = { version = "0.29", features = ["signal", "process"] }

[dev-dependencies]
dcu-mock = { path = "../dcu-mock" }
tempfile = "3"
```

## Verification Checklist

- [ ] Daemon starts, creates TUN interface, responds to signals
- [ ] All protothread patterns converted to async (grep for no remaining `PT_` references)
- [ ] Config file parsing handles all known config keys
- [ ] State machine handles all transitions from `doc/wpan-dbus-protocol.md`
- [ ] Task queue processes tasks in order
- [ ] Backoff manager exponential timing is correct
- [ ] Firmware upgrade via `tokio::process::Command`
- [ ] Graceful shutdown on SIGINT/SIGTERM
- [ ] `cargo test` passes
- [ ] `cargo clippy` produces zero warnings
- [ ] `unsafe` only in `firmware_upgrade.rs` (if fork needed) and integration tests
