use domain::ids::{PlaylistId, TrackId};
use domain::models::{Playlist, Track};
use eframe::egui::{self, Color32, CornerRadius, FontId, Vec2};

use crate::theme::Theme;

/// State for the floating playlist manager window.
#[derive(Default)]
pub struct PlaylistEditorState {
    pub is_open: bool,
    /// Buffer for the \"new playlist\" name field.
    pub new_name: String,
    /// Id of the playlist currently being renamed (inline edit).
    pub rename_id: Option<PlaylistId>,
    pub rename_buf: String,
    /// Playlist whose contents are shown on the right-hand side.
    pub selected: Option<PlaylistId>,
    /// Search text used to find tracks to add.
    pub search: String,
    /// Cache of the last search string so filtering only runs when it changes.
    search_cache: String,
    /// Indices into the library slice matching the current search.
    results: Vec<usize>,
}

impl PlaylistEditorState {
    /// Open the manager, optionally preselecting a playlist.
    pub fn open(&mut self, selected: Option<PlaylistId>) {
        self.is_open = true;
        if selected.is_some() {
            self.selected = selected;
        }
    }
}

#[derive(Default)]
pub struct PlaylistEditorActions {
    pub create: Option<String>,
    pub rename: Option<(PlaylistId, String)>,
    pub delete: Option<PlaylistId>,
    pub add_track: Option<(PlaylistId, TrackId)>,
    pub remove_track: Option<(PlaylistId, TrackId)>,
    /// The user picked a different playlist; the caller should load its tracks.
    pub select_changed: Option<PlaylistId>,
    /// Save the selected playlist to an `.m3u8` file.
    pub export_m3u: bool,
}

const MAX_SEARCH_RESULTS: usize = 60;

