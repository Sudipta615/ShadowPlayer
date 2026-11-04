use crate::audio::{downmix_and_resample_into, ResamplePlan};
use crate::{AudioReadError, AudioWindowSource};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

pub const FEATURE_COUNT: usize = 20;
pub const SERIALIZED_FEATURE_LEN: usize = 4 + 4 + FEATURE_COUNT * 4;
/// Bump this when feature semantics change. Cached vectors from another schema
/// must be discarded and recomputed.
pub const FEATURE_SCHEMA_VERSION: u32 = 2;
const FEATURE_MAGIC: [u8; 4] = *b"SRF2";

pub const FEATURE_NAMES: [&str; FEATURE_COUNT] = [
    "loudness",
    "dynamic_range",
    "spectral_centroid",
    "spectral_rolloff_85",
    "spectral_flatness",
    "spectral_flux",
    "low_frequency_ratio",
    "tempo_bpm_scaled",
    "chroma_c",
    "chroma_c_sharp",
    "chroma_d",
    "chroma_d_sharp",
    "chroma_e",
    "chroma_f",
    "chroma_f_sharp",
    "chroma_g",
    "chroma_g_sharp",
    "chroma_a",
    "chroma_a_sharp",
    "chroma_b",
];

/// Default weighted-distance importance. The values are relative weights and
/// are renormalized over dimensions available in both tracks.
pub const FEATURE_WEIGHTS: [f32; FEATURE_COUNT] = [
    0.04, 0.06, 0.12, 0.10, 0.09, 0.10, 0.12, 0.11, 0.0216667, 0.0216667, 0.0216667, 0.0216667,
    0.0216667, 0.0216667, 0.0216667, 0.0216667, 0.0216667, 0.0216667, 0.0216667, 0.0216667,
];

const ALL_FEATURES_MASK: u32 = (1_u32 << FEATURE_COUNT) - 1;
const CHROMA_MASK: u32 = ((1_u32 << 12) - 1) << 8;
const CHROMA_LOW_HZ: f32 = 55.0;
const CHROMA_HIGH_HZ: f32 = 2_000.0;

/// One normalized 20-dimensional track representation. Values are in [0, 1].
/// `valid_mask` marks dimensions supported by the analyzed audio; absent tempo
/// or near-silent chroma dimensions are ignored instead of interpreted as zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FeatureVector {
    pub values: [f32; FEATURE_COUNT],
    pub valid_mask: u32,
}

impl FeatureVector {
    pub fn new(values: [f32; FEATURE_COUNT], valid_mask: u32) -> Self {
        let valid_mask = valid_mask & ALL_FEATURES_MASK;
        let mut clean = values;
        for (index, value) in clean.iter_mut().enumerate() {
            if valid_mask & (1_u32 << index) == 0 || !value.is_finite() {
                *value = 0.0;
            } else {
                *value = value.clamp(0.0, 1.0);
            }
        }
        Self {
            values: clean,
            valid_mask,
        }
    }

    pub fn empty() -> Self {
        Self {
            values: [0.0; FEATURE_COUNT],
            valid_mask: 0,
        }
    }

    #[inline]
    pub fn is_valid(&self, index: usize) -> bool {
        index < FEATURE_COUNT && (self.valid_mask & (1_u32 << index)) != 0
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.valid_mask == 0
    }

    #[inline]
    pub fn valid_count(&self) -> u32 {
        self.valid_mask.count_ones()
    }

