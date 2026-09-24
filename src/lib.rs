//! `abrightd` — a Rust userspace port of Android's automatic brightness pipeline.
//!
//! The crate is split into a pure, I/O-free core (the algorithm) and thin I/O
//! adapters.  The core is modelled on AOSP's
//! `frameworks/base/services/core/java/com/android/server/display/` package:
//!
//! * [`spline`] — monotone cubic / linear interpolation (AOSP `android.util.Spline`)
//! * [`hysteresis`] — `HysteresisLevels`
//! * [`ring_buffer`] — `AmbientLightRingBuffer` and the weighted lux integral
//! * [`mapping`] — `BrightnessMappingStrategy.SimpleMappingStrategy`
//! * [`short_term`] — `AutomaticBrightnessController.ShortTermModel`
//! * [`controller`] — `AutomaticBrightnessController`
//!
//! Everything in the core takes time as an explicit millisecond argument so it
//! can be driven deterministically from tests and the replay harness.

pub mod als;
pub mod clock;
pub mod config;
pub mod controller;
pub mod daemon;
pub mod desktop;
pub mod hysteresis;
pub mod mapping;
pub mod output;
pub mod ramp;
pub mod ring_buffer;
pub mod short_term;
pub mod spline;
pub mod state;
pub mod status;

#[cfg(feature = "dbus")]
pub mod dbus;

#[cfg(feature = "tui")]
pub mod tui;

/// AOSP uses `-1` as the "no user data point" sentinel for lux.
pub const NO_USER_LUX: f32 = -1.0;
/// AOSP uses `-1` as the "no user data point" sentinel for brightness.
pub const NO_USER_BRIGHTNESS: f32 = -1.0;

/// Convert a normalized brightness `[0, 1]` to an absolute backlight integer.
pub fn brightness_to_raw(fraction: f32, max_raw: u32) -> u32 {
    let clamped = fraction.clamp(0.0, 1.0);
    (clamped * max_raw as f32).round() as u32
}

/// Convert an absolute backlight integer to a normalized brightness `[0, 1]`.
pub fn raw_to_brightness(raw: u32, max_raw: u32) -> f32 {
    if max_raw == 0 {
        return 0.0;
    }
    raw as f32 / max_raw as f32
}
