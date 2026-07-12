# Phase 3B: `dcu-tunnel-daemon` — Spinel Tasks

## Overview

Port all Spinel NCP tasks. The C codebase uses protothreads
(`EH_BEGIN`/`EH_SPAWN`/`EH_WAIT_UNTIL`/`EH_END`) because C has no coroutines.
**Rust has async/await**, so the protothread machinery is eliminated entirely.

Phase 3A already delivered the async foundation these tasks build on
(see [phase-3A-daemon-core.md](phase-3A-daemon-core.md)):

- `io_task` — the combined serial I/O task (HDLC encode/decode + read/write).
- `ResponseTable` — TID → `oneshot::Sender<SpinelFrame>` matching.
- `NcpInstanceBase::send_command()` — allocate TID, send frame, await response.
- `NcpInstanceBase::run()` — the event loop (`tokio::select!` on cancel / commands / frames).

Phase 3B is the layer **on top of that foundation**: each C
`SpinelNCPTask*.cpp` becomes a straight `async fn` (or `async` method on
`NcpInstanceBase`), `handle_command()` is wired to dispatch D-Bus commands to
those functions, and the missing Spinel property constants the tasks need are
filled in.

In Rust, every NCP operation is an **async function**. The C protothread
`EH_SPAWN(&mSubPT, vprocess_send_command(...))` becomes `.await` on
`send_command()`. The C `EH_WAIT_UNTIL` / `EH_REQUIRE_WITHIN` become
`tokio::select!` with `tokio::time::timeout`. There are no enum state machines,
no `process_event`, no `TaskProgress`, no `finish` callback.

```rust
// tasks/leave.rs — straight async/await, built on the phase-3A foundation.
use spinel::command::{CMD_NET_CLEAR, CMD_PROP_VALUE_SET, CMD_RESET};
use spinel::property::prop_value_set;
use spinel::pack::PackWriter;
use crate::DaemonError;
use crate::instance::NcpInstanceBase;

/// NET property IDs required by this task (see "Prerequisites" below).
const SPINEL_PROP_NET_STACK_UP: u32 = 0x42;
const SPINEL_PROP_NET_IF_UP: u32 = 0x41;

/// Encode a Spinel bool payload ("b" → 1 byte: 0x00 false / 0x01 true).
fn bool_payload(value: bool) -> Vec<u8> {
    let mut w = PackWriter::new();
    w.write_bool(value);
    w.into_bytes()
}

pub async fn leave(ncp: &NcpInstanceBase) -> Result<(), DaemonError> {
    ncp.wait_for_state(|s| !s.is_initializing(), TIMEOUT).await?; // C: EH_REQUIRE_WITHIN
    ncp.send_command(CMD_PROP_VALUE_SET, prop_value_set(SPINEL_PROP_NET_STACK_UP, bool_payload(false)).payload).await?;
    ncp.send_command(CMD_PROP_VALUE_SET, prop_value_set(SPINEL_PROP_NET_IF_UP,   bool_payload(false)).payload).await?;
    ncp.send_command(CMD_NET_CLEAR, Vec::new()).await?;
    ncp.send_command(CMD_RESET,    Vec::new()).await?;
    ncp.wait_for_state(|s| !s.is_initializing(), TIMEOUT).await?;
    Ok(())
}
```

`send_command()` already encapsulates the full request/response cycle
(TID allocate → register in `ResponseTable` → send via `outbound_tx` → await
the `oneshot` with a timeout → unregister on drop), so a task body is just a
sequence of `send_command(...).await?` calls interleaved with state waits.

## Architecture (as built in phase 3A)

