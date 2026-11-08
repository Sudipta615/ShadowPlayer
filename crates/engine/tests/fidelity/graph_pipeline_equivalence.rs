//! The frozen oracle, held to the production graph: **Graph 2.0 must render
//! the same samples as `DspPipeline`**, bit for bit.
//!
//! `crates/engine/AGENTS.md` §3.1 makes `src/dsp/pipeline/` the definition of
//! what the engine should do, and requires the Graph 2.0 runtime to match it
//! exactly. This suite is that requirement, executable.
//!
//! It is deliberately *not* the same claim as `tests/bit_perfect_equivalence.rs`.
//! That suite proves a **fully bypassed** chain reproduces its input; this one
//! proves two **independent implementations of the same non-trivial chain**
//! agree. A graph that silently dropped the EQ, or ran the limiter at a
//! different lookahead, would pass the bypass test and fail here.
//!
//! ## What is compared
//!
//! Both sides are built from one `EngineConfig` and fed one signal:
//!
//! * `DspPipeline::from_config` — the frozen reference oracle, a linear chain.
//! * `DspGraph::from_config` — the production arena graph the audio thread
//!   actually runs, lowered from the typed-port topology.
//!
//! The chain is the "full" configuration (EQ, limiter, multiband compressor,
//! crossfeed, stereo enhancer, EBU R128 loudness, dither) rather than an empty
//! one, because an empty chain agrees trivially and proves nothing.
//!
//! ## Why bit-exact and not a tolerance
//!
//! The engine's own classifier (`compare_buffers`) is used at
//! `DeterministicMode::StrictBitExact`. A tolerance would let a real
//! divergence hide behind a rounding difference, and the whole point of the
//! oracle is that the two paths are the *same arithmetic*, not merely similar
//! arithmetic. The measured result is bit-exact across every rate, block size
//! and precision mode in the matrix below; the suite asserts what is measured
//! rather than a bound it hopes holds.
//!
//! `the_comparison_is_not_vacuous` is the guard on the guard: it perturbs a
//! single sample by one ULP and asserts the classifier rejects it. Without it,
//! a comparison that always returned `BitExact` would pass every other test in
//! this file.

use engine::dsp::deterministic::{compare_buffers, DeterministicMode, EquivalenceClass};
use engine::dsp::graph2::prod::DspGraph;
use engine::dsp::pipeline::{DspPipeline, PrecisionMode};
use engine::EngineConfig;

/// The reference chain: every user-facing stage on, so the comparison has
/// something to disagree about if the two paths diverge.
fn full_chain_config() -> EngineConfig {
    let mut c = EngineConfig::default();
    c.eq.enabled = true;
    if c.eq.bands.len() >= 5 {
        c.eq.bands[0].enabled = true;
        c.eq.bands[0].gain_db = 3.0;
        c.eq.bands[2].enabled = true;
        c.eq.bands[2].gain_db = -2.0;
        c.eq.bands[4].gain_db = 1.5;
    }
    c.limiter.enabled = true;
    c.limiter.lookahead_ms = 2.0;
    c.multiband_compressor.enabled = true;
    c.crossfeed.enabled = true;
    c.stereo_enhancer.enabled = true;
    c.stereo_enhancer.width = 1.3;
    c.loudness.mode = config::LoudnessMode::EbuR128;
    c.dither_enabled = true;
    c
}

/// A chain with every stage bypassed, so the two paths must both be identity.
fn bypassed_config() -> EngineConfig {
    let mut c = EngineConfig::default();
    c.eq.enabled = false;
    c.limiter.enabled = false;
    c.multiband_compressor.enabled = false;
    c.crossfeed.enabled = false;
    c.stereo_enhancer.enabled = false;
    c.dither_enabled = false;
    c
}

