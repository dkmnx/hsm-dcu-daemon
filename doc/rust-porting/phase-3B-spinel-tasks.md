# Phase 3B: `dcu-daemon` — Spinel Tasks

## Overview

Port all Spinel NCP tasks. Tasks are **event sinks** driven by a single serial byte-reader data pump — NOT autonomous frame readers.
The C equivalent: `SpinelNCPTask::vprocess_event(int event, va_list args)` is called by the NCP instance's event loop with events like
`EVENT_NCP_DATA_READ`, `EVENT_STARTING_TASK`, etc. The task never owns or touches the serial transport directly.

**Replaces**: `src/ncp-spinel/SpinelNCPTask*.cpp` (13 task files)

**Effort**: 7-10 days

## Actual task files in C codebase

```text
SpinelNCPTaskSendCommand.cpp  SpinelNCPTaskScan.cpp      SpinelNCPTaskForm.cpp
SpinelNCPTaskJoin.cpp         SpinelNCPTaskLeave.cpp     SpinelNCPTaskDeepSleep.cpp
SpinelNCPTaskWake.cpp         SpinelNCPTaskHostDidWake.cpp
SpinelNCPTaskGetNetworkTopology.cpp  SpinelNCPTaskGetMsgBufferCounters.cpp
SpinelNCPTaskPeek.cpp         SpinelNCPTaskJoinerCommissioning.cpp
```

## Architecture (Event Sink, NOT Frame Reader)

```text
NCP (UART) → SerialPump → HDLC Decoder → SpinelFrame
                                              ↓
                                   Event Dispatcher
                                              ↓
                                   ActiveTask::process_event(event)
                                              ↓
                                   Decision: yield | finish(cb) | spawn(task)
```

Tasks DO NOT own the serial transport. The serial is owned by a single `DataPump` that:
1. Reads bytes from UART
2. Assembles HDLC frames
3. Produces `SpinelFrame` → wrapped as `NcpEvent::FrameReceived`
4. Dispatches to the active task's `process_event()`

A task's `process_event()` is called reentrantly — each call may advance the internal state machine.
The task yields by returning `TaskProgress::Yield`, and completes by calling its `finish()` callback.

### Task (Event Sink) Trait

```rust
#[derive(Debug)]
pub enum NcpEvent {
    /// A complete Spinel frame has been received from the NCP.
    FrameReceived(SpinelFrame),

    /// Timer/alarm fired (set by task in `start()`).
    Alarm(AlarmId),

    /// Task was started (sent to current task when it becomes active).
    Starting,

    /// A child subtask completed.
    ChildFinished { status: i32, value: Option<Box<dyn Any>> },

    /// An uncategorized event from the daemon.
    Custom(i32, Vec<Box<dyn Any>>),
}

#[derive(Debug)]
pub enum TaskProgress {
    /// Task is still running; call process_event again with next event.
    Yield,

    /// Task completed. The dispatcher should remove it and call finish().
    Done(i32),

    /// Task spawned a child and is waiting for it.
    AwaitingChild(Box<dyn EventDrivenTask>),
}

pub trait EventDrivenTask: Send {
    fn name(&self) -> &str;

    /// Called when the task becomes active in the scheduler.
    fn start(&mut self, instance: &mut NcpInstanceBase) -> TaskProgress;

    /// Called with each event from the dispatcher.
    /// Returns Yield (still waiting), Done (completed), or AwaitingChild.
    fn process_event(&mut self, event: NcpEvent, instance: &mut NcpInstanceBase) -> TaskProgress;

    /// Called to finalize the task and invoke its user callback.
    fn finish(self: Box<Self>, status: i32, value: Option<Box<dyn Any>>);
}
```

### Dispatcher (in `SpinelNCPInstance::run()`)

