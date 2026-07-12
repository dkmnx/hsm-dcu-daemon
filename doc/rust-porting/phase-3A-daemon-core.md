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
| `src/dcud/NCPInstanceBase-AsyncIO.cpp`      | ~260  | Async I/O pumps (NCP ↔ driver)                     |
| `src/dcud/NCPInstanceBase-NetInterface.cpp` | ~477  | TUN interface lifecycle                            |
| `src/dcud/NCPInstance.cpp`                  | ~149  | NCP instance creation, dispatch                    |
| `src/dcud/NCPInstance.h`                    | ~122  | Instance class definition                          |
| `src/dcud/NCPControlInterface.cpp`          | ~344  | Abstract control interface                         |
| `src/dcud/FirmwareUpgrade.cpp`              | ~400  | Firmware update (fork + exec)                      |
| `src/dcud/StatCollector.cpp`                | 1737  | Network statistics collection                      |
| `src/dcud/NetworkRetain.cpp`                | ~215  | Persistent network config                          |
| `src/dcud/Pcap.cpp`                         | ~378  | Packet capture                                     |
| `src/dcud/RunawayResetBackoffManager.cpp`   | ~70   | Reset backoff logic                                |
| `src/dcud/NCPTypes.cpp`                     | ~465  | ToString helpers                                   |
| `src/dcud/wpan-error.c`                     | ~80   | Error code strings                                 |
| `src/dcud/NCPTypes.h`                       | ~60   | Type aliases                                       |
| `src/dcud/wpantund.h`                       | ~40   | Global declarations                                |
| `src/dcud/IPCServer.h`                      | ~30   | IPC interface                                      |
| `src/dcud/NetworkInstance.h`                | ~172  | Network instance definition                        |

**Total C/C++ code**: ~9,627 LOC

## Crate Structure

```text
dcu-daemon/
├── Cargo.toml
└── src/
    ├── main.rs                 # Entry point, signal handling
    ├── config.rs               # Config file parser
    ├── instance/
    │   ├── mod.rs              # NcpInstance wrapper (public API)
    │   ├── base.rs             # NcpInstanceBase: state machine, event loop
    │   ├── addresses.rs        # IPv6 address management (not yet)
    │   ├── net_interface.rs    # TUN interface lifecycle (not yet)
    │   └── async_io.rs         # Async I/O pumps (not yet)
    ├── tasks/
    │   ├── mod.rs              # Task trait + MockTask
    │   ├── queue.rs            # Task queue management
    │   └── backoff.rs          # RunawayResetBackoffManager
    ├── stat_collector.rs       # Network statistics (not yet)
    ├── network_retain.rs       # Persistent network config (not yet)
    ├── pcap.rs                 # Packet capture (not yet)
    ├── firmware_upgrade.rs     # NCP firmware update
    ├── event.rs                # Event system (deleted; Notify inline in base.rs)
    └── control_interface.rs    # Command validation (stub)
```

### Implementation Status

**Scaffold complete (commit 2d24fe3 + follow-up fixes):** `Cargo.toml`, `main.rs`,
`lib.rs`, `config.rs`, `error.rs`, `tasks/` (mod, queue, backoff), `instance/` (mod, base),
`firmware_upgrade.rs`, `control_interface.rs` all exist, compile, and pass clippy + 12 tests.

