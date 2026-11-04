//! Shared panel controls: cards, the footer preamp/reset strip, the gradient
//! slider, and the toggle switch.

use super::{EqPanelActions, EqPanelState, EqTab};
use crate::theme::Theme;
use config::CrossfeedProfile;
use eframe::egui::{self, Color32, CornerRadius, Margin, Pos2, Rect, Sense, Stroke, Vec2};
use engine::dsp::limiter::LimiterMode;

/// A rounded, bordered card with the panel's standard inner spacing.
///
/// The fill is the app background rather than a raised surface: sections on
/// the panel already sit on `bg_window`, so one step *down* separates them
/// without stacking two nearly identical greys on top of each other.
pub(super) fn card(ui: &mut egui::Ui, theme: &Theme, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE
        .fill(theme.bg_app)
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
    // The preamp card and the reset button fit side by side on a normal
    // window. Below that they stack, so the footer never imposes a minimum
    // width on a panel the user deliberately narrowed.
    let stacked = ui.available_width() < 360.0;

    if stacked {
        render_preamp_card(ui, state, actions, theme, None);
        ui.add_space(theme.space_xs);
        render_reset_button(ui, state, actions, theme, tab, None);
    } else {
        let reset_w = 96.0;
        let preamp_w = (ui.available_width() - reset_w - theme.space_md).max(180.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_md, 0.0);
            render_preamp_card(ui, state, actions, theme, Some(preamp_w));
            render_reset_button(ui, state, actions, theme, tab, Some(reset_w));
        });
    }
}

/// The shared master preamp: one compact card with a label, a readout, and a
/// single slider row.
fn render_preamp_card(
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
                        ui.label(
                            egui::RichText::new(format!("{:+.1} dB", state.preamp_db))
                                .font(crate::theme::font(10.5))
                                .color(theme.text_secondary),
                        );
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
fn render_reset_button(
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

    let is_active = resp.hovered() || resp.dragged();
    let anim_t = ui.ctx().animate_bool_with_time(
        resp.id.with("eq_slider_active"),
        is_active,
        theme.motion_time(0.12),
    );

    // Paint background track using theme token
    let track_h = egui::lerp(4.0..=5.0, anim_t);
    let track_rect = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), track_h));
    ui.painter().rect_filled(
        track_rect,
        CornerRadius::same(theme.radius_xs),
        theme.slider_track,
    );

    // Center 0.0 dB tick mark if 0.0 dB lies within range
    if min < 0.0 && max > 0.0 {
        let zero_t = ((0.0 - min) / (max - min)).clamp(0.0, 1.0);
        let zero_x = rect.left() + zero_t * rect.width();
        ui.painter().line_segment(
            [
                Pos2::new(zero_x, rect.center().y - 4.0),
                Pos2::new(zero_x, rect.center().y + 4.0),
            ],
            Stroke::new(1.0, theme.text_muted.gamma_multiply(0.4)),
        );
    }

    // Paint active track up to thumb
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

    // Paint circular thumb with animated halo
    let thumb_pos = Pos2::new(thumb_x, rect.center().y);
    let halo_radius = egui::lerp(7.0..=9.5, anim_t);
    let halo_alpha = egui::lerp(0.18..=0.35, anim_t);
    let inner_radius = egui::lerp(5.0..=6.0, anim_t);

    ui.painter().circle_filled(
        thumb_pos,
        halo_radius,
        theme.accent_highlight.gamma_multiply(halo_alpha),
    );
    ui.painter()
        .circle_filled(thumb_pos, inner_radius, Color32::WHITE);
    ui.painter().circle_stroke(
        thumb_pos,
        inner_radius,
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

    let anim_t = ui.ctx().animate_bool_with_time(
        resp.id.with("eq_toggle_thumb"),
        *value,
        theme.motion_time(0.12),
    );
    let hover_t = ui.ctx().animate_bool_with_time(
        resp.id.with("eq_toggle_hover"),
        resp.hovered(),
        theme.motion_time(0.12),
    );

    let p = ui.painter_at(rect);
    let bg_off = crate::theme::lerp_color(theme.border_card, theme.bg_pill_hover, hover_t);
    let bg = crate::theme::lerp_color(bg_off, theme.accent, anim_t);
    p.rect_filled(rect, CornerRadius::same(theme.radius_lg), bg);

    let stroke_col = crate::theme::lerp_color(theme.border_card, theme.accent_highlight, anim_t);
    p.rect_stroke(
        rect,
        CornerRadius::same(theme.radius_lg),
        Stroke::new(1.0, stroke_col),
        egui::StrokeKind::Inside,
    );

    let thumb_x = egui::lerp((rect.left() + 10.0)..=(rect.right() - 10.0), anim_t);
    let thumb_pos = Pos2::new(thumb_x, rect.center().y);

    if hover_t > 0.01 {
        let halo_r = egui::lerp(7.5..=9.5, hover_t);
        let halo_col =
            crate::theme::lerp_color(theme.border_card, theme.accent, anim_t).gamma_multiply(0.25);
        p.circle_filled(thumb_pos, halo_r, halo_col);
    }

    p.circle_filled(thumb_pos, 7.5, Color32::WHITE);

    resp.on_hover_text(tooltip);
    changed
}

