//! The Tone & Acoustics tab: bass/treble shelves, stereo width, and balance.

use super::advanced::render_slider_card;
use super::controls::render_footer_controls;
use super::{EqPanelActions, EqPanelState, EqTab};
use crate::theme::Theme;
use eframe::egui::{self, Vec2};

// ─────────────────────────────────────────────────────────────────────────────
// 6. Tab 2: Tone & Acoustics (Bass, Treble, Stereo Width, Balance + Extras)
// ─────────────────────────────────────────────────────────────────────────────

pub(super) fn render_tone_acoustics_tab(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    let avail_w = ui.available_width();
    let two_col = avail_w >= 540.0;
    let card_w = if two_col {
        (avail_w - 12.0) * 0.5
    } else {
        avail_w
    };

    let tone_max = if state.extended_range { 24.0 } else { 12.0 };
    let tone_range = -tone_max..=tone_max;
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

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_lg);

        // Row 1: Bass & Treble
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);

            // Card 1: Bass Shelf (80-100 Hz)
            render_slider_card(
                ui,
                card_w,
                "Bass Shelf",
                &crate::components::info_box::INFO_BASS_SHELF,
                &mut state.bass_shelf_db,
                tone_range.clone(),
                0.0,
                min_lbl,
                "0dB",
                max_lbl,
                |v| format!("{:+.1} dB", v),
                |v| actions.bass_shelf_changed = Some(v),
                theme,
            );

            // Card 2: Treble Shelf (7.5-10 kHz)
            if two_col {
                render_slider_card(
                    ui,
                    card_w,
                    "Treble Shelf",
                    &crate::components::info_box::INFO_TREBLE_SHELF,
                    &mut state.treble_shelf_db,
                    tone_range.clone(),
                    0.0,
                    min_lbl,
                    "0dB",
                    max_lbl,
                    |v| format!("{:+.1} dB", v),
                    |v| actions.treble_shelf_changed = Some(v),
                    theme,
                );
            }
        });

        if !two_col {
            render_slider_card(
                ui,
                card_w,
                "Treble Shelf",
                &crate::components::info_box::INFO_TREBLE_SHELF,
                &mut state.treble_shelf_db,
                tone_range,
                0.0,
                min_lbl,
                "0dB",
                max_lbl,
                |v| format!("{:+.1} dB", v),
                |v| actions.treble_shelf_changed = Some(v),
                theme,
            );
        }

        // Row 2: Stereo Width & Balance
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);

            // Card 3: Stereo Width (0% to 200%)
            render_slider_card(
                ui,
                card_w,
                "Stereo Width",
                &crate::components::info_box::INFO_STEREO_WIDTH,
                &mut state.stereo_width,
                0.0..=2.0,
                1.0,
                "0 %",
                "100 %",
                "200 %",
                |v| format!("{:.0} %", v * 100.0),
                |v| actions.stereo_width_changed = Some(v),
                theme,
            );

            // Card 4: Balance (-1.00 to +1.00)
            if two_col {
                render_slider_card(
                    ui,
                    card_w,
                    "Balance",
                    &crate::components::info_box::INFO_BALANCE,
                    &mut state.balance,
                    -1.0..=1.0,
                    0.0,
                    "Left",
                    "Center",
                    "Right",
                    |v| {
                        if v.abs() < 0.01 {
                            "Center (0.00)".to_string()
                        } else if v < 0.0 {
                            format!("L {:.2}", -v)
                        } else {
                            format!("R {:.2}", v)
                        }
                    },
                    |v| actions.balance_changed = Some(v),
                    theme,
                );
            }
        });

        if !two_col {
            render_slider_card(
                ui,
                card_w,
                "Balance",
                &crate::components::info_box::INFO_BALANCE,
                &mut state.balance,
                -1.0..=1.0,
                0.0,
                "Left",
                "Center",
                "Right",
                |v| {
                    if v.abs() < 0.01 {
                        "Center (0.00)".to_string()
                    } else if v < 0.0 {
                        format!("L {:.2}", -v)
                    } else {
                        format!("R {:.2}", v)
                    }
                },
                |v| actions.balance_changed = Some(v),
                theme,
            );
        }

        // ── Bottom Controls: Preamp + Tab-Specific Reset ─────────────────────
        render_footer_controls(ui, state, actions, theme, EqTab::Tone);
    });
}
