//! Multichannel upmix/downmixing.
//!
//! Two layers, deliberately kept side by side because they answer different
//! questions:
//!
//!   * **Template mixing** ([`mix_interleaved_with_template`],
//!     [`mix_interleaved_to_stereo_with_template`]) applies a *configured*
//!     matrix, built from a [`config::ChannelMixTemplate`] and the source and
//!     target [`ChannelLayout`]s. This is the user-facing channel routing, and
//!     it can upmix as well as downmix.
//!   * **The BS.775 downmix** ([`downmix_interleaved_to_stereo`]) is a fixed
//!     fallback used when no template is configured. It predates the template
//!     layer and is what the decode path calls by default, so it is not
//!     expressed as a template: its per-frame behaviour is fixed by
//!     ITU-R BS.775 and making it configurable would change what "default"
//!     means for every existing user.
//!
//! They were separate modules until v1.0.0 (`symphonia_decoder/downmix.rs` and
//! `channel_mix.rs`), which meant the two halves of the same problem had to be
//! found in different places — and the BS.775 one was buried under a decoder
//! implementation despite having nothing to do with decoding.

use config;
use log;

use crate::buffer::MAX_CHANNELS;
use crate::decode::channel_layout::{ChannelId, ChannelLayout};

fn set_mix_gain(
    matrix: &mut [[f32; MAX_CHANNELS]; MAX_CHANNELS],
    source: Option<usize>,
    destination: Option<usize>,
    gain: f32,
) {
    if let (Some(src), Some(dst)) = (source, destination) {
        if src < MAX_CHANNELS && dst < MAX_CHANNELS {
            matrix[src][dst] += gain;
        }
    }
}

fn role_identity(
    source_layout: &ChannelLayout,
    target_layout: &ChannelLayout,
    matrix: &mut [[f32; MAX_CHANNELS]; MAX_CHANNELS],
) {
    for id in source_layout.channel_ids() {
        set_mix_gain(
            matrix,
            source_layout.position_of(id),
            target_layout.position_of(id),
            1.0,
        );
    }
}

