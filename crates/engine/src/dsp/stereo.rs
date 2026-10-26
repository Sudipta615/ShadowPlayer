//! Phase-safe stereo width enhancer
//!
//! Uses mid/side (M/S) processing to adjust stereo width without
//! introducing phase problems. Mono signals remain mono regardless
//! of width setting (phase-safe guarantee).
//!
//! Disabled by default — the user must opt in.

/// Stereo width enhancer
///
/// Operates on the mid/side representation:
/// ```text
/// mid  = (L + R) / 2    (mono component)
/// side = (L - R) / 2    (stereo component)
/// L' = mid + side * width
/// R' = mid - side * width
/// ```
///
/// - `width = 0.0`: mono (side eliminated)
/// - `width = 1.0`: passthrough (no change)
/// - `width > 1.0`: enhanced stereo (side boosted)
/// - `width = 2.0`: maximum safe widening
#[derive(Debug, Clone)]
pub struct StereoEnhancer {
    width: f32,
    current_width: f32,
    slew_rate: f32,
    enabled: bool,
}

impl StereoEnhancer {
    /// Create a new stereo enhancer (disabled, width = 1.0)
    pub fn new() -> Self {
        Self {
            width: 1.0,
            current_width: 1.0,
            slew_rate: 0.001,
            enabled: false,
        }
    }

    /// Set the stereo width factor.
    ///
    /// Clamped to [0.0, 2.0]:
    /// - 0.0 = mono collapse
    /// - 1.0 = passthrough
    /// - 2.0 = maximum widening
    pub fn set_width(&mut self, width: f32) {
        self.width = width.clamp(0.0, 2.0);
    }

    /// Get the current width setting
    pub fn width(&self) -> f32 {
        self.width
    }

    /// Enable or disable the stereo enhancer
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Whether the enhancer is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Process a stereo sample with width adjustment.
    ///
    /// This is phase-safe: if the input is mono (L == R), the output
    /// will also be mono regardless of the width setting.
    ///
    /// We now ALWAYS run the dezippering step (so current_width converges
    /// to the target even when the target is 1.0) and ALWAYS apply the
    /// widening math. At current_width == 1.0 the math is equivalent to
    /// passthrough (mid + side == left, mid - side == right), so removing
    /// the bypass is correct AND avoids the stale-current_width artifact.
    /// The cost is a few extra multiplies per sample, which is negligible
    /// on modern CPUs.
    #[inline]
    pub fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        if !self.enabled {
            return (left, right);
        }

        // Dezippering: smoothly approach target width
        self.current_width += (self.width - self.current_width) * self.slew_rate;

        // Mid/side decomposition
        let mid = (left + right) * 0.5;
        let side = (left - right) * 0.5;

        // Apply width to side channel only (phase-safe)
        let adjusted_side = side * self.current_width;

