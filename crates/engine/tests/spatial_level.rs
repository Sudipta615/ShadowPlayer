//! Spatial rendering: the renderer must not change the programme's level, and
//! the room must be a room — symmetric, with an audible late field.
//!
//! These are the three properties a listener notices immediately, and each of
//! them was broken in a way that no "is the output finite" test could see:
//!
//! * panning the two programme planes into one pair of ears summed at full
//!   amplitude, so switching Spatial on lifted the mix by ≈4 dB and drove the
//!   limiter (a level jump, not a spatial effect);
//! * reflection images carried no relative distance loss, so a 2nd-order room
//!   added up to +9 dB of early energy with a −0.7 dB left/right imbalance;
//! * the diffuse (late-field) planes were written into the output with a
//!   stride derived from the scratch buffer's *capacity*, which is not its
//!   channel count — the room's tail and every ambisonic field were silently
//!   missing from the output.

use engine::spatial::{BinauralRenderer, SpatialRenderer, SpatialScene, SpeakerLayout, Vec3};

const SR: u32 = 48_000;
const FRAMES: usize = 256;

/// A white-noise programme (deterministic), the same sample on both planes —
/// the worst case for coherent summing and therefore the level test.
struct Noise(u32);

impl Noise {
    fn new() -> Self {
        Self(0x1234_5678)
    }
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        ((self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0) * 0.2
    }
}

