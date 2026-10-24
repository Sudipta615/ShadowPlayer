//! Whole-chain named presets: save, recall, rename, and delete.

use super::super::widgets::{card_header, pill_btn, setting_card, setting_item};
use super::super::{SettingsTabActions, SettingsTabState};
use crate::theme::Theme;
use eframe::egui::{self, Vec2};

/// Whole-chain named presets: capture everything the EQ panel and the mix bus
/// own into one recallable profile.
///
/// The EQ panel can already save a bare curve; this saves the *chain* — the
/// parametric and graphic curves, preamp and headroom, tone, dynamics,
/// crossfeed and correction settings, plus bass management, LFE and up/down
/// mixing. Switching between a headphone rig and a speaker rig is then one
/// click instead of a re-dial.
pub(in crate::components::settings_tab) fn render_chain_presets_card(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
    actions: &mut SettingsTabActions,
) {
    // Split the borrows so the list can be drawn while the inline rename box
    // edits its own scratch field.
    let SettingsTabState {
        chain_presets,
        chain_preset_renaming,
        chain_preset_rename_buffer,
        chain_preset_draft_name,
        chain_preset_draft_note,
        ..
    } = state;

    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_headphones,
            &tr!("Chain Presets"),
            &tr!("Save the entire signal path — equalizer, tone, dynamics, filters and the mix bus — as one named profile"),
            None,
            theme,
        );

        // ── Capture the current chain ──────────────────────────────
        setting_item(
            ui,
            &tr!("Save Current Chain"),
            &tr!("Stores everything currently engaged. A note is handy for headphone or room profiles."),
            theme,
            |ui| {
                ui.add(
                    egui::TextEdit::singleline(chain_preset_draft_name)
                        .hint_text(tr!(Msg::S5E942739B58DDBD5))
                        .desired_width(136.0)
                        .font(crate::theme::font(12.0)),
                );
                ui.add(
                    egui::TextEdit::singleline(chain_preset_draft_note)
                        .hint_text(tr!(Msg::SF2F7DEFA2BD873BE))
                        .desired_width(150.0)
                        .font(crate::theme::font(12.0)),
                );
                let can_save = !chain_preset_draft_name.trim().is_empty();
                if ui
                    .add_enabled(can_save, pill_btn(tr!(Msg::S2517FA31F3933B41), false, theme))
                    .on_hover_text(tr!(Msg::SE1530F77DBF9F4E0))
                    .clicked()
                {
                    actions.chain_preset_save = Some(chain_preset_draft_name.trim().to_string());
                    chain_preset_draft_name.clear();
                    chain_preset_draft_note.clear();
                }
            },
        );

        if chain_presets.is_empty() {
            ui.label(
                egui::RichText::new(
                    tr!("No chain presets yet. Dial in a curve, a crossfeed stage or a crossover, then save it above."),
                )
                .font(crate::theme::font(11.5))
                .color(theme.text_dim),
            );
            return;
        }

        // ── Saved profiles ─────────────────────────────────────────
        let mut apply: Option<usize> = None;
        let mut delete: Option<usize> = None;
        let mut start_rename: Option<usize> = None;
        let mut commit_rename: Option<(usize, String)> = None;
        let mut cancel_rename = false;

        for (idx, preset) in chain_presets.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xxs);
                    ui.label(
                        egui::RichText::new(&preset.name)
                            .font(crate::theme::font(13.0))
                            .strong()
                            .color(theme.text_primary),
                    );
                    ui.label(
                        egui::RichText::new(preset.summary())
                            .font(crate::theme::font(11.0))
                            .color(theme.text_muted),
                    );
                    if !preset.note.is_empty() {
                        ui.label(
                            egui::RichText::new(&preset.note)
                                .font(crate::theme::font(10.5))
                                .italics()
                                .color(theme.text_dim),
                        );
                    }
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(pill_btn(tr!(Msg::SDC673E73B5C13AEA), false, theme))
                        .clicked()
                    {
                        delete = Some(idx);
                    }
                    if ui
                        .add(pill_btn(tr!(Msg::S60320A18282B2B33), false, theme))
                        .clicked()
                    {
                        start_rename = Some(idx);
                    }
                    if ui
                        .add(pill_btn(tr!(Msg::SB8F855B49234B81B), false, theme))
                        .on_hover_text(tr!(Msg::SAA1012D5FD87698A))
                        .clicked()
                    {
                        apply = Some(idx);
                    }
                });
            });

            // Inline rename for this row.
            if *chain_preset_renaming == Some(idx) {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(chain_preset_rename_buffer)
                            .hint_text(tr!(Msg::S383F5447E03936AE))
                            .desired_width(180.0)
                            .font(crate::theme::font(12.0)),
                    );
                    let can_commit = !chain_preset_rename_buffer.trim().is_empty();
                    if ui
                        .add_enabled(
                            can_commit,
                            pill_btn(tr!(Msg::S33F85F24C0F5F008), false, theme),
                        )
                        .clicked()
                    {
                        commit_rename = Some((idx, chain_preset_rename_buffer.trim().to_string()));
                    }
                    if ui
                        .add(pill_btn(tr!(Msg::S2CEB11BE2290BB1B), false, theme))
                        .clicked()
                    {
                        cancel_rename = true;
                    }
                });
            }

            if idx + 1 < chain_presets.len() {
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(2.0);
            }
        }

        if let Some(i) = apply {
            actions.chain_preset_apply = Some(i);
        }
        if let Some(i) = delete {
            actions.chain_preset_delete = Some(i);
        }
        if let Some(i) = start_rename {
            *chain_preset_renaming = Some(i);
            *chain_preset_rename_buffer = chain_presets[i].name.clone();
        }
        if let Some((i, name)) = commit_rename {
            actions.chain_preset_rename = Some((i, name));
            *chain_preset_renaming = None;
            chain_preset_rename_buffer.clear();
        }
        if cancel_rename {
            *chain_preset_renaming = None;
            chain_preset_rename_buffer.clear();
        }
    });
}