```text
Serial (UART/PTY)
    │  bytes
    ▼
io_task  (async fn in instance/base.rs, spawned by start_pumps())
    │  owns the dcu_serial::Transport, runs HDLC encode + decode
    │
    ├── inbound:  read bytes → HdlcDecoder → SpinelFrame::decode
    │             → frame_tx: mpsc::UnboundedSender<SpinelFrame>
    │
    └── outbound: outbound_rx: mpsc::UnboundedReceiver<SpinelFrame>
                  → HdlcEncoder → transport.write_all
    │
    ▼
NcpInstanceBase::run()  (tokio::select! on cancel / command_rx / frame_rx)
    │
    ├── frame_rx.recv() → ResponseTable::deliver(&frame)
    │     ├── TID matches a waiting task → oneshot::send → task's .await resumes
    │     └── no match (TID==0 or unsolicited) → unsolicited handler (phase 3B)
    │
    └── command_rx.recv() → handle_command(cmd)  (phase 3B wires the full set)
              └── calls the task async fns below, e.g. leave(&self).await
```

The single `io_task` replaces the C `SpinelNCPInstance-DataPump.cpp` pair
(`mDriverToNCPPumpPT` / `mNCPToDriverPumpPT`). There is **no separate
`DataPump` struct or `dispatcher/data_pump.rs` file** — the I/O task is a free
`async fn io_task<T: Transport + Unpin>(...)` in `instance/base.rs`.

### How the C protothread primitives map to async Rust

| C protothread                                         | Rust async/await (current)                                                       |
| ----------------------------------------------------- | -------------------------------------------------------------------------------- |
| `EH_BEGIN()` / `EH_END()`                             | function entry / return                                                          |
| `EH_SPAWN(&mSubPT, vprocess_send_command(...))`       | `self.send_command(cmd, payload).await?`                                         |
| `EH_WAIT_UNTIL(cond)`                                 | `tokio::select!` on `state_changed.notified()` + a `loop` that re-checks `cond`  |
| `EH_WAIT_UNTIL_WITH_TIMEOUT(cond, secs)`              | `tokio::time::timeout(dur, wait).await`                                          |
| `EH_REQUIRE_WITHIN(secs, cond, on_error)`             | `wait_for_state(\|s\| cond, timeout).await?` (helper, see below)                 |
| `EH_YIELD()`                                          | `tokio::task::yield_now().await`                                                 |
| `finish(ret)` / `finish(ret, value)`                  | `return Ok(value)` / `return Err(DaemonError::...)`                              |
| `EVENT_NCP_PROP_VALUE_IS` (unsolicited beacon/result) | unsolicited frame path in `run()` (TID==0) → dispatch to a collector in phase 3B |
| `mNextCommand` + `SpinelPackData(...)`                | `SpinelFrame` built via `spinel::property::prop_value_set(prop, payload)`        |

### `wait_for_state` helper (to add in phase 3B)

The C `EH_REQUIRE_WITHIN(... !ncp_state_is_initializing(...) ...)` pattern
repeats in nearly every task. Add one helper on `NcpInstanceBase` so task
bodies stay linear:

```rust
use std::time::Duration;
use tokio::time::timeout;
use wisun_types::NcpState;

const NCP_DEFAULT_COMMAND_RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);

impl NcpInstanceBase {
    /// Wait until `pred(get_ncp_state())` is true, or time out.
    /// Replaces C `EH_REQUIRE_WITHIN(secs, cond, on_error)`.
    ///
    /// Uses an absolute deadline (not per-notification reset) so the total
    /// wait never exceeds `dur`, matching the C semantics.
    pub async fn wait_for_state<F>(&self, pred: F, dur: Duration) -> Result<(), DaemonError>
    where
        F: Fn(NcpState) -> bool,
    {
        if pred(self.get_ncp_state().await) { return Ok(()); } // fast path
        let deadline = tokio::time::Instant::now() + dur;
        loop {
            let notified = self.state_changed.notified();
            tokio::pin!(notified);
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Err(DaemonError::Ncp("timeout waiting for NCP state".into()));
            }
            tokio::select! {
                _ = timeout(remaining, &mut notified) => {
                    if pred(self.get_ncp_state().await) { return Ok(()); }
                }
            }
        }
    }
}
```

