//! # Graph 2.0 topology lowering and plan compilation
//!
//! Measures the engine's **control-path** graph construction cost: how long
//! it takes to turn a chain request into a compiled, executable audio graph.
//! Nothing here runs on the audio thread — it is the cost a host pays when
//! the user changes the DSP configuration, and the cost that has to fit in
//! the control-thread latency budget.
//!
//! What the numbers mean:
//!
//! - Every row is one complete graph-construction operation. The `time` is
//!   wall-clock per operation; `Throughput::Elements` counts the graph *stages*
//!   the operation touches, so Criterion also reports stages/second.
//!
//! What is actually being measured:
//!
//! - `topology_build` — [`build_requested_topology`]: the Graph 2.0 typed-port
//!   topology for the canonical production chain (15 mix-chain stages plus the
//!   three output-domain stages), including node creation, edge wiring,
//!   structural validation and the deterministic topological compile. The
//!   `narrowed` row is the same call for a 5-stage chain, so the row pair
//!   shows how the cost scales with chain length.
//! - `prepare_chain` — [`Graph2Engine::prepare_chain`]: the *whole*
//!   control-path pipeline the engine actually runs before it may publish a
//!   new generation. It builds and compiles the topology, lowers it into the
//!   arena's plan set, allocates the node arena, applies the configuration,
//!   initialises the DSP state, prepares impulse responses and plugins, builds
//!   the FFT plans, validates the built generation against its own plan, and
//!   measures its resource footprint. This is the number a host's
//!   "reconfigure the DSP" latency is made of.
//! - `engine_from_config` — [`Graph2Engine::from_config`]: building the
//!   engine plus its first generation in one call (the process-start path).
//! - `reconfigure` — [`Graph2Engine::reconfigure`]: the live-reconfiguration
//!   path (build a fresh generation and publish it).
//! - `graph2_synthetic` — the *generic* [`Graph2`] API rather than the
//!   production chain: `add_gain`/`add_edge`/`validate`/`compile` on a chain of
//!   N gain nodes. It measures the topology substrate itself, with no arena
//!   and no DSP state.
//! - `preparation_estimate` — [`estimate_graph_preparation`]: the cheap
//!   planning call a host makes *before* deciding to build a generation at
//!   all.
//!
//! The transient compilation footprint the engine reports for the canonical
//! chain is printed once by this bench (it is a byte count, not a duration, so
//! it does not fit Criterion's time axis).

use std::hint::black_box;

use config::EngineConfig;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use engine::dsp::graph2::prod::{
    build_requested_topology, estimate_graph_preparation, ChainRequest, ChainRouting, Graph2Engine,
    CANONICAL_CHAIN,
};
use engine::dsp::graph2::{Graph2, PortId, ProdStage};

mod common;

/// A five-stage chain: the routing head, the mix bus, the EQ, the volume
/// node and the spatial master. Every stage the arena cannot run without is
/// present (`MixBus`, `Volume`), so the request is valid.
fn narrowed_request() -> ChainRequest {
    ChainRequest {
        stages: vec![
            ProdStage::Routing,
            ProdStage::MixBus,
            ProdStage::Eq,
            ProdStage::Volume,
            ProdStage::Spatial,
        ],
        routing: ChainRouting::Trimmed,
    }
}

/// Build a synthetic graph of `n` gain stages in a chain and compile it.
fn synthetic_graph(n: usize) -> Graph2 {
    let mut g = Graph2::new();
    let mut prev: Option<engine::dsp::graph2::NodeId> = None;
    for i in 0..n {
        let id = g.add_gain(&format!("gain{i}"), 1.0);
        if let Some(p) = prev {
            let _ = g.add_edge(p, PortId::OUT, id, PortId::IN);
        }
        prev = Some(id);
    }
    g
}

fn bench_graph_construction(c: &mut Criterion) {
    let sample_rate = common::SAMPLE_RATE;
    let config = EngineConfig::default();

    // Report the engine's own transient compilation footprint for the
    // canonical chain — a byte count, so it does not belong on Criterion's
    // time axis.
    if let Ok(topo) = build_requested_topology(&ChainRequest::canonical()) {
        eprintln!(
            "[graph_construction] canonical chain: {} stages, {} nodes, {} edges, \
             transient compilation footprint {} bytes",
            CANONICAL_CHAIN.len(),
            topo.graph.node_count(),
            topo.graph.edge_count(),
            topo.compilation_bytes(),
        );
    }

    // ── Topology construction + validation + topological compile ────────────
    let mut group = c.benchmark_group("graph_construction/topology_build");
    group.throughput(Throughput::Elements(CANONICAL_CHAIN.len() as u64));
    group.bench_function("canonical", |b| {
        b.iter(|| {
            black_box(build_requested_topology(black_box(
                &ChainRequest::canonical(),
            )))
        });
    });
    let narrowed = narrowed_request();
    group.throughput(Throughput::Elements(narrowed.stages.len() as u64));
    group.bench_function("narrowed_5_stages", |b| {
        b.iter(|| black_box(build_requested_topology(black_box(&narrowed))));
    });
    group.finish();

    // ── The full control-path generation preparation ───────────────────────
    let mut group = c.benchmark_group("graph_construction/prepare_chain");
    group.throughput(Throughput::Elements(CANONICAL_CHAIN.len() as u64));
    {
        let mut engine = Graph2Engine::from_config(&config, sample_rate);
        group.bench_function("canonical", |b| {
            b.iter(|| {
                black_box(
                    engine
                        .prepare_chain(black_box(&config), black_box(&ChainRequest::canonical()))
                        .expect("the canonical chain is valid by construction"),
                )
            });
        });
    }
    {
        let mut engine = Graph2Engine::from_config(&config, sample_rate);
        group.throughput(Throughput::Elements(narrowed.stages.len() as u64));
        group.bench_function("narrowed_5_stages", |b| {
            b.iter(|| {
                black_box(
                    engine
                        .prepare_chain(black_box(&config), black_box(&narrowed))
                        .expect("the narrowed chain is valid"),
                )
            });
        });
    }
    group.finish();

    // ── Engine construction and live reconfiguration ───────────────────────
    let mut group = c.benchmark_group("graph_construction/engine");
    group.throughput(Throughput::Elements(CANONICAL_CHAIN.len() as u64));
    group.bench_function("from_config", |b| {
        b.iter(|| black_box(Graph2Engine::from_config(black_box(&config), sample_rate)));
    });
    {
        let mut engine = Graph2Engine::from_config(&config, sample_rate);
        group.bench_function("reconfigure", |b| {
            b.iter(|| engine.reconfigure(black_box(&config)));
        });
    }
    group.finish();

    // ── The generic Graph2 substrate ────────────────────────────────────────
    let mut group = c.benchmark_group("graph_construction/graph2_synthetic");
    for n in [8usize, 64, 256] {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter(|| {
                let mut g = synthetic_graph(n);
                black_box(g.validate());
                black_box(g.compile().expect("a chain of gains is acyclic"));
            });
        });
    }
    group.finish();

    // ── The cheap pre-flight planning call ─────────────────────────────────
    let mut group = c.benchmark_group("graph_construction/preparation_estimate");
    for slots in [1usize, 8] {
        group.throughput(Throughput::Elements(slots as u64));
        group.bench_with_input(BenchmarkId::from_parameter(slots), &slots, |b, &s| {
            b.iter(|| black_box(estimate_graph_preparation(s)));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_graph_construction);
criterion_main!(benches);
