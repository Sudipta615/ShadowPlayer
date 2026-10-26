//! Equalizer filter-bank implementation.

use super::headroom;
use super::types::{EqBand, EqBandParams, EqFilterType, MAX_EQ_BANDS};
use crate::dsp::biquad::{FilterType, SmoothedBiquad};
use crate::dsp::filter_design::{design, BandwidthMode, FilterDesign};
use crate::{buffer::AudioFrame, dsp::gain::GainProcessor};

/// Parametric EQ processor — configurable multi-band equaliser
///
/// # Precision model
///
/// The filter cores are native `f64` biquads ([`SmoothedBiquad<f64>`]), so
/// the **entire cascade runs in f64** regardless of the pipeline precision
/// mode. The f32 (Performance) entry points promote the signal to f64 **once**
/// at the EQ boundary, run every band and the tone shelves in f64, and demote
/// **once** at the exit — there are no per-band f32↔f64 round-trips, which
/// both wastes cycles and loses the benefit of a high-precision cascade.
///
/// # Headroom model
///
/// Headroom is a **static pre-EQ attenuation** (default 0 dB = unity).  It
/// intentionally does *not* behave like a compressor: the gain is fixed at
/// configuration time and applied before the filter chain, so it cannot
/// react to transients and cannot "pump".  The intended use is to reserve
/// headroom for the EQ curve's own boost — e.g. combine `set_headroom_db`
/// with [`ParametricEq::combined_max_gain_db`] so a +6 dB EQ curve still
/// leaves the limiter headroom to work with.
///
/// (Historical note: a *dynamic* headroom stage with a peak detector and
/// attack/release once lived inside this struct.  It made the EQ a second
/// compressor in the chain — duplicating the downstream `LookaheadLimiter`
/// with a non-lookahead detector that reacted after transients had passed —
/// and it was removed.  Overshoot protection is the limiter's job; the EQ
/// only ever applies linear, time-invariant gain.)
/// Largest band gain [`ParametricEq::set_band`] will store, in dB.
///
/// Matches `Biquad::validate_gain_db`'s bound, so the value the read-back
/// reports is always one the biquad can actually realize as finite
/// coefficients. Any tighter bound would make the stored parameter and the
/// audible one disagree.
const MAX_BAND_GAIN_DB: f32 = 48.0;

/// Multi-band parametric equalizer.
#[derive(Debug, Clone)]
pub struct ParametricEq {
    bands: Vec<EqBand>,
    bass_band: EqBand,
    treble_band: EqBand,
    sample_rate: f32,
    enabled: bool,
    preamp_db: f32,
    /// Pre-EQ gain, slewed. A setter only moves the *target*; the applied gain
    /// ramps toward it, so changing a preamp is a fade rather than a click.
    preamp: GainProcessor,
    post_gain_db: f32,
    /// Post-EQ gain, slewed. See [`Self::preamp`].
    post_gain: GainProcessor,
    /// Static pre-EQ headroom in dB (≤ 0). Applied once per sample as a
    /// fixed gain before the filter chain — see struct doc.
    headroom_db: f32,
    /// Pre-EQ headroom, slewed. See [`Self::preamp`].
    ///
    /// This one is why the ramp was needed at all: `auto_headroom` recomputes
    /// it from the curve on *any* band edit, so before the slew, moving a
    /// single EQ slider changed the headroom and stepped the whole signal.
    headroom: GainProcessor,
    /// When true, the static pre-EQ headroom is recomputed automatically from
    /// the curve's peak combined boost whenever the curve changes.
    auto_headroom: bool,
    /// The last manually configured headroom, restored when auto headroom is
    /// disabled. Kept separate so automatic updates never overwrite it.
    manual_headroom_db: f32,
    /// Preallocated f64 scratch for whole-block vectorized processing on the f32 path.
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
    /// What every band's `q` means. See [`ParametricEq::set_bandwidth_mode`].
    bandwidth_mode: BandwidthMode,
}

impl ParametricEq {
    /// Create a new 10-band parametric EQ with standard ISO frequencies
    pub fn default_10_band(sample_rate: f32) -> Self {
        let freqs = [
            31.25, 62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
        ];
        let types = [
            EqFilterType::LowShelf,
            EqFilterType::Peaking,
            EqFilterType::Peaking,
            EqFilterType::Peaking,
            EqFilterType::Peaking,
            EqFilterType::Peaking,
            EqFilterType::Peaking,
            EqFilterType::Peaking,
            EqFilterType::Peaking,
            EqFilterType::HighShelf,
        ];

        let bands = freqs
            .iter()
            .zip(types.iter())
            .map(|(&freq, &ft)| EqBand {
                params: EqBandParams {
                    filter_type: ft,
                    frequency: freq,
                    gain_db: 0.0,
                    q: 1.4,
                    enabled: false,
                },
                filter_left: SmoothedBiquad::new(),
                filter_right: SmoothedBiquad::new(),
            })
            .collect();

        let bass_band = EqBand {
            params: EqBandParams::lowshelf(100.0, 0.0, 1.0),
            filter_left: SmoothedBiquad::new(),
            filter_right: SmoothedBiquad::new(),
        };
        let treble_band = EqBand {
            params: EqBandParams::highshelf(7500.0, 0.0, 0.70),
            filter_left: SmoothedBiquad::new(),
            filter_right: SmoothedBiquad::new(),
        };

        Self {
            bands,
            bass_band,
            treble_band,
            sample_rate,
            enabled: false,
            preamp_db: 0.0,
            preamp: GainProcessor::unbounded(),
            post_gain_db: 0.0,
            post_gain: GainProcessor::unbounded(),
            headroom_db: 0.0,
            headroom: GainProcessor::unbounded(),
            auto_headroom: false,
            manual_headroom_db: 0.0,
            scratch_l: vec![0.0; crate::buffer::MAX_AUDIO_BLOCK_FRAMES],
            scratch_r: vec![0.0; crate::buffer::MAX_AUDIO_BLOCK_FRAMES],
            bandwidth_mode: BandwidthMode::EquivalentQ,
        }
    }

