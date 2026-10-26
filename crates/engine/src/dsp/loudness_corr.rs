// Loudness correction filter — mirrors EqualizerAPO `LoudnessCorrectionFilter`:
// a single reference-level / attenuation model that drives a low shelf around 75 Hz and
// a high shelf around 10 kHz, plus a preamp that keeps the combined boost from clipping.
// The present skeleton uses fixed anchor frequencies and the RBJ shelf form already shipped
// in `biquad::BiquadCoeffsF32`; the EqualizerAPO gain model (referenceLevel, referenceOffset,
// attenuation, and the exponential volDiff shaping) is borrowed in the `*_gain_db` helpers so
// the contour can later be driven from the system volume or a stored reference rather than a
// single scalar.
use crate::dsp::{biquad::BiquadCoeffsF32, smoother::PolynomialSmoother};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoudnessCorrectionCurve {
    /// Very quiet listening: strong bass and Treble lift.
    Phons70,
    /// Moderate listening level: gentler lift.
    Phons85,
    /// Flat / no contour.
    Flat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoudnessCorrectionMode {
    /// Contour plus one scalar strength (0..=1 mapped to 0..=1 for the existing anchor gains).
    Contour { curve: LoudnessCorrectionCurve, strength: f32 },
    Off,
}

/// Parameters mirroring EqualizerAPO's `FilterParameters` for loudness correction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoudnessCorrectionParams {
    pub active: bool,
    pub reference_level_db: f32,
    pub reference_offset_db: f32,
    pub attenuation: f32,
}

impl Default for LoudnessCorrectionParams {
    fn default() -> Self {
        Self {
            active: false,
            reference_level_db: 0.0,
            reference_offset_db: 0.0,
            attenuation: 1.0,
        }
    }
}

/// Smoothed loudness correction: low shelf near 75 Hz, high shelf near 10 kHz, plus a
/// preamp derived from the low-shelf gain so the combined shelf stack stays flat at the
/// reference level.
#[derive(Debug)]
pub struct LoudnessCorrection {
    low_gain_db: f32,
    high_gain_db: f32,
    preamp_db: f32,
    low_shelf: BiquadCoeffsF32,
    high_shelf: BiquadCoeffsF32,
    mode: LoudnessCorrectionMode,
    params: LoudnessCorrectionParams,
    low_smooth: PolynomialSmoother<f32, 4>,
    high_smooth: PolynomialSmoother<f32, 4>,
    sample_rate: u32,
}

impl LoudnessCorrection {
    pub fn new(sample_rate: u32) -> Self {
        let (low_gain, high_gain, preamp) = Self::shelf_gains_and_preamp(
            0.0_f32,
            0.0_f32,
            1.0_f32,
        );
        Self {
            low_gain_db: low_gain,
            high_gain_db: high_gain,
            preamp_db: preamp,
            low_shelf: BiquadCoeffsF32::low_shelf(75.0, 0.52, low_gain, sample_rate as f32),
            high_shelf: BiquadCoeffsF32::high_shelf(10000.0, 0.9, high_gain, sample_rate as f32),
            mode: LoudnessCorrectionMode::Off,
            params: LoudnessCorrectionParams::default(),
            low_smooth: PolynomialSmoother::new(0.33),
            high_smooth: PolynomialSmoother::new(0.33),
            sample_rate,
        }
    }

    pub fn reset(&mut self) {
        self.low_smooth.reset();
        self.high_smooth.reset();
    }

    pub fn set_mode(&mut self, mode: LoudnessCorrectionMode) {
        self.mode = mode;
        self.update_coeffs();
    }

