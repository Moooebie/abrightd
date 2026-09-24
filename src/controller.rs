//! Port of AOSP `AutomaticBrightnessController`.
//!
//! Original: `services/core/java/com/android/server/display/AutomaticBrightnessController.java`
//! Copyright (C) 2013 The Android Open Source Project — Apache-2.0.
//!
//! Unlike AOSP this is not an event handler: it is a pure state machine driven
//! by explicit `now_ms` arguments.  The caller feeds sensor samples with
//! [`AutomaticBrightnessController::handle_light_sensor_event`] and fires
//! scheduled wake-ups with [`AutomaticBrightnessController::on_timer`]; each
//! call returns a [`ControllerOutput`] describing any brightness update and the
//! next wake-up time.

use std::collections::VecDeque;

use crate::hysteresis::HysteresisLevels;
use crate::mapping::BrightnessMappingStrategy;
use crate::ring_buffer::AmbientLightRingBuffer;
use crate::short_term::ShortTermModel;

/// Debounce for sampling user-initiated brightness changes.
pub const BRIGHTNESS_ADJUSTMENT_SAMPLE_DEBOUNCE_MILLIS: i64 = 10_000;

/// Tunables mirrored from `config.xml` / `DisplayDeviceConfig`.
#[derive(Debug, Clone)]
pub struct ControllerConfig {
    pub screen_brightness_range_minimum: f32,
    pub screen_brightness_range_maximum: f32,
    pub light_sensor_warm_up_time_ms: i64,
    pub doze_scale_factor: f32,
    pub normal_light_sensor_rate_ms: i64,
    pub initial_light_sensor_rate_ms: i64,
    pub brightening_light_debounce_ms: i64,
    pub darkening_light_debounce_ms: i64,
    pub reset_ambient_lux_after_warm_up: bool,
    pub ambient_light_horizon_long_ms: i64,
    pub ambient_light_horizon_short_ms: i64,
    pub ambient_thresholds: HysteresisLevels,
    pub screen_thresholds: HysteresisLevels,
}

impl Default for ControllerConfig {
    fn default() -> Self {
        Self {
            screen_brightness_range_minimum: 0.0,
            screen_brightness_range_maximum: 1.0,
            light_sensor_warm_up_time_ms: 0,
            doze_scale_factor: 1.0,
            normal_light_sensor_rate_ms: 200,
            initial_light_sensor_rate_ms: 1000,
            brightening_light_debounce_ms: 2000,
            darkening_light_debounce_ms: 4000,
            reset_ambient_lux_after_warm_up: true,
            ambient_light_horizon_long_ms: 10_000,
            ambient_light_horizon_short_ms: 2_000,
            ambient_thresholds: HysteresisLevels::default_ambient(),
            screen_thresholds: HysteresisLevels::default_screen(),
        }
    }
}

/// What the controller wants the caller to do after a step.
#[derive(Debug, Clone, Default)]
pub struct ControllerOutput {
    /// A new normalized brightness was emitted (`sendUpdate`).
    pub brightness: Option<f32>,
    /// When the controller next needs a timer wake-up.
    pub next_wakeup_ms: Option<i64>,
}

impl ControllerOutput {
    fn merge(mut self, other: ControllerOutput) -> ControllerOutput {
        if other.brightness.is_some() {
            self.brightness = other.brightness;
        }
        self.next_wakeup_ms = min_opt(self.next_wakeup_ms, other.next_wakeup_ms);
        self
    }
}

fn min_opt(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, b) => b,
    }
}

pub struct AutomaticBrightnessController {
    config: ControllerConfig,
    mapper: Box<dyn BrightnessMappingStrategy>,

    // --- inputs / config-derived state ---
    light_sensor_enabled: bool,
    light_sensor_enable_time: i64,
    current_light_sensor_rate: i64,

    // --- ambient estimate state ---
    ambient_lux: f32,
    slow_ambient_lux: f32,
    fast_ambient_lux: f32,
    ambient_lux_valid: bool,
    ambient_brightening_threshold: f32,
    ambient_darkening_threshold: f32,
    pre_threshold_lux: f32,

    // --- brightness state ---
    screen_auto_brightness: f32,
    raw_screen_auto_brightness: f32,
    pre_threshold_brightness: f32,
    screen_brightening_threshold: f32,
    screen_darkening_threshold: f32,