```rust
// Pseudocode for the event dispatch loop in NcpInstanceBase::run():
let mut active_task: Option<Box<dyn EventDrivenTask>> = None;

// Single serial reader pump
loop {
    tokio::select! {
        frame = framed_transport.recv_frame() => {
            let frame = frame?;
            if let Some(task) = &mut active_task {
                match task.process_event(NcpEvent::FrameReceived(frame), &mut instance) {
                    TaskProgress::Done(status) => {
                        let task = active_task.take().unwrap();
                        task.finish(status, None);
                        // Schedule next task from queue
                        active_task = task_queue.pop();
                        if let Some(ref mut t) = active_task {
                            t.start(&mut instance);
                        }
                    }
                    TaskProgress::Yield => {}
                    TaskProgress::AwaitingChild(child) => {
                        task_queue.push_front(child);
                    }
                }
            }
        }
        _ = cancel.cancelled() => break,
    }
}
```

## Source Files to Port

| C/C++ File                                             | LOC  | What to Extract                                 |
| ------------------------------------------------------ | ---- | ----------------------------------------------- |
| `src/ncp-spinel/SpinelNCPTask.cpp`                     | ~200 | Base task class, event dispatch                 |
| `src/ncp-spinel/SpinelNCPTask.h`                       | ~70  | Task class definition                           |
| `src/ncp-spinel/SpinelNCPInstance-Protothreads.cpp`    | 693  | State machine protothreads                      |
| `src/ncp-spinel/SpinelNCPInstance-DataPump.cpp`        | ~400 | Data pump: serial → HDLC → frame, event disp    |
| `src/ncp-spinel/SpinelNCPControlInterface.cpp`         | 1246 | Property get/set over Spinel                    |
| `src/ncp-spinel/SpinelNCPInstance.cpp`                 | 7710 | Main plugin class (property handling, dispatch) |
| `src/ncp-spinel/SpinelNCPTaskSendCommand.cpp`          | ~300 | Send command, wait for response                 |
| `src/ncp-spinel/SpinelNCPTaskScan.cpp`                 | ~250 | Channel scan task                               |
| `src/ncp-spinel/SpinelNCPTaskForm.cpp`                 | ~250 | Form network task                               |
| `src/ncp-spinel/SpinelNCPTaskJoin.cpp`                 | ~200 | Join network task                               |
| `src/ncp-spinel/SpinelNCPTaskLeave.cpp`                | ~100 | Leave network task                              |
| `src/ncp-spinel/SpinelNCPTaskDeepSleep.cpp`            | ~200 | Enter deep sleep task                           |
| `src/ncp-spinel/SpinelNCPTaskWake.cpp`                 | ~100 | Wake from sleep task                            |
| `src/ncp-spinel/SpinelNCPTaskHostDidWake.cpp`          | ~80  | Host wake notification                          |
| `src/ncp-spinel/SpinelNCPTaskGetNetworkTopology.cpp`   | ~300 | Get routing table                               |
| `src/ncp-spinel/SpinelNCPTaskGetMsgBufferCounters.cpp` | ~100 | Get message buffer statistics                   |
| `src/ncp-spinel/SpinelNCPTaskPeek.cpp`                 | ~80  | Peek at memory on NCP                           |
| `src/ncp-spinel/SpinelNCPTaskJoinerCommissioning.cpp`  | ~200 | Joiner commissioning flow                       |
| `src/ncp-spinel/SpinelNCPVendorCustom.cpp`             | ~300 | TI vendor-specific operations                   |
| `src/ncp-spinel/SpinelNCPThreadDataset.cpp`            | ~150 | Thread dataset handling                         |

**Total C/C++ code**: ~12,749 LOC

## Crate Structure

This phase adds to `dcu-daemon/src/`:

```text
dcu-daemon/src/
├── ... (files from phase 3A)
├── dispatcher/
│   ├── mod.rs          # Event dispatcher, task scheduler
│   ├── data_pump.rs    # Single serial reader → HDLC → events
│   └── events.rs       # NcpEvent enum, EventDrivenTask trait
└── tasks/
    ├── mod.rs          # Re-exports
    ├── send_command.rs # Base: send command, wait for response
    ├── scan.rs         # Channel scan
    ├── form.rs         # Form network
    ├── join.rs         # Join network
    ├── leave.rs        # Leave network
    ├── deep_sleep.rs   # Enter deep sleep
    ├── wake.rs         # Wake from sleep
    ├── host_did_wake.rs# Host wake notification
    ├── get_topology.rs # Get network topology / routing table
    ├── get_msg_buffer_counters.rs
    ├── peek.rs         # Peek at memory on NCP
    └── joiner_commission.rs
```

