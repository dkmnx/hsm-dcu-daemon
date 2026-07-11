//! FIFO task queue for NCP operations.
//!
//! Wraps [`EventDrivenTask`] in a FIFO buffer. This is a convenience struct
//! layered on top of the instance's own `VecDeque<dyn EventDrivenTask>` — the
//! instance uses its internal queue directly; this module exists for testing
//! and for external code that needs to batch tasks before spawning them.

use crate::dispatcher::EventDrivenTask;

/// A FIFO queue of tasks (wrapping [`EventDrivenTask`]).
pub struct TaskQueue {
    pending: Vec<Box<dyn EventDrivenTask>>,
}

impl Default for TaskQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskQueue {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
        }
    }

    /// Push a task onto the back of the queue.
    pub fn push(&mut self, task: Box<dyn EventDrivenTask>) {
        self.pending.push(task);
    }

    /// Pop the next task from the front.
    pub fn pop(&mut self) -> Option<Box<dyn EventDrivenTask>> {
        if self.pending.is_empty() {
            None
        } else {
            Some(self.pending.remove(0))
        }
    }

    /// Cancel all pending tasks.
    pub fn cancel_all(&mut self) {
        self.pending.clear();
    }

    /// Returns `true` if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Returns the number of pending tasks.
    pub fn len(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatcher::{EventDrivenTask, NcpEvent, TaskProgress};
    use crate::instance::base::NcpInstanceBase;

    /// A minimal mock task for testing the queue.
    #[derive(Debug)]
    struct MockTask {
        name: String,
        should_fail: bool,
    }

    impl MockTask {
        fn new(name: impl Into<String>) -> Self {
            Self { name: name.into(), should_fail: false }
        }
    }

    impl EventDrivenTask for MockTask {
        fn name(&self) -> &str { &self.name }
        fn start(&mut self, _inst: &NcpInstanceBase) -> TaskProgress {
            if self.should_fail {
                TaskProgress::Done(1)
            } else {
                TaskProgress::Done(0)
            }
        }
        fn process_event(&mut self, _evt: NcpEvent, _inst: &NcpInstanceBase) -> TaskProgress {
            TaskProgress::Done(0)
        }
        fn finish(self: Box<Self>, _status: i32, _value: Option<Box<dyn std::any::Any + Send>>) {}
    }

    #[test]
    fn task_queue_fifo() {
        let mut queue = TaskQueue::new();
        queue.push(Box::new(MockTask::new("first")));
        queue.push(Box::new(MockTask::new("second")));

        assert_eq!(queue.pop().unwrap().name(), "first");
        assert_eq!(queue.pop().unwrap().name(), "second");
        assert!(queue.is_empty());
    }

    #[test]
    fn task_queue_cancel_all() {
        let mut queue = TaskQueue::new();
        queue.push(Box::new(MockTask::new("a")));
        queue.push(Box::new(MockTask::new("b")));
        queue.cancel_all();
        assert!(queue.is_empty());
    }
}