    // --- observations ---
    recent_light_samples: u32,
    last_observed_lux: f32,
    last_observed_lux_time: i64,
    ring: AmbientLightRingBuffer,

    // --- learning ---
    short_term: ShortTermModel,

    // --- user adjustment sampling ---
    brightness_adjustment_sample_pending: bool,
    brightness_adjustment_sample_old_lux: f32,
    brightness_adjustment_sample_old_brightness: f32,
    brightness_adjustment_sample_at: Option<i64>,

    // --- handler timers ---
    ambient_update_at: Option<i64>,
    invalidate_short_term_at: Option<i64>,

    /// Pending brightness updates queued during a single step.
    pending: VecDeque<f32>,
}

impl AutomaticBrightnessController {
    pub fn new(config: ControllerConfig, mapper: Box<dyn BrightnessMappingStrategy>) -> Self {
        let mut controller = Self {
            light_sensor_enabled: false,
            light_sensor_enable_time: 0,
            current_light_sensor_rate: -1,
            ambient_lux: 0.0,
            slow_ambient_lux: 0.0,
            fast_ambient_lux: 0.0,
            ambient_lux_valid: false,
            ambient_brightening_threshold: 0.0,
            ambient_darkening_threshold: 0.0,
            pre_threshold_lux: f32::NAN,
            screen_auto_brightness: f32::NAN,
            raw_screen_auto_brightness: f32::NAN,
            pre_threshold_brightness: f32::NAN,
            screen_brightening_threshold: 0.0,
            screen_darkening_threshold: 0.0,
            recent_light_samples: 0,
            last_observed_lux: 0.0,
            last_observed_lux_time: 0,
            ring: AmbientLightRingBuffer::new(),
            short_term: ShortTermModel::new(),
            brightness_adjustment_sample_pending: false,
            brightness_adjustment_sample_old_lux: -1.0,
            brightness_adjustment_sample_old_brightness: f32::NAN,
            brightness_adjustment_sample_at: None,
            ambient_update_at: None,
            invalidate_short_term_at: None,
            config,
            mapper,
            pending: VecDeque::new(),
        };
        // Mirror AOSP's constructor: initialise the short-term model from the
        // supplied user point (default: none).
        let (lux, brightness) = (
            controller.mapper.get_user_lux(),
            controller.mapper.get_user_brightness(),
        );
        controller.set_screen_brightness_by_user_at(lux, brightness);
        controller
    }

    // ------------------------------------------------------------------
    // Accessors
    // ------------------------------------------------------------------

    pub fn get_automatic_screen_brightness(&self) -> f32 {
        if !self.ambient_lux_valid {
            return f32::NAN;
        }
        self.screen_auto_brightness
    }

    pub fn get_raw_automatic_screen_brightness(&self) -> f32 {
        self.raw_screen_auto_brightness
    }

    pub fn has_valid_ambient_lux(&self) -> bool {
        self.ambient_lux_valid
    }

    pub fn ambient_lux(&self) -> f32 {
        self.ambient_lux
    }

    pub fn slow_ambient_lux(&self) -> f32 {
        self.slow_ambient_lux
    }

    pub fn fast_ambient_lux(&self) -> f32 {
        self.fast_ambient_lux
    }

    pub fn ambient_brightening_threshold(&self) -> f32 {
        self.ambient_brightening_threshold
    }

    pub fn ambient_darkening_threshold(&self) -> f32 {
        self.ambient_darkening_threshold
    }

    pub fn screen_brightening_threshold(&self) -> f32 {
        self.screen_brightening_threshold
    }

    pub fn screen_darkening_threshold(&self) -> f32 {
        self.screen_darkening_threshold
    }

    pub fn last_observed_lux(&self) -> f32 {
        self.last_observed_lux
    }

    pub fn last_observed_lux_time(&self) -> i64 {
        self.last_observed_lux_time
    }

    pub fn recent_light_samples(&self) -> u32 {
        self.recent_light_samples
    }

    pub fn current_light_sensor_rate_ms(&self) -> i64 {
        self.current_light_sensor_rate
    }

    pub fn light_sensor_enabled(&self) -> bool {
        self.light_sensor_enabled
    }

    pub fn has_user_data_points(&self) -> bool {
        self.mapper.has_user_data_points()
    }

    pub fn get_auto_brightness_adjustment(&self) -> f32 {
        self.mapper.get_auto_brightness_adjustment()
    }

    pub fn mapper(&self) -> &dyn BrightnessMappingStrategy {
        self.mapper.as_ref()
    }

