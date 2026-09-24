//! Property tests for the pure core.

use abrightd::controller::{AutomaticBrightnessController, ControllerConfig};
use abrightd::hysteresis::HysteresisLevels;
use abrightd::mapping::{BrightnessMappingStrategy, SimpleMappingStrategy};
use abrightd::spline::create_spline;
use proptest::prelude::*;

proptest! {
    /// A strictly increasing set of control points yields a spline that passes
    /// through those points and stays monotone between them.
    #[test]
    fn spline_is_monotone_and_continuous(
        pairs in prop::collection::vec((0u32..1000u32, 0u32..1000u32), 2..8),
    ) {
        let mut x: Vec<f32> = pairs.iter().map(|(a, _)| *a as f32).collect();
        x.sort_by(|a, b| a.partial_cmp(b).unwrap());
        x.dedup();
        prop_assume!(x.len() >= 2);
        // Keep y the same length as the deduplicated x, monotone non-decreasing.
        let mut y: Vec<f32> = pairs.iter().take(x.len()).map(|(_, b)| *b as f32).collect();
        y.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let spline = create_spline(&x, &y).unwrap();
        // Passes through control points.
        for (xi, yi) in x.iter().zip(y.iter()) {
            prop_assert!((spline.interpolate(*xi) - yi).abs() < 1e-3);
        }
        // Monotone across a fine grid.
        let lo = x[0];
        let hi = x[x.len() - 1];
        let steps = 200;
        let mut prev = f32::NEG_INFINITY;
        for i in 0..=steps {
            let t = lo + (hi - lo) * (i as f32) / (steps as f32);
            let v = spline.interpolate(t);
            prop_assert!(v + 1e-3 >= prev, "not monotone at {t}: {v} < {prev}");
            prev = v;
        }
    }

    /// Hysteresis thresholds always bracket a positive value.
    #[test]
    fn hysteresis_brackets(v in 0.001f32..100000.0) {
        let h = HysteresisLevels::default_ambient();
        prop_assert!(h.brightening_threshold(v) > v);
        prop_assert!(h.darkening_threshold(v) <= v);
        prop_assert!(h.darkening_threshold(v) >= 0.0);
    }

    /// A learned user point is reproduced exactly on the next query.
    #[test]
    fn user_point_is_exact(lux in 1.0f32..50000.0, desired in 0.05f32..0.95) {
        let mut m = SimpleMappingStrategy::default_curve();
        m.add_user_data_point(lux, desired);
        let got = m.get_brightness(lux);
        prop_assert!((got - desired).abs() < 1e-4, "lux={lux} desired={desired} got={got}");
    }

    /// Constant ambient light never causes repeated brightness changes.
    #[test]
    fn constant_lux_does_not_oscillate(lux in 1.0f32..50000.0) {
        let mapper = Box::new(SimpleMappingStrategy::default_curve());
        let mut c = AutomaticBrightnessController::new(ControllerConfig::default(), mapper);
        c.set_light_sensor_enabled(true, 0);
        let mut emitted = 0;
        for t in 0..80i64 {
            let out = c.handle_light_sensor_event(t * 200, lux);
            if out.brightness.is_some() {
                emitted += 1;
            }
        }
        prop_assert!(emitted <= 2, "emitted {emitted} times at lux {lux}");
    }
}
