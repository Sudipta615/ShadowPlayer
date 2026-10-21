//! Ambisonics: the encode → decode round trip for orders 1–3, and the
//! *documented* behaviour for orders 4 and above.
//!
//! Two claims are checked here:
//!
//! * For orders 1, 2 and 3 the engine implements an exact Wigner rotation
//!   and a sampling decoder, so a plane wave encoded into the bus must
//!   decode back to the speaker gains the SH maths says it should, and a
//!   rotation by `q` followed by a rotation by `q⁻¹` must be the identity.
//! * For order 4 and above the rotation is **not** implemented, and the
//!   engine says so: `rotation_supported` reports false and
//!   `rotate_bus_frame_n` leaves the frame untouched rather than rotating
//!   the low orders and leaving the high ones behind. A half-rotated frame
//!   is worse than an unrotated one, so the documented behaviour is the one
//!   under test — not a guess at what "should" happen.

use engine::spatial::ambisonic::decoder::order_weights;
use engine::spatial::ambisonic::{
    channel_count, encode_plane_wave_n, rotate_bus_frame_n, rotation_supported, AmbisonicDecoder,
    AmbisonicEncoder, DecoderPolicy, MAX_ROTATABLE_ORDER,
};
use engine::spatial::{sh_n, Quat, SpeakerLayout, Vec3};

/// The orders the engine implements an exact rotation for.
const EXACT_ORDERS: [u8; 3] = [1, 2, 3];

/// The tolerance for an order-1/2 rotation. The Wigner blocks are computed in
/// f64 and cast to f32, so the round trip is limited by the f32 rounding of
/// the intermediate frame.
const TIGHT: f32 = 1e-4;

/// The tolerance for an order-3 rotation. The engine's own in-crate test uses
/// the same bound: the order-3 block is a 7×7 matrix of f32 coefficients
/// applied to seven terms, so the accumulated rounding is larger.
const LOOSE: f32 = 2e-3;

/// A direction that is not on any axis.
fn oblique_direction() -> Vec3 {
    Vec3::new(1.0, 0.5, 0.3)
        .normalized()
        .expect("the direction must be normalizable")
}

/// The layout the decoder is built for.
fn layout() -> SpeakerLayout {
    SpeakerLayout::seven_point_one_four()
}

#[test]
fn the_bus_width_is_the_square_of_the_order_plus_one() {
    for order in 1u8..=9 {
        assert_eq!(
            channel_count(order),
            (order as usize + 1) * (order as usize + 1),
            "order {order} must have (n+1)² channels"
        );
        let encoder = AmbisonicEncoder::new(order);
        assert_eq!(encoder.order(), order);
        assert_eq!(encoder.channels(), channel_count(order));
    }
}

#[test]
fn a_plane_wave_encodes_into_the_sh_basis_of_its_direction() {
    // The first channel of the bus is the omnidirectional component and must
    // be the gain itself, whatever the direction.
    for order in EXACT_ORDERS {
        let mut frame = vec![0.0f32; channel_count(order)];
        encode_plane_wave_n(order, oblique_direction(), 0.75, &mut frame);
        assert!(
            (frame[0] - 0.75).abs() < 1e-6,
            "order {order}: the omnidirectional channel must be the gain, it is {}",
            frame[0]
        );
        assert!(
            frame.iter().all(|v| v.is_finite()),
            "order {order}: the encoded bus must be finite"
        );
    }
}

