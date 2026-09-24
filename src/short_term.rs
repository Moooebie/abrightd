//! Port of AOSP `AutomaticBrightnessController.ShortTermModel` and the
//! `shouldResetShortTermModel` test.
//!
//! Original: `services/core/java/com/android/server/display/AutomaticBrightnessController.java`
//! Copyright (C) 2016 The Android Open Source Project — Apache-2.0.

use crate::{NO_USER_BRIGHTNESS, NO_USER_LUX};

/// Relative band around the anchor lux within which learned user data stays valid.
pub const SHORT_TERM_MODEL_THRESHOLD_RATIO: f32 = 0.6;

/// The recent user preference: an anchor lux and the brightness chosen there.
#[derive(Debug, Clone, PartialEq)]
pub struct ShortTermModel {
    anchor: f32,
    brightness: f32,
    valid: bool,
}

impl Default for ShortTermModel {
    fn default() -> Self {
        Self::new()
    }
}

impl ShortTermModel {
    pub fn new() -> Self {
        Self {
            anchor: NO_USER_LUX,
            brightness: NO_USER_BRIGHTNESS,
            valid: false,
        }
    }

    pub fn reset(&mut self) {
        self.anchor = NO_USER_LUX;
        self.brightness = NO_USER_BRIGHTNESS;
        self.valid = false;
    }

    /// Invalidate without forgetting: the model is re-validated if the ambient
    /// lux is still close enough to the anchor.
    pub fn invalidate(&mut self) {
        self.valid = false;
    }

    pub fn set_user_brightness(&mut self, lux: f32, brightness: f32) {
        self.anchor = lux;
        self.brightness = brightness;
        self.valid = true;
    }

    pub fn set(&mut self, anchor: f32, brightness: f32, valid: bool) {
        self.anchor = anchor;
        self.brightness = brightness;
        self.valid = valid;
    }

    pub fn copy_from(&mut self, from: &ShortTermModel) {
        self.anchor = from.anchor;
        self.brightness = from.brightness;
        self.valid = from.valid;
    }

    pub fn anchor(&self) -> f32 {
        self.anchor
    }

    pub fn brightness(&self) -> f32 {
        self.brightness
    }

    pub fn is_valid(&self) -> bool {
        self.valid
    }

    pub fn has_anchor(&self) -> bool {
        self.anchor != NO_USER_LUX
    }
}

/// Whether the learned model should be reset given the current ambient lux and
/// the anchor it was learned at.
pub fn should_reset_short_term_model(
    ambient_lux: f32,
    short_term_model_anchor: f32,
    min_threshold_ratio: f32,
    max_threshold_ratio: f32,
) -> bool {
    let min_ambient_lux = short_term_model_anchor - short_term_model_anchor * min_threshold_ratio;
    let max_ambient_lux = short_term_model_anchor + short_term_model_anchor * max_threshold_ratio;
    !(min_ambient_lux < ambient_lux && ambient_lux <= max_ambient_lux)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_clears_everything() {
        let mut m = ShortTermModel::new();
        m.set_user_brightness(100.0, 0.5);
        assert!(m.is_valid() && m.has_anchor());
        m.reset();
        assert!(!m.is_valid() && !m.has_anchor());
    }

    #[test]
    fn should_reset_outside_band() {
        // anchor 100 => band (40, 160]; avoid the exact boundaries because the
        // ratio multiplication is not exact in f32.
        assert!(should_reset_short_term_model(39.0, 100.0, 0.6, 0.6));
        assert!(!should_reset_short_term_model(41.0, 100.0, 0.6, 0.6));
        assert!(!should_reset_short_term_model(100.0, 100.0, 0.6, 0.6));
        assert!(!should_reset_short_term_model(160.0, 100.0, 0.6, 0.6));
        assert!(should_reset_short_term_model(161.0, 100.0, 0.6, 0.6));
    }
}