    pub fn mapper_mut(&mut self) -> &mut dyn BrightnessMappingStrategy {
        self.mapper.as_mut()
    }

    pub fn ring(&self) -> &AmbientLightRingBuffer {
        &self.ring
    }

    pub fn short_term(&self) -> &ShortTermModel {
        &self.short_term
    }

    /// Returns and clears the queued brightness updates from the last step.
    pub fn take_pending(&mut self) -> Vec<f32> {
        self.pending.drain(..).collect()
    }

    pub fn next_wakeup_ms(&self) -> Option<i64> {
        min_opt(
            min_opt(self.ambient_update_at, self.brightness_adjustment_sample_at),
            self.invalidate_short_term_at,
        )
    }

    fn queue_brightness(&mut self) {
        self.pending
            .push_back(self.get_automatic_screen_brightness());
    }

    // ------------------------------------------------------------------
    // Sensor lifecycle
    // ------------------------------------------------------------------

    /// Enables or disables the light sensor.  Mirrors `setLightSensorEnabled`.
    pub fn set_light_sensor_enabled(&mut self, enable: bool, now_ms: i64) -> bool {
        if enable {
            if !self.light_sensor_enabled {
                self.light_sensor_enabled = true;
                self.light_sensor_enable_time = now_ms;
                self.current_light_sensor_rate = self.config.initial_light_sensor_rate_ms;
                return true;
            }
        } else if self.light_sensor_enabled {
            self.light_sensor_enabled = false;
            self.ambient_lux_valid = !self.config.reset_ambient_lux_after_warm_up;
            if !self.ambient_lux_valid {
                self.pre_threshold_lux = f32::NAN;
            }
            self.screen_auto_brightness = f32::NAN;
            self.raw_screen_auto_brightness = f32::NAN;
            self.pre_threshold_brightness = f32::NAN;
            self.recent_light_samples = 0;
            self.ring.clear();
            self.current_light_sensor_rate = -1;
            self.ambient_update_at = None;
        }
        false
    }

    /// Feed one ambient-light sample.
    pub fn handle_light_sensor_event(&mut self, now_ms: i64, lux: f32) -> ControllerOutput {
        // The AOSP listener ignores events when the sensor is disabled.
        if !self.light_sensor_enabled {
            return ControllerOutput::default();
        }
        // mHandler.removeMessages(MSG_UPDATE_AMBIENT_LUX)
        self.ambient_update_at = None;

        if self.ring.is_empty() {
            // Switch to the steady-state sample rate after the first sample.
            self.adjust_light_sensor_rate(self.config.normal_light_sensor_rate_ms);
        }
        self.apply_light_sensor_measurement(now_ms, lux);
        self.update_ambient_lux(now_ms)
    }

    fn apply_light_sensor_measurement(&mut self, time_ms: i64, lux: f32) {
        self.recent_light_samples += 1;
        self.ring
            .prune(time_ms - self.config.ambient_light_horizon_long_ms);
        self.ring.push(time_ms, lux);
        self.last_observed_lux = lux;
        self.last_observed_lux_time = time_ms;
    }

    fn adjust_light_sensor_rate(&mut self, light_sensor_rate: i64) {
        if light_sensor_rate != self.current_light_sensor_rate {
            self.current_light_sensor_rate = light_sensor_rate;
        }
    }

    /// Run any scheduled work that is due at `now_ms`.
    pub fn on_timer(&mut self, now_ms: i64) -> ControllerOutput {
        let mut out = ControllerOutput::default();
        if self.ambient_update_at.is_some_and(|t| t <= now_ms) {
            self.ambient_update_at = None;
            self.ring
                .prune(now_ms - self.config.ambient_light_horizon_long_ms);
            out = out.merge(self.update_ambient_lux(now_ms));
        }
        if self
            .brightness_adjustment_sample_at
            .is_some_and(|t| t <= now_ms)
        {
            self.brightness_adjustment_sample_at = None;
            self.collect_brightness_adjustment_sample();
        }
        if self.invalidate_short_term_at.is_some_and(|t| t <= now_ms) {
            self.invalidate_short_term_at = None;
            self.short_term.invalidate();
        }
        out.next_wakeup_ms = self.next_wakeup_ms();
        out
    }

    // ------------------------------------------------------------------
    // Ambient lux
    // ------------------------------------------------------------------

