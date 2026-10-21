//! Per-stage bit-perfect reporting: which stage invalidates the verdict, and
//! that a fully bypassed ("pure direct") chain reports **no** cause at all.
//!
//! The engine has exactly one bit-perfect verdict — `BitPerfectReport` — and
//! the report names the *stage* that broke it through the typed
//! [`BitPerfectCause`] enum. This suite drives the report through the public
//! [`DspPipeline`] API and asserts both halves of the contract:
//!
//! * a chain with every user stage bypassed, matching rates, a lossless
//!   container and a direct/exclusive transport reports `cause == None` and
//!   `is_bit_perfect == true`; and
//! * each stage that *is* engaged produces the cause that names it, and the
//!   per-stage `*_bypassed` flags say which stage it was.

use engine::diagnostics::BitPerfectCause;
use engine::dsp::loudness::LoudnessMode;
use engine::dsp::pipeline::{BitPerfectReport, BitPerfectResult, DspPipeline, OutputSampleFormat};

/// A chain with every user DSP stage bypassed: the "pure direct" starting
/// point every bit-perfect assertion is made from.
///
/// The limiter is disabled explicitly because the engine's default
/// configuration enables it, and a limiter is by definition a sample-altering
/// stage.
fn pure_direct_pipeline() -> DspPipeline {
    DspPipeline::builder(48_000.0)
        .with_eq(false)
        .with_limiter_disabled()
        .with_crossfeed(false, config::CrossfeedProfile::Bauer)
        .with_stereo_width(1.0)
        .with_loudness(LoudnessMode::Off, -18.0)
        .build()
}

/// The report for a lossless 24-bit source at 48 kHz played to a 32-bit float
/// exclusive output — every condition satisfied.
fn ideal_report(pipeline: &DspPipeline) -> BitPerfectReport {
    pipeline.bit_perfect_report_with_format(
        48_000, // source rate
        48_000, // output rate
        24,     // source bit depth
        32,     // output bit depth
        OutputSampleFormat::F32,
        false, // resampler inactive
        true,  // output opened direct/exclusive
    )
}

#[test]
fn a_fully_bypassed_chain_reports_no_cause_at_all() {
    let pipeline = pure_direct_pipeline();
    let report = ideal_report(&pipeline);

    assert_eq!(
        report.cause, None,
        "a pure-direct chain must report no cause, got {:?} ({:?})",
        report.cause, report.reason
    );
    assert!(
        report.reason.is_none(),
        "no cause means no reason string either, got {:?}",
        report.reason
    );
    assert!(report.is_bit_perfect, "the verdict must be bit-perfect");
    assert_eq!(report.result, BitPerfectResult::BitPerfect);
    assert!(
        report.bit_perfect_samples,
        "the sample path must be untouched"
    );
    assert!(report.bit_perfect_transport, "the transport must be direct");
}

#[test]
fn a_fully_bypassed_chain_reports_every_stage_as_bypassed() {
    let pipeline = pure_direct_pipeline();
    let report = ideal_report(&pipeline);

    assert!(report.volume_unity, "volume must be unity");
    assert!(report.eq_bypassed, "EQ must be bypassed");
    assert!(report.dynamics_bypassed, "dynamics must be bypassed");
    assert!(report.compressor_bypassed, "compressor must be bypassed");
    assert!(report.convolution_bypassed, "convolution must be bypassed");
    assert!(report.correction_bypassed, "correction must be bypassed");
    assert!(report.crossfeed_bypassed, "crossfeed must be bypassed");
    assert!(report.stereo_bypassed, "stereo width must be bypassed");
    assert!(report.limiter_bypassed, "limiter must be bypassed");
    assert!(report.loudness_bypassed, "loudness must be bypassed");
    assert!(report.resampler_bypassed, "the resampler must be bypassed");
    assert!(report.sample_rate_matched, "the rates must match");
    assert!(
        report.bit_depth_not_truncated,
        "a 24-bit source in a 32-bit container is not truncated"
    );
    assert!(
        report.format_conversion_lossless,
        "24-bit into a float container is lossless"
    );
    assert!(report.output_exclusive, "the transport must be exclusive");
}

