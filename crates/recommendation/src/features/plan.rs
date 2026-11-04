use super::{
    estimate_tempo, log_frequency_normalized, quantile_sorted, CHROMA_HIGH_HZ, CHROMA_LOW_HZ,
    FEATURE_COUNT,
};
use super::{AnalysisConfig, RealFft};
use crate::FeatureVector;
use std::f32::consts::PI;

pub(crate) struct WindowFeatures {
    pub(crate) vector: FeatureVector,
    pub(crate) tempo_bpm: f32,
    pub(crate) tempo_confidence: f32,
    pub(crate) tuning_cents: f32,
    pub(crate) chroma_weight: f32,
}

pub(crate) struct AnalysisPlan {
    sample_rate: f32,
    fft_size: usize,
    hop_size: usize,
    frame_gate_dbfs: f32,
    relative_gate_db: f32,
    min_tempo_confidence: f32,
    hann: Vec<f32>,
    fft: RealFft,
    frame: Vec<f32>,
    powers: Vec<f32>,
    bin_frequencies: Vec<f32>,
    chroma_start: usize,
    chroma_end: usize,
    chroma_weights: Vec<[f32; 12]>,
    rms_db: Vec<f32>,
    sorted_rms_db: Vec<f32>,
    centroid: Vec<f32>,
    rolloff: Vec<f32>,
    flatness: Vec<f32>,
    flux: Vec<f32>,
    frame_low: Vec<f32>,
    frame_total: Vec<f32>,
    frame_chroma: Vec<[f32; 12]>,
    onset: Vec<f32>,
    previous_normalized_power: Vec<f32>,
    previous_log_magnitude: Vec<f32>,
}

impl AnalysisPlan {
    pub(crate) fn new(config: &AnalysisConfig) -> Self {
        let hann = (0..config.fft_size)
            .map(|i| {
                if config.fft_size <= 1 {
                    1.0
                } else {
                    // Periodic Hann window: standard for overlap-add spectral analysis.
                    0.5 - 0.5 * (2.0 * PI * i as f32 / config.fft_size as f32).cos()
                }
            })
            .collect();
        let sample_rate = config.target_sample_rate as f32;
        let bins = config.fft_size / 2 + 1;
        let bin_frequencies: Vec<f32> = (0..bins)
            .map(|k| k as f32 * sample_rate / config.fft_size as f32)
            .collect();
        let chroma_start = bin_frequencies
            .iter()
            .position(|frequency| *frequency >= CHROMA_LOW_HZ)
            .unwrap_or(bins.saturating_sub(1));
        let chroma_end = bin_frequencies
            .iter()
            .position(|frequency| *frequency > CHROMA_HIGH_HZ)
            .unwrap_or(bins)
            .max(chroma_start + 1);
        Self {
            sample_rate,
            fft_size: config.fft_size,
            hop_size: config.hop_size,
            frame_gate_dbfs: config.frame_gate_dbfs,
            relative_gate_db: config.relative_gate_db,
            min_tempo_confidence: config.min_tempo_confidence,
            hann,
            fft: RealFft::new(config.fft_size),
            frame: vec![0.0; config.fft_size],
            powers: vec![0.0; bins],
            bin_frequencies,
            chroma_start,
            chroma_end,
            chroma_weights: vec![[0.0; 12]; bins],
            rms_db: Vec::new(),
            sorted_rms_db: Vec::new(),
            centroid: Vec::new(),
            rolloff: Vec::new(),
            flatness: Vec::new(),
            flux: Vec::new(),
            frame_low: Vec::new(),
            frame_total: Vec::new(),
            frame_chroma: Vec::new(),
            onset: Vec::new(),
            previous_normalized_power: vec![0.0; bins],
            previous_log_magnitude: vec![0.0; bins],
        }
    }