    fn set_ambient_lux(&mut self, lux: f32) {
        let lux = if lux < 0.0 { 0.0 } else { lux };
        self.ambient_lux = lux;
        self.ambient_brightening_threshold =
            self.config.ambient_thresholds.brightening_threshold(lux);
        self.ambient_darkening_threshold = self.config.ambient_thresholds.darkening_threshold(lux);

        // If the short term model was invalidated and the change is drastic
        // enough, reset it.
        self.short_term_maybe_reset(self.ambient_lux);
    }

    fn calculate_ambient_lux(&self, now_ms: i64, horizon_ms: i64) -> Option<f32> {
        self.ring.calculate_ambient_lux(now_ms, horizon_ms)
    }

    fn next_ambient_light_brightening_transition(&self, time_ms: i64) -> i64 {
        let n = self.ring.size();
        let mut earliest_valid_time = time_ms;
        for i in (0..n).rev() {
            if self.ring.get_lux(i) <= self.ambient_brightening_threshold {
                break;
            }
            earliest_valid_time = self.ring.get_time(i);
        }
        earliest_valid_time + self.config.brightening_light_debounce_ms
    }

    fn next_ambient_light_darkening_transition(&self, time_ms: i64) -> i64 {
        let n = self.ring.size();
        let mut earliest_valid_time = time_ms;
        for i in (0..n).rev() {
            if self.ring.get_lux(i) >= self.ambient_darkening_threshold {
                break;
            }
            earliest_valid_time = self.ring.get_time(i);
        }
        earliest_valid_time + self.config.darkening_light_debounce_ms
    }

    fn update_ambient_lux(&mut self, time_ms: i64) -> ControllerOutput {
        let mut out = ControllerOutput::default();

        // If the light sensor was just turned on then immediately update our
        // initial estimate of the current ambient light level.
        if !self.ambient_lux_valid {
            let time_when_sensor_warmed_up =
                self.config.light_sensor_warm_up_time_ms + self.light_sensor_enable_time;
            if time_ms < time_when_sensor_warmed_up {
                self.ambient_update_at = Some(time_when_sensor_warmed_up);
                out.next_wakeup_ms = self.next_wakeup_ms();
                return out;
            }
            if let Some(lux) =
                self.calculate_ambient_lux(time_ms, self.config.ambient_light_horizon_short_ms)
            {
                self.set_ambient_lux(lux);
            }
            self.ambient_lux_valid = true;
            out = out.merge(self.update_auto_brightness(true, false));
        }

        let mut next_brighten_transition = self.next_ambient_light_brightening_transition(time_ms);
        let mut next_darken_transition = self.next_ambient_light_darkening_transition(time_ms);

        // A slow estimate ensures there is a true long-term change, while a fast
        // estimate determines the new brightness.  Both must cross the threshold.
        self.slow_ambient_lux = self
            .calculate_ambient_lux(time_ms, self.config.ambient_light_horizon_long_ms)
            .unwrap_or(self.ambient_lux);
        self.fast_ambient_lux = self
            .calculate_ambient_lux(time_ms, self.config.ambient_light_horizon_short_ms)
            .unwrap_or(self.ambient_lux);

        if (self.slow_ambient_lux >= self.ambient_brightening_threshold
            && self.fast_ambient_lux >= self.ambient_brightening_threshold
            && next_brighten_transition <= time_ms)
            || (self.slow_ambient_lux <= self.ambient_darkening_threshold
                && self.fast_ambient_lux <= self.ambient_darkening_threshold
                && next_darken_transition <= time_ms)
        {
            self.pre_threshold_lux = self.ambient_lux;
            let fast = self.fast_ambient_lux;
            self.set_ambient_lux(fast);
            out = out.merge(self.update_auto_brightness(true, false));
            next_brighten_transition = self.next_ambient_light_brightening_transition(time_ms);
            next_darken_transition = self.next_ambient_light_darkening_transition(time_ms);
        }

        let mut next_transition_time = next_darken_transition.min(next_brighten_transition);
        if next_transition_time <= time_ms {
            next_transition_time = time_ms + self.config.normal_light_sensor_rate_ms;
        }
        self.ambient_update_at = Some(next_transition_time);
        out.next_wakeup_ms = self.next_wakeup_ms();
        out
    }

    // ------------------------------------------------------------------
    // Brightness
    // ------------------------------------------------------------------

