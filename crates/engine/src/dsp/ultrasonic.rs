//! Output-domain ultrasonic low-pass ("junk filter").
//!
//! Some sources carry out-of-band energy that is inaudible but not harmless:
//! DSD noise-shaping residue, ADC artifacts, or analogue-tape transfers with
//! HF disturbance. Tweeters with low power handling, class-D amplifiers and
//! NOS/first-order DACs can misbehave in its presence. This stage is a
//! deliberate user filter — it is bypassed in bit-perfect/DoP modes like every
//! other sample-altering stage, and disabled it is a bit-exact passthrough.
//!
//! ## Design
//!
//! Minimum-phase Butterworth cascades built from RBJ biquads (the same
//! primitive the EQ uses), so there is no latency to report and no
//! pre-ringing:
//!
//! | Selection | Order | Slope | Corner |
//! | --- | --- | --- | --- |
//! | `20 kHz` | 4 | 24 dB/oct | 20 kHz |
//! | `30/40/50 kHz` | 2 | 12 dB/oct | the named corner |
//! | `2x/4x/8x` | 8 | 48 dB/oct | 2/4/8 × output base rate |
//!
//! A corner at or above 45 % of the sample rate cannot be placed below
//! Nyquist; the stage then reports itself **inactive** rather than pretending
//! to filter. That is the honest answer at a low output rate (e.g. `50 kHz`
//! at an 88.2 kHz output has nothing to remove).
//!
//! Realtime contract: all state is inline (`MAX_CHANNELS × 4` biquads), so a
//! block allocates nothing and takes no locks.

use crate::buffer::MAX_CHANNELS;
use crate::dsp::biquad::{BiquadCoeffs, BiquadState};
use config::UltrasonicFilter;

/// Maximum biquad sections any selection needs (8th order = 4).
const MAX_SECTIONS: usize = 4;

/// Butterworth Q values for the supported orders.
fn butterworth_qs(order: usize) -> &'static [f32] {
    match order {
        2 => &[std::f32::consts::FRAC_1_SQRT_2],
        4 => &[0.541_196_1, 1.306_563],
        8 => &[0.509_795_6, 0.601_344_9, 0.899_976_2, 2.562_915],
        _ => &[],
    }
}

/// The output-domain ultrasonic filter stage.
pub struct UltrasonicStage {
    filter: UltrasonicFilter,
    sample_rate: f32,
    /// Linear-phase? No — minimum-phase RBJ sections, see the module docs.
    sections_f32: [BiquadCoeffs<f32>; MAX_SECTIONS],
    sections_f64: [BiquadCoeffs<f64>; MAX_SECTIONS],
    section_count: usize,
    states_f32: [[BiquadState<f32>; MAX_SECTIONS]; MAX_CHANNELS],
    states_f64: [[BiquadState<f64>; MAX_SECTIONS]; MAX_CHANNELS],
    corner_hz: f32,
    /// True only when a real filter is installed and the corner is below
    /// Nyquist.
    active: bool,
}

impl UltrasonicStage {
    pub fn new(filter: UltrasonicFilter, sample_rate: f32) -> Self {
        let mut stage = Self {
            filter: UltrasonicFilter::Off,
            sample_rate: 0.0,
            sections_f32: [BiquadCoeffs::IDENTITY; MAX_SECTIONS],
            sections_f64: [BiquadCoeffs::<f64>::identity(); MAX_SECTIONS],
            section_count: 0,
            states_f32: [[BiquadState::default(); MAX_SECTIONS]; MAX_CHANNELS],
            states_f64: [[BiquadState::default(); MAX_SECTIONS]; MAX_CHANNELS],
            corner_hz: 0.0,
            active: false,
        };
        stage.configure(filter, sample_rate);
        stage
    }

    /// Replace the selection and/or sample rate. Control path only.
    pub fn configure(&mut self, filter: UltrasonicFilter, sample_rate: f32) {
        self.filter = filter;
        self.sample_rate = sample_rate;
        self.active = false;
        self.section_count = 0;
        self.corner_hz = 0.0;
        self.reset();

        let (order, corner) = match design(filter, sample_rate) {
            Some(design) => design,
            None => return,
        };
        // Nyquist guard: a corner at/above 45 % of fs cannot be a useful
        // low-pass. Report the stage inactive instead of installing a filter
        // that either clamps to Nyquist or does nothing.
        if corner <= 0.0 || corner >= 0.45 * sample_rate {
            return;
        }
        let qs = butterworth_qs(order);
        if qs.is_empty() {
            return;
        }
        for (index, &q) in qs.iter().enumerate() {
            self.sections_f32[index] = BiquadCoeffs::lowpass(sample_rate, corner, q);
            self.sections_f64[index] = BiquadCoeffs::<f32>::lowpass(sample_rate, corner, q).cast();
        }
        self.section_count = qs.len();
        self.corner_hz = corner;
        self.active = true;
    }

