//! Core NCP state machine with event-driven task dispatch.
//!
//! Reimplements `src/dcud/NCPInstanceBase.cpp` and `SpinelNCPInstance.cpp`.

use std::collections::VecDeque;
use std::sync::Arc;

use tokio::sync::{mpsc, Notify, RwLock};
use tokio_util::sync::CancellationToken;
use wisun_types::NcpState;

use crate::config::Config;
use crate::dispatcher::{EventDrivenTask, NcpEvent, TaskProgress};
use crate::DaemonError;

/// The base NCP instance — state machine, event loop, task queue.
pub struct NcpInstanceBase {
    ncp_state: Arc<RwLock<NcpState>>,
    interface_name: String,
    state_changed: Arc<Notify>,

    command_rx: mpsc::Receiver<dcu_dbus::commands::Command>,
    event_rx: mpsc::UnboundedReceiver<NcpEvent>,
    event_pump_tx: mpsc::UnboundedSender<NcpEvent>,

    active_task: Option<Box<dyn EventDrivenTask>>,
    task_queue: VecDeque<Box<dyn EventDrivenTask>>,
    outbound_queue: VecDeque<spinel::frame::SpinelFrame>,

    #[allow(dead_code)]
    shared_state: Arc<RwLock<dcu_dbus::DaemonState>>,

    #[allow(dead_code)]
    config: Config,
}

impl NcpInstanceBase {
    pub async fn new(
        config: Config,
        shared_state: Arc<RwLock<dcu_dbus::DaemonState>>,
        command_rx: mpsc::Receiver<dcu_dbus::commands::Command>,
    ) -> Result<Self, DaemonError> {
        let (event_pump_tx, event_rx) = mpsc::unbounded_channel();
        Ok(Self {
            ncp_state: Arc::new(RwLock::new(NcpState::Uninitialized)),
            interface_name: config.tun_interface_name.clone(),
            state_changed: Arc::new(Notify::new()),
            command_rx,
            event_rx,
            event_pump_tx,
            active_task: None,
            task_queue: VecDeque::new(),
            outbound_queue: VecDeque::new(),
            shared_state,
            config,
        })
    }

    pub fn interface_name(&self) -> &str {
        &self.interface_name
    }

    /// Run the main event loop.
    pub async fn run(&mut self, cancel: CancellationToken) {
        tracing::info!("Starting NCP instance event loop");
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                cmd = self.command_rx.recv() => {
                    match cmd {
                        Some(cmd) => { let _ = self.handle_command(cmd).await; }
                        None => break,
                    }
                }
                event = self.event_rx.recv() => {
                    match event {
                        Some(event) => self.dispatch_event(event).await,
                        None => break,
                    }
                }
                _ = self.state_changed.notified() => {}
            }
        }
    }

    /// Take the active task, dispatch an event, put it back or finish it.
    async fn dispatch_event(&mut self, event: NcpEvent) {
        let task = self.active_task.take();

        let (progress, task) = match task {
            Some(mut t) => {
                let progress = t.process_event(event, self);
                (progress, Some(t))
            }
            None => return,
        };

        match progress {
            TaskProgress::Yield => {
                self.active_task = task;
            }
            TaskProgress::YieldWithFrames(frames) => {
                self.outbound_queue.extend(frames);
                self.active_task = task;
            }
            TaskProgress::Done(status) => {
                task.unwrap().finish(status, None);
                self.schedule_next_task();
            }
            TaskProgress::AwaitingChild(child, frames) => {
                self.outbound_queue.extend(frames);
                let current = task.unwrap();
                self.task_queue.push_front(current);
                self.task_queue.push_front(child);
                self.schedule_next_task();
            }
        }
    }

    fn schedule_next_task(&mut self) {
        while let Some(mut task) = self.task_queue.pop_front() {
            let progress = task.start(self);
            match progress {
                TaskProgress::Yield => {
                    self.active_task = Some(task);
                    return;
                }
                TaskProgress::YieldWithFrames(frames) => {
                    self.outbound_queue.extend(frames);
                    self.active_task = Some(task);
                    return;
                }
                TaskProgress::Done(status) => {
                    task.finish(status, None);
                }
                TaskProgress::AwaitingChild(child, frames) => {
                    self.outbound_queue.extend(frames);
                    self.task_queue.push_front(task);
                    self.task_queue.push_front(child);
                }
            }
        }
    }

    /// Called by tasks during `start()` or `process_event()`.
    pub fn enqueue_outbound_frame(&mut self, frame: spinel::frame::SpinelFrame) {
        self.outbound_queue.push_back(frame);
    }

    /// Drain queued outbound frames for the pump.
    pub fn drain_outbound(&mut self) -> Vec<spinel::frame::SpinelFrame> {
        self.outbound_queue.drain(..).collect()
    }

    /// Borrow the pump-side event sender.
    pub fn event_pump_tx(&self) -> mpsc::UnboundedSender<NcpEvent> {
        self.event_pump_tx.clone()
    }

    /// Enqueue a task for execution.
    pub fn spawn_task(&mut self, task: Box<dyn EventDrivenTask>) {
        self.task_queue.push_back(task);
    }

    pub async fn start_pumps(&mut self) -> Result<(), DaemonError> {
        tracing::info!("Starting I/O pumps (stub)");
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<(), DaemonError> {
        tracing::info!("Stopping NCP instance");
        Ok(())
    }

    pub async fn set_ncp_state(&self, state: NcpState) {
        *self.ncp_state.write().await = state;
        self.state_changed.notify_waiters();
    }

    pub async fn get_ncp_state(&self) -> NcpState {
        *self.ncp_state.read().await
    }

    #[cfg(test)]
    pub fn mock_for_testing() -> Self {
        let (event_pump_tx, event_rx) = mpsc::unbounded_channel();
        Self {
            ncp_state: Arc::new(RwLock::new(NcpState::Uninitialized)),
            interface_name: "wfan0".into(),
            state_changed: Arc::new(Notify::new()),
            command_rx: mpsc::channel(16).1,
            event_rx,
            event_pump_tx,
            active_task: None,
            task_queue: VecDeque::new(),
            outbound_queue: VecDeque::new(),
            shared_state: Arc::new(RwLock::new(dcu_dbus::DaemonState::default())),
            config: Config::default(),
        }
    }

    pub async fn handle_command(
        &mut self,
        cmd: dcu_dbus::commands::Command,
    ) -> Result<String, DaemonError> {
        match cmd {
            dcu_dbus::commands::Command::Reset => {
                Ok(format!("NCP:State: {}", self.get_ncp_state().await))
            }
            dcu_dbus::commands::Command::Leave => {
                self.set_ncp_state(NcpState::Offline).await;
                Ok("Left network".into())
            }
            other => {
                tracing::warn!("Unhandled command: {other:?}");
                Ok("unhandled".into())
            }
        }
    }
}
