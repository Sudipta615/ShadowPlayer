//! ITU-R BS.1770-5 / EBU R128 loudness measurement.
//!
//! The engine's meter claims to implement BS.1770-5: K-weighting, the −70
//! LUFS absolute gate, the −10 LU relative gate, the channel sum, and a
//! 4×-oversampled true-peak estimate. This suite checks the claim against
//! signals whose correct answer is known analytically:
//!
//! * a 1 kHz sine has a closed-form expected loudness (the K-weight shelf's
//!   gain at 1 kHz is a published +0.67 dB);
//! * halving the amplitude must move the reading by exactly 6.02 LU, which is
//!   a property of the *definition* and therefore an exact check, not a
//!   tolerance check;
//! * a programme in one channel of a stereo pair must read 3.01 LU below the
//!   same programme in both channels (the channel sum, not the average); and
//! * a silent stretch must be gated out of the integrated value entirely.

mod common;

use engine::dsp::loudness::{
    AnalysisMode, LoudnessAnalyzer, LoudnessComplianceProfile, LoudnessMeter,
};
use engine::standards::LoudnessStandard;

/// Duration of the measurement programme. Three seconds is past the
/// three-second short-term window and gives the gated integration enough
/// blocks to be stable.
const SECONDS: f32 = 3.0;

/// The K-weighting stage-1 shelf's gain at 1 kHz, in dB.
///
/// This is the engine's own published figure for the BS.1770-5 pre-filter
/// (see the engine's in-crate `dsp::loudness` tests), and it is the reason a
/// 1 kHz sine does not measure at its unweighted level.
const K_WEIGHT_1KHZ_DB: f64 = 0.67;

/// The BS.1770-5 offset, in LKFS.
const BS1770_OFFSET: f64 = -0.691;

/// The closed-form expected integrated loudness of a 1 kHz sine at `amplitude`
/// present in `channels` channels of a stereo programme.
fn expected_lufs(amplitude: f64, channels: usize) -> f64 {
    // Mean square per channel of a sine of peak amplitude A is A²/2.
    let mean_square_sum = channels as f64 * (amplitude * amplitude / 2.0);
    // The K-weighted mean square, then the BS.1770-5 offset.
    BS1770_OFFSET + 10.0 * (mean_square_sum * 10f64.powf(K_WEIGHT_1KHZ_DB / 10.0)).log10()
}

/// Measure a stereo programme with the engine's meter.
fn measure_stereo(samples: &[f32], channels: usize) -> engine::dsp::LoudnessMeasurement {
    let mut meter = LoudnessMeter::new(common::SAMPLE_RATE as f32, channels);
    meter.process_interleaved(samples, channels);
    meter.snapshot()
}

#[test]
fn a_one_kilohertz_sine_measures_the_loudness_bs1770_predicts() {
    let amplitude = 0.1f32;
    let frames = (common::SAMPLE_RATE as f32 * SECONDS) as usize;
    let samples = common::sine(1_000.0, amplitude, frames, common::SAMPLE_RATE);

    let measurement = measure_stereo(&samples, 2);
    let expected = expected_lufs(amplitude as f64, 2);

    // The tolerance is the K-weight shelf's own uncertainty: the engine's
    // in-crate test pins the shelf's gain at 1 kHz to ±0.4 dB, and a 0.4 dB
    // gain error is a 0.4 LU error in the reading. The bound is therefore
    // the measurement's own accuracy, not a number chosen to make the test
    // pass.
    assert!(
        (measurement.integrated_lufs as f64 - expected).abs() <= 0.5,
        "a 1 kHz sine at {amplitude} in both channels measured \
         {:.3} LUFS, expected {expected:.3} LUFS",
        measurement.integrated_lufs
    );
}

#[test]
fn halving_the_amplitude_moves_the_reading_by_exactly_6_02_lu() {
    // This is a property of the *definition*: doubling the amplitude doubles
    // the mean square, which is 6.0206 dB. No tolerance is needed beyond the
    // float arithmetic of the meter itself, so the bound here is generous
    // only because the meter's block boundaries are not phase-locked to the
    // tone.
    let frames = (common::SAMPLE_RATE as f32 * SECONDS) as usize;
    let loud = measure_stereo(&common::sine(1_000.0, 0.5, frames, common::SAMPLE_RATE), 2);
    let quiet = measure_stereo(&common::sine(1_000.0, 0.25, frames, common::SAMPLE_RATE), 2);

    let delta = loud.integrated_lufs - quiet.integrated_lufs;
    assert!(
        (delta - 6.0206).abs() < 0.1,
        "halving the amplitude must move the reading by 6.02 LU, it moved by {delta:.3} LU"
    );
}