/// One option in a small mutually-exclusive set (limiter mode, compressor
/// band, crossfeed view).
///
/// `Button::fill` applies to every widget state, so a filled selected pill in
/// the same row also suppressed egui's hover feedback on its unselected
/// neighbours. Painting the pill by hand restores that feedback and lets the
/// selected label pick an ink colour that stays readable on the accent.
pub(super) fn choice_pill(
    ui: &mut egui::Ui,
    label: &str,
    selected: bool,
    accent: Color32,
    theme: &Theme,
) -> egui::Response {
    let text_color = if selected {
        crate::theme::on_fill(accent)
    } else {
        theme.text_secondary
    };
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_owned(), crate::theme::font(10.8), text_color);
    let size = Vec2::new(galley.size().x + 18.0, 24.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, label));

    // Hover snaps out instead of crossfading, so the pill the pointer left and
    // the one it entered are never lit at the same time.
    let hover_t = if resp.hovered() && !selected {
        ui.ctx()
            .animate_bool_with_time(resp.id.with("choice_hover"), true, theme.motion_time(0.1))
    } else {
        0.0
    };

    let painter = ui.painter_at(rect);
    let radius = CornerRadius::same(theme.radius_sm);
    if selected {
        painter.rect_filled(rect, radius, accent);
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(1.0, crate::theme::with_alpha(accent, 200)),
            egui::StrokeKind::Inside,
        );
    } else if hover_t > 0.01 {
        painter.rect_filled(rect, radius, theme.bg_card_hover.gamma_multiply(hover_t));
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(1.0, theme.border_card.gamma_multiply(hover_t)),
            egui::StrokeKind::Inside,
        );
    }
    if resp.has_focus() {
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(1.5, theme.accent_highlight),
            egui::StrokeKind::Inside,
        );
    }
    painter.galley(
        Pos2::new(
            rect.center().x - galley.size().x * 0.5,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        text_color,
    );

    resp
}

// ─────────────────────────────────────────────────────────────────────────────
// Tab Switcher Navigation
// ─────────────────────────────────────────────────────────────────────────────
fn is_tab_stage_active(tab: EqTab, state: &EqPanelState) -> bool {
    match tab {
        EqTab::Graphic => state.eq_enabled && state.eq_mode == config::EqMode::Graphic,
        EqTab::Parametric => state.eq_enabled && state.eq_mode == config::EqMode::Parametric,
        EqTab::Tone => {
            state.bass_shelf_db.abs() > 0.05
                || state.treble_shelf_db.abs() > 0.05
                || (state.stereo_width - 1.0).abs() > 0.05
                || state.balance.abs() > 0.05
                || state.midside_eq
        }
        EqTab::Dynamics => state.limiter_enabled || state.compressor_enabled,
        EqTab::Filters => state.crossfeed_enabled || state.correction_enabled,
        EqTab::Effects => state.fx.any_enabled(),
        EqTab::Advanced => false,
    }
}