    pub(crate) fn extract(&mut self, samples: &[f32]) -> Option<WindowFeatures> {
        if samples.len() < 32 {
            return None;
        }
        self.rms_db.clear();
        self.centroid.clear();
        self.rolloff.clear();
        self.flatness.clear();
        self.flux.clear();
        self.frame_low.clear();
        self.frame_total.clear();
        self.frame_chroma.clear();
        self.onset.clear();
        self.previous_normalized_power.fill(0.0);
        self.previous_log_magnitude.fill(0.0);

        let tuning_cents = self.estimate_tuning(samples);
        self.build_chroma_weights(tuning_cents);

        let frame_count = if samples.len() >= self.fft_size {
            (samples.len() - self.fft_size) / self.hop_size + 1
        } else {
            1
        };
        let mut have_previous = false;

        for frame_index in 0..frame_count {
            let start = frame_index * self.hop_size;
            let actual = (samples.len() - start).min(self.fft_size);
            if actual < self.fft_size / 2 {
                break;
            }
            let frame_samples = &samples[start..start + actual];
            let mut energy = 0.0_f64;
            for sample in frame_samples {
                energy += (*sample as f64) * (*sample as f64);
            }
            let rms = (energy / actual as f64).sqrt().max(1.0e-9);
            let db = (20.0 * rms.log10()).clamp(-120.0, 0.0) as f32;
            self.rms_db.push(db);

            self.fill_frame(frame_samples, actual);
            self.fft.forward_into(&self.frame, &mut self.powers);

            let bins = self.powers.len();
            let mut total_power = 0.0_f64;
            for power in &self.powers[1..] {
                total_power += *power as f64;
            }
            if total_power <= 1.0e-15 {
                self.centroid.push(0.0);
                self.rolloff.push(0.0);
                self.flatness.push(0.0);
                self.flux.push(0.0);
                self.frame_low.push(0.0);
                self.frame_total.push(0.0);
                self.frame_chroma.push([0.0; 12]);
                self.onset.push(0.0);
                self.previous_normalized_power.fill(0.0);
                self.previous_log_magnitude.fill(0.0);
                have_previous = true;
                continue;
            }

            let mut weighted_frequency = 0.0_f64;
            for k in 1..bins {
                weighted_frequency += self.bin_frequencies[k] as f64 * self.powers[k] as f64;
            }
            let centroid_hz = (weighted_frequency / total_power) as f32;
            self.centroid.push(log_frequency_normalized(
                centroid_hz,
                20.0,
                self.sample_rate * 0.5,
            ));

            let target_power = total_power * 0.85;
            let mut cumulative = 0.0_f64;
            let mut rolloff_hz = self.sample_rate * 0.5;
            for k in 1..bins {
                cumulative += self.powers[k] as f64;
                if cumulative >= target_power {
                    rolloff_hz = self.bin_frequencies[k];
                    break;
                }
            }
            self.rolloff.push(log_frequency_normalized(
                rolloff_hz,
                20.0,
                self.sample_rate * 0.5,
            ));

            let mut log_power_sum = 0.0_f64;
            let mut arithmetic_power_sum = 0.0_f64;
            let mut frame_low = 0.0_f64;
            let mut frame_total = 0.0_f64;
            for k in 1..bins {
                let power = self.powers[k] as f64 + 1.0e-18;
                log_power_sum += power.ln();
                arithmetic_power_sum += power;
                let frequency = self.bin_frequencies[k];
                if (30.0..=250.0).contains(&frequency) {
                    frame_low += power;
                }
                if frequency >= 30.0 {
                    frame_total += power;
                }
            }
            let count = (bins - 1).max(1) as f64;
            let geometric = (log_power_sum / count).exp();
            let arithmetic = arithmetic_power_sum / count;
            self.flatness
                .push((geometric / arithmetic.max(1.0e-18)).clamp(0.0, 1.0) as f32);
            self.frame_low.push(frame_low as f32);
            self.frame_total.push(frame_total as f32);

            let normalization = total_power.max(1.0e-18);
            let mut flux = 0.0_f32;
            let mut onset = 0.0_f32;
            for k in 1..bins {
                let normalized = self.powers[k] as f64 / normalization;
                let log_magnitude = (self.powers[k] as f64 + 1.0e-18).ln();
                if have_previous {
                    flux += (normalized - self.previous_normalized_power[k] as f64).max(0.0) as f32;
                    onset +=
                        (log_magnitude - self.previous_log_magnitude[k] as f64).max(0.0) as f32;
                }
                self.previous_normalized_power[k] = normalized as f32;
                self.previous_log_magnitude[k] = log_magnitude as f32;
            }
            self.flux.push(flux.clamp(0.0, 1.0));
            self.onset.push(onset / (bins - 1).max(1) as f32);
            have_previous = true;

            let mut chroma = [0.0_f64; 12];
            for k in self.chroma_start..self.chroma_end {
                let power = self.powers[k] as f64;
                if power <= 0.0 {
                    continue;
                }
                let weights = &self.chroma_weights[k];
                for (index, weight) in weights.iter().enumerate() {
                    let weight = *weight as f64;
                    if weight > 0.0 {
                        chroma[index] += power * weight;
                    }
                }
            }
            let chroma_sum: f64 = chroma.iter().sum();
            if chroma_sum > 1.0e-12 {
                for value in &mut chroma {
                    *value /= chroma_sum;
                }
                self.frame_chroma.push(chroma.map(|value| value as f32));
            } else {
                self.frame_chroma.push([0.0; 12]);
            }
        }

        if self.rms_db.is_empty() || self.centroid.is_empty() || self.frame_chroma.is_empty() {
            return None;
        }

        self.sorted_rms_db.clear();
        self.sorted_rms_db.extend_from_slice(&self.rms_db);
        self.sorted_rms_db.sort_by(f32::total_cmp);
        let p10 = quantile_sorted(&self.sorted_rms_db, 0.10);
        let p90 = quantile_sorted(&self.sorted_rms_db, 0.90);
        let max_db = self
            .sorted_rms_db
            .last()
            .copied()
            .unwrap_or(self.frame_gate_dbfs);
        let gate = (max_db - self.relative_gate_db).max(self.frame_gate_dbfs);

        let mut gated_frames = 0_u32;
        let mut power_sum = 0.0_f64;
        for db in &self.rms_db {
            power_sum += 10.0_f64.powf(*db as f64 / 10.0);
            if *db >= gate {
                gated_frames += 1;
            }
        }
        if gated_frames == 0 {
            return None;
        }
        let mean_power = power_sum / self.rms_db.len() as f64;
        let mean_db = (10.0 * mean_power.max(1.0e-18).log10()).clamp(-120.0, 0.0) as f32;

        let mut centroid_sum = 0.0_f32;
        let mut rolloff_sum = 0.0_f32;
        let mut flatness_sum = 0.0_f32;
        let mut flux_sum = 0.0_f32;
        let mut low_sum = 0.0_f64;
        let mut band_sum = 0.0_f64;
        let mut chroma_sum = [0.0_f64; 12];
        for index in 0..self.rms_db.len() {
            if self.rms_db[index] < gate {
                continue;
            }
            centroid_sum += self.centroid[index];
            rolloff_sum += self.rolloff[index];
            flatness_sum += self.flatness[index];
            flux_sum += self.flux[index];
            low_sum += self.frame_low[index] as f64;
            band_sum += self.frame_total[index] as f64;
            for (target, source) in chroma_sum.iter_mut().zip(self.frame_chroma[index].iter()) {
                *target += *source as f64;
            }
        }
        let gated = gated_frames as f32;
        let mut values = [0.0_f32; FEATURE_COUNT];
        values[0] = ((mean_db + 60.0) / 60.0).clamp(0.0, 1.0);
        values[1] = ((p90 - p10) / 40.0).clamp(0.0, 1.0);
        values[2] = centroid_sum / gated;
        values[3] = rolloff_sum / gated;
        values[4] = flatness_sum / gated;
        values[5] = (flux_sum / gated).clamp(0.0, 1.0);
        values[6] = if band_sum > 1.0e-12 {
            (low_sum / band_sum).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };

        let (tempo_bpm, tempo_confidence) =
            estimate_tempo(&self.onset, self.sample_rate / self.hop_size as f32);
        let mut valid_mask = (1_u32 << 7) - 1;
        if tempo_confidence >= self.min_tempo_confidence {
            values[7] = ((tempo_bpm - 50.0) / 150.0).clamp(0.0, 1.0);
            valid_mask |= 1_u32 << 7;
        }

        let chroma_total: f64 = chroma_sum.iter().sum();
        if chroma_total > 1.0e-10 {
            for index in 0..12 {
                values[8 + index] = (chroma_sum[index] / chroma_total).clamp(0.0, 1.0) as f32;
                valid_mask |= 1_u32 << (8 + index);
            }
        }

        Some(WindowFeatures {
            vector: FeatureVector::new(values, valid_mask),
            tempo_bpm,
            tempo_confidence,
            tuning_cents,
            chroma_weight: gated,
        })
    }

