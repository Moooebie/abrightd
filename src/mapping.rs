//! Port of AOSP `BrightnessMappingStrategy` (the `SimpleMappingStrategy`).
//!
//! Original: `services/core/java/com/android/server/display/BrightnessMappingStrategy.java`
//! Copyright (C) 2014 The Android Open Source Project — Apache-2.0.

use crate::short_term::SHORT_TERM_MODEL_THRESHOLD_RATIO;
use crate::spline::{create_spline, Spline};
use crate::{NO_USER_BRIGHTNESS, NO_USER_LUX};

const LUX_GRAD_SMOOTHING: f32 = 0.25;
const MAX_GRAD: f32 = 1.0;
/// `MIN_PERMISSABLE_INCREASE` (sic) guarantees a monotone curve can always
/// increase, even when brightness is 0.
const MIN_PERMISSABLE_INCREASE: f32 = 0.004;
/// `config_autoBrightnessAdjustmentMaxGamma` default.
pub const DEFAULT_MAX_GAMMA: f32 = 3.0;

/// Maps ambient lux to a normalized brightness in `[0, 1]`.
pub trait BrightnessMappingStrategy: Send + Sync {
    /// Normalized brightness for a given lux (not clamped to [0, 1]).
    fn get_brightness(&self, lux: f32) -> f32;
    fn get_auto_brightness_adjustment(&self) -> f32;
    /// Returns `true` if the adjustment changed.
    fn set_auto_brightness_adjustment(&mut self, adjustment: f32) -> bool;
    fn add_user_data_point(&mut self, lux: f32, brightness: f32);
    /// Restore a persisted user point *without* re-inferring the adjustment, so
    /// the saved net effect is reproduced exactly.
    fn restore_user_point(&mut self, lux: f32, brightness: f32);
    fn clear_user_data_points(&mut self);
    fn has_user_data_points(&self) -> bool;
    fn short_term_model_timeout_ms(&self) -> u64;
    fn get_user_lux(&self) -> f32;
    fn get_user_brightness(&self) -> f32;
    /// Convert normalized brightness to nits, if a physical profile is known.
    fn convert_to_nits(&self, _brightness: f32) -> Option<f32> {
        None
    }

    /// Whether the learned model should be reset at this ambient lux.
    fn should_reset_short_term_model(&self, ambient_lux: f32, anchor: f32) -> bool {
        crate::short_term::should_reset_short_term_model(
            ambient_lux,
            anchor,
            SHORT_TERM_MODEL_THRESHOLD_RATIO,
            SHORT_TERM_MODEL_THRESHOLD_RATIO,
        )
    }
}

/// Maps ambient lux directly to normalized backlight, as there is no
/// information about the display's physical brightness.
pub struct SimpleMappingStrategy {
    lux: Vec<f32>,
    brightness: Vec<f32>,
    spline: Box<dyn Spline>,
    max_gamma: f32,
    auto_brightness_adjustment: f32,
    user_lux: f32,
    user_brightness: f32,
    short_term_model_timeout_ms: u64,
}