> `NcpState` currently exposes `is_associated()`, `is_offline()`, `is_fault()`.
> A `is_initializing()` helper (true for **only** `Uninitialized`/`Upgrading`,
> per `NCPTypes.cpp:124` — **not** `Fault`) should be added to
> `wisun-types/src/ncp_state.rs`.

> **`mDriverState` gap.** The C also tracks a second state dimension,
> `SpinelNCPInstance::mDriverState` (`INITIALIZING` /
> `INITIALIZING_WAITING_FOR_RESET` / `NORMAL_OPERATION`, see
> `SpinelNCPInstance.h:122-125`). Several tasks' final wait checks
> `!ncp_state_is_initializing(...) && (mDriverState == NORMAL_OPERATION)`
> (e.g. `SpinelNCPTaskLeave.cpp:119-123`). The Rust port must add an
> equivalent field to `NcpInstanceBase` (e.g. `driver_state: DriverState`)
> and expose it to `wait_for_state` — either by widening the predicate to
> take the instance, or by adding a separate `wait_for_driver_ready()`
> helper. This is a phase-3B prerequisite not captured in the simple
> `wait_for_state(pred, dur)` signature above.

## Foundation already in place (phase 3A) — do not re-implement

These exist and are correct; phase 3B consumes them, it does not rebuild them.

| Component                                            | Location           | Status | Notes                                                                                             |
| ---------------------------------------------------- | ------------------ | ------ | ------------------------------------------------------------------------------------------------- |
| `io_task`                                            | `instance/base.rs` | Done   | Combined read+write I/O task; `tokio::select!` on cancel / outbound_rx / read.                    |
| `ResponseTable`                                      | `instance/base.rs` | Done   | `register(tid, oneshot::Sender)` / `deliver(&frame) -> bool` / `unregister(tid)`. 3 tests.        |
| `NcpInstanceBase::send_command(&self, cmd, payload)` | `instance/base.rs` | Done   | Allocates TID, builds header, registers oneshot, sends via `outbound_tx`, awaits with 5s timeout. |
| `alloc_tid()`                                        | `instance/base.rs` | Done   | Module-level `static AtomicU8`, wraps `1..=15` (matches `SPINEL_GET_NEXT_TID`). 1 test.           |
| `NcpInstanceBase::run()`                             | `instance/base.rs` | Done   | `tokio::select!` on `cancel` / `command_rx` / `frame_rx`. Unsolicited frames logged.              |
| `start_pumps()` / `stop()`                           | `instance/base.rs` | Done   | Opens `UartTransport`, spawns `io_task`, owns `io_cancel: CancellationToken`.                     |
| `BackoffManager`                                     | `tasks/backoff.rs` | Done   | Runaway-reset windowed quadratic delay. 1 test.                                                   |
| `handle_command()`                                   | `instance/base.rs` | Stub   | Handles `Reset` + `Leave` only; all others return `"unhandled"`. **Phase 3B fills this in.**      |

> **Note on the earlier trait-based design.** An earlier draft of the port plan
> proposed an `EventDrivenTask` trait with `process_event() -> TaskProgress`, a
> `TaskQueue`, an `NcpEvent` enum, and `dispatch_event()`/`schedule_next_task()`
> on the instance. **That design was discarded before any of it landed in the
> code** — `grep` confirms none of those symbols exist in `crates/dcu-tunnel-daemon`.
> There is nothing to remove; this phase simply never introduces them.

## Prerequisites (fill in before/early in phase 3B)

The task functions need Spinel constants that the Rust crates do not yet
define. Add them as the first step of this phase:

1. **NET-range property IDs** — `crates/spinel/src/property.rs` and/or
   `crates/wisun-types/src/property_key.rs` currently cover `LAST_STATUS` (0)
   through the MAC extended range (0x1300+) and the TI vendor range (0x3C00+),
   but **not** the standard NET range (`SPINEL_PROP_NET__BEGIN = 0x40`, per
   `third_party/openthread/src/ncp/spinel.h:2064`). Add at minimum:
   - `SPINEL_PROP_NET_SAVED: u32 = 0x40`
   - `SPINEL_PROP_NET_IF_UP: u32 = 0x41`
   - `SPINEL_PROP_NET_STACK_UP: u32 = 0x42`
   - `SPINEL_PROP_NET_ROLE: u32 = 0x43`
   - `SPINEL_PROP_NETWORK_KEY: u32 = 0x44` (confirm offset against spinel.h)
   - `SPINEL_PROP_NETWORK_KEY_INDEX: u32 = 0x45` (confirm offset against spinel.h)

2. **Scan properties** — `SPINEL_PROP_MAC_SCAN_BEACON` (the beacon property is
   a separate ID from `SCAN_MASK` 0x31 / `SCAN_PERIOD` 0x32 — confirm the exact
   value against `third_party/openthread/src/ncp/spinel.h`) and
   `SPINEL_SCAN_STATE_IDLE`/`SPINEL_SCAN_STATE_SCAN` enum values for the
   `PROP_MAC_SCAN_STATE` (0x30) payload.

3. **`NcpState::is_initializing()`** — add to
   `crates/wisun-types/src/ncp_state.rs`. Per `src/dcud/NCPTypes.cpp:124-133`,
   the C `ncp_state_is_initializing()` returns true for **only** `Uninitialized`
   and `Upgrading` — **not** `Fault`. Every task's entry guard uses this.

4. **`DriverState` enum + `driver_state` field** — the C `SpinelNCPInstance`
   tracks a second state dimension, `mDriverState` (`INITIALIZING` /
   `INITIALIZING_WAITING_FOR_RESET` / `NORMAL_OPERATION`, see
   `SpinelNCPInstance.h:122-125`). Several tasks' final wait condition is
   `!ncp_state_is_initializing(...) && (mDriverState == NORMAL_OPERATION)`.
   Add a `DriverState` enum and `driver_state` field to `NcpInstanceBase`,
   plus a `wait_for_driver_ready(timeout)` helper (or widen `wait_for_state`'s
   predicate to also inspect driver state).

> `PackWriter::write_bool` already exists in `crates/spinel/src/pack.rs`
> (encodes `bool` as a single byte, `0x00`/`0x01`, matching the Spinel `"b"`
> format) — no action needed.

## Tasks to implement (async functions)

Each C `SpinelNCPTask*.cpp` becomes an async function. The C's
`EH_SPAWN(&mSubPT, vprocess_send_command(...))` becomes `.await` on
`send_command()`. The C's `EH_WAIT_UNTIL`/`EH_REQUIRE_WITHIN` become
`wait_for_state()` or `tokio::select!` + `timeout`.

> **Important:** TI Wi-SUN has **no dedicated `CMD_FORM` / `CMD_JOIN` /
> `CMD_SCAN` command IDs**. Form/Join/Scan are all driven by
> `CMD_PROP_VALUE_SET` sequences against NET/MAC/vendor properties followed by
> waiting for `NcpState` transitions. The Spinel `command.rs` crate already
> contains every command ID these tasks use (`CMD_PROP_VALUE_GET/SET`,
> `CMD_NET_CLEAR`, `CMD_RESET`, `CMD_NOOP`, `CMD_PEEK`).

