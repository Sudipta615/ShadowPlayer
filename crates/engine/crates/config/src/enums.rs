//! Configuration enums shared across the engine's public API.

use serde::{Deserialize, Serialize};

/// Requested output transport. These are requests, not guarantees: the
/// engine reports the negotiated backend/access state and rejects an
/// exclusive request when the selected backend cannot provide it under the
/// active fallback policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AudioBackend {
    /// Let the platform choose its default shared output.
    #[default]
    Auto,
    /// Native WASAPI exclusive mode when the native backend is enabled;
    /// cpal's WASAPI path is shared-only and cannot satisfy this request.
    ExclusiveWasapi,
    /// Direct ALSA `hw:`/`plughw:` access on Linux.
    ExclusiveAlsa,
    /// Native CoreAudio hog mode; cpal itself does not implement hog mode.
    ExclusiveCoreAudioHog,
    /// ASIO direct output when the optional ASIO feature is compiled in.
    ExclusiveAsio,
    /// Linux PipeWire pro-audio output.
    ///
    /// **Declared, not implemented.** The engine has no PipeWire
    /// integration: selecting this requests a backend no build can open.
    /// `FallbackPolicy::Allow` degrades to the platform shared output with a
    /// warning; `FallbackPolicy::Strict` refuses to start. The variant is
    /// kept because it is part of the persisted settings format and of the
    /// FFI backend ids.
    PipeWire,
    /// Linux JACK pro-audio output. **Declared, not implemented** — see
    /// [`Self::PipeWire`].
    Jack,
    /// Hardware-free sink. Drains the master ring on a real-time-shaped
    /// schedule and discards the audio, opening no OS device — for headless
    /// qualification, CI, and hosts that want the engine's timing without a
    /// DAC. It never claims bit-perfectness: `OutputInfo::is_exclusive` is
    /// `false` and the access state reads `Shared`/unverified, because there
    /// is no mixer to bypass and no hardware to be perfect *to*.
    Null,
    /// Reserved for an application-provided backend; currently maps to the
    /// platform default shared output in the built-in engine.
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ResamplerQuality {
    Fast,
    #[default]
    Balanced,
    HighQuality,
    /// Longest filter, deepest stopband. Intended for offline-grade
    /// conversion and reference setups; costs the most CPU.
    Ultra,
}

/// Nominal characteristics of each resampler quality tier.
///
/// These are **design targets**: `tests/fidelity/resampler_measurement.rs`
/// measures the realized passband ripple, stopband rejection, and latency of
/// the actual rubato filters each tier maps to, so the numbers below are
/// validated rather than aspirational. rubato 5.0's `Fft` resampler derives
/// the anti-aliasing filter length from `chunk_size / sub_chunks`, so each
/// tier is a genuinely longer filter — 320 / 640 / 1120 / 2240 taps for
/// 44.1↔48 kHz — with a correspondingly deeper stopband and higher CPU cost.
///
/// Those tap counts are measured, not asserted:
/// `published_latency_matches_measurement_and_its_tap_counts` builds each
/// tier, reads its filter group delay from the resampler, and requires that
/// `taps = 2 × group delay` equals the count in the `algorithm` string below
/// and that the measured latency equals `typical_latency_ms`.
#[derive(Debug, Clone, PartialEq)]
pub struct ResamplerQualityInfo {
    pub name: &'static str,
    pub cpu_estimate: &'static str,
    pub stopband_attenuation_db: f32,
    pub passband_ripple_db: f32,
    pub typical_latency_ms: f32,
    pub algorithm: &'static str,
}