    /// Create a standard 31-band graphic/parametric EQ (1/3 octave ISO frequencies from 20 Hz to 20 kHz).
    pub fn standard_31_band(sample_rate: f32) -> Self {
        let freqs = [
            20.0, 25.0, 31.5, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 315.0,
            400.0, 500.0, 630.0, 800.0, 1000.0, 1250.0, 1600.0, 2000.0, 2500.0, 3150.0, 4000.0,
            5000.0, 6300.0, 8000.0, 10000.0, 12500.0, 16000.0, 20000.0,
        ];
        let bands = freqs
            .iter()
            .enumerate()
            .map(|(i, &freq)| {
                let ft = if i == 0 {
                    EqFilterType::LowShelf
                } else if i == freqs.len() - 1 {
                    EqFilterType::HighShelf
                } else {
                    EqFilterType::Peaking
                };
                EqBand {
                    params: EqBandParams {
                        filter_type: ft,
                        frequency: freq,
                        gain_db: 0.0,
                        q: 4.31, // Standard 1/3 octave Q
                        enabled: false,
                    },
                    filter_left: SmoothedBiquad::new(),
                    filter_right: SmoothedBiquad::new(),
                }
            })
            .collect();

        let bass_band = EqBand {
            params: EqBandParams::lowshelf(100.0, 0.0, 1.0),
            filter_left: SmoothedBiquad::new(),
            filter_right: SmoothedBiquad::new(),
        };
        let treble_band = EqBand {
            params: EqBandParams::highshelf(7500.0, 0.0, 0.70),
            filter_left: SmoothedBiquad::new(),
            filter_right: SmoothedBiquad::new(),
        };

        Self {
            bands,
            bass_band,
            treble_band,
            sample_rate,
            enabled: false,
            preamp_db: 0.0,
            preamp: GainProcessor::unbounded(),
            post_gain_db: 0.0,
            post_gain: GainProcessor::unbounded(),
            headroom_db: 0.0,
            headroom: GainProcessor::unbounded(),
            auto_headroom: false,
            manual_headroom_db: 0.0,
            scratch_l: vec![0.0; crate::buffer::MAX_AUDIO_BLOCK_FRAMES],
            scratch_r: vec![0.0; crate::buffer::MAX_AUDIO_BLOCK_FRAMES],
            bandwidth_mode: BandwidthMode::EquivalentQ,
        }
    }

    /// Create a high-resolution 64-band parametric EQ with true 1/6-octave
    /// spacing.
    ///
    /// Band `i` sits at `f_i = 20 Hz × 2^(i/6)` — an exact 1/6-octave ladder
    /// starting at 20 Hz, not an arbitrary log-spacing that merely *looks*
    /// like one.  The last band (i = 63) therefore lands at
    /// 20 × 2^10.5 ≈ 28 963 Hz, which is usable at sample rates ≥ ~58 kHz
    /// and harmless (disabled, unity gain) below that.
    ///
    /// The default Q is **derived from the bandwidth**, not invented:
    /// for a peaking filter with bandwidth `B` octaves,
    /// `Q = 1 / (2·sinh(ln(2)·B/2))`; for B = 1/6 octave that is ≈ 8.65.
    /// Each band remains individually configurable through `EqBandParams`
    /// (frequency / Q / gain / filter type) — the generator only supplies
    /// the mathematically-consistent default.
    pub fn standard_64_band(sample_rate: f32) -> Self {
        const BANDS: usize = 64;
        const OCTAVES_PER_BAND: f64 = 1.0 / 6.0;
        let f_min = 20.0f64;
        let q = 1.0 / (2.0 * (2.0f64.ln() * OCTAVES_PER_BAND / 2.0).sinh());
        let mut bands = Vec::with_capacity(BANDS);
        for i in 0..BANDS {
            let freq = f_min * (2.0f64).powf(i as f64 * OCTAVES_PER_BAND);
            let ft = if i == 0 {
                EqFilterType::LowShelf
            } else if i == BANDS - 1 {
                EqFilterType::HighShelf
            } else {
                EqFilterType::Peaking
            };
            bands.push(EqBand {
                params: EqBandParams {
                    filter_type: ft,
                    frequency: freq as f32,
                    gain_db: 0.0,
                    q: q as f32,
                    enabled: false,
                },
                filter_left: SmoothedBiquad::new(),
                filter_right: SmoothedBiquad::new(),
            });
        }

        let bass_band = EqBand {
            params: EqBandParams::lowshelf(100.0, 0.0, 1.0),
            filter_left: SmoothedBiquad::new(),
            filter_right: SmoothedBiquad::new(),
        };
        let treble_band = EqBand {
            params: EqBandParams::highshelf(7500.0, 0.0, 0.70),
            filter_left: SmoothedBiquad::new(),
            filter_right: SmoothedBiquad::new(),
        };

        Self {
            bands,
            bass_band,
            treble_band,
            sample_rate,
            enabled: false,
            preamp_db: 0.0,
            preamp: GainProcessor::unbounded(),
            post_gain_db: 0.0,
            post_gain: GainProcessor::unbounded(),
            headroom_db: 0.0,
            headroom: GainProcessor::unbounded(),
            auto_headroom: false,
            manual_headroom_db: 0.0,
            scratch_l: vec![0.0; crate::buffer::MAX_AUDIO_BLOCK_FRAMES],
            scratch_r: vec![0.0; crate::buffer::MAX_AUDIO_BLOCK_FRAMES],
            bandwidth_mode: BandwidthMode::EquivalentQ,
        }
    }