/// A deterministic stereo signal: two different tones, so a stage that mixes
/// channels or applies a per-channel curve differently cannot hide.
fn stereo_signal(frames: usize) -> (Vec<f32>, Vec<f32>) {
    let mut left = vec![0.0f32; frames];
    let mut right = vec![0.0f32; frames];
    for i in 0..frames {
        let t = i as f32;
        left[i] = (t * 0.01).sin() * 0.4;
        right[i] = (t * 0.015).cos() * 0.4;
    }
    (left, right)
}

/// Render `frames` samples through both implementations, in `block`-sized
/// chunks, and return `(pipeline_l, pipeline_r, graph_l, graph_r)`.
///
/// Chunking matters: a graph that mishandles a block boundary (a delay line
/// that resets, a smoother that restarts) agrees with the pipeline at one
/// block size and diverges at another.
fn render_pair(
    config: &EngineConfig,
    sample_rate: f32,
    frames: usize,
    block: usize,
) -> (Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>) {
    render_pair_at_volume(config, sample_rate, frames, block, 0.8)
}

/// As [`render_pair`], with an explicit master volume.
///
/// The volume is a parameter because the identity case needs unity: a chain
/// with every stage bypassed is transparent only when the volume is 1.0, and a
/// test that forgot that would be asserting the wrong thing about the engine.
fn render_pair_at_volume(
    config: &EngineConfig,
    sample_rate: f32,
    frames: usize,
    block: usize,
    volume: f32,
) -> (Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>) {
    let mut pipeline = DspPipeline::from_config(config, sample_rate);
    let mut graph = DspGraph::from_config(config, sample_rate);

    // The same volume on both sides, so the volume path is compared too rather
    // than left at unity where it would be a no-op.
    pipeline.set_volume(volume);
    graph.volume_mut().processor.set_gain(volume);

    let (input_l, input_r) = stereo_signal(frames);
    let mut pipe_l = input_l.clone();
    let mut pipe_r = input_r.clone();
    let mut graph_l = input_l;
    let mut graph_r = input_r;

    let mut offset = 0;
    while offset < frames {
        let end = (offset + block).min(frames);
        pipeline.process_block(&mut pipe_l[offset..end], &mut pipe_r[offset..end]);
        graph.process_block(&mut graph_l[offset..end], &mut graph_r[offset..end]);
        offset = end;
    }

    (pipe_l, pipe_r, graph_l, graph_r)
}

/// Assert one channel pair is bit-exact, naming the first sample that moved.
fn assert_bit_exact(reference: &[f32], actual: &[f32], what: &str) {
    assert_eq!(
        reference.len(),
        actual.len(),
        "{what}: the two paths must render the same number of frames"
    );

    let class = compare_buffers(reference, actual, 0.0, 0.0);
    assert_eq!(
        class,
        EquivalenceClass::BitExact,
        "{what}: the graph must match the reference pipeline bit for bit, got {class:?}"
    );
    assert!(
        class.satisfies(DeterministicMode::StrictBitExact),
        "{what}: the result must satisfy StrictBitExact"
    );

    // Compare the raw bit patterns too, so a failure reports the exact sample
    // and both values instead of only a classification.
    for (i, (a, b)) in reference.iter().zip(actual.iter()).enumerate() {
        assert_eq!(
            a.to_bits(),
            b.to_bits(),
            "{what}: sample {i} diverged: pipeline={a} ({:#010x}) vs graph={b} ({:#010x})",
            a.to_bits(),
            b.to_bits()
        );
    }
}