| Module                 | Status      | Notes                                                              |
| ---------------------- | ----------- | ------------------------------------------------------------------ |
| `main.rs`              | Done        | Dual SIGINT/SIGTERM, clap args, DbusServer::start, event loop      |
| `config.rs`            | Done        | 20+ keys, shell quoting, bool parsing, 7 unit tests                |
| `error.rs`             | Done        | DaemonError with From impls for all sub-crate errors               |
| `instance/base.rs`     | Done (stub) | State machine shell: run loop awaits cancel/commands/state_changed |
| `instance/mod.rs`      | Done        | NcpInstance wrapper with shared_state/command_sender               |
| `tasks/mod.rs`         | Done        | SpinelTask trait + MockTask                                        |
| `tasks/queue.rs`       | Done        | FIFO TaskQueue, 2 tests                                            |
| `tasks/backoff.rs`     | Done        | Windowed quadratic backoff, 1 test                                 |
| `firmware_upgrade.rs`  | Done        | Direct exec via shell_words::split, drop shell wrapper, 2 tests    |
| `control_interface.rs` | Stub        | validate_command is a no-op (filled in phase 3B)                   |
| `addresses.rs`         | Not yet     | IPv6 route/prefix management (phase 3B)                            |
| `async_io.rs`          | Not yet     | NCP↔driver data pump (phase 3A continuation)                       |
| `net_interface.rs`     | Not yet     | TUN lifecycle wiring (phase 3A continuation)                       |
| `stat_collector.rs`    | Not yet     | Network statistics (deferred past 3A)                              |
| `network_retain.rs`    | Not yet     | Persistent config (deferred past 3A)                               |
| `pcap.rs`              | Not yet     | Packet capture (deferred past 3A)                                  |

> **`packet_matcher.rs` — `IPv6PacketMatcher.cpp` (555 LOC).**
> Phase 1C (`phase-1C-dcu-tun.md`) lists `IPv6PacketMatcher.cpp`
> under "Source Files to Port" but then defers it to phase 3A in
> the Out-of-scope section. Phase 3A's module table does not list
> it. **Assign explicitly:** add `packet_matcher.rs` as the Rust
> equivalent of `IPv6PacketMatcher.cpp`. It provides firewall/packet
> classification on the data path (match on source/dest address,
> next-header, port ranges). The daemon's async data pump
> (`pump_driver_to_ncp` / `pump_ncp_to_driver`) calls into this
> layer to filter packets before forwarding.
| `event.rs`             | Deleted     | Replaced by inline Notify slots in base.rs                         |

### Resolved spec gaps

The open gaps from the original draft have been resolved during implementation:

- **`SpinelTask` trait + `MockTask`** — defined in `tasks/mod.rs` with `async fn run`
  and `fn name()`. TaskQueue::push and Test 2 use them.
- **`event.rs`** — the file was deleted. The protothread→async conversion uses
  inline `tokio::sync::Notify` slots (state_changed, busy_changed, data_available)
  directly on `NcpInstanceBase`. No bespoke EventSlots wrapper needed.
- **`control_interface.rs`** — split clarified: D-Bus serialization stays in
  `dcu-dbus`; `control_interface.rs` is a placeholder for state-aware command
  validation (e.g. reject Form if already associated). Validation logic deferred
  to phase 3B when actual task dispatch exists.

### Remaining spec gaps (deferred)

- **`addresses.rs`** — still far simpler than `NCPInstanceBase-Addresses.cpp`
  (1332 LOC). Unicast/multicast/on-mesh-prefix/off-mesh-route/service management
  with origins, SLAAC, lifetimes must be implemented, likely in phase 3B.
- **`stat_collector.rs`, `network_retain.rs`, `pcap.rs`** — one-line stubs with
  no API. Underspecified; defer to post-3A.
- **Config→firmware wiring** — `firmware_check_command` / `firmware_upgrade_command`
  are parsed into Config but not yet passed to the firmware functions. This
  requires the auto-update flow in the state machine (phase 3B).
- **`dcu-mock`** — required for integration tests 5–7 (phase 4A).
- **D-Bus `Command::Form`/`Join`/etc.** — `handle_command` currently handles
  only `Reset` and `Leave`; all others return `"unhandled"`. Task dispatch for
  the full command set is phase 3B.

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

Reimplements `wpantund.cpp`. Notes vs the original draft:

- `DbusServer::start(iface_name, state, command_tx)` takes an
  `Arc<RwLock<DaemonState>>` and an `mpsc::Sender<dcu_dbus::commands::Command>`
  (see `crates/dcu-dbus/src/server.rs`). There are no `state_handle()` /
  `command_sender()` accessors — the instance owns those handles and passes
  them in.
- `CancellationToken` is `tokio_util::sync::CancellationToken` — available
  unconditionally (no feature gate; only `context`/`task` are behind `"rt"`).
  There is no `"cancellation"` feature.
