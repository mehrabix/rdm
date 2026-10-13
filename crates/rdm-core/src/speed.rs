//! Throughput smoothing and ETA estimation.

use std::time::Duration;

/// Exponentially weighted moving average of throughput.
///
/// A raw bytes-per-tick reading is far too jittery to show a user, so we smooth
/// it: `avg = α·instant + (1 − α)·avg`. A higher `alpha` reacts faster.
#[derive(Debug, Clone)]
pub struct SpeedMeter {
    avg: f64,
    alpha: f64,
}

impl Default for SpeedMeter {
    fn default() -> Self {
        Self::new(0.3)
    }
}

impl SpeedMeter {
    pub fn new(alpha: f64) -> Self {
        Self {
            avg: 0.0,
            alpha: alpha.clamp(0.01, 1.0),
        }
    }

    /// Feed a delta of `bytes_delta` bytes that took `elapsed` to arrive.
    /// Returns the new smoothed speed in bytes/second.
    pub fn observe(&mut self, bytes_delta: u64, elapsed: Duration) -> f64 {
        let secs = elapsed.as_secs_f64();
        if secs <= 0.0 {
            return self.avg;
        }
        let instant = bytes_delta as f64 / secs;
        self.avg = if self.avg == 0.0 {
            instant
        } else {
            self.alpha * instant + (1.0 - self.alpha) * self.avg
        };
        self.avg
    }

    pub fn speed(&self) -> f64 {
        self.avg
    }

    pub fn reset(&mut self) {
        self.avg = 0.0;
    }
}

/// Seconds to finish `remaining` bytes at `speed`, or `None` when the speed is
/// too low to estimate.
pub fn eta_secs(remaining: u64, speed: f64) -> Option<u64> {
    if speed < 1.0 {
        None
    } else {
        Some((remaining as f64 / speed).ceil() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_sample_is_the_instant_rate() {
        let mut m = SpeedMeter::new(0.5);
        let s = m.observe(1024, Duration::from_secs(1));
        assert!((s - 1024.0).abs() < f64::EPSILON);
    }

    #[test]
    fn smooths_toward_new_rate() {
        let mut m = SpeedMeter::new(0.5);
        m.observe(1000, Duration::from_secs(1)); // 1000 B/s
        let s = m.observe(3000, Duration::from_secs(1)); // instant 3000 B/s
        assert!((s - 2000.0).abs() < 1e-9, "got {s}");
    }

    #[test]
    fn zero_elapsed_does_not_explode() {
        let mut m = SpeedMeter::default();
        m.observe(500, Duration::from_secs(1));
        let s = m.observe(999, Duration::ZERO);
        assert!((s - 500.0).abs() < 1e-9);
    }

    #[test]
    fn eta_math() {
        assert_eq!(eta_secs(2048, 1024.0), Some(2));
        assert_eq!(eta_secs(100, 1024.0), Some(1));
        assert_eq!(eta_secs(100, 0.0), None);
    }
}