    fn fill_frame(&mut self, samples: &[f32], actual: usize) {
        self.frame[..actual].copy_from_slice(samples);
        self.frame[actual..].fill(0.0);
        let hann = &self.hann;
        for (value, window) in self.frame.iter_mut().zip(hann.iter()) {
            *value *= *window;
        }
    }

    /// Estimate the recording's tuning offset in cents by locating spectral
    /// peaks and averaging their fractional MIDI deviation on a one-semitone
    /// circle. Harmonics share the same deviation, so this remains stable even
    /// when the fundamental is weak.
    fn estimate_tuning(&mut self, samples: &[f32]) -> f32 {
        if samples.len() < self.fft_size / 2 || self.chroma_end <= self.chroma_start + 2 {
            return 0.0;
        }
        let frame_count = if samples.len() >= self.fft_size {
            (samples.len() - self.fft_size) / self.fft_size + 1
        } else {
            1
        };
        let step = (frame_count / 16).max(1);
        let mut cosine_sum = 0.0_f64;
        let mut sine_sum = 0.0_f64;
        let mut total_weight = 0.0_f64;

        for frame_index in (0..frame_count).step_by(step).take(16) {
            let start = frame_index * self.fft_size;
            let actual = (samples.len() - start).min(self.fft_size);
            if actual < self.fft_size / 2 {
                break;
            }
            self.fill_frame(&samples[start..start + actual], actual);
            self.fft.forward_into(&self.frame, &mut self.powers);
            let max_power = self.powers[self.chroma_start..self.chroma_end]
                .iter()
                .copied()
                .fold(0.0_f32, f32::max);
            if max_power <= 1.0e-12 {
                continue;
            }
            for (offset, triple) in self.powers[self.chroma_start..self.chroma_end]
                .windows(3)
                .enumerate()
            {
                let left = triple[0];
                let center = triple[1];
                let right = triple[2];
                if center <= left || center < right || center < max_power * 0.01 {
                    continue;
                }
                let a = (left as f64 + 1.0e-18).ln();
                let b = (center as f64 + 1.0e-18).ln();
                let c = (right as f64 + 1.0e-18).ln();
                let denominator = a - 2.0 * b + c;
                let delta = if denominator.abs() > 1.0e-12 {
                    (0.5 * (a - c) / denominator).clamp(-0.5, 0.5)
                } else {
                    0.0
                };
                let bin = self.chroma_start + offset + 1;
                let frequency =
                    (bin as f64 + delta) * self.sample_rate as f64 / self.fft_size as f64;
                if !(CHROMA_LOW_HZ as f64..=CHROMA_HIGH_HZ as f64).contains(&frequency) {
                    continue;
                }
                let midi = 69.0 + 12.0 * (frequency / 440.0).log2();
                let deviation = midi - midi.round();
                let angle = 2.0 * std::f64::consts::PI * deviation;
                let weight = center as f64;
                cosine_sum += angle.cos() * weight;
                sine_sum += angle.sin() * weight;
                total_weight += weight;
            }
        }

        if total_weight <= 1.0e-12 {
            return 0.0;
        }
        let semitones = sine_sum.atan2(cosine_sum) / (2.0 * std::f64::consts::PI);
        (semitones.clamp(-0.5, 0.5) * 100.0) as f32
    }