| Operation            | C file                                  | Async function                                                         | Wire sequence                                                                                                                                    |
| -------------------- | --------------------------------------- | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| Send command         | `SpinelNCPTaskSendCommand.cpp`          | `NcpInstanceBase::send_command()` (already done)                       | Sequential command lists via a `loop` over `Vec<(cmd, payload)>`. Lock-property helper wraps each batch.                                         |
| Leave                | `SpinelNCPTaskLeave.cpp`                | `async fn leave(&self) -> Result<(), DaemonError>`                     | `SET(NET_STACK_UP,false)` → `SET(NET_IF_UP,false)` → `NET_CLEAR` → `RESET` → `wait_for_state(!initializing)` ×2.                                 |
| Form                 | `SpinelNCPTaskForm.cpp`                 | `async fn form(&self, params) -> Result<(), DaemonError>`              | `NET_CLEAR` → set channel/PANID/name/key (vendor props) → `SET(NET_IF_UP,true)` → `SET(NET_STACK_UP,true)` → `wait_for_state(Associated)`.       |
| Join                 | `SpinelNCPTaskJoin.cpp`                 | `async fn join(&self, params) -> Result<(), DaemonError>`              | Like Form but with credentials → `wait_for_state(Associated)`. Retry loop on `CredentialsNeeded`.                                                |
| Scan                 | `SpinelNCPTaskScan.cpp`                 | `async fn scan(&self, params) -> Vec<ScanResult>`                      | `SET(MAC_SCAN_MASK,...)` → `SET(MAC_SCAN_STATE,SCAN)` → collect unsolicited `PROP_VALUE_IS(MAC_SCAN_BEACON)` until `SCAN_STATE_IDLE` or timeout. |
| DeepSleep            | `SpinelNCPTaskDeepSleep.cpp`            | `async fn deep_sleep(&self) -> Result<(), DaemonError>`                | `NOOP` → wait for quiet (`select!` until no NCP event for 0.5s) → `SET(MCU_POWER_STATE, LOW_POWER)` or `set_ncp_power(false)`.                   |
| Wake                 | `SpinelNCPTaskWake.cpp`                 | `async fn wake(&self) -> Result<(), DaemonError>`                      | `set_ncp_power(true)` → `SET(MCU_POWER_STATE, ON)` (if `CAP_MCU_POWER_STATE`) else `NOOP` → `wait_for_state(!sleeping)`.                         |
| HostDidWake          | `SpinelNCPTaskHostDidWake.cpp`          | `async fn host_did_wake(&self, tickle) -> Result<(), DaemonError>`     | Wait for init completion, optional `NOOP` tickle.                                                                                                |
| GetTopology          | `SpinelNCPTaskGetNetworkTopology.cpp`   | `async fn get_topology(&self) -> Vec<Node>`                            | `GET` vendor topology property → decode the `t(...)` struct array response.                                                                      |
| GetMsgBufferCounters | `SpinelNCPTaskGetMsgBufferCounters.cpp` | `async fn get_buffer_counters(&self) -> Counters`                      | `CMD_PROP_VALUE_GET(MSG_BUFFER_COUNTERS)` → decode with `PackFormat`.                                                                            |
| Peek                 | `SpinelNCPTaskPeek.cpp`                 | `async fn peek(&self, addr, count) -> Vec<u8>`                         | `CMD_PEEK` (addr+count) → await `CMD_PEEK_RET` response by TID.                                                                                  |
| JoinerCommissioning  | `SpinelNCPTaskJoinerCommissioning.cpp`  | `async fn joiner_commission(&self, params) -> Result<(), DaemonError>` | Several `PROP_VALUE_SET` (joiner credentials) then `SET(NET_IF_UP,true)`/`SET(NET_STACK_UP,true)`. Lock-property wrap.                           |

### Worked example: `leave` (tasks/leave.rs)

The C `SpinelNCPTaskLeave.cpp` is the simplest real task and the cleanest
demonstration of the mapping. The full C body is: guard on
`!initializing` → `SET(NET_STACK_UP,false)` → `SET(NET_IF_UP,false)` →
`NET_CLEAR` → `RESET` → wait for init to start → wait for init to finish.

