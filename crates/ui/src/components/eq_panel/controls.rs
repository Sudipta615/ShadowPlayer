//! Shared panel controls: cards, the footer preamp/reset strip, the gradient
//! slider, and the toggle switch.

use super::{EqPanelActions, EqPanelState, EqTab};
use crate::theme::Theme;
use config::CrossfeedProfile;
use eframe::egui::{self, Color32, CornerRadius, Margin, Pos2, Rect, Sense, Stroke, Vec2};
use engine::dsp::limiter::LimiterMode;

/// A rounded, bordered card with the panel's standard inner spacing.
pub(super) fn card(ui: &mut egui::Ui, theme: &Theme, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE
        .fill(theme.bg_card_hover)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(theme.radius_lg))
        .inner_margin(Margin::symmetric(
            theme.space_3xl as i8,
            theme.space_2xl as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_sm);
                add_contents(ui);
            });
        });
}

// ─────────────────────────────────────────────────────────────────────────────
// 7. Footer Controls: Preamp Slider & Tab-Specific Reset Button
// ─────────────────────────────────────────────────────────────────────────────

/// Slider excursion that can represent the current shared preamp.
///
/// Normal mode is ±12 dB and Extended Range is ±24 dB. A curve computed
/// outside the panel (AutoEQ) can need more; the control widens just enough to
/// show it instead of clamping the knob while the readout tells the truth.
pub(super) fn preamp_display_limit(preamp_db: f32, extended_range: bool) -> f32 {
    if preamp_db.abs() > 24.0 {
        30.0
    } else if extended_range || preamp_db.abs() > 12.0 {
        24.0
    } else {
        12.0
    }
}

pub(super) fn render_footer_controls(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
    tab: EqTab,
) {
    // The preamp card and a fixed-width reset button need roughly 420 px
    // together. Below that, stacking them is what lets the panel actually stay
    // at the width the user dragged it to instead of being forced wider by its
    // content.
    let stacked = ui.available_width() < 420.0;

    if stacked {
        render_preamp_card(ui, state, actions, theme, None);
        ui.add_space(theme.space_xs);
        render_reset_button(ui, state, actions, theme, tab, None);
    } else {
        let preamp_w = (ui.available_width() - 95.0 - theme.space_xl).max(200.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);
            render_preamp_card(ui, state, actions, theme, Some(preamp_w));
            render_reset_button(ui, state, actions, theme, tab, Some(95.0));
        });
    }
}

/// The shared master preamp. `width` is the card's outer width; `None` fills
/// the available width (the stacked layout).
fn render_preamp_card(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
    width: Option<f32>,
) {
    // A preset or AutoEQ curve may compute a preamp outside the normal range.
    // Widen the control so the knob can actually represent it; otherwise the
    // readout said −18 dB while the slider was pinned at −12 dB, and touching
    // it silently reduced the headroom the curve needed.
    let preamp_max = preamp_display_limit(state.preamp_db, state.extended_range);
    let min_lbl = format!("-{preamp_max:.0}dB");
    let max_lbl = format!("+{preamp_max:.0}dB");

    egui::Frame::NONE
        .fill(theme.bg_card_hover)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(theme.radius_lg))
        .inner_margin(Margin::symmetric(
            theme.space_2xl as i8,
            theme.space_md as i8,
        ))
        .show(ui, |ui| {
            if let Some(width) = width {
                ui.set_width((width - 28.0).max(0.0));
            }
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xxs);

                // Row 1: Label + Info Button
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
                    ui.label(
                        egui::RichText::new(tr!("Preamp (Master)"))
                            .font(crate::theme::font(12.5))
                            .strong()
                            .color(theme.text_primary),
                    );
                    crate::components::info_box::render_info_icon_button(
                        ui,
                        "eq_footer_master_preamp",
                        &crate::components::info_box::INFO_MASTER_PREAMP,
                        theme,
                    );
                });

                // Row 2: Range Labels & Slider
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_md, 0.0);
                    ui.label(
                        egui::RichText::new(min_lbl)
                            .font(crate::theme::font(10.5))
                            .color(theme.text_muted),
                    );

                    let slider_w = (ui.available_width() - 50.0).max(80.0);
                    let mut p = state.preamp_db;
                    ui.allocate_ui_with_layout(
                        Vec2::new(slider_w, 20.0),
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
                        egui::RichText::new(max_lbl)
                            .font(crate::theme::font(10.5))
                            .color(theme.text_muted),
                    );
                });

                // Row 3: Readout
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{:+.1} dB", state.preamp_db))
                            .font(crate::theme::font(11.5))
                            .color(theme.text_secondary),
                    );
                });
            });
        });
}

