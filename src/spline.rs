//! Port of AOSP `android.util.Spline`.
//!
//! Original: `frameworks/base/core/java/android/util/Spline.java`
//! Copyright (C) 2012 The Android Open Source Project — Apache-2.0.
//!
//! Boundary handling is reproduced exactly: `x <= x[0]` returns `y[0]`,
//! `x >= x[n-1]` returns `y[n-1]`, and `NaN` passes through.

use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum SplineError {
    #[error("There must be at least two control points and the arrays must be of equal length.")]
    BadControlPoints,
    #[error("The control points must all have strictly increasing X values.")]
    NotStrictlyIncreasing,
    #[error("The control points must have monotonic Y values.")]
    NotMonotonic,
}

/// Performs spline interpolation given a set of control points.
pub trait Spline: Send + Sync {
    /// Interpolate `Y = f(X)`.  Clamps `X` to the domain of the spline.
    fn interpolate(&self, x: f32) -> f32;
}

/// Choose a spline based on the properties of the control points.
///
/// If the control points are monotonic the resulting spline preserves that,
/// otherwise a linear spline is used.
pub fn create_spline(x: &[f32], y: &[f32]) -> Result<Box<dyn Spline>, SplineError> {
    if !is_strictly_increasing(x)? {
        return Err(SplineError::NotStrictlyIncreasing);
    }
    if is_monotonic(y)? {
        Ok(Box::new(MonotoneCubicSpline::new(x, y)?))
    } else {
        Ok(Box::new(LinearSpline::new(x, y)?))
    }
}

fn is_strictly_increasing(x: &[f32]) -> Result<bool, SplineError> {
    if x.len() < 2 {
        return Err(SplineError::BadControlPoints);
    }
    let mut prev = x[0];
    for &curr in &x[1..] {
        if curr <= prev {
            return Ok(false);
        }
        prev = curr;
    }
    Ok(true)
}

fn is_monotonic(y: &[f32]) -> Result<bool, SplineError> {
    if y.len() < 2 {
        return Err(SplineError::BadControlPoints);
    }
    let mut prev = y[0];
    for &curr in &y[1..] {
        if curr < prev {
            return Ok(false);
        }
        prev = curr;
    }
    Ok(true)
}

/// Fritsch–Carlson monotone cubic Hermite spline.
pub struct MonotoneCubicSpline {
    x: Vec<f32>,
    y: Vec<f32>,
    m: Vec<f32>,
}

impl MonotoneCubicSpline {
    pub fn new(x: &[f32], y: &[f32]) -> Result<Self, SplineError> {
        if x.len() != y.len() || x.len() < 2 {
            return Err(SplineError::BadControlPoints);
        }
        let n = x.len();
        let mut d = vec![0.0f32; n - 1];
        let mut m = vec![0.0f32; n];

        // Slopes of the secant lines between successive points.
        for i in 0..n - 1 {
            let h = x[i + 1] - x[i];
            if h <= 0.0 {
                return Err(SplineError::NotStrictlyIncreasing);
            }
            d[i] = (y[i + 1] - y[i]) / h;
        }

        // Initialise the tangents as the average of the secants.
        m[0] = d[0];
        for i in 1..n - 1 {
            m[i] = (d[i - 1] + d[i]) * 0.5;
        }
        m[n - 1] = d[n - 2];

        // Update the tangents to preserve monotonicity.
        for i in 0..n - 1 {
            if d[i] == 0.0 {
                m[i] = 0.0;
                m[i + 1] = 0.0;
            } else {
                let a = m[i] / d[i];
                let b = m[i + 1] / d[i];
                if a < 0.0 || b < 0.0 {
                    return Err(SplineError::NotMonotonic);
                }
                let h = a.hypot(b);
                if h > 3.0 {
                    let t = 3.0 / h;
                    m[i] *= t;
                    m[i + 1] *= t;
                }
            }
        }

        Ok(Self {
            x: x.to_vec(),
            y: y.to_vec(),
            m,
        })
    }
}