impl ResamplerQuality {
    /// Stable label for reports and cross-engine policy reconciliation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Balanced => "balanced",
            Self::HighQuality => "high-quality",
            Self::Ultra => "ultra",
        }
    }

    /// The tier's published group delay in milliseconds.
    ///
    /// Ultimate Engine's policy layer mirrors this table to reason about latency
    /// budgets without depending on this crate; a root-crate test asserts the two
    /// still agree.
    pub fn latency_ms(&self) -> f32 {
        self.description().typical_latency_ms
    }

    pub fn description(&self) -> ResamplerQualityInfo {
        match self {
            Self::Fast => ResamplerQualityInfo {
                name: "Fast",
                cpu_estimate: "Low",
                stopband_attenuation_db: 150.0,
                passband_ripple_db: 0.01,
                typical_latency_ms: 3.3,
                algorithm: "Fft FixedSync::Both (~320-tap sinc)",
            },
            Self::Balanced => ResamplerQualityInfo {
                name: "Balanced",
                cpu_estimate: "Medium",
                stopband_attenuation_db: 160.0,
                passband_ripple_db: 0.005,
                typical_latency_ms: 6.7,
                algorithm: "Fft FixedSync::Input (~640-tap sinc)",
            },
            Self::HighQuality => ResamplerQualityInfo {
                name: "High Quality",
                cpu_estimate: "High",
                stopband_attenuation_db: 175.0,
                passband_ripple_db: 0.005,
                typical_latency_ms: 11.7,
                algorithm: "Fft FixedSync::Input (~1120-tap sinc)",
            },
            Self::Ultra => ResamplerQualityInfo {
                name: "Ultra",
                cpu_estimate: "Very High",
                stopband_attenuation_db: 180.0,
                passband_ripple_db: 0.005,
                typical_latency_ms: 23.3,
                algorithm: "Fft FixedSync::Input (~2240-tap sinc)",
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PerformanceMode {
    #[default]
    Normal,
    LowPower,
    /// Aggressive power-saving and low-CPU profile for legacy hardware or battery constraints.
    LegacyLowPower,
}

/// Resampler filter family.
///
/// This is the filter shape axis, independent of [`ResamplerQuality`]'s
/// length/cost tier. [`Self::Fft`] keeps the existing rubato FFT engine
/// (linear phase); the other variants select the engine's own rational
/// polyphase windowed-sinc bank:
///
/// - [`Self::Kaiser`] — linear-phase Kaiser-windowed sinc.
/// - [`Self::BlackmanHarris`] — linear-phase Blackman-Harris sinc with the
///   lowest sidelobes of the short windows.
/// - [`Self::MinimumPhase`] — the `Kaiser` passband converted to minimum phase
///   with the real-cepstrum method: no pre-ringing, and a much smaller latency
///   at the cost of frequency-dependent group delay. The cepstral tail is
///   tapered to fit the polyphase branch, so its realized stopband is measured
///   and is typically shallower than the linear design's.
/// - [`Self::Apodizing`] — a longer, steeper linear-phase filter whose
///   stopband starts just above the source Nyquist, for suppressing the
///   ringing left by an ADC's anti-alias filter.
/// - [`Self::SincM`] — a very long Kaiser sinc (twice the tier's taps) for
///   reference/offline conversion.
///
/// The realized tap count, measured passband ripple, stopband attenuation and
/// group delay are published per instance in
/// `dsp::resampler::ResamplerFilterMetadata`; they are measured from the
/// coefficients the engine actually runs, not asserted here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ResamplerFilter {
    /// Existing rubato FFT resampler (linear phase, quality-tier length).
    #[default]
    Fft,
    /// Rational polyphase Kaiser-windowed sinc (linear phase).
    Kaiser,
    /// Rational polyphase Blackman-Harris-windowed sinc (linear phase).
    BlackmanHarris,
    /// Real-cepstrum minimum-phase conversion of the Kaiser design.
    MinimumPhase,
    /// Long, steep linear-phase filter with the stopband at the source Nyquist.
    Apodizing,
    /// Very long Kaiser sinc for reference conversion.
    SincM,
}

impl ResamplerFilter {
    /// Stable label for reports, config dumps and hosts.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fft => "fft",
            Self::Kaiser => "kaiser",
            Self::BlackmanHarris => "blackman-harris",
            Self::MinimumPhase => "minimum-phase",
            Self::Apodizing => "apodizing",
            Self::SincM => "sinc-m",
        }
    }

    /// Whether the realized filter has (approximately) constant group delay.
    pub fn is_linear_phase(&self) -> bool {
        !matches!(self, Self::MinimumPhase)
    }

    /// Whether this family uses the engine's own polyphase bank rather than
    /// rubato's FFT engine.
    pub fn is_polyphase(&self) -> bool {
        !matches!(self, Self::Fft)
    }

    /// Taps per polyphase branch for a quality tier.
    ///
    /// These deliberately mirror the measured FFT tap ladder (320 / 640 /
    /// 1120 / 2240) so switching shape does not silently halve the filter
    /// length. `Apodizing` and `SincM` double it; `Fft` has no polyphase
    /// branch count and returns 0.
    pub fn taps_per_phase(&self, quality: ResamplerQuality) -> usize {
        let base = match quality {
            ResamplerQuality::Fast => 320,
            ResamplerQuality::Balanced => 640,
            ResamplerQuality::HighQuality => 1120,
            ResamplerQuality::Ultra => 2240,
        };
        match self {
            Self::Fft => 0,
            Self::Apodizing | Self::SincM => base * 2,
            Self::Kaiser | Self::BlackmanHarris | Self::MinimumPhase => base,
        }
    }
}

