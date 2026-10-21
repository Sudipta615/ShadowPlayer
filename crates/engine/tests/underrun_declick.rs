//! Underrun concealment: a starved output must produce silence plus a
//! counter, never a click and never a panic.
//!
//! An output underrun used to be papered over by filling the shortfall with
//! zeros. That is a full-scale step discontinuity — a click — once per
//! underrun, and an underrun is exactly when the listener least wants one.
//! The engine replaces the step with a bounded ramp and counts the event.
//!
//! This suite checks both halves: the *signal* (a ramp to silence, not a
//! step) and the *report* (a counter the host can read).

use std::sync::Arc;
use std::time::Duration;

use engine::buffer::{declick_ramp_frames, FixedFrameBuffer, UnderrunState};
use engine::output::{NullOutput, Output};

/// Sample rate for the measurements.
const SR: u32 = 48_000;

/// The largest inter-sample step in `lane`, and the index of its last
/// non-zero sample.
fn step_profile(lane: &[f32]) -> (f32, Option<usize>) {
    let mut max_step = 0.0f32;
    for w in lane.windows(2) {
        max_step = max_step.max((w[1] - w[0]).abs());
    }
    (max_step, lane.iter().rposition(|&v| v != 0.0))
}

/// The left channel of an interleaved block.
fn left(block: &[f32]) -> Vec<f32> {
    block.iter().step_by(2).copied().collect()
}

#[test]
fn a_total_starvation_ramps_to_silence_instead_of_stepping() {
    let ramp = declick_ramp_frames(SR);
    let mut state = UnderrunState::new(SR);

    // Steady state: full-scale DC, so the last emitted sample is 1.0.
    let mut warm = vec![1.0f32; 128];
    state.declick(&mut warm, 2, 64);
    assert!(state.is_live(), "the steady state must be a pass-through");

    // Total starvation. The caller must not have zeroed anything first.
    let mut block = vec![1.0f32; 128];
    state.declick(&mut block, 2, 0);

    // The boundary step only exists where the two blocks are concatenated,
    // which is exactly where a click would be heard.
    let mut signal = left(&warm);
    let warm_len = signal.len();
    signal.extend_from_slice(&left(&block));

    let (max_step, last_nonzero) = step_profile(&signal);
    let last_nonzero = last_nonzero.expect("the ramp must produce some output");
    assert!(
        last_nonzero - warm_len < ramp,
        "the signal must reach exact zero within the ramp ({ramp}); it took {} frames",
        last_nonzero - warm_len
    );
    assert!(
        max_step <= 1.0 / ramp as f32 * 1.05,
        "the inter-sample step {max_step} exceeds |last| / ramp = {}",
        1.0 / ramp as f32 * 1.05
    );
    assert!(
        signal[last_nonzero + 1..].iter().all(|&v| v == 0.0),
        "the tail of the ramp must be exact zeros"
    );
    assert!(
        !state.is_live(),
        "a completed fade-out leaves the state silent"
    );
}

#[test]
fn a_sustained_starvation_stays_at_exact_zero() {
    let mut state = UnderrunState::new(SR);
    let mut block = vec![1.0f32; 1024];
    state.declick(&mut block, 2, 0);
    let mut block2 = vec![7.0f32; 1024];
    state.declick(&mut block2, 2, 0);
    assert!(
        block2.iter().all(|&v| v == 0.0),
        "a sustained starvation must emit true zeros, not a decaying ramp"
    );
}

#[test]
fn the_recovery_ramps_up_from_silence_instead_of_stepping() {
    let ramp = declick_ramp_frames(SR);
    let mut state = UnderrunState::new(SR);

    // Starve long enough to latch to silence.
    let mut gap = vec![0.0f32; 512];
    state.declick(&mut gap, 2, 0);
    assert!(!state.is_live());

    // The audio comes back.
    let mut block = vec![1.0f32; 512];
    state.declick(&mut block, 2, 256);
    let mut signal = left(&gap);
    signal.extend_from_slice(&left(&block));

    assert!(
        signal[gap.len() / 2].abs() < 1e-9,
        "the first recovered sample must start from zero, got {}",
        signal[gap.len() / 2]
    );
    let (max_step, _) = step_profile(&signal);
    assert!(
        max_step <= 1.0 / ramp as f32 * 1.05,
        "the inter-sample step {max_step} exceeds |last| / ramp"
    );
    assert!(
        (signal[gap.len() / 2 + ramp] - 1.0).abs() < 1e-3,
        "unity gain one ramp after the recovery"
    );
    assert!(state.is_live(), "the ramp must complete");
}