impl SimpleMappingStrategy {
    /// `brightness` is expected normalized to `[0, 1]`.
    pub fn new(
        lux: &[f32],
        brightness: &[f32],
        max_gamma: f32,
        short_term_model_timeout_ms: u64,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            !lux.is_empty(),
            "Lux and brightness arrays must not be empty!"
        );
        anyhow::ensure!(
            lux.len() == brightness.len(),
            "Lux and brightness arrays must be the same length!"
        );
        let mut s = Self {
            lux: lux.to_vec(),
            brightness: brightness.to_vec(),
            spline: Box::new(crate::spline::LinearSpline::new(lux, brightness)?),
            max_gamma,
            auto_brightness_adjustment: 0.0,
            user_lux: NO_USER_LUX,
            user_brightness: NO_USER_BRIGHTNESS,
            short_term_model_timeout_ms,
        };
        s.compute_spline()?;
        Ok(s)
    }

    /// The PLAN's default curve seed.
    pub fn default_curve() -> Self {
        Self::new(
            &[
                0.0, 10.0, 50.0, 100.0, 500.0, 1000.0, 5000.0, 10_000.0, 100_000.0,
            ],
            &[0.03, 0.06, 0.12, 0.2, 0.35, 0.5, 0.7, 0.85, 1.0],
            DEFAULT_MAX_GAMMA,
            1_800_000,
        )
        .expect("default curve is valid")
    }

    fn compute_spline(&mut self) -> anyhow::Result<()> {
        let (lux, brightness) = self.adjusted_curve();
        self.spline = create_spline(&lux, &brightness)?;
        Ok(())
    }

    fn adjusted_curve(&self) -> (Vec<f32>, Vec<f32>) {
        get_adjusted_curve(
            &self.lux,
            &self.brightness,
            self.user_lux,
            self.user_brightness,
            self.auto_brightness_adjustment,
            self.max_gamma,
        )
    }

    fn unadjusted_brightness(&self, lux: f32) -> f32 {
        create_spline(&self.lux, &self.brightness)
            .map(|s| s.interpolate(lux))
            .unwrap_or(f32::NAN)
    }

    pub fn raw_lux(&self) -> &[f32] {
        &self.lux
    }

    pub fn raw_brightness(&self) -> &[f32] {
        &self.brightness
    }
}

impl BrightnessMappingStrategy for SimpleMappingStrategy {
    fn get_brightness(&self, lux: f32) -> f32 {
        self.spline.interpolate(lux)
    }

    fn get_auto_brightness_adjustment(&self) -> f32 {
        self.auto_brightness_adjustment
    }

    fn set_auto_brightness_adjustment(&mut self, adjustment: f32) -> bool {
        let adjustment = adjustment.clamp(-1.0, 1.0);
        if adjustment == self.auto_brightness_adjustment {
            return false;
        }
        self.auto_brightness_adjustment = adjustment;
        let _ = self.compute_spline();
        true
    }

    fn add_user_data_point(&mut self, lux: f32, brightness: f32) {
        let unadjusted = self.unadjusted_brightness(lux);
        self.auto_brightness_adjustment =
            infer_auto_brightness_adjustment(self.max_gamma, brightness, unadjusted);
        self.user_lux = lux;
        self.user_brightness = brightness;
        let _ = self.compute_spline();
    }

    fn restore_user_point(&mut self, lux: f32, brightness: f32) {
        if lux == NO_USER_LUX || brightness == NO_USER_BRIGHTNESS {
            return;
        }
        // Set the point and the adjustment *independently*; do not re-infer.
        self.user_lux = lux;
        self.user_brightness = brightness;
        let _ = self.compute_spline();
    }

    fn clear_user_data_points(&mut self) {
        if self.user_lux != NO_USER_LUX {
            self.auto_brightness_adjustment = 0.0;
            self.user_lux = NO_USER_LUX;
            self.user_brightness = NO_USER_BRIGHTNESS;
            let _ = self.compute_spline();
        }
    }

    fn has_user_data_points(&self) -> bool {
        self.user_lux != NO_USER_LUX
    }

    fn short_term_model_timeout_ms(&self) -> u64 {
        self.short_term_model_timeout_ms
    }

    fn get_user_lux(&self) -> f32 {
        self.user_lux
    }

    fn get_user_brightness(&self) -> f32 {
        self.user_brightness
    }

    fn convert_to_nits(&self, _brightness: f32) -> Option<f32> {
        None
    }
}