```rust
use std::time::Duration;
use spinel::command::{CMD_NET_CLEAR, CMD_PROP_VALUE_SET, CMD_RESET};
use spinel::property::prop_value_set;
use spinel::pack::PackWriter;
use crate::DaemonError;
use crate::instance::NcpInstanceBase;

const TIMEOUT: Duration = Duration::from_secs(5);
const SPINEL_PROP_NET_STACK_UP: u32 = 0x42; // prerequisite: add to spinel/property.rs
const SPINEL_PROP_NET_IF_UP: u32 = 0x41;

fn bool_payload(v: bool) -> Vec<u8> {
    let mut w = PackWriter::new();
    w.write_bool(v);
    w.into_bytes()
}

pub async fn leave(ncp: &NcpInstanceBase) -> Result<(), DaemonError> {
    // C: EH_REQUIRE_WITHIN(!ncp_state_is_initializing && !is_initializing_ncp)
    ncp.wait_for_state(|s| !s.is_initializing(), TIMEOUT).await?;

    // Each EH_SPAWN(vprocess_send_command) → send_command().await?
    ncp.send_command(
        CMD_PROP_VALUE_SET,
        prop_value_set(SPINEL_PROP_NET_STACK_UP, bool_payload(false)).payload,
    ).await?;
    ncp.send_command(
        CMD_PROP_VALUE_SET,
        prop_value_set(SPINEL_PROP_NET_IF_UP, bool_payload(false)).payload,
    ).await?;
    ncp.send_command(CMD_NET_CLEAR, Vec::new()).await?;
    ncp.send_command(CMD_RESET,    Vec::new()).await?;

    // C: EH_REQUIRE_WITHIN(ncp_state_is_initializing)
    ncp.wait_for_state(|s| s.is_initializing(), TIMEOUT).await?;
    // C: EH_REQUIRE_WITHIN(!initializing && mDriverState == NORMAL_OPERATION)
    // The mDriverState check is a phase-3B prerequisite — see the note above.
    ncp.wait_for_state(|s| !s.is_initializing(), TIMEOUT).await?;
    ncp.wait_for_driver_ready(TIMEOUT).await?;
    Ok(())
}
```

### Worked example: `scan` (tasks/scan.rs) — the unsolicited-frame pattern

Scan is the one task that must collect **multiple unsolicited** frames
(beacons / energy results), not just await a single TID-matched response.
The beacons arrive as `PROP_VALUE_IS` frames with `TID == 0`, so they flow
through the unsolicited path in `run()`. Phase 3B must route those into a
per-scan collector channel:

```rust
use std::time::Duration;
use tokio::sync::mpsc;
use spinel::command::CMD_PROP_VALUE_SET;
use spinel::property::{prop_value_set, PROP_MAC_SCAN_MASK, PROP_MAC_SCAN_STATE};
use spinel::pack::PackWriter;
use crate::DaemonError;
use crate::instance::NcpInstanceBase;

const SCAN_TIMEOUT: Duration = Duration::from_secs(15);
const SPINEL_SCAN_STATE_SCAN: u8 = 1; // prerequisite: add scan-state enum
const SPINEL_SCAN_STATE_IDLE: u8 = 0;

pub async fn scan(ncp: &NcpInstanceBase, channel_mask: &[u8]) -> Result<Vec<ScanResult>, DaemonError> {
    let (beacon_tx, mut beacon_rx) = mpsc::channel(32);
    ncp.register_scan_collector(beacon_tx).await; // phase 3B: add to NcpInstanceBase

    ncp.send_command(
        CMD_PROP_VALUE_SET,
        prop_value_set(PROP_MAC_SCAN_MASK, channel_mask.to_vec()).payload,
    ).await?;

    let mut w = PackWriter::new();
    w.write_uint8(SPINEL_SCAN_STATE_SCAN);
    ncp.send_command(
        CMD_PROP_VALUE_SET,
        prop_value_set(PROP_MAC_SCAN_STATE, w.into_bytes()).payload,
    ).await?;

    // Collect beacons until SCAN_STATE_IDLE arrives or we time out.
    let mut results = Vec::new();
    let deadline = tokio::time::sleep(SCAN_TIMEOUT);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            Some(beacon) = beacon_rx.recv() => {
                if beacon.is_scan_done() { break; }
                results.push(ScanResult::from_beacon(&beacon));
            }
            _ = &mut deadline => {
                ncp.unregister_scan_collector().await;
                return Err(DaemonError::Ncp("scan timed out".into()));
            }
        }
    }
    ncp.unregister_scan_collector().await;
    Ok(results)
}
```

