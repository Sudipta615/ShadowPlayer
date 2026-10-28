//! Source apodizing-need detector.
//!
//! An apodizing reconstruction filter is useful when a recording carries
//! high-frequency ringing/aliasing left by the ADC or a mastering resampler.
//! There is no way to recover the ADC's transfer function from the samples, so
//! this module does **not** claim to identify a specific converter. It counts
//! bursts of energy near the source Nyquist limit — the band an apodizing
//! filter rolls off — and turns the count into the same "high count, use an
//! apodizing filter" guidance HQPlayer exposes as its `Apod` counter.
//!
//! The detector is deliberately a control/decode-path component:
//!
//! - every buffer is fixed at construction (no allocation per block);
//! - it runs on the decode thread, never in the output callback;
//! - it is a *guidance heuristic*, and its report says so.
//!
//! The count is per track. A host can render it next to the limited counter,
//! and the recommendation threshold is ten incidents, matching the manual's
//! "more than ten per track" wording.

use crate::buffer::MAX_CHANNELS;

/// Incidents above which an apodizing filter is recommended.
pub const APODIZING_RECOMMENDATION_THRESHOLD: u64 = 10;

/// Highest high-pass corner used by the detector. At 44.1/48 kHz the corner is
/// below this; at 88.2 kHz and above it is clamped here so the detector does
/// not chase content that is plainly audible-band.
const MAX_DETECTOR_HIGHPASS_HZ: f32 = 40_000.0;

/// Minimum detector high-pass corner. Below this the detector would react to
/// ordinary programme material rather than the near-Nyquist band.
const MIN_DETECTOR_HIGHPASS_HZ: f32 = 8_000.0;

/// A minimum burst length, in seconds, so a single click is not an incident.
const MIN_BURST_SECS: f32 = 0.0005;

/// One incident per burst; a new burst cannot be counted again until the
/// detector has seen the band fall back below the threshold.
const REFRACTORY_SECS: f32 = 0.010;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApodizingRecommendation {
    /// No incident has been observed yet.
    InsufficientData,
    /// The count is at or below the threshold.
    NotNeeded,
    /// More than [`APODIZING_RECOMMENDATION_THRESHOLD`] incidents; use an
    /// apodizing filter for this track.
    Recommended,
}

/// Per-track detector report, published in `PlaybackInfo.apodizing`.
#[derive(Debug, Clone, PartialEq)]
pub struct ApodizingReport {
    /// Number of near-Nyquist energy bursts counted in this track.
    pub incidents: u64,
    /// Source audio actually analysed (seconds).
    pub analyzed_seconds: f64,
    /// Incidents normalised to a minute of analysed audio.
    pub incidents_per_minute: f32,
    /// Count above which [`Self::recommendation`] is
    /// [`ApodizingRecommendation::Recommended`].
    pub threshold: u64,
    pub recommendation: ApodizingRecommendation,
    /// High-pass corner used for the source rate (Hz).
    pub highpass_hz: f32,
    /// Absolute burst threshold (dBFS).
    pub threshold_dbfs: f32,
    /// Source rate of the analysed stream (Hz).
    pub sample_rate: u32,
    /// Source channel count of the analysed stream.
    pub channels: usize,
}