pub(super) fn render_tab_navigation(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    let tab_items = [
        (EqTab::Graphic, tr!("Graphic EQ")),
        (EqTab::Parametric, tr!("Parametric")),
        (EqTab::Tone, tr!("Tone")),
        (EqTab::Dynamics, tr!("Dynamics")),
        (EqTab::Filters, tr!("Filters")),
        (EqTab::Effects, tr!("Effects")),
        (EqTab::Advanced, tr!("Advanced")),
    ];

    egui::Frame::NONE
        .fill(theme.bg_pill)
        .stroke(Stroke::new(1.0, theme.border_subtle))
        .corner_radius(CornerRadius::same(theme.radius_lg))
        .inner_margin(Margin::symmetric(
            theme.space_xs as i8,
            theme.space_xs as i8,
        ))
        .show(ui, |ui| {
            // The strip stays interactive while the studio is bypassed: a tab
            // that looks normal but ignores clicks is a dead control. The
            // buttons dim themselves instead, so the bypassed state is still
            // visible without trapping the user on one tab.
            //
            // Tabs are content-sized and wrap. A fixed seven-column grid
            // needed ~500 px before it would shrink, which is what kept
            // snapping the window back to a wide minimum.
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);

                for (tab, label) in tab_items {
                    render_tab_button(ui, state, actions, theme, tab, &label);
                }
            });
        });
}

/// One content-width tab: it measures its own label so the row never forces a
/// minimum width, and it paints background before text so a hover fill cannot
/// cover the label.
fn render_tab_button(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
    tab: EqTab,
    label: &str,
) {
    let is_sel = state.active_tab == tab;
    let is_stage_on = is_tab_stage_active(tab, state);
    // A bypassed studio dims its strip instead of freezing it: the tabs still
    // switch, but no tab claims an engaged look while no stage is running.
    let studio_on = state.eq_enabled;
    let font = crate::theme::font(if is_sel { 11.5 } else { 11.0 });
    let text_color = if is_sel && studio_on {
        crate::theme::on_fill(theme.accent)
    } else if studio_on {
        theme.text_secondary
    } else {
        theme.text_dim
    };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font, text_color);
    let dot_space = if is_stage_on { 12.0 } else { 0.0 };
    let size = Vec2::new(galley.size().x + 20.0 + dot_space, 30.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, is_sel, label));

    // Hover snaps out rather than fading: during a crossfade the tab being
    // left and the tab being entered were both lit, which read as two tabs
    // selected at once.
    let hover_t = if resp.hovered() && !is_sel {
        ui.ctx()
            .animate_bool_with_time(resp.id.with("eq_tab_hover"), true, theme.motion_time(0.1))
    } else {
        0.0
    };

    let painter = ui.painter();
    if is_sel {
        let (fill, stroke) = if studio_on {
            (theme.accent, theme.accent_highlight)
        } else {
            (theme.bg_card_hover, theme.border_subtle)
        };
        painter.rect_filled(rect, theme.control_radius(), fill);
        painter.rect_stroke(
            rect,
            theme.control_radius(),
            Stroke::new(1.0, stroke),
            egui::StrokeKind::Inside,
        );
    } else if hover_t > 0.01 {
        painter.rect_filled(
            rect,
            theme.control_radius(),
            theme.bg_card_hover.gamma_multiply(hover_t),
        );
        painter.rect_stroke(
            rect,
            theme.control_radius(),
            Stroke::new(1.0, theme.border_subtle.gamma_multiply(hover_t)),
            egui::StrokeKind::Inside,
        );
    }

    // A stage dot marks tabs whose processing stage is actually engaged, so
    // "Dynamics" and "Filters" do not look idle while the limiter or
    // crossfeed is running.
    if is_stage_on {
        painter.circle_filled(
            Pos2::new(rect.right() - 9.0, rect.center().y),
            2.6,
            if !studio_on {
                theme.text_dim
            } else if is_sel {
                crate::theme::on_fill(theme.accent)
            } else {
                theme.accent_warm
            },
        );
    }

    painter.galley(
        Pos2::new(
            rect.center().x - galley.size().x * 0.5 - dot_space * 0.5,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        text_color,
    );

    if resp.clicked() {
        state.active_tab = tab;
        match tab {
            EqTab::Graphic => {
                state.set_eq_mode(config::EqMode::Graphic);
                actions.eq_mode_changed = true;
            }
            EqTab::Parametric => {
                state.set_eq_mode(config::EqMode::Parametric);
                actions.eq_mode_changed = true;
                actions.eq_bands_changed = true;
            }
            _ => {}
        }
    }
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