        // Reconstruct stereo
        (mid + adjusted_side, mid - adjusted_side)
    }

    /// Process a stereo sample pair in native f64 precision.
    #[inline]
    pub fn process_f64(&mut self, left: f64, right: f64) -> (f64, f64) {
        if !self.enabled {
            return (left, right);
        }

        // Dezippering: smoothly approach target width
        self.current_width += (self.width - self.current_width) * self.slew_rate;

        // Mid/side decomposition
        let mid = (left + right) * 0.5;
        let side = (left - right) * 0.5;

        // Apply width to side channel only (phase-safe)
        let adjusted_side = side * (self.current_width as f64);

        // Reconstruct stereo
        (mid + adjusted_side, mid - adjusted_side)
    }

    /// Process a block of stereo frames in place. Hoists the enabled check
    /// out of the per-frame loop, and switches to a 2x2 matrix once settled.
    #[inline]
    pub fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        if !self.enabled {
            return;
        }
        let n = left.len().min(right.len());
        if n == 0 {
            return;
        }

        // Check if settled
        if (self.width - self.current_width).abs() < 1e-5 {
            self.current_width = self.width;
            let w = self.current_width;
            // Passthrough check: if settled at width 1.0, mid + side = L, mid - side = R
            if (w - 1.0).abs() < 1e-5 {
                return;
            }
            let a = 0.5 * (1.0 + w);
            let b = 0.5 * (1.0 - w);
            for i in 0..n {
                let l = left[i];
                let r = right[i];
                left[i] = a * l + b * r;
                right[i] = b * l + a * r;
            }
            return;
        }

        for i in 0..n {
            self.current_width += (self.width - self.current_width) * self.slew_rate;
            if (self.width - self.current_width).abs() < 1e-5 {
                self.current_width = self.width;
                let w = self.current_width;
                let a = 0.5 * (1.0 + w);
                let b = 0.5 * (1.0 - w);
                let l = left[i];
                let r = right[i];
                left[i] = a * l + b * r;
                right[i] = b * l + a * r;
                if i + 1 < n && (w - 1.0).abs() >= 1e-5 {
                    for j in (i + 1)..n {
                        let lj = left[j];
                        let rj = right[j];
                        left[j] = a * lj + b * rj;
                        right[j] = b * lj + a * rj;
                    }
                }
                return;
            }
            let mid = (left[i] + right[i]) * 0.5;
            let side = (left[i] - right[i]) * 0.5;
            let adjusted_side = side * self.current_width;
            left[i] = mid + adjusted_side;
            right[i] = mid - adjusted_side;
        }
    }

    /// Process a block of stereo frames in native f64 precision. Hoists the
    /// enabled check out of the per-frame loop, and switches to a 2x2 matrix once settled.
    #[inline]
    pub fn process_block_f64(&mut self, left: &mut [f64], right: &mut [f64]) {
        if !self.enabled {
            return;
        }
        let n = left.len().min(right.len());
        if n == 0 {
            return;
        }

        if (self.width - self.current_width).abs() < 1e-5 {
            self.current_width = self.width;
            let w = self.current_width as f64;
            if (w - 1.0).abs() < 1e-5 {
                return;
            }
            let a = 0.5 * (1.0 + w);
            let b = 0.5 * (1.0 - w);
            for i in 0..n {
                let l = left[i];
                let r = right[i];
                left[i] = a * l + b * r;
                right[i] = b * l + a * r;
            }
            return;
        }

        for i in 0..n {
            self.current_width += (self.width - self.current_width) * self.slew_rate;
            if (self.width - self.current_width).abs() < 1e-5 {
                self.current_width = self.width;
                let w = self.current_width as f64;
                let a = 0.5 * (1.0 + w);
                let b = 0.5 * (1.0 - w);
                let l = left[i];
                let r = right[i];
                left[i] = a * l + b * r;
                right[i] = b * l + a * r;
                if i + 1 < n && (w - 1.0).abs() >= 1e-5 {
                    for j in (i + 1)..n {
                        let lj = left[j];
                        let rj = right[j];
                        left[j] = a * lj + b * rj;
                        right[j] = b * lj + a * rj;
                    }
                }
                return;
            }
            let w = self.current_width as f64;
            let mid = (left[i] + right[i]) * 0.5;
            let side = (left[i] - right[i]) * 0.5;
            let adjusted_side = side * w;
            left[i] = mid + adjusted_side;
            right[i] = mid - adjusted_side;
        }
    }

    /// Reset runtime state variables only (not user-configured settings).
    ///
    /// Preserves `width` so that after a seek or stop — which call
    /// `pipeline.reset()` — the user's stereo width setting is not lost.
    /// Only resets `current_width` (the dezippered runtime value) back to
    /// passthrough (1.0), allowing it to smoothly ramp up to `width` again.
    pub fn reset(&mut self) {
        self.current_width = 1.0;
        // Do NOT reset self.width — that's the user's configured setting.
        // slew_rate is set in set_sample_rate(); keep whatever was configured.
    }

    /// Set the sample rate and compute a sample-rate-dependent slew rate.
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate <= 0.0 || !sample_rate.is_finite() {
            return;
        }
        // 10 ms time constant: after 10 ms of samples, the width should be
        // ~63% of the way to the target. slew_rate = 1 - exp(-1 / (0.010 * sr))
        let tau_samples = 0.010 * sample_rate;
        self.slew_rate = 1.0 - (-1.0 / tau_samples).exp();
    }
}

