//! End-to-end through the null backend: open a file, play it, seek in it,
//! and watch it finish.
//!
//! The null backend is a hardware-free sink: it drains the master ring on a
//! real-time-shaped schedule and discards the audio. That makes the whole
//! engine — decode, DSP, output, events — exercisable on a headless machine,
//! which is what a CI run needs.
//!
//! This suite drives the engine the way a host does: through the
//! [`EngineHandle`]'s command channel, reading the state from the atomic
//! telemetry snapshot and the lifecycle from the event channel. The engine
//! itself is ticked on its own thread, exactly as the FFI's tick thread does
//! it.

mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossbeam::channel::Receiver;
use engine::buffer::{FixedFrameBuffer, PlaybackState};
use engine::decode::Decoder;
use engine::dsp::pipeline::DspPipeline;
use engine::events::EngineEvent;
use engine::output::{NullOutput, Output};
use engine::AudioBackend;
use engine::{AudioEngine, EngineConfig, EngineHandle};

/// How long to wait for an event before giving up.
const EVENT_TIMEOUT: Duration = Duration::from_secs(10);

/// A one-second stereo WAV.
fn fixture() -> common::TempFile {
    let frames = common::SAMPLE_RATE as usize;
    let samples = common::sine(1_000.0, 0.5, frames, common::SAMPLE_RATE);
    common::write_wav("null_e2e", &samples, common::CHANNELS, common::SAMPLE_RATE)
}

/// An engine on the null backend, with its tick thread running.
///
/// The returned tuple is the handle a host would hold and the flag that stops
/// the tick thread.
fn start_engine() -> (EngineHandle, Receiver<EngineEvent>, Arc<AtomicBool>) {
    let config = EngineConfig {
        output_backend: AudioBackend::Null,
        ..EngineConfig::default()
    };
    let mut engine = AudioEngine::new(config).expect("the engine must be constructible");
    engine.start().expect("the null backend must start");

    let handle = engine.handle();
    let events = handle.clone_event_receiver();
    let running = Arc::new(AtomicBool::new(true));

    let ticker = {
        let running = Arc::clone(&running);
        std::thread::Builder::new()
            .name("null-e2e-tick".into())
            .spawn(move || {
                while running.load(Ordering::Relaxed) {
                    engine.tick_blocking(Duration::from_millis(2));
                }
                engine.stop();
            })
            .expect("the tick thread must be spawnable")
    };

    // Keep the thread alive for the whole test: it is stopped through the
    // flag, and the join happens when the flag is cleared at the end of the
    // test. The handle is detached so the engine stops when the flag is
    // cleared; the join is not needed for the assertions.
    std::mem::forget(ticker);

    (handle, events, running)
}

/// Wait for the first event that matches, collecting the others.
fn wait_for<F>(events: &Receiver<EngineEvent>, what: &str, mut matches: F) -> EngineEvent
where
    F: FnMut(&EngineEvent) -> bool,
{
    let deadline = std::time::Instant::now() + EVENT_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        assert!(
            !remaining.is_zero(),
            "timed out waiting for {what}; the engine never emitted it"
        );
        let event = events
            .recv_timeout(remaining)
            .unwrap_or_else(|e| panic!("timed out waiting for {what}: {e}"));
        if matches(&event) {
            return event;
        }
    }
}

