//! The Lyrics tab of the right panel.
use eframe::egui::{self, Vec2};

use super::{RightPanel, RightPanelAction};

impl<'a> RightPanel<'a> {
    pub(super) fn render_lyrics_tab(&mut self, ui: &mut egui::Ui, action: &mut RightPanelAction) {
        // Lyrics Tab Content. A missing source means the lookup
        // has not finished yet: tag lyrics are read off-thread,
        // sidecars resolve synchronously.
        let lyrics_resolved = self.lyrics_source.is_some();
        let track_id = self.current_track.map(|t| t.id);

        // A draft belongs to exactly one track: never let it
        // be saved onto the next song.
        if self.lyrics_editor.track_id.is_some() && self.lyrics_editor.track_id != track_id {
            self.lyrics_editor.close();
        }
        let editing_here = self.lyrics_editor.editing && self.lyrics_editor.track_id == track_id;

        let caption = match self.lyrics_source {
            Some(s) if s.has_lyrics() => s.caption(),
            _ => None,
        };
        ui.horizontal(|ui| {
            if let Some(caption) = caption {
                ui.label(
                    egui::RichText::new(caption)
                        .font(crate::theme::font(11.0))
                        .color(self.theme.text_dim),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if editing_here {
                    if ui.small_button(tr!("Save")).clicked() {
                        action.save_lyrics = true;
                    }
                    if ui.small_button(tr!("Cancel")).clicked() {
                        self.lyrics_editor.close();
                    }
                } else if let Some(id) = track_id {
                    if ui.small_button(tr!("Edit")).clicked() {
                        self.lyrics_editor
                            .begin(id, self.plain_lyrics.unwrap_or("").to_string());
                    }
                }
            });
        });
        ui.add_space(6.0);

        if editing_here {
            ui.label(
            egui::RichText::new(tr!(
                "Edit the raw lyrics. Timestamps such as [00:12.50] stay synced; save writes a .lrc file beside the track."
            ))
            .font(crate::theme::font(11.0))
            .color(self.theme.text_dim),
        );
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .id_salt("lyrics_editor_scroll")
                .max_height(420.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.lyrics_editor.draft)
                            .desired_rows(16)
                            .desired_width(f32::INFINITY)
                            .hint_text(tr!("[00:00.00] First line\n[00:04.20] Second line"))
                            .font(egui::TextStyle::Monospace),
                    );
                });
            if let Some(error) = &self.lyrics_editor.error {
                ui.label(
                    egui::RichText::new(error)
                        .font(crate::theme::font(11.0))
                        .color(self.theme.destructive),
                );
            }
        } else if self.lyrics_lines.is_empty() {
            if let Some(plain) = self.plain_lyrics {
                egui::ScrollArea::vertical()
                    .id_salt("plain_lyrics_scroll")
                    .max_height((ui.available_height() - 40.0).clamp(120.0, 520.0))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(0.0, self.theme.space_sm);
                        for line in plain.lines() {
                            ui.label(
                                egui::RichText::new(line)
                                    .font(crate::theme::font(13.0))
                                    .color(self.theme.text_secondary),
                            );
                        }
                    });
            } else if track_id.is_none() {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label(
                        egui::RichText::new(tr!("No track playing yet."))
                            .font(crate::theme::font(13.5))
                            .color(self.theme.text_muted),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(tr!("Play a track to see its lyrics here."))
                            .font(crate::theme::font(11.5))
                            .color(self.theme.text_dim),
                    );
                });
            } else if !lyrics_resolved {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label(
                        egui::RichText::new(tr!("Looking for lyrics..."))
                            .font(crate::theme::font(13.5))
                            .color(self.theme.text_muted),
                    );
                });
            } else {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label(
                        egui::RichText::new(tr!("No lyrics available"))
                            .font(crate::theme::font(13.5))
                            .color(self.theme.text_muted),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(tr!("Place a .lrc or .txt file beside the track,"))
                            .font(crate::theme::font(11.5))
                            .color(self.theme.text_dim),
                    );
                    ui.label(
                        egui::RichText::new(tr!("or embed lyrics in the file's tags."))
                            .font(crate::theme::font(11.5))
                            .color(self.theme.text_dim),
                    );
                });
            }
        } else {
            let active_idx = self.active_lyric_idx.unwrap_or(0);
            let scroll_h = 360.0;

            egui::ScrollArea::vertical()
                .id_salt("lyrics_scroll_area")
                .max_height(scroll_h)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, self.theme.space_lg);

                    for (idx, line) in self.lyrics_lines.iter().enumerate() {
                        let is_current = idx == active_idx;

                        let (font_size, color) = if is_current {
                            (16.0, self.theme.accent_highlight)
                        } else if idx < active_idx {
                            (12.5, self.theme.text_muted)
                        } else {
                            (13.0, self.theme.text_secondary)
                        };

                        let rich = if is_current {
                            egui::RichText::new(&line.text)
                                .font(crate::theme::font(font_size))
                                .strong()
                                .color(color)
                        } else {
                            egui::RichText::new(&line.text)
                                .font(crate::theme::font(font_size))
                                .color(color)
                        };

                        let label_resp = ui.label(rich);
                        if is_current {
                            label_resp.scroll_to_me(Some(egui::Align::Center));
                        }
                    }
                });
        }
    }
}
