use std::path::PathBuf;
use eframe::egui::{self, Color32, CornerRadius, FontId, Sense, Stroke, Vec2};
use audio_session::AudioSession;
use database::{ScanRootsRepo, TrackRepo};

use crate::components::settings_tab::{show_settings_tab_extended, SettingsExtraContext};
use crate::state::DspState;
use crate::theme::Theme;

pub fn render_settings_view(
    ui: &mut egui::Ui,
    dsp_state: &mut DspState,
    session: &AudioSession,
    theme: &mut Theme,
    scan_roots_repo: &ScanRootsRepo,
    track_repo: &TrackRepo,
    is_scanning: bool,
    mut on_start_scan: impl FnMut(PathBuf),
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, 14.0);

        // ── 1. Page Header with Title, Description, and Live Badges ────────
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Settings & Audio Studio")
                    .font(FontId::proportional(22.0))
                    .strong()
                    .color(theme.text_primary),
            );

            // Right-aligned status pills
            ui.with_layout(
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);

                    // Bit-Perfect Indicator Pill
                    let (bp_text, bp_bg, bp_fg, bp_border) = if dsp_state.settings_tab.bit_perfect {
                        ("Bit-Perfect Direct Stream", Color32::from_rgba_premultiplied(16, 185, 129, 35), theme.bit_perfect_green, theme.bit_perfect_green)
                    } else {
                        ("DSP Processing Active", theme.bg_pill, theme.accent_magenta, theme.border_card)
                    };

                    egui::Frame::NONE
                        .fill(bp_bg)
                        .stroke(Stroke::new(1.0, bp_border))
                        .corner_radius(CornerRadius::same(7))
                        .inner_margin(egui::Margin::symmetric(10, 5))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
                                let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                                ui.painter_at(dot_rect).circle_filled(dot_rect.center(), 3.5, bp_fg);
                                ui.label(
                                    egui::RichText::new(bp_text)
                                        .font(FontId::proportional(11.5))
                                        .strong()
                                        .color(bp_fg),
                                );
                            });
                        });

                    // Engine Status Pill
                    egui::Frame::NONE
                        .fill(theme.bg_pill)
                        .stroke(Stroke::new(1.0, theme.border_card))
                        .corner_radius(CornerRadius::same(7))
                        .inner_margin(egui::Margin::symmetric(10, 5))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
                                let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                                ui.painter_at(dot_rect).circle_filled(dot_rect.center(), 3.5, theme.accent_pink);
                                ui.label(
                                    egui::RichText::new("Engine 1.2.1 Active (44.1 - 192kHz)")
                                        .font(FontId::proportional(11.5))
                                        .strong()
                                        .color(theme.accent_pink),
                                );
                            });
                        });
                },
            );
        });
        ui.label(
            egui::RichText::new("Configure Audio Engine v1.2.1, bit-perfect streaming, 3D acoustics, and library")
                .font(FontId::proportional(12.5))
                .color(theme.text_muted),
        );
        ui.add_space(2.0);

        // ── 2. Render Settings Content with Extra Context ──────────────────
        let scan_roots = scan_roots_repo.list().unwrap_or_default();
        let track_count = track_repo.count().unwrap_or_default();

        let extra = SettingsExtraContext {
            scan_roots: &scan_roots,
            track_count,
            is_scanning,
            is_dark_theme: !theme.is_light,
        };

        let s_actions = show_settings_tab_extended(ui, &mut dsp_state.settings_tab, theme, &extra);

        // ── 3. Handle Settings Actions ─────────────────────────────────────
        if s_actions.toggle_theme {
            theme.toggle();
        }
        if let Some(ref path) = s_actions.trigger_scan {
            let _ = scan_roots_repo.add(path);
            on_start_scan(path.clone());
        }
        if let Some(id) = s_actions.remove_scan_root {
            let _ = scan_roots_repo.remove(&id);
        }

        dsp_state.handle_settings_actions(session, &s_actions);
    });
}
