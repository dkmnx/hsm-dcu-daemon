//! FIFO task queue for NCP operations.

use crate::tasks::SpinelTask;

/// A FIFO queue of Spinel tasks.
pub struct TaskQueue {
    pending: Vec<Box<dyn SpinelTask>>,
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
    pub fn push(&mut self, task: Box<dyn SpinelTask>) {
        self.pending.push(task);
    }

    /// Pop the next task from the front. Returns `None` if the queue is empty.
    pub fn pop(&mut self) -> Option<Box<dyn SpinelTask>> {
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
    use crate::tasks::MockTask;

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
