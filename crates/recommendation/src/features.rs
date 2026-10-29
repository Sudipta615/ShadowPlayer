use crate::audio::{downmix_and_resample_into, ResamplePlan};
use crate::{AudioReadError, AudioWindowSource};
use std::error::Error;
use std::f32::consts::PI;
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

struct WindowFeatures {
    vector: FeatureVector,
    tempo_bpm: f32,
    tempo_confidence: f32,
    tuning_cents: f32,
    chroma_weight: f32,
}

struct AnalysisPlan {
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
    fn new(config: &AnalysisConfig) -> Self {
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

    fn extract(&mut self, samples: &[f32]) -> Option<WindowFeatures> {
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

#[derive(Clone, Copy, Default)]
struct Complex {
    re: f32,
    im: f32,
}

impl Complex {
    #[inline]
    fn conjugate(self) -> Self {
        Self {
            re: self.re,
            im: -self.im,
        }
    }
}

/// Iterative radix-2 complex FFT with a precomputed bit-reversal table and
/// twiddle factors. The table lookup avoids the accumulated error of updating
/// a complex twiddle by recurrence in every butterfly.
struct ComplexFft {
    n: usize,
    twiddles: Vec<Complex>,
    bit_reverse: Vec<usize>,
}

impl ComplexFft {
    fn new(n: usize) -> Self {
        assert!(n.is_power_of_two(), "FFT size must be a power of two");
        let twiddles = (0..n / 2)
            .map(|k| {
                let angle = -2.0 * PI * k as f32 / n as f32;
                Complex {
                    re: angle.cos(),
                    im: angle.sin(),
                }
            })
            .collect();
        let bits = n.trailing_zeros() as usize;
        let bit_reverse = (0..n)
            .map(|index| {
                if bits == 0 {
                    0
                } else {
                    index.reverse_bits() >> (usize::BITS as usize - bits)
                }
            })
            .collect();
        Self {
            n,
            twiddles,
            bit_reverse,
        }
    }

    fn forward(&self, values: &mut [Complex]) {
        debug_assert_eq!(values.len(), self.n);
        for (index, reversed) in self.bit_reverse.iter().enumerate() {
            if index < *reversed {
                values.swap(index, *reversed);
            }
        }
        let mut len = 2_usize;
        while len <= self.n {
            let half = len / 2;
            let step = self.n / len;
            for base in (0..self.n).step_by(len) {
                for offset in 0..half {
                    let twiddle = self.twiddles[offset * step];
                    let even = values[base + offset];
                    let odd_value = values[base + offset + half];
                    let odd = Complex {
                        re: odd_value.re * twiddle.re - odd_value.im * twiddle.im,
                        im: odd_value.re * twiddle.im + odd_value.im * twiddle.re,
                    };
                    values[base + offset] = Complex {
                        re: even.re + odd.re,
                        im: even.im + odd.im,
                    };
                    values[base + offset + half] = Complex {
                        re: even.re - odd.re,
                        im: even.im - odd.im,
                    };
                }
            }
            len <<= 1;
        }
    }
}

/// Real-input FFT using the standard N/2 complex packing. It produces the same
/// one-sided power spectrum as a complex N-point FFT at roughly half the cost.
struct RealFft {
    n: usize,
    half: usize,
    inner: ComplexFft,
    full_twiddles: Vec<Complex>,
    scratch: Vec<Complex>,
}

impl RealFft {
    fn new(n: usize) -> Self {
        assert!(n.is_power_of_two() && n >= 4, "real FFT needs N >= 4");
        let half = n / 2;
        let full_twiddles = (0..half)
            .map(|k| {
                let angle = -2.0 * PI * k as f32 / n as f32;
                Complex {
                    re: angle.cos(),
                    im: angle.sin(),
                }
            })
            .collect();
        Self {
            n,
            half,
            inner: ComplexFft::new(half),
            full_twiddles,
            scratch: vec![Complex::default(); half],
        }
    }

    fn forward_into(&mut self, input: &[f32], output: &mut [f32]) {
        debug_assert_eq!(input.len(), self.n);
        debug_assert_eq!(output.len(), self.half + 1);
        for (index, value) in self.scratch.iter_mut().enumerate() {
            *value = Complex {
                re: input[index * 2],
                im: input[index * 2 + 1],
            };
        }
        self.inner.forward(&mut self.scratch);

        let first = self.scratch[0];
        output[0] = (first.re + first.im) * (first.re + first.im);
        output[self.half] = (first.re - first.im) * (first.re - first.im);
        for (k, output_value) in output.iter_mut().enumerate().take(self.half).skip(1) {
            let current = self.scratch[k];
            let mirrored = self.scratch[self.half - k].conjugate();
            let even_re = 0.5 * (current.re + mirrored.re);
            let even_im = 0.5 * (current.im + mirrored.im);
            let diff_re = current.re - mirrored.re;
            let diff_im = current.im - mirrored.im;
            let odd_re = 0.5 * diff_im;
            let odd_im = -0.5 * diff_re;
            let twiddle = self.full_twiddles[k];
            let rotated_re = twiddle.re * odd_re - twiddle.im * odd_im;
            let rotated_im = twiddle.re * odd_im + twiddle.im * odd_re;
            let real = even_re + rotated_re;
            let imaginary = even_im + rotated_im;
            *output_value = real * real + imaginary * imaginary;
        }
    }
}

fn log_frequency_normalized(frequency: f32, minimum: f32, maximum: f32) -> f32 {
    let lo = minimum.max(1.0);
    let hi = maximum.max(lo + 1.0);
    ((frequency.max(lo).ln() - lo.ln()) / (hi.ln() - lo.ln())).clamp(0.0, 1.0)
}

fn quantile_sorted(sorted: &[f32], quantile: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((sorted.len() - 1) as f32 * quantile.clamp(0.0, 1.0)).round() as usize;
    sorted[index]
}

fn mean(values: &[f32]) -> f32 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f32>() / values.len() as f32
    }
}

fn tempo_prior(bpm: f32) -> f32 {
    let octaves = (bpm / 120.0).log2();
    (-0.5 * (octaves / 0.7).powi(2)).exp().clamp(0.05, 1.0)
}

/// Estimate tempo from a log-magnitude spectral-flux onset envelope.
///
/// The estimator uses normalized autocorrelation, harmonic summation, a broad
/// log-normal tempo prior centered near 120 BPM, and parabolic peak
/// interpolation. Confidence combines correlation strength with peak
/// prominence, so weak or ambiguous periodicity is omitted from similarity.
fn estimate_tempo(onset: &[f32], frame_rate: f32) -> (f32, f32) {
    const MIN_BPM: f32 = 50.0;
    const MAX_BPM: f32 = 200.0;
    if onset.len() < 40 || !frame_rate.is_finite() || frame_rate <= 0.0 {
        return (0.0, 0.0);
    }
    let average = mean(onset);
    let centered: Vec<f32> = onset.iter().map(|value| *value - average).collect();
    let min_lag = ((frame_rate * 60.0 / MAX_BPM).floor() as usize).max(2);
    let max_lag = ((frame_rate * 60.0 / MIN_BPM).ceil() as usize).min(centered.len() / 2);
    if max_lag <= min_lag {
        return (0.0, 0.0);
    }

    let mut correlations = vec![0.0_f32; max_lag + 1];
    for lag in min_lag..=max_lag {
        let mut numerator = 0.0_f64;
        let mut left_power = 0.0_f64;
        let mut right_power = 0.0_f64;
        for index in lag..centered.len() {
            let left = centered[index] as f64;
            let right = centered[index - lag] as f64;
            numerator += left * right;
            left_power += left * left;
            right_power += right * right;
        }
        let denominator = (left_power * right_power).sqrt();
        if denominator > 1.0e-12 {
            correlations[lag] = (numerator / denominator).clamp(0.0, 1.0) as f32;
        }
    }

    let mut scores = vec![0.0_f32; max_lag + 1];
    let mut best_lag = 0_usize;
    let mut best_score = 0.0_f32;
    for lag in min_lag..=max_lag {
        let mut combined = correlations[lag];
        if lag * 2 <= max_lag {
            combined += 0.5 * correlations[lag * 2];
        }
        if lag * 3 <= max_lag {
            combined += 0.25 * correlations[lag * 3];
        }
        let bpm = 60.0 * frame_rate / lag as f32;
        let score = combined * tempo_prior(bpm);
        scores[lag] = score;
        if score > best_score {
            best_score = score;
            best_lag = lag;
        }
    }
    if best_lag == 0 || best_score <= 1.0e-9 {
        return (0.0, 0.0);
    }

    let second_best = scores
        .iter()
        .enumerate()
        .filter(|(lag, _)| (*lag as isize - best_lag as isize).abs() > 2)
        .map(|(_, score)| *score)
        .fold(0.0_f32, f32::max);
    let prominence = ((best_score - second_best) / best_score).clamp(0.0, 1.0);
    let previous = if best_lag > min_lag {
        scores[best_lag - 1]
    } else {
        scores[best_lag]
    };
    let next = if best_lag < max_lag {
        scores[best_lag + 1]
    } else {
        scores[best_lag]
    };
    let denominator = previous - 2.0 * best_score + next;
    let delta = if denominator.abs() > 1.0e-9 {
        (0.5 * (previous - next) / denominator).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    let lag = best_lag as f32 + delta;
    let mut bpm = 60.0 * frame_rate / lag.max(0.5);
    while bpm < MIN_BPM {
        bpm *= 2.0;
    }
    while bpm > MAX_BPM {
        bpm *= 0.5;
    }
    let confidence = (correlations[best_lag] * (0.5 + 0.5 * prominence)).clamp(0.0, 1.0);
    (bpm, confidence)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AudioReadError, PcmWindow};

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