/// Infer the auto-brightness adjustment from `desired = current^gamma`.
///
/// Edge cases are ported verbatim from AOSP.
pub fn infer_auto_brightness_adjustment(
    max_gamma: f32,
    desired_brightness: f32,
    current_brightness: f32,
) -> f32 {
    let adjustment = if current_brightness <= 0.1 || current_brightness >= 0.9 {
        // Extreme edge cases: use a simpler heuristic, as proper gamma correction
        // around the edges affects the curve rather drastically.
        desired_brightness - current_brightness
    } else if desired_brightness == 0.0 {
        -1.0
    } else if desired_brightness == 1.0 {
        1.0
    } else {
        // current^gamma = desired => gamma = log[current](desired)
        let gamma = desired_brightness.ln() / current_brightness.ln();
        // max^-adjustment = gamma => adjustment = -log[max](gamma)
        -gamma.ln() / max_gamma.ln()
    };
    adjustment.clamp(-1.0, 1.0)
}

/// Apply the gamma adjustment and insert the user control point.
fn get_adjusted_curve(
    lux: &[f32],
    brightness: &[f32],
    user_lux: f32,
    user_brightness: f32,
    adjustment: f32,
    max_gamma: f32,
) -> (Vec<f32>, Vec<f32>) {
    let mut new_lux = lux.to_vec();
    let mut new_brightness = brightness.to_vec();
    let adjustment = adjustment.clamp(-1.0, 1.0);
    let gamma = max_gamma.powf(-adjustment);
    if gamma != 1.0 {
        for b in new_brightness.iter_mut() {
            *b = b.powf(gamma);
        }
    }
    if user_lux != NO_USER_LUX {
        let (l, b) = insert_control_point(&new_lux, &new_brightness, user_lux, user_brightness);
        new_lux = l;
        new_brightness = b;
    }
    (new_lux, new_brightness)
}

/// Insert a control point, then smooth the curve around it.
fn insert_control_point(
    lux_levels: &[f32],
    brightness_levels: &[f32],
    lux: f32,
    brightness: f32,
) -> (Vec<f32>, Vec<f32>) {
    let idx = find_insertion_point(lux_levels, lux);
    let mut new_lux_levels;
    let mut new_brightness_levels;
    if idx == lux_levels.len() {
        new_lux_levels = lux_levels.to_vec();
        new_brightness_levels = brightness_levels.to_vec();
        new_lux_levels.push(lux);
        new_brightness_levels.push(brightness);
    } else if lux_levels[idx] == lux {
        new_lux_levels = lux_levels.to_vec();
        new_brightness_levels = brightness_levels.to_vec();
        new_brightness_levels[idx] = brightness;
    } else {
        new_lux_levels = lux_levels.to_vec();
        new_lux_levels.insert(idx, lux);
        new_brightness_levels = brightness_levels.to_vec();
        new_brightness_levels.insert(idx, brightness);
    }
    smooth_curve(&new_lux_levels, &mut new_brightness_levels, idx);
    (new_lux_levels, new_brightness_levels)
}

/// Index of the first value that is `>= val`.  Assumes `arr` is sorted.
fn find_insertion_point(arr: &[f32], val: f32) -> usize {
    for (i, &v) in arr.iter().enumerate() {
        if val <= v {
            return i;
        }
    }
    arr.len()
}

fn smooth_curve(lux: &[f32], brightness: &mut [f32], idx: usize) {
    // Smooth curve for data points above the newly introduced point.
    let mut prev_lux = lux[idx];
    let mut prev_brightness = brightness[idx];
    for i in idx + 1..lux.len() {
        let curr_lux = lux[i];
        let curr_brightness = brightness[i];
        let max_brightness = (prev_brightness * permissible_ratio(curr_lux, prev_lux))
            .max(prev_brightness + MIN_PERMISSABLE_INCREASE);
        let new_brightness = curr_brightness.clamp(prev_brightness, max_brightness);
        if new_brightness == curr_brightness {
            break;
        }
        prev_lux = curr_lux;
        prev_brightness = new_brightness;
        brightness[i] = new_brightness;
    }
    // Smooth curve for data points below the newly introduced point.
    prev_lux = lux[idx];
    prev_brightness = brightness[idx];
    for i in (0..idx).rev() {
        let curr_lux = lux[i];
        let curr_brightness = brightness[i];
        let min_brightness = prev_brightness * permissible_ratio(curr_lux, prev_lux);
        let new_brightness = curr_brightness.clamp(min_brightness, prev_brightness);
        if new_brightness == curr_brightness {
            break;
        }
        prev_lux = curr_lux;
        prev_brightness = new_brightness;
        brightness[i] = new_brightness;
    }
}