#[test]
fn the_full_chain_is_bit_exact_at_every_rate_block_and_precision() {
    // The matrix is the point: a single configuration can agree by luck, and
    // a divergence in a per-rate coefficient table or a block-boundary
    // smoother only shows up at the sizes that exercise it.
    const RATES: [f32; 4] = [44_100.0, 48_000.0, 96_000.0, 192_000.0];
    const BLOCKS: [usize; 3] = [64, 256, 1024];
    const PRECISIONS: [PrecisionMode; 2] = [PrecisionMode::Performance, PrecisionMode::Quality];
    const FRAMES: usize = 2048;

    for &sample_rate in &RATES {
        for &block in &BLOCKS {
            for &precision in &PRECISIONS {
                let mut config = full_chain_config();
                config.precision_mode = precision;

                let (pipe_l, pipe_r, graph_l, graph_r) =
                    render_pair(&config, sample_rate, FRAMES, block);

                let label = format!("full chain @ {sample_rate} Hz, block {block}, {precision:?}");
                assert_bit_exact(&pipe_l, &graph_l, &format!("{label}, left"));
                assert_bit_exact(&pipe_r, &graph_r, &format!("{label}, right"));
            }
        }
    }
}

#[test]
fn a_bypassed_chain_is_identity_on_both_paths() {
    // Both implementations must be transparent when nothing is enabled. This
    // is the weaker claim `tests/bit_perfect_equivalence.rs` makes for the
    // pipeline alone; here both sides are held to it, so a graph that applies
    // an unexpected gain is caught.
    let config = bypassed_config();
    let frames = 1024;
    let (input_l, input_r) = stereo_signal(frames);
    // Unity volume: a bypassed chain is identity only when the master gain is
    // 1.0, and this test is about the stages, not the volume.
    let (pipe_l, pipe_r, graph_l, graph_r) =
        render_pair_at_volume(&config, 48_000.0, frames, 256, 1.0);

    assert_bit_exact(&input_l, &pipe_l, "bypassed pipeline, left");
    assert_bit_exact(&input_r, &pipe_r, "bypassed pipeline, right");
    assert_bit_exact(&input_l, &graph_l, "bypassed graph, left");
    assert_bit_exact(&input_r, &graph_r, "bypassed graph, right");
}

#[test]
fn a_single_block_renders_the_same_as_many_blocks() {
    // Block-boundary state is where a graph and a linear pipeline are most
    // likely to drift: delay lines, smoothers and limiter lookahead all carry
    // state across the boundary. Rendering the same signal as one long block
    // and as many small blocks must agree on both sides.
    let config = full_chain_config();
    let frames = 1024;

    let (whole_l, whole_r, whole_graph_l, whole_graph_r) =
        render_pair(&config, 48_000.0, frames, frames);
    let (split_l, split_r, split_graph_l, split_graph_r) =
        render_pair(&config, 48_000.0, frames, 64);

    assert_bit_exact(&whole_l, &split_l, "pipeline, one block vs many");
    assert_bit_exact(&whole_r, &split_r, "pipeline, one block vs many (right)");
    assert_bit_exact(&whole_graph_l, &split_graph_l, "graph, one block vs many");
    assert_bit_exact(
        &whole_graph_r,
        &split_graph_r,
        "graph, one block vs many (right)",
    );
}

#[test]
fn the_comparison_is_not_vacuous() {
    // Guard on the guard. If `compare_buffers` returned `BitExact`
    // unconditionally, every assertion above would pass while proving nothing.
    // One ULP on one sample must be rejected.
    let reference = vec![0.5f32; 64];
    let mut perturbed = reference.clone();
    perturbed[17] = f32::from_bits(perturbed[17].to_bits() ^ 1);

    assert_ne!(
        perturbed[17].to_bits(),
        reference[17].to_bits(),
        "the perturbation must actually change the bits"
    );
    assert_ne!(
        compare_buffers(&reference, &perturbed, 0.0, 0.0),
        EquivalenceClass::BitExact,
        "a one-ULP difference must not classify as bit-exact, or the \
         equivalence assertions in this suite are vacuous"
    );

    // And an identical buffer must still classify as bit-exact, so the test
    // above is not passing because the classifier rejects everything.
    assert_eq!(
        compare_buffers(&reference, &reference.clone(), 0.0, 0.0),
        EquivalenceClass::BitExact,
        "identical buffers must classify as bit-exact"
    );
}
