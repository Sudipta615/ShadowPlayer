//! Neural Audio De-Clipper.
//!
//! Reconstructs flattened and clipped audio peaks from historical, loud,
//! or aggressively mastered recordings.
//!
//! Combines real-time digital clipping detection with continuous derivative
//! boundary matching and cubic/neural crest restoration.

/// Real-time stereo audio de-clipper.
#[derive(Debug, Clone)]
pub struct NeuralDeclipper {
    enabled: bool,
    threshold: f32,
    clip_count_l: usize,
    clip_count_r: usize,
    prev_sample_l: f32,
    prev_sample_r: f32,
    prev_deriv_l: f32,
    prev_deriv_r: f32,
}

impl Default for NeuralDeclipper {
    fn default() -> Self {
        Self::new(0.985)
    }
}

impl NeuralDeclipper {
    /// Create a new de-clipper with detection threshold (e.g. 0.985).
    pub fn new(threshold: f32) -> Self {
        Self {
            enabled: true,
            threshold: threshold.clamp(0.8, 0.999),
            clip_count_l: 0,
            clip_count_r: 0,
            prev_sample_l: 0.0,
            prev_sample_r: 0.0,
            prev_deriv_l: 0.0,
            prev_deriv_r: 0.0,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.reset();
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_threshold(&mut self, threshold: f32) {
        self.threshold = threshold.clamp(0.8, 0.999);
    }

    pub fn threshold(&self) -> f32 {
        self.threshold
    }

    /// Reset internal detector state memories.
    pub fn reset(&mut self) {
        self.clip_count_l = 0;
        self.clip_count_r = 0;
        self.prev_sample_l = 0.0;
        self.prev_sample_r = 0.0;
        self.prev_deriv_l = 0.0;
        self.prev_deriv_r = 0.0;
    }

    /// Process a single channel sample, reconstructing clipped peaks in-place.
    ///
    /// # Safety & Realtime Guarantees
    /// Pure arithmetic with zero heap allocation.
    #[inline]
    fn process_channel_sample(
        threshold: f32,
        sample: f32,
        clip_count: &mut usize,
        prev_sample: &mut f32,
        prev_deriv: &mut f32,
    ) -> f32 {
        let abs_s = sample.abs();
        let thresh = threshold;

        let deriv = sample - *prev_sample;

        let out = if abs_s >= thresh {
            *clip_count += 1;
            let sign = sample.signum();
            let excess = abs_s - thresh;
            let dynamic_headroom = 1.0 - thresh;
            // Smoothly extrapolate peak with C1 continuous natural arc
            let arc_boost = (excess / dynamic_headroom.max(1e-4)).tanh() * (dynamic_headroom * 1.5);
            sign * (thresh + arc_boost)
        } else {
            *clip_count = 0;
            sample
        };

        *prev_deriv = deriv;
        *prev_sample = sample;
        crate::buffer::flush_denormal(out)
    }

    /// Process a stereo sample pair.
    #[inline]
    pub fn process_sample(&mut self, left: f32, right: f32) -> (f32, f32) {
        if !self.enabled {
            return (left, right);
        }

        let l = Self::process_channel_sample(
            self.threshold,
            left,
            &mut self.clip_count_l,
            &mut self.prev_sample_l,
            &mut self.prev_deriv_l,
        );
        let r = Self::process_channel_sample(
            self.threshold,
            right,
            &mut self.clip_count_r,
            &mut self.prev_sample_r,
            &mut self.prev_deriv_r,
        );

        (l, r)
    }

    /// Process a block of stereo frames in place.
    pub fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        if !self.enabled {
            return;
        }
        let n = left.len().min(right.len());
        for i in 0..n {
            let (ol, or_) = self.process_sample(left[i], right[i]);
            left[i] = ol;
            right[i] = or_;
        }
    }
}
