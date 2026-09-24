//! TOML configuration.
//!
//! See `examples/abrightd.toml` for a fully commented profile.

use std::path::Path;

use serde::Deserialize;

use crate::controller::ControllerConfig;
use crate::hysteresis::HysteresisLevels;
use crate::mapping::SimpleMappingStrategy;
use crate::ramp::RampConfig;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub als: AlsConfig,
    pub output: OutputConfig,
    pub ramp: RampSection,
    pub curve: CurveConfig,
    pub timing: TimingConfig,
    pub hysteresis: HysteresisSection,
    pub learning: LearningConfig,
    pub integration: IntegrationConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct IntegrationConfig {
    /// Treat desktop brightness changes (keys/slider) as user intent.
    pub watch_user_changes: bool,
    /// Disable the sensor while the session is locked.
    pub pause_when_locked: bool,
    /// Disable the sensor while the machine is suspended.
    pub pause_on_suspend: bool,
}

impl Default for IntegrationConfig {
    fn default() -> Self {
        Self {
            watch_user_changes: true,
            pause_when_locked: true,
            pause_on_suspend: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AlsConfig {
    /// `"iio"` or `"replay"`.
    pub kind: String,
    /// IIO device name (e.g. `iio:device0`) or path; `None` auto-discovers.
    pub device: Option<String>,
    /// Replay CSV path when `kind = "replay"`.
    pub replay_path: Option<String>,
    /// Poll period for sysfs reads.
    pub poll_rate_ms: i64,
    /// Per-device calibration multiplier applied to computed lux.
    pub lux_multiplier: f32,
}

impl Default for AlsConfig {
    fn default() -> Self {
        Self {
            kind: "iio".into(),
            device: None,
            replay_path: None,
            poll_rate_ms: 200,
            lux_multiplier: 1.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct OutputConfig {
    /// `"logind"` or `"sysfs"`.
    pub kind: String,
    /// Backlight device name (e.g. `intel_backlight`); `None` auto-discovers.
    pub device: Option<String>,
    /// `"linear"` or `"perceptual"`.
    pub gamma: String,
    /// Minimum normalized brightness to ever command.
    pub min: f32,
    /// Maximum normalized brightness to ever command.
    pub max: f32,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            kind: "logind".into(),
            device: None,
            gamma: "linear".into(),
            min: 0.0,
            max: 1.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RampSection {
    pub brighten_step: f32,
    pub darken_step: f32,
    pub min_interval_ms: i64,
}

impl Default for RampSection {
    fn default() -> Self {
        Self {
            brighten_step: 0.05,
            darken_step: 0.05,
            min_interval_ms: 100,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CurveConfig {
    pub max_gamma: f32,
    pub lux: Vec<f32>,
    pub bri: Vec<f32>,
}

impl Default for CurveConfig {
    fn default() -> Self {
        Self {
            max_gamma: 3.0,
            lux: vec![
                0.0, 10.0, 50.0, 100.0, 500.0, 1000.0, 5000.0, 10_000.0, 100_000.0,
            ],
            bri: vec![0.03, 0.06, 0.12, 0.2, 0.35, 0.5, 0.7, 0.85, 1.0],
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TimingConfig {
    pub horizon_long_ms: i64,
    pub horizon_short_ms: i64,
    pub sensor_rate_ms: i64,
    pub initial_sensor_rate_ms: i64,
    pub warmup_ms: i64,
    pub brightening_debounce_ms: i64,
    pub darkening_debounce_ms: i64,
    pub reset_ambient_lux_after_warm_up: bool,
    pub doze_scale_factor: f32,
}

impl Default for TimingConfig {
    fn default() -> Self {
        Self {
            horizon_long_ms: 10_000,
            horizon_short_ms: 2_000,
            sensor_rate_ms: 200,
            initial_sensor_rate_ms: 1000,
            warmup_ms: 0,
            brightening_debounce_ms: 2000,
            darkening_debounce_ms: 4000,
            reset_ambient_lux_after_warm_up: true,
            doze_scale_factor: 1.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct HysteresisPairConfig {
    pub percentages: Vec<f32>,
    pub levels: Vec<f32>,
    pub min: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct HysteresisSection {
    pub ambient_brightening: HysteresisPairConfig,
    pub ambient_darkening: HysteresisPairConfig,
    pub screen_brightening: HysteresisPairConfig,
    pub screen_darkening: HysteresisPairConfig,
}

impl Default for HysteresisPairConfig {
    fn default() -> Self {
        Self {
            percentages: vec![0.0],
            levels: vec![],
            min: 0.0,
        }
    }
}

impl Default for HysteresisSection {
    fn default() -> Self {
        // AOSP fallback (config.xml): single level 0, 10% brightening / 20%
        // darkening, no minimum.  Overridable per device in TOML.
        Self {
            ambient_brightening: HysteresisPairConfig {
                percentages: vec![0.10],
                levels: vec![0.0],
                min: 0.0,
            },
            ambient_darkening: HysteresisPairConfig {
                percentages: vec![0.20],
                levels: vec![0.0],
                min: 0.0,
            },
            screen_brightening: HysteresisPairConfig {
                percentages: vec![0.10],
                levels: vec![0.0],
                min: 0.0,
            },
            screen_darkening: HysteresisPairConfig {
                percentages: vec![0.20],
                levels: vec![0.0],
                min: 0.0,
            },
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LearningConfig {
    pub enabled: bool,
    pub short_term_timeout_ms: u64,
    pub short_term_threshold_ratio: f32,
}

impl Default for LearningConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            short_term_timeout_ms: 1_800_000,
            short_term_threshold_ratio: 0.6,
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&text)?;
        Ok(config)
    }

    pub fn from_toml(text: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(text)?)
    }

    pub fn mapper(&self) -> anyhow::Result<SimpleMappingStrategy> {
        SimpleMappingStrategy::new(
            &self.curve.lux,
            &self.curve.bri,
            self.curve.max_gamma,
            self.learning.short_term_timeout_ms,
        )
    }

    pub fn controller_config(&self) -> anyhow::Result<ControllerConfig> {
        let ambient = HysteresisLevels::new(
            &self.hysteresis.ambient_brightening.percentages,
            &self.hysteresis.ambient_darkening.percentages,
            &self.hysteresis.ambient_brightening.levels,
            &self.hysteresis.ambient_darkening.levels,
            self.hysteresis.ambient_darkening.min,
            self.hysteresis.ambient_brightening.min,
        )?;
        let screen = HysteresisLevels::new(
            &self.hysteresis.screen_brightening.percentages,
            &self.hysteresis.screen_darkening.percentages,
            &self.hysteresis.screen_brightening.levels,
            &self.hysteresis.screen_darkening.levels,
            self.hysteresis.screen_darkening.min,
            self.hysteresis.screen_brightening.min,
        )?;
        Ok(ControllerConfig {
            screen_brightness_range_minimum: self.output.min,
            screen_brightness_range_maximum: self.output.max,
            light_sensor_warm_up_time_ms: self.timing.warmup_ms,
            doze_scale_factor: self.timing.doze_scale_factor,
            normal_light_sensor_rate_ms: self.timing.sensor_rate_ms,
            initial_light_sensor_rate_ms: self.timing.initial_sensor_rate_ms,
            brightening_light_debounce_ms: self.timing.brightening_debounce_ms,
            darkening_light_debounce_ms: self.timing.darkening_debounce_ms,
            reset_ambient_lux_after_warm_up: self.timing.reset_ambient_lux_after_warm_up,
            ambient_light_horizon_long_ms: self.timing.horizon_long_ms,
            ambient_light_horizon_short_ms: self.timing.horizon_short_ms,
            ambient_thresholds: ambient,
            screen_thresholds: screen,
        })
    }

    pub fn ramp_config(&self) -> RampConfig {
        RampConfig {
            brighten_step: self.ramp.brighten_step,
            darken_step: self.ramp.darken_step,
            min_interval_ms: self.ramp.min_interval_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        let c = Config::default();
        c.mapper().unwrap();
        c.controller_config().unwrap();
    }

    #[test]
    fn parses_plan_example() {
        let toml = r#"
            [als]
            kind = "iio"
            device = "iio:device0"

            [output]
            kind = "logind"
            device = "intel_backlight"
            gamma = "linear"

            [curve]
            max_gamma = 3.0
            lux = [0, 10, 50, 100, 500, 1000, 5000, 10000, 100000]
            bri = [0.03, 0.06, 0.12, 0.2, 0.35, 0.5, 0.7, 0.85, 1.0]

            [timing]
            horizon_long_ms = 10000
            horizon_short_ms = 2000
            sensor_rate_ms = 200
            initial_sensor_rate_ms = 1000
            warmup_ms = 0
            brightening_debounce_ms = 2000
            darkening_debounce_ms = 4000

            [learning]
            enabled = true
            short_term_timeout_ms = 1800000
        "#;
        let c = Config::from_toml(toml).unwrap();
        assert_eq!(c.als.kind, "iio");
        assert_eq!(c.curve.lux.len(), 9);
        assert_eq!(c.timing.sensor_rate_ms, 200);
        c.controller_config().unwrap();
    }
}