#[test]
fn a_non_unity_volume_names_the_volume_as_the_cause() {
    let mut pipeline = pure_direct_pipeline();
    pipeline.volume.gain = 0.5;
    pipeline.volume.target_gain = 0.5;

    let report = ideal_report(&pipeline);
    assert_eq!(report.cause, Some(BitPerfectCause::VolumeNotUnity));
    assert!(!report.volume_unity);
    assert!(
        !report.is_bit_perfect,
        "a non-unity volume can never be bit-perfect"
    );
    assert_eq!(report.result, BitPerfectResult::Dsp);
}

#[test]
fn an_active_eq_names_the_eq_as_the_cause() {
    let mut pipeline = pure_direct_pipeline();
    pipeline.eq.set_enabled(true);

    let report = ideal_report(&pipeline);
    assert_eq!(report.cause, Some(BitPerfectCause::EqActive));
    assert!(!report.eq_bypassed, "the EQ must be reported as active");
    assert!(
        report.dynamics_bypassed,
        "the EQ is its own cause, not a dynamics cause"
    );
    assert!(!report.is_bit_perfect);
}

#[test]
fn an_active_crossfeed_names_the_dynamics_as_the_cause() {
    let mut pipeline = pure_direct_pipeline();
    pipeline.crossfeed.set_enabled(true);

    let report = ideal_report(&pipeline);
    assert_eq!(report.cause, Some(BitPerfectCause::DynamicsActive));
    assert!(
        !report.crossfeed_bypassed,
        "the crossfeed must be reported active"
    );
    assert!(!report.dynamics_bypassed);
    assert!(!report.is_bit_perfect);
}

#[test]
fn an_active_stereo_enhancer_names_the_dynamics_as_the_cause() {
    let mut pipeline = pure_direct_pipeline();
    pipeline.stereo_enhancer.set_enabled(true);
    pipeline.stereo_enhancer.set_width(1.5);

    let report = ideal_report(&pipeline);
    assert_eq!(report.cause, Some(BitPerfectCause::DynamicsActive));
    assert!(!report.stereo_bypassed);
}

#[test]
fn an_enabled_limiter_names_the_dynamics_as_the_cause() {
    let mut pipeline = pure_direct_pipeline();
    pipeline.limiter.set_enabled(true);

    let report = ideal_report(&pipeline);
    assert_eq!(report.cause, Some(BitPerfectCause::DynamicsActive));
    assert!(!report.limiter_bypassed);
}

#[test]
fn an_active_loudness_normaliser_names_the_dynamics_as_the_cause() {
    let mut pipeline = pure_direct_pipeline();
    pipeline.out_loudness.set_mode(LoudnessMode::EbuR128);

    let report = ideal_report(&pipeline);
    assert_eq!(report.cause, Some(BitPerfectCause::DynamicsActive));
    assert!(!report.loudness_bypassed);
}

#[test]
fn an_active_resampler_names_the_resampler_as_the_cause() {
    let pipeline = pure_direct_pipeline();
    let report = pipeline.bit_perfect_report_with_format(
        48_000,
        48_000,
        24,
        32,
        OutputSampleFormat::F32,
        true, // resampler active
        true,
    );
    assert_eq!(report.cause, Some(BitPerfectCause::SpeedOrResampleActive));
    assert!(!report.resampler_bypassed);
}