## Detailed Task Specs

### `send_command.rs` — Base SendCommand Task

This is the most commonly used task. Sends a command and waits for a matching response frame.

```rust
pub struct SendCommandTask {
    command_id: u32,
    payload: Vec<u8>,
    expected_prop_key: Option<spinel_prop_key_t>,
    tid: u8,
    frame_sent: bool,
    timeout_alarm_id: Option<AlarmId>,
    callback: CallbackWithStatusArg1,
}

impl EventDrivenTask for SendCommandTask {
    fn name(&self) -> &str { "SendCommand" }

    fn start(&mut self, instance: &mut NcpInstanceBase) -> TaskProgress {
        // 1. Serialize frame: header(FLAG|IID_0|self.tid) + packed(command_id) + payload
        // 2. Queue the frame bytes for the data pump to send
        // 3. Set a timeout alarm
        // Returns Yield — waiting for response
        let frame = SpinelFrame::new_with_tid(self.tid, self.command_id, self.payload.clone());
        instance.send_frame(frame);
        self.frame_sent = true;
        TaskProgress::Yield
    }

    fn process_event(&mut self, event: NcpEvent, instance: &mut NcpInstanceBase) -> TaskProgress {
        match event {
            NcpEvent::FrameReceived(frame) => {
                // Match response by TID
                if frame.tid() != self.tid {
                    return TaskProgress::Yield; // Not our response
                }
                match frame.command_id {
                    CMD_PROP_VALUE_IS => {
                        if let Some(key) = self.expected_prop_key {
                            let prop_key = frame.read_uint_packed();
                            if prop_key == key {
                                return TaskProgress::Done(STATUS_OK);
                            }
                        }
                        return TaskProgress::Done(STATUS_OK);
                    }
                    CMD_LAST_STATUS => {
                        let status = frame.read_uint_packed();
                        return TaskProgress::Done(status as i32);
                    }
                    _ => TaskProgress::Yield,
                }
            }
            NcpEvent::Alarm(id) => {
                if Some(id) == self.timeout_alarm_id {
                    return TaskProgress::Done(STATUS_TIMEOUT);
                }
                TaskProgress::Yield
            }
            _ => TaskProgress::Yield,
        }
    }
}
```

### `scan.rs`

```rust
pub struct ScanTask {
    channel_mask: ChannelMask,
    scan_duration_ms: u32,
    results: Vec<ScanResult>,
    state: ScanState,
    // tracks internal protothread equivalent via enum
}

impl EventDrivenTask for ScanTask {
    fn start(&mut self, instance: &mut NcpInstanceBase) -> TaskProgress {
        // Send CMD_SCAN with params
        // Transition to state WaitingForResults
        // Each FrameReceived is either a scan result or scan done
    }

    fn process_event(&mut self, event: NcpEvent, instance: &mut NcpInstanceBase) -> TaskProgress {
        match event {
            NcpEvent::FrameReceived(frame) => {
                match self.state {
                    ScanState::WaitingForResults => {
                        match frame.command_id {
                            CMD_SCAN_RESULT => {
                                self.results.push(decode_scan_result(&frame.payload));
                                TaskProgress::Yield
                            }
                            CMD_SCAN_DONE => TaskProgress::Done(STATUS_OK),
                            _ => TaskProgress::Yield,
                        }
                    }
                }
            }
            NcpEvent::Alarm(id) => {
                if self.state == ScanState::WaitingForResults && alarm_timed_out(id) {
                    TaskProgress::Done(STATUS_TIMEOUT)
                } else {
                    TaskProgress::Yield
                }
            }
            _ => TaskProgress::Yield,
        }
    }

    fn finish(self: Box<Self>, status: i32, value: Option<Box<dyn Any>>) {
        // Invoke callback with self.results
    }
}
```

