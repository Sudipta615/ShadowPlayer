//! Clock-drift correction: the controller must converge on the right ratio
//! and must never leave its bounds.
//!
//! Two independent devices have two independent crystals, and they drift
//! relative to each other by tens of ppm. Left alone, the endpoint's ring
//! slowly fills or drains until it underruns or overflows. The drift
//! controller steers a per-endpoint slip resampler from the ring's fill level
//! so the two clocks stay reconciled.
//!
//! This suite simulates that loop: a device whose clock differs from the
//! master's by a known amount, a ring that fills and drains accordingly, and
//! the controller in the middle. The controller must
//!
//! * move the ratio in the direction that fixes the mismatch,
//! * converge on the ratio that exactly compensates it,
//! * stay inside the published ±500 ppm bound at all times, and
//! * notice a stalled device instead of winding the ratio up.
//!
//! The ring's fill is modelled as a real number and only *reported* to the
//! controller as whole frames, because that is what the controller sees. The
//! truncation matters: a fill that is rounded every push loses the fractional
//! drift and the controller never sees the mismatch at all.

use engine::output::drift::{DriftController, MAX_DRIFT_RATIO};

/// Ring capacity used by the simulation, in stereo frames.
const CAPACITY: usize = 8192;

/// Frames the master produces per push (a 4 ms block at 48 kHz).
const FRAMES_PER_PUSH: usize = 192;

/// The simulated outcome of running the loop.
struct Outcome {
    ratio: f64,
    fill: f64,
    controller: DriftController,
}

/// Run the drift loop against a device whose clock differs from the
/// master's by `mismatch_ppm` parts per million.
fn simulate(mismatch_ppm: f64, pushes: usize) -> Outcome {
    let mut controller = DriftController::new(true, CAPACITY);
    let mut fill = (CAPACITY / 2) as f64;

    for _ in 0..pushes {
        // The master produces `frames × ratio` frames for the endpoint; the
        // device consumes `frames × (1 + mismatch)`.
        let produced = FRAMES_PER_PUSH as f64 * controller.ratio();
        let consumed = FRAMES_PER_PUSH as f64 * (1.0 + mismatch_ppm * 1e-6);
        fill = (fill + produced - consumed).clamp(0.0, CAPACITY as f64);
        // The controller only ever sees whole frames.
        controller.update(fill.round() as usize);
    }

    Outcome {
        ratio: controller.ratio(),
        fill,
        controller,
    }
}

#[test]
fn a_disabled_controller_never_moves() {
    let mut controller = DriftController::new(false, CAPACITY);
    assert!(!controller.active());
    for _ in 0..1000 {
        controller.update(0);
    }
    assert_eq!(controller.ratio(), 1.0);
    assert_eq!(controller.offset_ppm(), 0);
    assert!(
        !controller.is_locked(),
        "a disabled controller cannot claim to be locked"
    );
}

#[test]
fn a_fast_device_makes_the_controller_speed_the_endpoint_up() {
    // The device's clock is 100 ppm fast: it consumes more than the master
    // produces, the ring drains, and the controller has to raise the ratio so
    // the endpoint is fed faster.
    let outcome = simulate(100.0, 400_000);
    assert!(
        outcome.ratio > 1.0,
        "a fast device must push the ratio above 1, it is {}",
        outcome.ratio
    );
    assert!(
        (outcome.ratio - 1.0) * 1e6 > 50.0,
        "the correction must be in the right ballpark (100 ppm), it is {} ppm",
        (outcome.ratio - 1.0) * 1e6
    );
}

#[test]
fn a_slow_device_makes_the_controller_slow_the_endpoint_down() {
    let outcome = simulate(-100.0, 400_000);
    assert!(
        outcome.ratio < 1.0,
        "a slow device must push the ratio below 1, it is {}",
        outcome.ratio
    );
    assert!(
        (outcome.ratio - 1.0) * 1e6 < -50.0,
        "the correction must be in the right ballpark (−100 ppm), it is {} ppm",
        (outcome.ratio - 1.0) * 1e6
    );
}

#[test]
fn the_controller_converges_on_the_ratio_that_compensates_the_mismatch() {
    // The exact compensation is `1 + mismatch`; the controller has to find
    // it from the ring's fill level alone. The bound is 2 ppm — two orders of
    // magnitude tighter than the mismatch being corrected, and far tighter
    // than the controller's own slew rate per push.
    for mismatch in [50.0f64, 100.0, 300.0, -100.0, -300.0] {
        let outcome = simulate(mismatch, 400_000);
        let expected = 1.0 + mismatch * 1e-6;
        let error_ppm = (outcome.ratio - expected) * 1e6;
        assert!(
            error_ppm.abs() < 2.0,
            "the {mismatch} ppm case settled at {:.3} ppm, the exact \
             compensation is {expected:.6} ({error_ppm:.3} ppm away)",
            (outcome.ratio - 1.0) * 1e6
        );
    }
}

