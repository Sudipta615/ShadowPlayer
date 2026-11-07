//! The tab switcher: stage indicators and the hand-painted tab buttons.
use super::choice_pill;
use crate::components::eq_panel::{EqPanelActions, EqPanelState, EqTab};
use crate::theme::Theme;
use eframe::egui::{self, CornerRadius, Margin, Pos2, Sense, Stroke, Vec2};

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

pub(in crate::components::eq_panel) fn render_tab_navigation(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    // The signal flow follows the true physical DSP pipeline:
    //   Stage 1: Filters (Headphone crossfeed & Convolution room correction)
    //   Stage 2: Equalizer (Graphic 10-64 band ladder or Parametric AutoEQ)
    //   Stage 3: Tone & Acoustics (Bass/Treble shelves, M/S, Stereo width)
    //   Stage 4: Creative Effects (Chorus, Flanger, Phaser, Delay, Tube saturation)
    //   Stage 5: Dynamics (Multiband compressor & True-Peak mastering limiter)
    // Followed by Advanced settings (TPDF dither, Auto-headroom, AutoEQ catalog).
    let eq_tab = if state.eq_mode == config::EqMode::Parametric {
        EqTab::Parametric
    } else {
        EqTab::Graphic
    };
    let stages = [
        (EqTab::Filters, tr!("Filters")),
        (eq_tab, tr!("Equalizer")),
        (EqTab::Tone, tr!("Tone")),
        (EqTab::Effects, tr!("Effects")),
        (EqTab::Dynamics, tr!("Dynamics")),
    ];

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xs);

        egui::Frame::NONE
            .fill(theme.bg_pill)
            .stroke(Stroke::new(1.0, theme.border_subtle))
            .corner_radius(CornerRadius::same(theme.radius_lg))
            .inner_margin(Margin::symmetric(
                theme.space_xs as i8,
                theme.space_xs as i8,
            ))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);

                    // Render serial DSP stages with visual flow arrows
                    for (idx, (tab, label)) in stages.iter().enumerate() {
                        if idx > 0 {
                            ui.label(
                                egui::RichText::new("→")
                                    .font(crate::theme::font(10.5))
                                    .color(theme.text_dim.gamma_multiply(0.4)),
                            );
                        }
                        let is_eq_stage = *tab == EqTab::Graphic || *tab == EqTab::Parametric;
                        let is_sel = if is_eq_stage {
                            state.active_tab == EqTab::Graphic
                                || state.active_tab == EqTab::Parametric
                        } else {
                            state.active_tab == *tab
                        };
                        let display_label = format!("{}. {}", idx + 1, label);
                        render_tab_button_custom(
                            ui,
                            state,
                            actions,
                            theme,
                            *tab,
                            &display_label,
                            is_sel,
                        );
                    }

                    // Visual separator between the DSP signal stages and Advanced
                    let (sep_rect, _) =
                        ui.allocate_exact_size(Vec2::new(9.0, 20.0), Sense::hover());
                    ui.painter().line_segment(
                        [sep_rect.center_top(), sep_rect.center_bottom()],
                        Stroke::new(1.0, theme.border_subtle),
                    );

                    render_tab_button(ui, state, actions, theme, EqTab::Advanced, &tr!("Advanced"));
                });
            });

        // ── Equalizer Engine Mode Selector (Visible on the Equalizer stage) ─────
        let on_eq_stage =
            state.active_tab == EqTab::Graphic || state.active_tab == EqTab::Parametric;
        if on_eq_stage {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_xs, 0.0);
                ui.label(
                    egui::RichText::new("Equalizer Engine:")
                        .font(crate::theme::font(10.5))
                        .color(theme.text_muted),
                );

                let is_graphic = state.eq_mode == config::EqMode::Graphic;
                let graphic_label = format!("{} (10-64 Bands)", tr!("Graphic EQ"));
                if choice_pill(ui, &graphic_label, is_graphic, theme.accent_warm, theme).clicked()
                    && !is_graphic
                {
                    state.active_tab = EqTab::Graphic;
                    state.set_eq_mode(config::EqMode::Graphic);
                    actions.eq_mode_changed = true;
                }

                let is_parametric = state.eq_mode == config::EqMode::Parametric;
                let parametric_label = format!("{} (AutoEQ / Biquad)", tr!("Parametric"));
                if choice_pill(
                    ui,
                    &parametric_label,
                    is_parametric,
                    theme.accent_warm,
                    theme,
                )
                .clicked()
                    && !is_parametric
                {
                    state.active_tab = EqTab::Parametric;
                    state.set_eq_mode(config::EqMode::Parametric);
                    actions.eq_mode_changed = true;
                    actions.eq_bands_changed = true;
                }
            });
        }
    });
}

fn render_tab_button(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
    tab: EqTab,
    label: &str,
) {
    let is_sel = state.active_tab == tab;
    render_tab_button_custom(ui, state, actions, theme, tab, label, is_sel);
}

/// One content-width tab button supporting custom selection state:
fn render_tab_button_custom(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
    tab: EqTab,
    label: &str,
    is_sel: bool,
) {
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