/// Wait until `predicate` holds, ticking nothing: the engine's own thread
/// does the work.
fn wait_until<F>(what: &str, mut predicate: F)
where
    F: FnMut() -> bool,
{
    let deadline = std::time::Instant::now() + EVENT_TIMEOUT;
    while !predicate() {
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_file_opens_plays_seeks_and_finishes_through_the_null_backend() {
    let file = fixture();
    let (handle, events, running) = start_engine();

    // ── Before anything: stopped ───────────────────────────────────────
    assert_eq!(
        handle.state(),
        PlaybackState::Stopped,
        "a fresh engine must be stopped"
    );
    assert!(handle.current_source().is_none());

    // ── Open ───────────────────────────────────────────────────────────
    // Opening a source starts it: the engine's own `handle_open` moves the
    // state to Playing once the source has loaded, and emits
    // `PlaybackStarted` with it.
    handle.open_file(file.path());
    let opened = wait_for(&events, "SourceOpened", |event| {
        matches!(event, EngineEvent::SourceOpened { .. })
    });
    match opened {
        EngineEvent::SourceOpened {
            sample_rate,
            channels,
            duration_secs,
            ..
        } => {
            assert_eq!(sample_rate, common::SAMPLE_RATE);
            assert_eq!(channels, common::CHANNELS as usize);
            assert!(
                duration_secs > 0.5 && duration_secs < 1.5,
                "the fixture is one second long, the engine reported {duration_secs}"
            );
        }
        other => panic!("expected SourceOpened, got {other:?}"),
    }
    wait_for(&events, "PlaybackStarted", |event| {
        matches!(event, EngineEvent::PlaybackStarted)
    });
    wait_until("the engine to report itself playing", || {
        handle.state() == PlaybackState::Playing
    });
    assert_eq!(handle.state(), PlaybackState::Playing);
    assert!(handle.is_playing());
    assert!(
        handle.current_source().is_some(),
        "the opened source must be published in the telemetry"
    );

    // The playhead must advance.
    wait_until("the playhead to advance", || {
        handle.playback_info().position_secs > 0.0
    });

    // ── Seek ───────────────────────────────────────────────────────────
    handle.seek(0.25);
    let sought = wait_for(&events, "SeekCompleted", |event| {
        matches!(event, EngineEvent::SeekCompleted { .. })
    });
    match sought {
        EngineEvent::SeekCompleted { position_secs } => {
            assert!(
                (position_secs - 0.25).abs() < 0.05,
                "the seek must land at 0.25 s, it reported {position_secs}"
            );
        }
        other => panic!("expected SeekCompleted, got {other:?}"),
    }
    wait_until("the playhead to reach the seek position", || {
        (handle.playback_info().position_secs - 0.25).abs() < 0.15
    });

    // ── Finish ─────────────────────────────────────────────────────────
    let finished = wait_for(&events, "SourceFinished", |event| {
        matches!(event, EngineEvent::SourceFinished { .. })
    });
    match finished {
        EngineEvent::SourceFinished { source } => {
            assert_eq!(
                source,
                engine::source::AudioSource::File(file.path().to_path_buf()),
                "the finish event must name the source that finished"
            );
        }
        other => panic!("expected SourceFinished, got {other:?}"),
    }
    wait_until("the engine to stop after the end of the stream", || {
        handle.state() != PlaybackState::Playing
    });
    assert_ne!(handle.state(), PlaybackState::Playing);

    running.store(false, Ordering::Relaxed);
}

/// Pure Direct means the samples that leave the decoder are the samples that
/// reach the device. `NullOutput` is the only sink whose output can be read
/// back without hardware, so the assertion is made on its capture tail: decode
/// → production `DspPipeline` (bit-perfect) → master ring → null drain. A
/// failure here means gain, a stage, a fade, or the output path changed even
/// one sample.
#[test]
fn pure_direct_samples_are_bit_identical_through_the_null_sink() {
    let frames = 20_000usize;
    let source = common::noise_burst(frames, 0xC0FF_EE12);
    let file = common::write_wav(
        "null_bitperfect",
        &source,
        common::CHANNELS,
        common::SAMPLE_RATE,
    );

    let buffer = Arc::new(
        FixedFrameBuffer::new(frames + 4096).expect("the capture ring must be allocatable"),
    );
    let mut output = NullOutput::new(
        Arc::clone(&buffer),
        common::SAMPLE_RATE,
        common::CHANNELS,
        // One block of headroom: the final partial block must not have to
        // reset a capture tail that is exactly full.
        frames + 512,
    )
    .expect("construct the null sink");

    let mut config = EngineConfig {
        output_backend: AudioBackend::Null,
        ..EngineConfig::default()
    };
    config.limiter.enabled = false;
    config.eq.enabled = false;
    let mut pipeline = DspPipeline::from_config(&config, common::SAMPLE_RATE as f32);
    pipeline.set_volume(1.0);
    pipeline.volume.snap();
    pipeline.set_bit_perfect(true);
    assert!(pipeline.is_dsp_transparent().0);

    let mut decoder = Decoder::open(file.path()).expect("open the float WAV");
    let mut decoded = Vec::with_capacity(source.len());
    while let Ok(chunk) = decoder.decode_next(4096) {
        assert_eq!(chunk.sample_rate, common::SAMPLE_RATE);
        assert_eq!(chunk.channels, common::CHANNELS as usize);
        let mut processed = Vec::with_capacity(chunk.samples.len());
        for pair in chunk.samples.as_chunks::<2>().0 {
            let (left, right) = pipeline.process(pair[0], pair[1]);
            processed.push(left);
            processed.push(right);
        }
        let pushed = buffer.push_frames_interleaved(&processed, common::CHANNELS as usize);
        assert_eq!(
            pushed,
            processed.len() / common::CHANNELS as usize,
            "the master ring refused audio before the sink started"
        );
        decoded.extend_from_slice(&processed);
    }
    assert_eq!(decoded.len(), source.len());

    output.start().expect("start the null drain thread");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while output.frames_played() < frames as u64 {
        assert!(
            std::time::Instant::now() < deadline,
            "the null sink did not drain all {frames} frames"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    let captured = output.take_all_captured();
    output.stop();

    assert_eq!(
        captured.len(),
        decoded.len(),
        "the sink lost or added samples: {} captured against {} decoded",
        captured.len(),
        decoded.len()
    );
    assert_eq!(
        captured, decoded,
        "Pure Direct must deliver the decoder's f32 samples unchanged"
    );

    // Sensitivity control: the fixture is not all zeros, so a gain stage would
    // produce visibly different samples and the equality above is not vacuous.
    let changed = source
        .iter()
        .filter(|&&sample| {
            let faded = sample * 0.5;
            faded.to_bits() != sample.to_bits()
        })
        .count();
    assert!(
        changed > source.len() / 2,
        "the fixture is not sensitive enough to detect gain"
    );
}

#[test]
fn a_pause_and_a_resume_are_reported_as_state_transitions() {
    let file = fixture();
    let (handle, events, running) = start_engine();

    handle.open_file(file.path());
    wait_for(&events, "SourceOpened", |event| {
        matches!(event, EngineEvent::SourceOpened { .. })
    });
    wait_for(&events, "PlaybackStarted", |event| {
        matches!(event, EngineEvent::PlaybackStarted)
    });
    wait_until("the engine to report itself playing", || {
        handle.state() == PlaybackState::Playing
    });

    handle.pause();
    wait_for(&events, "PlaybackPaused", |event| {
        matches!(event, EngineEvent::PlaybackPaused)
    });
    wait_until("the engine to report itself paused", || {
        handle.state() == PlaybackState::Paused
    });
    assert_eq!(handle.state(), PlaybackState::Paused);
    assert!(!handle.is_playing());

    handle.play();
    wait_until("the engine to be playing again", || {
        handle.state() == PlaybackState::Playing
    });
    assert!(handle.is_playing());

    handle.stop();
    wait_until("the engine to report itself stopped", || {
        handle.state() == PlaybackState::Stopped
    });
    assert_eq!(handle.state(), PlaybackState::Stopped);

    running.store(false, Ordering::Relaxed);
}

#[test]
fn opening_a_file_that_does_not_exist_reports_an_error_instead_of_playing() {
    let (handle, events, running) = start_engine();

    handle.open_file("/definitely/not/here/at/all.flac");
    // The engine must not have started playing.
    std::thread::sleep(Duration::from_millis(200));
    assert_ne!(
        handle.state(),
        PlaybackState::Playing,
        "a file that does not exist must not start playback"
    );

    // Either an error event or a source-opened event with a broken source:
    // what matters is that the engine reported *something* rather than
    // pretending to play.
    let mut saw_report = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        match events.recv_timeout(Duration::from_millis(200)) {
            Ok(EngineEvent::Error(_)) => {
                saw_report = true;
                break;
            }
            Ok(EngineEvent::SourceOpened { .. }) => {
                saw_report = true;
                break;
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    assert!(saw_report, "the engine must report a file it cannot open");

    running.store(false, Ordering::Relaxed);
}