impl PerformanceMode {
    /// Returns true if this mode represents reduced CPU / power consumption.
    pub fn is_low_power(&self) -> bool {
        matches!(self, Self::LowPower | Self::LegacyLowPower)
    }
}

/// Configurable quality and latency tuning profile.
///
/// **DEAD — this field is read nowhere in the audio engine.** It looked like a
/// quality axis and behaved like one, which is what made it a trap: a host that
/// set `EngineConfig::quality_profile` got no effect and no warning. Do not map
/// a quality tier onto it.
///
/// The real quality and latency axes are `engine_core::QualityLevel` and
/// `engine_core::LatencyPolicy`, resolved by `engine_plan::ResolvedExecutionPolicy`
/// and mapped onto `PrecisionMode`, `ResamplerQuality` and the other native
/// settings that *are* read. Kept (unused) only so the audio engine's public
/// config struct stays source-compatible; it carries no behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum QualityLatencyProfile {
    /// Highest fidelity: maximum precision, deep-stopband filtering, full lookahead.
    Quality,
    /// Balanced fidelity and latency (engine default).
    #[default]
    Balanced,
    /// Lowest latency: short buffers, low lookahead, low-latency filter parameters.
    LowLatency,
    /// Deep buffering (1-2s cushion) optimized for spinning HDDs and low-power devices.
    HddSafe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LoudnessMode {
    #[default]
    Off,
    TrackReplayGain,
    AlbumReplayGain,
    EbuR128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FilterType {
    #[default]
    Peaking,
    LowShelf,
    HighShelf,
    LowPass,
    HighPass,
    Notch,
    Bandpass,
    AllPass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CrossfeedProfile {
    #[default]
    Bauer,
    ChuMoy,
    Jmeier,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CrossfadeCurve {
    #[default]
    ConstantPower,
    Linear,
    Exponential,
    Logarithmic,
    SCurve,
}

/// Explicit track-to-track transition mode.
/// `Crossfade` intentionally overlaps audio; `Gapless` preserves the exact
/// logical sample boundary (no overlap, no silence).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TransitionMode {
    /// Sample-accurate gapless: hand off at logical EOS with zero silence or overlap.
    #[default]
    Gapless,
    /// Overlapping crossfade (duration set by `CrossfadeConfig::duration_ms`).
    Crossfade,
    /// Fade-out current track, silence gap, fade-in next.
    Fade,
    /// Stop at EOS; do not auto-advance.
    Stop,
}

/// Playback speed mode: whether changing speed also changes pitch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SpeedMode {
    /// Varispeed: pitch changes proportionally with speed (resampling-based).
    #[default]
    Varispeed,
    /// Time-stretch: pitch remains constant regardless of speed (WSOLA algorithm).
    TimeStretch,
    /// Pitch-shift: tempo remains constant, pitch transposed (WSOLA algorithm).
    PitchShift,
}

/// Time-stretch/pitch-shift quality tier (spec §22).
///
/// Maps to real, measurable WSOLA parameter changes — synthesis window
/// length, overlap ratio, and waveform-similarity search range — not to a
/// marketing label. Higher tiers use longer analysis windows and finer
/// alignment search, which improves transient and sustained-tonal fidelity
/// at the cost of CPU and algorithmic latency. See
/// [`TimeStretchQuality::params`] for the exact numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TimeStretchQuality {
    /// Smallest window (512), 25% hop of window, narrow search. Lowest CPU
    /// and algorithmic latency; usable for speech/monologue where
    /// transient alignment is less critical.
    Low,
    /// 1024-sample window, 75% overlap, 128-sample search — the historical
    /// WSOLA defaults. Balanced quality/cost trade-off.
    #[default]
    Balanced,
    /// 2048-sample window, 75% overlap, 256-sample search. Best alignment
    /// for sustained tonal and polyphonic material at roughly double the
    /// search cost and double the algorithmic latency of `Balanced`.
    High,
}

