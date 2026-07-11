//! Runaway reset backoff manager.
//!
//! Reimplements `RunawayResetBackoffManager.cpp`: windowed reset counter with
//! quadratic delay. No delay until the windowed count exceeds `K_BACKOFF_THRESHOLD`
//! (4), then `delay = (count - 4)² / 2` seconds. The count decays by one every
//! `K_DECAY_PERIOD` (15s).

use std::time::{Duration, Instant};

/// Rolling-window threshold for reset backoff.
const K_BACKOFF_THRESHOLD: u32 = 4;

/// Decay period in seconds — one decrement per period.
const K_DECAY_PERIOD: Duration = Duration::from_secs(15);

/// Tracks unexpected NCP resets and computes the backoff delay.
pub struct BackoffManager {
    windowed_reset_count: u32,
    decrement_at: Instant,
}

impl Default for BackoffManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BackoffManager {
    pub fn new() -> Self {
        Self {
            windowed_reset_count: 0,
            decrement_at: Instant::now() + K_DECAY_PERIOD,
        }
    }

    /// Seconds to delay before acting on an unexpected reset.
    /// Returns 0.0 until the windowed count exceeds the threshold,
    /// then `(count - 4)² / 2`.
    pub fn delay_for_unexpected_reset(&self) -> f64 {
        if self.windowed_reset_count > K_BACKOFF_THRESHOLD {
            let n = (self.windowed_reset_count - K_BACKOFF_THRESHOLD) as f64;
            (n * n) / 2.0
        } else {
            0.0
        }
    }

    /// Record an unexpected reset, opening/refreshing the decay window.
    pub fn count_unexpected_reset(&mut self) {
        self.windowed_reset_count += 1;
        self.decrement_at = Instant::now() + K_DECAY_PERIOD;
    }

    /// Advance the decay window. Call periodically from the event loop.
    pub fn update(&mut self) {
        if self.windowed_reset_count > 0 && Instant::now() >= self.decrement_at {
            self.windowed_reset_count = self.windowed_reset_count.saturating_sub(1);
            self.decrement_at = Instant::now() + K_DECAY_PERIOD;
        }
    }

    /// Clear all counts.
    pub fn reset(&mut self) {
        self.windowed_reset_count = 0;
        self.decrement_at = Instant::now() + K_DECAY_PERIOD;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runaway_reset_backoff_quadratic() {
        let mut mgr = BackoffManager::new();

        // Under threshold -> no delay.
        for _ in 0..4 {
            mgr.count_unexpected_reset();
        }
        assert_eq!(mgr.delay_for_unexpected_reset(), 0.0);

        // 5th reset: (5-4)²/2 = 0.5s
        mgr.count_unexpected_reset();
        assert!((mgr.delay_for_unexpected_reset() - 0.5).abs() < 1e-9);

        // 7th reset: (7-4)²/2 = 4.5s
        mgr.count_unexpected_reset();
        mgr.count_unexpected_reset();
        assert!((mgr.delay_for_unexpected_reset() - 4.5).abs() < 1e-9);

        // reset() clears everything.
        mgr.reset();
        assert_eq!(mgr.delay_for_unexpected_reset(), 0.0);
    }
}