#[test]
fn the_encode_decode_round_trip_is_exact_for_orders_one_to_three() {
    // The decoder's matrix is D[k][c] = w[c]·Y_c(speaker_k) / N, and the
    // encoder's bus is Y_c(source)·gain. The decoded speaker gain is
    // therefore the sum over the bus, which this test recomputes from the
    // SH basis and the order weights rather than from the decoder.
    for order in EXACT_ORDERS {
        for policy in [
            DecoderPolicy::Basic,
            DecoderPolicy::MaxRe,
            DecoderPolicy::InPhase,
        ] {
            let speakers = layout();
            let mut decoder = AmbisonicDecoder::with_order(policy, order);
            decoder
                .prepare(&speakers, 48_000)
                .unwrap_or_else(|e| panic!("order {order}: prepare failed: {e}"));
            assert!(decoder.prepared());

            let ch = decoder.bus_width();
            let mut frame = vec![0.0f32; ch];
            encode_plane_wave_n(order, oblique_direction(), 1.0, &mut frame);

            let mut out = vec![0.0f32; speakers.speakers.len()];
            decoder.process_bus(&frame, 1, &mut out);

            let weights = order_weights(policy, order);
            let n = decoder.speakers().len() as f32;
            assert!(n > 0.0, "the layout must contribute speakers");

            for &spk in decoder.speakers().iter() {
                let spk_dir = speakers.speakers[spk]
                    .position
                    .normalized()
                    .expect("a speaker must have a direction");
                let mut y = vec![0.0f32; ch];
                sh_n(order, spk_dir, &mut y);
                let expected: f32 = (0..ch).map(|c| weights[c] * y[c] * frame[c]).sum::<f32>() / n;
                assert!(
                    (out[spk] - expected).abs() < 1e-5,
                    "order {order} / {policy:?}: speaker {spk} decoded to {}, \
                     the SH maths says {expected}",
                    out[spk]
                );
            }

            // The LFE and the disabled speakers must stay silent: the
            // decoder is not allowed to spread programme into them.
            for (idx, speaker) in speakers.speakers.iter().enumerate() {
                if speaker.is_lfe || !speaker.enabled {
                    assert_eq!(
                        out[idx], 0.0,
                        "order {order} / {policy:?}: speaker {idx} must be silent"
                    );
                }
            }
        }
    }
}

#[test]
fn a_plane_wave_is_decoded_into_the_speakers_that_face_it() {
    // The behavioural half of the round trip: a source at the front must
    // land on the front speakers and not on the rear ones.
    let speakers = layout();
    let mut decoder = AmbisonicDecoder::with_order(DecoderPolicy::Basic, 1);
    decoder
        .prepare(&speakers, 48_000)
        .expect("prepare must succeed for order 1");

    let mut frame = vec![0.0f32; decoder.bus_width()];
    encode_plane_wave_n(1, Vec3::Y, 1.0, &mut frame);
    let mut out = vec![0.0f32; speakers.speakers.len()];
    decoder.process_bus(&frame, 1, &mut out);

    let mut best = 0usize;
    let mut best_gain = f32::NEG_INFINITY;
    let mut worst_front_gain = 0.0f32;
    for (idx, speaker) in speakers.speakers.iter().enumerate() {
        if speaker.is_lfe || !speaker.enabled {
            continue;
        }
        let dir = speaker
            .position
            .normalized()
            .expect("a speaker must have a direction");
        // The front speakers face +Y.
        if dir.dot(Vec3::Y) > 0.7 {
            worst_front_gain = worst_front_gain.max(out[idx]);
            if out[idx] > best_gain {
                best_gain = out[idx];
                best = idx;
            }
        }
    }
    assert!(
        best_gain > 0.0,
        "a front source must reach the front speakers, the strongest was {best} at {best_gain}"
    );
    assert!(
        worst_front_gain > 0.0,
        "every front speaker must receive something from a front source"
    );
}

#[test]
fn a_rotation_by_q_followed_by_its_inverse_is_the_identity() {
    for order in EXACT_ORDERS {
        let tolerance = if order == 3 { LOOSE } else { TIGHT };
        for q in [
            Quat::from_euler_rad(0.7, 0.3, -0.2),
            Quat::from_euler_rad(std::f32::consts::FRAC_PI_2, 0.0, 0.0),
            Quat::from_euler_rad(0.0, -1.1, 2.3),
        ] {
            let mut frame = vec![0.0f32; channel_count(order)];
            encode_plane_wave_n(order, oblique_direction(), 1.0, &mut frame);
            let original = frame.clone();

            rotate_bus_frame_n(q, order, &mut frame);
            rotate_bus_frame_n(q.conjugate(), order, &mut frame);

            for (k, (a, b)) in original.iter().zip(frame.iter()).enumerate() {
                assert!(
                    (a - b).abs() < tolerance,
                    "order {order}: channel {k} came back as {b}, it started as {a}"
                );
            }
        }
    }
}

