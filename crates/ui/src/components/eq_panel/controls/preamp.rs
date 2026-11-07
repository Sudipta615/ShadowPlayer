//! The master preamp card and the per-tab reset control.
//!
//! Both are painted by hand where the stock widgets would put the hover fill
//! over the label; the footer places them side by side or stacked.
use super::{gradient_horizontal_slider, preamp_display_limit};
use crate::components::eq_panel::{EqPanelActions, EqPanelState, EqTab};
use crate::theme::Theme;
use audio_session::engine::dsp::limiter::LimiterMode;
use config::CrossfeedProfile;
use eframe::egui::{self, CornerRadius, Margin, Pos2, Rect, Sense, Stroke, Vec2};

/// The shared master preamp: one compact card with a label, a readout, and a
/// single slider row.
pub(in crate::components::eq_panel) fn render_preamp_card(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
    width: Option<f32>,
) {
    // A preset or AutoEQ curve may compute a preamp outside the normal range.
    // Widen the control so the knob can actually represent it; otherwise the
    // readout says −18 dB while the slider is pinned at −12 dB.
    let preamp_max = preamp_display_limit(state.preamp_db, state.extended_range);
    let min_lbl = format!("-{preamp_max:.0}");
    let max_lbl = format!("+{preamp_max:.0}");

    egui::Frame::NONE
        .fill(theme.bg_app)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(theme.radius_lg))
        .inner_margin(Margin::symmetric(
            theme.space_xl as i8,
            theme.space_sm as i8,
        ))
        .show(ui, |ui| {
            if let Some(width) = width {
                ui.set_width((width - 24.0).max(0.0));
            }
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xxs);

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_xs, 0.0);
                    ui.label(
                        egui::RichText::new(tr!("Preamp (Master)"))
                            .font(crate::theme::font(11.5))
                            .strong()
                            .color(theme.text_primary),
                    );
                    crate::components::info_box::render_info_icon_button(
                        ui,
                        "eq_footer_master_preamp",
                        &crate::components::info_box::INFO_MASTER_PREAMP,
                        theme,
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let id = ui.make_persistent_id("eq_preamp_text_edit");
                        let is_editing =
                            ui.data(|d| d.get_temp::<bool>(id.with("editing")).unwrap_or(false));
                        if is_editing {
                            let mut text = ui
                                .data(|d| d.get_temp::<String>(id.with("text")))
                                .unwrap_or_else(|| format!("{:.1}", state.preamp_db));
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut text)
                                    .desired_width(50.0)
                                    .font(crate::theme::mono_font(10.0)),
                            );
                            if resp.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                if let Ok(parsed) = text.trim().parse::<f32>() {
                                    state.set_preamp(parsed.clamp(-preamp_max, preamp_max));
                                    actions.graphic_eq_changed = true;
                                }
                                ui.data_mut(|d| d.insert_temp(id.with("editing"), false));
                            } else {
                                ui.data_mut(|d| d.insert_temp(id.with("text"), text));
                            }
                        } else {
                            let resp = ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!("{:+.1} dB", state.preamp_db))
                                        .font(crate::theme::font(10.5))
                                        .color(theme.text_secondary),
                                )
                                .sense(Sense::click()),
                            );
                            if resp.on_hover_text("Click to type exact dB value").clicked() {
                                ui.data_mut(|d| {
                                    d.insert_temp(id.with("editing"), true);
                                    d.insert_temp(
                                        id.with("text"),
                                        format!("{:.1}", state.preamp_db),
                                    );
                                });
                            }
                        }
                    });
                });

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
                    ui.label(
                        egui::RichText::new(&min_lbl)
                            .font(crate::theme::font(9.5))
                            .color(theme.text_muted),
                    );

                    let slider_w = (ui.available_width() - 34.0).max(60.0);
                    let mut p = state.preamp_db;
                    ui.allocate_ui_with_layout(
                        Vec2::new(slider_w, 18.0),
                        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                        |ui| {
                            if gradient_horizontal_slider(
                                ui,
                                &mut p,
                                -preamp_max..=preamp_max,
                                0.0,
                                theme,
                            ) {
                                state.set_preamp(p);
                                actions.graphic_eq_changed = true;
                            }
                        },
                    );

                    ui.label(
                        egui::RichText::new(&max_lbl)
                            .font(crate::theme::font(9.5))
                            .color(theme.text_muted),
                    );
                });
            });
        });
}

