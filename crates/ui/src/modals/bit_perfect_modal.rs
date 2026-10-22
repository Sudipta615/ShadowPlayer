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

                // 3. Detailed Signal Path Telemetry Table
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

                // 4. Negotiated output facts. This is the same list Settings
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

                // 5. Action Row: 1-Click Bit-Perfect Pure Direct Mode
                ui.horizontal(|ui| {
                    let pure_direct_btn = egui::Button::new(
                        egui::RichText::new(tr!("Enable Bit-Perfect Pure Direct"))
                            .font(crate::theme::font(12.5))
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(theme.bit_perfect_green)
                    .corner_radius(CornerRadius::same(theme.radius_md))
                    .min_size(Vec2::new(ui.available_width() - 90.0, 32.0));

                    if ui.add(pure_direct_btn).clicked() {
                        // Reset session to bit-perfect unity
                        session.set_volume(1.0);
                        session.set_preamp(0.0);
                        session.set_speed(1.0);
                        session.set_eq_enabled(false);
                        session.set_graphic_eq_enabled(false);
                        session.set_limiter_enabled(false);
                        session.set_compressor_enabled(false);
                        session.set_spatial_enabled(false);
                        session.set_stereo_enhancer_enabled(false);
                        session.set_bit_perfect(true);

                        // Sync UI state
                        dsp_state.eq_panel.eq_enabled = false;
                        dsp_state.eq_panel.graphic_enabled = false;
                        dsp_state.eq_panel.limiter_enabled = false;
                        dsp_state.eq_panel.compressor_enabled = false;
                        dsp_state.eq_panel.preamp_db = 0.0;
                        dsp_state.settings_tab.bit_perfect = true;
                    }

                    if ui.button(tr!(Msg::S5E8250FB85D64C23)).clicked() {
                        close_clicked = true;
                    }
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
