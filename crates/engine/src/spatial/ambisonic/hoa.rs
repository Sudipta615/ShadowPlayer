//! Higher-Order Ambisonic (HOA) advanced pipeline.
//!
//! Provides dedicated HOA encoding, decoding, near-field distance compensation (NFC),
//! and per-speaker propagation delay compensation for ambisonic orders 1 through 9.

use super::basis::{channel_count, MAX_AMBISONIC_ORDER};
use super::decoder::{AmbisonicDecoder, DecoderPolicy};
use crate::dsp::biquad::{BiquadCoeffsF32, BiquadStateF32};
use crate::spatial::math::Vec3;
use crate::spatial::render::RenderError;
use crate::spatial::speaker::SpeakerLayout;

/// HOA decoding optimization policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HoaDecoding {
    /// Basic sampling decoder (pin-pointing without energy normalization).
    Basic,
    /// Energy-vector maximized (Max-rE) for optimal high-frequency localization.
    #[default]
    MaxRe,
    /// In-phase decoding: zero negative lobes, optimal for narrow sweet-spots.
    InPhase,
    /// Energy-velocity optimized dual-band transition.
    EnergyVelocityOptimized,
}

impl From<HoaDecoding> for DecoderPolicy {
    fn from(dec: HoaDecoding) -> Self {
        match dec {
            HoaDecoding::Basic => DecoderPolicy::Basic,
            HoaDecoding::MaxRe | HoaDecoding::EnergyVelocityOptimized => DecoderPolicy::MaxRe,
            HoaDecoding::InPhase => DecoderPolicy::InPhase,
        }
    }
}

/// Configuration for HOA rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HoaConfig {
    /// Ambisonic order (1 to 9).
    pub order: u8,
    /// Decoding policy.
    pub decoding: HoaDecoding,
}

impl Default for HoaConfig {
    fn default() -> Self {
        Self {
            order: 3,
            decoding: HoaDecoding::MaxRe,
        }
    }
}

/// Near-field distance compensation (NFC) filter bank for spherical wave expansion.
#[derive(Debug, Clone)]
pub struct NearFieldCompensation {
    order: u8,
    filters: Vec<[BiquadCoeffsF32; 4]>,
    states: Vec<[BiquadStateF32; 4]>,
}

impl NearFieldCompensation {
    /// Precompute NFC filters for speaker layout distance and sample rate.
    pub fn new(order: u8, speaker_distance_m: f32, sample_rate: f32) -> Self {
        let o = order.min(MAX_AMBISONIC_ORDER);
        let n_ch = channel_count(o);
        let coeffs = crate::spatial::nearfield::hoa_distance_encode_filter(
            o,
            speaker_distance_m,
            sample_rate,
        );

        Self {
            order: o,
            filters: vec![coeffs; n_ch],
            states: vec![[BiquadStateF32::default(); 4]; n_ch],
        }
    }

    /// Apply near-field compensation to an ambisonic bus frame in place.
    /// Realtime-safe: zero allocations.
    #[inline]
    pub fn process_frame(&mut self, frame: &mut [f32]) {
        let n = channel_count(self.order).min(frame.len());
        for (ch, sample) in frame[..n].iter_mut().enumerate() {
            let mut s = *sample;
            for (coeff, state) in self.filters[ch].iter().zip(self.states[ch].iter_mut()) {
                s = state.process(s, coeff);
            }
            *sample = s;
        }
    }
}

/// Optional per-speaker delay compensation (aligns physical speaker arrival times).
#[derive(Debug, Clone)]
pub struct PerSpeakerDelay {
    pub delays_samples: [u32; 64],
}

impl Default for PerSpeakerDelay {
    fn default() -> Self {
        Self {
            delays_samples: [0; 64],
        }
    }
}

/// Dedicated Higher-Order Ambisonic encoder.
#[derive(Debug, Clone)]
pub struct HoaEncoder {
    order: u8,
}

impl HoaEncoder {
    pub fn new(order: u8) -> Self {
        Self {
            order: order.min(MAX_AMBISONIC_ORDER),
        }
    }

    pub fn order(&self) -> u8 {
        self.order
    }

    pub fn channels(&self) -> usize {
        channel_count(self.order)
    }

    /// Encode direction and gain into `ambi_bus`. Allocation-free.
    #[inline]
    pub fn encode(&self, dir: Vec3, gain: f32, ambi_bus: &mut [f32]) {
        super::encode::encode_plane_wave_n(self.order, dir, gain, ambi_bus);
    }
}

/// Dedicated Higher-Order Ambisonic decoder.
#[derive(Debug)]
pub struct HoaDecoder {
    config: HoaConfig,
    decoder: AmbisonicDecoder,
}

impl HoaDecoder {
    pub fn new(config: HoaConfig) -> Self {
        let policy = config.decoding.into();
        Self {
            config,
            decoder: AmbisonicDecoder::with_order(policy, config.order),
        }
    }

    pub fn config(&self) -> &HoaConfig {
        &self.config
    }

    pub fn prepare(&mut self, layout: &SpeakerLayout, sample_rate: u32) -> Result<(), RenderError> {
        self.decoder.prepare(layout, sample_rate)
    }

    pub fn decode(&self, ambi_bus: &[f32], frames: usize, out: &mut [f32]) {
        self.decoder.process_bus(ambi_bus, frames, out);
    }

    pub fn channels(&self) -> usize {
        self.decoder.channels()
    }

    pub fn bus_width(&self) -> usize {
        self.decoder.bus_width()
    }
}

