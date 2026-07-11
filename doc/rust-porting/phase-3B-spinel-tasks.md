# Phase 3B: `dcu-daemon` — Spinel Tasks

## Overview

Port all Spinel NCP tasks to the `EventDrivenTask` trait used by the phase 3A daemon core.
Tasks are **event sinks**: they receive `NcpEvent` from the dispatcher and return `TaskProgress`
to indicate whether they are done, yielding, or awaiting a child task.

## Architecture (matching the implemented code)

The daemon's event-driven task model already exists in `dispatcher/mod.rs`:

- **`NcpEvent`** — typed enum (`FrameReceived(SpinelFrame)`, `Alarm(AlarmId)`, `Starting`)
- **`EventDrivenTask`** — trait with `start()`, `process_event()`, `finish()`
- **`TaskProgress`** — `Yield`, `YieldWithFrames(Vec<SpinelFrame>)`, `Done(i32)`,
  `AwaitingChild(Box<dyn EventDrivenTask>, Vec<SpinelFrame>)`
- **`SendCommandTask`** — sends one command, matches response by TID (not full header byte)

| Aspect             | C (`SpinelNCPTaskSendCommand`)                                             | Rust (implemented)                                                  |
| ------------------ | -------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| Send model         | `mOutboundBuffer` + flush + wait (synchronous protothread)                 | `instance.enqueue_outbound_frame(frame)` + TID match (event-driven) |
| TID allocation     | `SPINEL_GET_NEXT_TID(mLastTID)` on instance, wraps at 15                   | `static AtomicU8` per task type, wraps at 15                        |
| Response matching  | Full header byte `mInboundHeader == mLastHeader`                           | TID only (`frame.tid() == self.tid`)                                |
| Lock property      | `EH_SPAWN` two commands around the main list (set true → list → set false) | Not yet implemented                                                 |
| Multi-command list | `while (mCommandIter != mCommandList.end())`                               | Single command only (to be extended)                                |

## Implemented (committed in phase 3A)

- **`dispatcher/mod.rs`** — `NcpEvent`, `EventDrivenTask`, `TaskProgress`, `AlarmId`
- **`dispatcher/data_pump.rs`** — `DataPump`: reads bytes from any `Transport`, feeds `HdlcDecoder`,
  sends `NcpEvent::FrameReceived` into an `mpsc::UnboundedSender`
- **`tasks/send_command.rs`** — `SendCommandTask`: sends one command with a TID, waits for
  matching response by TID, stashes the response frame for `finish()`
- **`instance/base.rs`** — event loop with `dispatch_event()`/`schedule_next_task()`,
  `outbound_queue: VecDeque<SpinelFrame>`, `enqueue_outbound_frame()`, `event_pump_tx()`,
  `mock_for_testing()` for unit tests

## Remaining tasks to port (this phase)

All follow the same `EventDrivenTask` pattern. Each protothread in C becomes a Rust struct
with an enum state machine and `start()`/`process_event()`/`finish()`.

| Task file (C)                           | Built by   | Complexity   | Notes                                                |
| --------------------------------------- | ---------- | ------------ | ---------------------------------------------------- |
| `SpinelNCPTaskSendCommand.cpp`          | ✅ Done     | 410 LOC      | Single command, TID match, stashed response          |
| `SpinelNCPTaskScan.cpp`                 | ❌          | ~250 LOC     | Send CMD_SCAN, collect results until CMD_SCAN_DONE   |
| `SpinelNCPTaskForm.cpp`                 | ❌          | ~250 LOC     | Sequence of PROP_VALUE_SET + CMD_FORM, channel retry |
| `SpinelNCPTaskJoin.cpp`                 | ❌          | ~200 LOC     | Set credentials + CMD_JOIN, wait for state change    |
| `SpinelNCPTaskLeave.cpp`                | ❌          | ~100 LOC     | Send CMD_LEAVE, wait for state change                |
| `SpinelNCPTaskDeepSleep.cpp`            | ❌          | ~200 LOC     | Enter low-power mode                                 |
| `SpinelNCPTaskWake.cpp`                 | ❌          | ~100 LOC     | Wake from deep sleep                                 |
| `SpinelNCPTaskHostDidWake.cpp`          | ❌          | ~80 LOC      | Notify NCP host woke                                 |
| `SpinelNCPTaskGetNetworkTopology.cpp`   | ❌          | ~300 LOC     | Request routing table                                |
| `SpinelNCPTaskGetMsgBufferCounters.cpp` | ❌          | ~100 LOC     | Request buffer stats                                 |
| `SpinelNCPTaskPeek.cpp`                 | ❌          | ~80 LOC      | Peek at NCP memory                                   |
| `SpinelNCPTaskJoinerCommissioning.cpp`  | ❌          | ~200 LOC     | Joiner flow                                          |

## Enhancements to existing `SendCommandTask` (before building more tasks)

The current `SendCommandTask` handles only a single command. The C `SpinelNCPTaskSendCommand`
supports:

1. **Command list iteration** — `Factory::add_command()` chains multiple commands. Each
   is sent sequentially, sharing the same TID. `process_event` advances through the list.
2. **Lock property** — before the list, send `PROP_VALUE_SET(ALLOW_LOCAL_NET_DATA_CHANGE, true)`;
   after the list (even on error), send `PROP_VALUE_SET(ALLOW_LOCAL_NET_DATA_CHANGE, false)`.
3. **Reply unpacker** — the final response is decoded by a custom `ReplyUnpacker` callback.
4. **Check handler** — after the list, yield until `mCheckHandler() == true` (or timeout).

These should be added to `SendCommandTask` before implementing the specific tasks that
depend on them (Scan, Form, Join all use `SendCommandTask` via `Factory`).

## Tests

- **send_command_matches_response_by_tid** — start, receive matching TID, assert Done
- **send_command_ignores_wrong_tid** — start, receive wrong TID, assert Yield
- **send_command_times_out** — start, receive Alarm with matching ID, assert Done(TIMEOUT)

Remaining: command list iteration, lock property, reply unpacker, check handler.

## Verification Checklist

- [ ] `SendCommandTask` iterates command list (matching C `while (mCommandIter != mCommandList.end())`)
- [ ] Lock property set/unset wraps the command sequence (C `mLockProperty != 0` branch)
- [ ] Reply unpacker extracts one value from final response
- [ ] Check handler + timeout (C `mCheckTimeout != 0` branch) yields until condition is true
- [ ] Scan task: sends CMD_SCAN, collects scan results until CMD_SCAN_DONE or timeout
- [ ] Form task: sequences PROP_VALUE_SET commands → CMD_FORM → wait for Associated
- [ ] Join task: sets credentials via PROP_VALUE_SET → CMD_JOIN → wait for Associated
- [ ] Leave task: CMD_LEAVE → state transition
- [ ] DeepSleep/Wake/HostDidWake tasks ported
- [ ] GetTopology, GetMsgBufferCounters, Peek tasks ported
- [ ] JoinerCommissioning task ported
- [ ] `cargo test` passes
- [ ] `cargo clippy` produces zero warnings