impl TimeStretchQuality {
    /// Stable label for reports and cross-engine policy reconciliation.
    pub fn as_str(self) -> &'static str {
        match self {
            TimeStretchQuality::Low => "low",
            TimeStretchQuality::Balanced => "balanced",
            TimeStretchQuality::High => "high",
        }
    }

    /// Concrete WSOLA parameters for this tier as `(window, hop, search)`.
    ///
    /// `window` is the synthesis-frame length in samples, `hop` the
    /// synthesis hop (25% of `window` → 75% overlap-add), and `search` the
    /// ±sample range over which waveform-similarity alignment is evaluated.
    pub fn params(&self) -> (usize, usize, usize) {
        match self {
            Self::Low => (512, 128, 64),
            Self::Balanced => (1024, 256, 128),
            Self::High => (2048, 512, 256),
        }
    }

    /// One-line human description (used by diagnostics, spec §22/§29).
    pub fn description(&self) -> &'static str {
        match self {
            Self::Low => "512-sample window / 128-hop / 64-search — lowest CPU & latency",
            Self::Balanced => "1024-sample window / 256-hop / 128-search — balanced quality",
            Self::High => {
                "2048-sample window / 512-hop / 256-search — best transient & tonal fidelity"
            }
        }
    }
}

/// How DSD audio is handled at the output stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DsdOutput {
    /// Convert DSD to PCM before any further processing (safe default).
    #[default]
    PcmConvert,
    /// DSD-over-PCM (DoP): requires I32 output and a DoP-capable DAC.
    DoP,
    /// Native DSD bitstream output via direct DAC transport.
    ///
    /// Supported today by the native ALSA backend (`ExclusiveAlsa` with a
    /// direct `hw:` node) for DSD_U8/U16/U32 wire formats; on every other
    /// backend the engine negotiates an explicit downgrade — Native → DoP →
    /// PCM conversion — recorded in `DsdTransportReport` (never a silent
    /// fallback).
    ///
    /// Platform status (verified against the `Output` trait contract):
    /// - **ALSA** (exact `hw:` node) — raw native DSD works; `plughw:` is
    ///   rejected for native transport because its conversion plugin is not
    ///   bit-perfect.
    /// - **WASAPI exclusive** — DoP works (I32 container at bit_rate/16);
    ///   raw native DSD has no WASAPI stream format and is not possible.
    /// - **ASIO** (`ExclusiveAsio`, via cpal) — DoP works (the engine's DoP
    ///   path reopens the stream as I32 at bit_rate/16); raw native DSD
    ///   requires a vendor ASIO DSD extension (e.g. the Thesycon-style
    ///   private format) and is not implemented in this build.
    /// - **CoreAudio** (macOS) — no native CoreAudio DSD stream format exists;
    ///   DoP over a hog-mode CoreAudio backend is the target, and is not yet
    ///   implemented (cpal's CoreAudio path is shared-only).
    NativeDsd,
}