/// Dual-band Higher-Order Ambisonic Decoder.
///
/// Implements dual-band frequency-dependent Ambisonic decoding:
/// - Low-frequency band (< crossover): Velocity-matched decoding (Basic / Mode-Matching)
///   for accurate ITD phase wavefront reconstruction.
/// - High-frequency band (>= crossover): Energy-matched decoding (Max-rE windowing)
///   maximizing the energy vector to eliminate phantom-source comb filtering and maximize ILD contrast.
/// - Realtime-safe: zero allocations during `decode`.
#[derive(Debug)]
pub struct DualBandHoaDecoder {
    order: u8,
    crossover_hz: f32,
    sample_rate: f32,
    lp_coeffs: BiquadCoeffsF32,
    hp_coeffs: BiquadCoeffsF32,
    lp_states: Vec<BiquadStateF32>,
    hp_states: Vec<BiquadStateF32>,
    low_decoder: AmbisonicDecoder,
    high_decoder: AmbisonicDecoder,
    scratch_bus_lp: Vec<f32>,
    scratch_bus_hp: Vec<f32>,
    scratch_out_lp: Vec<f32>,
    scratch_out_hp: Vec<f32>,
    prepared: bool,
}

impl DualBandHoaDecoder {
    /// Construct a new dual-band HOA decoder. Default crossover is ~500 Hz.
    pub fn new(order: u8, crossover_hz: f32) -> Self {
        let o = order.min(MAX_AMBISONIC_ORDER);
        Self {
            order: o,
            crossover_hz: crossover_hz.clamp(200.0, 1500.0),
            sample_rate: 48_000.0,
            lp_coeffs: BiquadCoeffsF32::lowpass(48_000.0, 500.0, 0.707),
            hp_coeffs: BiquadCoeffsF32::highpass(48_000.0, 500.0, 0.707),
            lp_states: Vec::new(),
            hp_states: Vec::new(),
            low_decoder: AmbisonicDecoder::with_order(DecoderPolicy::Basic, o),
            high_decoder: AmbisonicDecoder::with_order(DecoderPolicy::MaxRe, o),
            scratch_bus_lp: Vec::new(),
            scratch_bus_hp: Vec::new(),
            scratch_out_lp: Vec::new(),
            scratch_out_hp: Vec::new(),
            prepared: false,
        }
    }

    /// Prepare decoders, crossover filter banks, and preallocate scratch buffers. Control path.
    pub fn prepare(&mut self, layout: &SpeakerLayout, sample_rate: u32) -> Result<(), RenderError> {
        let sr = sample_rate.max(1) as f32;
        self.sample_rate = sr;
        self.lp_coeffs = BiquadCoeffsF32::lowpass(sr, self.crossover_hz, 0.707);
        self.hp_coeffs = BiquadCoeffsF32::highpass(sr, self.crossover_hz, 0.707);

        let ch = channel_count(self.order);
        self.lp_states = vec![BiquadStateF32::default(); ch];
        self.hp_states = vec![BiquadStateF32::default(); ch];

        self.low_decoder.prepare(layout, sample_rate)?;
        self.high_decoder.prepare(layout, sample_rate)?;

        let n_spk = layout.speakers.len();
        const MAX_FRAMES: usize = crate::buffer::MAX_AUDIO_BLOCK_FRAMES;
        self.scratch_bus_lp = vec![0.0f32; ch * MAX_FRAMES];
        self.scratch_bus_hp = vec![0.0f32; ch * MAX_FRAMES];
        self.scratch_out_lp = vec![0.0f32; n_spk * MAX_FRAMES];
        self.scratch_out_hp = vec![0.0f32; n_spk * MAX_FRAMES];

        self.prepared = true;
        Ok(())
    }

    pub fn prepared(&self) -> bool {
        self.prepared
    }

    pub fn order(&self) -> u8 {
        self.order
    }

    pub fn crossover_hz(&self) -> f32 {
        self.crossover_hz
    }

    pub fn channels(&self) -> usize {
        self.low_decoder.channels()
    }

    pub fn bus_width(&self) -> usize {
        channel_count(self.order)
    }

    /// Decode an interleaved ambisonic bus using dual-band energy-velocity optimization.
    /// Realtime-safe: strictly zero allocations.
    pub fn decode(&mut self, ambi_bus: &[f32], frames: usize, out: &mut [f32]) {
        if !self.prepared || frames == 0 {
            return;
        }

        let ch = channel_count(self.order);
        let bus_len = frames * ch;
        if ambi_bus.len() < bus_len {
            return;
        }

        let n_spk = self.channels();
        let out_len = frames * n_spk;
        if out.len() < out_len {
            return;
        }

        // Apply crossover filter bank to separate low- and high-frequency ambisonic bus frames
        for f in 0..frames {
            let offset = f * ch;
            for c in 0..ch {
                let sample = ambi_bus[offset + c];
                let lp = self.lp_states[c].process(sample, &self.lp_coeffs);
                let hp = self.hp_states[c].process(sample, &self.hp_coeffs);
                self.scratch_bus_lp[offset + c] = lp;
                self.scratch_bus_hp[offset + c] = hp;
            }
        }

        self.scratch_out_lp[..out_len].fill(0.0);
        self.scratch_out_hp[..out_len].fill(0.0);

        self.low_decoder.process_bus(
            &self.scratch_bus_lp[..bus_len],
            frames,
            &mut self.scratch_out_lp[..out_len],
        );
        self.high_decoder.process_bus(
            &self.scratch_bus_hp[..bus_len],
            frames,
            &mut self.scratch_out_hp[..out_len],
        );

        // Recombine the low- and high-frequency decoded signals
        for (i, sample) in out[..out_len].iter_mut().enumerate() {
            *sample += self.scratch_out_lp[i] + self.scratch_out_hp[i];
        }
    }
}