    fn update_auto_brightness(
        &mut self,
        send_update: bool,
        is_manually_set: bool,
    ) -> ControllerOutput {
        let mut out = ControllerOutput::default();
        if !self.ambient_lux_valid {
            return out;
        }

        let value = self.mapper.get_brightness(self.ambient_lux);
        self.raw_screen_auto_brightness = value;
        let new_screen_auto_brightness = self.clamp_screen_brightness(value);

        // The min/max range can change (e.g. HBM); check the current value is
        // still within the allowed range.
        let current_brightness_within_allowed_range = float_equals(
            self.screen_auto_brightness,
            self.clamp_screen_brightness(self.screen_auto_brightness),
        );
        let within_threshold = !self.screen_auto_brightness.is_nan()
            && new_screen_auto_brightness > self.screen_darkening_threshold
            && new_screen_auto_brightness < self.screen_brightening_threshold;

        if within_threshold && !is_manually_set && current_brightness_within_allowed_range {
            return out;
        }
        if !float_equals(self.screen_auto_brightness, new_screen_auto_brightness) {
            if !within_threshold {
                self.pre_threshold_brightness = self.screen_auto_brightness;
            }
            self.screen_auto_brightness = new_screen_auto_brightness;
            self.screen_brightening_threshold = self.clamp_screen_brightness(
                self.config
                    .screen_thresholds
                    .brightening_threshold(new_screen_auto_brightness),
            );
            self.screen_darkening_threshold = self.clamp_screen_brightness(
                self.config
                    .screen_thresholds
                    .darkening_threshold(new_screen_auto_brightness),
            );
            if send_update {
                self.queue_brightness();
                out.brightness = Some(self.get_automatic_screen_brightness());
            }
        }
        out
    }

    fn clamp_screen_brightness(&self, value: f32) -> f32 {
        value.clamp(
            self.config.screen_brightness_range_minimum,
            self.config.screen_brightness_range_maximum,
        )
    }

    /// Force a recalculation, mirroring `AutomaticBrightnessController.update()`.
    pub fn update(&mut self, now_ms: i64) -> ControllerOutput {
        let _ = now_ms;
        self.update_auto_brightness(true, false)
    }

    /// Recompute and emit brightness after a user-initiated change, bypassing
    /// the screen hysteresis (AOSP's `isManuallySet`).
    pub fn refresh_after_user_change(&mut self, now_ms: i64) -> ControllerOutput {
        let _ = now_ms;
        self.update_auto_brightness(true, true)
    }

    // ------------------------------------------------------------------
    // User learning
    // ------------------------------------------------------------------

    /// Set brightness from a user action at the current ambient lux.
    pub fn set_screen_brightness_by_user(&mut self, brightness: f32) -> bool {
        if !self.ambient_lux_valid {
            return false;
        }
        let lux = self.ambient_lux;
        self.set_screen_brightness_by_user_at(lux, brightness)
    }

    /// Set brightness from a user action at an explicit lux.
    pub fn set_screen_brightness_by_user_at(&mut self, lux: f32, brightness: f32) -> bool {
        if lux == crate::NO_USER_LUX || brightness == crate::NO_USER_BRIGHTNESS {
            return false;
        }
        self.mapper.add_user_data_point(lux, brightness);
        self.short_term.set_user_brightness(lux, brightness);
        true
    }

    pub fn reset_short_term_model(&mut self) {
        self.mapper.clear_user_data_points();
        self.short_term.reset();
    }

    /// `ShortTermModel.maybeReset`: reset when invalidated and the lux moved
    /// far enough away from the anchor, otherwise re-validate.
    fn short_term_maybe_reset(&mut self, current_lux: f32) -> bool {
        if !self.short_term.is_valid() && self.short_term.has_anchor() {
            let should_reset = self
                .mapper
                .should_reset_short_term_model(current_lux, self.short_term.anchor());
            if should_reset {
                self.mapper.clear_user_data_points();
                self.short_term.reset();
            } else {
                self.short_term
                    .set(self.short_term.anchor(), self.short_term.brightness(), true);
            }
            return self.short_term.is_valid();
        }
        false
    }

    /// Schedule an invalidation of the short-term model after its timeout.
    pub fn schedule_short_term_invalidation(&mut self, now_ms: i64) {
        self.invalidate_short_term_at =
            Some(now_ms + self.mapper.short_term_model_timeout_ms() as i64);
    }

    // ------------------------------------------------------------------
    // Brightness adjustment sampling
    // ------------------------------------------------------------------