fn rms(v: &[f32]) -> f32 {
    (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt()
}

fn db(level: f32, reference: f32) -> f32 {
    20.0 * (level / reference).log10()
}

/// The player's default room, and the listening position `SpatialNode` puts
/// the listener at: the room's centre. The room's origin is a corner, so a
/// listener at the origin would stand *in* two walls and its reflections would
/// arrive at zero delay.
const ROOM: (f32, f32, f32) = (12.0, 10.0, 3.0);
fn listening_position() -> Vec3 {
    Vec3::new(ROOM.0 * 0.5, ROOM.1 * 0.5, ROOM.2 * 0.5)
}

/// A scene with the player's two programme objects at ±`half_deg`, 2 m in front
/// of the listener.
fn player_scene(half_deg: f32) -> SpatialScene {
    let mut scene = SpatialScene::new(SR);
    scene.listener.position = listening_position();
    let base = scene.listener.position;
    for sign in [-1.0f32, 1.0] {
        let az = sign * half_deg.to_radians();
        let id = scene
            .create_audio_object(Vec3::new(
                base.x + az.sin() * 2.0,
                base.y + az.cos() * 2.0,
                base.z,
            ))
            .expect("two programme objects must fit in the scene");
        let obj = scene.object_mut(id).expect("object just created");
        obj.gain = 1.0;
        obj.room_send = 0.0;
        // What `SpatialNode` does for its two programme objects: the screen
        // radius (2 m) is a rendering convention, not a distance the listener
        // should be charged for by the inverse-reference model.
        obj.distance_model = engine::spatial::DistanceModel::Linear;
    }
    scene
}

/// Render `blocks` blocks of noise through `renderer`, returning the joined
/// stereo output of the last `tail_blocks`.
fn render(
    renderer: &mut BinauralRenderer,
    scene: &SpatialScene,
    blocks: usize,
    tail_blocks: usize,
) -> (Vec<f32>, Vec<f32>) {
    renderer
        .prepare(&SpeakerLayout::stereo(), SR)
        .expect("stereo layout must prepare");
    let mut noise = Noise::new();
    let (mut left, mut right) = (Vec::new(), Vec::new());
    for blk in 0..blocks {
        let mut l = vec![0.0f32; FRAMES];
        let mut r = vec![0.0f32; FRAMES];
        for i in 0..FRAMES {
            let s = noise.next();
            l[i] = s;
            r[i] = s;
        }
        let mut out = vec![0.0f32; 2 * FRAMES];
        let refs = [l.as_slice(), r.as_slice()];
        renderer
            .process_block(scene, &refs, FRAMES, &mut out)
            .expect("rendering must succeed");
        if blk + tail_blocks >= blocks {
            left.extend(out.iter().step_by(2));
            right.extend(out.iter().skip(1).step_by(2));
        }
    }
    (left, right)
}

#[test]
fn enabling_spatial_keeps_the_programme_level() {
    let mut scene = player_scene(30.0);
    let mut renderer = BinauralRenderer::new(10.0);
    let (l, r) = render(&mut renderer, &scene, 120, 20);
    // The same noise generator, bypassed: 0.2 peak uniform noise → 0.1155 rms.
    let input = 0.2 / 3f32.sqrt();
    let level = db(rms(&l), input);
    assert!(
        level.abs() < 1.5,
        "spatial changed the programme level by {level:+.1} dB \
         (left {:.4}, input {input:.4})",
        rms(&l)
    );
    assert!(
        (rms(&l) - rms(&r)).abs() / rms(&l) < 0.02,
        "a centred scene must stay level-balanced"
    );

    // And the room must not undo it: the reflection path is normalized the
    // same way as the direct path.
    scene.room.enabled = true;
    scene.room.reflection_order = 1;
    scene.room.late_mix = 0.8;
    for id in [engine::spatial::ObjectId(0), engine::spatial::ObjectId(1)] {
        if let Some(obj) = scene.object_mut(id) {
            obj.room_send = 0.3;
        }
    }
    let mut renderer = BinauralRenderer::new(10.0);
    let (l, r) = render(&mut renderer, &scene, 120, 20);
    let imbalance = db(rms(&r), rms(&l));
    assert!(
        imbalance.abs() < 0.3,
        "the room made the left/right balance asymmetric by {imbalance:+.2} dB"
    );
}

#[test]
fn the_room_tail_actually_reaches_the_output() {
    // The late field is a diffuse plane decoded onto the virtual ring. If the
    // stride is wrong it lands nowhere and `late_mix` has no effect at all —
    // which is exactly what the RT60 / late-mix controls used to do.
    let mut quiet = player_scene(30.0);
    quiet.room.enabled = true;
    quiet.room.reflection_order = 1;
    quiet.room.late_mix = 0.0;
    let mut loud = player_scene(30.0);
    loud.room.enabled = true;
    loud.room.reflection_order = 1;
    loud.room.late_mix = 1.0;
    for scene in [&mut quiet, &mut loud] {
        for id in [engine::spatial::ObjectId(0), engine::spatial::ObjectId(1)] {
            if let Some(obj) = scene.object_mut(id) {
                obj.room_send = 0.4;
            }
        }
    }

    let mut renderer = BinauralRenderer::new(10.0);
    let (quiet_l, _) = render(&mut renderer, &quiet, 200, 50);
    let mut renderer = BinauralRenderer::new(10.0);
    let (loud_l, _) = render(&mut renderer, &loud, 200, 50);

    let delta = db(rms(&loud_l), rms(&quiet_l));
    // The tail is a diffuse plane, so it is quieter than the send by design
    // (the FDN normalises for a stable decay). What matters here is that it
    // is *there*: the broken stride produced exactly 0.00 dB, and only the
    // first frame of every block was ever written.
    assert!(
        delta > 0.2,
        "the late field is inaudible: late_mix 0 → 1 changed the output by {delta:+.2} dB"
    );
}

#[test]
fn a_second_order_room_does_not_explode_the_level() {
    let mut bypass = player_scene(30.0);
    bypass.room.enabled = false;
    let mut room = player_scene(30.0);
    room.room.enabled = true;
    room.room.width = ROOM.0;
    room.room.depth = ROOM.1;
    room.room.height = ROOM.2;
    room.room.reflection_order = 2;
    room.room.late_mix = 0.8;
    for id in [engine::spatial::ObjectId(0), engine::spatial::ObjectId(1)] {
        if let Some(obj) = room.object_mut(id) {
            obj.room_send = 0.3;
        }
    }
    let mut renderer = BinauralRenderer::new(10.0);
    let (dry, _) = render(&mut renderer, &bypass, 200, 50);
    let mut renderer = BinauralRenderer::new(10.0);
    let (wet, _) = render(&mut renderer, &room, 200, 50);
    let delta = db(rms(&wet), rms(&dry));
    // 30 % wet with the room's own absorption: a lift, but a room-sized one.
    // The unattenuated image sources summed to +8.8 dB with a channel skew;
    // with the reflection energy normalized and the listener actually inside
    // the room it is under a dB.
    assert!(
        (-0.5..2.0).contains(&delta),
        "a 30 % wet 2nd-order room shifted the level by {delta:+.1} dB"
    );
}

/// `wet` is a wet/dry mix, not a level control. Sweeping it must move the
/// room's level monotonically and gently: 30 % (the player's default send)
/// is level-neutral, and even a fully wet 2nd-order room stays within a few
/// dB. Before the reflection-energy normalization this was +4.4 dB at 30 %
/// and +10 dB at full wet.
#[test]
fn the_wet_control_is_a_mix_and_not_a_level_boost() {
    let mut levels = Vec::new();
    for wet in [0.0f32, 0.3, 0.6, 1.0] {
        let mut scene = player_scene(30.0);
        scene.room.enabled = true;
        scene.room.width = ROOM.0;
        scene.room.depth = ROOM.1;
        scene.room.height = ROOM.2;
        scene.room.reflection_order = 2;
        scene.room.late_mix = 0.8;
        for id in [engine::spatial::ObjectId(0), engine::spatial::ObjectId(1)] {
            if let Some(obj) = scene.object_mut(id) {
                obj.room_send = wet;
            }
        }
        let mut renderer = BinauralRenderer::new(10.0);
        let (l, _) = render(&mut renderer, &scene, 200, 50);
        levels.push(db(rms(&l), 0.2 / 3f32.sqrt()));
    }
    assert!(
        levels.windows(2).all(|w| w[0] <= w[1] + 0.2),
        "more wet must not be quieter: {levels:?}"
    );
    assert!(
        levels[1].abs() < 1.5,
        "the default 30 % send shifted the level by {:+.1} dB",
        levels[1]
    );
    assert!(
        levels[3] < 5.0,
        "a fully wet room lifted the level by {:+.1} dB",
        levels[3]
    );
}

/// Absorption has to remain audible: the normalization divides out the room's
/// geometry, not its walls.
#[test]
fn a_padded_room_is_quieter_than_a_live_one() {
    let level = |absorption: f32| {
        let mut scene = player_scene(30.0);
        scene.room.enabled = true;
        scene.room.width = ROOM.0;
        scene.room.depth = ROOM.1;
        scene.room.height = ROOM.2;
        scene.room.reflection_order = 2;
        scene.room.late_mix = 0.8;
        scene.room.absorption = absorption;
        for id in [engine::spatial::ObjectId(0), engine::spatial::ObjectId(1)] {
            if let Some(obj) = scene.object_mut(id) {
                obj.room_send = 1.0;
            }
        }
        let mut renderer = BinauralRenderer::new(10.0);
        let (l, _) = render(&mut renderer, &scene, 200, 50);
        db(rms(&l), 0.2 / 3f32.sqrt())
    };
    let (live, padded) = (level(0.2), level(0.8));
    assert!(
        padded < live - 0.5,
        "a padded room ({padded:+.1} dB) is not quieter than a live one ({live:+.1} dB)"
    );
}