    pub fn to_bytes(&self) -> [u8; SERIALIZED_FEATURE_LEN] {
        let mut bytes = [0_u8; SERIALIZED_FEATURE_LEN];
        bytes[0..4].copy_from_slice(&FEATURE_MAGIC);
        bytes[4..8].copy_from_slice(&self.valid_mask.to_le_bytes());
        for (index, value) in self.values.iter().enumerate() {
            let offset = 8 + index * 4;
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, FeatureCodecError> {
        if bytes.len() != SERIALIZED_FEATURE_LEN {
            return Err(FeatureCodecError::InvalidLength {
                expected: SERIALIZED_FEATURE_LEN,
                actual: bytes.len(),
            });
        }
        if bytes[0..4] != FEATURE_MAGIC {
            return Err(FeatureCodecError::UnknownFormat);
        }
        let valid_mask = u32::from_le_bytes(bytes[4..8].try_into().expect("four-byte slice"));
        if valid_mask & !ALL_FEATURES_MASK != 0 {
            return Err(FeatureCodecError::InvalidMask);
        }
        let mut values = [0.0_f32; FEATURE_COUNT];
        for (index, value) in values.iter_mut().enumerate() {
            let offset = 8 + index * 4;
            *value = f32::from_le_bytes(
                bytes[offset..offset + 4]
                    .try_into()
                    .expect("four-byte slice"),
            );
            if !value.is_finite() {
                return Err(FeatureCodecError::NonFiniteValue { index });
            }
            if !(0.0..=1.0).contains(value) {
                return Err(FeatureCodecError::OutOfRangeValue { index });
            }
        }
        Ok(Self::new(values, valid_mask))
    }

    /// Similarity in [0, 1] using weighted RMS distance across shared valid dimensions.
    pub fn similarity(&self, other: &Self) -> Option<f32> {
        let mut weighted_sq = 0.0_f32;
        let mut weight_sum = 0.0_f32;
        for (i, weight) in FEATURE_WEIGHTS.iter().enumerate() {
            if self.is_valid(i) && other.is_valid(i) {
                let delta = self.values[i] - other.values[i];
                weighted_sq += *weight * delta * delta;
                weight_sum += *weight;
            }
        }
        if weight_sum <= f32::EPSILON {
            return None;
        }
        Some((1.0 - (weighted_sq / weight_sum).sqrt()).clamp(0.0, 1.0))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FeatureCodecError {
    InvalidLength { expected: usize, actual: usize },
    UnknownFormat,
    InvalidMask,
    NonFiniteValue { index: usize },
    OutOfRangeValue { index: usize },
}

impl Display for FeatureCodecError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidLength { expected, actual } => {
                write!(f, "expected {expected} feature bytes, got {actual}")
            }
            Self::UnknownFormat => f.write_str("unknown feature-vector format/version"),
            Self::InvalidMask => f.write_str("feature vector contains unknown valid-mask bits"),
            Self::NonFiniteValue { index } => write!(f, "feature {index} is not finite"),
            Self::OutOfRangeValue { index } => write!(f, "feature {index} is outside [0, 1]"),
        }
    }
}

impl Error for FeatureCodecError {}

/// Presets trade indexing time against feature stability and CPU cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalysisProfile {
    LowResource,
    Balanced,
    HighQuality,
}

#[derive(Clone, Debug)]
pub struct AnalysisConfig {
    pub target_sample_rate: u32,
    /// Target duration of each of the three sampled regions, in seconds.
    pub segment_seconds: f64,
    pub fft_size: usize,
    pub hop_size: usize,
    /// Frames below this absolute level are ignored for spectral descriptors.
    pub frame_gate_dbfs: f32,
    /// Frames more than this many dB below the loudest frame are ignored for
    /// spectral descriptors. This keeps a quiet intro from flattening the
    /// timbre statistics of an otherwise loud track.
    pub relative_gate_db: f32,
    /// Minimum normalized autocorrelation confidence before tempo is persisted.
    pub min_tempo_confidence: f32,
}

impl AnalysisConfig {
    pub fn for_profile(profile: AnalysisProfile) -> Self {
        match profile {
            AnalysisProfile::LowResource => Self {
                target_sample_rate: 8_000,
                segment_seconds: 4.0,
                fft_size: 1_024,
                hop_size: 512,
                frame_gate_dbfs: -72.0,
                relative_gate_db: 35.0,
                min_tempo_confidence: 0.18,
            },
            AnalysisProfile::Balanced => Self {
                target_sample_rate: 11_025,
                segment_seconds: 6.0,
                fft_size: 2_048,
                hop_size: 512,
                frame_gate_dbfs: -72.0,
                relative_gate_db: 35.0,
                min_tempo_confidence: 0.18,
            },
            AnalysisProfile::HighQuality => Self {
                target_sample_rate: 16_000,
                segment_seconds: 6.5,
                fft_size: 2_048,
                hop_size: 512,
                frame_gate_dbfs: -72.0,
                relative_gate_db: 35.0,
                min_tempo_confidence: 0.18,
            },
        }
    }