/// DSD→PCM decimation filter.
///
/// DSD carries a large amount of ultrasonic noise by construction (the
/// modulator's shaped quantization noise above the audio band). The
/// decimation filter decides how much of that noise survives into the PCM
/// output: a gentler filter keeps more of the audio band and more of the
/// noise, an aggressive one trades bandwidth for a cleaner ultrasonic
/// spectrum.
///
/// Every variant is a real, measured five-stage 2:1 FIR cascade — the
/// response figures in the engine README are produced by
/// `decode::dsd::tests`, not asserted here. The default [`Self::Standard`]
/// reproduces the historical 63-tap design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DsdNoiseFilter {
    /// 63-tap Blackman half-band cascade, linear phase. The default: flat to
    /// 20 kHz and > 40 dB rejection at 100 kHz for DSD64.
    #[default]
    Standard,
    /// Lower corner frequency with 95 taps, giving a flatter ultrasonic noise
    /// profile at the cost of a little bandwidth.
    Low,
    /// Higher-order (127-tap Kaiser) cascade for material produced with
    /// high-order modulators; deeper stopband.
    HighOrder,
    /// 191-tap Kaiser cascade with a very steep transition, rejecting nearly
    /// all out-of-band noise at the cost of the most CPU.
    Brickwall,
    /// 31-tap linear-phase Hanning cascade: gentle roll-off, short latency,
    /// more ultrasonic leakage.
    SlowLp,
    /// The same gentle magnitude response as [`Self::SlowLp`], converted to
    /// minimum phase (no pre-ringing, all energy after the impulse).
    SlowMp,
}

impl DsdNoiseFilter {
    /// Stable, human-readable name (diagnostics and UI).
    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Low => "low",
            Self::HighOrder => "high-order",
            Self::Brickwall => "brickwall",
            Self::SlowLp => "slow-lp",
            Self::SlowMp => "slow-mp",
        }
    }

    /// Every selectable filter, in UI order.
    pub const ALL: [DsdNoiseFilter; 6] = [
        Self::Standard,
        Self::Low,
        Self::HighOrder,
        Self::Brickwall,
        Self::SlowLp,
        Self::SlowMp,
    ];
}

/// Optional ultrasonic low-pass at the output.
///
/// Audiophile sources can carry out-of-band energy (DSD noise shaping, ADC
/// artifacts, 24/192 transfers from analogue tape). Some tweeters, class-D
/// amplifiers, and NOS/first-order DACs misbehave in its presence. This is a
/// deliberate user filter, not a correction: it is bypassed in bit-perfect
/// mode like every other sample-altering stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum UltrasonicFilter {
    /// No filter (bit-exact bypass).
    #[default]
    Off,
    /// 4th-order (24 dB/oct) low-pass at 20 kHz: the sharp metering filter.
    Cut20k,
    /// 2nd-order (12 dB/oct) gentle low-pass at 30 kHz.
    Cut30k,
    /// 2nd-order gentle low-pass at 40 kHz.
    Cut40k,
    /// 2nd-order gentle low-pass at 50 kHz, for ADC/DSD noise-shaping
    /// residue.
    Cut50k,
    /// 8th-order (48 dB/oct) low-pass at 2× the output base rate.
    X2,
    /// 8th-order low-pass at 4× the output base rate.
    X4,
    /// 8th-order low-pass at 8× the output base rate.
    X8,
}

impl UltrasonicFilter {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Cut20k => "20 kHz",
            Self::Cut30k => "30 kHz",
            Self::Cut40k => "40 kHz",
            Self::Cut50k => "50 kHz",
            Self::X2 => "2x",
            Self::X4 => "4x",
            Self::X8 => "8x",
        }
    }

    pub const ALL: [UltrasonicFilter; 8] = [
        Self::Off,
        Self::Cut20k,
        Self::Cut30k,
        Self::Cut40k,
        Self::Cut50k,
        Self::X2,
        Self::X4,
        Self::X8,
    ];
}

