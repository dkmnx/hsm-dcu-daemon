//! SendCommand task — base class for sending a Spinel command and waiting for
//! a matching response by TID.
//!
//! Replaces `SpinelNCPTaskSendCommand.cpp`.

use spinel::command::CMD_PROP_VALUE_IS;
use spinel::frame::{make_header, SpinelFrame};
use spinel::pack::PackReader;
use spinel::property::PROP_LAST_STATUS;

use crate::dispatcher::{AlarmId, EventDrivenTask, NcpEvent, TaskProgress};
use crate::instance::base::NcpInstanceBase;
use crate::DaemonError;

/// Status code for successful completion (matches `SPINEL_STATUS_OK = 0`).
pub const STATUS_OK: i32 = 0;
/// Status code for timeout.
pub const STATUS_TIMEOUT: i32 = -1;

/// Sends a command and waits for a matching response by TID.
///
/// The task matches incoming frames by TID (transaction ID). Frames with
/// non-matching TIDs are ignored (yielded). When the matching response
/// arrives, the task completes with that response frame.
///
/// ## TID management
/// TIDs are allocated from a global atomic counter (not per-instance as in C).
/// TID 0 is reserved for "no response expected" and is skipped. When the
/// counter reaches 15 it wraps to 1, matching `SPINEL_GET_NEXT_TID` behavior
/// (C: `(x) >= 0xF ? 1 : (x) + 1`).
#[derive(Debug)]
pub struct SendCommandTask {
    /// The command ID to send (e.g. `CMD_PROP_VALUE_GET`).
    command_id: u32,
    /// Payload to append after the packed command ID.
    payload: Vec<u8>,
    /// TID assigned at creation time.
    tid: u8,
    /// Alarm ID for timeout tracking.
    timeout_alarm_id: Option<AlarmId>,
    /// Stashed response frame received from the NCP.
    response: Option<SpinelFrame>,
    /// Completion sender — resolves with the received response frame.
    completion: Option<tokio::sync::oneshot::Sender<Result<SpinelFrame, DaemonError>>>,
}

impl SendCommandTask {
    pub fn new(command_id: u32, payload: Vec<u8>) -> Self {
        static NEXT_TID: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
        let tid = NEXT_TID.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 15 + 1;

        Self {
            command_id,
            payload,
            tid,
            timeout_alarm_id: None,
            response: None,
            completion: None,
        }
    }

    /// Attach a oneshot sender so `finish` can deliver the received frame.
    pub fn with_completion(mut self, sender: tokio::sync::oneshot::Sender<Result<SpinelFrame, DaemonError>>) -> Self {
        self.completion = Some(sender);
        self
    }

    pub fn tid(&self) -> u8 {
        self.tid
    }
}

impl EventDrivenTask for SendCommandTask {
    fn name(&self) -> &str {
        "SendCommand"
    }

    fn start(&mut self, _instance: &NcpInstanceBase) -> TaskProgress {
        let header = make_header(0, self.tid);
        let frame = SpinelFrame::with_header(header, self.command_id, self.payload.clone());

        self.timeout_alarm_id = Some(AlarmId(self.tid as u64));

        tracing::debug!("SendCommandTask started: TID={}, CMD={}", self.tid, self.command_id);
        TaskProgress::YieldWithFrames(vec![frame])
    }

    fn process_event(
        &mut self,
        event: NcpEvent,
        _instance: &NcpInstanceBase,
    ) -> TaskProgress {
        match event {
            NcpEvent::FrameReceived(frame) => {
                if frame.tid() != self.tid {
                    return TaskProgress::Yield;
                }

                // All matching responses are delivered as PROP_VALUE_IS frames
                // (the data pump dispatches via handle_ncp_spinel_callback,
                // which fires EVENT_NCP_PROP_VALUE_IS for all responses).
                //
                // If the first packed integer in the payload is
                // PROP_LAST_STATUS (0), the remaining bytes carry the error
                // code. Otherwise it's a successful property value response.
                if frame.command_id == CMD_PROP_VALUE_IS {
                    let mut reader = PackReader::new(&frame.payload);
                    if let Ok(key) = reader.read_uint_packed() {
                        if key == PROP_LAST_STATUS {
                            if let Ok(status) = reader.read_uint_packed() {
                                return TaskProgress::Done(status as i32);
                            }
                            return TaskProgress::Done(-2);
                        }
                    }
                }

        self.response = Some(frame);
        TaskProgress::Done(STATUS_OK)
            }
            NcpEvent::Alarm(id) => {
                if Some(id) == self.timeout_alarm_id {
                    tracing::warn!("SendCommandTask timed out: TID={}", self.tid);
                    TaskProgress::Done(STATUS_TIMEOUT)
                } else {
                    TaskProgress::Yield
                }
            }
            NcpEvent::Starting => TaskProgress::Yield,
        }
    }

    fn finish(self: Box<Self>, status: i32, _value: Option<Box<dyn std::any::Any + Send>>) {
        let Self { tid, command_id, payload, response, completion, .. } = *self;
        tracing::info!("SendCommandTask finished: TID={tid}, status={status}");
        if let Some(sender) = completion {
            if status == STATUS_OK {
                let frame = response.unwrap_or_else(|| SpinelFrame::new(command_id, payload));
                let _ = sender.send(Ok(frame));
            } else {
                let _ = sender.send(Err(DaemonError::Ncp(format!(
                    "command {command_id} failed with status {status}"
                ))));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spinel::command::CMD_PROP_VALUE_IS;

    /// Minimal mock instance for unit testing tasks.
    fn mock_instance() -> NcpInstanceBase {
        NcpInstanceBase::mock_for_testing()
    }

    #[test]
    fn send_command_matches_response_by_tid() {
        let mut task = SendCommandTask::new(
            spinel::command::CMD_PROP_VALUE_GET,
            vec![0x00, 0x01],
        );

        let inst = mock_instance();
        let progress = task.start(&inst);
        assert!(matches!(progress, TaskProgress::YieldWithFrames(_)));

        let tid = task.tid();
        let resp = SpinelFrame::with_header(make_header(0, tid), CMD_PROP_VALUE_IS, vec![]);
        let progress = task.process_event(NcpEvent::FrameReceived(resp), &inst);
        assert!(matches!(progress, TaskProgress::Done(STATUS_OK)));
    }

    #[test]
    fn send_command_ignores_wrong_tid() {
        let mut task = SendCommandTask::new(
            spinel::command::CMD_PROP_VALUE_GET,
            vec![0x00, 0x01],
        );

        let inst = mock_instance();
        task.start(&inst);

        let resp = SpinelFrame::with_header(make_header(0, 15), CMD_PROP_VALUE_IS, vec![]);
        let progress = task.process_event(NcpEvent::FrameReceived(resp), &inst);
        assert!(matches!(progress, TaskProgress::Yield));
    }

    #[test]
    fn send_command_times_out() {
        let mut task = SendCommandTask::new(
            spinel::command::CMD_PROP_VALUE_GET,
            vec![0x00, 0x01],
        );

        let inst = mock_instance();
        task.start(&inst);

        let alarm_id = AlarmId(task.tid() as u64);
        let progress = task.process_event(NcpEvent::Alarm(alarm_id), &inst);
        assert!(matches!(progress, TaskProgress::Done(STATUS_TIMEOUT)));
    }
}
