//! The frozen oracle: a fully bypassed signal path must be **sample
//! identical** to its input.
//!
//! This is the platform-independent core of the bit-perfect claim. The
//! transport-level claim ("the DAC received the exact bits") depends on the
//! operating system's mixer, the driver and the hardware; the *sample-level*
//! claim does not: if every stage in the engine's chain is bypassed, the
//! samples the engine hands to the output must be the very same samples it
//! decoded, bit for bit, on every platform and every CPU.
//!
//! The engine ships the classifier for exactly this comparison:
//! [`compare_buffers`] with [`DeterministicMode::StrictBitExact`]. This suite
//! drives it with a signal that is awkward on purpose (a sine, a noise
//! burst, an impulse and a ramp in one buffer) so that a stage which quietly
//! touched a single sample cannot hide in the rounding.

mod common;

use engine::dsp::deterministic::{compare_buffers, DeterministicMode, EquivalenceClass};
use engine::dsp::pipeline::DspPipeline;

/// The known signal: four different kinds of content in one stereo buffer.
///
/// * a 1 kHz sine (musical content),
/// * a deterministic noise burst (broadband),
/// * an impulse (transient), and
/// * a ramp (a non-stationary, exactly-representable signal).
///
/// A bypassed chain has to reproduce every one of them exactly; a chain that
/// applies even a unity-gain multiply with a different rounding would show up
/// here and nowhere else.
fn known_signal(frames: usize) -> Vec<f32> {
    let sine = common::sine(1_000.0, 0.5, frames, common::SAMPLE_RATE);
    let noise = common::noise_burst(frames, 0x5eed_1234_9abc_def0);
    let impulse = common::impulse(frames);
    let ramp = common::ramp(frames, 0.25);

    let mut out = Vec::with_capacity(sine.len());
    for i in 0..sine.len() {
        // Every term is exactly representable in f32 (a sum of dyadic
        // rationals and a noise sample), and the sum is scaled so no term
        // can overflow the [-1, 1] range.
        out.push(0.5 * (sine[i] + 0.25 * noise[i] + impulse[i] + ramp[i]));
    }
    out
}

/// A chain with every user stage bypassed.
fn bypassed_pipeline() -> DspPipeline {
    DspPipeline::builder(common::SAMPLE_RATE as f32)
        .with_eq(false)
        .with_limiter_disabled()
        .with_crossfeed(false, config::CrossfeedProfile::Bauer)
        .with_stereo_width(1.0)
        .build()
}

/// The same chain, built in the f64 ("Quality") precision mode.
fn bypassed_pipeline_quality() -> DspPipeline {
    DspPipeline::builder(common::SAMPLE_RATE as f32)
        .with_eq(false)
        .with_limiter_disabled()
        .with_crossfeed(false, config::CrossfeedProfile::Bauer)
        .with_stereo_width(1.0)
        .with_precision(config::PrecisionMode::Quality)
        .build()
}

#[test]
fn the_bypassed_block_path_is_sample_identical_to_its_input() {
    let frames = 4096;
    let input = known_signal(frames);
    let mut pipeline = bypassed_pipeline();

    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);
    for f in 0..frames {
        left.push(input[f * 2]);
        right.push(input[f * 2 + 1]);
    }
    let reference = left.clone();
    let reference_right = right.clone();

    pipeline.process_block(&mut left, &mut right);

    assert_eq!(
        left.len(),
        reference.len(),
        "the bypassed path must not change the frame count"
    );
    assert_eq!(
        right.len(),
        reference_right.len(),
        "the bypassed path must not change the frame count"
    );

    // The engine's own classifier, at the strictest setting.
    let eq = compare_buffers(&reference, &left, 0.0, 0.0);
    assert_eq!(
        eq,
        EquivalenceClass::BitExact,
        "the bypassed left channel must be bit-exact"
    );
    assert!(eq.satisfies(DeterministicMode::StrictBitExact));

    let eq = compare_buffers(&reference_right, &right, 0.0, 0.0);
    assert_eq!(
        eq,
        EquivalenceClass::BitExact,
        "the bypassed right channel must be bit-exact"
    );

    // And the raw bit patterns, so a failure says *which* sample moved.
    for (i, (a, b)) in reference.iter().zip(left.iter()).enumerate() {
        assert_eq!(
            a.to_bits(),
            b.to_bits(),
            "left sample {i} changed: {a} -> {b}"
        );
    }
}

#[test]
fn the_bypassed_per_frame_path_is_sample_identical_too() {
    let frames = 2048;
    let input = known_signal(frames);
    let mut pipeline = bypassed_pipeline();

    let mut out = Vec::with_capacity(frames * 2);
    for f in 0..frames {
        let (l, r) = pipeline.process(input[f * 2], input[f * 2 + 1]);
        out.push(l);
        out.push(r);
    }

    assert_eq!(
        compare_buffers(&input, &out, 0.0, 0.0),
        EquivalenceClass::BitExact,
        "the per-frame bypassed path must be bit-exact"
    );
}

