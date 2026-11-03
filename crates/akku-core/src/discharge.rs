/// A battery sample used for discharge-rate estimation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DischargeSample {
    /// Monotonic seconds, supplied by the caller so tests are deterministic.
    pub seconds: f64,
    pub percent: f32,
}

/// Estimates battery drain from recent, non-charging samples.
#[derive(Clone, Debug, Default)]
pub struct DischargeTracker {
    samples: Vec<DischargeSample>,
    max_gap_seconds: f64,
    window_seconds: f64,
}

impl DischargeTracker {
    pub fn new() -> Self {
        Self {
            samples: Vec::new(),
            max_gap_seconds: 15.0 * 60.0,
            window_seconds: 30.0 * 60.0,
        }
    }

    pub fn with_limits(max_gap_seconds: f64, window_seconds: f64) -> Self {
        Self {
            samples: Vec::new(),
            max_gap_seconds: max_gap_seconds.max(1.0),
            window_seconds: window_seconds.max(1.0),
        }
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    /// Add a sample. Returns true when the sample was accepted into the
    /// ongoing series. Returns false when the sample was rejected (bad value,
    /// non-monotonic time) or when it follows a sleep/suspend gap — such a
    /// sample is not a usable continuation, but it becomes the baseline of a
    /// fresh series.
    pub fn observe(&mut self, percent: u8, seconds: f64) -> bool {
        if !seconds.is_finite() || percent > 100 {
            return false;
        }
        if let Some(last) = self.samples.last() {
            if seconds <= last.seconds {
                return false;
            }
            if seconds - last.seconds > self.max_gap_seconds {
                // A long gap means the device likely slept or disconnected:
                // old samples say nothing about the new baseline. Start over.
                self.samples.clear();
                self.samples.push(DischargeSample {
                    seconds,
                    percent: f32::from(percent),
                });
                return false;
            }
        }
        self.samples.push(DischargeSample {
            seconds,
            percent: f32::from(percent),
        });
        let cutoff = seconds - self.window_seconds;
        self.samples.retain(|sample| sample.seconds >= cutoff);
        true
    }

    /// Positive values mean percent per hour. `None` until there are two usable
    /// samples or when the trend is flat/charging.
    pub fn drain_percent_per_hour(&self) -> Option<f64> {
        if self.samples.len() < 2 {
            return None;
        }
        let n = self.samples.len() as f64;
        let mean_x = self.samples.iter().map(|s| s.seconds).sum::<f64>() / n;
        let mean_y = self
            .samples
            .iter()
            .map(|s| f64::from(s.percent))
            .sum::<f64>()
            / n;
        let mut numerator = 0.0;
        let mut denominator = 0.0;
        for sample in &self.samples {
            let dx = sample.seconds - mean_x;
            numerator += dx * (f64::from(sample.percent) - mean_y);
            denominator += dx * dx;
        }
        if denominator <= f64::EPSILON {
            return None;
        }
        let slope = numerator / denominator; // percent per second
        let drain = -slope * 3600.0;
        (drain > 0.0).then_some(drain)
    }

    pub fn time_to_empty(&self) -> Option<std::time::Duration> {
        let rate = self.drain_percent_per_hour()?;
        let last = self.samples.last()?;
        let hours = f64::from(last.percent) / rate;
        if !hours.is_finite() || hours <= 0.0 {
            return None;
        }
        Some(std::time::Duration::from_secs_f64(hours * 3600.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_drain_rate() {
        // The synthetic samples are an hour apart; widen the gap and window
        // limits (the production defaults assume 30–90 s polling).
        let mut tracker = DischargeTracker::with_limits(4.0 * 3600.0, 4.0 * 3600.0);
        assert!(tracker.observe(100, 0.0));
        assert!(tracker.observe(99, 3600.0));
        let rate = tracker.drain_percent_per_hour().unwrap();
        assert!((rate - 1.0).abs() < 0.001);
        let eta = tracker.time_to_empty().unwrap();
        assert!((eta.as_secs_f64() - 99.0 * 3600.0).abs() < 1.0);
    }

    #[test]
    fn rejects_sleep_gap() {
        let mut tracker = DischargeTracker::new();
        assert!(tracker.observe(100, 0.0));
        // 20 minutes without a sample: the series was interrupted, so the
        // next observation may not be fit against the old samples.
        assert!(!tracker.observe(90, 20.0 * 60.0));
        assert_eq!(tracker.drain_percent_per_hour(), None);
    }

    #[test]
    fn rejects_non_monotonic_samples() {
        let mut tracker = DischargeTracker::new();
        assert!(tracker.observe(100, 10.0));
        assert!(!tracker.observe(99, 9.0));
    }

    #[test]
    fn ignores_charging_trend() {
        // Widen the limits so the rising trend is actually fitted rather
        // than discarded by the sleep-gap reset.
        let mut tracker = DischargeTracker::with_limits(4.0 * 3600.0, 4.0 * 3600.0);
        tracker.observe(50, 0.0);
        tracker.observe(60, 3600.0);
        assert_eq!(tracker.drain_percent_per_hour(), None);
    }
}
