//! Real-time parameter smoothing processors (Signalsmith Audio DSP model).
//!
//! # Overview
//!
//! Standard 1-pole exponential smoothers ($y[n] = y[n-1] + \alpha(x[n] - y[n-1])$)
//! exhibit discontinuous first derivatives ($C^0$ continuity) when the target
//! steps. This corner in the velocity curve produces audible high-frequency
//! click transients and zipper artifacts when dragging UI sliders or when
//! automation curves arrive.
//!
//! This module provides:
//! 1. [`CriticallyDampedSmoother`]: A 2-pole critically damped IIR filter
//!    ($H(s) = \frac{\omega_0^2}{(s + \omega_0)^2}$) with continuous velocity
//!    ($C^1$ continuity), exactly zero overshoot, and monotonic settling.
//! 2. [`PolynomialSmoother`]: A 5th-order Perlin smootherstep ($C^2$ continuous,
//!    zero initial and final velocity and acceleration) for block ramps.
//!
//! # Realtime Safety
//!
//! Both smoothers are fully zero-allocation on the audio thread.

use std::f64::consts::PI;

/// A 2-pole critically damped parameter smoother.
///
/// Features:
/// - Asymptotically reaches target with zero overshoot ($Q = 0.5$).
/// - Continuous first derivative (zero initial jerk/click).
/// - Exact unity DC gain.
#[derive(Debug, Clone, Copy)]
pub struct CriticallyDampedSmoother {
    target: f64,
    y1: f64,
    y2: f64,
    p: f64,
    b0: f64,
    response_ms: f64,
    sample_rate: f64,
}

impl Default for CriticallyDampedSmoother {
    fn default() -> Self {
        Self::new(0.0, 20.0, 44100.0)
    }
}

impl CriticallyDampedSmoother {
    /// Create a new critically damped smoother.
    ///
    /// - `initial`: Starting value.
    /// - `response_ms`: Time in milliseconds to reach ~95% of target (default 20.0 ms).
    /// - `sample_rate`: Stream sample rate in Hz.
    pub fn new(initial: f64, response_ms: f64, sample_rate: f64) -> Self {
        let sr = if sample_rate <= 0.0 || !sample_rate.is_finite() {
            44100.0
        } else {
            sample_rate
        };
        let mut s = Self {
            target: initial,
            y1: initial,
            y2: initial,
            p: 0.0,
            b0: 1.0,
            response_ms: response_ms.max(0.1),
            sample_rate: sr,
        };
        s.recompute_coefficients();
        s
    }

    fn recompute_coefficients(&mut self) {
        let resp_sec = (self.response_ms * 0.001).max(0.0001);
        // omega_0 = 2 * PI / resp_sec, pole p = exp(-omega_0 / fs)
        let omega = 2.0 * PI / resp_sec;
        let p = (-omega / self.sample_rate).exp().clamp(0.0, 0.99999);
        self.p = p;
        // H(z) = (1 - p)^2 / (1 - 2p z^-1 + p^2 z^-2)
        // b0 = (1 - p)^2 guarantees exact unity DC gain
        self.b0 = (1.0 - p) * (1.0 - p);
    }

    /// Set the target destination value.
    #[inline]
    pub fn set_target(&mut self, target: f64) {
        self.target = target;
    }

    /// Update the response time in milliseconds.
    pub fn set_response_ms(&mut self, response_ms: f64) {
        self.response_ms = response_ms.max(0.1);
        self.recompute_coefficients();
    }

    /// Update the sample rate.
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        if sample_rate > 0.0 && sample_rate.is_finite() {
            self.sample_rate = sample_rate;
            self.recompute_coefficients();
        }
    }

    /// Advance one sample and return the smoothed value.
    #[inline]
    pub fn next_value(&mut self) -> f64 {
        // y[n] = 2p * y[n-1] - p^2 * y[n-2] + b0 * x[n]
        let out = 2.0 * self.p * self.y1 - (self.p * self.p) * self.y2 + self.b0 * self.target;
        self.y2 = self.y1;
        self.y1 = out;

        // Snap when settled within machine precision
        if (self.target - self.y1).abs() < 1e-7 && (self.y1 - self.y2).abs() < 1e-7 {
            self.y1 = self.target;
            self.y2 = self.target;
        }

        self.y1
    }

    /// Current smoothed output value.
    #[inline]
    pub fn current_value(&self) -> f64 {
        self.y1
    }

    /// Target destination value.
    #[inline]
    pub fn target(&self) -> f64 {
        self.target
    }

    /// Whether the smoother has fully settled at its target.
    #[inline]
    pub fn is_settled(&self) -> bool {
        (self.target - self.y1).abs() < 1e-7 && (self.y1 - self.y2).abs() < 1e-7
    }

    /// Instantly snap both state and target to the given value (bypass smoothing).
    pub fn reset(&mut self, value: f64) {
        self.target = value;
        self.y1 = value;
        self.y2 = value;
    }
}