fn render_reset_button(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
    tab: EqTab,
    width: Option<f32>,
) {
    let width = width.unwrap_or(ui.available_width());
    let (reset_rect, reset_resp) = ui.allocate_exact_size(Vec2::new(width, 40.0), Sense::click());
    reset_resp
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Reset this section"));
    let _ = reset_resp
        .clone()
        .on_hover_text(tr!(Msg::SEE997457CAFF5C1B));
    let rp = ui.painter_at(reset_rect);
    let bg = if reset_resp.hovered() {
        theme.accent
    } else {
        theme.bg_card_hover
    };
    rp.rect_filled(reset_rect, CornerRadius::same(theme.radius_lg), bg);
    rp.rect_stroke(
        reset_rect,
        CornerRadius::same(theme.radius_lg),
        Stroke::new(1.0, theme.border_card),
        egui::StrokeKind::Inside,
    );

    // Centre the icon and the label as one group, so the button looks the same
    // at 95 px and when it spans a narrow stacked footer.
    let text_color = if reset_resp.hovered() {
        Color32::WHITE
    } else {
        theme.text_primary
    };
    let icon_color = if reset_resp.hovered() {
        Color32::WHITE
    } else {
        theme.accent_highlight
    };
    let font = crate::theme::font(13.0);
    let galley = rp.layout_no_wrap("Reset".to_string(), font, text_color);
    let icon_size = 16.0;
    let gap = 6.0;
    let group_w = icon_size + gap + galley.size().x;
    let left = reset_rect.center().x - group_w / 2.0;
    let icon_rect = Rect::from_min_size(
        Pos2::new(left, reset_rect.center().y - icon_size / 2.0),
        Vec2::splat(icon_size),
    );
    crate::components::icons::draw_reset(ui, icon_rect, icon_color);
    rp.galley(
        Pos2::new(
            left + icon_size + gap,
            reset_rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        text_color,
    );

    let tooltip = match tab {
        EqTab::Graphic => "Reset Graphic EQ band sliders to 0.0 dB flat",
        EqTab::Tone => "Reset Bass, Treble, Stereo Width, and Balance to defaults",
        EqTab::Parametric => "Flatten every parametric band and reset the shared preamp",
        EqTab::Dynamics => "Reset the limiter and all three compressor bands to defaults",
        EqTab::Filters => "Reset crossfeed and correction to their bypassed defaults",
        EqTab::Advanced => "Reset Advanced dynamic range and DSP processing modes to defaults",
        _ => "Reset settings for this tab",
    };

    if reset_resp.on_hover_text(tooltip).clicked() {
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

pub(super) fn gradient_horizontal_slider(
    ui: &mut egui::Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    default_val: f32,
    theme: &Theme,
) -> bool {
    let desired_size = Vec2::new(ui.available_width(), 18.0);
    let (rect, resp) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
    let mut changed = false;

    let min = *range.start();
    let max = *range.end();
    resp.widget_info(|| {
        egui::WidgetInfo::slider(
            true,
            f64::from(((*value - min) / (max - min)).clamp(0.0, 1.0)),
            format!("Setting slider, {value:.2}"),
        )
    });

    if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let t = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let new_val = min + t * (max - min);
            if (new_val - *value).abs() > 0.001 {
                *value = new_val;
                changed = true;
            }
        }
    }

    if resp.double_clicked() {
        *value = default_val;
        changed = true;
    }

    // Paint background track using theme token (clearly visible in both dark & light modes)
    let track_rect = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), 4.0));
    ui.painter().rect_filled(
        track_rect,
        CornerRadius::same(theme.radius_xs),
        theme.slider_track,
    );

    // Paint active track up to thumb (single jade hue).
    let thumb_t = ((*value - min) / (max - min)).clamp(0.0, 1.0);
    let thumb_x = rect.left() + thumb_t * rect.width();
    let filled_w = (thumb_x - track_rect.min.x).max(0.0);
    if filled_w > 0.0 {
        let fill_rect = Rect::from_min_max(
            Pos2::new(track_rect.min.x, track_rect.min.y),
            Pos2::new(track_rect.min.x + filled_w, track_rect.max.y),
        );
        ui.painter().rect_filled(
            fill_rect,
            CornerRadius::same(theme.radius_xs),
            theme.accent_highlight,
        );
    }

    // Paint circular thumb
    let thumb_pos = Pos2::new(thumb_x, rect.center().y);
    let is_hovered = resp.hovered() || resp.dragged();

    ui.painter().circle_filled(
        thumb_pos,
        if is_hovered { 8.5 } else { 7.0 },
        theme
            .accent_highlight
            .gamma_multiply(if is_hovered { 0.35 } else { 0.18 }),
    );
    ui.painter().circle_filled(
        thumb_pos,
        if is_hovered { 5.8 } else { 5.0 },
        Color32::WHITE,
    );
    ui.painter().circle_stroke(
        thumb_pos,
        if is_hovered { 5.8 } else { 5.0 },
        Stroke::new(1.6, theme.accent_highlight),
    );

    changed
}

/// The panel's switch: a rounded track with a sliding thumb.
///
/// Shared with the creative-effects panel, so both switches look and feel
/// the same.
pub fn custom_toggle_switch(
    ui: &mut egui::Ui,
    value: &mut bool,
    tooltip: &str,
    theme: &Theme,
) -> bool {
    let desired_size = Vec2::new(38.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(desired_size, Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, *value, tooltip));
    let mut changed = false;

    if resp.clicked() {
        *value = !*value;
        changed = true;
    }

    let p = ui.painter_at(rect);
    let bg = if *value {
        theme.accent
    } else {
        theme.border_card
    };
    p.rect_filled(rect, CornerRadius::same(theme.radius_lg), bg);

    let thumb_x = if *value {
        rect.right() - 10.0
    } else {
        rect.left() + 10.0
    };
    let thumb_pos = Pos2::new(thumb_x, rect.center().y);
    p.circle_filled(thumb_pos, 7.5, Color32::WHITE);

    resp.on_hover_text(tooltip);
    changed
}

#[cfg(test)]
mod tests {
    use super::preamp_display_limit;

    #[test]
    fn preamp_slider_widens_to_show_a_preset_that_needs_more_headroom() {
        assert_eq!(preamp_display_limit(0.0, false), 12.0);
        assert_eq!(preamp_display_limit(-12.0, false), 12.0);
        assert_eq!(preamp_display_limit(-12.5, false), 24.0);
        assert_eq!(preamp_display_limit(-24.0, false), 24.0);
        assert_eq!(preamp_display_limit(-25.0, false), 30.0);
        assert_eq!(preamp_display_limit(0.0, true), 24.0);
        assert_eq!(preamp_display_limit(26.0, true), 30.0);
    }
}
