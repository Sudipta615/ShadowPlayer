//! The Playback category: speed, pitch, transitions, and loudness.

use super::super::widgets::{
    badge_label, card_header, dual_tone_slider, pill_btn, segment_picker, setting_card,
    setting_item, setting_item_with_info,
};
use super::super::{SettingsExtraContext, SettingsTabActions, SettingsTabState};
use crate::theme::Theme;
use config::{CrossfadeCurve, LoudnessMode, SpeedMode, TransitionMode};
use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

pub(in crate::components::settings_tab) fn render_playback_category(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
    extra: &SettingsExtraContext<'_>,
    actions: &mut SettingsTabActions,
) {
    // Card 1: Speed, Pitch & Mode
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_disc,
            &tr!("Playback Speed & Pitch Transposition"),
            &tr!("Adjust playback rate and pitch using high-precision time-stretching or vintage varispeed"),
            Some((
                if state.speed == 1.0 && state.pitch_semitones == 0.0 { "NORMAL" } else { "MODIFIED" },
                if state.speed == 1.0 && state.pitch_semitones == 0.0 { theme.bg_pill } else { theme.accent.linear_multiply(0.3) },
                if state.speed == 1.0 && state.pitch_semitones == 0.0 { theme.text_muted } else { theme.accent_warm },
            )),
            theme,
        );

        // Speed Slider + Presets
        setting_item(
            ui,
            &tr!("Playback Speed"),
            &tr!("Tempo multiplier (0.25x slow down to 4.00x speed up)"),
            theme,
            |ui| {
                let mut v = state.speed;
                if dual_tone_slider(ui, &mut v, 0.25..=4.0, 0.05, 0.0, theme) {
                    state.speed = v;
                    actions.speed_changed = Some(v);
                }
                badge_label(ui, format!("{:.2}x", state.speed), theme);
                if state.speed != 1.0
                    && ui
                        .add(pill_btn(tr!(Msg::S41E813F1165AB4DE), false, theme))
                        .clicked()
                {
                    state.speed = 1.0;
                    actions.speed_changed = Some(1.0);
                }
            },
        );

        // Pitch Transposition
        setting_item(
            ui,
            &tr!("Pitch Transposition"),
            &tr!("Shift musical key in semitone increments (-24 to +24 semitones)"),
            theme,
            |ui| {
                let mut v = state.pitch_semitones;
                if dual_tone_slider(ui, &mut v, -24.0..=24.0, 0.5, 0.0, theme) {
                    state.pitch_semitones = v;
                    actions.pitch_changed = Some(v);
                }
                badge_label(ui, format!("{:+.1} st", state.pitch_semitones), theme);
                if state.pitch_semitones != 0.0
                    && ui
                        .add(pill_btn(tr!(Msg::S255857544A9D5EC0), false, theme))
                        .clicked()
                {
                    state.pitch_semitones = 0.0;
                    actions.pitch_changed = Some(0.0);
                }
            },
        );

        // Speed Processing Mode
        setting_item_with_info(
            ui,
            &tr!("Processing Engine"),
            &tr!("Varispeed (analog tape pitch shift) vs TimeStretch (formant-preserved tempo)"),
            Some(&crate::components::info_box::INFO_SPEED_PITCH),
            theme,
            |ui| {
                let options = [
                    (SpeedMode::Varispeed, "Varispeed"),
                    (SpeedMode::TimeStretch, "TimeStretch"),
                    (SpeedMode::PitchShift, "PitchShift"),
                ];
                if segment_picker(ui, &mut state.speed_mode, &options, theme) {
                    actions.speed_mode_changed = Some(state.speed_mode);
                }
            },
        );
    });

    // Card 2: Track Transitions & Crossfade
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_shuffle,
            &tr!("Track Transitions & Crossfading"),
            &tr!("Continuous gapless audio streaming or smooth multi-second DJ crossfading between songs"),
            Some((
                match state.transition_mode {
                    TransitionMode::Gapless => "GAPLESS DIRECT",
                    TransitionMode::Crossfade => "CROSSFADE ACTIVE",
                    TransitionMode::Fade => "FADE",
                    TransitionMode::Stop => "HARD STOP",
                },
                theme.bg_pill,
                theme.accent,
            )),
            theme,
        );

        setting_item_with_info(
            ui,
            &tr!("Transition Mode"),
            &tr!("Gapless mode provides seamless album playback without clicks or silence gaps"),
            Some(&crate::components::info_box::INFO_TRANSITION_CROSSFADE),
            theme,
            |ui| {
                let options = [
                    (TransitionMode::Gapless, "Gapless (Audiophile)"),
                    (TransitionMode::Crossfade, "Crossfade"),
                    (TransitionMode::Fade, "Fade"),
                    (TransitionMode::Stop, "Stop"),
                ];
                if segment_picker(ui, &mut state.transition_mode, &options, theme) {
                    actions.transition_mode_changed = Some(state.transition_mode);
                }
            },
        );

        if state.transition_mode == TransitionMode::Crossfade {
            ui.add_space(4.0);
            let sub_frame = egui::Frame::NONE
                .fill(theme.bg_pill.linear_multiply(0.6))
                .stroke(Stroke::new(1.0, theme.border_card))
                .corner_radius(CornerRadius::same(theme.radius_lg))
                .inner_margin(egui::Margin::symmetric(
                    theme.space_2xl as i8,
                    theme.space_xl as i8,
                ));

            sub_frame.show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_lg);

                setting_item(
                    ui,
                    &tr!("Crossfade Duration"),
                    &tr!("Overlap time in seconds between fading out song and fading in next song"),
                    theme,
                    |ui| {
                        let mut v = state.crossfade_duration_secs;
                        if dual_tone_slider(ui, &mut v, 0.5..=15.0, 0.1, 0.0, theme) {
                            state.crossfade_duration_secs = v;
                            actions.crossfade_config_changed = true;
                        }
                        badge_label(ui, format!("{:.1} s", state.crossfade_duration_secs), theme);
                    },
                );

                setting_item(
                    ui,
                    &tr!("Crossfade Curve"),
                    &tr!("Constant Power maintains perceived volume energy; Linear creates gradual dip"),
                    theme,
                    |ui| {
                        let options = [
                            (
                                CrossfadeCurve::ConstantPower,
                                "Constant Power (Equal Energy)",
                            ),
                            (CrossfadeCurve::Linear, "Linear"),
                            (CrossfadeCurve::SCurve, "S-Curve"),
                        ];
                        if segment_picker(ui, &mut state.crossfade_curve, &options, theme) {
                            actions.crossfade_curve_changed = Some(state.crossfade_curve);
                            actions.crossfade_config_changed = true;
                        }
                    },
                );

                let preview = extra.crossfade_preview;
                setting_item(
                    ui,
                    &tr!("Audition Transition"),
                    &tr!("Play a three-second local demo through this output device. It does not touch the current song or the playback queue."),
                    theme,
                    |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(theme.space_md, 0.0);
                            let label = if preview.playing {
                                tr!("Stop Preview")
                            } else {
                                tr!("Play Preview")
                            };
                            if ui.add(pill_btn(label, preview.playing, theme)).clicked() {
                                actions.toggle_crossfade_preview = true;
                            }

                            // The preview envelope is rendered from the same
                            // buffer the audition plays, so the picture is
                            // the sound, not a decorative squiggle.
                            let width = 176.0;
                            let height = 34.0;
                            let (rect, _) = ui.allocate_exact_size(
                                Vec2::new(width, height),
                                Sense::hover(),
                            );
                            let painter = ui.painter_at(rect);
                            painter.rect_filled(
                                rect,
                                CornerRadius::same(theme.radius_sm),
                                theme.bg_pill,
                            );
                            let peaks = preview.peaks;
                            if peaks.is_empty() {
                                painter.text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    tr!("No preview rendered"),
                                    crate::theme::font(10.0),
                                    theme.text_dim,
                                );
                            } else {
                                let mid = rect.center().y;
                                let bar_w = (rect.width() / peaks.len() as f32).max(1.0);
                                for (index, peak) in peaks.iter().enumerate() {
                                    let x = rect.min.x + index as f32 * bar_w;
                                    let half = (peak.clamp(0.0, 1.0) * (height * 0.42)).max(0.7);
                                    let played =
                                        index as f32 / peaks.len().max(1) as f32 <= preview.progress;
                                    painter.rect_filled(
                                        Rect::from_min_max(
                                            Pos2::new(x, mid - half),
                                            Pos2::new(x + bar_w * 0.7, mid + half),
                                        ),
                                        CornerRadius::same(theme.radius_xs),
                                        if played {
                                            theme.accent_highlight
                                        } else {
                                            theme.text_dim.gamma_multiply(0.55)
                                        },
                                    );
                                }
                                if preview.playing {
                                    let x = rect.min.x + rect.width() * preview.progress;
                                    painter.line_segment(
                                        [
                                            Pos2::new(x, rect.min.y + 2.0),
                                            Pos2::new(x, rect.max.y - 2.0),
                                        ],
                                        Stroke::new(1.5_f32, theme.accent_warm),
                                    );
                                }
                            }

                            if let Some(error) = preview.error {
                                ui.label(
                                    egui::RichText::new(error)
                                        .font(crate::theme::font(10.5))
                                        .color(theme.destructive),
                                );
                            }
                        });
                    },
                );
            });
        }
    });

    // Card 3: Loudness Normalization
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_bar_chart,
            &tr!("Loudness Normalization (ReplayGain & EBU R128)"),
            &tr!("Automated gain matching prevents sudden volume jumps between quiet acoustic and loud mastered tracks"),
            None,
            theme,
        );

        setting_item_with_info(
            ui,
            &tr!("Target Standard"),
            &tr!("EBU R128 (-14 LUFS) or legacy ReplayGain 2.0 tags"),
            Some(&crate::components::info_box::INFO_LOUDNESS_NORMALIZATION),
            theme,
            |ui| {
                let options = [
                    (LoudnessMode::Off, "Disabled (Pure)"),
                    (LoudnessMode::TrackReplayGain, "Track ReplayGain"),
                    (LoudnessMode::AlbumReplayGain, "Album ReplayGain"),
                    (LoudnessMode::EbuR128, "EBU R128 Broadcast"),
                ];
                if segment_picker(ui, &mut state.loudness_mode, &options, theme) {
                    actions.loudness_mode_changed = Some(state.loudness_mode);
                }
            },
        );
    });
}
