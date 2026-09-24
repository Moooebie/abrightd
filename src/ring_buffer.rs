//! Port of AOSP `AmbientLightRingBuffer` and the weighted ambient-lux integral.
//!
//! Original: nested class in
//! `services/core/java/com/android/server/display/AutomaticBrightnessController.java`
//! Copyright (C) 2016 The Android Open Source Project — Apache-2.0.
//!
//! AOSP's ring buffer grows by doubling; a `VecDeque` is behaviourally
//! equivalent and simpler.  The timestamp-rewriting behaviour of [`prune`] is
//! reproduced exactly, because it is what makes report-on-change-only sensors
//! usable.

use std::collections::VecDeque;

/// How long the current sensor reading is assumed to be valid beyond `now`.
/// Provides a little prediction and guarantees the last sample has non-zero
/// weight.
pub const AMBIENT_LIGHT_PREDICTION_TIME_MILLIS: i64 = 100;

#[derive(Debug, Clone, Default)]
pub struct AmbientLightRingBuffer {
    /// (timestamp_ms, lux), oldest first.
    samples: VecDeque<(i64, f32)>,
}

impl AmbientLightRingBuffer {
    pub fn new() -> Self {
        Self {
            samples: VecDeque::new(),
        }
    }

    pub fn size(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    pub fn get_lux(&self, index: usize) -> f32 {
        self.samples[index].1
    }

    pub fn get_time(&self, index: usize) -> i64 {
        self.samples[index].0
    }

    pub fn push(&mut self, time_ms: i64, lux: f32) {
        self.samples.push_back((time_ms, lux));
    }

    /// Drop everything older than `horizon` (an absolute timestamp).
    ///
    /// The youngest element that would be removed is retained and its timestamp
    /// is rewritten to `horizon`: for change-only sensors that reading is still
    /// the current ambient level, and removing it would drop a valid data point.
    pub fn prune(&mut self, horizon: i64) {
        while self.samples.len() > 1 {
            if self.samples[1].0 > horizon {
                break;
            }
            self.samples.pop_front();
        }
        if let Some(front) = self.samples.front_mut() {
            if front.0 < horizon {
                front.0 = horizon;
            }
        }
    }

    pub fn lux_values(&self) -> Vec<f32> {
        self.samples.iter().map(|(_, lux)| *lux).collect()
    }

    pub fn timestamps(&self) -> Vec<i64> {
        self.samples.iter().map(|(t, _)| *t).collect()
    }

    /// Weighted average lux over `[now - horizon, now]`, treating the "now"
    /// edge as `now + AMBIENT_LIGHT_PREDICTION_TIME_MILLIS`.
    ///
    /// Returns `None` when there are no samples (AOSP returns `-1`).
    pub fn calculate_ambient_lux(&self, now_ms: i64, horizon_ms: i64) -> Option<f32> {
        let n = self.samples.len();
        if n == 0 {
            return None;
        }
        let weighting_intercept = horizon_ms;

        // Find the first measurement that is just outside of the horizon.
        let horizon_start_time = now_ms - horizon_ms;
        let mut end_index = 0usize;
        for i in 0..n - 1 {
            if self.samples[i + 1].0 <= horizon_start_time {
                end_index += 1;
            } else {
                break;
            }
        }

        let mut sum = 0.0f32;
        let mut total_weight = 0.0f32;
        let mut end_time = AMBIENT_LIGHT_PREDICTION_TIME_MILLIS;
        for i in (end_index..n).rev() {
            let mut event_time = self.samples[i].0;
            if i == end_index && event_time < horizon_start_time {
                // Only consider the part of the sample within our horizon.
                event_time = horizon_start_time;
            }
            let start_time = event_time - now_ms;
            let weight = weight_integral(end_time, weighting_intercept)
                - weight_integral(start_time, weighting_intercept);
            total_weight += weight;
            sum += self.samples[i].1 * weight;
            end_time = start_time;
        }
        if total_weight == 0.0 {
            return None;
        }
        Some(sum / total_weight)
    }
}

/// Integral of `y = x + intercept`, i.e. `x * (x * 0.5 + intercept)`.
///
/// This is always positive over the horizon and provides non-linear weighting.
pub fn weight_integral(x: i64, intercept: i64) -> f32 {
    let xf = x as f32;
    xf * (xf * 0.5 + intercept as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_returns_none() {
        let b = AmbientLightRingBuffer::new();
        assert_eq!(b.calculate_ambient_lux(1000, 10000), None);
    }

    #[test]
    fn constant_lux_is_constant() {
        let mut b = AmbientLightRingBuffer::new();
        for t in 0..20 {
            b.push(t * 200, 42.0);
        }
        let v = b.calculate_ambient_lux(4000, 10_000).unwrap();
        assert!((v - 42.0).abs() < 1e-3, "got {v}");
    }

    #[test]
    fn recent_samples_weigh_more() {
        let mut b = AmbientLightRingBuffer::new();
        // Old dark samples, then a recent bright sample.
        for t in 0..10 {
            b.push(t * 100, 0.0);
        }
        b.push(1000, 100.0);
        let v = b.calculate_ambient_lux(1000, 10_000).unwrap();
        assert!(v > 0.0 && v < 100.0, "weighted value {v}");
        let short = b.calculate_ambient_lux(1000, 2000).unwrap();
        assert!(short > v, "short horizon {short} should be closer to 100");
    }

    #[test]
    fn prune_keeps_youngest_out_of_horizon_sample() {
        let mut b = AmbientLightRingBuffer::new();
        for t in 0..10 {
            b.push(t * 100, t as f32);
        }
        // Samples strictly before 350 are dropped, but the youngest sample older
        // than the horizon (300) is retained and rewritten to 350.
        b.prune(350);
        assert_eq!(b.get_time(0), 350);
        assert_eq!(b.size(), 7); // 300(->350),400,..,900
    }

    #[test]
    fn boundary_clamps_to_horizon_start() {
        let mut b = AmbientLightRingBuffer::new();
        b.push(0, 10.0);
        b.push(5000, 20.0);
        // horizon start = 3000, so sample 0 is clamped to 3000.
        let v = b.calculate_ambient_lux(5000, 2000).unwrap();
        assert!((10.0..=20.0).contains(&v));
    }
}