#[test]
fn the_ratio_never_leaves_its_published_bound() {
    // A broken device, a stuck ring or a mis-scaled error must not be able to
    // drive the rate wildly off: the correction is bounded to ±500 ppm.
    for mismatch in [1_000.0f64, 10_000.0, 100_000.0, -10_000.0] {
        let outcome = simulate(mismatch, 100_000);
        assert!(
            (outcome.ratio - 1.0).abs() <= MAX_DRIFT_RATIO + 1e-12,
            "the {mismatch} ppm case drove the ratio to {}, outside the \
             ±{MAX_DRIFT_RATIO} bound",
            outcome.ratio
        );
    }
}

#[test]
fn the_ratio_never_moves_faster_than_the_slew_limit() {
    // A sudden jump in the ratio is audible as pitch modulation, so the
    // change per update is rate-limited.
    let mut controller = DriftController::new(true, CAPACITY);
    let mut previous = controller.ratio();
    // Drive the ring hard against the controller: full, then empty.
    for fill in [CAPACITY, 0, CAPACITY, 0, CAPACITY, 0] {
        controller.update(fill);
        let step = (controller.ratio() - previous).abs();
        assert!(
            step <= 2.0e-6 + 1e-12,
            "the ratio moved by {step} in one update, more than the 2 ppm slew limit"
        );
        previous = controller.ratio();
    }
}

#[test]
fn the_controller_reports_its_lock_state() {
    // A locked controller is one whose fill error has settled inside ±2 % of
    // the ring's capacity. With the proportional controller the steady-state
    // error is `mismatch / p_gain` frames, which for 100 ppm is about 67
    // frames — comfortably inside the ±164-frame tolerance.
    let outcome = simulate(100.0, 400_000);
    assert!(
        outcome.controller.is_locked(),
        "a converged controller must report itself locked (fill {:.1})",
        outcome.fill
    );

    // And a mismatch too large for the proportional term to hide inside the
    // tolerance must NOT claim to be locked.
    let large = simulate(300.0, 400_000);
    assert!(
        !large.controller.is_locked(),
        "a controller sitting {} frames from its setpoint must not claim to \
         be locked",
        (CAPACITY / 2) as f64 - large.fill
    );
}

#[test]
fn a_stalled_device_is_detected_and_the_ratio_relaxes_back() {
    // A disconnected endpoint stops consuming: the fill never changes, which
    // the loss-of-clock detector recognises. The ratio must then relax back
    // to nominal rather than winding up against a device that is not there.
    //
    // The stall detector is armed in the *adaptive* controller only — the
    // plain proportional controller has no stall threshold, so it would keep
    // integrating against a device that is not there.
    let mut controller = DriftController::new_adaptive(true, CAPACITY);
    let mut fill = CAPACITY / 2;
    // Drive it hard so the ratio is far from nominal. The fill changes every
    // other push here, so the stall detector stays quiet and the ring never
    // saturates.
    for i in 0..5000 {
        if i % 2 == 0 {
            fill = (fill + 1).min(CAPACITY);
        }
        controller.update(fill);
    }
    let disturbed = controller.ratio();
    assert!(
        disturbed < 1.0,
        "an over-full ring must pull the ratio below 1, it is {disturbed}"
    );
    assert!(
        !controller.is_loss_of_clock(),
        "a moving ring is not a lost clock"
    );

    // Now the ring stops changing entirely: a stalled device. The detector
    // needs 300 identical readings before it declares the loss, and the
    // relaxation then slews the ratio back at 2 ppm per push.
    let stalled_fill = fill;
    for _ in 0..2000 {
        controller.update(stalled_fill);
    }
    assert!(
        controller.is_loss_of_clock(),
        "a ring that never changes must be reported as a lost clock"
    );
    assert!(
        (controller.ratio() - 1.0).abs() < 1e-6,
        "a lost clock must relax the ratio back to nominal, it is {}",
        controller.ratio()
    );
}

#[test]
fn the_plain_controller_has_no_stall_detector() {
    // The two controllers differ in more than their gains: the plain
    // proportional controller has no stall threshold at all, so a stalled
    // device is not something it can report.
    let mut controller = DriftController::new(true, CAPACITY);
    for _ in 0..2000 {
        controller.update(1234);
    }
    assert!(
        !controller.is_loss_of_clock(),
        "the plain controller must not claim a stall it cannot detect"
    );
}

#[test]
fn the_reported_offset_agrees_with_the_ratio() {
    let outcome = simulate(100.0, 50_000);
    let expected_ppm = ((outcome.ratio - 1.0) * 1e6).round() as i64;
    assert_eq!(
        outcome.controller.offset_ppm(),
        expected_ppm,
        "the ppm figure must be the ratio's own offset"
    );
    assert!(
        (outcome.controller.smoothed_ppm() - (outcome.ratio - 1.0) * 1e6).abs() < 1e-6,
        "the high-resolution ppm figure must agree with the ratio"
    );
}

#[test]
fn resetting_the_controller_returns_it_to_nominal() {
    let mut controller = DriftController::new(true, CAPACITY);
    let mut fill = CAPACITY / 2;
    for _ in 0..5000 {
        fill = (fill + 1).min(CAPACITY);
        controller.update(fill);
    }
    assert!(
        (controller.ratio() - 1.0).abs() > 1e-9,
        "the controller must have moved"
    );
    controller.reset();
    assert_eq!(controller.ratio(), 1.0);
    assert_eq!(controller.offset_ppm(), 0);
    assert!(!controller.is_loss_of_clock());
}