    /// Build a ParametricEq from a configured [`config::EqPreset`].
    pub fn from_preset(sample_rate: f32, preset: &config::EqPreset) -> Self {
        let num_bands = preset.bands.len().clamp(10, MAX_EQ_BANDS);
        let mut eq = Self::new(num_bands, sample_rate);
        eq.set_preamp_db(preset.preamp_db);
        for (i, b) in preset.bands.iter().enumerate().take(MAX_EQ_BANDS) {
            let ft = match b.filter_type {
                config::FilterType::Peaking => EqFilterType::Peaking,
                config::FilterType::LowShelf => EqFilterType::LowShelf,
                config::FilterType::HighShelf => EqFilterType::HighShelf,
                config::FilterType::LowPass => EqFilterType::LowPass,
                config::FilterType::HighPass => EqFilterType::HighPass,
                config::FilterType::Bandpass => EqFilterType::Bandpass,
                config::FilterType::Notch => EqFilterType::Notch,
                config::FilterType::AllPass => EqFilterType::AllPass,
            };
            eq.set_band(
                i,
                EqBandParams {
                    frequency: b.frequency,
                    gain_db: b.gain_db,
                    q: b.q,
                    filter_type: ft,
                    enabled: b.enabled,
                },
            );
        }
        eq.set_enabled(true);
        // A preset is a *starting state*, not a change to one. The gain stages
        // were constructed at unity and then given targets by the setters
        // above, so without this they would slew in over ~20 000 samples —
        // which for a preset carrying attenuation means the first 200 ms play
        // louder than the preset says. Construction snaps; a setter ramps.
        eq.snap_gain_stages();
        eq
    }

    /// Create a new EQ with all bands disabled
    pub fn new(num_bands: usize, sample_rate: f32) -> Self {
        let clamped_bands = num_bands.min(MAX_EQ_BANDS);
        let bands = (0..clamped_bands).map(|_| EqBand::new()).collect();
        let mut bass_band = EqBand::new();
        bass_band.params = EqBandParams::lowshelf(100.0, 0.0, 1.0);
        let mut treble_band = EqBand::new();
        treble_band.params = EqBandParams::highshelf(7500.0, 0.0, 0.70);

        Self {
            bands,
            bass_band,
            treble_band,
            sample_rate,
            enabled: false,
            preamp_db: 0.0,
            preamp: GainProcessor::unbounded(),
            post_gain_db: 0.0,
            post_gain: GainProcessor::unbounded(),
            headroom_db: 0.0,
            headroom: GainProcessor::unbounded(),
            auto_headroom: false,
            manual_headroom_db: 0.0,
            scratch_l: vec![0.0; crate::buffer::MAX_AUDIO_BLOCK_FRAMES],
            scratch_r: vec![0.0; crate::buffer::MAX_AUDIO_BLOCK_FRAMES],
            bandwidth_mode: BandwidthMode::EquivalentQ,
        }
    }