    /// Change the selection while keeping the sample rate.
    pub fn set_filter(&mut self, filter: UltrasonicFilter) {
        self.configure(filter, self.sample_rate);
    }

    /// Change the sample rate while keeping the selection.
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.configure(self.filter, sample_rate);
    }

    pub fn filter(&self) -> UltrasonicFilter {
        self.filter
    }

    /// The realized corner frequency, or 0 when inactive.
    pub fn corner_hz(&self) -> f32 {
        self.corner_hz
    }

    /// The realized filter order (0 when inactive).
    pub fn order(&self) -> usize {
        self.section_count * 2
    }

    /// Whether a filter is actually installed (selection not `Off` and the
    /// corner is below Nyquist).
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Deterministic latency. Minimum-phase biquads add none.
    pub fn latency_samples(&self) -> usize {
        0
    }

    pub fn reset(&mut self) {
        for channel in &mut self.states_f32 {
            for state in channel {
                *state = BiquadState::default();
            }
        }
        for channel in &mut self.states_f64 {
            for state in channel {
                *state = BiquadState::default();
            }
        }
    }

    /// Process one stereo sample pair. Disabled/inactive = bit-exact.
    #[inline]
    pub fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        if !self.active {
            return (left, right);
        }
        let mut l = left;
        let mut r = right;
        for section in 0..self.section_count {
            let coeffs = self.sections_f32[section];
            l = self.states_f32[0][section].process(l, &coeffs);
            r = self.states_f32[1][section].process(r, &coeffs);
        }
        (l, r)
    }

    /// Process a planar block on every supplied channel.
    pub fn process_block(&mut self, planes: &mut [&mut [f32]]) {
        if !self.active {
            return;
        }
        let channels = planes.len().min(MAX_CHANNELS);
        for (channel, plane) in planes.iter_mut().enumerate().take(channels) {
            for section in 0..self.section_count {
                let coeffs = self.sections_f32[section];
                let state = &mut self.states_f32[channel][section];
                for sample in plane.iter_mut() {
                    *sample = state.process(*sample, &coeffs);
                }
            }
        }
    }

    /// Process an interleaved block on every supplied channel.
    pub fn process_interleaved(&mut self, buffer: &mut [f32], channels: usize) {
        if !self.active || channels == 0 {
            return;
        }
        let channels = channels.min(MAX_CHANNELS);
        for frame in buffer.chunks_exact_mut(channels) {
            for (channel, sample) in frame.iter_mut().enumerate() {
                let mut value = *sample;
                for section in 0..self.section_count {
                    let coeffs = self.sections_f32[section];
                    value = self.states_f32[channel][section].process(value, &coeffs);
                }
                *sample = value;
            }
        }
    }

    /// f64 planar block (Quality mode).
    pub fn process_block_f64(&mut self, planes: &mut [&mut [f64]]) {
        if !self.active {
            return;
        }
        let channels = planes.len().min(MAX_CHANNELS);
        for (channel, plane) in planes.iter_mut().enumerate().take(channels) {
            for section in 0..self.section_count {
                let coeffs = self.sections_f64[section];
                let state = &mut self.states_f64[channel][section];
                for sample in plane.iter_mut() {
                    *sample = state.process(*sample, &coeffs);
                }
            }
        }
    }
}

