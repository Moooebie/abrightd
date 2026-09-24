//! Deterministic replay tests (no hardware required).

use abrightd::als::replay::Replay;
use abrightd::config::Config;
use abrightd::controller::AutomaticBrightnessController;
use abrightd::daemon::{self, ReplayPoint};
use abrightd::ramp::Ramp;

fn run() -> Vec<ReplayPoint> {
    let config = Config::default();
    let mapper = Box::new(config.mapper().unwrap());
    let mut controller =
        AutomaticBrightnessController::new(config.controller_config().unwrap(), mapper);
    let mut ramp = Ramp::new(config.ramp_config());
    let replay = Replay::from_path(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/replay_trace.csv")
            .as_path(),
    )
    .unwrap();
    daemon::replay(&mut controller, &mut ramp, &replay.into_vec())
}

/// Brightness at the last point in a time range.
fn output_at(points: &[ReplayPoint], t0: i64, t1: i64) -> f32 {
    points
        .iter()
        .filter(|p| p.time_ms >= t0 && p.time_ms <= t1 && !p.output.is_nan())
        .next_back()
        .map(|p| p.output)
        .unwrap_or(f32::NAN)
}

#[test]
fn replay_is_deterministic() {
    let a = run();
    let b = run();
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(b.iter()) {
        assert_eq!(x.time_ms, y.time_ms);
        assert_eq!(x.output.to_bits(), y.output.to_bits());
    }
}

#[test]
fn bright_step_brightens_and_dark_step_darkens() {
    let points = run();
    let dark = output_at(&points, 8_000, 10_000);
    let bright = output_at(&points, 18_000, 20_000);
    assert!(bright > dark, "bright {bright} !> dark {dark}");

    let office = output_at(&points, 28_000, 30_000);
    assert!(office < bright, "office {office} !< bright {bright}");
}

#[test]
fn constant_light_does_not_oscillate() {
    let points = run();
    // The trace is constant 10 lux for the first 10 s.
    let mut values: Vec<u32> = points
        .iter()
        .filter(|p| p.time_ms < 10_000 && !p.controller_brightness.is_nan())
        .map(|p| p.controller_brightness.to_bits())
        .collect();
    values.dedup();
    assert!(
        values.len() <= 2,
        "oscillated in constant light: {} values",
        values.len()
    );
}

#[test]
fn output_is_always_in_range() {
    for p in run() {
        if !p.output.is_nan() {
            assert!(
                (0.0..=1.0).contains(&p.output),
                "out of range: {}",
                p.output
            );
        }
    }
}