    fn validate(&self) -> Result<(), AnalysisError> {
        if self.target_sample_rate < 8_000 || self.target_sample_rate > 48_000 {
            return Err(AnalysisError::InvalidConfig(
                "target sample rate must be between 8000 and 48000 Hz".into(),
            ));
        }
        if !self.segment_seconds.is_finite() || self.segment_seconds <= 0.0 {
            return Err(AnalysisError::InvalidConfig(
                "segment duration must be a finite positive number".into(),
            ));
        }
        if self.fft_size < 512 || self.fft_size > 8_192 || !self.fft_size.is_power_of_two() {
            return Err(AnalysisError::InvalidConfig(
                "FFT size must be a power of two between 512 and 8192".into(),
            ));
        }
        if self.segment_seconds > 10.0 {
            return Err(AnalysisError::InvalidConfig(
                "segment duration must not exceed 10 seconds per region".into(),
            ));
        }
        if self.hop_size < self.fft_size / 4 || self.hop_size > self.fft_size {
            return Err(AnalysisError::InvalidConfig(
                "hop size must be in fft_size/4..=fft_size to bound frame cost".into(),
            ));
        }
        if !self.frame_gate_dbfs.is_finite() || !(-120.0..=-20.0).contains(&self.frame_gate_dbfs) {
            return Err(AnalysisError::InvalidConfig(
                "frame gate must be a finite value between -120 and -20 dBFS".into(),
            ));
        }
        if !self.relative_gate_db.is_finite() || !(6.0..=60.0).contains(&self.relative_gate_db) {
            return Err(AnalysisError::InvalidConfig(
                "relative gate must be between 6 and 60 dB".into(),
            ));
        }
        if !self.min_tempo_confidence.is_finite()
            || !(0.0..=1.0).contains(&self.min_tempo_confidence)
        {
            return Err(AnalysisError::InvalidConfig(
                "minimum tempo confidence must be between 0 and 1".into(),
            ));
        }
        Ok(())
    }
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self::for_profile(AnalysisProfile::Balanced)
    }
}

#[derive(Clone, Debug)]
pub struct AnalysisResult {
    /// The 20-dimensional vector persisted by the host.
    pub features: FeatureVector,
    /// Number of regions that decoded and produced usable features.
    pub windows_analyzed: u8,
    /// Number of regions that failed to decode or analyze. Partial failure is
    /// non-fatal as long as at least one region produced usable audio.
    pub windows_failed: u8,
    /// Sample rate used for the feature analysis.
    pub target_sample_rate: u32,
    /// Seconds of audio actually analyzed after resampling.
    pub sampled_seconds: f32,
    /// Confidence of the combined tempo estimate in `[0, 1]`.
    pub tempo_confidence: f32,
    /// Estimated tuning offset in cents; positive means the recording is sharp.
    pub tuning_cents: f32,
}

#[derive(Debug)]
pub enum AnalysisError {
    InvalidConfig(String),
    InvalidDuration,
    EmptyTrack,
    Source(AudioReadError),
    NoUsableAudio,
}

impl Display for AnalysisError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(message) => write!(f, "invalid analysis config: {message}"),
            Self::InvalidDuration => f.write_str("audio source duration is unknown or invalid"),
            Self::EmptyTrack => f.write_str("audio source contains no samples"),
            Self::Source(error) => write!(f, "audio source error: {error}"),
            Self::NoUsableAudio => f.write_str("no usable audio energy found in sampled windows"),
        }
    }
}

impl Error for AnalysisError {}

impl From<AudioReadError> for AnalysisError {
    fn from(value: AudioReadError) -> Self {
        Self::Source(value)
    }
}

