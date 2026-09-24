//! Output rate limiting.
//!
//! Android's brightness ramp lives in `DisplayPowerController` and is not part
//! of the algorithm being ported, so this is a deliberate addition.  It moves
//! the commanded brightness toward the controller's target with separate
//! brighten/darken per-step caps and a minimum interval between steps.

/// Per-step limits.
#[derive(Debug, Clone)]
pub struct RampConfig {
    /// Maximum normalized-brightness change for one brightening step.
    pub brighten_step: f32,
    /// Maximum normalized-brightness change for one darkening step.
    pub darken_step: f32,
    /// Minimum time between steps.
    pub min_interval_ms: i64,
}

impl Default for RampConfig {
    fn default() -> Self {
        Self {
            brighten_step: 0.05,
            darken_step: 0.05,
            min_interval_ms: 100,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Ramp {
    config: RampConfig,
    current: f32,
    initialized: bool,
    last_update_ms: i64,
}

impl Ramp {
    pub fn new(config: RampConfig) -> Self {
        Self {
            config,
            current: 0.0,
            initialized: false,
            last_update_ms: 0,
        }
    }

    pub fn current(&self) -> f32 {
        self.current
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Snap to a value, ignoring rate limits (e.g. on startup).
    pub fn reset(&mut self, value: f32) {
        self.current = value.clamp(0.0, 1.0);
        self.initialized = true;
        // Allow the next update to proceed regardless of its timestamp.
        self.last_update_ms = i64::MIN / 2;
    }

    /// Advance toward `target`, returning the new commanded value.
    pub fn update(&mut self, now_ms: i64, target: f32) -> f32 {
        let target = target.clamp(0.0, 1.0);
        if !self.initialized {
            self.reset(target);
            self.last_update_ms = now_ms;
            return self.current;
        }
        if now_ms - self.last_update_ms < self.config.min_interval_ms {
            return self.current;
        }
        self.last_update_ms = now_ms;

        let delta = target - self.current;
        let limit = if delta >= 0.0 {
            self.config.brighten_step
        } else {
            self.config.darken_step
        };
        if delta.abs() <= limit {
            self.current = target;
        } else {
            self.current += limit.copysign(delta);
        }
        self.current = self.current.clamp(0.0, 1.0);
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_update_snaps() {
        let mut r = Ramp::new(RampConfig::default());
        assert_eq!(r.update(0, 0.7), 0.7);
    }

    #[test]
    fn rate_limits_steps() {
        let mut r = Ramp::new(RampConfig {
            brighten_step: 0.1,
            darken_step: 0.1,
            min_interval_ms: 100,
        });
        r.reset(0.0);
        r.update(0, 0.5);
        assert!((r.current() - 0.1).abs() < 1e-6);
        // Too soon: no movement.
        r.update(50, 0.5);
        assert!((r.current() - 0.1).abs() < 1e-6);
        // After the interval: one more step.
        r.update(100, 0.5);
        assert!((r.current() - 0.2).abs() < 1e-6);
    }

    #[test]
    fn reaches_target_exactly() {
        let mut r = Ramp::new(RampConfig {
            brighten_step: 1.0,
            darken_step: 1.0,
            min_interval_ms: 0,
        });
        r.reset(0.0);
        assert_eq!(r.update(0, 0.25), 0.25);
    }
}