/// Resolve a selection to `(order, corner_hz)` at the given rate.
///
/// `2x/4x/8x` are relative to the output clock family's base rate (44.1 kHz
/// or 48 kHz), matching HQPlayer's documented meaning.
fn design(filter: UltrasonicFilter, sample_rate: f32) -> Option<(usize, f32)> {
    if !sample_rate.is_finite() || sample_rate <= 0.0 {
        return None;
    }
    let base = if (sample_rate.round() as u32).is_multiple_of(48_000) {
        48_000.0
    } else {
        44_100.0
    };
    match filter {
        UltrasonicFilter::Off => None,
        UltrasonicFilter::Cut20k => Some((4, 20_000.0)),
        UltrasonicFilter::Cut30k => Some((2, 30_000.0)),
        UltrasonicFilter::Cut40k => Some((2, 40_000.0)),
        UltrasonicFilter::Cut50k => Some((2, 50_000.0)),
        UltrasonicFilter::X2 => Some((8, 2.0 * base)),
        UltrasonicFilter::X4 => Some((8, 4.0 * base)),
        UltrasonicFilter::X8 => Some((8, 8.0 * base)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gain_at(stage: &mut UltrasonicStage, freq: f32, sample_rate: f32) -> f32 {
        // Measure the steady-state magnitude through the public stereo entry
        // point: run a sine for long enough that the biquads settle, then take
        // the RMS ratio.
        stage.reset();
        let samples = (sample_rate / 10.0) as usize; // 100 ms
        let settle = samples / 2;
        let mut input_energy = 0.0f64;
        let mut output_energy = 0.0f64;
        for n in 0..samples {
            let x = (2.0 * std::f32::consts::PI * freq * n as f32 / sample_rate).sin();
            let (l, _) = stage.process(x, x);
            if n >= settle {
                input_energy += (x as f64) * (x as f64);
                output_energy += (l as f64) * (l as f64);
            }
        }
        ((output_energy / input_energy).sqrt()) as f32
    }

    #[test]
    fn off_is_a_bit_exact_passthrough() {
        let mut stage = UltrasonicStage::new(UltrasonicFilter::Off, 44_100.0);
        assert!(!stage.is_active());
        assert_eq!(stage.process(0.125, -0.5), (0.125, -0.5));
        let mut block = vec![0.1f32; 64];
        let before = block.clone();
        stage.process_block(&mut [&mut block]);
        assert_eq!(block, before);
    }

    #[test]
    fn twenty_kilohertz_passes_audio_and_rejects_ultrasonics() {
        let mut stage = UltrasonicStage::new(UltrasonicFilter::Cut20k, 192_000.0);
        assert!(stage.is_active());
        assert_eq!(stage.order(), 4);
        let audio = gain_at(&mut stage, 5_000.0, 192_000.0);
        assert!(audio > 0.98, "5 kHz gain was {audio}");
        let stop = gain_at(&mut stage, 60_000.0, 192_000.0);
        assert!(stop < 0.02, "60 kHz gain was {stop}");
    }

    #[test]
    fn a_corner_above_nyquist_reports_inactive() {
        // 50 kHz cannot exist under a 44.1 kHz Nyquist.
        let stage = UltrasonicStage::new(UltrasonicFilter::Cut50k, 44_100.0);
        assert!(!stage.is_active());
        assert_eq!(stage.corner_hz(), 0.0);
        assert_eq!(stage.order(), 0);
    }

    #[test]
    fn base_rate_multipliers_follow_the_output_family() {
        // 352.8 kHz (44.1 family): 2x = 88.2 kHz is well below Nyquist.
        let stage = UltrasonicStage::new(UltrasonicFilter::X2, 352_800.0);
        assert!(stage.is_active());
        assert!((stage.corner_hz() - 88_200.0).abs() < 1.0);
        assert_eq!(stage.order(), 8);

        // 768 kHz (48 family): 4x = 192 kHz.
        let stage48 = UltrasonicStage::new(UltrasonicFilter::X4, 768_000.0);
        assert!(stage48.is_active());
        assert!((stage48.corner_hz() - 192_000.0).abs() < 1.0);
        assert_eq!(stage48.filter(), UltrasonicFilter::X4);

        // At 352.8 kHz, 4x sits at Nyquist itself and must report inactive.
        let at_nyquist = UltrasonicStage::new(UltrasonicFilter::X4, 352_800.0);
        assert!(!at_nyquist.is_active());
    }

    #[test]
    fn interleaved_processing_matches_planar() {
        let mut planar = UltrasonicStage::new(UltrasonicFilter::Cut20k, 96_000.0);
        let mut interleaved = UltrasonicStage::new(UltrasonicFilter::Cut20k, 96_000.0);
        let mut left = vec![0.0f32; 256];
        let mut right = vec![0.0f32; 256];
        for n in 0..256 {
            left[n] = (n as f32 * 0.05).sin();
            right[n] = (n as f32 * 0.07).cos();
        }
        let mut buffer = Vec::with_capacity(512);
        for n in 0..256 {
            buffer.push(left[n]);
            buffer.push(right[n]);
        }
        planar.process_block(&mut [&mut left[..], &mut right[..]]);
        interleaved.process_interleaved(&mut buffer, 2);
        for n in 0..256 {
            assert!((buffer[n * 2] - left[n]).abs() < 1e-6);
            assert!((buffer[n * 2 + 1] - right[n]).abs() < 1e-6);
        }
    }
}