### `form.rs`

```rust
pub struct FormTask {
    network_name: String,
    pan_id: u16,
    channel: u8,
    state: FormState,
}

impl EventDrivenTask for FormTask {
    fn start(&mut self, instance: &mut NcpInstanceBase) -> TaskProgress {
        // 1. Queue PROP_VALUE_SET for network name
        // 2. Transition to SettingParams
        TaskProgress::Yield
    }

    fn process_event(&mut self, event: NcpEvent, instance: &mut NcpInstanceBase) -> TaskProgress {
        match self.state {
            FormState::SettingParams(idx) => {
                // After each PROP_VALUE_SET response, send the next
                // After all params set, send CMD_FORM
                // Transition to WaitingForAssociation
            }
            FormState::WaitingForAssociation => {
                // Check frame for state change → Associated
                // Or timeout/cancel
                if is_associated {
                    TaskProgress::Done(STATUS_OK)
                } else if is_fault {
                    TaskProgress::Done(STATUS_CHANNEL_ERROR)
                } else {
                    TaskProgress::Yield
                }
            }
        }
    }

    fn finish(self: Box<Self>, status: i32, value: Option<Box<dyn Any>>) {
        // Invoke callback
    }
}
```

### `data_pump.rs`

The single serial reader that feeds events to the dispatcher. This is where the HDLC decoder lives.

```rust
pub struct DataPump {
    transport: FramedTransport<UartTransport>,
    hdlc_decoder: HdlcDecoder,
    read_buf: [u8; 4096],
}

impl DataPump {
    pub async fn run(
        &mut self,
        event_sink: &tokio::sync::mpsc::UnboundedSender<NcpEvent>,
        cancel: CancellationToken,
    ) {
        loop {
            tokio::select! {
                result = self.transport.read(&mut self.read_buf) => {
                    let n = result?;
                    for &byte in &self.read_buf[..n] {
                        if let Some(result) = self.hdlc_decoder.feed_byte(byte) {
                            match result {
                                Ok(frame_data) => {
                                    match SpinelFrame::decode(&frame_data) {
                                        Ok(frame) => {
                                            event_sink.send(NcpEvent::FrameReceived(frame)).ok();
                                        }
                                        Err(e) => tracing::error!("Frame decode: {e}"),
                                    }
                                }
                                Err(e) => tracing::warn!("HDLC error: {e}"),
                            }
                        }
                    }
                }
                _ = cancel.cancelled() => break,
            }
        }
    }
}

#### Insecure-Stream Path (CREDENTIALS_NEEDED state)
When the NCP is in `CREDENTIALS_NEEDED` state, the data pump must route NCP→driver frames through
`SPINEL_PROP_STREAM_NET_INSECURE` instead of the normal secure path. This is a security-relevant
branch: application-layer frames are forwarded to the TUN interface before PTK establishment.

In the Rust data pump, model this as:

```rust
impl DataPump {
    pub async fn run(
        &mut self,
        event_sink: &UnboundedSender<NcpEvent>,
        instance_state: &Arc<RwLock<NcpState>>,
        cancel: CancellationToken,
    ) {
        loop {
            // ... read from serial, decode HDLC ...
            let ncp_state = instance_state.read().unwrap().clone();
            if ncp_state == NcpState::CredentialsNeeded {
                // Forward frame payload directly to TUN without task dispatch
                // (matches C: DataPump.cpp SPINEL_PROP_STREAM_NET_INSECURE path)
                self.forward_to_tun(&frame.payload);
            } else {
                event_sink.send(NcpEvent::FrameReceived(frame));
            }
        }
    }
}
```

This branch must be covered by integration tests with a mock NCP that triggers `CREDENTIALS_NEEDED`.

### Protothread → Enum State Machine Conversion

The C protothreads use `EH_BEGIN`/`EH_WAIT_UNTIL`/`EH_END` macros. Each one becomes a Rust enum state machine:

```rust
// C protothread from SpinelNCPInstance-Protothreads.cpp:
// EH_BEGIN_SUB(&mSubPT);
// while (!mEnabled) {
//     EH_WAIT_UNTIL_WITH_TIMEOUT(NCP_DEFAULT_COMMAND_RESPONSE_TIMEOUT, mEnabled || !is_busy());
//     ...
// }
// EH_END();