impl Default for StereoEnhancer {
    fn default() -> Self {
        Self::new()
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_width_1_is_passthrough() {
        let mut enhancer = StereoEnhancer::new();
        enhancer.set_enabled(true);
        enhancer.set_width(1.0);
        let (l, r) = enhancer.process(0.5, 0.3);
        assert!((l - 0.5).abs() < 1e-5);
        assert!((r - 0.3).abs() < 1e-5);
    }

    #[test]
    fn test_mono_collapse() {
        let mut enhancer = StereoEnhancer::new();
        enhancer.set_enabled(true);
        enhancer.set_width(0.0);
        for _ in 0..10000 {
            enhancer.process(0.8, 0.2);
        }
        let (l, r) = enhancer.process(0.8, 0.2);
        // Width 0 = mono, both channels should be the average
        assert!((l - r).abs() < 1e-4, "Width 0 should produce mono");
        assert!((l - 0.5).abs() < 1e-4, "Mono should be average of L and R");
    }

    #[test]
    fn test_phase_safe() {
        let mut enhancer = StereoEnhancer::new();
        enhancer.set_enabled(true);
        enhancer.set_width(1.5);
        // If input is mono, output should remain mono (no artificial stereo)
        let (l, r) = enhancer.process(0.5, 0.5);
        assert!((l - r).abs() < 1e-5, "Mono input should stay mono");
    }

    #[test]
    fn test_widening_increases_stereo_separation() {
        let mut enhancer = StereoEnhancer::new();
        enhancer.set_enabled(true);

        // Input with some stereo content
        let input_l = 0.7;
        let input_r = 0.3;

        enhancer.set_width(1.0);
        let (norm_l, norm_r) = enhancer.process(input_l, input_r);

        enhancer.set_width(1.5);
        let (wide_l, wide_r) = enhancer.process(input_l, input_r);

        // Widening should increase the L-R difference
        let normal_diff = (norm_l - norm_r).abs();
        let wide_diff = (wide_l - wide_r).abs();
        assert!(
            wide_diff > normal_diff,
            "Widening should increase stereo separation"
        );
    }

    #[test]
    fn test_width_clamped() {
        let mut enhancer = StereoEnhancer::new();
        enhancer.set_width(5.0); // Should be clamped to 2.0
        assert!(
            (enhancer.width() - 2.0).abs() < 1e-5,
            "Width should be clamped to 2.0"
        );

        enhancer.set_width(-1.0); // Should be clamped to 0.0
        assert!(
            (enhancer.width() - 0.0).abs() < 1e-5,
            "Width should be clamped to 0.0"
        );
    }

    #[test]
    fn test_disabled_is_passthrough() {
        let mut enhancer = StereoEnhancer::new();
        enhancer.set_enabled(false);
        enhancer.set_width(0.0); // Even with width=0
        let (l, r) = enhancer.process(0.8, 0.2);
        assert!(
            (l - 0.8).abs() < 1e-5,
            "Disabled enhancer should pass through"
        );
        assert!(
            (r - 0.2).abs() < 1e-5,
            "Disabled enhancer should pass through"
        );
    }

    #[test]
    fn test_mid_preserved() {
        let mut enhancer = StereoEnhancer::new();
        enhancer.set_enabled(true);
        enhancer.set_width(1.5);

        let input_l = 0.6;
        let input_r = 0.4;
        let (out_l, out_r) = enhancer.process(input_l, input_r);

        // The mid component (average) should always be preserved
        let input_mid = (input_l + input_r) * 0.5;
        let output_mid = (out_l + out_r) * 0.5;
        assert!(
            (input_mid - output_mid).abs() < 1e-5,
            "Mid component should be preserved"
        );
    }

    #[test]
    fn test_process_block_equivalence() {
        let mut e_sample = StereoEnhancer::new();
        e_sample.set_enabled(true);
        e_sample.set_width(1.8);

        let mut e_block = StereoEnhancer::new();
        e_block.set_enabled(true);
        e_block.set_width(1.8);

        let n = 256;
        let mut sample_l = vec![0.5f32; n];
        let mut sample_r = vec![0.2f32; n];
        for i in 0..n {
            let (l, r) = e_sample.process(sample_l[i], sample_r[i]);
            sample_l[i] = l;
            sample_r[i] = r;
        }

        let mut block_l = vec![0.5f32; n];
        let mut block_r = vec![0.2f32; n];
        e_block.process_block(&mut block_l, &mut block_r);

        for i in 0..n {
            assert!(
                (sample_l[i] - block_l[i]).abs() < 1e-5,
                "L mismatch at {i}: {} vs {}",
                sample_l[i],
                block_l[i]
            );
            assert!(
                (sample_r[i] - block_r[i]).abs() < 1e-5,
                "R mismatch at {i}: {} vs {}",
                sample_r[i],
                block_r[i]
            );
        }
    }
}