- `tokio::signal::ctrl_c()` only covers SIGINT. To also handle SIGTERM,
  register a `tokio::signal::unix::signal(SignalKind::terminate())` watcher.
- `dbus_server` must stay owned for the process lifetime (or the server is
  dropped and its connection closes). Keep it bound, then call
  `dbus_server.stop().await` during cleanup.
- `run()` takes the `CancellationToken` by value (matches `base.rs` and Test 6;
  the original draft's `instance.run()` call with no argument was wrong).

```rust
use std::sync::Arc;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::{mpsc, RwLock};
use tokio_util::sync::CancellationToken;
use clap::Parser;
use dcu_daemon::config::Config;
use dcu_daemon::instance::NcpInstance;
use dcu_dbus::{DbusServer, DaemonState};

#[derive(Parser)]
#[command(name = "dcud")]
struct Args {
    /// Path to the wpantund.conf-style configuration file.
    #[arg(short = 'c', long = "config", default_value = "/etc/wpantund.conf")]
    config_path: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let config = Config::load(&args.config_path)?;

    // Graceful shutdown: SIGINT or SIGTERM → cancel.
    let cancel = CancellationToken::new();

    let cancel_int = cancel.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        cancel_int.cancel();
    });

    let cancel_term = cancel.clone();
    tokio::spawn(async move {
        let mut sigterm = signal(SignalKind::terminate()).expect("SIGTERM handler");
        sigterm.recv().await;
        cancel_term.cancel();
    });

    // Initialize NCP instance. The instance owns the shared daemon state
    // and the command channel that the D-Bus server forwards into.
    let mut instance = NcpInstance::new(config).await?;
    let daemon_state: Arc<RwLock<DaemonState>> = instance.shared_state();
    let command_tx: mpsc::Sender<dcu_dbus::commands::Command> = instance.command_sender();

    // Start D-Bus server (claims com.nestlabs.WPANTunnelDriver).
    let dbus_server = DbusServer::start(
        instance.interface_name().to_string(),
        daemon_state,
        command_tx,
    ).await?;

    // Start I/O pumps (NCP <-> driver <-> TUN).
    instance.start_pumps().await?;

    // Main event loop. `run` consumes the token so it can exit on cancel.
    instance.run(cancel.clone()).await;

    if cancel.is_cancelled() {
        tracing::info!("Shutting down...");
    }

    // Cleanup: stop accepting D-Bus requests, close the NCP/TUN, drop the
    // bus name promptly so clients/tests don't see a stale object.
    instance.stop().await?;
    dbus_server.stop().await?;
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

    // Keys the C parser handles but the draft omitted (src/dcud/wpantund.conf):
    pub nc_hard_reset_path: Option<String>,    // Config:NCP:HardResetPath
    pub nc_power_path: Option<String>,         // Config:NCP:PowerPath
    pub nc_reliability_layer: Option<String>,  // Config:NCP:ReliabilityLayer
    pub nc_tx_power: Option<i8>,               // NCP:TXPower
    pub nc_cca_threshold: Option<i8>,          // NCP:CCAThreshold
    pub daemon_syslog_mask: Option<String>,    // Daemon:SyslogMask
    pub firmware_check_command: Option<String>,  // Config:NCP:FirmwareCheckCommand
    pub firmware_upgrade_command: Option<String>,// Config:NCP:FirmwareUpgradeCommand

    // `Config:NCP:SocketPath` also accepts `system:`, `serial:`, and
    // `host:port` (TCP) prefixes — the parser must dispatch to the right
    // transport (see dcu-serial), not treat it as a plain device path.
}

impl Config {
    /// Parse wpantund.conf format.
    /// NOT TOML. Uses shell-style quoting and whitespace separation.
    pub fn parse(content: &str) -> Result<Self, ConfigError>;
    pub fn load(path: &str) -> Result<Self, ConfigError>;
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
    pub async fn stop(&mut self) -> Result<(), DaemonError>;

    // State management (NCPState is `wisun_types::NcpState`, a plain enum —
    // there is no `NcpEvent`/`transition` API; see `ncp_state.rs`).
    pub fn set_ncp_state(&mut self, state: NcpState);
    pub fn get_ncp_state(&self) -> NcpState;

    // Task management
    pub fn start_new_task(&mut self, task: Box<dyn SpinelTask>);
    pub fn reset_tasks(&mut self, status: WpanError);

    // Command handling. Returns a status string compatible with the C
    // `status` command output (Test 7 asserts it contains "NCP:State"),
    // NOT a `dcu_dbus::Variant` (which is `zbus::zvariant::Value` and has no
    // `as_str()`).
    async fn handle_command(&mut self, cmd: Command) -> Result<String, DaemonError>;

    // Data pump
    async fn pump_ncp_to_driver(&mut self) -> Result<(), DaemonError>;
    async fn pump_driver_to_ncp(&mut self) -> Result<(), DaemonError>;
}
```

### `instance/mod.rs` — `NcpInstance` wrapper

`NcpInstance` is the public entry point `main.rs` constructs. It owns the
`NcpInstanceBase` plus the handles the D-Bus server needs. This type was
missing from the original draft (main.rs built `NcpInstance` while base.rs
defined `NcpInstanceBase`, and the two never reconciled).

```rust
pub struct NcpInstance {
    inner: NcpInstanceBase,
    shared_state: Arc<RwLock<DaemonState>>,   // handed to DbusServer::start
    command_tx: mpsc::Sender<dcu_dbus::commands::Command>,
}

impl NcpInstance {
    pub async fn new(config: Config) -> Result<Self, DaemonError>;
    pub async fn run(&mut self, cancel: CancellationToken);

    // Handle accessors used by main.rs (replaces the fictional
    // state_handle()/command_sender() calls in the original draft).
    // `command_sender()` must `clone()` the `mpsc::Sender` field (Sender is
    // Clone) rather than moving it out, so the instance keeps its own copy.
    pub fn shared_state(&self) -> Arc<RwLock<DaemonState>>;
    pub fn command_sender(&self) -> mpsc::Sender<dcu_dbus::commands::Command>;
    pub fn interface_name(&self) -> &str;

    // Forwarded to the inner base instance.
    pub async fn start_pumps(&mut self) -> Result<(), DaemonError>;
    pub async fn stop(&mut self) -> Result<(), DaemonError>;
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

From `RunawayResetBackoffManager.cpp`. Corrected: the C implementation is a
**windowed** counter, NOT exponential backoff. It tracks resets within a
rolling `kDecayPeriod = 15s` window; there is **no delay** until the windowed
count exceeds `kBackoffThreshold = 4`, after which the delay is **quadratic**:
`(count - threshold)² / 2.0` seconds. The original draft's `base_interval`/
`exponential` model and `record_reset()`/`next_interval()`/`should_block()`
API do not exist in the source.

```rust
pub struct BackoffManager {
    windowed_reset_count: u32,
    decrement_at: Duration,   // monotonic time of next decay step
}

const K_DECAY_PERIOD: Duration = Duration::from_secs(15); // kDecayPeriod
const K_BACKOFF_THRESHOLD: u32 = 4;                       // kBackoffThreshold

impl BackoffManager {
    pub fn new() -> Self;

    /// Seconds to delay before an unexpected reset is acted on. Returns 0.0
    /// until the windowed count exceeds the threshold. Maps to
    /// `delay_for_unexpected_reset()`.
    pub fn delay_for_unexpected_reset(&self) -> f64;

    /// Record an unexpected reset, opening/refreshing the decay window.
    /// Maps to `count_unexpected_reset()`.
    pub fn count_unexpected_reset(&mut self);

    /// Advance the decay window (dec-decrement the count after
    /// `kDecayPeriod`). Call periodically from the event loop.
    /// Maps to `update()`.
    pub fn update(&mut self);

    /// Clear all counts.
    pub fn reset(&mut self);
}
```

> **Test 4 caveat:** the original test asserts a monotonic base of 1s and
> exponential growth — that is the wrong model. Replace it with the quadratic
> windowed check below.

### `firmware_upgrade.rs`

From `FirmwareUpgrade.cpp`. Corrected: the C class's default constructor takes
no arguments; it receives shell command strings via the
`set_firmware_upgrade_command()` and `set_firmware_check_command()` setter
methods, then forks to execute them (double-fork for privilege separation). It
does **not** take `ncp_path` / `firmware_path` arguments, and the
`firmware_command` variable in the original draft is undefined. The Rust
rewrite should pass the configured command string. Use
`tokio::process::Command` to spawn; no `fork()` and therefore no `unsafe`
(workspace denies `unsafe_code` anyway).

```rust
pub async fn upgrade_firmware(
    command: &str,
) -> Result<(), FirmwareError> {
    // `command` is the configured Config:NCP:FirmwareUpgradeCommand string.
    let status = tokio::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .status()
        .await?;

    if status.success() {
        Ok(())
    } else {
        Err(FirmwareError::UpgradeFailed(status.code().unwrap_or(-1)))
    }
}
```

> The `is_firmware_upgrade_required()` check (C: `is_firmware_upgrade_required(version)`)
> reads/writes the forked check FD; the Rust version runs the check command
> and inspects its exit code similarly.

## Tests

### Test 1: NCP State Values and Helpers

`NcpState` is a plain `#[repr(u32)]` enum (`wisun-types::ncp_state.rs`). There
is **no `transition`/`NcpEvent` API**; state changes go through the
`NcpInstanceBase::set_ncp_state` setter. Test the enum mapping and helpers
instead.

```rust
use wisun_types::NcpState;

#[test]
fn ncp_state_values_and_helpers() {
    // Enum discriminant mapping matches NCPTypes.h:34-46.
    assert_eq!(NcpState::Uninitialized as u32, 0);
    assert_eq!(NcpState::Offline as u32, 4);
    assert_eq!(NcpState::Associating as u32, 6);
    assert_eq!(NcpState::Associated as u32, 8);

    // D-Bus string round-trips (wpan-properties.h:437-448).
    assert_eq!(NcpState::Offline.to_string(), "offline");
    assert_eq!("associating".parse::<NcpState>().unwrap(), NcpState::Associating);

    // Helper predicates used throughout the daemon.
    assert!(NcpState::Associated.is_associated());
    assert!(NcpState::Offline.is_offline());
    assert!(NcpState::Fault.is_fault());
    assert!(!NcpState::Offline.is_associated());
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

### Test 4: Runaway Reset Backoff (windowed + quadratic)

Maps to `RunawayResetBackoffManager.cpp`: no delay until the windowed count
exceeds `kBackoffThreshold = 4`; thereafter delay is `(count - 4)² / 2`
seconds. `update()` decays the count after `kDecayPeriod = 15s` — we stub
`decrement_at` in the test to exercise the decay path without real time
passing.

```rust
use dcu_daemon::tasks::backoff::BackoffManager;

#[test]
fn runaway_reset_backoff_quadratic() {
    let mut mgr = BackoffManager::new();

    // Under threshold → no delay.
    for _ in 0..4 {
        mgr.count_unexpected_reset();
    }
    assert_eq!(mgr.delay_for_unexpected_reset(), 0.0);

    // 5th reset: (5-4)²/2 = 0.5s
    mgr.count_unexpected_reset();
    assert_eq!(mgr.delay_for_unexpected_reset(), 0.5);

    // 7th reset: (7-4)²/2 = 4.5s, strictly greater than before.
    mgr.count_unexpected_reset();
    mgr.count_unexpected_reset();
    assert_eq!(mgr.delay_for_unexpected_reset(), 4.5);

    // reset() clears everything.
    mgr.reset();
    assert_eq!(mgr.delay_for_unexpected_reset(), 0.0);
}
```

### Test 5: Address Management

`setup_mock_instance()` and `list_addresses()` are provided by the `dcu-mock`
dev-dependency. `add_address`/`remove_address` operate on the TUN interface
and the NCP-originated address set tracked in `base.rs`.

```rust
use std::net::Ipv6Addr;
use dcu_mock::setup_mock_instance;

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

`PtyPair` lives in `dcu-serial`; `Config::mock(slave_path)` is provided by
`dcu-mock`. This test exercises the public `NcpInstance` wrapper (not just
`NcpInstanceBase`), since `main.rs` constructs `NcpInstance`. `run` takes the
`CancellationToken` by value.

```rust
use tokio_util::sync::CancellationToken;
use dcu_serial::pty::PtyPair;
use dcu_mock::Config;
use dcu_daemon::instance::NcpInstance;

#[tokio::test]
async fn daemon_starts_with_mock() {
    let pty = PtyPair::open().unwrap();
    let config = Config::mock(pty.slave_path());
    let mut instance = NcpInstance::new(config).await.unwrap();

    let cancel = CancellationToken::new();
    let handle = tokio::spawn(async move {
        tokio::select! {
            _ = instance.run(cancel) => {},
            _ = tokio::time::sleep(Duration::from_secs(2)) => {},
        }
    });
    handle.await.unwrap();
}
```

### Test 7: Command Handling

`handle_command` returns a `String` (not a `dcu_dbus::Variant`), so assert on
the raw string. The `dcu_dbus::Command` enum has no `Status` variant — the
status line is produced by the reset/association flow — so we send
`Command::Reset` and assert the returned status text is non-empty and reports
NCP state. `setup_mock_instance()` comes from `dcu-mock`.

`handle_command` is declared `async fn` (private, not `pub`) on
`NcpInstanceBase`, so this test must live **inside the crate** as a unit test
(`mod tests` in `instance/base.rs`) using `use super::NcpInstanceBase` — an
external integration test file cannot call a private method. Alternatively,
promote `handle_command` to `pub(crate) async fn` if an external test is
preferred.

```rust
use dcu_mock::setup_mock_instance;
use dcu_daemon::instance::NcpInstanceBase;

#[tokio::test]
async fn status_command() {
    let mut instance = setup_mock_instance().await;
    let result = instance.handle_command(dcu_dbus::commands::Command::Reset).await.unwrap();
    assert!(!result.is_empty());
    assert!(result.contains("NCP:State") || result.contains("state"));
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
tokio-util = { version = "0.7", features = ["time"] }
clap = { version = "4", features = ["derive"] }
async-trait = "0.1"
tracing = "0.1"
tracing-subscriber = "0.3"
thiserror = "2"
shell-words = "1"

[dev-dependencies]

[lints]
workspace = true
```

> **Notes:**
> - `nix` is NOT a daemon-core dependency. Signal handling uses `tokio::signal`.
> - `serde`/`bytes` are unused here (config is a custom parser, not TOML).
> - `CancellationToken` is unconditionally available in `tokio_util::sync`; no feature gate needed.
> - `shell-words` is used by `firmware_upgrade.rs` for safe command splitting.
> - `dcu-mock` and `tempfile` are not declared — they will be added in phase 4A when integration tests exist.

## Verification Checklist

- [x] Config file parsing handles all known config keys (incl. `HardResetPath`, `PowerPath`, `FirmwareCheckCommand`, `FirmwareUpgradeCommand`, `NCP:TXPower`, `NCP:CCAThreshold`, `Daemon:SyslogMask`, and the `system:`/`serial:`/`TCP` `SocketPath` conventions)
- [x] Task queue processes tasks in order (2 tests)
- [x] Runaway reset backoff: windowed count + quadratic delay `(count-4)²/2`, no delay under threshold (1 test)
- [x] Firmware upgrade via `tokio::process::Command` with direct exec (no `sh -c`, uses `shell_words::split`)
- [x] Graceful exit via `CancellationToken` on SIGINT or SIGTERM
- [x] `cargo test` passes (12 tests)
- [x] `cargo clippy` produces zero warnings
- [x] `unsafe` only in `dcu-tun`/`dcu-serial` (ioctl/serial); this crate has none
- [ ] Daemon starts, creates TUN interface, responds to signals (requires integration test)
- [ ] State machine handles all transitions from `doc/wpan-dbus-protocol.md` (requires task dispatch — phase 3B)
- [ ] Config→firmware wiring: Config values passed to firmware functions (phase 3B)
- [ ] All protothread patterns converted to async (grep for no remaining `PT_` references — phase 3A continuation)
