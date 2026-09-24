//! Golden vectors ported from the behaviour of the AOSP Java implementations.

use abrightd::hysteresis::HysteresisLevels;
use abrightd::mapping::{
    infer_auto_brightness_adjustment, BrightnessMappingStrategy, SimpleMappingStrategy,
};
use abrightd::ring_buffer::{weight_integral, AmbientLightRingBuffer};
use abrightd::spline::{MonotoneCubicSpline, Spline};

fn approx(a: f32, b: f32, eps: f32) -> bool {
    (a - b).abs() < eps
}

#[test]
fn spline_boundary_handling() {
    let s = MonotoneCubicSpline::new(&[0.0, 10.0, 20.0], &[0.0, 1.0, 3.0]).unwrap();
    assert!(s.interpolate(f32::NAN).is_nan());
    assert_eq!(s.interpolate(-1.0), 0.0);
    assert_eq!(s.interpolate(0.0), 0.0);
    assert_eq!(s.interpolate(20.0), 3.0);
    assert_eq!(s.interpolate(100.0), 3.0);
}

#[test]
fn spline_monotone_through_points() {
    let x = [0.0, 1.0, 2.0, 5.0, 10.0];
    let y = [0.0, 2.0, 3.0, 3.5, 4.0];
    let s = MonotoneCubicSpline::new(&x, &y).unwrap();
    for (xi, yi) in x.iter().zip(y.iter()) {
        assert!(approx(s.interpolate(*xi), *yi, 1e-5));
    }
}

#[test]
fn weight_integral_matches_formula() {
    // x * (x * 0.5 + intercept)
    assert!(approx(
        weight_integral(100, 10000),
        100.0 * (50.0 + 10000.0),
        1e-2
    ));
    assert!(approx(weight_integral(0, 10000), 0.0, 1e-6));
}

#[test]
fn ring_buffer_weighted_golden() {
    let mut b = AmbientLightRingBuffer::new();
    b.push(0, 10.0);
    b.push(5000, 20.0);
    // now=5000, horizon=2000 => horizon start 3000.
    // weight(0..100)=100*(50+2000)=205000 ; weight(-2000..0)=2_000_000
    let expected = (20.0 * 205_000.0 + 10.0 * 2_000_000.0) / (205_000.0 + 2_000_000.0);
    let got = b.calculate_ambient_lux(5000, 2000).unwrap();
    assert!(
        approx(got, expected, 1e-3),
        "got {got}, expected {expected}"
    );
}

#[test]
fn hysteresis_defaults_bracket() {
    let h = HysteresisLevels::default_ambient();
    for v in [1.0f32, 10.0, 100.0, 200.0, 500.0, 50_000.0] {
        assert!(h.brightening_threshold(v) > v);
        assert!(h.darkening_threshold(v) < v);
    }
}

#[test]
fn infer_adjustment_golden() {
    // current=0.35, desired=0.42, max_gamma=3
    // gamma = ln(0.42)/ln(0.35) = 0.82633...
    // adjustment = -ln(gamma)/ln(3) = 0.17364...
    let a = infer_auto_brightness_adjustment(3.0, 0.42, 0.35);
    assert!(approx(a, 0.17364, 1e-3), "got {a}");
}

#[test]
fn simple_mapping_golden_points() {
    let m = SimpleMappingStrategy::new(
        &[0.0, 10.0, 100.0, 1000.0, 10000.0],
        &[0.05, 0.1, 0.3, 0.6, 1.0],
        3.0,
        1_800_000,
    )
    .unwrap();
    // Monotone cubic passes exactly through control points.
    for (lux, b) in [
        (0.0, 0.05),
        (10.0, 0.1),
        (100.0, 0.3),
        (1000.0, 0.6),
        (10000.0, 1.0),
    ] {
        assert!(approx(m.get_brightness(lux), b, 1e-5), "lux {lux}");
    }
    // Clamped outside the domain.
    assert!(approx(m.get_brightness(-5.0), 0.05, 1e-6));
    assert!(approx(m.get_brightness(1e9), 1.0, 1e-6));
}
