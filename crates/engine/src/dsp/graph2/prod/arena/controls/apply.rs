use super::*;

pub(super) fn apply_node_cmd(node: &mut GraphNode, cmd: &NodeCmd) {
    match (&mut *node, cmd) {
        (GraphNode::Volume(n), NodeCmd::SetVolumeTarget(v)) => n.processor.set_gain(*v),
        (GraphNode::Balance(n), NodeCmd::SetBalance(b)) => n.set_balance(*b),
        (GraphNode::SeekFade(n), NodeCmd::BeginSeekFadeout) => n.fade.fade_out(),
        (GraphNode::SeekFade(n), NodeCmd::BeginSeekFadein) => n.fade.fade_in(),
        (GraphNode::Mix(n), NodeCmd::MixInput { input, cmd }) => {
            n.apply_input(*input as usize, *cmd)
        }
        (GraphNode::Mix(n), NodeCmd::MixTransition(cmd)) => n.apply_transition(*cmd),
        (GraphNode::Mix(n), NodeCmd::SetDuck(cfg)) => n.apply_duck(*cfg),
        (
            GraphNode::Aux(n),
            NodeCmd::SetAux {
                enabled,
                return_gain,
            },
        ) => n.apply_aux(*enabled, *return_gain),
        (GraphNode::Aux(n), NodeCmd::SetAuxInsert { enabled, wet_mix }) => {
            n.set_aux_insert(*enabled, *wet_mix)
        }
        (GraphNode::Correction(n), NodeCmd::SetCorrectionEnabled(enabled)) => {
            n.set_runtime(*enabled, n.depth())
        }
        (GraphNode::Correction(n), NodeCmd::SetCorrectionDepth(depth)) => {
            n.set_runtime(n.enabled(), *depth)
        }
        (GraphNode::Spatial(n), NodeCmd::SetSpatialEnabled(enabled)) => n.set_enabled(*enabled),
        (GraphNode::PluginHost(n), NodeCmd::SetPluginEnabled(enabled)) => {
            n.set_runtime_enabled(*enabled)
        }
        (GraphNode::PluginHost(n), NodeCmd::SetPluginParams(batch)) => n.apply_params(batch),
        (GraphNode::PluginHost(n), NodeCmd::SetClapPluginEnabled(enabled)) => {
            n.set_clap_enabled(*enabled)
        }
        (GraphNode::PluginHost(n), NodeCmd::SetClapPluginParam { id, value }) => {
            n.set_clap_param(*id, *value)
        }
        (
            GraphNode::Spatial(n),
            NodeCmd::SetSpatialScreen {
                center_azimuth_deg,
                half_width_deg,
                elevation_deg,
                gain,
            },
        ) => n.apply_screen(*center_azimuth_deg, *half_width_deg, *elevation_deg, *gain),
        (
            GraphNode::Spatial(n),
            NodeCmd::SetSpatialRoom {
                enabled,
                width,
                depth,
                height,
                absorption,
                reflection_order,
                rt60_ms,
                late_mix,
                late_distance,
                wet,
            },
        ) => n.apply_room(
            *enabled,
            *width,
            *depth,
            *height,
            *absorption,
            *reflection_order,
            *rt60_ms,
            *late_mix,
            *late_distance,
            *wet,
        ),
        (GraphNode::Spatial(n), NodeCmd::SetSpatialAir(air)) => n.set_air_absorption(*air),
        (
            GraphNode::Spatial(n),
            NodeCmd::SetSpatialListener {
                yaw_deg,
                pitch_deg,
                roll_deg,
            },
        ) => n.apply_listener(*yaw_deg, *pitch_deg, *roll_deg),
        (
            GraphNode::Spatial(n),
            NodeCmd::SetSpatialListenerPose {
                qx,
                qy,
                qz,
                qw,
                px,
                py,
                pz,
            },
        ) => {
            use crate::spatial::math::{Quat, Vec3};
            n.set_listener_pose_target(Quat::new(*qx, *qy, *qz, *qw), Vec3::new(*px, *py, *pz));
        }
        (
            GraphNode::Spatial(n),
            NodeCmd::SetSpatialListenerTracking {
                smoothing_ms,
                max_angular_rate_deg_s,
            },
        ) => n.set_listener_tracking(crate::spatial::TrackingConfig {
            smoothing_ms: *smoothing_ms,
            max_angular_rate_deg_s: *max_angular_rate_deg_s,
        }),
        // ── Scene animation cues ──
        (GraphNode::Spatial(n), NodeCmd::TriggerSpatialCue(i)) => {
            n.trigger_cue(*i);
        }
        (GraphNode::Spatial(n), NodeCmd::StopSpatialCue(t)) => n.stop_cue(*t),
        (GraphNode::Spatial(n), NodeCmd::StopAllSpatialCues) => n.stop_all_cues(),
        (GraphNode::Mix(n), NodeCmd::SetMixCurve(c)) => n.curve = (*c).into(),
        (GraphNode::Mix(n), NodeCmd::SetMixEnabled(e)) => n.crossfade_enabled = *e,
        (GraphNode::Mix(n), NodeCmd::SetMixDurationFrames(f)) => {
            n.crossfade_duration_frames = (*f).max(1)
        }
        (GraphNode::Limiter(n), NodeCmd::SetLimiterEnabled(e)) => n.limiter.set_enabled(*e),
        (GraphNode::Limiter(n), NodeCmd::SetLimiterMode(m)) => n.limiter.set_mode(*m),
        (
            GraphNode::Limiter(n),
            NodeCmd::SetLimiterParams {
                lookahead_ms,
                attack_ms,
                release_ms,
                ceiling_db,
                soft_clip,
            },
        ) => {
            n.limiter.set_lookahead(*lookahead_ms);
            n.limiter.set_attack(*attack_ms);
            n.limiter.set_release(*release_ms);
            n.limiter.set_ceiling_db(*ceiling_db);
            n.limiter.set_soft_clip(*soft_clip);
        }
        (GraphNode::Limiter(n), NodeCmd::SetLimiterTruePeak(e)) => n.limiter.enable_true_peak(*e),
        (GraphNode::Eq(n), NodeCmd::SetEqEnabled(e)) => n.eq.set_enabled(*e),
        (GraphNode::Eq(n), NodeCmd::SetEqAutoHeadroom(e)) => n.eq.set_auto_headroom(*e),
        (GraphNode::Eq(n), NodeCmd::SetEqPreampDb(db)) => n.eq.set_preamp_db(*db),
        (GraphNode::Eq(n), NodeCmd::SetEqBassShelf(db)) => n.eq.set_bass_shelf(*db),
        (GraphNode::Eq(n), NodeCmd::SetEqTrebleShelf(db)) => n.eq.set_treble_shelf(*db),
        (GraphNode::Eq(n), NodeCmd::SetEqBand { index, params }) => n.eq.set_band(*index, *params),
        (GraphNode::Eq(n), NodeCmd::SetMidsideEq(e)) => n.midside_enabled = *e,
        (GraphNode::Eq(n), NodeCmd::SetDynamicEqEnabled(e)) => n.set_dynamic_enabled(*e),
        (GraphNode::Eq(n), NodeCmd::SetDynamicEqBand { index, params }) => {
            n.set_dynamic_band(*index, *params);
        }
        (GraphNode::Convolution(n), NodeCmd::SetConvolutionWetMix(mix)) => {
            n.engine.set_wet_mix(*mix)
        }
        (GraphNode::Stereo(n), NodeCmd::SetStereoWidth(w)) => {
            let normalized = if *w > 2.0 { *w / 100.0 } else { *w };
            n.enhancer.set_width(normalized);
            n.enhancer.set_enabled((normalized - 1.0).abs() > 0.001);
        }
        (GraphNode::Stereo(n), NodeCmd::SetStereoEnhancerEnabled(e)) => n.enhancer.set_enabled(*e),
        (GraphNode::Crossfeed(n), NodeCmd::SetCrossfeedEnabled(e)) => n.crossfeed.set_enabled(*e),
        (GraphNode::Crossfeed(n), NodeCmd::SetCrossfeedProfile(p)) => n.crossfeed.set_profile(*p),
        (GraphNode::Crossfeed(n), NodeCmd::SetCrossfeedCustom(f, q, d)) => {
            n.crossfeed.set_custom_params(*f, *q, *d)
        }
        (GraphNode::Fx(n), NodeCmd::SetFxConfig(cfg)) => n.apply_config(cfg),
        (GraphNode::Dynamics(n), NodeCmd::SetCompressorEnabled(e)) => n.compressor.set_enabled(*e),
        (
            GraphNode::Dynamics(n),
            NodeCmd::SetCompressorBand {
                band,
                threshold_db,
                ratio,
                attack_ms,
                release_ms,
                makeup_gain_db,
            },
        ) => n.compressor.set_band_params(
            *band,
            *threshold_db,
            *ratio,
            *attack_ms,
            *release_ms,
            *makeup_gain_db,
        ),
        (
            GraphNode::Dynamics(n),
            NodeCmd::SetCompressorBandFeatures {
                band,
                knee_db,
                detector,
                stereo_link,
            },
        ) => n
            .compressor
            .set_band_features(*band, *knee_db, *detector, *stereo_link),
        _ => {
            debug_assert!(
                false,
                "control command {:?} applied to wrong node kind {:?}",
                cmd,
                std::mem::discriminant(node)
            );
        }
    }
}