#[test]
fn the_bypassed_quality_precision_path_is_sample_identical_too() {
    // Quality mode promotes the block to f64 and back. A promotion round trip
    // is exact for every f32 value, so the bypassed path must still be
    // bit-exact — a widened precision mode that quietly changed a sample
    // would be a bit-perfectness bug in the mode most audiophiles use.
    let frames = 2048;
    let input = known_signal(frames);
    let mut pipeline = bypassed_pipeline_quality();

    let mut left: Vec<f32> = (0..frames).map(|f| input[f * 2]).collect();
    let mut right: Vec<f32> = (0..frames).map(|f| input[f * 2 + 1]).collect();
    let reference = left.clone();

    pipeline.process_block(&mut left, &mut right);

    assert_eq!(
        compare_buffers(&reference, &left, 0.0, 0.0),
        EquivalenceClass::BitExact,
        "the f64 quality path must be bit-exact when every stage is bypassed"
    );
}

#[test]
fn the_oracle_can_actually_see_a_stage_that_touches_the_samples() {
    // A test that only ever compares identical buffers proves nothing: the
    // classifier could be broken and still pass. This asserts the opposite
    // direction — an enabled EQ is detected as a change.
    let frames = 4096;
    let input = known_signal(frames);
    let mut pipeline = bypassed_pipeline();
    pipeline.eq.set_enabled(true);
    // The default EQ bands are flat, so enabling the EQ on its own is a
    // no-op; a band with a real gain is what actually alters the samples.
    pipeline.eq.set_band(
        0,
        engine::dsp::EqBandParams {
            enabled: true,
            filter_type: engine::dsp::EqFilterType::Peaking,
            frequency: 1_000.0,
            gain_db: 6.0,
            q: 1.0,
        },
    );

    let mut left: Vec<f32> = (0..frames).map(|f| input[f * 2]).collect();
    let mut right: Vec<f32> = (0..frames).map(|f| input[f * 2 + 1]).collect();
    let reference = left.clone();

    pipeline.process_block(&mut left, &mut right);

    let eq = compare_buffers(&reference, &left, 0.0, 0.0);
    assert!(
        !eq.is_bit_exact(),
        "an enabled EQ must be visible to the oracle, got {eq:?}"
    );
}

#[test]
fn a_float32_wav_round_trips_through_the_decoder_without_changing_a_sample() {
    // The oracle's input is the decoder's output: if the container path
    // changed a sample, the "bit-perfect" claim would be false before the
    // DSP chain even started. A float32 WAV is the engine's own lossless
    // fixture format, so this is exact.
    let frames = 8192;
    let samples = known_signal(frames);
    let file = common::write_wav(
        "bitperfect",
        &samples,
        common::CHANNELS,
        common::SAMPLE_RATE,
    );

    let mut decoder =
        engine::decode::Decoder::open(file.path()).expect("the engine's own float32 WAV must open");
    assert_eq!(decoder.info().sample_rate, common::SAMPLE_RATE);
    assert_eq!(decoder.info().channels, common::CHANNELS as usize);

    let mut decoded = Vec::new();
    loop {
        match decoder.decode_next(4096) {
            Ok(chunk) => decoded.extend_from_slice(&chunk.samples),
            Err(engine::decode::DecodeError::EndOfStream) => break,
            Err(other) => panic!("decoding the fixture failed: {other}"),
        }
    }

    assert_eq!(
        decoded.len(),
        samples.len(),
        "the decoded fixture must have the same frame count"
    );
    assert_eq!(
        compare_buffers(&samples, &decoded, 0.0, 0.0),
        EquivalenceClass::BitExact,
        "a float32 WAV must decode to the very same samples"
    );
}

#[test]
fn a_bypassed_chain_leaves_the_decoded_samples_untouched_end_to_end() {
    // The whole oracle in one test: decode → bypassed chain → compare with
    // the samples that were written to the file.
    let frames = 8192;
    let samples = known_signal(frames);
    let file = common::write_wav(
        "bitperfect_e2e",
        &samples,
        common::CHANNELS,
        common::SAMPLE_RATE,
    );

    let mut decoder =
        engine::decode::Decoder::open(file.path()).expect("the engine's own float32 WAV must open");
    let mut pipeline = bypassed_pipeline();

    let mut decoded = Vec::new();
    loop {
        match decoder.decode_next(1024) {
            Ok(chunk) => {
                let ch = chunk.channels.max(1);
                // Derive the frame count from the payload rather than from
                // `frame_count`: the decoder reports the logical frame count,
                // and a chunk that ends the stream may carry fewer samples
                // than that.
                let frames: Vec<&[f32]> = chunk.samples.chunks_exact(ch).collect();
                let n = frames.len();
                let mut left: Vec<f32> = frames.iter().map(|f| f[0]).collect();
                let mut right: Vec<f32> = frames.iter().map(|f| f[1.min(ch - 1)]).collect();
                pipeline.process_block(&mut left, &mut right);
                for i in 0..n {
                    decoded.push(left[i]);
                    decoded.push(right[i]);
                }
            }
            Err(engine::decode::DecodeError::EndOfStream) => break,
            Err(other) => panic!("decoding the fixture failed: {other}"),
        }
    }

    assert_eq!(
        compare_buffers(&samples, &decoded, 0.0, 0.0),
        EquivalenceClass::BitExact,
        "decode → bypassed chain must reproduce the source samples exactly"
    );
}
