//! ReplayGain 2.0 gain and peak.
//!
//! ReplayGain 2.0 inherits the whole BS.1770-5 measurement and adds one
//! thing: a reference point. `RG = −18 LUFS − L`, where `L` is the gated
//! programme loudness. This suite checks that the engine's ReplayGain numbers
//! are that number — finite, reproducible, and consistent with the loudness
//! measurement they are derived from — and that the *album* gain is the
//! documented power mean rather than the arithmetic mean of the dB values,
//! which is the mistake the implementation exists to avoid.

mod common;

use engine::decode::scanner::{
    accumulate_album_replaygain, scan_track_loudness, LoudnessScanResult,
};
use engine::standards::replaygain_2_track_db;

/// Duration of the measurement programme.
const SECONDS: f32 = 2.0;

/// Write a 1 kHz sine at `amplitude` to a temporary WAV and scan it.
fn scan_sine(amplitude: f32, name: &str) -> (common::TempFile, LoudnessScanResult) {
    let frames = (common::SAMPLE_RATE as f32 * SECONDS) as usize;
    let samples = common::sine(1_000.0, amplitude, frames, common::SAMPLE_RATE);
    let file = common::write_wav(name, &samples, common::CHANNELS, common::SAMPLE_RATE);
    let result = scan_track_loudness(file.path())
        .unwrap_or_else(|| panic!("the engine must be able to scan its own fixture {name}"));
    (file, result)
}

#[test]
fn the_track_gain_is_the_replaygain_2_definition_applied_to_the_measurement() {
    let (_file, scan) = scan_sine(0.5, "rg_track");

    let lufs = scan
        .ebu_r128_loudness
        .expect("a measurable file must report an integrated loudness");
    let gain = scan
        .replaygain_track_db
        .expect("a measurable file must report a ReplayGain track gain");

    assert!(lufs.is_finite(), "the integrated loudness must be finite");
    assert!(gain.is_finite(), "the ReplayGain gain must be finite");

    let expected = replaygain_2_track_db(lufs);
    assert!(
        (gain - expected).abs() < 1e-3,
        "the ReplayGain track gain {gain:.4} dB must be −18 − {lufs:.4} = {expected:.4} dB"
    );
}

#[test]
fn a_quiet_programme_gets_a_positive_gain_and_a_loud_one_a_negative_gain() {
    // The sign is the whole point of the number: a programme quieter than the
    // −18 LUFS reference must be boosted.
    let (_quiet_file, quiet) = scan_sine(0.05, "rg_quiet");
    let (_loud_file, loud) = scan_sine(0.9, "rg_loud");

    let quiet_gain = quiet.replaygain_track_db.expect("a track gain");
    let loud_gain = loud.replaygain_track_db.expect("a track gain");

    assert!(
        quiet_gain > 0.0,
        "a programme at −26 LUFS must get a positive gain, got {quiet_gain:.3} dB"
    );
    assert!(
        loud_gain < 0.0,
        "a programme above −18 LUFS must get a negative gain, got {loud_gain:.3} dB"
    );
}

#[test]
fn the_track_peak_matches_the_signal_s_true_peak() {
    let (_file, scan) = scan_sine(0.5, "rg_peak");

    let peak = scan
        .replaygain_track_peak
        .expect("a measurable file must report a ReplayGain track peak");
    assert!(peak.is_finite(), "the peak must be finite");
    assert!(
        (peak - 0.5).abs() < 0.01,
        "a 0.5-amplitude sine must peak at 0.5, the engine reported {peak}"
    );

    // The peak and the dBTP figure are two views of the same measurement.
    let dbtp = scan
        .ebu_r128_peak_dbtp
        .expect("a measurable file must report a true peak");
    let from_dbtp = 10f64.powf(f64::from(dbtp) / 20.0);
    assert!(
        (from_dbtp - f64::from(peak)).abs() < 0.01,
        "the linear peak {peak} and the dBTP figure {dbtp} dBTP disagree"
    );
}

#[test]
fn two_scans_of_the_same_file_give_the_same_numbers() {
    // Reproducibility: a scanner that drifted between runs would write
    // different tags for the same album on every pass.
    let (file, first) = scan_sine(0.5, "rg_stable");
    let second = scan_track_loudness(file.path())
        .expect("the engine must be able to scan its own fixture twice");

    assert_eq!(
        first.ebu_r128_loudness, second.ebu_r128_loudness,
        "two scans of the same file must measure the same loudness"
    );
    assert_eq!(
        first.replaygain_track_db, second.replaygain_track_db,
        "two scans of the same file must produce the same gain"
    );
    assert_eq!(
        first.replaygain_track_peak, second.replaygain_track_peak,
        "two scans of the same file must produce the same peak"
    );
    assert_eq!(
        first.frames_scanned, second.frames_scanned,
        "two scans of the same file must decode the same number of frames"
    );
    assert!(
        first.frames_scanned > 0,
        "the scan must have decoded frames"
    );
}

