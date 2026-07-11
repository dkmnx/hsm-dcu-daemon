pub mod data_pump;
pub mod events;

/// Identifier for a timer alarm set by a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AlarmId(pub u64);

/// Events that the data pump delivers to the active task.
#[derive(Debug)]
pub enum NcpEvent {
    FrameReceived(spinel::frame::SpinelFrame),
    Alarm(AlarmId),
    Starting,
}

/// Progress returned by [`EventDrivenTask`] methods.
#[derive(Debug)]
pub enum TaskProgress {
    /// Task is still running; await the next event.
    Yield,
    /// Task yielded with frames to send to the NCP.
    YieldWithFrames(Vec<spinel::frame::SpinelFrame>),
    /// Task completed with the given status.
    Done(i32),
    /// Task spawned a child and is waiting for it.
    AwaitingChild(Box<dyn EventDrivenTask>, Vec<spinel::frame::SpinelFrame>),
}

/// An event-driven task that does NOT own the serial transport.
///
/// Replaces `SpinelNCPTask::vprocess_event(int event, va_list args)`.
/// The task returns [`TaskProgress`] which may include outbound frames.
/// The instance dispatches frames to the I/O pump.
pub trait EventDrivenTask: std::fmt::Debug + Send {
    fn name(&self) -> &str;

    fn start(&mut self, instance: &crate::instance::base::NcpInstanceBase) -> TaskProgress;

    fn process_event(
        &mut self,
        event: NcpEvent,
        instance: &crate::instance::base::NcpInstanceBase,
    ) -> TaskProgress;

    fn finish(self: Box<Self>, status: i32, value: Option<Box<dyn std::any::Any + Send>>);
}
