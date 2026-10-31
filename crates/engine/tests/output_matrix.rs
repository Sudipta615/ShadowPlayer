//! Multi-endpoint fan-out.
//!
//! The engine can play to several devices at once. Each endpoint owns its
//! own ring, its own configuration and its own counters — no state is shared
//! between them, so one endpoint's backlog can never stall another's.
//!
//! This suite uses the null backend (no OS device) to check two things:
//!
//! * **Fan-out** — N endpoints each receive the frames that were pushed, and
//!   each ring is genuinely its own.
//! * **Per-endpoint trim** — each endpoint applies *its own* gain to the
//!   frames it receives, so two endpoints with different trims receive
//!   differently scaled copies of the same master block.
//!
//! The endpoint routing model has a per-endpoint **gain** (the trim) and no
//! separate pan axis: the pan belongs to the source (the mixer's lanes), not
//! to the output device. This suite therefore asserts the trim, which is the
//! per-endpoint adjustment the endpoint layer actually owns.

use engine::output::endpoint::{EndpointConfig, EndpointId, EndpointRegistry, EndpointWorker};
use engine::output::{NullOutput, Output, OutputVolume};
use engine::AudioBackend;

/// Sample rate both the master and the null endpoint run at.
const SR: u32 = 48_000;

/// Number of endpoints in the fan-out tests.
const ENDPOINTS: usize = 3;

/// A constant-amplitude stereo block.
///
/// A constant signal is what makes the trim assertion immune to the null
/// backend's drain thread: whatever the thread takes is the same amplitude as
/// whatever it leaves, so the per-sample ratio stays exact no matter how the
/// two interleave.
fn constant_block(frames: usize, amplitude: f32) -> Vec<f32> {
    vec![amplitude; frames * 2]
}

#[test]
fn every_endpoint_of_the_fan_out_receives_the_frames() {
    let mut registry = EndpointRegistry::new();
    let mut rings = Vec::new();
    for i in 0..ENDPOINTS {
        let config = EndpointConfig::new(format!("endpoint-{i}"), None);
        let ring = registry
            .add(config, 1024)
            .expect("an endpoint ring must be creatable");
        rings.push(ring);
    }
    assert_eq!(registry.len(), ENDPOINTS);

    let block = constant_block(512, 0.5);
    for ring in &rings {
        let written = ring.push_interleaved(&block, 2);
        assert_eq!(written, 512, "every endpoint must accept every frame");
    }

    for (i, ring) in rings.iter().enumerate() {
        let stats = ring.stats();
        assert_eq!(
            stats.written_frames, 512,
            "endpoint {i} must have received every frame"
        );
        assert_eq!(stats.dropped_frames, 0, "endpoint {i} must not drop frames");

        // And the frames it holds are the ones that were pushed.
        let mut out = vec![0.0f32; 1024];
        let popped = ring.pop_interleaved(&mut out, 2);
        assert_eq!(popped, 512, "endpoint {i} must hand the frames back");
        assert!(
            out.iter().all(|v| (*v - 0.5).abs() < 1e-9),
            "endpoint {i} must have received the block that was pushed"
        );
    }
}

#[test]
fn the_endpoints_own_separate_rings() {
    let mut registry = EndpointRegistry::new();
    let a = registry
        .add(EndpointConfig::new("a", None), 64)
        .expect("an endpoint ring must be creatable");
    let b = registry
        .add(EndpointConfig::new("b", None), 64)
        .expect("an endpoint ring must be creatable");
    assert!(
        !std::sync::Arc::ptr_eq(a.buffer(), b.buffer()),
        "two endpoints must not share one ring"
    );

    // A push into one endpoint must not appear in the other.
    let block = constant_block(16, 0.25);
    a.push_interleaved(&block, 2);
    assert_eq!(a.stats().written_frames, 16);
    assert_eq!(
        b.stats().written_frames,
        0,
        "an endpoint must not see another endpoint's frames"
    );
}

#[test]
fn adding_the_same_endpoint_twice_returns_the_same_ring() {
    let mut registry = EndpointRegistry::new();
    let first = registry
        .add(EndpointConfig::new("a", None), 64)
        .expect("an endpoint ring must be creatable");
    let second = registry
        .add(EndpointConfig::new("a", None), 64)
        .expect("an endpoint ring must be creatable");
    assert!(
        std::sync::Arc::ptr_eq(first.buffer(), second.buffer()),
        "re-adding an endpoint must not create a second ring"
    );
    assert_eq!(registry.len(), 1);
}