/// One-bit delta-sigma modulator used for PCM→SDM output.
///
/// These are ShadowPlayer's own modulators, not clones of any vendor's
/// proprietary designs. Each is a real fifth- or seventh-order CRFB
/// (cascade-of-resonators, feedback) one-bit modulator with an optimized
/// noise transfer function; the realized idle noise floor is measured by
/// `dsp::sdm::tests`, not asserted in this table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SdmModulator {
    /// Fifth-order one-bit modulator. Best match for DACs with a simple
    /// analog reconstruction filter (e.g. single-bit / one-switching-element
    /// converters).
    #[default]
    FifthOrder,
    /// Seventh-order one-bit modulator. Deeper in-band noise shaping; needs
    /// a DAC whose analog stage tolerates more out-of-band energy.
    SeventhOrder,
}

impl SdmModulator {
    /// The modulator order (5 or 7).
    pub fn order(self) -> usize {
        match self {
            Self::FifthOrder => 5,
            Self::SeventhOrder => 7,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::FifthOrder => "5th order",
            Self::SeventhOrder => "7th order",
        }
    }

    pub const ALL: [SdmModulator; 2] = [Self::FifthOrder, Self::SeventhOrder];
}

/// Requested SDM output rate, as a multiple of the output base rate.
///
/// The real rate is always the source clock family (44.1 kHz or 48 kHz)
/// times the selected multiple, and is only advertised when the output
/// device actually accepts it. `Auto` asks for the highest multiple the
/// device reports and the PCM rate divides integrally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SdmRate {
    /// Highest rate the device supports (DSD512 → DSD256 → … → DSD64).
    #[default]
    Auto,
    /// 64 × base rate (2.8224 / 3.072 MHz).
    Dsd64,
    /// 128 × base rate (5.6448 / 6.144 MHz).
    Dsd128,
    /// 256 × base rate (11.2896 / 12.288 MHz).
    Dsd256,
    /// 512 × base rate (22.5792 / 24.576 MHz).
    Dsd512,
}

impl SdmRate {
    /// Fixed multiple of the base rate, or `None` for [`Self::Auto`].
    pub fn multiple(self) -> Option<u32> {
        match self {
            Self::Auto => None,
            Self::Dsd64 => Some(64),
            Self::Dsd128 => Some(128),
            Self::Dsd256 => Some(256),
            Self::Dsd512 => Some(512),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Dsd64 => "DSD64",
            Self::Dsd128 => "DSD128",
            Self::Dsd256 => "DSD256",
            Self::Dsd512 => "DSD512",
        }
    }

    pub const ALL: [SdmRate; 5] = [
        Self::Auto,
        Self::Dsd64,
        Self::Dsd128,
        Self::Dsd256,
        Self::Dsd512,
    ];
}

/// Per-channel routing policy for multichannel sources.
///
/// ## Architectural limitation
///
/// Multichannel preservation is conditional. The source layout, output
/// layout, DSP channel limit (`MAX_CHANNELS`), and resampler state must all be
/// compatible. When any condition is not met, the engine performs a documented
/// stereo downmix rather than dropping channels silently. The codec's decode
/// capability and the output device's channel capability are reported by
/// separate layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChannelPolicy {
    /// Always downmix to stereo at the decoder boundary (default behaviour).
    /// Mono sources are duplicated to both channels; multichannel is ITU-R BS.775.
    #[default]
    ForceDownmixStereo,
    /// Request the source channel layout.
    ///
    /// For 1–2 channel sources this preserves the layout. For wider sources it
    /// is preserved only when the output has the same width, the width is
    /// supported by the DSP path, and no resampler is active; otherwise the
    /// engine explicitly downmixes to stereo.
    PassThrough,
    /// Request up to N source channels; downmix if the source has more than N
    /// or if the output/DSP path cannot preserve the requested layout. N is
    /// also bounded by the engine's maximum supported channel count.
    MaxChannels(u8),
    /// Render a spatial scene through a spatial renderer (`SpatialObjects`
    /// mode, spec §6 / Part I §6).
    ///
    /// In this mode the conventional decoded-signal path is routed through
    /// the engine's spatial renderer, which places scene objects onto the
    /// negotiated output layout. This is **opt-in**: the default remains
    /// [`Self::ForceDownmixStereo`], and every other access to the channel
    /// engine is unchanged. Beds/fields and object audio that have no spatial
    /// counterpart fall back to a conventional downmix so no channel is
    /// silently dropped.
    SpatialRender,
}