/// Decode only three regions (start, middle, end), resample them, and calculate
/// the 20-dimensional representation. The method is synchronous and creates no
/// threads: schedule it on the host application's low-priority worker.
///
/// A failed region does not discard the other two. If every region fails, the
/// last decoder error is returned; if regions decode but contain no usable
/// energy, `NoUsableAudio` is returned.
pub fn analyze_track<S: AudioWindowSource>(
    source: &mut S,
    config: &AnalysisConfig,
) -> Result<AnalysisResult, AnalysisError> {
    config.validate()?;
    let duration = source.duration_seconds();
    if !duration.is_finite() || duration <= 0.0 {
        return Err(AnalysisError::InvalidDuration);
    }

    // For tracks shorter than three nominal windows, shrink each segment so the
    // three samples cover the full track instead of overlapping excessively.
    let window_seconds = config.segment_seconds.min(duration / 3.0);
    let max_start = (duration - window_seconds).max(0.0);
    let starts = [0.0, max_start / 2.0, max_start];
    let mut plan = AnalysisPlan::new(config);
    let mut sum = [0.0_f64; FEATURE_COUNT];
    let mut dimension_weights = [0.0_f64; FEATURE_COUNT];
    let mut counts = [0_u8; FEATURE_COUNT];
    let mut tempo_candidates: Vec<(f32, f32)> = Vec::with_capacity(3);
    let mut tuning_sum = 0.0_f64;
    let mut tuning_weight = 0.0_f64;
    let mut windows_analyzed = 0_u8;
    let mut windows_failed = 0_u8;
    let mut sampled_seconds = 0.0_f64;
    let mut received_any_samples = false;
    let mut last_error: Option<AudioReadError> = None;
    let mut resample_cache: Option<Arc<ResamplePlan>> = None;
    let mut mono = Vec::new();
    let mut resampled = Vec::new();

    for start in starts {
        let remaining = (duration - start).max(0.0);
        let requested_duration = window_seconds.min(remaining);
        if requested_duration <= 0.0 {
            continue;
        }
        let pcm = match source.read_window(start, requested_duration) {
            Ok(pcm) => pcm,
            Err(error) => {
                windows_failed = windows_failed.saturating_add(1);
                last_error = Some(error);
                continue;
            }
        };
        if let Err(error) = downmix_and_resample_into(
            &pcm,
            config.target_sample_rate,
            &mut resample_cache,
            &mut mono,
            &mut resampled,
        ) {
            windows_failed = windows_failed.saturating_add(1);
            last_error = Some(error);
            continue;
        }
        if resampled.is_empty() {
            continue;
        }
        received_any_samples = true;
        let actual_seconds = resampled.len() as f64 / config.target_sample_rate as f64;
        let Some(window_features) = plan.extract(&resampled) else {
            windows_failed = windows_failed.saturating_add(1);
            continue;
        };
        windows_analyzed = windows_analyzed.saturating_add(1);
        sampled_seconds += actual_seconds.min(requested_duration);
        if window_features.tempo_confidence >= config.min_tempo_confidence {
            tempo_candidates.push((window_features.tempo_bpm, window_features.tempo_confidence));
        }
        tuning_sum += window_features.tuning_cents as f64 * window_features.chroma_weight as f64;
        tuning_weight += window_features.chroma_weight as f64;

        for i in 0..FEATURE_COUNT {
            if i == 7 {
                continue;
            }
            if window_features.vector.is_valid(i) {
                let weight = 1.0;
                sum[i] += window_features.vector.values[i] as f64 * weight;
                dimension_weights[i] += weight;
                counts[i] = counts[i].saturating_add(1);
            }
        }
    }

    if windows_analyzed == 0 {
        return Err(match last_error {
            Some(error) => AnalysisError::Source(error),
            None if received_any_samples => AnalysisError::NoUsableAudio,
            None => AnalysisError::EmptyTrack,
        });
    }

    let mut values = [0.0_f32; FEATURE_COUNT];
    let mut valid_mask = 0_u32;
    for i in 0..FEATURE_COUNT {
        if i == 7 {
            continue;
        }
        if counts[i] > 0 {
            let denominator = dimension_weights[i].max(f64::EPSILON);
            values[i] = (sum[i] / denominator).clamp(0.0, 1.0) as f32;
            valid_mask |= 1_u32 << i;
        }
    }

    let (tempo_bpm, tempo_confidence) = combine_tempos(&tempo_candidates);
    if tempo_confidence >= config.min_tempo_confidence {
        values[7] = ((tempo_bpm - 50.0) / 150.0).clamp(0.0, 1.0);
        valid_mask |= 1_u32 << 7;
    }

    // Re-normalize chroma after averaging segment-level chroma distributions.
    let chroma_sum: f32 = values[8..20].iter().sum();
    if chroma_sum > 1.0e-8 {
        for value in &mut values[8..20] {
            *value /= chroma_sum;
        }
    } else {
        valid_mask &= !CHROMA_MASK;
    }

    Ok(AnalysisResult {
        features: FeatureVector::new(values, valid_mask),
        windows_analyzed,
        windows_failed,
        target_sample_rate: config.target_sample_rate,
        sampled_seconds: sampled_seconds as f32,
        tempo_confidence,
        tuning_cents: if tuning_weight > 0.0 {
            (tuning_sum / tuning_weight) as f32
        } else {
            0.0
        },
    })
}

