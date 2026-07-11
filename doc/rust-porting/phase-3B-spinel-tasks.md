# Phase 3B: `dcu-daemon` — Spinel Tasks

## Overview

Port all Spinel NCP tasks. The C codebase uses protothreads (`EH_BEGIN`/`EH_SPAWN`/`EH_END`)
because C has no coroutines. **Rust has async/await**, so the protothread machinery is eliminated.

In Rust, every NCP operation is an **async free function** (or method on `NcpInstanceBase`):

```rust
async fn leave(ncp: &NcpInstanceBase) -> Result<(), SpinelError> {
    ncp.send_prop_set(PROP_NET_STACK_UP, false).await?;
    ncp.send_prop_set(PROP_NET_IF_UP, false).await?;
    ncp.send_cmd(NET_CLEAR).await?;
    ncp.send_cmd(RESET).await?;
    ncp.wait_for_state(|s| s == Init).await?;
    Ok(())
}
```

No enum state machines, no `process_event`, no `TaskProgress`, no `finish` callback.
The C protothread `EH_BEGIN`/`EH_WAIT_UNTIL`/`EH_SPAWN` → tokio `select!` + `await`.

## Architecture

```text
Serial (UART/PTY)
    │  bytes
    ▼
DataPump (separate tokio task)
    │  SpinelFrame
    ▼
mpsc::UnboundedSender<SpinelFrame>
    │
    ├── NcpInstanceBase::run()  (receives unsolicited frames, dispatches properties)
    │
    └── Per-command response channels
            Each async task sends a frame with a TID, then awaits on a
            TID-indexed slot for the matching response:
                send(TID=5, CMD).await;
                let resp = recv(TID=5).await;
                process(resp);
```

### Key differences from C (protothreads) and from the earlier EventDrivenTask design

| Aspect          | C protothreads                                          | Earlier Rust (discarded)                                                | Rust async/await (current)                                       |
| --------------- | ------------------------------------------------------- | ----------------------------------------------------------------------- | ---------------------------------------------------------------- |
| State machine   | `EH_BEGIN`/`EH_WAIT_UNTIL`/`EH_END` macros              | `EventDrivenTask` trait with `process_event()` returning `TaskProgress` | Straight async functions with `?`                                |
| Command sending | `mNextCommand` copy → `EH_SPAWN(vprocess_send_command)` | `SendCommandTask` (struct + trait impl)                                 | `send_frame(frame)` → await matching TID                         |
| TID matching    | `mInboundHeader == mLastHeader`                         | `frame.tid() == self.tid`                                               | Same, via shared TID response table                              |
| NCP events      | Flat `u32` with `IS_EVENT_FROM_NCP` mask                | `enum NcpEvent`                                                         | `SpinelFrame` over channels                                      |
| Dispatcher      | `vprocess_event` chain, task scheduler                  | `dispatch_event()`/`schedule_next_task()` on `NcpInstanceBase`          | No dispatcher needed — `run()` only processes unsolicited frames |

## What to keep from the existing code

- **`DataPump`** (`dispatcher/data_pump.rs`) — reads bytes, decodes HDLC, produces frames. The pump task pushes `SpinelFrame` into an `mpsc::UnboundedSender` that `NcpInstanceBase` owns. Keep as-is.

- **`SendCommandTask`** (`tasks/send_command.rs`) — the request/response matching by TID is a utility, not a trait. Instead of the `EventDrivenTask` trait, expose a free async function:

```rust
/// Send a frame to the NCP and await the matching response by TID.
/// The TID is allocated from the instance's shared counter.
async fn send_command(
    instance: &mut NcpInstanceBase,
    command_id: u32,
    payload: Vec<u8>,
) -> Result<SpinelFrame, DaemonError>
{
    let tid = instance.allocate_tid();
    let frame = SpinelFrame::with_header(make_header(0, tid), command_id, payload);
    instance.send_frame(frame).await;
    instance.await_response(tid).await
}
```

- **TID allocation** — instance-scoped atomic counter wrapping 1..15 (already correct in `send_command.rs`).

- **`NcpInstanceBase::run()`** — the event loop. Remove `dispatch_event()`/`schedule_next_task()`. Instead, it owns the response table and dispatches incoming frames either to a waiting task's response slot or to the unsolicited handler.

- **`enqueue_outbound_frame()`** — replace with an `mpsc` channel to the `DataPump` (or direct write if the pump polls a queue). The pump drains this channel and writes bytes to the serial transport.

## What to remove (dead code)