/// Build the fixed matrix for a named template. Matrix orientation is
/// `[source][destination]`, matching `ChannelRoutingConfig`.
fn build_mix_matrix(
    source_layout: &ChannelLayout,
    target_layout: &ChannelLayout,
    template: &config::ChannelMixTemplate,
) -> [[f32; MAX_CHANNELS]; MAX_CHANNELS] {
    let mut matrix = [[0.0f32; MAX_CHANNELS]; MAX_CHANNELS];
    let source_count = source_layout.channel_count();
    let target_count = target_layout.channel_count();

    match template {
        config::ChannelMixTemplate::Custom(custom) => {
            if custom.len() == source_count
                && target_count <= MAX_CHANNELS
                && custom.iter().all(|row| row.len() == target_count)
            {
                for src in 0..source_count.min(MAX_CHANNELS) {
                    for dst in 0..target_count.min(MAX_CHANNELS) {
                        matrix[src][dst] = custom[src][dst];
                    }
                }
            } else {
                log::warn!(
                    "ChannelMix custom matrix shape does not match {}x{}; using semantic identity",
                    source_count,
                    target_count
                );
                role_identity(source_layout, target_layout, &mut matrix);
            }
        }
        config::ChannelMixTemplate::StereoToFiveOne
        | config::ChannelMixTemplate::StereoToSevenOne
        | config::ChannelMixTemplate::StereoToSevenPointOneFour
            if source_count == 2 =>
        {
            let fl = source_layout.position_of(ChannelId::FrontLeft).or(Some(0));
            let fr = source_layout.position_of(ChannelId::FrontRight).or(Some(1));
            set_mix_gain(
                &mut matrix,
                fl,
                target_layout.position_of(ChannelId::FrontLeft),
                1.0,
            );
            set_mix_gain(
                &mut matrix,
                fr,
                target_layout.position_of(ChannelId::FrontRight),
                1.0,
            );
            set_mix_gain(
                &mut matrix,
                fl,
                target_layout.position_of(ChannelId::Center),
                std::f32::consts::FRAC_1_SQRT_2,
            );
            set_mix_gain(
                &mut matrix,
                fr,
                target_layout.position_of(ChannelId::Center),
                std::f32::consts::FRAC_1_SQRT_2,
            );
            // Conservative decorrelated-free fill: surrounds/rears receive a
            // half-level copy, while LFE remains silent by design.
            for (src, dst) in [
                (fl, ChannelId::SideLeft),
                (fl, ChannelId::RearLeft),
                (fr, ChannelId::SideRight),
                (fr, ChannelId::RearRight),
            ] {
                set_mix_gain(&mut matrix, src, target_layout.position_of(dst), 0.5);
            }
            if matches!(
                template,
                config::ChannelMixTemplate::StereoToSevenPointOneFour
            ) {
                for (src, dst) in [
                    (fl, ChannelId::TopFrontLeft),
                    (fl, ChannelId::TopRearLeft),
                    (fr, ChannelId::TopFrontRight),
                    (fr, ChannelId::TopRearRight),
                ] {
                    set_mix_gain(
                        &mut matrix,
                        src,
                        target_layout.position_of(dst),
                        0.353_553_4,
                    );
                }
            }
        }
        config::ChannelMixTemplate::FiveOneToStereo
        | config::ChannelMixTemplate::SevenOneToStereo
        | config::ChannelMixTemplate::SevenPointOneFourToStereo
        | config::ChannelMixTemplate::ItuBs775
            if target_count == 2 =>
        {
            // ITU-R BS.775-compatible fold. The named templates deliberately
            // share this conservative matrix; the 7.1.4 variant additionally
            // folds overhead content at a lower level.
            set_mix_gain(
                &mut matrix,
                source_layout.position_of(ChannelId::FrontLeft),
                target_layout.position_of(ChannelId::FrontLeft),
                1.0,
            );
            set_mix_gain(
                &mut matrix,
                source_layout.position_of(ChannelId::FrontRight),
                target_layout.position_of(ChannelId::FrontRight),
                1.0,
            );
            // Center and back-center are shared; lateral/rear speakers stay
            // on their corresponding side so a left-only surround impulse
            // cannot leak into the right output.
            for (id, gain) in [
                (ChannelId::Center, std::f32::consts::FRAC_1_SQRT_2),
                (ChannelId::BackCenter, 0.5),
            ] {
                set_mix_gain(
                    &mut matrix,
                    source_layout.position_of(id),
                    target_layout.position_of(ChannelId::FrontLeft),
                    gain,
                );
                set_mix_gain(
                    &mut matrix,
                    source_layout.position_of(id),
                    target_layout.position_of(ChannelId::FrontRight),
                    gain,
                );
            }
            for (id, destination, gain) in [
                (
                    ChannelId::SideLeft,
                    ChannelId::FrontLeft,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
                (
                    ChannelId::SideRight,
                    ChannelId::FrontRight,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
                (ChannelId::RearLeft, ChannelId::FrontLeft, 0.5),
                (ChannelId::RearRight, ChannelId::FrontRight, 0.5),
            ] {
                set_mix_gain(
                    &mut matrix,
                    source_layout.position_of(id),
                    target_layout.position_of(destination),
                    gain,
                );
            }
            // Fold height and wide channels into FrontLeft and FrontRight
            // per ITU-R BS.2051-3 when present in the source:
            for (id, destination, gain) in [
                (ChannelId::TopFrontLeft, ChannelId::FrontLeft, 0.353_553_4),
                (ChannelId::TopRearLeft, ChannelId::FrontLeft, 0.353_553_4),
                (ChannelId::TopSideLeft, ChannelId::FrontLeft, 0.353_553_4),
                (ChannelId::TopFrontRight, ChannelId::FrontRight, 0.353_553_4),
                (ChannelId::TopRearRight, ChannelId::FrontRight, 0.353_553_4),
                (ChannelId::TopSideRight, ChannelId::FrontRight, 0.353_553_4),
                (
                    ChannelId::WideLeft,
                    ChannelId::FrontLeft,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
                (
                    ChannelId::WideRight,
                    ChannelId::FrontRight,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
            ] {
                set_mix_gain(
                    &mut matrix,
                    source_layout.position_of(id),
                    target_layout.position_of(destination),
                    gain,
                );
            }
            // LFE is never silently mixed into mains.
        }
        config::ChannelMixTemplate::ItuBs775 if target_count == 8 => {
            // Target is 7.1: match roles that exist in both, and fold
            // height and wide channels down into the horizontal bed.
            role_identity(source_layout, target_layout, &mut matrix);
            for (id, destination, gain) in [
                (
                    ChannelId::TopFrontLeft,
                    ChannelId::FrontLeft,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
                (
                    ChannelId::TopFrontRight,
                    ChannelId::FrontRight,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
                (
                    ChannelId::TopRearLeft,
                    ChannelId::RearLeft,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
                (
                    ChannelId::TopRearRight,
                    ChannelId::RearRight,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
                (
                    ChannelId::TopSideLeft,
                    ChannelId::SideLeft,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
                (
                    ChannelId::TopSideRight,
                    ChannelId::SideRight,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
                (
                    ChannelId::WideLeft,
                    ChannelId::FrontLeft,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
                (
                    ChannelId::WideRight,
                    ChannelId::FrontRight,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
            ] {
                set_mix_gain(
                    &mut matrix,
                    source_layout.position_of(id),
                    target_layout.position_of(destination),
                    gain,
                );
            }
        }
        config::ChannelMixTemplate::ItuBs775 if target_count == 6 => {
            // Target is 5.1: match roles that exist in both, fold rear
            // surrounds into side surrounds, and fold heights/wides down.
            role_identity(source_layout, target_layout, &mut matrix);
            for (id, destination, gain) in [
                (
                    ChannelId::RearLeft,
                    ChannelId::SideLeft,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
                (
                    ChannelId::RearRight,
                    ChannelId::SideRight,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
                (
                    ChannelId::TopFrontLeft,
                    ChannelId::FrontLeft,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
                (
                    ChannelId::TopFrontRight,
                    ChannelId::FrontRight,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
                (
                    ChannelId::TopRearLeft,
                    ChannelId::SideLeft,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
                (
                    ChannelId::TopRearRight,
                    ChannelId::SideRight,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
                (
                    ChannelId::TopSideLeft,
                    ChannelId::SideLeft,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
                (
                    ChannelId::TopSideRight,
                    ChannelId::SideRight,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
                (
                    ChannelId::WideLeft,
                    ChannelId::FrontLeft,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
                (
                    ChannelId::WideRight,
                    ChannelId::FrontRight,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
            ] {
                set_mix_gain(
                    &mut matrix,
                    source_layout.position_of(id),
                    target_layout.position_of(destination),
                    gain,
                );
            }
        }
        _ => role_identity(source_layout, target_layout, &mut matrix),
    }
    matrix
}

/// Mix an interleaved source into an interleaved target using an explicit
/// template. The matrix is built on the stack and all output writes are
/// bounded by the declared layouts; no per-frame allocation occurs.
pub fn mix_interleaved_with_template(
    samples: &[f32],
    source_layout: &ChannelLayout,
    source_channels: usize,
    target_layout: &ChannelLayout,
    template: &config::ChannelMixTemplate,
    output: &mut [f32],
    frames: usize,
) -> usize {
    let source_channels = source_channels.min(MAX_CHANNELS);
    let target_channels = target_layout.channel_count().min(MAX_CHANNELS);
    let actual_frames = (samples.len() / source_channels.max(1))
        .min(frames)
        .min(output.len() / target_channels.max(1));
    let matrix = build_mix_matrix(source_layout, target_layout, template);
    for frame in 0..actual_frames {
        let src_base = frame * source_channels;
        let dst_base = frame * target_channels;
        for dst in 0..target_channels {
            let mut value = 0.0f32;
            for src in 0..source_channels {
                value += samples[src_base + src] * matrix[src][dst];
            }
            output[dst_base + dst] = value;
        }
    }
    actual_frames
}

/// Stereo-plane form used by the decode loop's downmix path.
pub fn mix_interleaved_to_stereo_with_template(
    samples: &[f32],
    source_layout: &ChannelLayout,
    source_channels: usize,
    template: &config::ChannelMixTemplate,
    plane_l: &mut [f32],
    plane_r: &mut [f32],
    frames: usize,
) -> usize {
    let source_channels = source_channels.min(MAX_CHANNELS);
    let actual_frames = (samples.len() / source_channels.max(1))
        .min(frames)
        .min(plane_l.len())
        .min(plane_r.len());
    let matrix = build_mix_matrix(source_layout, &ChannelLayout::Stereo, template);
    for frame in 0..actual_frames {
        let base = frame * source_channels;
        let mut left = 0.0f32;
        let mut right = 0.0f32;
        for src in 0..source_channels {
            left += samples[base + src] * matrix[src][0];
            right += samples[base + src] * matrix[src][1];
        }
        plane_l[frame] = left;
        plane_r[frame] = right;
    }
    actual_frames
}

/// Downmix an interleaved PCM slice to stereo using ITU-R BS.775 / BS.2051 semantic channel matrix.
pub fn downmix_interleaved_to_stereo(
    samples: &[f32],
    layout: &ChannelLayout,
    src_channels: usize,
    plane_l: &mut [f32],
    plane_r: &mut [f32],
    frames: usize,
) -> usize {
    let actual_frames = (samples.len() / src_channels.max(1))
        .min(frames)
        .min(plane_l.len())
        .min(plane_r.len());

    if src_channels == 2 {
        for frame in 0..actual_frames {
            let idx = frame * 2;
            plane_l[frame] = samples[idx];
            plane_r[frame] = samples[idx + 1];
        }
        return actual_frames;
    }

    if src_channels == 1 {
        for frame in 0..actual_frames {
            let s = samples[frame];
            plane_l[frame] = s;
            plane_r[frame] = s;
        }
        return actual_frames;
    }

    if src_channels > 2 {
        use crate::decode::ChannelId;
        let fl_idx = layout.position_of(ChannelId::FrontLeft).unwrap_or(0);
        let fr_idx = layout.position_of(ChannelId::FrontRight).unwrap_or(1);
        let c_idx = layout.position_of(ChannelId::Center);
        let sl_idx = layout.position_of(ChannelId::SideLeft);
        let sr_idx = layout.position_of(ChannelId::SideRight);
        let rl_idx = layout.position_of(ChannelId::RearLeft);
        let rr_idx = layout.position_of(ChannelId::RearRight);
        let bc_idx = layout.position_of(ChannelId::BackCenter);
        let wl_idx = layout.position_of(ChannelId::WideLeft);
        let wr_idx = layout.position_of(ChannelId::WideRight);
        let tfl_idx = layout.position_of(ChannelId::TopFrontLeft);
        let tfr_idx = layout.position_of(ChannelId::TopFrontRight);
        let trl_idx = layout.position_of(ChannelId::TopRearLeft);
        let trr_idx = layout.position_of(ChannelId::TopRearRight);
        let tsl_idx = layout.position_of(ChannelId::TopSideLeft);
        let tsr_idx = layout.position_of(ChannelId::TopSideRight);

        const SQRT_HALF: f32 = std::f32::consts::FRAC_1_SQRT_2;
        const HEIGHT_GAIN: f32 = 0.353_553_4;
        for frame in 0..actual_frames {
            let base = frame * src_channels;
            let fl = if base + fl_idx < samples.len() {
                samples[base + fl_idx]
            } else {
                0.0
            };
            let fr = if base + fr_idx < samples.len() {
                samples[base + fr_idx]
            } else {
                0.0
            };

            let get_s = |idx: Option<usize>| -> f32 {
                idx.and_then(|i| (base + i < samples.len()).then(|| samples[base + i]))
                    .unwrap_or(0.0)
            };

            let center = get_s(c_idx);
            let sl = get_s(sl_idx);
            let sr = get_s(sr_idx);
            let rl = get_s(rl_idx);
            let rr = get_s(rr_idx);
            let bc = get_s(bc_idx);
            let wl = get_s(wl_idx);
            let wr = get_s(wr_idx);
            let tfl = get_s(tfl_idx);
            let tfr = get_s(tfr_idx);
            let trl = get_s(trl_idx);
            let trr = get_s(trr_idx);
            let tsl = get_s(tsl_idx);
            let tsr = get_s(tsr_idx);

            // Fold in front, center, side surrounds, back center and rear surrounds (ITU-R BS.775),
            // plus wide and overhead height channels (ITU-R BS.2051).
            let l = fl
                + SQRT_HALF * center
                + SQRT_HALF * sl
                + 0.5 * bc
                + 0.5 * rl
                + SQRT_HALF * wl
                + HEIGHT_GAIN * (tfl + trl + tsl);
            let r = fr
                + SQRT_HALF * center
                + SQRT_HALF * sr
                + 0.5 * bc
                + 0.5 * rr
                + SQRT_HALF * wr
                + HEIGHT_GAIN * (tfr + trr + tsr);

            plane_l[frame] = l;
            plane_r[frame] = r;
        }
        return actual_frames;
    }

    actual_frames
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::ChannelLayout;

    #[test]
    fn test_downmix_five_point_one() {
        let layout = ChannelLayout::FivePointOne;
        // FL, FR, C, LFE, SL, SR
        let samples = vec![1.0, 0.5, 0.8, 0.9, 0.4, 0.2];
        let mut l = vec![0.0];
        let mut r = vec![0.0];
        let frames = downmix_interleaved_to_stereo(&samples, &layout, 6, &mut l, &mut r, 1);
        assert_eq!(frames, 1);

        const SQRT_HALF: f32 = std::f32::consts::FRAC_1_SQRT_2;
        let expected_l = 1.0 + SQRT_HALF * 0.8 + SQRT_HALF * 0.4;
        let expected_r = 0.5 + SQRT_HALF * 0.8 + SQRT_HALF * 0.2;
        assert!((l[0] - expected_l).abs() < 1e-5);
        assert!((r[0] - expected_r).abs() < 1e-5);
    }

    #[test]
    fn test_downmix_seven_point_one_four_preserves_overheads() {
        let layout = ChannelLayout::SevenPointOneFour;
        // Layout: FL, FR, C, LFE, SL, SR, RL, RR, TFL, TFR, TRL, TRR
        let mut samples = vec![0.0f32; 12];
        // Only overheads have signal:
        samples[8] = 1.0; // TFL
        samples[9] = 1.0; // TFR
        samples[10] = 0.5; // TRL
        samples[11] = 0.5; // TRR

        let mut l = vec![0.0];
        let mut r = vec![0.0];
        let frames = downmix_interleaved_to_stereo(&samples, &layout, 12, &mut l, &mut r, 1);
        assert_eq!(frames, 1);

        const HEIGHT_GAIN: f32 = 0.353_553_4;
        let expected_l = HEIGHT_GAIN * (1.0 + 0.5);
        let expected_r = HEIGHT_GAIN * (1.0 + 0.5);
        assert!((l[0] - expected_l).abs() < 1e-5);
        assert!((r[0] - expected_r).abs() < 1e-5);
        assert!(l[0] > 0.0, "Overhead energy must not be muted");
    }

    #[test]
    fn test_downmix_nine_point_one_six_preserves_wides_and_heights() {
        let layout = ChannelLayout::NinePointOneSix;
        // 16 channels: FL FR C LFE SL SR RL RR WL WR TFL TFR TSL TSR TRL TRR
        let mut samples = vec![0.0f32; 16];
        samples[8] = 0.8; // WL
        samples[9] = 0.8; // WR
        samples[10] = 1.0; // TFL
        samples[11] = 1.0; // TFR
        samples[12] = 0.6; // TSL
        samples[13] = 0.6; // TSR

        let mut l = vec![0.0];
        let mut r = vec![0.0];
        let frames = downmix_interleaved_to_stereo(&samples, &layout, 16, &mut l, &mut r, 1);
        assert_eq!(frames, 1);

        const SQRT_HALF: f32 = std::f32::consts::FRAC_1_SQRT_2;
        const HEIGHT_GAIN: f32 = 0.353_553_4;
        let expected_l = SQRT_HALF * 0.8 + HEIGHT_GAIN * (1.0 + 0.6);
        let expected_r = SQRT_HALF * 0.8 + HEIGHT_GAIN * (1.0 + 0.6);
        assert!((l[0] - expected_l).abs() < 1e-5);
        assert!((r[0] - expected_r).abs() < 1e-5);
    }

    #[test]
    fn test_template_seven_point_one_four_to_seven_point_one() {
        let src_layout = ChannelLayout::SevenPointOneFour;
        let dst_layout = ChannelLayout::SevenPointOne;
        let matrix = build_mix_matrix(
            &src_layout,
            &dst_layout,
            &config::ChannelMixTemplate::ItuBs775,
        );

        // TFL (src index 8) should route to FL (dst index 0)
        assert!((matrix[8][0] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5);
        // TFR (src index 9) should route to FR (dst index 1)
        assert!((matrix[9][1] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5);
        // TRL (src index 10) should route to RL (dst index 6)
        assert!((matrix[10][6] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5);
        // TRR (src index 11) should route to RR (dst index 7)
        assert!((matrix[11][7] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5);
    }
}