// Rust equivalent:
enum DisabledState {
    WaitingForEnabled,
    WaitingForNotBusy { deadline: Instant },
}

fn process_disabled(state: &mut DisabledState, event: &NcpEvent) -> TaskProgress {
    match state {
        DisabledState::WaitingForEnabled => {
            match event {
                NcpEvent::Starting => {
                    *state = DisabledState::WaitingForNotBusy {
                        deadline: Instant::now() + Duration::from_secs(5),
                    };
                    TaskProgress::Yield
                }
                _ => TaskProgress::Yield,
            }
        }
        DisabledState::WaitingForNotBusy { deadline } => {
            match event {
                NcpEvent::FrameReceived(_) => {
                    // Check if we should transition
                    TaskProgress::Yield
                }
                NcpEvent::Alarm(_) => {
                    // Timeout expired
                    TaskProgress::Done(STATUS_TIMEOUT)
                }
                _ => TaskProgress::Yield,
            }
        }
    }
}
```

## Tests

### Test 1: Send Command Response Matching

```rust
#[test]
fn send_command_matches_response_by_tid() {
    let mut task = SendCommandTask::new(CMD_PROP_VALUE_GET, encode_prop_key(PROP_NCP_VERSION));

    // Task started — should queue a frame
    let start = task.start(&mut mock_instance());
    assert_eq!(start, TaskProgress::Yield);

    // Receive a frame with matching TID and the expected property key
    let resp = SpinelFrame::new_with_tid(1, CMD_PROP_VALUE_IS, encode_u32(PROP_NCP_VERSION));
    let progress = task.process_event(NcpEvent::FrameReceived(resp), &mut mock_instance());
    assert_eq!(progress, TaskProgress::Done(STATUS_OK));
}
```

### Test 2: Send Command Ignores Wrong TID

```rust
#[test]
fn send_command_ignores_other_tid() {
    let mut task = SendCommandTask::new(CMD_PROP_VALUE_GET, encode_prop_key(PROP_NCP_VERSION));
    task.start(&mut mock_instance());

    // Wrong TID — should yield
    let resp = SpinelFrame::new_with_tid(2, CMD_PROP_VALUE_IS, vec![]);
    let progress = task.process_event(NcpEvent::FrameReceived(resp), &mut mock_instance());
    assert_eq!(progress, TaskProgress::Yield);
}
```

### Test 3: Scan Task Collects Results

```rust
#[test]
fn scan_task_collects_results() {
    let mut task = ScanTask::new(ChannelMask::all(), 3000);
    task.start(&mut mock_instance());

    // Feed scan results
    for i in 0..3 {
        let result = encode_scan_result(i, 0xABCD, format!("Net{i}"));
        let frame = SpinelFrame::new(CMD_SCAN_RESULT, result);
        task.process_event(NcpEvent::FrameReceived(frame), &mut mock_instance());
    }

    // Feed scan done
    let done = SpinelFrame::new(CMD_SCAN_DONE, vec![]);
    let progress = task.process_event(NcpEvent::FrameReceived(done), &mut mock_instance());
    assert_eq!(progress, TaskProgress::Done(STATUS_OK));
    assert_eq!(task.results.len(), 3);
}
```

### Test 4: Form Task Sets Params Then Forms

```rust
#[test]
fn form_task_state_machine() {
    let mut task = FormTask::new("TestNet".into(), 0xABCD, 1);
    let mut inst = mock_instance();

    // Start → should queue PROP_VALUE_SET for network name
    assert_eq!(task.start(&mut inst), TaskProgress::Yield);
    assert_eq!(inst.last_sent_command(), Some(CMD_PROP_VALUE_SET));

    // Respond to each param set...
    let resp = SpinelFrame::new_with_tid(task.tid(), CMD_LAST_STATUS, encode_u32(STATUS_OK));
    task.process_event(NcpEvent::FrameReceived(resp), &mut inst);
    // ... eventually sends CMD_FORM
    assert_eq!(inst.last_sent_command(), Some(CMD_FORM));
}
```

### Test 5: Task Timeout

```rust
#[test]
fn send_command_times_out() {
    let mut task = SendCommandTask::with_timeout(
        CMD_PROP_VALUE_GET,
        encode_prop_key(PROP_NCP_VERSION),
        Duration::from_millis(100),
    );
    task.start(&mut mock_instance());

    // Feed alarm event instead of frame
    let progress = task.process_event(NcpEvent::Alarm(AlarmId(1)), &mut mock_instance());
    assert_eq!(progress, TaskProgress::Done(STATUS_TIMEOUT));
}
```

### Test 6: Data Pump HDLC → Event

```rust
#[tokio::test]
async fn data_pump_produces_events() {
    let (mut transport, mock_peer) = create_mock_pair();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    // Mock peer sends an HDLC-framed Spinel frame
    let frame = SpinelFrame::new(CMD_PROP_VALUE_IS, vec![0x00, 0x01]);
    let mut encoder = HdlcEncoder::new();
    let hdlc_data = encoder.encode_frame(&frame);
    mock_peer.write(&hdlc_data).await.unwrap();

    let cancel = CancellationToken::new();
    tokio::spawn(async move {
        let mut pump = DataPump::new(transport);
        pump.run(&tx, cancel).await.ok();
    });

    let event = tokio::time::timeout(Duration::from_secs(1), rx.recv()).await.unwrap().unwrap();
    match event {
        NcpEvent::FrameReceived(f) => assert_eq!(f.command_id, CMD_PROP_VALUE_IS),
        _ => panic!("Expected FrameReceived"),
    }
}
```

### Test 7: Scan Done Completes Task (Not a Signal)

```rust
#[test]
fn scan_completes_via_cmd_scan_done() {
    // Verifies NetScanComplete is NOT a signal — it's the async reply
    let mut task = ScanTask::new(ChannelMask::all(), 3000);
    task.start(&mut mock_instance());

    let done = SpinelFrame::new(CMD_SCAN_DONE, vec![]);
    let progress = task.process_event(NcpEvent::FrameReceived(done), &mut mock_instance());
    assert_eq!(progress, TaskProgress::Done(STATUS_OK));
}
```

## Dependencies

```toml
[dependencies]
wisun-types = { path = "../../wisun-types" }
spinel = { path = "../../spinel" }
dcu-serial = { path = "../../dcu-serial" }
tokio = { version = "1", features = ["full", "test-util"] }
tokio-util = { version = "0.7", features = ["cancellation"] }
tracing = "0.1"
thiserror = "2"
```

## Verification Checklist

- [ ] Tasks DO NOT own the serial transport — they receive events from the dispatcher
- [ ] Each task implements `EventDrivenTask` trait (not `SpinelTask` with `&mut FramedTransport`)
- [ ] Each protothread from C is converted to an explicit enum state machine
- [ ] SendCommand matches responses by TID, ignores wrong TIDs
- [ ] Scan task collects multiple CMD_SCAN_RESULT frames before CMD_SCAN_DONE
- [ ] Form task sequences: set params → CMD_FORM → wait for associated state
- [ ] Timeout alarms work (Alarm event, not tokio::time::sleep)
- [ ] DataPump reads bytes, decodes HDLC, produces NcpEvent::FrameReceived
- [ ] Scan completion comes via CMD_SCAN_DONE frame, not a signal
- [ ] `cargo test` passes
- [ ] `cargo clippy` produces zero warnings