- `EventDrivenTask` trait — entirely replaced by `async fn`
- `TaskProgress` enum — replaced by `Result`
- `NcpEvent` enum — replaced by `SpinelFrame` over channels
- `dispatch_event()` — deleted
- `schedule_next_task()` — deleted
- `TaskQueue` over `EventDrivenTask` — keep as `VecDeque<Box<dyn Future>>` if queuing is needed, or drop entirely
- `AlarmId` — use `tokio::time::sleep` in async tasks
- `AwaitingChild` — use `tokio::spawn` + `JoinHandle`
- `DataPump::run` signature — takes a `mpsc::Sender<SpinelFrame>` instead of `mpsc::UnboundedSender<NcpEvent>`

## Remaining tasks (async functions)

Each C `SpinelNCPTask*.cpp` file becomes an async function. The C's `EH_SPAWN(&mSubPT, vprocess_send_command(...))` becomes `.await` on `send_command()`. The C's `EH_WAIT_UNTIL` becomes `tokio::select!` or polling.

| Operation            | C file                                  | Async function                                      | Notes                                                                                    |
| -------------------- | --------------------------------------- | --------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| Send command         | `SpinelNCPTaskSendCommand.cpp`          | `send_command()` utility                            | Sequential command lists via loop. Lock property via helper.                             |
| Scan                 | `SpinelNCPTaskScan.cpp`                 | `async fn scan(ncp, params) -> Vec<ScanResult>`     | Send `CMD_SCAN`, collect results until `CMD_SCAN_DONE` or timeout.                       |
| Form                 | `SpinelNCPTaskForm.cpp`                 | `async fn form(ncp, params) -> Result`              | Sequence: `PROP_VALUE_SET` × N → `CMD_FORM` → wait for `Associated`. Channel retry loop. |
| Join                 | `SpinelNCPTaskJoin.cpp`                 | `async fn join(ncp, params) -> Result`              | Similar to Form but `CMD_JOIN` + credentials.                                            |
| Leave                | `SpinelNCPTaskLeave.cpp`                | `async fn leave(ncp) -> Result`                     | `NET_STACK_UP(false)` → `NET_IF_UP(false)` → `NET_CLEAR` → `RESET` → wait for init.      |
| DeepSleep            | `SpinelNCPTaskDeepSleep.cpp`            | `async fn deep_sleep(ncp) -> Result`                | Send `NOOP`, wait for quiet, `PROP_MCU_POWER_STATE(LOW_POWER)`.                          |
| Wake                 | `SpinelNCPTaskWake.cpp`                 | `async fn wake(ncp) -> Result`                      | `PROP_MCU_POWER_STATE(ON)` or `NOOP` + wait for `!sleeping`.                             |
| HostDidWake          | `SpinelNCPTaskHostDidWake.cpp`          | `async fn host_did_wake(ncp, tickle) -> Result`     | Wait for init completion, optional `NOOP`.                                               |
| GetTopology          | `SpinelNCPTaskGetNetworkTopology.cpp`   | `async fn get_topology(ncp) -> Vec<Node>`           | Send `CMD_NET_TOPOLOGY_GET`, decode response.                                            |
| GetMsgBufferCounters | `SpinelNCPTaskGetMsgBufferCounters.cpp` | `async fn get_buffer_counters(ncp) -> Counters`     | Send `PROP_VALUE_GET(MSG_BUFFER_COUNTERS)`, decode.                                      |
| Peek                 | `SpinelNCPTaskPeek.cpp`                 | `async fn peek(ncp, addr, count) -> Vec<u8>`        | Send `CMD_PEEK`, await `PEEK_RET`.                                                       |
| JoinerCommissioning  | `SpinelNCPTaskJoinerCommissioning.cpp`  | `async fn joiner_commission(ncp, params) -> Result` | Several PROP_VALUE_SET + `CMD_JOINER_COMMISSION`.                                        |

## Verification Checklist

- [ ] `DataPump` still runs, now feeding `SpinelFrame` (not `NcpEvent`)
- [ ] Response matching by TID works across concurrent async tasks via shared response table
- [ ] TID allocated from instance-scoped counter wrapping 1..15
- [ ] `run()` event loop processes unsolicited frames (property IS, state changes) without blocking response delivery
- [ ] Each task's C protothread converted to straight async/await with `?`
- [ ] No remaining `EventDrivenTask`, `TaskProgress`, `NcpEvent`, `dispatch_event`, `schedule_next_task` references
- [ ] `cargo test` passes
- [ ] `cargo clippy` produces zero warnings