impl Spline for MonotoneCubicSpline {
    fn interpolate(&self, x: f32) -> f32 {
        let n = self.x.len();
        if x.is_nan() {
            return x;
        }
        if x <= self.x[0] {
            return self.y[0];
        }
        if x >= self.x[n - 1] {
            return self.y[n - 1];
        }

        let mut i = 0usize;
        while x >= self.x[i + 1] {
            i += 1;
            if x == self.x[i] {
                return self.y[i];
            }
        }

        let h = self.x[i + 1] - self.x[i];
        let t = (x - self.x[i]) / h;
        (self.y[i] * (1.0 + 2.0 * t) + h * self.m[i] * t) * (1.0 - t) * (1.0 - t)
            + (self.y[i + 1] * (3.0 - 2.0 * t) + h * self.m[i + 1] * (t - 1.0)) * t * t
    }
}

/// Piecewise-linear interpolation.
pub struct LinearSpline {
    x: Vec<f32>,
    y: Vec<f32>,
    m: Vec<f32>,
}

impl LinearSpline {
    pub fn new(x: &[f32], y: &[f32]) -> Result<Self, SplineError> {
        if x.len() != y.len() || x.len() < 2 {
            return Err(SplineError::BadControlPoints);
        }
        let n = x.len();
        let mut m = vec![0.0f32; n - 1];
        for i in 0..n - 1 {
            m[i] = (y[i + 1] - y[i]) / (x[i + 1] - x[i]);
        }
        Ok(Self {
            x: x.to_vec(),
            y: y.to_vec(),
            m,
        })
    }
}

impl Spline for LinearSpline {
    fn interpolate(&self, x: f32) -> f32 {
        let n = self.x.len();
        if x.is_nan() {
            return x;
        }
        if x <= self.x[0] {
            return self.y[0];
        }
        if x >= self.x[n - 1] {
            return self.y[n - 1];
        }

        let mut i = 0usize;
        while x >= self.x[i + 1] {
            i += 1;
            if x == self.x[i] {
                return self.y[i];
            }
        }
        self.y[i] + self.m[i] * (x - self.x[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_boundary_and_interior() {
        let s = LinearSpline::new(&[0.0, 10.0, 20.0], &[0.0, 1.0, 3.0]).unwrap();
        assert_eq!(s.interpolate(-5.0), 0.0);
        assert_eq!(s.interpolate(0.0), 0.0);
        assert_eq!(s.interpolate(5.0), 0.5);
        assert_eq!(s.interpolate(15.0), 2.0);
        assert_eq!(s.interpolate(25.0), 3.0);
        assert!(s.interpolate(f32::NAN).is_nan());
    }

    #[test]
    fn monotone_passes_through_control_points() {
        let x = [0.0, 1.0, 2.0, 5.0, 10.0];
        let y = [0.0, 0.5, 0.8, 0.95, 1.0];
        let s = create_spline(&x, &y).unwrap();
        for (xi, yi) in x.iter().zip(y.iter()) {
            assert!((s.interpolate(*xi) - yi).abs() < 1e-5, "at {xi}");
        }
    }

    #[test]
    fn non_monotonic_y_falls_back_to_linear() {
        let s = create_spline(&[0.0, 1.0, 2.0], &[0.0, 1.0, 0.5]).unwrap();
        // Linear behaviour: midpoint is the average of the two endpoints.
        assert!((s.interpolate(0.5) - 0.5).abs() < 1e-6);
        assert!((s.interpolate(1.5) - 0.75).abs() < 1e-6);
    }

    #[test]
    fn rejects_non_increasing_x() {
        assert_eq!(
            create_spline(&[0.0, 0.0], &[0.0, 1.0]).err(),
            Some(SplineError::NotStrictlyIncreasing)
        );
        assert_eq!(
            create_spline(&[0.0], &[0.0]).err(),
            Some(SplineError::BadControlPoints)
        );
    }
}