#[test]
fn each_endpoint_applies_its_own_trim() {
    // The null backend is a hardware-free sink, so the whole fan-out can be
    // exercised on a headless machine.
    let gains = [1.0f32, 0.5, 0.25];
    let mut workers = Vec::new();
    for (i, gain) in gains.iter().enumerate() {
        let mut config = EndpointConfig::new(format!("trim-{i}"), None);
        config.backend = AudioBackend::Null;
        config.gain = *gain;
        let worker = EndpointWorker::open(
            config,
            8192,
            SR,
            engine::ResamplerQuality::Balanced,
            config::ResamplerFilter::Fft,
            0.95,
            config::PrecisionMode::Performance,
        )
        .unwrap_or_else(|e| panic!("the null endpoint must open: {e}"));
        assert!(worker.is_running());
        assert_eq!(worker.config().gain, *gain);
        workers.push(worker);
    }

    let block = constant_block(4096, 1.0);
    for worker in workers.iter_mut() {
        let accepted = worker.push_interleaved(&block, 2);
        assert_eq!(accepted, 4096, "the endpoint must accept every frame");
    }

    for (i, worker) in workers.iter().enumerate() {
        let gain = gains[i];
        let mut out = vec![0.0f32; 8192 * 2];
        let popped = worker.ring().pop_interleaved(&mut out, 2);
        assert!(
            popped > 0,
            "endpoint {i} must still hold frames after the push"
        );
        let samples = &out[..popped * 2];
        for (n, sample) in samples.iter().enumerate() {
            assert!(
                (sample - gain).abs() < 1e-6,
                "endpoint {i}: sample {n} is {sample}, its own trim is {gain}"
            );
        }
    }
}

#[test]
fn an_endpoint_with_a_unity_trim_is_a_passthrough() {
    let mut config = EndpointConfig::new("unity", None);
    config.backend = AudioBackend::Null;
    config.gain = 1.0;
    let mut worker = EndpointWorker::open(
        config,
        8192,
        SR,
        engine::ResamplerQuality::Balanced,
        config::ResamplerFilter::Fft,
        0.95,
        config::PrecisionMode::Performance,
    )
    .expect("the null endpoint must open");

    let block = constant_block(4096, 0.75);
    worker.push_interleaved(&block, 2);

    let mut out = vec![0.0f32; 8192 * 2];
    let popped = worker.ring().pop_interleaved(&mut out, 2);
    assert!(popped > 0, "the endpoint must hold frames");
    for (n, sample) in out[..popped * 2].iter().enumerate() {
        assert!(
            (sample - 0.75).abs() < 1e-9,
            "a unity trim must be a passthrough, sample {n} is {sample}"
        );
    }
}

#[test]
fn an_endpoint_reports_its_own_counters() {
    let mut config = EndpointConfig::new("counters", None);
    config.backend = AudioBackend::Null;
    let mut worker = EndpointWorker::open(
        config,
        8192,
        SR,
        engine::ResamplerQuality::Balanced,
        config::ResamplerFilter::Fft,
        0.95,
        config::PrecisionMode::Performance,
    )
    .expect("the null endpoint must open");

    assert_eq!(worker.ring().stats().written_frames, 0);
    let block = constant_block(100, 1.0);
    worker.push_interleaved(&block, 2);
    let stats = worker.ring().stats();
    assert_eq!(stats.written_frames, 100);
    assert_eq!(stats.dropped_frames, 0);

    // The endpoint's id is the one it was configured with.
    assert_eq!(worker.ring().id(), &EndpointId::new("counters"));
    assert_eq!(worker.config().id, EndpointId::new("counters"));
}

#[test]
fn the_null_endpoint_is_hardware_free_and_never_claims_exclusivity() {
    // There is no mixer to bypass and no hardware to be perfect *to*, so the
    // null backend must not claim otherwise.
    let buffer = std::sync::Arc::new(
        engine::buffer::FixedFrameBuffer::new(1024).expect("a ring must be creatable"),
    );
    let output = NullOutput::new(buffer, 0, 0, 0).expect("the null sink must open");
    let info = output.output_info();
    assert!(!info.is_exclusive, "the null backend cannot be exclusive");
    assert_eq!(info.device_name, "null (no device)");
    assert_eq!(info.channels, 2);
    assert_eq!(info.actual_rate, 48_000);
    assert!(
        !output.supports_hardware_volume(),
        "the null backend has no hardware volume"
    );
}

#[test]
fn a_registry_update_replaces_the_endpoints_configuration() {
    let mut registry = EndpointRegistry::new();
    let id = EndpointId::new("trimmed");
    registry
        .add(EndpointConfig::new("trimmed", None), 64)
        .expect("an endpoint ring must be creatable");

    let mut config = EndpointConfig::new("trimmed", None);
    config.gain = 0.5;
    let previous = registry.update(config).expect("the endpoint must exist");
    assert_eq!(
        previous.gain, 1.0,
        "the previous configuration must be returned"
    );
    let (_, ring) = registry.get(&id).expect("the endpoint must still exist");
    assert_eq!(ring.stats().written_frames, 0);
}
