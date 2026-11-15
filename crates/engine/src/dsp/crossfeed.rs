use config::CrossfeedProfile;

use super::biquad::{BiquadCoeffs, BiquadState};

/// Headphone Crossfeed DSP node
/// Reduces listening fatigue on hard-panned stereo tracks by blending
/// low-pass filtered and delayed audio from the opposite channel.
pub struct Crossfeed {
    enabled: bool,
    level: f32,
    profile: CrossfeedProfile,
    custom_freq: f32,
    custom_q: f32,
    custom_delay_ms: f32,
    sample_rate: f32,

    /// Biquad coefficients for the crossfeed low-pass filter
    coeffs: BiquadCoeffs<f64>,
    /// State for the Left-to-Right crossfeed filter
    state_lr: BiquadState<f64>,
    /// State for the Right-to-Left crossfeed filter
    state_rl: BiquadState<f64>,

    /// Delay ring buffer for Left-to-Right
    delay_lr: Vec<f64>,
    /// Delay ring buffer for Right-to-Left
    delay_rl: Vec<f64>,
    delay_pos: usize,
    delay_len: usize,
}

impl Crossfeed {
    pub fn new(sample_rate: f32) -> Self {
        let mut cf = Self {
            enabled: false,
            level: 1.0,
            profile: CrossfeedProfile::Bauer,
            custom_freq: 700.0,
            custom_q: 0.707,
            custom_delay_ms: 0.3,
            sample_rate,
            coeffs: BiquadCoeffs::identity(),
            state_lr: BiquadState::default(),
            state_rl: BiquadState::default(),
            delay_lr: vec![0.0; Self::ring_capacity(sample_rate)],
            delay_rl: vec![0.0; Self::ring_capacity(sample_rate)],
            delay_pos: 0,
            delay_len: 1,
        };
        cf.update_params();
        cf
    }

    /// The longest inter-aural delay any profile can ask for, in
    /// milliseconds. Every profile is a fraction of a millisecond; the cap is
    /// generous so a custom value never has to reallocate the ring.
    const MAX_DELAY_MS: f32 = 5.0;