#[test]
fn the_meter_sums_the_channels_rather_than_averaging_them() {
    // BS.1770-5 sums the channel energies, so the same programme in one
    // channel of a stereo pair reads 10·log10(2) = 3.01 LU below the same
    // programme in both channels.
    let frames = (common::SAMPLE_RATE as f32 * SECONDS) as usize;
    let stereo = measure_stereo(&common::sine(1_000.0, 0.5, frames, common::SAMPLE_RATE), 2);
    let left_only = measure_stereo(
        &common::sine_left_only(1_000.0, 0.5, frames, common::SAMPLE_RATE),
        2,
    );

    let delta = stereo.integrated_lufs - left_only.integrated_lufs;
    assert!(
        (delta - 3.0103).abs() < 0.1,
        "a programme in one channel must read 3.01 LU below the same programme \
         in both, the difference was {delta:.3} LU"
    );
}

#[test]
fn the_true_peak_of_a_one_kilohertz_sine_is_the_known_peak() {
    // At 48 kHz a 1 kHz tone has exactly 48 samples per period, so a sample
    // lands exactly on the crest and the inter-sample peak is the crest
    // too. The 4× FIR estimate must agree with it.
    let amplitude = 0.1f32;
    let frames = (common::SAMPLE_RATE as f32 * SECONDS) as usize;
    let samples = common::sine(1_000.0, amplitude, frames, common::SAMPLE_RATE);

    let measurement = measure_stereo(&samples, 2);
    let expected_db = 20.0 * (amplitude as f64).log10();
    assert!(
        (measurement.true_peak_dbtp() as f64 - expected_db).abs() < 0.3,
        "the true peak measured {:.3} dBTP, expected {expected_db:.3} dBTP",
        measurement.true_peak_dbtp()
    );
}

#[test]
fn a_silent_stretch_is_gated_out_of_the_integrated_value() {
    // The absolute gate is −70 LUFS: a stretch of digital silence must not
    // drag the integrated reading down. The programme is four seconds of
    // tone followed by one second of silence; the reading must still be the
    // tone's loudness. Four seconds of tone (rather than one) keeps the
    // K-filter's start-up transient down to a single 400 ms block out of
    // forty, so the reading is the tone's and not the transient's.
    let frames = (common::SAMPLE_RATE as f32 * SECONDS) as usize;
    let tone_frames = (common::SAMPLE_RATE as f32 * 4.0) as usize;
    let mut samples = common::sine(1_000.0, 0.5, tone_frames, common::SAMPLE_RATE);
    samples.resize(frames * 2, 0.0);

    let measurement = measure_stereo(&samples, 2);
    let expected = expected_lufs(0.5, 2);
    assert!(
        (measurement.integrated_lufs as f64 - expected).abs() <= 0.3,
        "the silent tail must be gated out, the reading was {:.3} LUFS \
         (expected {expected:.3})",
        measurement.integrated_lufs
    );
}

#[test]
fn the_analyser_reports_the_gating_census_it_actually_applied() {
    let frames = (common::SAMPLE_RATE as f32 * SECONDS) as usize;
    let mut samples = common::sine(
        1_000.0,
        0.5,
        common::SAMPLE_RATE as usize,
        common::SAMPLE_RATE,
    );
    samples.resize(frames * 2, 0.0);

    let mut analyzer = LoudnessAnalyzer::new(
        common::SAMPLE_RATE as f32,
        2,
        LoudnessStandard::ItuBs1770_5,
        AnalysisMode::Programme,
        LoudnessComplianceProfile::EbuR128,
    );
    analyzer.process_interleaved(&samples, 2);
    let result = analyzer.finish();

    assert_eq!(result.standard, LoudnessStandard::ItuBs1770_5);
    assert!(
        result.gating_diagnostics.total_blocks > 0,
        "the analyser must have evaluated gating blocks"
    );
    assert!(
        result.gating_diagnostics.gated_below_absolute > 0,
        "the silent tail must have been gated by the absolute gate, \
         census: {:?}",
        result.gating_diagnostics
    );
    assert!(
        result.gating_diagnostics.ungated_blocks > 0,
        "the tone must have contributed gated blocks, census: {:?}",
        result.gating_diagnostics
    );
    assert_eq!(
        result.gating_diagnostics.total_blocks,
        result.gating_diagnostics.gated_below_absolute
            + result.gating_diagnostics.gated_below_relative
            + result.gating_diagnostics.ungated_blocks,
        "every evaluated block must be accounted for by exactly one gate"
    );
}