/// Envelope detector mode for a compressor band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CompressorDetector {
    /// Instantaneous peak follower (classic; reacts to transients).
    #[default]
    Peak,
    /// Windowed RMS level (reacts to program level, not transients).
    Rms,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FallbackPolicy {
    Strict,
    #[default]
    Allow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VolumeMode {
    /// Apply gain in the DSP pipeline and never touch endpoint/hardware
    /// volume control (spec §12 "SoftwareOnly").
    #[default]
    SoftwareOnly,
    /// Prefer native endpoint volume control: when the active backend
    /// supports hardware volume, the DSP pipeline runs at unity and the
    /// endpoint owns the level. When it does not (e.g. ASIO), the engine
    /// falls back to software DSP gain instead of failing (spec §12
    /// "HardwarePreferred").
    HardwarePreferred,
    /// Hardware endpoint volume is **required** (spec §12 "HardwareOnly").
    ///
    /// When the active backend has no endpoint volume control, the engine
    /// does **not** fall back to software gain — it leaves the signal
    /// untouched (software pipeline at unity) and reports the failure in
    /// `PlaybackInfo::volume_error`. This is the strict companion to
    /// [`Self::HardwarePreferred`], and the mode that guarantees a
    /// bit-perfect request can never be silently turned into a
    /// sample-modifying software path (§5.1).
    HardwareOnly,
    /// Software gain is explicitly permitted (spec §12 "SoftwareAllowed").
    ///
    /// Runtime behaviour matches [`Self::SoftwareOnly`] (DSP gain, endpoint
    /// never touched); the variant exists to complete the §12 four-mode
    /// contract as an explicit opt-in for setups that accept software
    /// attenuation and therefore do not claim bit-perfect transport.
    SoftwareAllowed,
}

/// Per-output dither policy (spec §10 profile: "Dither: Off / TPDF").
/// `FollowGlobal` uses the engine-wide `config.dither_enabled`; the force
/// variants override it for a specific device profile (e.g. always-on for a
/// noisy 16-bit DAC, always-off for a 24-bit reference DAC). The policy only
/// decides *whether* dither runs; [`NoiseShaper`] decides *which* shaper it
/// uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DitherPolicy {
    /// Use the engine-wide `dither_enabled` setting.
    #[default]
    FollowGlobal,
    /// Force dither on at the integer-quantization boundary, using the
    /// configured [`NoiseShaper`].
    ForceOn,
    /// Force dither off for this device.
    ForceOff,
}

/// Noise-shaping / dither family applied at the integer-quantization
/// boundary.
///
/// This is deliberately separate from [`DitherPolicy`]: the policy decides
/// *whether* dither runs for a device, this decides *which* shaper runs.
/// `Tpdf` is the engine's historical behaviour and remains the default, so a
/// config written before this field existed round-trips unchanged.
///
/// The designs are the engine's own, not HQPlayer's proprietary coefficient
/// sets. They are named by order and weighting intent:
///
/// - `Shibata`: the existing 9-tap F-weighted design at 44.1/48 kHz.
/// - `Ns5` / `Ns9`: 5th/9th-order error-feedback shapers with a rising
///   high-frequency noise transfer function.
/// - `Lns15`: a gentler 15th-order "low-noise shaping" design for high-rate
///   output, where the audible band is a small fraction of Nyquist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NoiseShaper {
    /// Triangular-PDF dither with no noise shaping (the historical default).
    #[default]
    Tpdf,
    /// Shibata F-weighted 9-tap noise shaping (44.1/48 kHz designs).
    Shibata,
    /// 5th-order high-pass noise shaping.
    Ns5,
    /// 9th-order high-pass noise shaping.
    Ns9,
    /// 15th-order, low-noise high-pass noise shaping.
    Lns15,
}