The `register_scan_collector` / `unregister_scan_collector` methods are new
plumbing on `NcpInstanceBase`: the unsolicited branch of `run()` (currently
just `tracing::trace!`) gains a small match on `frame.command_id` /
`property_id` to forward `PROP_VALUE_IS(MAC_SCAN_BEACON)` into the active
collector. Only one scan runs at a time, so a single `Option<mpsc::Sender>` slot
suffices.

### Wiring `handle_command()`

`handle_command()` in `instance/base.rs` is currently a stub returning
`"unhandled"` for everything except `Reset`/`Leave`. Phase 3B replaces the stub
with dispatch to the task functions. Because the tasks take `&self` and
`send_command` takes `&self` (it only needs `&ResponseTable` + `&outbound_tx`),
they can be called directly from `&mut self`:

```rust
pub async fn handle_command(&mut self, cmd: dcu_dbus::commands::Command) -> Result<String, DaemonError> {
    use dcu_dbus::commands::Command;
    match cmd {
        Command::Reset   => { /* send RESET, wait for init */ Ok(format!("NCP:State: {}", self.get_ncp_state().await)) }
        Command::Leave   => { crate::tasks::leave::leave(self).await?; Ok("Left network".into()) }
        Command::Form { .. }    => { crate::tasks::form::form(self, /*params*/).await?; Ok("Formed".into()) }
        Command::Join { .. }    => { crate::tasks::join::join(self, /*params*/).await?; Ok("Joined".into()) }
        other => { tracing::warn!("Unhandled command: {other:?}"); Ok("unhandled".into()) }
    }
}
```

> `handle_command` is `pub async fn` on `NcpInstanceBase` (it is called from
> `run()` in the same module). External integration tests can call it through
> the `NcpInstance` wrapper or directly if the crate re-exports it; otherwise
> unit tests in `instance/base.rs` (`mod tests`) are the natural home — see
> the phase-3A Test 7 note.

## Module layout (phase 3B additions)

```text
dcu-tunnel-daemon/src/
├── instance/
│   └── base.rs          # +wait_for_state(), +scan-collector slot, handle_command wired
├── tasks/
│   ├── mod.rs           # +pub mod leave; pub mod form; pub mod join; pub mod scan; ...
│   ├── backoff.rs       # (unchanged, phase 3A)
│   ├── send_command.rs  # (unchanged — STATUS_* constants; the method is on the instance)
│   ├── leave.rs         # NEW
│   ├── form.rs          # NEW
│   ├── join.rs          # NEW
│   ├── scan.rs          # NEW
│   ├── sleep.rs         # NEW (deep_sleep + wake + host_did_wake)
│   ├── topology.rs      # NEW (get_topology + get_buffer_counters)
│   └── peek.rs          # NEW
```

Each task module is a small file of free `async fn`s taking `&NcpInstanceBase`
(or `&self` methods if preferred). They return `Result<T, DaemonError>` and use
`?` for error propagation — no `TaskProgress`, no `finish()`.

## Implementation notes (read before coding each task)

The task table above gives the **wire-sequence summary** for each operation —
the happy path. It does **not** capture every edge case in the C source. Treat
the corresponding `src/ncp-spinel/SpinelNCPTask*.cpp` as the authoritative
reference when implementing that specific task. Concretely:

1. **Read each C `SpinelNCPTask*.cpp` end-to-end before porting it.** The
   summary table omits:
   - **Capability guards** — e.g. `SpinelNCPTaskForm.cpp:143` refuses to form
     unless `mCapabilities.count(SPINEL_CAP_ROLE_ROUTER)`; `SpinelNCPTaskWake.cpp:68`
     branches on `SPINEL_CAP_MCU_POWER_STATE`. The Rust tasks must query the
     NCP capability set (populated from `PROP_CAPS` during init) the same way.
   - **Error-label / `on_error` cleanup** — every C task has an `on_error:`
     label that calls `reinitialize_ncp()` (or a partial rollback) on failure.
     The Rust equivalent is `?` propagation up to `handle_command`, but the
     *rollback side-effects* (e.g. `mIsCommissioned = false` in Leave) must be
     replicated, not just the `Err` return.
   - **Retry loops** — `SpinelNCPTaskJoin.cpp` retries on
     `CredentialsNeeded`; `SpinelNCPTaskScan.cpp` has a `do { ... } while`
     beacon loop. These are control flow, not just a linear command list.