/// Floating playlist manager: create / rename / delete playlists and add or
/// remove tracks. Returns the actions the caller should apply to the database.
pub fn show_playlist_editor(
    ctx: &egui::Context,
    state: &mut PlaylistEditorState,
    playlists: &[Playlist],
    library_tracks: &[Track],
    playlist_tracks: &[Track],
    theme: &Theme,
) -> PlaylistEditorActions {
    let mut actions = PlaylistEditorActions::default();
    if !state.is_open {
        return actions;
    }

    let mut is_open = state.is_open;

    egui::Window::new("Playlist Manager")
        .open(&mut is_open)
        .collapsible(false)
        .resizable(true)
        .default_size(Vec2::new(520.0, 430.0))
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 8.0);

            // ── Create new playlist ─────────────────────────────────────
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                let edit = egui::TextEdit::singleline(&mut state.new_name)
                    .hint_text("New playlist name…")
                    .desired_width(ui.available_width() - 90.0);
                ui.add(edit);
                let btn = egui::Button::new(egui::RichText::new("Create").color(Color32::WHITE))
                    .fill(theme.accent_violet);
                if ui.add(btn).clicked() && !state.new_name.trim().is_empty() {
                    actions.create = Some(state.new_name.trim().to_string());
                    state.new_name.clear();
                }
            });

            ui.separator();

            // ── Selected playlist summary + M3U export ─────────────────
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                let selected_name = state
                    .selected
                    .and_then(|id| playlists.iter().find(|p| p.id == id))
                    .map(|p| p.name.clone());
                match selected_name {
                    Some(name) => {
                        ui.label(
                            egui::RichText::new(format!(
                                "{name} · {} track(s)",
                                playlist_tracks.len()
                            ))
                            .font(FontId::proportional(12.0))
                            .color(theme.text_muted),
                        );
                    }
                    None => {
                        ui.label(
                            egui::RichText::new("Select a playlist")
                                .font(FontId::proportional(12.0))
                                .color(theme.text_muted),
                        );
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let can_export = state.selected.is_some() && !playlist_tracks.is_empty();
                    let btn = egui::Button::new(
                        egui::RichText::new("Export M3U").font(FontId::proportional(12.0)),
                    );
                    if ui
                        .add_enabled(can_export, btn)
                        .on_hover_text("Save this playlist as an .m3u8 file")
                        .clicked()
                    {
                        actions.export_m3u = true;
                    }
                });
            });

            ui.horizontal_top(|ui| {
                let col_w = (ui.available_width() * 0.42).max(180.0);

                // ── Left column: playlist list ─────────────────────────
                ui.allocate_ui_with_layout(
                    Vec2::new(col_w, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.label(
                            egui::RichText::new("Playlists")
                                .font(FontId::proportional(13.5))
                                .strong()
                                .color(theme.text_primary),
                        );
                        ui.add_space(4.0);

                        if playlists.is_empty() {
                            ui.label(
                                egui::RichText::new("No playlists yet.")
                                    .font(FontId::proportional(12.0))
                                    .color(theme.text_muted),
                            );
                        }

                        egui::ScrollArea::vertical()
                            .id_salt("playlist_editor_list")
                            .auto_shrink([false, false])
                            .max_height(ui.available_height() - 8.0)
                            .show(ui, |ui| {
                                for pl in playlists {
                                    let is_sel = state.selected == Some(pl.id);

                                    if state.rename_id == Some(pl.id) {
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
                                            let edit =
                                                egui::TextEdit::singleline(&mut state.rename_buf)
                                                    .desired_width(ui.available_width() - 56.0);
                                            ui.add(edit);
                                            if ui.small_button("OK").clicked() {
                                                let name = state.rename_buf.trim().to_string();
                                                if !name.is_empty() {
                                                    actions.rename = Some((pl.id, name));
                                                }
                                                state.rename_id = None;
                                            }
                                            if ui.small_button("Cancel").clicked() {
                                                state.rename_id = None;
                                            }
                                        });
                                        continue;
                                    }

                                    let (row_rect, resp) = ui.allocate_exact_size(
                                        Vec2::new(ui.available_width(), 30.0),
                                        egui::Sense::click(),
                                    );
                                    let p = ui.painter_at(row_rect);
                                    let bg = if is_sel {
                                        theme.bg_card_active
                                    } else if resp.hovered() {
                                        theme.bg_card_hover
                                    } else {
                                        theme.bg_card
                                    };
                                    p.rect_filled(row_rect, CornerRadius::same(7), bg);
                                    p.text(
                                        egui::Pos2::new(row_rect.min.x + 10.0, row_rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        &pl.name,
                                        FontId::proportional(12.8),
                                        theme.text_primary,
                                    );
                                    let rename_rect = egui::Rect::from_center_size(
                                        egui::Pos2::new(row_rect.max.x - 30.0, row_rect.center().y),
                                        Vec2::new(16.0, 16.0),
                                    );
                                    let del_rect = egui::Rect::from_center_size(
                                        egui::Pos2::new(row_rect.max.x - 54.0, row_rect.center().y),
                                        Vec2::new(16.0, 16.0),
                                    );
                                    let over_sub = resp.hovered()
                                        && (ui.rect_contains_pointer(rename_rect)
                                            || ui.rect_contains_pointer(del_rect));

                                    p.text(
                                        egui::Pos2::new(row_rect.max.x - 72.0, row_rect.center().y),
                                        egui::Align2::RIGHT_CENTER,
                                        format!("{}", pl.track_count),
                                        FontId::proportional(11.5),
                                        theme.text_muted,
                                    );

                                    if resp.clicked() && !over_sub {
                                        state.selected = Some(pl.id);
                                        state.search.clear();
                                        state.search_cache.clear();
                                        actions.select_changed = Some(pl.id);
                                    }

                                    // Rename / delete affordances on hover.
                                    if resp.hovered() {
                                        crate::components::icons::draw_more_menu(
                                            ui,
                                            rename_rect,
                                            theme.text_secondary,
                                        );
                                        crate::components::icons::draw_window_close(
                                            ui,
                                            del_rect,
                                            theme.text_muted,
                                        );
                                        if ui.rect_contains_pointer(rename_rect)
                                            && ui.input(|i| i.pointer.primary_clicked())
                                        {
                                            state.rename_id = Some(pl.id);
                                            state.rename_buf = pl.name.clone();
                                        }
                                        if ui.rect_contains_pointer(del_rect)
                                            && ui.input(|i| i.pointer.primary_clicked())
                                        {
                                            actions.delete = Some(pl.id);
                                        }
                                    }
                                }
                            });
                    },
                );

                ui.separator();

                // ── Right column: contents + add tracks ────────────────
                ui.allocate_ui_with_layout(
                    Vec2::new(ui.available_width(), ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        let Some(selected_id) = state.selected else {
                            ui.label(
                                egui::RichText::new("Select a playlist to edit its tracks.")
                                    .font(FontId::proportional(12.0))
                                    .color(theme.text_muted),
                            );
                            return;
                        };

                        ui.label(
                            egui::RichText::new(format!("Tracks ({})", playlist_tracks.len()))
                                .font(FontId::proportional(13.5))
                                .strong()
                                .color(theme.text_primary),
                        );
                        ui.add_space(4.0);

                        egui::ScrollArea::vertical()
                            .id_salt("playlist_editor_tracks")
                            .auto_shrink([false, false])
                            .max_height(160.0)
                            .show(ui, |ui| {
                                if playlist_tracks.is_empty() {
                                    ui.label(
                                        egui::RichText::new("Empty — add tracks below.")
                                            .font(FontId::proportional(11.5))
                                            .color(theme.text_muted),
                                    );
                                }
                                for t in playlist_tracks {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{} — {}",
                                                t.title, t.artist
                                            ))
                                            .font(FontId::proportional(12.0))
                                            .color(theme.text_secondary),
                                        );
                                        if ui
                                            .small_button("Del")
                                            .on_hover_text("Remove from playlist")
                                            .clicked()
                                        {
                                            actions.remove_track = Some((selected_id, t.id));
                                        }
                                    });
                                }
                            });

                        ui.add_space(6.0);
                        ui.separator();

                        ui.label(
                            egui::RichText::new("Add Tracks")
                                .font(FontId::proportional(13.0))
                                .strong()
                                .color(theme.text_primary),
                        );
                        let edit = egui::TextEdit::singleline(&mut state.search)
                            .hint_text("Search library…")
                            .desired_width(ui.available_width());
                        ui.add(edit);

                        // Recompute matches only when the query changes.
                        if state.search != state.search_cache {
                            state.search_cache = state.search.clone();
                            state.results.clear();
                            let q = state.search.trim().to_lowercase();
                            let mut n = 0;
                            for (i, t) in library_tracks.iter().enumerate() {
                                if n >= MAX_SEARCH_RESULTS {
                                    break;
                                }
                                let matched = if q.is_empty() {
                                    n < 12
                                } else {
                                    t.title.to_lowercase().contains(&q)
                                        || t.artist.to_lowercase().contains(&q)
                                        || t.album.to_lowercase().contains(&q)
                                };
                                if matched {
                                    state.results.push(i);
                                    n += 1;
                                }
                            }
                        }

                        egui::ScrollArea::vertical()
                            .id_salt("playlist_editor_search")
                            .auto_shrink([false, false])
                            .max_height(ui.available_height() - 8.0)
                            .show(ui, |ui| {
                                for &i in &state.results {
                                    let Some(t) = library_tracks.get(i) else {
                                        continue;
                                    };
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
                                        if ui
                                            .small_button("+")
                                            .on_hover_text("Add to playlist")
                                            .clicked()
                                        {
                                            actions.add_track = Some((selected_id, t.id));
                                        }
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{} — {}",
                                                t.title, t.artist
                                            ))
                                            .font(FontId::proportional(12.0))
                                            .color(theme.text_secondary),
                                        );
                                    });
                                }
                            });
                    },
                );
            });
        });

    state.is_open = is_open;
    actions
}