impl NoiseShaper {
    /// Stable configuration-file spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tpdf => "Tpdf",
            Self::Shibata => "Shibata",
            Self::Ns5 => "Ns5",
            Self::Ns9 => "Ns9",
            Self::Lns15 => "Lns15",
        }
    }

    /// Every variant, in configuration order. Used by tests and the settings
    /// surface to prove the enum is fully described.
    pub const ALL: [Self; 5] = [Self::Tpdf, Self::Shibata, Self::Ns5, Self::Ns9, Self::Lns15];
}

/// Fallback strategy when the exact desired sample rate is not supported by the hardware device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RateFallbackPolicy {
    /// Pick the supported rate with minimal numerical difference.
    #[default]
    Nearest,
    /// Prefer the lowest supported rate >= target (avoiding downsampling), falling back to highest supported.
    PreferHigher,
    /// Prefer the highest supported rate <= target, falling back to lowest supported.
    PreferLower,
    /// Prefer rates in the same clock family first, then nearest.
    SameFamilyFirst,
}

/// DSP signal-path precision.
///
/// ## Actual precision guarantees
///
/// **Performance (f32):** the pipeline's sample buffers and the final safety
/// limiter are f32. Most processors still accumulate internally in f64 where
/// it matters (biquad states, convolution accumulation), and the EQ cascade
/// promotes once to f64 at the EQ boundary and demotes once at its exit —
/// there are no per-band precision conversions.
///
/// **Quality (f64):** the entire chain — loudness, EQ cascade, multiband
/// compressor, convolution, crossfeed, stereo, volume, seek fade — runs in
/// f64, demoted to f32 only at the output boundary (and into the final
/// safety limiter, which is deliberately f32 in both modes).
///
/// ## Documented exceptions
///
/// - **Time-stretch / pitch-shift** (`TimeStretcher`): the WSOLA synthesis
///   core operates internally in f32 for cache locality and SIMD
///   cross-correlation in **both** modes; Quality mode converts the f64
///   stream to f32 around that core. The core's interpolation is a
///   precomputed 16-tap windowed-sinc table, so this is a deliberate,
///   documented precision trade-off rather than an accident.
/// - **Resampler** (rubato): f32/f64 follows the precision mode, but the
///   internal FFT engine precision is rubato's own; the configured quality
///   tier (Fast/Balanced/HighQuality) governs its filter characteristics
///   (see [`ResamplerQuality::description`]).
/// - **Final safety limiter**: f32 in both modes (it runs in the output
///   domain after the resampler, where the sample container is f32).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PrecisionMode {
    #[default]
    Performance,
    Quality,
}

/// Operating access mode of an audio output stream.
///
/// Lives in the config crate (not the output module) so the DSP pipeline's
/// bit-perfect report — which is compiled in builds without the
/// `audio-output` feature — can carry the real requested/actual access
/// vocabulary instead of a boolean approximation. `src/output/capabilities.rs`
/// re-exports it for API compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum OutputAccessMode {
    /// Shared system mixer (e.g. PulseAudio, PipeWire, WASAPI Shared, CoreAudio Default).
    #[default]
    Shared,
    /// Exclusive mode stream (e.g. WASAPI Exclusive, CoreAudio Hog Mode).
    Exclusive,
    /// Direct hardware access endpoint (e.g. ALSA `hw:X,Y` direct device node).
    DirectHw,
    /// Bitstream pass-through mode for encoded or DSD streams.
    BitstreamPassthrough,
}

impl OutputAccessMode {
    /// True if the access mode bypasses the OS audio mixer for bit-perfect output.
    pub fn is_direct(&self) -> bool {
        matches!(
            self,
            Self::Exclusive | Self::DirectHw | Self::BitstreamPassthrough
        )
    }
}

/// Comprehensive output access state distinguishing requested access, actual
/// negotiated access, and whether direct hardware access was verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct OutputAccessState {
    /// Access mode requested by the user/config.
    pub requested: OutputAccessMode,
    /// Access mode actually negotiated with the driver.
    pub actual: OutputAccessMode,
    /// Whether direct, unmixed hardware access was verified.
    pub verified: bool,
}

impl OutputAccessState {
    pub fn is_bit_perfect(&self) -> bool {
        self.actual.is_direct() && self.verified
    }
}
