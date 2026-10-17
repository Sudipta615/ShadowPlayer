use std::path::PathBuf;
use eframe::egui::{self, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use domain::ids::{ScanRootId, TrackId};
use domain::models::{ScanRoot, Track};
use crate::theme::Theme;

pub struct FoldersViewAction {
    pub add_folder: Option<PathBuf>,
    pub remove_folder: Option<ScanRootId>,
    pub rescan_folder: Option<PathBuf>,
    pub play_folder: Option<PathBuf>,
    pub queue_folder: Option<PathBuf>,
    pub play_track: Option<TrackId>,
    pub toggle_favorite: Option<TrackId>,
}

pub struct FoldersView<'a> {
    pub scan_roots: &'a [ScanRoot],
    pub selected_folder: &'a mut Option<PathBuf>,
    pub folder_tracks: &'a [Track],
    pub new_folder_input: &'a mut String,
    pub is_scanning: bool,
    pub scan_progress_text: &'a str,
    pub active_track_id: Option<TrackId>,
    pub favorite_track_ids: &'a std::collections::HashSet<TrackId>,
    pub theme: &'a Theme,
}

impl<'a> FoldersView<'a> {
    pub fn show(self, ui: &mut Ui) -> FoldersViewAction {
        let mut action = FoldersViewAction {
            add_folder: None,
            remove_folder: None,
            rescan_folder: None,
            play_folder: None,
            queue_folder: None,
            play_track: None,
            toggle_favorite: None,
        };

        if let Some(sel_path) = self.selected_folder.clone() {
            // ── Folder Detail / Track List View ────────────────────────────
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(10.0, 0.0);

                let back_btn = egui::Button::new(
                    egui::RichText::new("‹ Back to Folders")
                        .font(FontId::proportional(13.5))
                        .color(self.theme.accent_purple),
                )
                .fill(self.theme.bg_pill)
                .corner_radius(CornerRadius::same(8));

                if ui.add(back_btn).clicked() {
                    *self.selected_folder = None;
                }
            });

            ui.add_space(8.0);

            // Folder Header Card
            let header_frame = egui::Frame::NONE
                .fill(self.theme.bg_card)
                .stroke(Stroke::new(1.0_f32, self.theme.border_card))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(egui::Margin::same(16));

            header_frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(18.0, 0.0);

                    let box_size = Vec2::new(80.0, 80.0);
                    let (box_rect, _) = ui.allocate_exact_size(box_size, Sense::hover());
                    let p = ui.painter_at(box_rect);
                    p.rect_filled(box_rect, CornerRadius::same(12), Color32::from_rgb(30, 24, 48));
                    p.rect_stroke(box_rect, CornerRadius::same(12), Stroke::new(1.2_f32, self.theme.accent_violet), egui::StrokeKind::Inside);
                    crate::components::icons::draw_folder(ui, box_rect.shrink2(Vec2::new(20.0, 20.0)), self.theme.accent_pink);

                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(0.0, 4.0);
                        ui.label(
                            egui::RichText::new("FOLDER")
                                .font(FontId::proportional(11.0))
                                .strong()
                                .color(self.theme.accent_magenta),
                        );