    /// Build a tuning-aware, approximately constant-Q chroma filterbank. Each
    /// linear FFT bin contributes to nearby pitch classes through a Gaussian
    /// kernel, and the `1/f` gain compensates for the larger number of high
    /// frequency bins per semitone in a linear spectrum.
    fn build_chroma_weights(&mut self, tuning_cents: f32) {
        self.chroma_weights.fill([0.0; 12]);
        let tuning_semitones = tuning_cents / 100.0;
        for k in self.chroma_start..self.chroma_end {
            let frequency = self.bin_frequencies[k];
            if frequency < CHROMA_LOW_HZ || frequency > CHROMA_HIGH_HZ {
                continue;
            }
            let midi = 69.0 + 12.0 * (frequency / 440.0).log2() - tuning_semitones;
            let gain = 1.0 / frequency;
            let weights = &mut self.chroma_weights[k];
            for (pitch_class, weight) in weights.iter_mut().enumerate() {
                let distance = (midi - pitch_class as f32).rem_euclid(12.0);
                let distance = distance.min(12.0 - distance);
                if distance > 1.5 {
                    continue;
                }
                let kernel = (-0.5 * (distance / 0.55).powi(2)).exp();
                if kernel > 1.0e-4 {
                    *weight = kernel * gain;
                }
            }
        }
    }
}
