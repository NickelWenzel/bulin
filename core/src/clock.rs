//! The stopwatch behind the built-in `time` uniform.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Elapsed time that advances while running. The caller measures time and passes the deltas in,
/// so the clock works the same on every target.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq)]
pub struct Clock {
    elapsed: Duration,
    running: bool,
}

impl Clock {
    /// A clock at zero that is already running.
    pub fn started() -> Self {
        Self {
            elapsed: Duration::ZERO,
            running: true,
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn toggle(&mut self) {
        self.running = !self.running;
    }

    pub fn reset(&mut self) {
        self.elapsed = Duration::ZERO;
    }

    /// Adds `delta` while running.
    pub fn advance(&mut self, delta: Duration) {
        if self.running {
            self.elapsed += delta;
        }
    }

    pub fn seconds(&self) -> f32 {
        self.elapsed.as_secs_f32()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_paused_clock_ignores_time_passing() {
        let mut clock = Clock::started();
        clock.advance(Duration::from_millis(1500));
        clock.toggle();
        clock.advance(Duration::from_secs(10));

        assert_eq!(clock.seconds(), 1.5);
        assert!(!clock.is_running());
    }

    #[test]
    fn reset_keeps_a_running_clock_running() {
        let mut clock = Clock::started();
        clock.advance(Duration::from_secs(3));
        clock.reset();
        clock.advance(Duration::from_secs(1));

        assert_eq!(clock.seconds(), 1.0);
    }
}
