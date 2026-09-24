//! Shared status snapshot and control commands exchanged with D-Bus.

use std::collections::HashMap;
use std::collections::VecDeque;

/// Commands queued by the D-Bus service and applied by the daemon loop.
#[derive(Debug, Clone)]
pub enum Command {
    Enable(bool),
    AddUserPoint { lux: f32, brightness: f32 },
    ClearUserPoints,
    SetProfile(String),
    SetAdjustment(f32),
}

/// A snapshot of the pipeline, updated by the daemon after every step.
#[derive(Debug, Clone)]
pub struct SharedState {
    pub enabled: bool,
    pub profile: String,
    pub lux: f32,
    pub last_observed_lux: f32,
    pub slow_lux: f32,
    pub fast_lux: f32,
    pub ambient_brightening_threshold: f32,
    pub ambient_darkening_threshold: f32,
    pub screen_brightening_threshold: f32,
    pub screen_darkening_threshold: f32,
    pub controller_brightness: f32,
    pub output_brightness: f32,
    pub adjustment: f32,
    pub user_points: Vec<(f32, f32)>,
    pub commands: VecDeque<Command>,
}

impl Default for SharedState {
    fn default() -> Self {
        Self {
            enabled: true,
            profile: "default".into(),
            lux: f32::NAN,
            last_observed_lux: f32::NAN,
            slow_lux: f32::NAN,
            fast_lux: f32::NAN,
            ambient_brightening_threshold: 0.0,
            ambient_darkening_threshold: 0.0,
            screen_brightening_threshold: 0.0,
            screen_darkening_threshold: 0.0,
            controller_brightness: f32::NAN,
            output_brightness: f32::NAN,
            adjustment: 0.0,
            user_points: Vec::new(),
            commands: VecDeque::new(),
        }
    }
}

impl SharedState {
    /// Status as a string map, suitable for the `Status` D-Bus method.
    pub fn as_map(&self) -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("enabled".into(), self.enabled.to_string());
        m.insert("profile".into(), self.profile.clone());
        m.insert("lux".into(), fmt(self.lux));
        m.insert("last_observed_lux".into(), fmt(self.last_observed_lux));
        m.insert("slow_lux".into(), fmt(self.slow_lux));
        m.insert("fast_lux".into(), fmt(self.fast_lux));
        m.insert(
            "ambient_brightening_threshold".into(),
            fmt(self.ambient_brightening_threshold),
        );
        m.insert(
            "ambient_darkening_threshold".into(),
            fmt(self.ambient_darkening_threshold),
        );
        m.insert(
            "screen_brightening_threshold".into(),
            fmt(self.screen_brightening_threshold),
        );
        m.insert(
            "screen_darkening_threshold".into(),
            fmt(self.screen_darkening_threshold),
        );
        m.insert(
            "controller_brightness".into(),
            fmt(self.controller_brightness),
        );
        m.insert("output_brightness".into(), fmt(self.output_brightness));
        m.insert("adjustment".into(), fmt(self.adjustment));
        m.insert(
            "user_points".into(),
            self.user_points
                .iter()
                .map(|(lux, b)| format!("{lux:.3}:{b:.4}"))
                .collect::<Vec<_>>()
                .join(";"),
        );
        m
    }
}

fn fmt(v: f32) -> String {
    if v.is_nan() {
        "nan".into()
    } else {
        format!("{v:.6}")
    }
}