    /// Enable or disable the EQ
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.refresh_auto_headroom();
    }

    /// Whether EQ is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Current preamp gain in dB.
    pub fn preamp_db(&self) -> f32 {
        self.preamp_db
    }

    /// Set preamp gain in dB (applied before EQ).
    pub fn set_preamp_db(&mut self, gain_db: f32) {
        if !gain_db.is_finite() {
            log::warn!(
                "ParametricEq::set_preamp_db: non-finite value {}; ignoring",
                gain_db
            );
            return;
        }
        let clamped = gain_db.clamp(-30.0, 30.0);
        self.preamp_db = clamped;
        // Target only. The applied gain ramps toward it, so a preamp change
        // is a fade rather than a click.
        self.preamp
            .set_gain_unclamped(10.0_f32.powf(clamped / 20.0));
    }

    /// Set post-EQ gain in dB
    pub fn set_post_gain_db(&mut self, gain_db: f32) {
        if !gain_db.is_finite() {
            log::warn!(
                "ParametricEq::set_post_gain_db: non-finite value {}; ignoring",
                gain_db
            );
            return;
        }
        let clamped = gain_db.clamp(-30.0, 30.0);
        self.post_gain_db = clamped;
        // Target only — see `set_preamp_db`. The linear equivalent is still
        // computed here rather than per sample, so `process_f64` does no powf.
        self.post_gain
            .set_gain_unclamped(10.0_f32.powf(clamped / 20.0));
    }

    /// Current static pre-EQ headroom in dB (≤ 0; 0 = unity).
    ///
    /// This is the *requested* value. While the slew is in progress the applied
    /// gain lags it — see [`Self::applied_headroom_gain`].
    pub fn headroom_db(&self) -> f32 {
        self.headroom_db
    }

    /// Jump all three gain stages to their targets, cancelling any ramp.
    ///
    /// For construction and for explicit "no transition" paths only. A preset
    /// load, a track change and a device switch are boundaries the listener
    /// expects to be discontinuous; ramping across them would silence the
    /// first samples of the new material, or — worse, for a preset that
    /// attenuates, play the first 200 ms louder than the preset asked for.
    pub fn snap_gain_stages(&mut self) {
        self.preamp.gain = self.preamp.target_gain;
        self.post_gain.gain = self.post_gain.target_gain;
        self.headroom.gain = self.headroom.target_gain;
    }

    /// The pre-EQ headroom actually being applied, as a linear gain.
    ///
    /// Distinct from `headroom_db()` because that reports the setting and this
    /// reports the signal. With the slew in place the two differ for a few
    /// milliseconds after any change, and only this one describes what came
    /// out. A host that reports the applied figure will show the dip that the
    /// ramp is traversing rather than jumping to the new setting.
    pub fn applied_headroom_gain(&self) -> f32 {
        self.headroom.current_gain_t()
    }

    /// The preamp actually being applied, as a linear gain.
    pub fn applied_preamp_gain(&self) -> f32 {
        self.preamp.current_gain_t()
    }

    /// The post-EQ gain actually being applied, as a linear gain.
    pub fn applied_post_gain(&self) -> f32 {
        self.post_gain.current_gain_t()
    }

    /// Set the static pre-EQ headroom in dB (range [-60, 0]).
    ///
    /// This is a fixed attenuation applied *before* the filter chain — it is
    /// deliberately NOT a dynamic gain-reduction stage (see the struct doc).
    /// The default is 0 dB (unity).
    pub fn set_headroom_db(&mut self, headroom_db: f32) {
        if !headroom_db.is_finite() {
            log::warn!(
                "ParametricEq::set_headroom_db: non-finite value {}; ignoring",
                headroom_db
            );
            return;
        }
        let clamped = headroom_db.clamp(-60.0, 0.0);
        self.headroom_db = clamped;
        // Target only — see `set_preamp_db`. This is the hot one:
        // `refresh_auto_headroom` calls this on every band edit, so before the
        // ramp an ordinary EQ tweak stepped the whole signal.
        self.headroom
            .set_gain_unclamped(10.0_f32.powf(clamped / 20.0));
        // Track the manual value only when auto headroom is not the one
        // driving the knob; auto updates must not clobber it.
        if !self.auto_headroom {
            self.manual_headroom_db = clamped;
        }
    }

    /// Enable or disable automatic headroom management.
    ///
    /// When enabled, the static pre-EQ headroom is recomputed from the curve's
    /// own peak combined boost ([`Self::combined_max_gain_db`]) after every
    /// band, tone shelf, enable, or sample-rate change, so the downstream
    /// limiter never has to absorb the EQ's boost. Disabling restores the
    /// last manually configured headroom.
    pub fn set_auto_headroom(&mut self, enabled: bool) {
        if self.auto_headroom == enabled {
            return;
        }
        self.auto_headroom = enabled;
        if enabled {
            self.refresh_auto_headroom();
        } else {
            self.headroom_db = self.manual_headroom_db;
            self.headroom
                .set_gain_unclamped(10.0_f32.powf(self.manual_headroom_db / 20.0));
        }
    }

    /// Whether automatic headroom management is enabled.
    pub fn is_auto_headroom(&self) -> bool {
        self.auto_headroom
    }

    /// Recompute the static pre-EQ headroom from the current curve when
    /// automatic headroom is enabled; a no-op otherwise (preserving any
    /// manually configured headroom).
    fn refresh_auto_headroom(&mut self) {
        if !self.auto_headroom {
            return;
        }
        let boost = self.combined_max_gain_db(self.sample_rate);
        let headroom = if boost > 0.0 { -boost } else { 0.0 };
        self.set_headroom_db(headroom);
    }

    /// Process a stereo sample pair through the full EQ chain (f32).
    ///
    /// The signal is promoted to f64 once, cascaded through every band in
    /// f64, and demoted once — see the struct-level precision note. This is
    /// numerically identical to [`Self::process_f64`] up to the final cast.
    #[inline]
    pub fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        if !self.enabled {
            return (left, right);
        }
        let (l, r) = self.process_f64(left as f64, right as f64);
        (l as f32, r as f32)
    }

    /// Process a stereo sample pair in native f64 precision (Quality mode).
    #[inline]
    pub fn process_f64(&mut self, left: f64, right: f64) -> (f64, f64) {
        if !self.enabled {
            return (left, right);
        }

        // Advance both input-side ramps, then read them. Order matters only for
        // reproducibility: the preamp and the headroom are applied as a
        // product, so their order within the frame is not observable.
        let preamp = self.preamp.process_sample(1.0f32) as f64;
        let headroom = self.headroom.process_sample(1.0f32) as f64;
        let in_gain = preamp * headroom;
        let mut l = left * in_gain;
        let mut r = right * in_gain;

        for band in &mut self.bands {
            (l, r) = band.process_f64(l, r);
        }

        (l, r) = self.bass_band.process_f64(l, r);
        (l, r) = self.treble_band.process_f64(l, r);

        // `process_sample(1.0)` advances the ramp and returns the gain to
        // apply. The post-gain is the last stage, so a change made during this
        // frame takes effect from the next one rather than from this sample.
        let post_linear = self.post_gain.process_sample(1.0f32) as f64;
        (l * post_linear, r * post_linear)
    }

    /// Process a block of stereo frames through the full EQ chain in place.
    /// Promotes the block to f64 scratch once, runs the SSE2/vectorized multi-band
    /// block cascade, and demotes once back to f32.
    #[inline]
    pub fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        if !self.enabled {
            return;
        }
        let n = left.len().min(right.len()).min(self.scratch_l.len());
        if n == 0 {
            return;
        }
        let mut sl = std::mem::take(&mut self.scratch_l);
        let mut sr = std::mem::take(&mut self.scratch_r);
        for i in 0..n {
            sl[i] = f64::from(left[i]);
            sr[i] = f64::from(right[i]);
        }
        self.process_block_f64(&mut sl[..n], &mut sr[..n]);
        for i in 0..n {
            left[i] = sl[i] as f32;
            right[i] = sr[i] as f32;
        }
        self.scratch_l = sl;
        self.scratch_r = sr;
    }

    /// Process a block of stereo frames through the full EQ chain in native
    /// f64 precision. Hoists the whole-EQ enabled check out of the loop.
    #[inline]
    pub fn process_block_f64(&mut self, left: &mut [f64], right: &mut [f64]) {
        if !self.enabled {
            return;
        }
        let n = left.len().min(right.len());
        // The input gain can no longer be hoisted out of the loop: it is two
        // slewed processors, so it is a different value on every sample of the
        // block whenever a ramp is in progress. Hoisting it was correct only
        // while the gain was a cached constant, which is the thing that made an
        // EQ tweak click. The extra work is three flops and two multiplies per
        // sample, against a stage whose entire job is to be transparent.
        for i in 0..n {
            let preamp = self.preamp.process_sample(1.0f32) as f64
                * self.headroom.process_sample(1.0f32) as f64;
            left[i] *= preamp;
            right[i] *= preamp;
        }
        for band in &mut self.bands {
            band.process_block_f64(left, right);
        }
        self.bass_band.process_block_f64(left, right);
        self.treble_band.process_block_f64(left, right);
        for i in 0..n {
            let post = self.post_gain.process_sample(1.0f32) as f64;
            left[i] *= post;
            right[i] *= post;
        }
    }

    /// Helper to compute band coefficients matching the active bandwidth mode.
    #[inline]
    fn compute_band_coeffs(
        &self,
        sr: f32,
        kind: FilterType,
        freq: f32,
        gain_db: f32,
        q: f32,
    ) -> crate::dsp::biquad::BiquadCoeffs<f64> {
        match self.bandwidth_mode {
            BandwidthMode::EquivalentQ => kind.compute_coeffs(sr, freq, gain_db, q),
            BandwidthMode::NyquistOctaves => design(
                kind,
                FilterDesign::NyquistOctaves,
                sr,
                freq,
                gain_db,
                config::Bandwidth::Octaves(config::Bandwidth::octaves_for_q(q)),
            ),
        }
    }

    /// Collect the coefficients of every active band (parametric bands plus
    /// the bass/treble tone shelves) at the given sample rate.
    fn active_band_coeffs(&self, sr: f32) -> Vec<crate::dsp::biquad::BiquadCoeffs<f64>> {
        let mut coeffs: Vec<crate::dsp::biquad::BiquadCoeffs<f64>> = Vec::new();
        for band in &self.bands {
            let is_gain_active = band.params.gain_db.abs() > 0.001;
            let is_shape_active = matches!(
                band.params.filter_type,
                EqFilterType::LowPass
                    | EqFilterType::HighPass
                    | EqFilterType::Bandpass
                    | EqFilterType::Notch
            );
            if band.params.enabled && (is_gain_active || is_shape_active) {
                coeffs.push(self.compute_band_coeffs(
                    sr,
                    band.params.filter_type.to_filter_type(),
                    band.params.frequency,
                    band.params.gain_db,
                    band.params.q,
                ));
            }
        }
        if self.bass_band.params.enabled && self.bass_band.params.gain_db.abs() > 0.001 {
            coeffs.push(self.compute_band_coeffs(
                sr,
                self.bass_band.params.filter_type.to_filter_type(),
                self.bass_band.params.frequency,
                self.bass_band.params.gain_db,
                self.bass_band.params.q,
            ));
        }
        if self.treble_band.params.enabled && self.treble_band.params.gain_db.abs() > 0.001 {
            coeffs.push(self.compute_band_coeffs(
                sr,
                self.treble_band.params.filter_type.to_filter_type(),
                self.treble_band.params.frequency,
                self.treble_band.params.gain_db,
                self.treble_band.params.q,
            ));
        }
        coeffs
    }

    /// Compute the maximum combined magnitude response (in dB) of all cascaded EQ bands
    /// across the audible spectrum (20 Hz to 20 kHz) at the given sample rate.
    ///
    /// The estimate is deliberately conservative: DC and Nyquist are evaluated
    /// analytically (where shelving filters realise their full boost), and the
    /// interior is covered by a dense logarithmic sweep with local
    /// golden-section refinement so narrow, high-Q peaks are not missed.
    pub fn combined_max_gain_db(&self, sample_rate: f32) -> f32 {
        if !self.enabled {
            return 0.0;
        }
        let sr = if sample_rate > 0.0 {
            sample_rate
        } else {
            self.sample_rate
        };
        let min_f = 20.0_f64;
        let max_f = (sr as f64 * 0.49).min(20000.0);
        if max_f <= min_f {
            return 0.0;
        }
        let active_coeffs = self.active_band_coeffs(sr);
        headroom::max_curve_gain_db(&active_coeffs, sr as f64, min_f, max_f) as f32
    }

    /// Process an audio frame (alternative API)
    ///
    /// to avoid out-of-bounds access on `frame.channels[1]`.
    pub fn process_frame(&mut self, frame: &mut AudioFrame) {
        if frame.num_channels <= 1 {
            // Mono: process the single channel through both L and R filters
            // to maintain consistent filter state, then copy result back.
            let (l, _r) = self.process(frame.channels[0], frame.channels[0]);
            frame.channels[0] = l;
        } else {
            let (l, r) = self.process(frame.channels[0], frame.channels[1]);
            frame.channels[0] = l;
            frame.channels[1] = r;
        }
    }

    /// Set a band's parameters and update its coefficients
    /// Set band parameters.
    ///
    /// Validates and clamps like [`Self::set_bass_shelf`] does, and for the
    /// same reason. `update_coefficients` is not validation: it stores the
    /// target parameters verbatim and the biquad clamps non-finite *gains*
    /// later, at coefficient-computation time. That leaves `params.gain_db`
    /// holding a NaN, which is worse than it sounds — `combined_max_gain_db`
    /// sums the band gains, so one NaN makes it return NaN forever, and
    /// `refresh_auto_headroom` then feeds that NaN to `set_headroom_db`, which
    /// rejects it. The result is that auto-headroom silently stops responding
    /// to *every* band edit for the rest of the session, with no indication
    /// of why. Validating here keeps the stored parameters honest.
    pub fn set_band(&mut self, index: usize, params: EqBandParams) {
        let Some(band) = self.bands.get_mut(index) else {
            return;
        };
        apply_band_params(band, index, params);
        band.update_coefficients(self.sample_rate);
        self.refresh_auto_headroom();
    }

    /// Set a band's parameters with its width given in any of the spellings
    /// the file formats use (`Q`, `BW Oct`, `BW Hz`, `S`).
    ///
    /// The width is converted to the Q this EQ's biquads take
    /// (`config::eq_width`), after the gain is clamped the way
    /// [`Self::set_band`] clamps it — so an imported `S 0.5` shelf and a hand-
    /// typed `Q 0.49` land on the same filter. `EqBandParams::q` in `params`
    /// is ignored; the width argument is the width.
    pub fn set_band_width(&mut self, index: usize, params: EqBandParams, width: config::Bandwidth) {
        let mut params = params;
        let gain = if params.gain_db.is_finite() {
            params.gain_db.clamp(-MAX_BAND_GAIN_DB, MAX_BAND_GAIN_DB)
        } else {
            params.gain_db
        };
        params.q = width.to_q(params.frequency, gain);
        self.set_band(index, params);
    }

    /// Replace the complete band layout in one atomic update.
    ///
    /// This is the path every multi-band edit must take. Calling [`Self::set_band`]
    /// once per slider works, but each call re-runs the auto-headroom
    /// `combined_max_gain_db` sweep — O(bands²) work for a preset load, and a
    /// second source of truth (the final per-band refresh wins while the
    /// intermediate ones were wasted). Here the coefficients are installed for
    /// every band first and the headroom is recomputed exactly once.
    ///
    /// A band-count change resets the biquad state because the frequencies
    /// moved; a same-count update leaves the filters' history intact so slider
    /// drags keep riding the existing click-free parameter smoothing.
    pub fn set_band_layout(&mut self, params: &[EqBandParams]) {
        let n = params.len().min(MAX_EQ_BANDS);
        if self.bands.len() != n {
            self.bands.clear();
            self.bands.reserve(n);
            let mode = self.bandwidth_mode;
            self.bands.extend((0..n).map(|_| {
                let mut band = EqBand::new();
                band.set_bandwidth_mode(mode);
                band
            }));
        }
        for (index, params) in params.iter().copied().take(n).enumerate() {
            let band = &mut self.bands[index];
            apply_band_params(band, index, params);
            band.update_coefficients(self.sample_rate);
        }
        self.refresh_auto_headroom();
    }

    /// Grow or shrink the band bank while preserving the bands that remain.
    ///
    /// Used when a live parametric edit addresses a band beyond the current
    /// count. New bands start disabled at unity; shrinking drops the tail so
    /// stale filters cannot keep processing. The headroom is refreshed once
    /// for the whole resize.
    pub fn set_band_count(&mut self, count: usize) {
        let n = count.clamp(1, MAX_EQ_BANDS);
        if self.bands.len() == n {
            return;
        }
        let mode = self.bandwidth_mode;
        self.bands.resize_with(n, || {
            let mut band = EqBand::new();
            band.set_bandwidth_mode(mode);
            band
        });
        self.refresh_auto_headroom();
    }

    /// Get number of bands
    pub fn num_bands(&self) -> usize {
        self.bands.len()
    }

    /// Get band parameters
    pub fn band_params(&self, index: usize) -> Option<&EqBandParams> {
        self.bands.get(index).map(|b| &b.params)
    }

    /// The bass shelf's current gain in dB, after
    /// [`Self::set_bass_shelf`]'s ±30 dB clamp.
    ///
    /// Exposed for settings read-back — a host cannot otherwise learn what
    /// the engine holds after the clamp.
    pub fn bass_shelf_gain_db(&self) -> f32 {
        self.bass_band.params.gain_db
    }

    /// The treble shelf's current gain in dB, after
    /// [`Self::set_treble_shelf`]'s ±30 dB clamp.
    pub fn treble_shelf_gain_db(&self) -> f32 {
        self.treble_band.params.gain_db
    }

    /// Set bass shelf gain
    pub fn set_bass_shelf(&mut self, gain_db: f32) {
        if !gain_db.is_finite() {
            log::warn!(
                "ParametricEq::set_bass_shelf: non-finite value {}; ignoring",
                gain_db
            );
            return;
        }
        let clamped = gain_db.clamp(-30.0, 30.0);
        self.bass_band.params.gain_db = clamped;
        self.bass_band.params.enabled = clamped.abs() > 0.001;
        self.bass_band.update_coefficients(self.sample_rate);
        self.refresh_auto_headroom();
    }

    /// Set treble shelf gain
    pub fn set_treble_shelf(&mut self, gain_db: f32) {
        if !gain_db.is_finite() {
            log::warn!(
                "ParametricEq::set_treble_shelf: non-finite value {}; ignoring",
                gain_db
            );
            return;
        }
        let clamped = gain_db.clamp(-30.0, 30.0);
        self.treble_band.params.gain_db = clamped;
        self.treble_band.params.enabled = clamped.abs() > 0.001;
        self.treble_band.update_coefficients(self.sample_rate);
        self.refresh_auto_headroom();
    }

    /// Choose what every band's stored `q` means.
    ///
    /// The default, [`BandwidthMode::EquivalentQ`], is the cookbook Q the EQ
    /// has always used. [`BandwidthMode::NyquistOctaves`] reads each band's `q`
    /// as the low-frequency equivalent of an octave width and designs the band
    /// so that width holds up to Nyquist, which is what an Equalizer APO
    /// `BW Oct` curve means and what a 1/3-octave graphic EQ wants above
    /// ~10 kHz (a `Q`-parameterised band narrows there).
    ///
    /// Applies to the parametric bands and to the bass/treble tone shelves,
    /// and to any band created later, so it survives a layout change. This is
    /// a configuration switch: it swaps the filters the bands are running, so
    /// it snaps rather than ramps — set it while the stage is bypassed or
    /// between tracks. Returns `false` (and changes nothing) when the mode is
    /// already active, so a caller can skip the repaint.
    pub fn set_bandwidth_mode(&mut self, mode: BandwidthMode) -> bool {
        if self.bandwidth_mode == mode {
            return false;
        }
        self.bandwidth_mode = mode;
        for band in &mut self.bands {
            band.set_bandwidth_mode(mode);
        }
        self.bass_band.set_bandwidth_mode(mode);
        self.treble_band.set_bandwidth_mode(mode);
        // The curve did not change, but the response did: a graphic EQ's
        // octave bands reach higher than their Q-equivalents, so the headroom
        // derived from the curve has to be re-derived.
        self.refresh_auto_headroom();
        true
    }

    /// What this EQ's band `q` values mean.
    #[inline]
    pub fn bandwidth_mode(&self) -> BandwidthMode {
        self.bandwidth_mode
    }

    /// Update sample rate and recompute all coefficients
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        for band in &mut self.bands {
            band.update_coefficients(sample_rate);
        }
        self.bass_band.update_coefficients(sample_rate);
        self.treble_band.update_coefficients(sample_rate);
        self.refresh_auto_headroom();
    }

    /// Reset all bands
    pub fn reset(&mut self) {
        for band in &mut self.bands {
            band.reset();
        }
        self.bass_band.reset();
        self.treble_band.reset();
        // Note: headroom_db / headroom_linear are persistent settings, not
        // runtime state — no reset needed. Downstream LookaheadLimiter owns overshoot
        // protection.
    }
}