#[test]
fn the_steady_state_is_bit_exact() {
    let mut state = UnderrunState::new(SR);
    let original: Vec<f32> = (0..256).map(|i| (i as f32 * 0.01).sin() * 0.9).collect();
    let mut block = original.clone();
    state.declick(&mut block, 2, 128);
    assert_eq!(block, original, "a live signal must pass through untouched");
}

#[test]
fn a_starved_output_produces_silence_and_a_counter() {
    // The null backend drains the ring on a real-time-shaped schedule. With
    // nothing in the ring it is starved: it must count the event and emit
    // silence, not a click and not a panic.
    let buffer = Arc::new(FixedFrameBuffer::new(8192).expect("a ring buffer must be creatable"));
    let mut output =
        NullOutput::new(Arc::clone(&buffer), SR, 2, 0).expect("the null sink must open");
    output.start().expect("the null sink must start");
    assert!(output.is_running());

    // Let the drain loop run a few periods with an empty ring.
    std::thread::sleep(Duration::from_millis(120));

    let underruns = output.take_underruns();
    assert!(
        underruns > 0,
        "a starved output must count the starvation events"
    );
    assert_eq!(
        output.frames_played(),
        0,
        "a starved output must not have played anything"
    );
    let captured = output.take_all_captured();
    assert!(
        captured.is_empty(),
        "a starved output must produce silence, it captured {} samples",
        captured.len()
    );

    // The counter is a *reading*, so reading it resets it.
    assert_eq!(
        output.take_underruns(),
        0,
        "the underrun counter must reset when it is read"
    );

    output.stop();
}

#[test]
fn a_fed_output_produces_audio_and_stops_counting_starvation() {
    // The other half of the contract: once the ring has audio in it, the
    // sink plays it and the starvation counter stays quiet.
    let buffer = Arc::new(FixedFrameBuffer::new(8192).expect("a ring buffer must be creatable"));
    let mut output =
        NullOutput::new(Arc::clone(&buffer), SR, 2, 0).expect("the null sink must open");
    output.start().expect("the null sink must start");

    // Let it starve once, then clear the counter.
    std::thread::sleep(Duration::from_millis(60));
    let _ = output.take_underruns();

    // Feed it a block of audio.
    let block: Vec<f32> = (0..2048)
        .map(|i| if i % 2 == 0 { 0.5 } else { -0.5 })
        .collect();
    let written = buffer.push_frames_interleaved(&block, 2);
    assert_eq!(written, 1024, "the ring must accept the block");

    std::thread::sleep(Duration::from_millis(120));
    let captured = output.take_all_captured();
    assert!(!captured.is_empty(), "a fed output must produce audio");
    assert!(
        captured.iter().all(|v| v.is_finite()),
        "the captured audio must be finite"
    );
    assert!(
        captured.iter().any(|&v| v.abs() > 0.1),
        "the captured audio must not be silence"
    );

    output.stop();
}

#[test]
fn the_declick_ramp_length_is_derived_from_the_sample_rate() {
    // The ramp length is derived, not chosen: it must scale with the rate
    // and be clamped into a sane band.
    assert_eq!(declick_ramp_frames(48_000), 48);
    assert_eq!(declick_ramp_frames(44_100), 44);
    assert_eq!(declick_ramp_frames(96_000), 96);
    assert!(
        declick_ramp_frames(8_000) >= 16,
        "a low rate hits the floor"
    );
    assert!(
        declick_ramp_frames(768_000) <= 512,
        "an absurd rate hits the ceiling"
    );
}
