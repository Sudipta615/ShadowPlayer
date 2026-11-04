pub(crate) fn log_frequency_normalized(frequency: f32, minimum: f32, maximum: f32) -> f32 {
    let lo = minimum.max(1.0);
    let hi = maximum.max(lo + 1.0);
    ((frequency.max(lo).ln() - lo.ln()) / (hi.ln() - lo.ln())).clamp(0.0, 1.0)
}

pub(crate) fn quantile_sorted(sorted: &[f32], quantile: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((sorted.len() - 1) as f32 * quantile.clamp(0.0, 1.0)).round() as usize;
    sorted[index]
}

pub(crate) fn mean(values: &[f32]) -> f32 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f32>() / values.len() as f32
    }
}

pub(crate) fn tempo_prior(bpm: f32) -> f32 {
    let octaves = (bpm / 120.0).log2();
    (-0.5 * (octaves / 0.7).powi(2)).exp().clamp(0.05, 1.0)
}

/// Estimate tempo from a log-magnitude spectral-flux onset envelope.
///
/// The estimator uses normalized autocorrelation, harmonic summation, a broad
/// log-normal tempo prior centered near 120 BPM, and parabolic peak
/// interpolation. Confidence combines correlation strength with peak
/// prominence, so weak or ambiguous periodicity is omitted from similarity.
pub(crate) fn estimate_tempo(onset: &[f32], frame_rate: f32) -> (f32, f32) {
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