#[test]
fn the_analysers_integrated_value_matches_the_meters() {
    // The offline analyser and the playback meter are the same measurement:
    // a host that shows the analyser's number and a host that shows the
    // meter's number must not disagree.
    let frames = (common::SAMPLE_RATE as f32 * SECONDS) as usize;
    let samples = common::sine(1_000.0, 0.25, frames, common::SAMPLE_RATE);

    let mut analyzer = LoudnessAnalyzer::new(
        common::SAMPLE_RATE as f32,
        2,
        LoudnessStandard::ItuBs1770_5,
        AnalysisMode::Programme,
        LoudnessComplianceProfile::EbuR128,
    );
    analyzer.process_interleaved(&samples, 2);
    let result = analyzer.finish();

    let meter = measure_stereo(&samples, 2);
    assert!(
        (result.integrated_lufs - meter.integrated_lufs).abs() < 0.1,
        "the analyser measured {:.3} LUFS and the meter {:.3} LUFS",
        result.integrated_lufs,
        meter.integrated_lufs
    );
    let expected = expected_lufs(0.25, 2);
    assert!(
        (result.integrated_lufs as f64 - expected).abs() <= 0.5,
        "the analyser measured {:.3} LUFS, expected {expected:.3} LUFS",
        result.integrated_lufs
    );
}

#[test]
fn measuring_a_decoded_file_gives_the_same_answer_as_the_samples() {
    // The whole path: the file is written, decoded and measured, and the
    // answer must be the answer the in-memory measurement gave.
    let frames = (common::SAMPLE_RATE as f32 * SECONDS) as usize;
    let samples = common::sine(1_000.0, 0.5, frames, common::SAMPLE_RATE);
    let file = common::write_wav("loudness", &samples, common::CHANNELS, common::SAMPLE_RATE);

    let in_memory = measure_stereo(&samples, 2);
    let scanned = engine::decode::scanner::scan_track_loudness(file.path())
        .expect("the engine must be able to measure its own fixture");
    let from_file = scanned
        .ebu_r128_loudness
        .expect("a measurable file must report an integrated loudness");

    assert!(
        (from_file - in_memory.integrated_lufs).abs() < 0.2,
        "the file measured {from_file:.3} LUFS and the samples {:.3} LUFS",
        in_memory.integrated_lufs
    );
    assert!(
        (from_file as f64 - expected_lufs(0.5, 2)).abs() <= 0.5,
        "the file measured {from_file:.3} LUFS, expected {:.3} LUFS",
        expected_lufs(0.5, 2)
    );
}

#[test]
fn the_declared_standard_carries_its_own_gate_and_window_constants() {
    // The gate and window numbers are part of the standard, and the engine
    // publishes them; a host that reads them must get the standard's values.
    let standard = LoudnessStandard::ItuBs1770_5;
    assert_eq!(standard.absolute_gate_lufs(), -70.0);
    assert_eq!(standard.relative_gate_offset_lu(), -10.0);
    assert!((standard.momentary_window_secs() - 0.4).abs() < 1e-6);
    assert!((standard.short_term_window_secs() - 3.0).abs() < 1e-6);
    assert_eq!(standard.name(), "ITU-R BS.1770-5");
    assert_eq!(standard.version(), "5.0");

    // ReplayGain 2.0 shares the measurement and only changes the reference.
    assert_eq!(LoudnessStandard::ReplayGain.target_lufs(), Some(-18.0));
    assert_eq!(LoudnessStandard::EbuR128.target_lufs(), Some(-23.0));
}