fn combine_tempos(candidates: &[(f32, f32)]) -> (f32, f32) {
    let Some((reference, best_confidence)) = candidates
        .iter()
        .copied()
        .filter(|(bpm, confidence)| bpm.is_finite() && confidence.is_finite())
        .max_by(|left, right| left.1.total_cmp(&right.1))
    else {
        return (0.0, 0.0);
    };
    if reference <= 0.0 || best_confidence <= 0.0 {
        return (0.0, 0.0);
    }

    // Tempo octaves are ambiguous. Fold every candidate toward the most
    // confident window, then average only estimates that agree after folding.
    let mut weighted_sum = 0.0_f64;
    let mut weight_sum = 0.0_f64;
    for (bpm, confidence) in candidates.iter().copied() {
        if !bpm.is_finite() || !confidence.is_finite() || confidence <= 0.0 {
            continue;
        }
        let mut folded = bpm;
        while folded < reference * 0.75 {
            folded *= 2.0;
        }
        while folded > reference * 1.34 {
            folded *= 0.5;
        }
        if (folded / reference - 1.0).abs() <= 0.05 {
            weighted_sum += folded as f64 * confidence as f64;
            weight_sum += confidence as f64;
        }
    }
    if weight_sum <= f64::EPSILON {
        return (reference, best_confidence);
    }
    let bpm = (weighted_sum / weight_sum) as f32;
    (bpm.clamp(50.0, 200.0), best_confidence)
}

mod fft;
mod helpers;
mod plan;