    pub fn set_params(&mut self, params: LoudnessCorrectionParams) {
        self.params = params;
        self.update_coeffs();
    }

    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        self.sample_rate = sample_rate;
    }

    fn update_coeffs(&mut self) {
        let active = match self.mode {
            LoudnessCorrectionMode::Contour { .. } => true,
            LoudnessCorrectionMode::Off => false,
        };
        let (low_target, high_target, preamp_target) = if active {
            let strength = match self.mode {
                LoudnessCorrectionMode::Contour { strength, .. } => strength.max(0.0).min(1.0),
                LoudnessCorrectionMode::Off => 0.0,
            };
            Self::shelf_gains_and_preamp(
                self.params.reference_level_db,
                self.params.reference_offset_db,
                self.params.attenuation,
            )
            .into_tuple_with_strength(strength)
        } else {
            (0.0, 0.0, 0.0)
        };
        let low = self.low_smooth.smooth(low_target.max(-18.0).min(18.0));
        let high = self.high_smooth.smooth(high_target.max(-18.0).min(18.0));
        self.low_gain_db = low;
        self.high_gain_db = high;
        self.preamp_db = preamp_target.max(-12.0).min(12.0);
        self.low_shelf = BiquadCoeffsF32::low_shelf(75.0, 0.52, low, self.sample_rate as f32);
        self.high_shelf = BiquadCoeffsF32::high_shelf(10000.0, 0.9, high, self.sample_rate as f32);
    }

    fn shelf_gains_and_preamp(
        reference_level_db: f32,
        reference_offset_db: f32,
        attenuation: f32,
    ) -> (f32, f32, f32) {
        let vol_diff = reference_level_db - reference_offset_db;
        let low_gain = if vol_diff > 0.0 {
            vol_diff * 0.55 / (1.0 - 0.55) * attenuation
        } else if vol_diff < 0.0 {
            vol_diff * 0.55 * (vol_diff / 90.0).exp() * attenuation
        } else {
            0.0
        };
        let high_gain = if vol_diff > 0.0 {
            vol_diff * 0.225 * (-vol_diff / 100.0).exp() * attenuation
        } else if vol_diff < 0.0 {
            vol_diff * 0.175 * (vol_diff / 80.0).exp() * attenuation
        } else {
            0.0
        };
        // EqualizerAPO derives the preamp from the low shelf only.
        let preamp = if low_gain > 0.0 { -low_gain } else { 0.0 };
        (low_gain, high_gain, preamp)
    }

    /// Apply the current smoothed shelf gains in the RBJ form.
    pub fn process_block_f32(&mut self, left: &mut [f32], right: &mut [f32]) {
        if !self.is_active() {
            return;
        }
        let n = left.len().min(right.len());
        for i in 0..n {
            let lo = self.low_shelf.apply_1p(left[i]);
            left[i] = self.preamp_db.db_gain() * lo;
            let hi = self.high_shelf.apply_1p(left[i]);
            left[i] = hi;
            let lor = self.low_shelf.apply_1p(right[i]);
            right[i] = self.preamp_db.db_gain() * lor;
            let hir = self.high_shelf.apply_1p(right[i]);
            right[i] = hir;
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self.mode, LoudnessCorrectionMode::Contour { .. })
    }

    pub fn params(&self) -> LoudnessCorrectionParams {
        self.params
    }
}

trait IntoTupleWithStrength {
    fn into_tuple_with_strength(self, strength: f32) -> (f32, f32, f32);
}

impl IntoTupleWithStrength for (f32, f32, f32) {
    fn into_tuple_with_strength(self, strength: f32) -> (f32, f32, f32) {
        let (a, b, pre) = self;
        (a * strength, b * strength, pre * strength)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shelf_gains_and_preamp_refl_is_flat_at_reference() {
        let (low, high, pre) = LoudnessCorrection::shelf_gains_and_preamp(0.0, 0.0, 1.0);
        assert!((low.abs() < 1e-6 && high.abs() < 1e-6 && pre.abs() < 1e-6));
    }

    #[test]
    fn low_gain_positive_produces_preamp_attenuation() {
        let (low, high, pre) = LoudnessCorrection::shelf_gains_and_preamp(10.0, 0.0, 1.0);
        assert!(low > 0.0);
        assert!((pre + low).abs() < 1e-6);
    }

    #[test]
    fn inactive_mode_is_inert() {
        let mut lc = LoudnessCorrection::new(48000);
        lc.set_mode(LoudnessCorrectionMode::Off);
        assert!(!lc.is_active());
        lc.set_mode(LoudnessCorrectionMode::Contour { curve: LoudnessCorrectionCurve::Phons70, strength: 1.0 });
        assert!(lc.is_active());
    }

    #[test]
    fn inactive_process_leaves_buffer_unchanged() {
        let mut lc = LoudnessCorrection::new(48000);
        lc.set_mode(LoudnessCorrectionMode::Off);
        let mut left = vec![0.5f32; 16];
        let mut right = vec![-0.5f32; 16];
        lc.process_block_f32(&mut left, &mut right);
        assert!(left.iter().copied().all(|x| (x - 0.5).abs() < 1e-6));
        assert!(right.iter().copied().all(|x| (x + 0.5).abs() < 1e-6));
    }
}