#[test]
fn a_rate_mismatch_names_the_rate_mismatch_as_the_cause() {
    let pipeline = pure_direct_pipeline();
    let report = pipeline.bit_perfect_report_with_format(
        44_100,
        48_000,
        24,
        32,
        OutputSampleFormat::F32,
        false,
        true,
    );
    assert_eq!(report.cause, Some(BitPerfectCause::SampleRateMismatch));
    assert!(!report.sample_rate_matched);
    assert!(
        !report.bit_perfect_samples,
        "a rate mismatch means the sample sequence *is* touched — the stream \
         has to be resampled to reach the output rate"
    );
    assert!(!report.is_bit_perfect);
}

#[test]
fn an_unknown_output_format_names_unknown_precision_as_the_cause() {
    let pipeline = pure_direct_pipeline();
    let report = pipeline.bit_perfect_report_with_format(
        48_000,
        48_000,
        24,
        0, // unknown output depth
        OutputSampleFormat::Unknown,
        false,
        true,
    );
    assert_eq!(report.cause, Some(BitPerfectCause::UnknownPrecision));
}

#[test]
fn a_narrower_output_container_names_bit_depth_truncation() {
    let pipeline = pure_direct_pipeline();
    let report = pipeline.bit_perfect_report_with_format(
        48_000,
        48_000,
        24,
        16, // 16-bit container truncates a 24-bit source
        OutputSampleFormat::I16,
        false,
        true,
    );
    assert_eq!(report.cause, Some(BitPerfectCause::BitDepthTruncation));
    assert!(!report.bit_depth_not_truncated);
}

#[test]
fn a_32_bit_source_names_the_lossy_format_conversion() {
    let pipeline = pure_direct_pipeline();
    let report = pipeline.bit_perfect_report_with_format(
        48_000,
        48_000,
        32, // a 32-bit integer source does not survive the f32 boundary
        32,
        OutputSampleFormat::F32,
        false,
        true,
    );
    assert_eq!(report.cause, Some(BitPerfectCause::FormatConversionLossy));
    assert!(
        report.bit_depth_not_truncated,
        "the container is wide enough; the conversion is what loses the bits"
    );
}

#[test]
fn a_shared_output_names_the_transport_as_the_cause() {
    let pipeline = pure_direct_pipeline();
    let report = pipeline.bit_perfect_report_with_format(
        48_000,
        48_000,
        24,
        32,
        OutputSampleFormat::F32,
        false,
        false, // shared, not exclusive
    );
    assert_eq!(
        report.cause,
        Some(BitPerfectCause::OutputNotDirectExclusive)
    );
    assert!(
        report.bit_perfect_samples,
        "the samples are untouched; only the transport is unverifiable"
    );
    assert!(!report.bit_perfect_transport);
    assert_eq!(
        report.result,
        BitPerfectResult::Unknown,
        "an untouched sample path on a shared transport is 'unknown', not 'bit-perfect'"
    );
}

#[test]
fn the_cause_order_is_stable_volume_before_eq_before_dynamics() {
    // The report names the *first* failure in a documented order. This test
    // pins that order, because a host renders the cause string verbatim and a
    // reordered cause would make the same configuration report two different
    // reasons on two different days.
    let mut pipeline = pure_direct_pipeline();
    pipeline.volume.gain = 0.5;
    pipeline.volume.target_gain = 0.5;
    pipeline.eq.set_enabled(true);
    pipeline.crossfeed.set_enabled(true);

    let report = ideal_report(&pipeline);
    assert_eq!(report.cause, Some(BitPerfectCause::VolumeNotUnity));

    pipeline.volume.gain = 1.0;
    pipeline.volume.target_gain = 1.0;
    let report = ideal_report(&pipeline);
    assert_eq!(report.cause, Some(BitPerfectCause::EqActive));

    pipeline.eq.set_enabled(false);
    let report = ideal_report(&pipeline);
    assert_eq!(report.cause, Some(BitPerfectCause::DynamicsActive));
}