#[test]
fn the_album_gain_of_a_single_track_is_that_track_s_gain() {
    let (_file, scan) = scan_sine(0.5, "rg_album_single");
    let album = accumulate_album_replaygain([&scan]).expect("one usable track must give an album");

    assert_eq!(album.track_count, 1);
    assert!(
        (album.album_gain_db - scan.replaygain_track_db.expect("a track gain")).abs() < 1e-3,
        "the album gain of a single track must be that track's gain"
    );
    assert!(
        (album.album_peak - scan.replaygain_track_peak.expect("a track peak")).abs() < 1e-6,
        "the album peak of a single track must be that track's peak"
    );
}

#[test]
fn the_album_gain_is_a_power_mean_not_the_mean_of_the_decibels() {
    // Two tracks 10 dB apart. The arithmetic mean of the two dB values is
    // 5 dB below the loud one; the power mean is 3.01 dB below it (half the
    // energy). The difference is 2 dB, and it is the difference that makes an
    // album play at the right level.
    let (_loud_file, loud) = scan_sine(0.9, "rg_album_loud");
    let (_quiet_file, quiet) = scan_sine(0.9 * 10f32.powf(-10.0 / 20.0), "rg_album_quiet");

    let loud_gain = loud.replaygain_track_db.expect("a track gain");
    let quiet_gain = quiet.replaygain_track_db.expect("a track gain");
    let loud_lufs = loud.ebu_r128_loudness.expect("a loudness");

    let album = accumulate_album_replaygain([&loud, &quiet]).expect("two usable tracks");
    assert_eq!(album.track_count, 2);

    let arithmetic_mean = (loud_gain + quiet_gain) / 2.0;
    let power_mean = replaygain_2_track_db(
        10.0 * ((10f64.powf(f64::from(loud_lufs) / 10.0)
            + 10f64.powf(f64::from(quiet.ebu_r128_loudness.expect("a loudness")) / 10.0))
            / 2.0)
            .log10() as f32,
    );
    assert!(
        (album.album_gain_db - power_mean).abs() < 1e-3,
        "the album gain {} must be the power mean {power_mean:.4}",
        album.album_gain_db
    );
    assert!(
        (album.album_gain_db - arithmetic_mean).abs() > 1.0,
        "the album gain must NOT be the arithmetic mean of the dB values \
         (arithmetic {arithmetic_mean:.4}, album {:.4})",
        album.album_gain_db
    );
    // The power mean is the *conservative* one: it sits below the arithmetic
    // mean, so an album padded with quiet tracks does not play too loudly.
    assert!(
        album.album_gain_db < arithmetic_mean,
        "the album gain {:.4} must be below the arithmetic mean {arithmetic_mean:.4}",
        album.album_gain_db
    );
    // And it must lie between the two tracks' own gains.
    assert!(
        album.album_gain_db > loud_gain && album.album_gain_db < quiet_gain,
        "the album gain {:.4} must lie between the track gains {loud_gain:.4} and \
         {quiet_gain:.4}",
        album.album_gain_db
    );
}

#[test]
fn the_album_peak_is_the_largest_track_peak() {
    let (_loud_file, loud) = scan_sine(0.9, "rg_peak_album_loud");
    let (_quiet_file, quiet) = scan_sine(0.2, "rg_peak_album_quiet");

    let album = accumulate_album_replaygain([&loud, &quiet]).expect("two usable tracks");
    let loud_peak = loud.replaygain_track_peak.expect("a track peak");
    let quiet_peak = quiet.replaygain_track_peak.expect("a track peak");

    assert_eq!(
        album.album_peak,
        loud_peak.max(quiet_peak),
        "the album peak is the maximum of the track peaks"
    );
}

#[test]
fn an_album_with_no_usable_track_has_no_album_gain() {
    // Writing an album gain derived from zero tracks would be a fabricated
    // number, and a player applying it would attenuate the whole album.
    let mut unusable = LoudnessScanResult {
        ebu_r128_loudness: None,
        ebu_r128_peak_dbtp: None,
        replaygain_track_db: None,
        replaygain_track_peak: None,
        lra_lu: None,
        frames_scanned: 0,
    };
    assert!(
        accumulate_album_replaygain([&unusable]).is_none(),
        "an album with no measurement must have no album gain"
    );
    assert!(
        accumulate_album_replaygain(std::iter::empty()).is_none(),
        "an empty album must have no album gain"
    );

    // A track whose loudness is not finite is skipped rather than counted.
    unusable.ebu_r128_loudness = Some(f32::NAN);
    assert!(
        accumulate_album_replaygain([&unusable]).is_none(),
        "a track with a non-finite loudness must not produce an album gain"
    );
}