/// Install one band's parameters with the same per-field validation used by
/// [`ParametricEq::set_band`].
///
/// Kept separate so the single-band and whole-layout paths cannot drift on
/// which non-finite values they reject.
fn apply_band_params(band: &mut EqBand, index: usize, params: EqBandParams) {
    // Per-field, not whole-struct: a non-finite gain should not also throw
    // away a perfectly good frequency change in the same command.
    if params.gain_db.is_finite() {
        band.params.gain_db = params.gain_db.clamp(-MAX_BAND_GAIN_DB, MAX_BAND_GAIN_DB);
    } else {
        log::warn!(
            "ParametricEq::set_band: band {} non-finite gain {}; keeping {}",
            index,
            params.gain_db,
            band.params.gain_db
        );
    }
    if params.frequency.is_finite() {
        band.params.frequency = params.frequency;
    } else {
        log::warn!(
            "ParametricEq::set_band: band {} non-finite frequency {}; keeping {}",
            index,
            params.frequency,
            band.params.frequency
        );
    }
    if params.q.is_finite() {
        band.params.q = params.q;
    } else {
        log::warn!(
            "ParametricEq::set_band: band {} non-finite Q {}; keeping {}",
            index,
            params.q,
            band.params.q
        );
    }
    band.params.filter_type = params.filter_type;
    band.params.enabled = params.enabled;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Full-scale DC, for measuring a stage's gain directly.
    const RATE: f32 = 44_100.0;

    /// An enabled 10-band EQ. `process_f64` returns its input untouched when
    /// the stage is disabled, so a test that forgot to enable it would measure
    /// a passthrough and conclude the ramp never happens.
    fn enabled_eq() -> ParametricEq {
        let mut eq = ParametricEq::default_10_band(RATE);
        eq.set_enabled(true);
        eq
    }

    /// Feed DC through the EQ and return the applied gain once settled.
    fn settled_gain(eq: &mut ParametricEq) -> f64 {
        for _ in 0..100_000 {
            eq.process_f64(1.0, 1.0);
        }
        eq.process_f64(1.0, 1.0).0
    }

    /// The largest inter-sample step in a block, and the total change.
    fn step_profile(values: &[f64]) -> (f64, f64) {
        let mut max_step: f64 = 0.0;
        for w in values.windows(2) {
            max_step = max_step.max((w[1] - w[0]).abs());
        }
        (max_step, values[values.len() - 1] - values[0])
    }

    /// A preamp change must be a ramp, not a step.
    ///
    /// The bound is the one the one-pole actually implies: the first sample
    /// after the change moves by `slew_rate * |Δgain|`, and the profile only
    /// gets flatter from there. Asserting against `|Δgain| / 1` would pass for
    /// a ramp and fail for a step; asserting against the slew coefficient is
    /// what makes the test a statement about the ramp rather than about "it is
    /// not instantly equal".
    #[test]
    fn a_preamp_change_ramps_instead_of_stepping() {
        let mut eq = enabled_eq();
        // Settle at unity so the only change measured is the preamp's.
        assert!((settled_gain(&mut eq) - 1.0).abs() < 1e-4);

        // +12 dB is a 3.98x gain: a step here is a full-scale click.
        eq.set_preamp_db(12.0);
        let target = 10f64.powf(12.0 / 20.0);

        let mut out = Vec::with_capacity(20_000);
        for _ in 0..20_000 {
            out.push(eq.process_f64(1.0, 1.0).0);
        }
        let (max_step, _) = step_profile(&out);

        // One sample of a jump would be the whole |Δgain|; one sample of the
        // ramp is `slew_rate` of it, and the bound allows a little slack for
        // the f32/f64 hop in the multiplier.
        let bound = (target - 1.0) / 50.0;
        assert!(
            max_step < bound,
            "preamp ramp step {max_step:.6} exceeds {bound:.6} — the change is \
             landing as a step, not a ramp"
        );
        assert!(
            (out[out.len() - 1] - target).abs() < 1e-3,
            "the ramp must still reach the target: got {}, wanted {target}",
            out[out.len() - 1]
        );
    }

    /// Same bound, for the two gain stages that are not the preamp.
    ///
    /// Post-gain and headroom were written the same way preamp was, so testing
    /// only preamp would leave two of the three clicky.
    #[test]
    fn post_gain_and_headroom_also_ramp() {
        for (name, set) in [
            (
                "post_gain",
                ParametricEq::set_post_gain_db as fn(&mut ParametricEq, f32),
            ),
            (
                "headroom",
                ParametricEq::set_headroom_db as fn(&mut ParametricEq, f32),
            ),
        ] {
            let mut eq = enabled_eq();
            settled_gain(&mut eq);
            set(&mut eq, -12.0);
            let target = 10f64.powf(-12.0 / 20.0);

            let mut out = Vec::with_capacity(20_000);
            for _ in 0..20_000 {
                out.push(eq.process_f64(1.0, 1.0).0);
            }
            let (max_step, _) = step_profile(&out);
            let bound = (1.0 - target) / 50.0;
            assert!(
                max_step < bound,
                "{name} ramp step {max_step:.6} exceeds {bound:.6} — the change is \
                 landing as a step"
            );
            assert!(
                (out[out.len() - 1] - target).abs() < 1e-3,
                "{name}: the ramp must reach the target, got {}, wanted {target}",
                out[out.len() - 1]
            );
        }
    }

    /// The real reason the headroom needed a ramp: an ordinary EQ tweak used to
    /// change it, and therefore used to step the whole signal.
    ///
    /// Measured on the headroom itself rather than on the output, because a
    /// band edit moves *two* things at once — the biquad's own parameter
    /// smoothing and this stage's headroom — and a step measured on the output
    /// would be a blend of both, which is a number with no theory behind it.
    /// The preamp and post-gain tests above cover the output-signal case.
    #[test]
    fn a_band_edit_ramps_the_headroom_it_derives() {
        let mut eq = enabled_eq();
        eq.set_auto_headroom(true);
        eq.set_band(0, EqBandParams::peaking(200.0, 8.0, 1.0));
        eq.set_band(1, EqBandParams::peaking(3000.0, 6.0, 1.0));
        settled_gain(&mut eq);

        let before = eq.applied_headroom_gain();
        assert!(
            before < 0.9,
            "auto headroom should have attenuated: {before}"
        );

        // The ordinary case: a host moves one slider, which re-derives the
        // headroom from the new curve.
        eq.set_band(0, EqBandParams::peaking(200.0, 2.0, 1.0));

        let mut trace = Vec::with_capacity(20_000);
        for _ in 0..20_000 {
            // Advance the EQ so the headroom processor steps once per sample,
            // then read what it is actually applying.
            let _ = eq.process_f64(1.0, 1.0);
            trace.push(eq.applied_headroom_gain() as f64);
        }

        let (max_step, total) = step_profile(&trace);
        assert!(
            total.abs() > 1e-4,
            "the test is not exercising anything: the headroom did not move"
        );
        // The first sample of a one-pole moves by `slew_rate * |Δ|` and the
        // profile only flattens, so this is the worst case by construction.
        let bound = total.abs() / 50.0;
        assert!(
            max_step < bound,
            "the band edit moved the headroom in a step of {max_step:.6} (total \
             {total:.6}, bound {bound:.6})"
        );
    }
}