/// 5th-order Perlin smootherstep ramp for finite duration parameter fades ($C^2$ continuous).
///
/// $s(t) = 6t^5 - 15t^4 + 10t^3$
///
/// Both velocity ($s'(t)$) and acceleration ($s''(t)$) are zero at $t = 0$ and $t = 1$,
/// completely suppressing spectral leakage and clicking.
#[derive(Debug, Clone, Copy)]
pub struct PolynomialSmoother {
    start: f64,
    target: f64,
    current_frame: usize,
    total_frames: usize,
}

impl PolynomialSmoother {
    /// Create a new polynomial smoother over `total_frames`.
    pub fn new(start: f64, target: f64, total_frames: usize) -> Self {
        Self {
            start,
            target,
            current_frame: 0,
            total_frames: total_frames.max(1),
        }
    }

    /// Advance one frame and return the interpolated value.
    #[inline]
    pub fn next_value(&mut self) -> f64 {
        if self.current_frame >= self.total_frames {
            return self.target;
        }
        let t = self.current_frame as f64 / self.total_frames as f64;
        self.current_frame += 1;

        // Perlin smootherstep: 6t^5 - 15t^4 + 10t^3 = t^3 * (t * (t * 6.0 - 15.0) + 10.0)
        let s = t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
        self.start + (self.target - self.start) * s
    }

    /// Whether the polynomial ramp has reached its destination.
    #[inline]
    pub fn is_finished(&self) -> bool {
        self.current_frame >= self.total_frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_critically_damped_smoother_monotonic_and_no_overshoot() {
        let mut s = CriticallyDampedSmoother::new(0.0, 10.0, 48000.0);
        s.set_target(1.0);

        let mut prev = 0.0;
        for _ in 0..4800 {
            let val = s.next_value();
            assert!(
                val >= prev - 1e-12,
                "critically damped step response must be monotonic non-decreasing, got val={val} < prev={prev}"
            );
            assert!(
                val <= 1.0 + 1e-7,
                "critically damped filter must have zero overshoot, got {val}"
            );
            prev = val;
        }
        assert!((s.current_value() - 1.0).abs() < 1e-4);
        assert!(s.is_settled());
    }

    #[test]
    fn test_critically_damped_continuous_velocity() {
        let mut s = CriticallyDampedSmoother::new(0.0, 20.0, 48000.0);
        s.set_target(1.0);

        // Compute numerical first differences (velocity)
        let mut max_acceleration = 0.0f64;
        let mut prev_val = 0.0f64;
        let mut prev_vel = 0.0f64;

        for i in 0..1000 {
            let val = s.next_value();
            let vel = val - prev_val;
            if i > 1 {
                let accel = (vel - prev_vel).abs();
                max_acceleration = max_acceleration.max(accel);
            }
            prev_val = val;
            prev_vel = vel;
        }

        // Acceleration should be smooth and very small (no delta-function impulses)
        assert!(
            max_acceleration < 0.01,
            "acceleration must be continuous and bounded, got {max_acceleration}"
        );
    }

    #[test]
    fn test_polynomial_smootherstep_endpoints() {
        let mut poly = PolynomialSmoother::new(10.0, 20.0, 100);
        assert_eq!(poly.next_value(), 10.0);

        for _ in 1..99 {
            poly.next_value();
        }
        let end_val = poly.next_value();
        assert!((end_val - 20.0).abs() < 1e-4);
        assert!(poly.is_finished());
        assert_eq!(poly.next_value(), 20.0);
    }
}