#[test]
fn dither_and_crossfade_invalidate_the_sample_path_when_the_engine_fills_them_in() {
    // The pipeline does not own the dither or the crossfade state — the engine
    // tick fills them in and calls `finalize_with_engine_state`. Both perturb
    // the sample sequence by design, so they must be reported as causes.
    let pipeline = pure_direct_pipeline();

    let mut report = ideal_report(&pipeline);
    report.dither_active = true;
    report.finalize_with_engine_state();
    assert_eq!(report.cause, Some(BitPerfectCause::DitherActive));
    assert!(!report.bit_perfect_samples);
    assert!(!report.is_bit_perfect);

    let mut report = ideal_report(&pipeline);
    report.crossfade_active = true;
    report.finalize_with_engine_state();
    assert_eq!(report.cause, Some(BitPerfectCause::CrossfadeActive));
    assert!(!report.bit_perfect_samples);
    assert!(!report.is_bit_perfect);
}

#[test]
fn an_unverified_access_state_never_claims_a_bit_perfect_transport() {
    // The transport verdict is derived from the backend's own access report,
    // never inferred. An unverified shared stream is not a transport failure
    // either — it is "unknown", which the coarse verdict must say.
    let pipeline = pure_direct_pipeline();
    let report = pipeline.bit_perfect_report_with_access(
        48_000,
        48_000,
        24,
        32,
        OutputSampleFormat::F32,
        false,
        config::OutputAccessState {
            requested: config::OutputAccessMode::Exclusive,
            actual: config::OutputAccessMode::Shared,
            verified: false,
        },
        false,
    );
    assert!(!report.bit_perfect_transport);
    assert!(report.bit_perfect_samples);
    assert_eq!(report.result, BitPerfectResult::Unknown);
    assert_eq!(report.access_requested, config::OutputAccessMode::Exclusive);
    assert_eq!(report.access_actual, config::OutputAccessMode::Shared);
    assert!(!report.access_verified);
}

#[test]
fn a_fallback_to_a_shared_stream_is_reported_as_a_fallback() {
    let pipeline = pure_direct_pipeline();
    let report = pipeline.bit_perfect_report_with_access(
        48_000,
        48_000,
        24,
        32,
        OutputSampleFormat::F32,
        false,
        config::OutputAccessState {
            requested: config::OutputAccessMode::Exclusive,
            actual: config::OutputAccessMode::Shared,
            verified: false,
        },
        true, // the exclusive request fell back
    );
    assert!(report.fallback_occurred);
    assert!(
        !report.bit_perfect_transport,
        "a fallback can never be a bit-perfect transport"
    );
    assert!(!report.is_bit_perfect);
}

#[test]
fn every_cause_code_is_stable_and_round_trips_through_its_display_form() {
    // The cause strings are a public contract (they are serialised, sent over
    // the C ABI and shown in the UI), so their spelling is pinned here.
    let cases = [
        (BitPerfectCause::None, "none"),
        (BitPerfectCause::VolumeNotUnity, "volume_not_unity"),
        (BitPerfectCause::EqActive, "eq_active"),
        (BitPerfectCause::DynamicsActive, "dynamics_active"),
        (
            BitPerfectCause::SpeedOrResampleActive,
            "speed_or_resample_active",
        ),
        (BitPerfectCause::SampleRateMismatch, "sample_rate_mismatch"),
        (BitPerfectCause::UnknownPrecision, "unknown_precision"),
        (BitPerfectCause::BitDepthTruncation, "bit_depth_truncation"),
        (
            BitPerfectCause::FormatConversionLossy,
            "format_conversion_lossy",
        ),
        (
            BitPerfectCause::OutputNotDirectExclusive,
            "output_not_direct_exclusive",
        ),
        (BitPerfectCause::DitherActive, "dither_active"),
        (BitPerfectCause::CrossfadeActive, "crossfade_active"),
    ];
    for (cause, code) in cases {
        assert_eq!(cause.code(), code, "cause code for {cause:?}");
        assert_eq!(format!("{cause}"), code, "display form for {cause:?}");
    }
}
