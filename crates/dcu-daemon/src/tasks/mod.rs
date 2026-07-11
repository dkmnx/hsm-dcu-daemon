//! Task trait and mock task for daemon task management.

pub mod backoff;
pub mod queue;

use crate::DaemonError;

/// A runnable unit of work on the NCP. Replaces `SpinelNCPTask` protothreads.
///
/// Each task owns its state machine and is driven by the daemon's event loop
/// via `run`.
#[async_trait::async_trait]
pub trait SpinelTask: Send + 'static {
    /// Execute the task. Returns `Ok(())` on success or a `DaemonError` on
    /// failure/cancellation.
    async fn run(&mut self) -> Result<(), DaemonError>;

    /// Human-readable name for logging.
    fn name(&self) -> &str;
}

/// A mock task for unit testing the task queue.
#[cfg(test)]
pub struct MockTask {
    name: String,
    should_fail: bool,
}

#[cfg(test)]
impl MockTask {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            should_fail: false,
        }
    }

    pub fn failing(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            should_fail: true,
        }
    }
}

#[cfg(test)]
#[async_trait::async_trait]
impl SpinelTask for MockTask {
    async fn run(&mut self) -> Result<(), DaemonError> {
        if self.should_fail {
            Err(DaemonError::Ncp("mock failure".into()))
        } else {
            Ok(())
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}


