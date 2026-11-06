//! Real-time stereo Neural Processor.
//!
//! Applies machine learning / neural analog modeling to audio streams with
//! zero heap allocations on the audio path.

use super::model::NeuralModel;

/// Real-time stereo neural processor.
#[derive(Debug, Clone)]
pub struct NeuralProcessor {
    enabled: bool,
    drive_db: f32,
    drive_linear: f32,
    output_db: f32,
    output_linear: f32,
    wet_mix: f32,
    current_wet_mix: f32,
    model_l: NeuralModel,
    model_r: NeuralModel,
    in_buf_l: [f32; 1],
    out_buf_l: [f32; 1],
    in_buf_r: [f32; 1],
    out_buf_r: [f32; 1],
}

impl NeuralProcessor {
    /// Construct a new neural processor with duplicate models for Left and Right channels.
    pub fn new(model: NeuralModel) -> Self {
        let model_l = model.clone();
        let model_r = model;
        Self {
            enabled: false,
            drive_db: 0.0,
            drive_linear: 1.0,
            output_db: 0.0,
            output_linear: 1.0,
            wet_mix: 1.0,
            current_wet_mix: 1.0,
            model_l,
            model_r,
            in_buf_l: [0.0],
            out_buf_l: [0.0],
            in_buf_r: [0.0],
            out_buf_r: [0.0],
        }
    }

    /// Factory constructor for 12AX7 Triode Tube Preamp emulation.
    pub fn tube_preamp() -> Self {
        Self::new(NeuralModel::tube_preamp_12ax7())
    }

    /// Factory constructor for Studer A800 Analog Tape saturation.
    pub fn tape_saturation() -> Self {
        Self::new(NeuralModel::tape_saturation_studer())
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.enabled = enabled;
            if !enabled {
                self.reset();
            }
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_drive_db(&mut self, drive_db: f32) {
        let db = drive_db.clamp(-24.0, 24.0);
        self.drive_db = db;
        self.drive_linear = 10.0_f32.powf(db / 20.0);
    }

    pub fn drive_db(&self) -> f32 {
        self.drive_db
    }

    pub fn set_output_db(&mut self, output_db: f32) {
        let db = output_db.clamp(-24.0, 24.0);
        self.output_db = db;
        self.output_linear = 10.0_f32.powf(db / 20.0);
    }

    pub fn output_db(&self) -> f32 {
        self.output_db
    }

    pub fn set_wet_mix(&mut self, wet_mix: f32) {
        self.wet_mix = wet_mix.clamp(0.0, 1.0);
    }

    pub fn wet_mix(&self) -> f32 {
        self.wet_mix
    }

    /// Algorithmic latency in samples (0 for memoryless and recurrent step models).
    pub fn latency_samples(&self) -> usize {
        0
    }

    /// Reset internal recurrent state memories.
    pub fn reset(&mut self) {
        self.model_l.reset();
        self.model_r.reset();
        self.in_buf_l.fill(0.0);
        self.out_buf_l.fill(0.0);
        self.in_buf_r.fill(0.0);
        self.out_buf_r.fill(0.0);
    }

    /// Process a stereo sample pair.
    ///
    /// # Safety & Realtime Guarantees
    /// Pure in-place execution with ZERO allocations.
    #[inline]
    pub fn process_sample(&mut self, left: f32, right: f32) -> (f32, f32) {
        if !self.enabled {
            return (left, right);
        }

        // Apply input drive gain
        self.in_buf_l[0] = left * self.drive_linear;
        self.in_buf_r[0] = right * self.drive_linear;

        // Neural forward pass
        self.model_l.forward(&self.in_buf_l, &mut self.out_buf_l);
        self.model_r.forward(&self.in_buf_r, &mut self.out_buf_r);

        // Apply output makeup gain
        let wet_l = self.out_buf_l[0] * self.output_linear;
        let wet_r = self.out_buf_r[0] * self.output_linear;

        // Parameter smoothing for wet/dry crossfade
        if (self.current_wet_mix - self.wet_mix).abs() > 1e-4 {
            self.current_wet_mix += 0.005 * (self.wet_mix - self.current_wet_mix);
        } else {
            self.current_wet_mix = self.wet_mix;
        }

        let mix = self.current_wet_mix;
        let out_l = left * (1.0 - mix) + wet_l * mix;
        let out_r = right * (1.0 - mix) + wet_r * mix;

        (
            crate::buffer::flush_denormal(out_l),
            crate::buffer::flush_denormal(out_r),
        )
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

