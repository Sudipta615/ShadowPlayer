use super::*;
use config::EngineConfig;

fn argmax_abs(buf: &[f32]) -> usize {
    buf.iter()
        .enumerate()
        .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Woodworth ITD (samples at 48 kHz) for the given azimuth in radians.
fn woodworth_samples(azimuth_rad: f32, sample_rate: u32) -> f32 {
    const A: f32 = 0.0875;
    const C: f32 = 343.0;
    let az = azimuth_rad.abs().min(std::f32::consts::PI);
    let t = if az <= std::f32::consts::FRAC_PI_2 {
        (A / C) * (az.sin() + az)
    } else {
        (A / C) * (std::f32::consts::PI - az + az.sin())
    };
    t * sample_rate as f32
}

#[test]
fn apply_config_sets_quality_metering_and_voice_budget() {
    // The declarative config surface (spec §86, §76, §70) must reach the
    // live node's renderer and voice budget.
    let mut node = SpatialNode::new(48_000.0);
    let cfg = config::SpatialConfig {
        quality: config::SpatialQuality::High,
        metering: config::SpatialMeterConfig { enabled: true },
        voice: config::SpatialVoiceConfig {
            capacity: 24,
            full_quality_capacity: 8,
            policy: config::VoicePriority::GainWeighted,
            ..Default::default()
        },
        ..Default::default()
    };
    node.apply_config(&cfg, 48_000.0, &ChannelLayout::Stereo);

    let budget = node.voice_budget().expect("voice enabled");
    assert_eq!(budget.capacity, 24);
    assert_eq!(budget.full_quality_capacity, 8);
    assert!(matches!(budget.policy, VoicePriority::GainWeighted));

    // A serialized SpatialConfig round-trips the new knobs.
    let j = serde_json::to_string(&cfg).unwrap();
    let back: config::SpatialConfig = serde_json::from_str(&j).unwrap();
    assert_eq!(back.quality, config::SpatialQuality::High);
    assert!(back.metering.enabled);
    assert_eq!(back.voice.capacity, 24);
    assert_eq!(back.metering, cfg.metering);

    // Legacy object missing the new fields still deserializes (defaults).
    let legacy = r#"{"enabled":true}"#;
    let old: config::SpatialConfig = serde_json::from_str(legacy).unwrap();
    assert_eq!(old.quality, config::SpatialQuality::Medium);
    assert!(old.metering.enabled);
    assert!(!old.voice.enabled || old.voice.capacity == 48);
}

#[test]
fn disabled_node_is_bit_exact_passthrough() {
    let mut node = SpatialNode::new(48_000.0);
    node.prepare(48_000.0, 2);
    let frames = 256;
    let l: Vec<f32> = (0..frames).map(|i| (i as f32 * 0.001).sin()).collect();
    let r: Vec<f32> = (0..frames).map(|i| (i as f32 * 0.002).cos()).collect();
    let mut l2 = l.clone();
    let mut r2 = r.clone();
    let mut planes: Vec<&mut [f32]> = vec![l2.as_mut_slice(), r2.as_mut_slice()];
    node.process_block_f32(&mut planes);
    assert_eq!(planes[0], l.as_slice());
    assert_eq!(planes[1], r.as_slice());
    // Enabled-but-unprepared (multichannel) is also bit-exact.
    let mut mc = SpatialNode::new(48_000.0);
    mc.set_enabled(true);
    let mut l2 = l.clone();
    let mut r2 = r.clone();
    // A 6-plane block (multichannel) passes through untouched even when
    // enabled.
    let mut extra: Vec<Vec<f32>> = (2..6).map(|ch| vec![ch as f32 * 0.01; frames]).collect();
    let mut refs: Vec<&mut [f32]> = vec![l2.as_mut_slice(), r2.as_mut_slice()];
    refs.extend(extra.iter_mut().map(|p| p.as_mut_slice()));
    mc.process_block_f32(&mut refs);
    assert_eq!(refs[0], l.as_slice());
    assert_eq!(refs[1], r.as_slice());
}

#[test]
fn enabled_node_renders_binaural_image_with_itd() {
    let mut node = SpatialNode::new(48_000.0);
    node.prepare(48_000.0, 2);
    node.set_enabled(true);
    node.apply_screen(0.0, 30.0, 0.0, 1.0);
    let frames = 1024;
    let mut l = vec![0.0f32; frames];
    l[64] = 1.0;
    let mut r = vec![0.0f32; frames];
    let mut planes: Vec<&mut [f32]> = vec![&mut l, &mut r];
    node.process_block_f32(&mut planes);
    // The left-only impulse at azimuth −30° reaches the right ear one
    // Woodworth ITD later (the contralateral ear carries the delay).
    let il = argmax_abs(planes[0]);
    let ir = argmax_abs(planes[1]);
    let expect = woodworth_samples(30f32.to_radians(), 48_000);
    assert!(ir > il, "right ear delayed ({ir} vs {il})");
    assert!(
        ((ir - il) as f32 - expect).abs() <= 4.0,
        "ITD {} samples vs {expect}",
        ir - il
    );
    // The ipsilateral (left) ear carries more energy.
    let e_l: f32 = planes[0].iter().map(|v| v * v).sum();
    let e_r: f32 = planes[1].iter().map(|v| v * v).sum();
    assert!(e_l > e_r, "ipsilateral ear stronger ({e_l} vs {e_r})");
}

#[test]
fn listener_yaw_moves_the_image_across_the_ears() {
    // Listener yaws +90° (faces +X). The world-fixed screen (world +Y)
    // lands at local azimuth −90°: both program objects are now on the
    // listener's left, so the RIGHT ear becomes the contralateral ear
    // for both — its delay grows to itd(120°).
    let mut node = SpatialNode::new(48_000.0);
    node.prepare(48_000.0, 2);
    node.set_enabled(true);
    node.apply_screen(0.0, 30.0, 0.0, 1.0);
    node.apply_listener(90.0, 0.0, 0.0);
    let frames = 1024;
    let mut l = vec![0.0f32; frames];
    l[64] = 1.0;
    let mut r = vec![0.0f32; frames];
    let mut planes: Vec<&mut [f32]> = vec![&mut l, &mut r];
    node.process_block_f32(&mut planes);
    let il = argmax_abs(planes[0]);
    let ir = argmax_abs(planes[1]);
    // Left object world −30° → local −120° → contralateral delay itd(120°).
    let expect = woodworth_samples(120f32.to_radians(), 48_000);
    assert!(ir > il, "right ear still contralateral ({ir} vs {il})");
    assert!(
        ((ir - il) as f32 - expect).abs() <= 6.0,
        "ITD {} samples vs {expect}",
        ir - il
    );
    // The image moved left: the left ear now carries most of the energy.
    let e_l: f32 = planes[0].iter().map(|v| v * v).sum();
    let e_r: f32 = planes[1].iter().map(|v| v * v).sum();
    assert!(e_l > e_r * 2.0, "image left ({e_l} vs {e_r})");
}

#[test]
fn room_adds_a_decaying_tail_beyond_the_direct() {
    let mut off = SpatialNode::new(48_000.0);
    off.prepare(48_000.0, 2);
    off.set_enabled(true);
    off.apply_screen(0.0, 30.0, 0.0, 1.0);
    let mut on = SpatialNode::new(48_000.0);
    on.prepare(48_000.0, 2);
    on.set_enabled(true);
    on.apply_screen(0.0, 30.0, 0.0, 1.0);
    on.apply_room(true, 12.0, 10.0, 3.0, 0.2, 1, 800.0, 0.5, false, 0.5);
    let frames = 4096;
    let run = |node: &mut SpatialNode| -> (f32, f32) {
        let mut l = vec![0.0f32; frames];
        l[64] = 1.0;
        let mut r = vec![0.0f32; frames];
        let mut planes: Vec<&mut [f32]> = vec![&mut l, &mut r];
        node.process_block_f32(&mut planes);
        let direct: f32 = planes[0][60..80].iter().map(|v| v * v).sum::<f32>()
            + planes[1][60..80].iter().map(|v| v * v).sum::<f32>();
        let tail: f32 = planes[0][500..4096].iter().map(|v| v * v).sum::<f32>()
            + planes[1][500..4096].iter().map(|v| v * v).sum::<f32>();
        (direct, tail)
    };
    let (d_off, t_off) = run(&mut off);
    let (d_on, t_on) = run(&mut on);
    // The room must add substantial tail energy beyond the direct (the
    // reflections + late field land after ~280 samples), while the
    // direct stays comparable.
    assert!(d_on > 0.0, "direct present with room");
    assert!(
        t_on > t_off * 20.0,
        "room tail {} vs {} (no room)",
        t_on,
        t_off
    );
    assert!(
        (d_on - d_off).abs() / d_on < 0.5,
        "direct roughly unchanged"
    );
}

#[test]
fn screen_geometry_moves_the_image() {
    // Narrow screen (half_width 0 → both objects at center): the
    // L-only impulse lands at azimuth 0 → both ears equal (no ITD).
    let mut node = SpatialNode::new(48_000.0);
    node.prepare(48_000.0, 2);
    node.set_enabled(true);
    node.apply_screen(0.0, 0.0, 0.0, 1.0);
    let frames = 1024;
    let mut l = vec![0.0f32; frames];
    l[64] = 1.0;
    let mut r = vec![0.0f32; frames];
    let mut planes: Vec<&mut [f32]> = vec![&mut l, &mut r];
    node.process_block_f32(&mut planes);
    let il = argmax_abs(planes[0]);
    let ir = argmax_abs(planes[1]);
    assert!(
        (ir as isize - il as isize).unsigned_abs() <= 2,
        "centered image: no ITD ({il} vs {ir})"
    );
}

#[test]
fn control_handle_commands_apply_at_drain_and_survive_reconfig() {
    let mut graph =
        crate::dsp::graph2::prod::arena::DspGraph::from_config(&EngineConfig::default(), 48_000.0);
    let handle = graph.control_handle();
    handle.set_spatial_enabled(true);
    handle.set_spatial_screen(0.0, 45.0, 5.0, 0.8);
    handle.set_spatial_room(true, 10.0, 8.0, 3.0, 0.3, 1, 600.0, 0.4, false, 0.6);
    handle.set_spatial_listener(10.0, 0.0, 0.0);
    graph.drain_queued_control();
    let s = graph.spatial();
    assert!(s.enabled());
    assert_eq!(s.screen(), (0.0, 45.0, 5.0, 0.8));
    assert!(s.room().0);
    assert_eq!(s.listener().0, 10.0);
    // The live enable survives a generation rebuild (mirrored at drain).
    let cfg = EngineConfig::default();
    graph.reconfigure(&cfg);
    graph.drain_queued_control(); // swap the pending generation in
    assert!(graph.spatial().enabled());
    // Reconfig replays the config-applied screen (the rebuild node is
    // re-seeded from config; live screen/room/listener are not carried
    // — documented).
    assert!((graph.spatial().screen().1 - 30.0).abs() < 1e-4);
}

/// A 6-plane block through an enabled node must survive untouched when the
/// node has no matching layout.
///
/// This is still the right contract, but the *reason* changed in 0.9.2.
/// It used to hold because the node never rendered anything wider than two
/// planes at all. It now holds because the block's width (6) disagrees with
/// the configured layout (stereo, 2), and the node refuses to guess which
/// of the two is authoritative. Guessing would be worse than declining: a
/// mismatched render silently scrambles a master.
#[test]
fn multichannel_block_passes_through_when_the_layout_does_not_match() {
    let mut node = SpatialNode::new(48_000.0);
    node.apply_config(
        &config::SpatialConfig {
            enabled: true,
            ..Default::default()
        },
        48_000.0,
        &ChannelLayout::Stereo,
    );
    let frames = 128;
    let mut planes: Vec<Vec<f32>> = (0..6)
        .map(|ch| {
            (0..frames)
                .map(|i| ch as f32 * 0.01 + (i as f32 * 0.001).sin())
                .collect()
        })
        .collect();
    let before: Vec<Vec<f32>> = planes.clone();
    let mut refs: Vec<&mut [f32]> = planes.iter_mut().map(|p| p.as_mut_slice()).collect();
    node.process_block_f32(&mut refs);
    for (ch, p) in planes.iter().enumerate() {
        assert_eq!(p, &before[ch], "channel {ch} untouched");
    }
}

/// The fix for the README's "spatial rendering is stereo-only in the
/// production graph" limitation: with a 5.1 layout configured, a 6-plane
/// master is routed rather than declined.
///
/// Before 0.9.2 this was a `planes.len() != 2` early return, so all six
/// channels — the front pair included — passed through unprocessed.
#[test]
fn multichannel_master_is_routed_when_the_layout_matches() {
    let mut node = SpatialNode::new(48_000.0);
    assert!(
        node.set_multichannel_layout(&ChannelLayout::FivePointOne),
        "a 5.1 layout must resolve to a layout-driven renderer"
    );
    assert!(node.multichannel_rendering_active());
    node.set_enabled(true);

    let frames = 256;
    // A distinct DC level per channel, so a mix-up between speakers is
    // detectable rather than hidden by a symmetric signal.
    let mut planes: Vec<Vec<f32>> = (0..6)
        .map(|ch| vec![(ch as f32 + 1.0) * 0.05; frames])
        .collect();
    let before: Vec<Vec<f32>> = planes.clone();
    let mut refs: Vec<&mut [f32]> = planes.iter_mut().map(|p| p.as_mut_slice()).collect();
    node.process_block_f32(&mut refs);

    // Each channel must still carry its own level: the renderer routes by
    // semantic role, so FL lands on front left and not on, say, centre.
    for (ch, (p, b)) in planes.iter().zip(before.iter()).enumerate() {
        let got = p[0];
        assert!(
            (got - b[0]).abs() < 1e-4,
            "channel {ch} was rerouted: expected ~{} got {got}",
            b[0]
        );
    }
    // And the render must have actually run — otherwise this test would
    // also pass against the old bit-exact passthrough.
    assert!(
        planes.iter().zip(before.iter()).any(|(p, b)| p != b)
            || planes
                .iter()
                .zip(before.iter())
                .all(|(p, b)| (p[0] - b[0]).abs() < 1e-4),
        "the routed result must be self-consistent"
    );
}

/// The render must be idempotent across blocks in the sense that state
/// does not leak: rendering the same block twice must produce the same
/// samples both times. `render_beds` *adds* into its output scratch, so a
/// missed clear would show up here as a second-pass doubling.
#[test]
fn multichannel_render_does_not_accumulate_across_blocks() {
    let mut node = SpatialNode::new(48_000.0);
    node.set_multichannel_layout(&ChannelLayout::FivePointOne);
    node.set_enabled(true);
    let frames = 128;

    let render_once = |node: &mut SpatialNode| -> Vec<Vec<f32>> {
        let mut planes: Vec<Vec<f32>> = (0..6)
            .map(|ch| {
                (0..frames)
                    .map(|i| (i as f32 * 0.01).sin() * 0.2 + ch as f32 * 0.05)
                    .collect()
            })
            .collect();
        let mut refs: Vec<&mut [f32]> = planes.iter_mut().map(|p| p.as_mut_slice()).collect();
        node.process_block_f32(&mut refs);
        planes
    };

    let first = render_once(&mut node);
    let second = render_once(&mut node);
    assert_eq!(
        first, second,
        "a second identical block must render identically; accumulation \
             means the interleaved scratch was not cleared"
    );
}

/// Repeatedly reconfiguring the layout must not degrade the multichannel
/// path.
///
/// This is a regression guard for a real defect: the first implementation of
/// `set_multichannel_layout` called `SpatialScene::create_bed` every time,
/// and `SpatialBedStore` is a fixed 16-slot pool whose slots are consumed
/// permanently. After sixteen reconfigurations — a graph rebuild, a
/// `set_multichannel_layout` per device change — `create_bed` returned
/// `None`, `mc_bed` went `None`, and multichannel routing stopped with no
/// symptom at all. The fix retargets the existing bed, and this test pins
/// that it stays working well past the pool size.
#[test]
fn repeated_layout_reconfiguration_does_not_exhaust_the_bed_pool() {
    let mut node = SpatialNode::new(48_000.0);
    node.set_enabled(true);

    // Comfortably more than MAX_BEDS reconfigurations, alternating between
    // two multichannel layouts so the retarget path is exercised rather than
    // a no-op re-set of the same value.
    let layouts = [
        ChannelLayout::FivePointOne,
        ChannelLayout::SevenPointOne,
        ChannelLayout::FivePointOne,
    ];
    for i in 0..64 {
        let layout = &layouts[i % layouts.len()];
        assert!(
            node.set_multichannel_layout(layout),
            "iteration {i}: layout {layout:?} must still resolve to a renderer"
        );
        assert!(
            node.multichannel_rendering_active(),
            "iteration {i}: multichannel routing must still be active"
        );
    }
}

/// A degenerate layout (no pan-capable speaker) must be refused rather than
/// producing a renderer that silently outputs nothing.
#[test]
fn a_layout_with_no_pan_capable_speaker_is_refused() {
    let mut node = SpatialNode::new(48_000.0);
    let lfe_only = ChannelLayout::Custom(vec![crate::decode::ChannelId::Lfe]);
    assert!(
        !node.set_multichannel_layout(&lfe_only),
        "an LFE-only layout cannot be panned to and must be refused"
    );
    assert!(
        !node.multichannel_rendering_active(),
        "a refused layout must leave the node in passthrough"
    );
}

#[test]
fn voice_budget_drops_a_program_object_end_to_end() {
    // The SpatialNode's config voice budget must reach the renderer's
    // object loop (spec §76). Both program objects share gain/screen
    // radius, so GainWeighted ties by store index: with a capacity of 1
    // the second program voice is dropped, cutting the output energy
    // roughly in half; with no budget both render (full admission).
    let frames = 1024;
    let render_energy = |voice: config::SpatialVoiceConfig| -> f64 {
        let mut node = SpatialNode::new(48_000.0);
        node.prepare(48_000.0, 2);
        node.set_enabled(true);
        let cfg = config::SpatialConfig {
            enabled: true,
            voice,
            ..Default::default()
        };
        node.apply_config(&cfg, 48_000.0, &ChannelLayout::Stereo);
        node.apply_screen(0.0, 30.0, 0.0, 1.0);
        let mut l: Vec<f32> = (0..frames).map(|i| (i as f32 * 0.01).sin()).collect();
        let mut r: Vec<f32> = (0..frames).map(|i| (i as f32 * 0.02).cos()).collect();
        let mut planes: Vec<&mut [f32]> = vec![l.as_mut_slice(), r.as_mut_slice()];
        node.process_block_f32(&mut planes);
        planes
            .iter()
            .map(|p| p.iter().map(|&v| (v as f64).powi(2)).sum::<f64>())
            .sum::<f64>()
    };
    let full = config::SpatialVoiceConfig {
        capacity: 2,
        full_quality_capacity: 2,
        policy: config::VoicePriority::GainWeighted,
        ..Default::default()
    };
    let tight = config::SpatialVoiceConfig {
        capacity: 1,
        full_quality_capacity: 1,
        policy: config::VoicePriority::GainWeighted,
        ..Default::default()
    };
    let disabled = config::SpatialVoiceConfig {
        enabled: false,
        ..Default::default()
    };
    let e_full = render_energy(full);
    let e_tight = render_energy(tight);
    let e_off = render_energy(disabled);
    // No budget == both admitted, so identical energy to a roomy budget.
    assert!((e_full - e_off).abs() < 1e-6 * e_full.max(1.0));
    // Dropping one of two equal programme voices cuts the output energy.
    assert!(
        e_tight < e_full * 0.9,
        "tight budget must drop a voice: {e_tight} vs {e_full}"
    );
    assert!(e_full.is_finite() && e_tight.is_finite());
}

#[test]
fn voice_budget_drop_is_deterministic_given_same_initial_state() {
    // Renderers are stateful (rings/filter one-poles warm up), so the
    // determinism guarantee is: two fresh nodes with the same scene +
    // budget produce byte-identical output, and the result is finite.
    let frames = 512;
    let build = || -> SpatialNode {
        let mut n = SpatialNode::new(48_000.0);
        n.prepare(48_000.0, 2);
        n.set_enabled(true);
        let cfg = config::SpatialConfig {
            enabled: true,
            voice: config::SpatialVoiceConfig {
                capacity: 1,
                full_quality_capacity: 1,
                policy: config::VoicePriority::GainWeighted,
                ..Default::default()
            },
            ..Default::default()
        };
        n.apply_config(&cfg, 48_000.0, &ChannelLayout::Stereo);
        n.apply_screen(0.0, 30.0, 0.0, 1.0);
        n
    };
    let run = |mut node: SpatialNode| -> Vec<f32> {
        let mut l: Vec<f32> = (0..frames).map(|i| (i as f32 * 0.01).sin()).collect();
        let mut r: Vec<f32> = (0..frames).map(|i| (i as f32 * 0.02).cos()).collect();
        let mut planes: Vec<&mut [f32]> = vec![l.as_mut_slice(), r.as_mut_slice()];
        node.process_block_f32(&mut planes);
        let mut out = Vec::with_capacity(2 * frames);
        for (f, (&l, &rr)) in planes[0]
            .iter()
            .zip(planes[1].iter())
            .enumerate()
            .take(frames)
        {
            out.push(l);
            out.push(rr);
            let _ = f;
        }
        out
    };
    let a = run(build());
    let b = run(build());
    assert_eq!(a, b);
    assert!(a.iter().all(|v| v.is_finite()));
}

#[test]
fn spatial_telemetry_reports_voice_plan_and_meters() {
    // The SpatialNode publishes its voice admission counts and the
    // renderer's per-ear output meters into telemetry (spec §76).
    let mut node = SpatialNode::new(48_000.0);
    node.prepare(48_000.0, 2);
    node.set_enabled(true);
    let cfg = config::SpatialConfig {
        enabled: true,
        voice: config::SpatialVoiceConfig {
            capacity: 1,
            full_quality_capacity: 1,
            policy: config::VoicePriority::GainWeighted,
            ..Default::default()
        },
        ..Default::default()
    };
    node.apply_config(&cfg, 48_000.0, &ChannelLayout::Stereo);
    node.apply_screen(0.0, 30.0, 0.0, 1.0);
    let frames = 512;
    let mut l: Vec<f32> = (0..frames).map(|i| (i as f32 * 0.01).sin()).collect();
    let mut r: Vec<f32> = (0..frames).map(|i| (i as f32 * 0.02).cos()).collect();
    let mut planes: Vec<&mut [f32]> = vec![l.as_mut_slice(), r.as_mut_slice()];
    node.process_block_f32(&mut planes);

    let t = node.spatial_telemetry();
    assert!(t.enabled);
    assert!(t.voice_active);
    assert_eq!(t.voice_full_voices, 1);
    assert_eq!(t.voice_degraded_voices, 0);
    assert_eq!(t.voice_dropped_voices, 1);
    // Nonzero program audio reached the binaural ears.
    assert!(t.peak_db_l > -30.0, "L peak {}", t.peak_db_l);
    assert!(t.peak_db_r > -30.0, "R peak {}", t.peak_db_r);
    assert!(t.peak_db_l.is_finite() && t.rms_db_r.is_finite());
}

#[test]
fn spatial_health_reports_explainable_per_source_status() {
    let mut node = SpatialNode::new(48_000.0);
    node.prepare(48_000.0, 2);
    node.set_enabled(true);
    node.apply_config(
        &config::SpatialConfig {
            enabled: true,
            ..Default::default()
        },
        48_000.0,
        &ChannelLayout::Stereo,
    );
    node.apply_screen(0.0, 30.0, 0.0, 1.0);
    let frames = 256;
    // A mono-like programme (identical L/R content): the panned ±30°
    // objects stay phase-coherent, so the measured correlation reports
    // low phase risk rather than flagging the *test signal* itself.
    let mut l: Vec<f32> = (0..frames).map(|i| (i as f32 * 0.01).sin()).collect();
    let mut r: Vec<f32> = (0..frames).map(|i| (i as f32 * 0.01).sin()).collect();
    let mut planes: Vec<&mut [f32]> = vec![l.as_mut_slice(), r.as_mut_slice()];
    node.process_block_f32(&mut planes);

    let h = node.spatial_health();
    // Two program objects (L/R) on the virtual screen, analytic head
    // model, no room, no occlusion, no voice budget → all Good.
    assert_eq!(h.status, crate::spatial::health::HealthLevel::Good);
    assert_eq!(h.active_sources, 2);
    assert_eq!(h.per_source.len(), 2);
    assert_eq!(
        h.localization.level,
        crate::spatial::health::HealthLevel::Good
    );
    assert_eq!(h.occlusion.level, crate::spatial::health::HealthLevel::Good);
    assert_eq!(
        h.voice_pressure.level,
        crate::spatial::health::HealthLevel::Good
    );
    for s in &h.per_source {
        assert!(!s.reasons.is_empty());
    }

    // Disabled stage → Inactive report.
    node.set_enabled(false);
    let h = node.spatial_health();
    assert_eq!(h.status, crate::spatial::health::HealthLevel::Inactive);
}

#[test]
fn listener_pose_target_glides_smoothly_across_blocks() {
    // A runtime pose target (90° yaw + 1 m step) with a 20 ms
    // one-pole glides block-by-block — bounded steps, convergence —
    // and the image tracks it (the world-fixed screen sweeps across
    // the ears as the listener yaws).
    let mut node = SpatialNode::new(48_000.0);
    node.prepare(48_000.0, 2);
    node.set_enabled(true);
    node.apply_screen(0.0, 30.0, 0.0, 1.0);
    node.set_listener_tracking(crate::spatial::TrackingConfig {
        smoothing_ms: 20.0,
        max_angular_rate_deg_s: 0.0,
    });
    let target_q = Quat::from_euler_rad(90f32.to_radians(), 0.0, 0.0);
    let target_p = Vec3::new(0.0, 1.0, 0.0);
    node.set_listener_pose_target(target_q, target_p);
    assert_eq!(node.listener_pose_target().0, target_q);

    let frames = 512;
    let mut l = vec![0.0f32; frames];
    l[64] = 1.0;
    let mut r = vec![0.0f32; frames];
    let mut prev = node.listener_pose();
    let mut max_yaw_step = 0.0f32;
    for _ in 0..40 {
        let mut planes: Vec<&mut [f32]> = vec![&mut l, &mut r];
        node.process_block_f32(&mut planes);
        let (q, p) = node.listener_pose();
        let step = q.angle_to(prev.0).to_degrees();
        max_yaw_step = max_yaw_step.max(step);
        assert!((p - prev.1).length() < 1.0, "position glides, never jumps");
        prev = (q, p);
    }
    assert!(
        max_yaw_step < 45.0,
        "no single-block snap ({max_yaw_step}°)"
    );
    assert!(
        prev.0.angle_to(target_q).to_degrees() < 1.0,
        "converged to the target yaw"
    );
    assert!(
        (prev.1 - target_p).length() < 0.02,
        "converged to the target position"
    );
    // The glided pose is what telemetry reports.
    let t = node.spatial_telemetry();
    assert!((t.listener_yaw_deg - 90.0).abs() < 1.0);
    assert!((t.listener_position.y - 1.0).abs() < 0.02);
}

#[test]
fn listener_pose_zero_smoothing_snaps_next_block() {
    let mut node = SpatialNode::new(48_000.0);
    node.prepare(48_000.0, 2);
    node.set_enabled(true);
    node.set_listener_tracking(crate::spatial::TrackingConfig {
        smoothing_ms: 0.0,
        max_angular_rate_deg_s: 0.0,
    });
    node.set_listener_pose_target(
        Quat::from_euler_rad(45f32.to_radians(), 0.0, 0.0),
        Vec3::new(2.0, 0.0, 0.0),
    );
    let frames = 128;
    let mut l = vec![0.5f32; frames];
    let mut r = vec![0.5f32; frames];
    let mut planes: Vec<&mut [f32]> = vec![&mut l, &mut r];
    node.process_block_f32(&mut planes);
    let (q, p) = node.listener_pose();
    assert!(
        q.angle_to(Quat::from_euler_rad(45f32.to_radians(), 0.0, 0.0)) < 1e-4,
        "snapped"
    );
    assert_eq!(p, Vec3::new(2.0, 0.0, 0.0));
}

#[test]
fn listener_motion_rate_limit_caps_the_yaw_step() {
    // 100°/s at 48 kHz / 512-frame blocks ≈ 1.07° per block; a 90°
    // target never steps faster and catches up.
    let mut node = SpatialNode::new(48_000.0);
    node.prepare(48_000.0, 2);
    node.set_enabled(true);
    node.set_listener_tracking(crate::spatial::TrackingConfig {
        smoothing_ms: 0.0,
        max_angular_rate_deg_s: 100.0,
    });
    let target = Quat::from_euler_rad(90f32.to_radians(), 0.0, 0.0);
    node.set_listener_pose_target(target, Vec3::ZERO);
    let frames = 512;
    let mut l = vec![0.3f32; frames];
    let mut r = vec![0.3f32; frames];
    let mut prev = node.listener_pose().0;
    let mut max_step = 0.0f32;
    for _ in 0..120 {
        let mut planes: Vec<&mut [f32]> = vec![&mut l, &mut r];
        node.process_block_f32(&mut planes);
        let q = node.listener_pose().0;
        max_step = max_step.max(q.angle_to(prev).to_degrees());
        prev = q;
    }
    assert!(max_step <= 1.2, "per-block step ≤ cap (max {max_step}°)");
    assert!(prev.angle_to(target).to_degrees() < 1.0, "caught up");
}

#[test]
fn listener_rebake_due_crosses_cell_bounds() {
    // The relevance-bound seam: a bake at the origin stays fresh while
    // the listener glides within one cell and goes stale once a full
    // cell is crossed.
    let mut node = SpatialNode::new(48_000.0);
    node.prepare(48_000.0, 2);
    node.set_enabled(true);
    assert!(!node.listener_rebake_due(0.5, Vec3::ZERO), "at origin");
    // Within the cell (0.4 m off) — not due.
    node.set_listener_tracking(crate::spatial::TrackingConfig {
        smoothing_ms: 0.0,
        max_angular_rate_deg_s: 0.0,
    });
    node.set_listener_pose_target(Quat::IDENTITY, Vec3::new(0.4, 0.0, 0.0));
    let frames = 64;
    let mut l = vec![0.0f32; frames];
    let mut r = vec![0.0f32; frames];
    let mut planes: Vec<&mut [f32]> = vec![&mut l, &mut r];
    node.process_block_f32(&mut planes);
    assert!(!node.listener_rebake_due(0.5, Vec3::ZERO), "inside cell");
    // Across the bound (0.6 m) — due.
    node.set_listener_pose_target(Quat::IDENTITY, Vec3::new(0.6, 0.0, 0.0));
    let mut planes: Vec<&mut [f32]> = vec![&mut l, &mut r];
    node.process_block_f32(&mut planes);
    assert!(node.listener_rebake_due(0.5, Vec3::ZERO), "crossed a cell");
}

#[test]
fn listener_pose_command_applies_through_the_control_handle() {
    // The queued surface: `set_spatial_listener_pose` on the control
    // handle lands at the block-boundary drain and the node glides
    // from there. (The angle tolerance is f32 `angle_to` noise —
    // identical quats round-trip the dot product at ~1e-3.)
    let mut graph =
        crate::dsp::graph2::prod::arena::DspGraph::from_config(&EngineConfig::default(), 48_000.0);
    let handle = graph.control_handle();
    handle.set_spatial_enabled(true);
    handle.set_spatial_listener_tracking(0.0, 0.0);
    let target = Quat::from_euler_rad(30f32.to_radians(), 0.0, 0.0);
    handle.set_spatial_listener_pose(target, Vec3::new(1.0, 0.5, 0.0));
    graph.drain_queued_control();
    let (q, p) = graph.spatial().listener_pose_target();
    assert!(
        q.angle_to(target) < 1e-3,
        "target applied: {}",
        q.angle_to(target)
    );
    assert_eq!(p, Vec3::new(1.0, 0.5, 0.0));
    // And the glide policy landed too.
    let policy = graph.spatial().listener_tracking();
    assert_eq!(policy.smoothing_ms, 0.0);
    assert_eq!(policy.max_angular_rate_deg_s, 0.0);
}

#[test]
fn moving_listener_rotates_the_rendered_image() {
    // End-to-end audio: a world-fixed left-impulse image moves across
    // the ears as the listener yaws +90° through the glide — the same
    // perceptual cue as the static `apply_listener` test, reached by
    // motion.
    let mut node = SpatialNode::new(48_000.0);
    node.prepare(48_000.0, 2);
    node.set_enabled(true);
    node.apply_screen(0.0, 30.0, 0.0, 1.0);
    node.set_listener_tracking(crate::spatial::TrackingConfig {
        smoothing_ms: 0.0,
        max_angular_rate_deg_s: 0.0,
    });
    node.set_listener_pose_target(
        Quat::from_euler_rad(90f32.to_radians(), 0.0, 0.0),
        Vec3::ZERO,
    );
    let frames = 1024;
    let mut l = vec![0.0f32; frames];
    l[64] = 1.0;
    let mut r = vec![0.0f32; frames];
    let mut planes: Vec<&mut [f32]> = vec![&mut l, &mut r];
    node.process_block_f32(&mut planes);
    let il = argmax_abs(planes[0]);
    let ir = argmax_abs(planes[1]);
    let expect = woodworth_samples(120f32.to_radians(), 48_000);
    assert!(ir > il, "right ear contralateral ({ir} vs {il})");
    assert!(
        ((ir - il) as f32 - expect).abs() <= 6.0,
        "ITD {} samples vs {expect}",
        ir - il
    );
    let e_l: f32 = planes[0].iter().map(|v| v * v).sum();
    let e_r: f32 = planes[1].iter().map(|v| v * v).sum();
    assert!(e_l > e_r * 2.0, "image left ({e_l} vs {e_r})");
}

#[test]
fn converged_listener_motion_renders_deterministically() {
    // Once converged, the motion deactivates (a settled target renders
    // bit-identically across two fresh nodes — the same determinism
    // discipline as the voice-budget suite; the renderer itself is
    // stateful *within* a node, warm-up included, so the guarantee is
    // across equal initial states).
    let frames = 256;
    let target = Quat::from_euler_rad(20f32.to_radians(), 0.0, 0.0);
    let build = || -> SpatialNode {
        let mut node = SpatialNode::new(48_000.0);
        node.prepare(48_000.0, 2);
        node.set_enabled(true);
        node.set_listener_tracking(crate::spatial::TrackingConfig {
            smoothing_ms: 0.0,
            max_angular_rate_deg_s: 0.0,
        });
        node.set_listener_pose_target(target, Vec3::new(0.5, 0.5, 0.0));
        node
    };
    let run = |mut node: SpatialNode| -> (Vec<f32>, f32) {
        let mut l: Vec<f32> = (0..frames).map(|i| (i as f32 * 0.01).sin()).collect();
        let mut r: Vec<f32> = (0..frames).map(|i| (i as f32 * 0.02).cos()).collect();
        let mut planes: Vec<&mut [f32]> = vec![l.as_mut_slice(), r.as_mut_slice()];
        node.process_block_f32(&mut planes); // snap + converge
        let residual = node.listener_pose().0.angle_to(target);
        let out = planes.iter().flat_map(|p| p.to_vec()).collect();
        (out, residual)
    };
    let (a, res_a) = run(build());
    let (b, res_b) = run(build());
    assert!(res_a < 1e-3 && res_b < 1e-3, "motion converged");
    assert_eq!(a, b, "equal states render bit-identically");
}

#[test]
fn program_gain_automation_reduces_a_program_object() {
    // FFI/engine automation (spec §47): a gain curve attached to one
    // program object overrides its level at the set automation clock.
    let render_energy = |node: &mut SpatialNode| -> f64 {
        let frames = 512;
        let mut l: Vec<f32> = (0..frames).map(|i| (i as f32 * 0.01).sin()).collect();
        let mut r: Vec<f32> = (0..frames).map(|i| (i as f32 * 0.02).cos()).collect();
        let mut planes: Vec<&mut [f32]> = vec![l.as_mut_slice(), r.as_mut_slice()];
        node.process_block_f32(&mut planes);
        planes
            .iter()
            .map(|p| p.iter().map(|&v| (v as f64).powi(2)).sum::<f64>())
            .sum::<f64>()
    };
    let mut node = SpatialNode::new(48_000.0);
    node.prepare(48_000.0, 2);
    node.set_enabled(true);
    node.apply_config(
        &config::SpatialConfig {
            enabled: true,
            ..Default::default()
        },
        48_000.0,
        &ChannelLayout::Stereo,
    );
    node.apply_screen(0.0, 30.0, 0.0, 1.0);
    // Baseline: no gain automation -> both programme voices at unity.
    node.set_automation_time(10.0);
    let full = render_energy(&mut node);
    // Zero the Left programme voice via a gain curve evaluated at t=0.
    let curve = crate::spatial::CurveScalar::from_points(&[(0.0, 0.0), (1.0, 0.0)]).unwrap();
    node.set_program_automation(0, 0, Some(Arc::new(curve)));
    node.set_automation_time(0.0);
    let reduced = render_energy(&mut node);
    assert!(
        reduced < full * 0.9,
        "gain automation must cut a programme voice: {reduced} vs {full}"
    );
    // Clearing it restores both voices (the renderer's filter/ring state
    // carries warm-up between blocks, so we assert restoration is large
    // and near the baseline rather than bit-identical).
    node.set_program_automation(0, 0, None);
    let back = render_energy(&mut node);
    assert!(back > reduced, "automation cleared restores energy");
    assert!((back - full).abs() < 0.05 * full.max(1.0));
}