    pub fn prepare_brightness_adjustment_sample(&mut self, now_ms: i64) {
        if !self.brightness_adjustment_sample_pending {
            self.brightness_adjustment_sample_pending = true;
            self.brightness_adjustment_sample_old_lux = if self.ambient_lux_valid {
                self.ambient_lux
            } else {
                -1.0
            };
            self.brightness_adjustment_sample_old_brightness = self.screen_auto_brightness;
        }
        self.brightness_adjustment_sample_at =
            Some(now_ms + BRIGHTNESS_ADJUSTMENT_SAMPLE_DEBOUNCE_MILLIS);
    }

    pub fn cancel_brightness_adjustment_sample(&mut self) {
        if self.brightness_adjustment_sample_pending {
            self.brightness_adjustment_sample_pending = false;
            self.brightness_adjustment_sample_at = None;
        }
    }

    /// Returns the collected `(old_lux, old_brightness, lux, brightness)` sample,
    /// if the pending sample was valid.
    pub fn collect_brightness_adjustment_sample(&mut self) -> Option<(f32, f32, f32, f32)> {
        if !self.brightness_adjustment_sample_pending {
            return None;
        }
        self.brightness_adjustment_sample_pending = false;
        // BRIGHTNESS_MIN is 0.0 and BRIGHTNESS_OFF_FLOAT is -1.0.
        if self.ambient_lux_valid
            && (self.screen_auto_brightness >= 0.0 || self.screen_auto_brightness == -1.0)
        {
            Some((
                self.brightness_adjustment_sample_old_lux,
                self.brightness_adjustment_sample_old_brightness,
                self.ambient_lux,
                self.screen_auto_brightness,
            ))
        } else {
            None
        }
    }
}

/// Rough equivalent of `BrightnessSynchronizer.floatEquals`.
fn float_equals(a: f32, b: f32) -> bool {
    if a == b {
        return true;
    }
    if a.is_nan() || b.is_nan() {
        return false;
    }
    (a - b).abs() <= f32::EPSILON * a.abs().max(b.abs()).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::SimpleMappingStrategy;

    fn controller() -> AutomaticBrightnessController {
        let mapper = Box::new(SimpleMappingStrategy::default_curve());
        let config = ControllerConfig {
            light_sensor_warm_up_time_ms: 0,
            ..ControllerConfig::default()
        };
        let mut c = AutomaticBrightnessController::new(config, mapper);
        c.set_light_sensor_enabled(true, 0);
        c
    }

    #[test]
    fn first_sample_initializes_brightness() {
        let mut c = controller();
        let out = c.handle_light_sensor_event(0, 500.0);
        assert!(c.has_valid_ambient_lux());
        assert!(out.brightness.is_some());
        let b = c.get_automatic_screen_brightness();
        assert!((b - 0.35).abs() < 0.05, "got {b}");
    }

    #[test]
    fn constant_lux_does_not_oscillate() {
        let mut c = controller();
        let mut emitted = vec![];
        for t in 0..100 {
            let out = c.handle_light_sensor_event(t * 200, 500.0);
            if let Some(b) = out.brightness {
                emitted.push(b);
            }
        }
        // After the initial sample, constant light must not keep emitting.
        assert!(emitted.len() <= 2, "emitted {emitted:?}");
    }

    #[test]
    fn step_up_brightens_after_debounce() {
        let mut c = controller();
        c.handle_light_sensor_event(0, 10.0);
        let before = c.get_automatic_screen_brightness();
        // Jump to a bright environment and keep sampling.
        let mut last = None;
        for t in 1..80 {
            let out = c.handle_light_sensor_event(t * 200, 2000.0);
            if let Some(b) = out.brightness {
                last = Some(b);
            }
        }
        let after = c.get_automatic_screen_brightness();
        assert!(after > before, "did not brighten: {before} -> {after}");
        assert!(last.unwrap_or(after) > before);
    }

    #[test]
    fn user_point_is_learned() {
        let mut c = controller();
        c.handle_light_sensor_event(0, 500.0);
        assert!(c.set_screen_brightness_by_user(0.8));
        assert!(c.has_user_data_points());
        // Force recalculation and check the learned point is applied.
        let out = c.update(1000);
        let b = out
            .brightness
            .or_else(|| Some(c.get_automatic_screen_brightness()))
            .unwrap();
        assert!((b - 0.8).abs() < 0.05, "got {b}");
    }
}