pub(crate) use fft::RealFft;
pub(crate) use helpers::{estimate_tempo, log_frequency_normalized, quantile_sorted};
pub(crate) use plan::AnalysisPlan;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AudioReadError, PcmWindow};
    use std::f32::consts::PI;

    struct MemorySource {
        samples: Vec<f32>,
        sample_rate: u32,
        duration: f64,
        fail_window: Option<usize>,
        calls: usize,
    }

    impl AudioWindowSource for MemorySource {
        fn duration_seconds(&self) -> f64 {
            self.duration
        }

        fn read_window(
            &mut self,
            start_seconds: f64,
            duration_seconds: f64,
        ) -> Result<PcmWindow, AudioReadError> {
            let call = self.calls;
            self.calls += 1;
            if self.fail_window == Some(call) {
                return Err(AudioReadError::new("synthetic seek failure"));
            }
            let start = (start_seconds * self.sample_rate as f64) as usize;
            let end = ((start_seconds + duration_seconds) * self.sample_rate as f64) as usize;
            let start = start.min(self.samples.len());
            let end = end.min(self.samples.len()).max(start);
            Ok(PcmWindow {
                samples: self.samples[start..end].to_vec(),
                sample_rate: self.sample_rate,
                channels: 1,
            })
        }
    }

    fn tone_source(duration: f64, frequencies: &[(f32, f32)]) -> MemorySource {
        let sample_rate = 22_050_u32;
        let samples: Vec<f32> = (0..(sample_rate as f64 * duration) as usize)
            .map(|index| {
                let t = index as f32 / sample_rate as f32;
                frequencies
                    .iter()
                    .map(|(frequency, amplitude)| (2.0 * PI * frequency * t).sin() * amplitude)
                    .sum()
            })
            .collect();
        MemorySource {
            samples,
            sample_rate,
            duration,
            fail_window: None,
            calls: 0,
        }
    }

    fn naive_power_spectrum(samples: &[f32]) -> Vec<f32> {
        let n = samples.len();
        let mut output = Vec::with_capacity(n / 2 + 1);
        for k in 0..=n / 2 {
            let mut real = 0.0_f64;
            let mut imaginary = 0.0_f64;
            for (index, sample) in samples.iter().enumerate() {
                let angle = -2.0 * std::f64::consts::PI * k as f64 * index as f64 / n as f64;
                real += *sample as f64 * angle.cos();
                imaginary += *sample as f64 * angle.sin();
            }
            output.push((real * real + imaginary * imaginary) as f32);
        }
        output
    }

    #[test]
    fn real_fft_matches_naive_dft() {
        let n = 64_usize;
        let mut input = vec![0.0_f32; n];
        let mut state = 0x1234_5678_u32;
        for sample in &mut input {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *sample = ((state >> 8) as f32 / 16_777_215.0) - 0.5;
        }
        let expected = naive_power_spectrum(&input);
        let mut fft = RealFft::new(n);
        let mut actual = vec![0.0_f32; n / 2 + 1];
        fft.forward_into(&input, &mut actual);
        for (index, (actual, expected)) in actual.iter().zip(expected.iter()).enumerate() {
            let tolerance = 1.0e-4_f32.max(expected.abs() * 1.0e-4);
            assert!(
                (actual - expected).abs() <= tolerance,
                "bin {index}: actual {actual}, expected {expected}"
            );
        }
    }

    #[test]
    fn feature_blob_round_trip_and_rejects_wrong_version() {
        let mut values = [0.0_f32; FEATURE_COUNT];
        values[0] = 0.5;
        values[8 + 9] = 1.0;
        let features = FeatureVector::new(values, (1 << FEATURE_COUNT) - 1);
        assert_eq!(
            FeatureVector::from_bytes(&features.to_bytes()).unwrap(),
            features
        );
        assert_eq!(&features.to_bytes()[0..4], b"SRF2");
        let mut bytes = features.to_bytes();
        bytes[3] = b'1';
        assert_eq!(
            FeatureVector::from_bytes(&bytes),
            Err(FeatureCodecError::UnknownFormat)
        );
    }

    #[test]
    fn feature_vector_canonicalizes_invalid_dimensions() {
        let mut values = [0.9_f32; FEATURE_COUNT];
        values[1] = f32::NAN;
        let features = FeatureVector::new(values, 1);
        assert_eq!(features.values[0], 0.9);
        assert_eq!(features.values[1], 0.0);
        assert_eq!(features.values[2], 0.0);
        assert_eq!(features.valid_count(), 1);
    }

    #[test]
    fn similarity_ignores_missing_dimensions() {
        let mut left = [0.0_f32; FEATURE_COUNT];
        let mut right = [0.0_f32; FEATURE_COUNT];
        left[0] = 0.2;
        right[0] = 0.2;
        left[2] = 1.0;
        right[2] = 0.0;
        let left = FeatureVector::new(left, 1);
        let right = FeatureVector::new(right, 1);
        assert_eq!(left.similarity(&right), Some(1.0));
        assert_eq!(
            FeatureVector::new([0.0; FEATURE_COUNT], 0).similarity(&right),
            None
        );
    }

    #[test]
    fn analyzes_three_regions_and_returns_finite_features() {
        let mut source = tone_source(30.0, &[(440.0, 0.35), (880.0, 0.12)]);
        let result = analyze_track(&mut source, &AnalysisConfig::default()).unwrap();
        assert_eq!(result.windows_analyzed, 3);
        assert_eq!(result.windows_failed, 0);
        assert!((17.0..=18.5).contains(&result.sampled_seconds));
        assert!(result
            .features
            .values
            .iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(value)));
        assert!(result.features.valid_mask & 0x7f != 0);
        let chroma_total: f32 = result.features.values[8..20].iter().sum();
        assert!((chroma_total - 1.0).abs() < 1.0e-4);
        // A4 is chroma index 8 + 9.
        let a = result.features.values[17];
        let other_max = result.features.values[8..17]
            .iter()
            .chain(result.features.values[18..20].iter())
            .copied()
            .fold(0.0_f32, f32::max);
        assert!(a > other_max, "A bin {a} did not dominate {other_max}");
    }

    #[test]
    fn chroma_handles_detuned_recordings() {
        let detune = 2.0_f32.powf(30.0 / 1_200.0);
        let mut source = tone_source(12.0, &[(440.0 * detune, 0.4), (880.0 * detune, 0.1)]);
        let result = analyze_track(&mut source, &AnalysisConfig::default()).unwrap();
        assert!(
            result.tuning_cents.abs() > 15.0,
            "tuning estimate was {} cents",
            result.tuning_cents
        );
        let a = result.features.values[17];
        let other_max = result.features.values[8..17]
            .iter()
            .chain(result.features.values[18..20].iter())
            .copied()
            .fold(0.0_f32, f32::max);
        assert!(a > other_max, "A bin {a} did not dominate {other_max}");
    }

    #[test]
    fn tempo_is_detected_from_a_click_track() {
        let sample_rate = 22_050_u32;
        let duration = 12.0_f64;
        let bpm = 120.0_f32;
        let interval = 60.0 / bpm;
        let samples: Vec<f32> = (0..(sample_rate as f64 * duration) as usize)
            .map(|index| {
                let t = index as f32 / sample_rate as f32;
                let phase = (t / interval).fract();
                // Short decaying click with a little body.
                if phase < 0.02 {
                    (-phase * 220.0).exp() * (2.0 * PI * 1_000.0 * t).sin() * 0.8
                } else {
                    0.0
                }
            })
            .collect();
        let mut source = MemorySource {
            samples,
            sample_rate,
            duration,
            fail_window: None,
            calls: 0,
        };
        let result = analyze_track(&mut source, &AnalysisConfig::default()).unwrap();
        assert!(result.features.is_valid(7), "tempo was marked invalid");
        let detected = 50.0 + result.features.values[7] * 150.0;
        assert!(
            (detected - bpm).abs() < 8.0,
            "detected {detected} BPM, expected {bpm}"
        );
    }

    #[test]
    fn low_frequency_ratio_separates_bass_from_treble() {
        let mut bass = tone_source(12.0, &[(100.0, 0.5)]);
        let mut treble = tone_source(12.0, &[(3_000.0, 0.5)]);
        let bass = analyze_track(&mut bass, &AnalysisConfig::default()).unwrap();
        let treble = analyze_track(&mut treble, &AnalysisConfig::default()).unwrap();
        assert!(bass.features.values[6] > treble.features.values[6]);
        assert!(bass.features.values[6] > 0.5);
    }

    #[test]
    fn short_tracks_are_covered_by_three_shrunk_windows() {
        let mut source = tone_source(2.0, &[(440.0, 0.3)]);
        let result = analyze_track(&mut source, &AnalysisConfig::default()).unwrap();
        assert_eq!(result.windows_analyzed, 3);
        assert!((result.sampled_seconds - 2.0).abs() < 0.1);
    }

    #[test]
    fn silent_track_is_rejected() {
        let source = MemorySource {
            samples: vec![0.0; 22_050 * 4],
            sample_rate: 22_050,
            duration: 4.0,
            fail_window: None,
            calls: 0,
        };
        let mut source = source;
        assert!(matches!(
            analyze_track(&mut source, &AnalysisConfig::default()),
            Err(AnalysisError::NoUsableAudio)
        ));
    }

    #[test]
    fn a_failed_region_does_not_discard_the_other_regions() {
        let mut source = tone_source(30.0, &[(440.0, 0.3)]);
        source.fail_window = Some(1);
        let result = analyze_track(&mut source, &AnalysisConfig::default()).unwrap();
        assert_eq!(result.windows_analyzed, 2);
        assert_eq!(result.windows_failed, 1);
        assert!(result.features.is_valid(0));
    }

    #[test]
    fn analysis_is_deterministic() {
        let mut first_source = tone_source(15.0, &[(220.0, 0.3), (660.0, 0.1)]);
        let mut second_source = tone_source(15.0, &[(220.0, 0.3), (660.0, 0.1)]);
        let first = analyze_track(&mut first_source, &AnalysisConfig::default()).unwrap();
        let second = analyze_track(&mut second_source, &AnalysisConfig::default()).unwrap();
        assert_eq!(first.features.to_bytes(), second.features.to_bytes());
        assert_eq!(first.tuning_cents, second.tuning_cents);
    }

    #[test]
    fn invalid_config_is_rejected() {
        let mut source = tone_source(5.0, &[(440.0, 0.3)]);
        let config = AnalysisConfig {
            fft_size: 1_000,
            ..AnalysisConfig::default()
        };
        assert!(matches!(
            analyze_track(&mut source, &config),
            Err(AnalysisError::InvalidConfig(_))
        ));
    }
}