    /// Capacity of the delay rings for `sample_rate`, sized once for the
    /// largest supported delay.
    fn ring_capacity(sample_rate: f32) -> usize {
        ((Self::MAX_DELAY_MS / 1000.0) * sample_rate.max(1.0)).ceil() as usize + 2
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// The active acoustic profile.
    ///
    /// The custom profile's three parameters are meaningful only when this
    /// returns `CrossfeedProfile::Custom`; they are read separately by
    /// [`Self::custom_params`].
    pub fn profile(&self) -> CrossfeedProfile {
        self.profile
    }

    /// The custom profile's `(frequency_hz, q, delay_ms)`.
    pub fn custom_params(&self) -> (f32, f32, f32) {
        (self.custom_freq, self.custom_q, self.custom_delay_ms)
    }

    /// Fixed delay-line latency of the crossfeed network in milliseconds
    /// (0 when disabled). The low-pass biquads add phase delay too, but the
    /// *buffer* delay is the deterministic term worth accounting for in the
    /// graph latency model.
    pub fn latency_ms(&self) -> f32 {
        if !self.enabled || self.sample_rate <= 0.0 {
            0.0
        } else {
            self.delay_len as f32 / self.sample_rate * 1000.0
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.enabled = enabled;
            if !enabled {
                self.reset();
            }
        }
    }

    pub fn set_profile(&mut self, profile: CrossfeedProfile) {
        if self.profile != profile {
            self.profile = profile;
            self.update_params();
        }
    }

    pub fn set_level(&mut self, level: f32) {
        self.level = level.clamp(0.0, 1.0);
    }

    pub fn set_custom_params(&mut self, freq: f32, q: f32, delay_ms: f32) {
        self.custom_freq = freq;
        self.custom_q = q;
        self.custom_delay_ms = delay_ms;
        if self.profile == CrossfeedProfile::Custom {
            self.update_params();
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if (self.sample_rate - sample_rate).abs() > 0.01 {
            self.sample_rate = sample_rate;
            // A rate change is the one case that must re-size the ring.
            let cap = Self::ring_capacity(sample_rate);
            if self.delay_lr.len() != cap {
                self.delay_lr = vec![0.0; cap];
                self.delay_rl = vec![0.0; cap];
            }
            self.update_params();
            self.reset();
        }
    }

    fn update_params(&mut self) {
        let (freq, q, delay_ms) = match self.profile {
            CrossfeedProfile::Bauer => (700.0, 0.5, 0.3),
            CrossfeedProfile::ChuMoy => (700.0, 0.707, 0.25),
            CrossfeedProfile::Jmeier => (600.0, 0.6, 0.35),
            CrossfeedProfile::Custom => {
                let freq = if self.custom_freq <= 0.0 || !self.custom_freq.is_finite() {
                    log::warn!(
                        "Crossfeed custom_freq {} is invalid; clamping to 700.0",
                        self.custom_freq
                    );
                    700.0
                } else {
                    self.custom_freq
                };
                let q = if self.custom_q <= 0.0 || !self.custom_q.is_finite() {
                    log::warn!(
                        "Crossfeed custom_q {} is invalid; clamping to 0.707",
                        self.custom_q
                    );
                    0.707
                } else {
                    self.custom_q
                };
                let delay_ms = if self.custom_delay_ms < 0.0 || !self.custom_delay_ms.is_finite() {
                    log::warn!(
                        "Crossfeed custom_delay_ms {} is invalid; clamping to 0.3",
                        self.custom_delay_ms
                    );
                    0.3
                } else {
                    self.custom_delay_ms
                };
                (freq, q, delay_ms)
            }
        };

        self.coeffs = BiquadCoeffs::lowpass(self.sample_rate, freq, q);

        // The delay is a *read offset* into a ring that was sized for the
        // largest supported delay, not a buffer length. Re-sizing the ring
        // here meant every profile change or custom-delay drag allocated two
        // `Vec`s and zeroed the audio history on the queued control path — a
        // click on every click. A longer delay now just reads further back.
        let cap = self.delay_lr.len().max(1);
        let want = ((delay_ms / 1000.0) * self.sample_rate) as usize;
        self.delay_len = want.clamp(1, cap.saturating_sub(1).max(1));
    }

    #[inline]
    pub fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        if !self.enabled || self.level <= 0.001 {
            return (left, right);
        }

        let (ol, or_) = self.process_f64(left as f64, right as f64);
        (ol as f32, or_ as f32)
    }

    /// Process a stereo sample pair in native f64 precision.
    #[inline]
    pub fn process_f64(&mut self, left: f64, right: f64) -> (f64, f64) {
        if !self.enabled || self.level <= 0.001 {
            return (left, right);
        }

        // Apply low-pass filter to each channel to create the crossfeed signal
        let filtered_to_right = self.state_lr.process(left, &self.coeffs);
        let filtered_to_left = self.state_rl.process(right, &self.coeffs);

        // Process delay line: read `delay_len` frames back, then overwrite the
        // current slot with the newly filtered opposite channel.
        let cap = self.delay_lr.len();
        let read_pos = if self.delay_pos >= self.delay_len {
            self.delay_pos - self.delay_len
        } else {
            self.delay_pos + cap - self.delay_len
        };
        let cross_to_right = self.delay_lr[read_pos];
        let cross_to_left = self.delay_rl[read_pos];

        self.delay_lr[self.delay_pos] = filtered_to_right;
        self.delay_rl[self.delay_pos] = filtered_to_left;

        self.delay_pos += 1;
        if self.delay_pos >= cap {
            self.delay_pos = 0;
        }

        // Blend the crossfeed signals.
        //
        // The direct path stays at unity: crossfeed narrows the image by
        // *adding* a filtered, delayed opposite channel, it does not attenuate
        // the near one. The previous `direct = 1 − 0.5·level` took 6 dB off the
        // direct signal at level 1.0, so enabling crossfeed made the mix
        // quieter and duller instead of narrower. The cross term is then
        // normalised by `√(1 + cross²)`, which keeps total power constant for
        // uncorrelated programme (and mono within ≈+2.6 dB of coherent bass
        // summing, which is the intended bass lift of the classic networks).
        let cross_level = 0.5 * (self.level as f64);
        let norm = 1.0 / (1.0 + cross_level * cross_level).sqrt();

        let out_l = (left + cross_to_left * cross_level) * norm;
        let out_r = (right + cross_to_right * cross_level) * norm;

        (out_l, out_r)
    }

    /// Process a block of stereo frames in place. Hoists the enabled/level
    /// checks out of the per-frame loop.
    #[inline]
    pub fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        if !self.enabled || self.level <= 0.001 {
            return;
        }
        let n = left.len().min(right.len());
        for i in 0..n {
            let (ol, or_) = self.process_f64(left[i] as f64, right[i] as f64);
            left[i] = ol as f32;
            right[i] = or_ as f32;
        }
    }

    /// Process a block of stereo frames in native f64 precision. Hoists the
    /// enabled/level checks out of the per-frame loop.
    #[inline]
    pub fn process_block_f64(&mut self, left: &mut [f64], right: &mut [f64]) {
        if !self.enabled || self.level <= 0.001 {
            return;
        }
        let n = left.len().min(right.len());
        for i in 0..n {
            let (ol, or_) = self.process_f64(left[i], right[i]);
            left[i] = ol;
            right[i] = or_;
        }
    }

    pub fn reset(&mut self) {
        self.state_lr.reset();
        self.state_rl.reset();
        self.delay_lr.fill(0.0);
        self.delay_rl.fill(0.0);
        self.delay_pos = 0;
    }
}