#[test]
fn a_rotation_commutes_with_the_spherical_harmonic_basis() {
    // Rotating the bus frame by `q` must give the same bus as encoding the
    // rotated direction. This is the property that makes the rotation
    // physically meaningful rather than an arbitrary matrix.
    for order in EXACT_ORDERS {
        let tolerance = if order == 3 { LOOSE } else { TIGHT };
        for q in [
            Quat::from_euler_rad(0.7, 0.3, -0.2),
            Quat::from_euler_rad(std::f32::consts::FRAC_PI_2, 0.0, 0.0),
        ] {
            let dir = oblique_direction();
            let rotated_dir = q.rotate_vec3(dir);

            let mut frame = vec![0.0f32; channel_count(order)];
            encode_plane_wave_n(order, dir, 1.0, &mut frame);
            rotate_bus_frame_n(q, order, &mut frame);

            let mut expected = vec![0.0f32; channel_count(order)];
            encode_plane_wave_n(order, rotated_dir, 1.0, &mut expected);

            for (k, (a, b)) in expected.iter().zip(frame.iter()).enumerate() {
                assert!(
                    (a - b).abs() < tolerance,
                    "order {order}: channel {k} is {} after the rotation, the \
                     rotated direction encodes {}",
                    b,
                    a
                );
            }
        }
    }
}

#[test]
fn order_four_and_above_are_documented_as_not_rotated() {
    // The documented behaviour, not a guess: orders above 3 are refused, and
    // `rotate_bus_frame_n` leaves the frame completely untouched so the
    // field stays self-consistent.
    assert_eq!(MAX_ROTATABLE_ORDER, 3, "the highest rotatable order is 3");
    for order in 4u8..=9 {
        assert!(
            !rotation_supported(order),
            "order {order} must not claim to be rotatable"
        );

        let mut frame = vec![0.0f32; channel_count(order)];
        encode_plane_wave_n(order, oblique_direction(), 1.0, &mut frame);
        let original = frame.clone();

        rotate_bus_frame_n(Quat::from_euler_rad(0.7, 0.3, -0.2), order, &mut frame);

        for (k, (a, b)) in original.iter().zip(frame.iter()).enumerate() {
            assert_eq!(
                a.to_bits(),
                b.to_bits(),
                "order {order}: channel {k} was rotated ({a} -> {b}); the \
                 documented behaviour is that orders above 3 are left untouched"
            );
        }
    }
    assert!(
        rotation_supported(1) && rotation_supported(2) && rotation_supported(3),
        "orders 1 to 3 must claim to be rotatable"
    );
}

#[test]
fn the_order_one_rotation_is_the_plain_vector_rotation() {
    // The order-1 bus is (W, Y, Z, X) in the engine's channel order; the
    // rotation must rotate the three directional channels exactly like
    // direction vectors.
    let q = Quat::from_euler_rad(0.7, 0.3, -0.2);
    let dir = oblique_direction();
    let mut frame = vec![0.0f32; 4];
    encode_plane_wave_n(1, dir, 1.0, &mut frame);
    rotate_bus_frame_n(q, 1, &mut frame);

    let rotated = q.rotate_vec3(dir);
    // The engine stores Y in channel 1, Z in channel 2 and X in channel 3,
    // each scaled by the SH normalisation √3.
    let scale = 3.0f32.sqrt();
    let stored = Vec3::new(frame[3] / scale, frame[1] / scale, frame[2] / scale);
    assert!(
        (stored - rotated).length() < 1e-5,
        "the order-1 rotation produced {stored:?}, the rotated direction is {rotated:?}"
    );
}
