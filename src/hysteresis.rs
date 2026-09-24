//! Port of AOSP `HysteresisLevels`.
//!
//! Original: `services/core/java/com/android/server/display/HysteresisLevels.java`
//! Copyright (C) 2016 The Android Open Source Project — Apache-2.0.

use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum HysteresisError {
    #[error("Mismatch between hysteresis array lengths.")]
    LengthMismatch,
}

/// A helper for handling illuminance / brightness hysteresis thresholds.
///
/// Threshold percentages are stored as fractions (AOSP stores them as
/// thousandths of a percent, i.e. `0.10` == 10%).
#[derive(Debug, Clone)]
pub struct HysteresisLevels {
    brightening_percentages: Vec<f32>,
    darkening_percentages: Vec<f32>,
    brightening_levels: Vec<f32>,
    darkening_levels: Vec<f32>,
    min_darkening: f32,
    min_brightening: f32,
}

impl HysteresisLevels {
    pub fn new(
        brightening_thresholds_percentages: &[f32],
        darkening_thresholds_percentages: &[f32],
        brightening_threshold_levels: &[f32],
        darkening_threshold_levels: &[f32],
        min_darkening_threshold: f32,
        min_brightening_threshold: f32,
    ) -> Result<Self, HysteresisError> {
        if brightening_thresholds_percentages.len() != brightening_threshold_levels.len()
            || darkening_thresholds_percentages.len() != darkening_threshold_levels.len()
        {
            return Err(HysteresisError::LengthMismatch);
        }
        Ok(Self {
            brightening_percentages: brightening_thresholds_percentages.to_vec(),
            darkening_percentages: darkening_thresholds_percentages.to_vec(),
            brightening_levels: brightening_threshold_levels.to_vec(),
            darkening_levels: darkening_threshold_levels.to_vec(),
            min_darkening: min_darkening_threshold,
            min_brightening: min_brightening_threshold,
        })
    }

    /// AOSP fallback hysteresis for ambient lux.
    ///
    /// From `config.xml` / `HysteresisLevels`: a single threshold level `0`
    /// with 10% brightening / 20% darkening and **no minimum** (AOSP uses `0f`
    /// when the device config supplies no minimum).
    pub fn default_ambient() -> Self {
        Self::new(&[0.10], &[0.20], &[0.0], &[0.0], 0.0, 0.0).unwrap()
    }

    /// AOSP fallback hysteresis for screen brightness (normalized [0, 1]).
    pub fn default_screen() -> Self {
        Self::new(&[0.10], &[0.20], &[0.0], &[0.0], 0.0, 0.0).unwrap()
    }

    /// Return the brightening hysteresis threshold for the given value level.
    pub fn brightening_threshold(&self, value: f32) -> f32 {
        let bright_constant = reference_level(
            value,
            &self.brightening_levels,
            &self.brightening_percentages,
        );
        let bright_threshold = value * (1.0 + bright_constant);
        bright_threshold.max(value + self.min_brightening)
    }

    /// Return the darkening hysteresis threshold for the given value level.
    pub fn darkening_threshold(&self, value: f32) -> f32 {
        let dark_constant =
            reference_level(value, &self.darkening_levels, &self.darkening_percentages);
        let dark_threshold = value * (1.0 - dark_constant);
        (dark_threshold.min(value - self.min_darkening)).max(0.0)
    }

    pub fn min_brightening(&self) -> f32 {
        self.min_brightening
    }

    pub fn min_darkening(&self) -> f32 {
        self.min_darkening
    }
}

/// Return the hysteresis constant for the closest threshold value.
fn reference_level(value: f32, levels: &[f32], percentages: &[f32]) -> f32 {
    if levels.is_empty() || value < levels[0] {
        return 0.0;
    }
    let mut index = 0usize;
    while index < levels.len() - 1 && value >= levels[index + 1] {
        index += 1;
    }
    percentages[index]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thresholds_bracket_the_value() {
        let h = HysteresisLevels::default_ambient();
        for v in [1.0f32, 50.0, 150.0, 1000.0, 100000.0] {
            let b = h.brightening_threshold(v);
            let d = h.darkening_threshold(v);
            assert!(b > v, "brightening {b} not > {v}");
            assert!(d < v, "darkening {d} not < {v}");
            assert!(d >= 0.0);
        }
    }

    #[test]
    fn reference_level_walks_points() {
        assert_eq!(
            reference_level(50.0, &[0.0, 100.0, 200.0], &[0.1, 0.2, 0.3]),
            0.1
        );
        assert_eq!(
            reference_level(100.0, &[0.0, 100.0, 200.0], &[0.1, 0.2, 0.3]),
            0.2
        );
        assert_eq!(
            reference_level(250.0, &[0.0, 100.0, 200.0], &[0.1, 0.2, 0.3]),
            0.3
        );
        assert_eq!(reference_level(-1.0, &[0.0], &[0.1]), 0.0);
        assert_eq!(reference_level(5.0, &[], &[]), 0.0);
    }

    #[test]
    fn empty_levels_use_minima() {
        // With no threshold points, AOSP's reference level is 0 and the minima
        // are what provide hysteresis.
        let h = HysteresisLevels::new(&[], &[], &[], &[], 0.2, 0.1).unwrap();
        assert!((h.brightening_threshold(1.0) - 1.1).abs() < 1e-6);
        assert!((h.darkening_threshold(1.0) - 0.8).abs() < 1e-6);
    }

    #[test]
    fn rejects_length_mismatch() {
        assert_eq!(
            HysteresisLevels::new(&[0.1], &[0.1], &[], &[], 0.0, 0.0).unwrap_err(),
            HysteresisError::LengthMismatch
        );
    }
}