fn permissible_ratio(curr_lux: f32, prev_lux: f32) -> f32 {
    ((curr_lux + LUX_GRAD_SMOOTHING) / (prev_lux + LUX_GRAD_SMOOTHING)).powf(MAX_GRAD)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn default_curve_is_monotonic() {
        let m = SimpleMappingStrategy::default_curve();
        let mut prev = -1.0;
        for lux in [0.0f32, 5.0, 50.0, 500.0, 5000.0, 100_000.0, 1_000_000.0] {
            let b = m.get_brightness(lux);
            assert!(b >= prev - 1e-6, "not monotonic at {lux}: {b} < {prev}");
            assert!((0.0..=1.0).contains(&b));
            prev = b;
        }
    }

    #[test]
    fn adjustment_gamma_changes_midpoint() {
        let mut m = SimpleMappingStrategy::default_curve();
        let base = m.get_brightness(500.0);
        assert!(m.set_auto_brightness_adjustment(-1.0));
        let darker = m.get_brightness(500.0);
        assert!(
            darker < base,
            "adjustment -1 should darken: {darker} !< {base}"
        );
        assert!(m.set_auto_brightness_adjustment(1.0));
        let brighter = m.get_brightness(500.0);
        assert!(
            brighter > base,
            "adjustment +1 should brighten: {brighter} !< {base}"
        );
    }

    #[test]
    fn user_data_point_is_reproduced() {
        let mut m = SimpleMappingStrategy::default_curve();
        let lux = 250.0;
        let desired = 0.42;
        m.add_user_data_point(lux, desired);
        assert!(m.has_user_data_points());
        let got = m.get_brightness(lux);
        assert!(approx(got, desired), "expected {desired}, got {got}");
    }

    #[test]
    fn user_data_point_edge_cases() {
        assert_eq!(infer_auto_brightness_adjustment(3.0, 0.0, 0.5), -1.0);
        assert_eq!(infer_auto_brightness_adjustment(3.0, 1.0, 0.5), 1.0);
        // current outside (0.1, 0.9) falls back to a linear delta.
        assert!(approx(
            infer_auto_brightness_adjustment(3.0, 0.5, 0.95),
            -0.45
        ));
    }

    #[test]
    fn clear_removes_user_point() {
        let mut m = SimpleMappingStrategy::default_curve();
        m.add_user_data_point(250.0, 0.42);
        assert!(m.has_user_data_points());
        m.clear_user_data_points();
        assert!(!m.has_user_data_points());
        assert_eq!(m.get_auto_brightness_adjustment(), 0.0);
    }

    #[test]
    fn restore_reproduces_the_net_effect() {
        // Live: adding a point overwrites the adjustment (AOSP).
        let mut live = SimpleMappingStrategy::default_curve();
        live.add_user_data_point(250.0, 0.42);
        let adjustment = live.get_auto_brightness_adjustment();

        // Restored: apply the saved adjustment, then insert the point directly.
        let mut restored = SimpleMappingStrategy::default_curve();
        restored.set_auto_brightness_adjustment(adjustment);
        restored.restore_user_point(250.0, 0.42);

        for lux in [0.0f32, 10.0, 40.0, 250.0, 1000.0, 5000.0, 100_000.0] {
            let a = live.get_brightness(lux);
            let b = restored.get_brightness(lux);
            assert!((a - b).abs() < 1e-6, "lux {lux}: live {a} != restored {b}");
        }
    }
}
