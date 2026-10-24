//! Shared panel controls: the footer preamp/reset strip, the gradient slider,
//! and the toggle switch.

use super::presets::EqPreset;
use super::{EqPanelActions, EqPanelState, EqTab};
use crate::theme::Theme;
use eframe::egui::{self, Color32, CornerRadius, Margin, Pos2, Rect, Sense, Stroke, Vec2};

// ─────────────────────────────────────────────────────────────────────────────
// 7. Footer Controls: Preamp Slider & Tab-Specific Reset Button
// ─────────────────────────────────────────────────────────────────────────────

pub(super) fn render_footer_controls(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
    tab: EqTab,
) {
    let avail_w = ui.available_width();
    let reset_btn_w = 95.0;
    let preamp_card_w = (avail_w - reset_btn_w - 12.0).max(200.0);

    let preamp_max = if state.extended_range { 24.0 } else { 12.0 };
    let min_lbl = if state.extended_range {
        "-24dB"
    } else {
        "-12dB"
    };
    let max_lbl = if state.extended_range {
        "+24dB"
    } else {
        "+12dB"
    };

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);

        // Preamp Card (All-in-one control shared across tabs)
        egui::Frame::NONE
            .fill(theme.bg_card_hover)
            .stroke(Stroke::new(1.0, theme.border_card))
            .corner_radius(CornerRadius::same(theme.radius_lg))
            .inner_margin(Margin::symmetric(
                theme.space_2xl as i8,
                theme.space_md as i8,
            ))
            .show(ui, |ui| {
                ui.set_width(preamp_card_w - 28.0);
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
                        let mut p = state.graphic_preamp_db;
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
                                    state.graphic_preamp_db = p;
                                    state.preamp_db = p;
                                    actions.graphic_preamp_changed = Some(p);
                                    actions.eq_preamp_changed = Some(p);
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
                            egui::RichText::new(format!("{:+.1} dB", state.graphic_preamp_db))
                                .font(crate::theme::font(11.5))
                                .color(theme.text_secondary),
                        );
                    });
                });
            });

        // Tab-Specific Reset Button
        let (reset_rect, reset_resp) =
            ui.allocate_exact_size(Vec2::new(reset_btn_w, 58.0), Sense::click());
        reset_resp.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, true, "Reset this section")
        });
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

        // High quality SVG Reset icon and Reset text
        let icon_rect = Rect::from_center_size(
            Pos2::new(reset_rect.center().x + 22.0, reset_rect.center().y),
            Vec2::splat(16.0),
        );
        crate::components::icons::draw_reset(ui, icon_rect, theme.accent_highlight);

        rp.text(
            Pos2::new(reset_rect.center().x - 12.0, reset_rect.center().y),
            egui::Align2::CENTER_CENTER,
            "Reset",
            crate::theme::font(13.0),
            theme.text_primary,
        );

        let tooltip = match tab {
            EqTab::Graphic => "Reset Graphic EQ band sliders to 0.0 dB flat",
            EqTab::Tone => "Reset Bass, Treble, Stereo Width, and Balance to defaults",
            EqTab::Advanced => "Reset Advanced dynamic range and DSP processing modes to defaults",
            _ => "Reset settings for this tab",
        };

        if reset_resp.on_hover_text(tooltip).clicked() {
            match tab {
                EqTab::Graphic => {
                    // Reset Graphic EQ bands and master preamp to 0.0 dB flat
                    let num = state.graphic_layout.num_bands();
                    for i in 0..num {
                        state.graphic_gains[i] = 0.0;
                        actions.graphic_slider_changed = Some(i);
                    }
                    state.graphic_preamp_db = 0.0;
                    actions.graphic_preamp_changed = Some(0.0);
                    state.selected_preset = EqPreset::Flat;
                    state.selected_custom_preset_idx = None;
                    actions.graphic_reset = true;
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

                    state.graphic_preamp_db = 0.0;
                    actions.graphic_preamp_changed = Some(0.0);
                }
                EqTab::Advanced => {
                    // Reset Advanced settings to audiophile defaults
                    state.extended_range = false;
                    state.auto_headroom = true;
                    actions.eq_auto_headroom_changed = Some(true);
                    state.midside_eq = false;
                    actions.midside_eq_changed = Some(false);
                    state.dither_enabled = false;
                    actions.dither_enabled_changed = Some(false);
                }
                _ => {}
            }
        }
    });
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