2. **The `scan` worked example uses illustrative types.** `ScanResult`,
   `is_scan_done()`, and `ScanResult::from_beacon()` are placeholders showing
   the *collector-channel pattern*. The real `ScanResult` struct shape must be
   derived from the beacon unpack format in `SpinelNCPTaskScan.cpp:221-237`
   (`"Cct(ESSC)t(iCUd)"` — channel, rssi, laddr, saddr, panid, lqi, proto,
   flags, networkid, xpanid). Decode that with `spinel::PackFormat`.

3. **`register_scan_collector` / `unregister_scan_collector` plumbing is
   conceptual, not fully specified.** The implementer must decide how `run()`'s
   unsolicited branch parses an incoming `PROP_VALUE_IS` frame: read the
   property ID from `frame.payload` via `PackReader::read_uint_packed()`,
   match it against `SPINEL_PROP_MAC_SCAN_BEACON`, and forward into the active
   `Option<mpsc::Sender<SpinelFrame>>` slot. Only one scan runs at a time, so a
   single slot suffices — but the slot must be cleared on drop/timeout to avoid
   a dangling sender.

4. **Verification of this doc is against the summary, not the full C.** The
   property IDs, state predicates, `mDriverState` requirement, and API names
   were cross-checked against `spinel.h`, `NCPTypes.cpp`, `SpinelNCPInstance.h`,
   and the Rust crates. The per-task edge cases above were **not** all
   line-by-line verified — that is the implementer's responsibility during
   porting.

## Verification Checklist

- [ ] NET-range Spinel property IDs (`NET_IF_UP` 0x41, `NET_STACK_UP` 0x42, …)
      added to `spinel/src/property.rs` (or `wisun-types` property_key.rs)
- [ ] Scan properties (`MAC_SCAN_BEACON`, `SCAN_STATE_*`) added
- [ ] `NcpState::is_initializing()` added to `wisun-types/src/ncp_state.rs`
      (true for **only** `Uninitialized` + `Upgrading`, not `Fault`)
- [ ] `DriverState` enum + `driver_state` field on `NcpInstanceBase`
      (`INITIALIZING` / `INITIALIZING_WAITING_FOR_RESET` / `NORMAL_OPERATION`),
      plus `wait_for_driver_ready(timeout)` helper
- [ ] `wait_for_state(pred, timeout)` helper on `NcpInstanceBase` (absolute
      deadline, not per-notification reset)
- [ ] `io_task` unchanged — still feeds `SpinelFrame` into `frame_rx`; `run()`
      delivers TID-matched frames via `ResponseTable`
- [ ] Response matching by TID works across concurrent async tasks via the
      shared `ResponseTable` (existing 3 tests still pass)
- [ ] TID allocated from the module-level `alloc_tid()` static, wrapping 1..=15
- [ ] `run()` event loop routes unsolicited `PROP_VALUE_IS` frames (TID==0) to
      the scan collector / property-change handlers without blocking responses
- [ ] Each C `SpinelNCPTask*.cpp` converted to a straight `async fn` with `?`
- [ ] `handle_command()` dispatches `Leave`/`Form`/`Join`/`Reset`/… to the task
      functions; no more `"unhandled"` for implemented commands
- [ ] No `EventDrivenTask`, `TaskProgress`, `NcpEvent`, `dispatch_event`,
      `schedule_next_task`, `TaskQueue`, `SpinelTask` trait, or `MockTask`
      introduced (these were the discarded design — keep it that way)
- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` produces zero warnings