/// The per-tab reset control.
///
/// It is painted by hand for the same reason the tab buttons are: it has to
/// look right at 96 px beside the preamp and at full width when the footer
/// stacks, and the hover fill must sit *behind* the icon and label. Painting a
/// fill after `ui.add(button)` would tint the content instead.
pub(in crate::components::eq_panel) fn render_reset_button(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
    tab: EqTab,
    width: Option<f32>,
) {
    let width = width.unwrap_or(ui.available_width());
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 44.0), Sense::click());
    let label = tr!(Msg::SEE997457CAFF5C1B);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));

    // Snap the highlight out rather than crossfading it: a fade leaves the
    // button half-lit while the pointer is already on the next control.
    let hover_t = if resp.hovered() {
        ui.ctx().animate_bool_with_time(
            resp.id.with("eq_reset_hover"),
            true,
            theme.motion_time(0.1),
        )
    } else {
        0.0
    };

    let painter = ui.painter_at(rect);
    painter.rect_filled(
        rect,
        CornerRadius::same(theme.radius_lg),
        crate::theme::lerp_color(theme.bg_card_hover, theme.accent, hover_t),
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(theme.radius_lg),
        Stroke::new(
            1.0,
            crate::theme::lerp_color(theme.border_card, theme.accent, hover_t),
        ),
        egui::StrokeKind::Inside,
    );

    // Centre the icon and the label as one group so the button reads the same
    // at 96 px and when it spans a stacked footer.
    let text_color = crate::theme::lerp_color(
        theme.text_primary,
        crate::theme::on_fill(theme.accent),
        hover_t,
    );
    let icon_color = crate::theme::lerp_color(
        theme.accent_highlight,
        crate::theme::on_fill(theme.accent),
        hover_t,
    );
    let font = crate::theme::font(12.0);
    let galley = painter.layout_no_wrap(tr!("Reset").to_owned(), font, text_color);
    let icon_size = 15.0;
    let gap = 6.0;
    let group_w = icon_size + gap + galley.size().x;
    let left = rect.center().x - group_w / 2.0;
    let icon_rect = Rect::from_min_size(
        Pos2::new(left, rect.center().y - icon_size / 2.0),
        Vec2::splat(icon_size),
    );
    crate::components::icons::draw_reset(ui, icon_rect, icon_color);
    painter.galley(
        Pos2::new(
            left + icon_size + gap,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        text_color,
    );

    if resp.on_hover_text(label).clicked() {
        apply_reset(state, actions, tab);
    }
}

fn apply_reset(state: &mut EqPanelState, actions: &mut EqPanelActions, tab: EqTab) {
    match tab {
        EqTab::Graphic => {
            // One snapshot resets the whole curve, not 64 queued
            // single-band commands racing the engine's tick budget.
            state.reset_graphic_eq();
            actions.graphic_eq_changed = true;
        }
        EqTab::Tone => {
            // Reset Tone shelves, stereo width, balance, and master preamp
            state.bass_shelf_db = 0.0;
            actions.bass_shelf_changed = Some(0.0);

            state.treble_shelf_db = 0.0;
            actions.treble_shelf_changed = Some(0.0);

            state.stereo_width = 1.0;
            actions.stereo_width_changed = Some(1.0);

            state.balance = 0.0;
            actions.balance_changed = Some(0.0);

            state.set_preamp(0.0);
            actions.graphic_eq_changed = true;
        }
        EqTab::Advanced => {
            // Reset Advanced settings to the new honest defaults:
            // no hidden static attenuation, but the true-peak guard
            // remains available and on for new state.
            // Do not hide a curve that still exceeds the standard
            // range: the engine would keep applying it while the
            // canvas drew a clamped picture.
            state.extended_range = state.needs_extended_range();
            state.auto_headroom = false;
            actions.eq_auto_headroom_changed = Some(false);
            state.midside_eq = false;
            actions.midside_eq_changed = Some(false);
            state.dither_enabled = false;
            actions.dither_enabled_changed = Some(false);
        }
        EqTab::Parametric => {
            for band in &mut state.bands {
                band.gain_db = 0.0;
            }
            state.set_preamp(0.0);
            actions.eq_bands_changed = true;
            actions.graphic_eq_changed = true;
        }
        EqTab::Dynamics => {
            state.limiter_enabled = true;
            actions.limiter_enabled_changed = Some(true);
            state.limiter_mode = LimiterMode::Transparent;
            actions.limiter_mode_changed = Some(LimiterMode::Transparent);
            state.limiter_true_peak = true;
            actions.limiter_true_peak_changed = Some(true);
            state.limiter_lookahead_ms = 5.0;
            state.limiter_attack_ms = 0.3;
            state.limiter_release_ms = 100.0;
            state.limiter_ceiling_db = -1.0;
            state.limiter_soft_clip = false;
            actions.limiter_params_changed = true;
            state.compressor_enabled = false;
            actions.compressor_enabled_changed = Some(false);
            state.comp_threshold = [-24.0, -18.0, -18.0];
            state.comp_ratio = [4.0, 3.0, 3.0];
            state.comp_attack_ms = [10.0, 10.0, 5.0];
            state.comp_release_ms = [100.0, 100.0, 80.0];
            state.comp_makeup_db = [0.0; 3];
            state.comp_knee_db = [3.0; 3];
            state.comp_stereo_link = [true; 3];
            state.comp_band = 0;
            actions.comp_params_reset = true;
        }
        EqTab::Filters => {
            state.crossfeed_enabled = false;
            actions.crossfeed_enabled_changed = Some(false);
            state.crossfeed_profile = CrossfeedProfile::Bauer;
            actions.crossfeed_profile_changed = Some(CrossfeedProfile::Bauer);
            state.crossfeed_freq = 700.0;
            state.crossfeed_q = 0.707;
            state.crossfeed_delay_ms = 0.3;
            actions.crossfeed_custom_changed = true;
            state.correction_enabled = false;
            actions.correction_enabled_changed = Some(false);
            state.correction_depth = 1.0;
            actions.correction_depth_changed = Some(1.0);
        }
        EqTab::Effects => {}
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// UI Primitives: Gradient Slider, Toggle Switch, Spline Math, Vector Icons
// ─────────────────────────────────────────────────────────────────────────────