                        let folder_name = sel_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| sel_path.to_string_lossy().to_string());
                        ui.label(
                            egui::RichText::new(folder_name)
                                .font(FontId::proportional(20.0))
                                .strong()
                                .color(self.theme.text_primary),
                        );

                        ui.label(
                            egui::RichText::new(sel_path.to_string_lossy())
                                .font(FontId::proportional(11.5))
                                .color(self.theme.text_dim),
                        );

                        ui.label(
                            egui::RichText::new(format!("{} songs in directory", self.folder_tracks.len()))
                                .font(FontId::proportional(12.0))
                                .color(self.theme.text_muted),
                        );

                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let play_all_btn = egui::Button::new(
                                egui::RichText::new("▶ Play All")
                                    .font(FontId::proportional(12.5))
                                    .color(Color32::WHITE),
                            )
                            .fill(self.theme.accent_violet)
                            .corner_radius(CornerRadius::same(8));

                            if ui.add(play_all_btn).clicked() {
                                action.play_folder = Some(sel_path.clone());
                            }

                            let queue_all_btn = egui::Button::new(
                                egui::RichText::new("➕ Queue All")
                                    .font(FontId::proportional(12.5))
                                    .color(self.theme.text_primary),
                            )
                            .fill(self.theme.bg_pill)
                            .corner_radius(CornerRadius::same(8));

                            if ui.add(queue_all_btn).clicked() {
                                action.queue_folder = Some(sel_path.clone());
                            }

                            let rescan_btn = egui::Button::new(
                                egui::RichText::new("🔄 Rescan")
                                    .font(FontId::proportional(12.5))
                                    .color(self.theme.text_primary),
                            )
                            .fill(self.theme.bg_pill)
                            .corner_radius(CornerRadius::same(8));

                            if ui.add(rescan_btn).clicked() {
                                action.rescan_folder = Some(sel_path.clone());
                            }
                        });
                    });
                });
            });

            ui.add_space(10.0);

            // Folder Tracks Table
            egui::ScrollArea::vertical()
                .id_salt("folder_tracks_scroll")
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
                    let full_w = ui.available_width();

                    for (idx, track) in self.folder_tracks.iter().enumerate() {
                        let is_cur = self.active_track_id == Some(track.id);
                        let is_fav = self.favorite_track_ids.contains(&track.id);
                        let row_h = 38.0;

                        let (row_rect, row_resp) = ui.allocate_exact_size(Vec2::new(full_w, row_h), Sense::click());
                        if row_resp.double_clicked() {
                            action.play_track = Some(track.id);
                        }

                        let painter = ui.painter_at(row_rect);
                        if is_cur {
                            painter.rect_filled(row_rect, CornerRadius::same(6), Color32::from_rgb(38, 21, 70));
                            painter.rect_stroke(row_rect, CornerRadius::same(6), Stroke::new(1.0_f32, Color32::from_rgb(98, 41, 172)), egui::StrokeKind::Inside);
                        } else if row_resp.hovered() {
                            painter.rect_filled(row_rect, CornerRadius::same(6), self.theme.bg_card_hover);
                        }

                        let cy = row_rect.center().y;
                        painter.text(
                            Pos2::new(row_rect.min.x + 10.0, cy),
                            egui::Align2::LEFT_CENTER,
                            &format!("{:02}", idx + 1),
                            FontId::proportional(12.0),
                            self.theme.text_muted,
                        );

                        painter.text(
                            Pos2::new(row_rect.min.x + 44.0, cy),
                            egui::Align2::LEFT_CENTER,
                            &track.title,
                            FontId::proportional(13.2),
                            if is_cur { Color32::WHITE } else { self.theme.text_primary },
                        );

                        painter.text(
                            Pos2::new(row_rect.min.x + 280.0, cy),
                            egui::Align2::LEFT_CENTER,
                            &track.artist,
                            FontId::proportional(12.5),
                            self.theme.text_muted,
                        );

                        let dur_s = format_time(track.duration_secs as f64);
                        painter.text(
                            Pos2::new(row_rect.max.x - 48.0, cy),
                            egui::Align2::RIGHT_CENTER,
                            &dur_s,
                            FontId::proportional(12.0),
                            self.theme.text_muted,
                        );

                        let fav_rect = Rect::from_center_size(Pos2::new(row_rect.max.x - 16.0, cy), Vec2::new(18.0, 18.0));
                        let fav_hover = ui.rect_contains_pointer(fav_rect);
                        let heart_col = if is_fav { self.theme.accent_pink } else if fav_hover { self.theme.text_primary } else { self.theme.text_muted };
                        crate::components::icons::draw_heart(ui, fav_rect, heart_col, is_fav);

                        if fav_hover && ui.input(|i| i.pointer.primary_clicked()) {
                            action.toggle_favorite = Some(track.id);
                        }
                    }
                });
        } else {
            // ── Folders & Library Manager ──────────────────────────────────
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(10.0, 0.0);
                ui.label(
                    egui::RichText::new("Music Folders")
                        .font(FontId::proportional(24.0))
                        .strong()
                        .color(self.theme.text_primary),
                );
                ui.label(
                    egui::RichText::new(format!("{} indexed folders", self.scan_roots.len()))
                        .font(FontId::proportional(13.5))
                        .color(self.theme.text_muted),
                );
            });

            ui.add_space(8.0);

            // Add Folder Card / Input Row
            let add_frame = egui::Frame::NONE
                .fill(self.theme.bg_card)
                .stroke(Stroke::new(1.0_f32, self.theme.border_card))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(egui::Margin::same(14));

            add_frame.show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 8.0);
                    ui.label(
                        egui::RichText::new("Add Music Folder to Library")
                            .font(FontId::proportional(14.0))
                            .strong()
                            .color(self.theme.text_primary),
                    );

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);

                        let hint = dirs::audio_dir()
                            .map(|d| d.to_string_lossy().to_string())
                            .unwrap_or_else(|| "/path/to/music".to_string());

                        let edit = egui::TextEdit::singleline(self.new_folder_input)
                            .hint_text(hint)
                            .desired_width(ui.available_width() - 220.0);

                        ui.add(edit);

                        let add_btn = egui::Button::new(
                            egui::RichText::new("+ Add & Scan")
                                .font(FontId::proportional(12.5))
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(self.theme.accent_violet)
                        .corner_radius(CornerRadius::same(8));

                        if ui.add(add_btn).clicked() && !self.new_folder_input.trim().is_empty() {
                            let p = PathBuf::from(self.new_folder_input.trim());
                            action.add_folder = Some(p);
                            self.new_folder_input.clear();
                        }

                        // Quick music directory shortcut if available
                        if let Some(audio_d) = dirs::audio_dir() {
                            let quick_btn = egui::Button::new(
                                egui::RichText::new("📁 Use ~/Music")
                                    .font(FontId::proportional(12.0))
                                    .color(self.theme.text_secondary),
                            )
                            .fill(self.theme.bg_pill)
                            .corner_radius(CornerRadius::same(8));

                            if ui.add(quick_btn).clicked() {
                                action.add_folder = Some(audio_d);
                            }
                        }
                    });

                    // Drag and drop hint
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
                        ui.label(
                            egui::RichText::new("💡 Tip:")
                                .font(FontId::proportional(11.5))
                                .color(self.theme.accent_magenta),
                        );
                        ui.label(
                            egui::RichText::new("You can also drag and drop audio files or entire folders directly into ShadowPlayer at any time.")
                                .font(FontId::proportional(11.5))
                                .color(self.theme.text_muted),
                        );
                    });
                });
            });

            ui.add_space(8.0);

            // Scanning progress banner if active
            if self.is_scanning {
                let scan_frame = egui::Frame::NONE
                    .fill(Color32::from_rgb(38, 20, 64))
                    .stroke(Stroke::new(1.0_f32, self.theme.accent_magenta))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(egui::Margin::symmetric(14, 8));

                scan_frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(10.0, 0.0);
                        ui.spinner();
                        ui.label(
                            egui::RichText::new(self.scan_progress_text)
                                .font(FontId::proportional(13.0))
                                .color(Color32::WHITE),
                        );
                    });
                });
                ui.add_space(8.0);
            }

            // Scanned Roots List
            ui.label(
                egui::RichText::new("Configured Folders")
                    .font(FontId::proportional(14.0))
                    .strong()
                    .color(self.theme.text_primary),
            );

            ui.add_space(4.0);

            if self.scan_roots.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(30.0);
                    ui.label(egui::RichText::new("No library folders configured yet").font(FontId::proportional(14.0)).color(self.theme.text_muted));
                    ui.label(egui::RichText::new("Add your music folder above or drag and drop files to start building your library").font(FontId::proportional(12.0)).color(self.theme.text_dim));
                });
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("roots_list_scroll")
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(0.0, 6.0);

                        for root in self.scan_roots {
                            let item_frame = egui::Frame::NONE
                                .fill(self.theme.bg_card)
                                .stroke(Stroke::new(1.0_f32, self.theme.border_card))
                                .corner_radius(CornerRadius::same(10))
                                .inner_margin(egui::Margin::symmetric(14, 10));

                            item_frame.show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing = Vec2::new(12.0, 0.0);

                                    let (folder_icon_rect, _) = ui.allocate_exact_size(Vec2::new(24.0, 24.0), Sense::hover());
                                    crate::components::icons::draw_folder(ui, folder_icon_rect, self.theme.accent_pink);

                                    ui.vertical(|ui| {
                                        ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
                                        let name = root.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| root.path.to_string_lossy().to_string());
                                        ui.label(
                                            egui::RichText::new(name)
                                                .font(FontId::proportional(13.8))
                                                .strong()
                                                .color(self.theme.text_primary),
                                        );
                                        ui.label(
                                            egui::RichText::new(root.path.to_string_lossy())
                                                .font(FontId::proportional(11.5))
                                                .color(self.theme.text_muted),
                                        );
                                    });

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);

                                        let del_btn = egui::Button::new(
                                            egui::RichText::new("✕")
                                                .font(FontId::proportional(12.0))
                                                .color(self.theme.accent_magenta),
                                        )
                                        .fill(self.theme.bg_pill)
                                        .corner_radius(CornerRadius::same(6));

                                        if ui.add(del_btn).on_hover_text("Remove folder from library").clicked() {
                                            action.remove_folder = Some(root.id);
                                        }

                                        let scan_btn = egui::Button::new(
                                            egui::RichText::new("🔄 Scan")
                                                .font(FontId::proportional(12.0))
                                                .color(self.theme.text_primary),
                                        )
                                        .fill(self.theme.bg_pill)
                                        .corner_radius(CornerRadius::same(6));

                                        if ui.add(scan_btn).clicked() {
                                            action.rescan_folder = Some(root.path.clone());
                                        }

                                        let view_btn = egui::Button::new(
                                            egui::RichText::new("View Tracks →")
                                                .font(FontId::proportional(12.0))
                                                .color(self.theme.accent_purple),
                                        )
                                        .fill(self.theme.bg_pill)
                                        .corner_radius(CornerRadius::same(6));

                                        if ui.add(view_btn).clicked() {
                                            *self.selected_folder = Some(root.path.clone());
                                        }
                                    });
                                });
                            });
                        }
                    });
            }
        }

        action
    }
}

fn format_time(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    let mins = s / 60;
    let secs = s % 60;
    format!("{}:{:02}", mins, secs)
}