impl Default for ApodizingReport {
    fn default() -> Self {
        Self {
            incidents: 0,
            analyzed_seconds: 0.0,
            incidents_per_minute: 0.0,
            threshold: APODIZING_RECOMMENDATION_THRESHOLD,
            recommendation: ApodizingRecommendation::InsufficientData,
            highpass_hz: 0.0,
            threshold_dbfs: -60.0,
            sample_rate: 0,
            channels: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct ChannelState {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

/// Fixed-size, allocation-free source detector.
pub struct ApodizingDetector {
    channels: [ChannelState; MAX_CHANNELS],
    sample_rate: f32,
    channels_in_use: usize,
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    highpass_hz: f32,
    threshold_dbfs: f32,
    threshold_linear: f32,
    min_burst_frames: u32,
    burst_frames: u32,
    refractory_remaining: u32,
    armed: bool,
    incidents: u64,
    analyzed_frames: u64,
    /// The report is a snapshot, rebuilt on telemetry reads rather than every
    /// block; keeping only scalar state here means `report()` does not borrow
    /// the audio path.
    last_report: ApodizingReport,
}

impl ApodizingDetector {
    pub fn new(sample_rate: f32) -> Self {
        let mut detector = Self {
            channels: [ChannelState::default(); MAX_CHANNELS],
            sample_rate: 0.0,
            channels_in_use: 0,
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            highpass_hz: 0.0,
            threshold_dbfs: -60.0,
            threshold_linear: 10.0f32.powf(-60.0 / 20.0),
            min_burst_frames: 1,
            burst_frames: 0,
            refractory_remaining: 0,
            armed: true,
            incidents: 0,
            analyzed_frames: 0,
            last_report: ApodizingReport::default(),
        };
        detector.reset(sample_rate);
        detector
    }

    /// Start a new track: clear the count and all filter state, and configure
    /// the high-pass for the source rate. Allocation-free.
    pub fn reset(&mut self, sample_rate: f32) {
        let rate = sample_rate.max(1.0);
        if (rate - self.sample_rate).abs() > 0.5 {
            self.sample_rate = rate;
            self.highpass_hz =
                (0.45 * rate).clamp(MIN_DETECTOR_HIGHPASS_HZ, MAX_DETECTOR_HIGHPASS_HZ);
            self.set_highpass(self.highpass_hz, rate);
        }
        self.channels = [ChannelState::default(); MAX_CHANNELS];
        self.channels_in_use = 0;
        self.burst_frames = 0;
        self.refractory_remaining = 0;
        self.armed = true;
        self.incidents = 0;
        self.analyzed_frames = 0;
        self.min_burst_frames = ((MIN_BURST_SECS * rate).round() as u32).max(1);
        self.refresh_report();
    }

    /// Analyse an interleaved decoded source block.
    ///
    /// `channels` is clamped to [`MAX_CHANNELS`]; extra interleaved channels
    /// are ignored rather than corrupting the frame walk. No allocation, no
    /// logging, no locks: this is called from the decode loop.
    pub fn analyze_interleaved(&mut self, samples: &[f32], channels: usize, sample_rate: f32) {
        let ch = channels.clamp(1, MAX_CHANNELS);
        let frames = samples.len() / ch;
        if frames == 0 {
            return;
        }
        if (sample_rate - self.sample_rate).abs() > 0.5 {
            // A rate change mid-track means the source was reopened; keep the
            // count but rebuild the filter for the new Nyquist.
            let incidents = self.incidents;
            let analyzed = self.analyzed_frames;
            self.reset(sample_rate);
            self.incidents = incidents;
            self.analyzed_frames = analyzed;
        }
        self.channels_in_use = ch;

        let refractory_frames = ((REFRACTORY_SECS * self.sample_rate) as u32).max(1);
        for frame in 0..frames {
            self.refractory_remaining = self.refractory_remaining.saturating_sub(1);
            let mut over = false;
            for channel in 0..ch {
                let x = samples[frame * ch + channel];
                let state = &mut self.channels[channel];
                let y = self.b0 * x + self.b1 * state.x1 + self.b2 * state.x2
                    - self.a1 * state.y1
                    - self.a2 * state.y2;
                state.x2 = state.x1;
                state.x1 = x;
                state.y2 = state.y1;
                state.y1 = y;
                if y.abs() > self.threshold_linear {
                    over = true;
                }
            }

            if over {
                self.burst_frames = self.burst_frames.saturating_add(1);
                if self.armed && self.burst_frames >= self.min_burst_frames {
                    self.incidents = self.incidents.saturating_add(1);
                    self.armed = false;
                    self.refractory_remaining = refractory_frames;
                }
            } else {
                self.burst_frames = 0;
                // Only a gap after the refractory window re-arms the counter,
                // so a continuous HF passage is one incident rather than one
                // incident every ten milliseconds.
                if self.refractory_remaining == 0 {
                    self.armed = true;
                }
            }
            self.analyzed_frames = self.analyzed_frames.saturating_add(1);
        }
        self.refresh_report();
    }

    pub fn report(&self) -> ApodizingReport {
        self.last_report.clone()
    }

    pub fn incidents(&self) -> u64 {
        self.incidents
    }

    fn refresh_report(&mut self) {
        let seconds = self.analyzed_frames as f64 / self.sample_rate.max(1.0) as f64;
        let recommendation = if self.analyzed_frames == 0 {
            ApodizingRecommendation::InsufficientData
        } else if self.incidents > APODIZING_RECOMMENDATION_THRESHOLD {
            ApodizingRecommendation::Recommended
        } else {
            ApodizingRecommendation::NotNeeded
        };
        self.last_report = ApodizingReport {
            incidents: self.incidents,
            analyzed_seconds: seconds,
            incidents_per_minute: if seconds > 0.0 {
                (self.incidents as f64 * 60.0 / seconds) as f32
            } else {
                0.0
            },
            threshold: APODIZING_RECOMMENDATION_THRESHOLD,
            recommendation,
            highpass_hz: self.highpass_hz,
            threshold_dbfs: self.threshold_dbfs,
            sample_rate: self.sample_rate as u32,
            channels: self.channels_in_use,
        };
    }

    fn set_highpass(&mut self, frequency: f32, sample_rate: f32) {
        // RBJ second-order high-pass, Q = 1/sqrt(2).
        let w0 = std::f32::consts::TAU * frequency / sample_rate;
        let cos_w0 = w0.cos();
        let alpha = w0.sin() / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
        let a0 = 1.0 + alpha;
        self.b0 = (1.0 + cos_w0) * 0.5 / a0;
        self.b1 = -(1.0 + cos_w0) / a0;
        self.b2 = (1.0 + cos_w0) * 0.5 / a0;
        self.a1 = -2.0 * cos_w0 / a0;
        self.a2 = (1.0 - alpha) / a0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analyse(detector: &mut ApodizingDetector, samples: &[f32], rate: f32) {
        detector.analyze_interleaved(samples, 1, rate);
    }

    fn sine(frequency: f32, rate: f32, seconds: f32, amplitude: f32) -> Vec<f32> {
        let frames = (rate * seconds) as usize;
        (0..frames)
            .map(|i| amplitude * (std::f32::consts::TAU * frequency * i as f32 / rate).sin())
            .collect()
    }

    #[test]
    fn silence_and_audible_tones_are_not_incidents() {
        let mut detector = ApodizingDetector::new(48_000.0);
        analyse(&mut detector, &vec![0.0; 48_000], 48_000.0);
        assert_eq!(detector.report().incidents, 0);
        analyse(&mut detector, &sine(1_000.0, 48_000.0, 0.5, 0.5), 48_000.0);
        assert_eq!(
            detector.report().incidents,
            0,
            "a 1 kHz tone is not near-Nyquist content"
        );
    }

    #[test]
    fn a_near_nyquist_burst_is_counted() {
        let mut detector = ApodizingDetector::new(48_000.0);
        let burst = sine(23_000.0, 48_000.0, 0.010, 0.25);
        analyse(&mut detector, &burst, 48_000.0);
        let report = detector.report();
        assert_eq!(report.incidents, 1, "{report:?}");
        assert_eq!(report.recommendation, ApodizingRecommendation::NotNeeded);
        assert!(report.analyzed_seconds > 0.0);
        assert!((report.highpass_hz - 21_600.0).abs() < 1.0);
    }

    #[test]
    fn the_recommendation_turns_on_above_ten_incidents() {
        let mut detector = ApodizingDetector::new(48_000.0);
        let burst = sine(23_000.0, 48_000.0, 0.005, 0.25);
        let silence = vec![0.0f32; 4_800];
        for _ in 0..11 {
            analyse(&mut detector, &burst, 48_000.0);
            analyse(&mut detector, &silence, 48_000.0);
        }
        let report = detector.report();
        assert_eq!(report.incidents, 11);
        assert_eq!(report.recommendation, ApodizingRecommendation::Recommended);
    }

    #[test]
    fn reset_clears_the_track_and_reconfigures_nyquist() {
        let mut detector = ApodizingDetector::new(48_000.0);
        analyse(
            &mut detector,
            &sine(23_000.0, 48_000.0, 0.01, 0.25),
            48_000.0,
        );
        assert!(detector.incidents() > 0);
        detector.reset(96_000.0);
        let report = detector.report();
        assert_eq!(report.incidents, 0);
        assert_eq!(
            report.recommendation,
            ApodizingRecommendation::InsufficientData
        );
        assert!((report.highpass_hz - 40_000.0).abs() < 1.0);
    }
}
