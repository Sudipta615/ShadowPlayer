use audio_session::AudioSession;
use domain::models::Track;
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Stroke, Vec2};
use engine::diagnostics::BitPerfectCause;

use crate::state::DspState;
use crate::theme::Theme;

/// Render the Live Bit-Perfect Signal Path Inspector & Diagnostic Modal
pub fn show_bit_perfect_modal(
    ctx: &egui::Context,
    is_open: &mut bool,
    current_track: Option<&Track>,
    session: &AudioSession,
    dsp_state: &mut DspState,
    theme: &Theme,
) {
    if !*is_open {
        return;
    }

    let playback_info = session.playback_info();
    let is_bit_perfect = playback_info.bit_perfect;
    let cause = session.bit_perfect_cause();
    let engine_stats = playback_info.engine_stats.as_ref();
    let output_facts = crate::components::output_facts::output_facts(
        playback_info.output_info.as_ref(),
        session.latency_ms(),
    );

    let mut open = *is_open;
    let mut close_clicked = false;

    let closed = crate::dialog::show_modal(
        ctx,
        "bit_perfect",
        "Bit-Perfect Signal Path Inspector",
        620.0,
        theme,
        |ui| {
            egui::ScrollArea::vertical()
                .id_salt("bit_perfect_body")
                .auto_shrink([false, false])
                .max_height(
                    (crate::dialog::viewport_rect(ctx).height() - 140.0).max(160.0),
                )
                .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_2xl);

                // 1. Status Banner
                let banner_color = if is_bit_perfect {
                    theme.bit_perfect_green.linear_multiply(0.18)
                } else {
                    theme.accent.linear_multiply(0.18)
                };
                let banner_stroke = if is_bit_perfect {
                    theme.bit_perfect_green
                } else {
                    theme.accent
                };

                egui::Frame::NONE
                    .fill(banner_color)
                    .stroke(Stroke::new(1.0_f32, banner_stroke))
                    .corner_radius(CornerRadius::same(theme.radius_lg))
                    .inner_margin(egui::Margin::symmetric(theme.space_2xl as i8, theme.space_xl as i8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(theme.space_lg, 0.0);
                            if is_bit_perfect {
                                paint_status_dot(ui, theme.bit_perfect_green, 6.5);
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new(tr!("SIGNAL PATH IS 100% BIT-PERFECT"))
                                            .font(crate::theme::font(14.0))
                                            .strong()
                                            .color(theme.bit_perfect_green),
                                    );
                                    ui.label(
                                        egui::RichText::new(tr!("Direct unmolested bitstream to audio hardware DAC. Zero DSP coloration."))
                                            .font(crate::theme::font(11.5))
                                            .color(theme.text_secondary),
                                    );
                                });
                            } else {
                                paint_warning_triangle(ui, theme.accent_highlight);
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new(tr!("DSP SIGNAL ALTERATION ACTIVE"))
                                            .font(crate::theme::font(14.0))
                                            .strong()
                                            .color(theme.accent_highlight),
                                    );
                                    let cause_text = match cause {
                                        Some(BitPerfectCause::VolumeNotUnity) => "Software volume / balance is modifying sample values (< 0 dB).",
                                        Some(BitPerfectCause::EqActive) => "Equalizer filters are shaping frequency amplitudes.",
                                        Some(BitPerfectCause::DynamicsActive) => "Compressor, limiter, crossfeed or spatial room DSP is active.",
                                        Some(BitPerfectCause::SpeedOrResampleActive) => "Resampling engine or variable speed playback is engaged.",
                                        Some(BitPerfectCause::SampleRateMismatch) => "Source sample rate differs from DAC endpoint hardware rate.",
                                        Some(BitPerfectCause::OutputNotDirectExclusive) => "Audio output is routed through OS shared mixer, not exclusive mode.",
                                        Some(BitPerfectCause::DitherActive) => "Dither quantization is modifying lowest bits.",
                                        Some(c) => c.code(),
                                        None => "DSP chain is processing audio.",
                                    };
                                    ui.label(
                                        egui::RichText::new(cause_text)
                                            .font(crate::theme::font(11.5))
                                            .color(theme.text_secondary),
                                    );
                                });
                            }
                        });
                    });

                // 2. Visual Signal Path Diagram
                ui.label(
                    egui::RichText::new(tr!("AUDIO SIGNAL FLOW"))
                        .font(crate::theme::font(11.0))
                        .strong()
                        .color(theme.text_muted),
                );

                let (flow_rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 84.0), egui::Sense::hover());
                let painter = ui.painter_at(flow_rect);

                painter.rect_filled(flow_rect, 8.0, theme.meter_bg);
                painter.rect_stroke(
                    flow_rect,
                    8.0,
                    Stroke::new(1.0_f32, theme.border_subtle),
                    egui::StrokeKind::Inside,
                );

                // Stages
                let src_text = current_track.map(|t| {
                    format!("Source\n{} {:.1}kHz", t.format, t.sample_rate as f32 / 1000.0)
                }).unwrap_or_else(|| "Source\nPCM Direct".into());

                let dec_text = if playback_info.native_dsd_active {
                    "Decoder\nNative DSD"
                } else if playback_info.dop_active {
                    "Decoder\nDoP 24-bit"
                } else {
                    "Decoder\nPure Lossless"
                };

                let dsp_text = if is_bit_perfect {
                    "DSP Chain\nBypassed"
                } else {
                    "DSP Chain\nProcessing"
                };

                let resamp_text = if engine_stats.map(|s| s.resampler_active).unwrap_or(false) {
                    "Resampler\nActive"
                } else {
                    "Resampler\nDirect"
                };

                let out_text = engine_stats.map(|s| {
                    format!("DAC Endpoint\n{}", s.output_backend)
                }).unwrap_or_else(|| "DAC Endpoint\nHardware".into());

                let stages = [
                    (src_text, true),
                    (dec_text.to_string(), true),
                    (dsp_text.to_string(), is_bit_perfect),
                    (resamp_text.to_string(), is_bit_perfect),
                    (out_text, is_bit_perfect),
                ];

                let stage_w = (flow_rect.width() - 48.0) / 5.0;
                let node_h = 52.0;
                let y_pos = flow_rect.center().y - (node_h / 2.0);

                for (idx, (label, bit_clean)) in stages.iter().enumerate() {
                    let x_pos = flow_rect.left() + 8.0 + (idx as f32) * (stage_w + 8.0);
                    let node_rect = Rect::from_min_size(Pos2::new(x_pos, y_pos), Vec2::new(stage_w, node_h));

                    let (bg_col, stroke_col) = if *bit_clean {
                        (theme.bit_perfect_green.linear_multiply(0.18), theme.bit_perfect_green)
                    } else {
                        (theme.accent_highlight.linear_multiply(0.18), theme.accent_highlight)
                    };

                    painter.rect_filled(node_rect, CornerRadius::same(theme.radius_sm), bg_col);
                    painter.rect_stroke(node_rect, CornerRadius::same(theme.radius_sm), Stroke::new(1.0_f32, stroke_col), egui::StrokeKind::Inside);

                    painter.text(
                        node_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        label,
                        crate::theme::font(10.5),
                        Color32::WHITE,
                    );

                    // Connector arrow to next stage
                    if idx < 4 {
                        // A stroked chevron: the bundled fonts have no "▶" glyph.
                        let arrow_x = node_rect.right() + 4.0;
                        let cy = node_rect.center().y;
                        let stroke = Stroke::new(1.4_f32, theme.text_muted);
                        painter.line_segment(
                            [
                                Pos2::new(arrow_x - 2.0, cy - 4.0),
                                Pos2::new(arrow_x + 1.5, cy),
                            ],
                            stroke,
                        );
                        painter.line_segment(
                            [
                                Pos2::new(arrow_x + 1.5, cy),
                                Pos2::new(arrow_x - 2.0, cy + 4.0),
                            ],
                            stroke,
                        );
                    }
                }

                ui.add_space(2.0);

                // 3. Itemized Live Transformation Ledger
                ui.label(
                    egui::RichText::new(tr!("ACTIVE TRANSFORMATIONS & SIGNAL ALTERATIONS"))
                        .font(crate::theme::font(11.0))
                        .strong()
                        .color(theme.text_muted),
                );

                let ledger_frame = egui::Frame::NONE
                    .fill(theme.bg_card)
                    .stroke(Stroke::new(1.0_f32, theme.border_subtle))
                    .corner_radius(CornerRadius::same(theme.radius_md))
                    .inner_margin(egui::Margin::symmetric(theme.space_2xl as i8, theme.space_md as i8));

                ledger_frame.show(ui, |ui| {
                    if is_bit_perfect {
                        ui.horizontal(|ui| {
                            paint_status_dot(ui, theme.bit_perfect_green, 4.0);
                            ui.label(
                                egui::RichText::new(tr!("Direct Signal Path: Bit-Perfect passthrough. Zero DSP coloration."))
                                    .font(crate::theme::font(12.0))
                                    .color(theme.bit_perfect_green),
                            );
                        });
                    } else {
                        ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_sm);
                        let mut alterations: Vec<(String, String, &'static str)> = Vec::new();

                        // 1. Digital Volume
                        let vol = playback_info.volume;
                        if (vol - 1.0).abs() > 0.001 {
                            let gain_db = 20.0 * (vol + 1e-5).log10();
                            alterations.push((
                                tr!("Digital Volume / Attenuation"),
                                format!("{gain_db:.1} dB attenuation"),
                                "Software gain",
                            ));
                        }

                        // 2. Equalizer
                        if dsp_state.eq_panel.eq_enabled {
                            let (name, details) = if dsp_state.eq_panel.eq_mode == config::EqMode::Graphic {
                                let n = dsp_state.eq_panel.graphic_layout.num_bands();
                                (
                                    tr!("Graphic Equalizer"),
                                    format!("{n}-band Graphic EQ ({:+.1} dB preamp)", dsp_state.eq_panel.graphic_preamp_db),
                                )
                            } else {
                                let active = dsp_state.eq_panel.bands.iter().filter(|b| b.enabled && b.gain_db.abs() > 0.05).count();
                                (
                                    tr!("Parametric Equalizer"),
                                    format!("{active} active filter bands ({:+.1} dB preamp)", dsp_state.eq_panel.preamp_db),
                                )
                            };
                            alterations.push((name, details, "EQ filters"));
                        }

                        // 3. Tone controls
                        let tone = &dsp_state.eq_panel;
                        if tone.bass_shelf_db.abs() > 0.05
                            || tone.treble_shelf_db.abs() > 0.05
                            || (tone.stereo_width - 1.0).abs() > 0.05
                            || tone.balance.abs() > 0.05
                            || tone.midside_eq
                        {
                            let details = format!(
                                "Bass {:+.1} dB, Treble {:+.1} dB, Width {:.0}%",
                                tone.bass_shelf_db,
                                tone.treble_shelf_db,
                                tone.stereo_width * 100.0,
                            );
                            alterations.push((tr!("Tone Controls"), details, "Tone shaping"));
                        }

                        // 4. Headphone Crossfeed
                        if dsp_state.eq_panel.crossfeed_enabled {
                            let profile = match dsp_state.eq_panel.crossfeed_profile {
                                config::CrossfeedProfile::Bauer => "Bauer standard",
                                config::CrossfeedProfile::Jmeier => "Jan Meier natural",
                                config::CrossfeedProfile::ChuMoy => "Chu Moy classic",
                                config::CrossfeedProfile::Custom => "Custom parameters",
                            };
                            let details = format!(
                                "{} ({:.0} Hz, {:.1} ms delay)",
                                profile,
                                dsp_state.eq_panel.crossfeed_freq,
                                dsp_state.eq_panel.crossfeed_delay_ms,
                            );
                            alterations.push((tr!("Headphone Crossfeed"), details, "Binaural ITD"));
                        }

                        // 5. Dynamics (Limiter & Multiband Compressor)
                        if dsp_state.eq_panel.limiter_enabled {
                            let details = format!(
                                "Ceiling {:.1} dBTP, Lookahead {:.1} ms",
                                dsp_state.eq_panel.limiter_ceiling_db,
                                dsp_state.eq_panel.limiter_lookahead_ms,
                            );
                            alterations.push((tr!("True-Peak Limiter"), details, "Brickwall guard"));
                        }
                        if dsp_state.eq_panel.compressor_enabled {
                            alterations.push((
                                tr!("Multiband Compressor"),
                                "3-band RMS dynamics processor".to_string(),
                                "Dynamic range",
                            ));
                        }

                        // 6. Spatial Audio & Room Acoustics
                        if dsp_state.settings_tab.spatial_enabled || dsp_state.settings_tab.room_enabled {
                            let details = if dsp_state.settings_tab.room_enabled {
                                format!("Acoustic room simulation (RT60: {:.0} ms)", dsp_state.settings_tab.room_rt60_ms)
                            } else {
                                "Binaural listener tracking & HRTF".to_string()
                            };
                            alterations.push((tr!("Spatial Audio & Room Acoustics"), details, "Spatial convolution"));
                        }

                        // 7. Creative Effects
                        if dsp_state.eq_panel.fx.any_enabled() {
                            let mut active_fx = Vec::new();
                            if dsp_state.eq_panel.fx.chorus.enabled { active_fx.push("Chorus"); }
                            if dsp_state.eq_panel.fx.flanger.enabled { active_fx.push("Flanger"); }
                            if dsp_state.eq_panel.fx.phaser.enabled { active_fx.push("Phaser"); }
                            if dsp_state.eq_panel.fx.delay.enabled { active_fx.push("Delay"); }
                            if dsp_state.eq_panel.fx.distortion.enabled { active_fx.push("Saturation"); }
                            alterations.push((tr!("Creative Effects (Insert)"), active_fx.join(", "), "FX insert"));
                        }

                        // 8. Resampler
                        if let Some(stats) = engine_stats {
                            if stats.resampler_active {
                                let details = format!(
                                    "{} Hz \u{2192} {} Hz ({})",
                                    playback_info.sample_rate,
                                    stats.output_sample_rate,
                                    stats.resampler_quality,
                                );
                                alterations.push((tr!("Sample Rate Conversion (Resampler)"), details, "Sinc resampling"));
                            }
                        }

                        // 9. Dither
                        if dsp_state.settings_tab.dither_enabled || dsp_state.eq_panel.dither_enabled {
                            alterations.push((tr!("Dither & Noise Shaping"), "TPDF triangular dither active".to_string(), "Quantization"));
                        }

                        if alterations.is_empty() {
                            ui.horizontal(|ui| {
                                paint_status_dot(ui, theme.accent_highlight, 3.5);
                                ui.label(
                                    egui::RichText::new("DSP processing active in audio pipeline")
                                        .font(crate::theme::font(11.5))
                                        .color(theme.text_secondary),
                                );
                            });
                        } else {
                            for (name, details, impact) in &alterations {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_md, 0.0);
                                    paint_status_dot(ui, theme.accent_highlight, 3.0);
                                    ui.label(
                                        egui::RichText::new(name)
                                            .font(crate::theme::font(12.0))
                                            .strong()
                                            .color(theme.text_primary),
                                    );

                                    ui.label(
                                        egui::RichText::new(details)
                                            .font(crate::theme::font(11.5))
                                            .color(theme.text_secondary),
                                    );
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(
                                            egui::RichText::new(*impact)
                                                .font(crate::theme::mono_font(10.5))
                                                .color(theme.accent_warm),
                                        );
                                    });
                                });
                            }
                        }
                    }
                });

                ui.add_space(theme.space_sm);

                // 4. Graph Latency Breakdown
                if let Some(stats) = engine_stats {
                    let rep = &stats.latency_report;
                    if rep.total_latency_ms > 0.01 {
                        ui.label(
                            egui::RichText::new(tr!("AUDIO LATENCY BREAKDOWN"))
                                .font(crate::theme::font(11.0))
                                .strong()
                                .color(theme.text_muted),
                        );

                        let lat_frame = egui::Frame::NONE
                            .fill(theme.bg_card)
                            .stroke(Stroke::new(1.0_f32, theme.border_subtle))
                            .corner_radius(CornerRadius::same(theme.radius_md))
                            .inner_margin(egui::Margin::symmetric(theme.space_2xl as i8, theme.space_md as i8));

                        lat_frame.show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(tr!("Total End-to-End Latency"))
                                        .font(crate::theme::font(12.0))
                                        .color(theme.text_secondary),
                                );
                                ui.label(
                                    egui::RichText::new(format!("{:.1} ms", rep.total_latency_ms))
                                        .font(crate::theme::mono_font(12.0))
                                        .strong()
                                        .color(theme.accent_highlight),
                                );
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let pills = [
                                        ("Device", rep.output_device_latency_ms),
                                        ("Resampler", rep.resampler_latency_ms),
                                        ("Limiter", rep.limiter_lookahead_ms),
                                        ("Crossfeed", rep.crossfeed_delay_ms),
                                    ];
                                    for (lbl, val) in pills {
                                        if val > 0.05 {
                                            ui.label(
                                                egui::RichText::new(format!("{lbl}: {val:.1}ms"))
                                                    .font(crate::theme::mono_font(10.5))
                                                    .color(theme.text_muted),
                                            );
                                        }
                                    }
                                });
                            });
                        });

                        ui.add_space(theme.space_sm);
                    }
                }

                // 5. Detailed Signal Path Telemetry Table
                ui.label(
                    egui::RichText::new(tr!("SIGNAL SPECIFICATIONS"))
                        .font(crate::theme::font(11.0))
                        .strong()
                        .color(theme.text_muted),
                );

                let table_frame = egui::Frame::NONE
                    .fill(theme.bg_card)
                    .stroke(Stroke::new(1.0_f32, theme.border_subtle))
                    .corner_radius(CornerRadius::same(theme.radius_md))
                    .inner_margin(egui::Margin::symmetric(theme.space_2xl as i8, theme.space_lg as i8));

                table_frame.show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_sm);

                    let rows = [
                        ("Input Format", current_track.map(|t| t.format.as_str()).unwrap_or("Unknown")),
                        ("Source Sample Rate", &format!("{} Hz", playback_info.sample_rate)),
                        ("Source Precision", engine_stats.map(|s| s.dsp_precision.as_str()).unwrap_or("f32")),
                        ("Volume Gain", &format!("{:.1} dB ({})", 20.0 * (playback_info.volume + 0.00001).log10(), if playback_info.volume == 1.0 { "Unity 0 dB" } else { "Scaled" })),
                        ("DSD Transport", if playback_info.native_dsd_active { "Native Direct Bitstream (1-bit DSD)" } else if playback_info.dop_active { "DSD over PCM (DoP 24-bit)" } else { "PCM Direct" }),
                    ];

                    for (label, val) in rows {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(label)
                                    .font(crate::theme::font(12.0))
                                    .color(theme.text_secondary),
                            );
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(
                                    egui::RichText::new(val)
                                        .font(crate::theme::mono_font(11.5))
                                        .color(theme.text_primary),
                                );
                            });
                        });
                    }
                });

                ui.add_space(theme.space_lg);

                // 6. Negotiated output facts. This is the same list Settings
                // renders; the modal does not assemble an independent opinion
                // about exclusive access or the device.
                ui.label(
                    egui::RichText::new(tr!("NEGOTIATED OUTPUT"))
                        .font(crate::theme::font(11.0))
                        .strong()
                        .color(theme.text_muted),
                );
                crate::components::output_facts::render_fact_table(
                    ui,
                    &output_facts,
                    theme,
                );

                ui.add_space(6.0);

                // 7. Action Row: 1-Click Bit-Perfect Pure Direct Mode & DSP Studio Link
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_md, 0.0);
                    let is_pure = dsp_state.is_pure_direct();
                    let (btn_label, btn_col) = if is_pure {
                        (tr!("Exit Pure Direct (Restore DSP Chain)"), theme.accent_highlight)
                    } else {
                        (tr!("Enable Bit-Perfect Pure Direct"), theme.bit_perfect_green)
                    };

                    let pure_direct_btn = egui::Button::new(
                        egui::RichText::new(btn_label)
                            .font(crate::theme::font(12.0))
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(btn_col)
                    .corner_radius(CornerRadius::same(theme.radius_md))
                    .min_size(Vec2::new(175.0, 32.0));

                    if ui.add(pure_direct_btn).clicked() {
                        if is_pure {
                            dsp_state.exit_pure_direct(session);
                        } else {
                            dsp_state.enter_pure_direct(session);
                        }
                    }

                    let dsp_btn = egui::Button::new(
                        egui::RichText::new(tr!("Configure DSP Studio"))
                            .font(crate::theme::font(12.0))
                            .color(theme.text_primary),
                    )
                    .fill(theme.bg_pill)
                    .stroke(Stroke::new(1.0, theme.border_subtle))
                    .corner_radius(CornerRadius::same(theme.radius_md))
                    .min_size(Vec2::new(135.0, 32.0));

                    if ui.add(dsp_btn).clicked() {
                        dsp_state.eq_panel.is_open = true;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(tr!(Msg::S5E8250FB85D64C23)).clicked() {
                            close_clicked = true;
                        }
                    });
                });
                });
        },
    );
    if closed {
        open = false;
    }

    if close_clicked {
        open = false;
    }
    *is_open = open;
}

/// A filled status dot. The bundled fonts have no "●" glyph, so it is painted.
fn paint_status_dot(ui: &mut egui::Ui, color: Color32, radius: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(radius * 2.0 + 6.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), radius, color);
}

/// An upward warning triangle, replacing the missing "▲" glyph.
fn paint_warning_triangle(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let c = rect.center();
    let s = 7.5;
    painter.add(egui::Shape::convex_polygon(
        vec![
            Pos2::new(c.x, c.y - s),
            Pos2::new(c.x - s * 0.92, c.y + s * 0.72),
            Pos2::new(c.x + s * 0.92, c.y + s * 0.72),
        ],
        color,
        Stroke::NONE,
    ));
}
