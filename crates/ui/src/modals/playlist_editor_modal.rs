use domain::ids::{PlaylistId, TrackId};
use domain::models::{Playlist, Track};
use eframe::egui::{self, Color32, CornerRadius, Key, Margin, Pos2, Rect, Sense, Stroke, Vec2};

use crate::theme::Theme;

/// State for the floating playlist manager window.
#[derive(Default)]
pub struct PlaylistEditorState {
    pub is_open: bool,
    /// Buffer for the "new playlist" name field.
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
    /// Format used by the next export.
    pub export_format: library::PlaylistFormat,
    /// Whether the studio was open last frame. Used to move the keyboard
    /// focus into the name field exactly once, when it opens.
    was_open: bool,
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
    /// Move a track from one playlist position to another.
    pub move_track: Option<(PlaylistId, usize, usize)>,
    /// The user picked a different playlist; the caller should load its tracks.
    pub select_changed: Option<PlaylistId>,
    /// Export the selected playlist in this format.
    pub export: Option<library::PlaylistFormat>,
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
        // Reset the "was open" flag, so the next time the dialog opens it
        // takes the keyboard focus again.
        state.was_open = false;
        return actions;
    }

    let mut is_open = state.is_open;
    // A stable id, so the keyboard focus can be parked in the name field when
    // the studio opens.
    let new_name_field_id = egui::Id::unique("playlist_editor_new_name");

    let closed = crate::dialog::show_modal(
        ctx,
        "playlist_editor",
        "Playlist Studio",
        760.0,
        theme,
        |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_lg);

            // ── 1. Top Branded Header ──────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_lg, 0.0);
                let (icon_rect, _) = ui.allocate_exact_size(Vec2::new(28.0, 28.0), Sense::hover());
                ui.painter().rect_filled(
                    icon_rect,
                    CornerRadius::same(theme.radius_md),
                    theme.accent,
                );
                crate::components::icons::draw_list_icon(
                    ui,
                    icon_rect.shrink2(Vec2::splat(5.0)),
                    Color32::WHITE,
                );

                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xxs);
                    ui.label(
                        egui::RichText::new("Playlist Studio")
                            .font(crate::theme::font(16.0))
                            .strong()
                            .color(theme.text_primary),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Create custom playlists and curate tracks from your library",
                        )
                        .font(crate::theme::font(11.5))
                        .color(theme.text_muted),
                    );
                });
            });

            // ── 2. Create Playlist Hero Card ──────────────────────────────────
            let create_card = egui::Frame::NONE
                .fill(theme.bg_card)
                .stroke(Stroke::new(1.0, theme.border_card))
                .corner_radius(CornerRadius::same(theme.radius_lg))
                .inner_margin(Margin::symmetric(
                    theme.space_2xl as i8,
                    theme.space_xl as i8,
                ));

            create_card.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_md, 0.0);

                    let (plus_rect, _) =
                        ui.allocate_exact_size(Vec2::new(20.0, 20.0), Sense::hover());
                    crate::components::icons::draw_plus_circle(
                        ui,
                        plus_rect,
                        theme.accent_highlight,
                    );

                    let edit = egui::TextEdit::singleline(&mut state.new_name)
                        .id(new_name_field_id)
                        .hint_text(
                            "Enter new playlist name (e.g. Audiophile Favorites, Late Night)...",
                        )
                        .desired_width(ui.available_width() - 130.0)
                        .font(crate::theme::font(13.0));
                    let edit_resp = ui.add(edit);

                    let enter_pressed =
                        edit_resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));

                    let btn = egui::Button::new(
                        egui::RichText::new("+ Create Playlist")
                            .font(crate::theme::font(12.5))
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(theme.accent)
                    .corner_radius(CornerRadius::same(theme.radius_md))
                    .min_size(Vec2::new(115.0, 30.0));

                    if (ui.add(btn).clicked() || enter_pressed) && !state.new_name.trim().is_empty()
                    {
                        actions.create = Some(state.new_name.trim().to_string());
                        state.new_name.clear();
                    }
                });
            });

            ui.add_space(2.0);

            // ── 3. Two-Column Workspace ───────────────────────────────────────
            ui.horizontal_top(|ui| {
                let col_w = (ui.available_width() * 0.36).max(210.0);

                // ── Left Column: Playlists List ────────────────────────────────
                ui.allocate_ui_with_layout(
                    Vec2::new(col_w, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("Playlists ({})", playlists.len()))
                                    .font(crate::theme::font(13.5))
                                    .strong()
                                    .color(theme.text_primary),
                            );

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let can_export = state.selected.is_some() && !playlist_tracks.is_empty();
                                let btn = egui::Button::new(
                                    egui::RichText::new("Export")
                                        .font(crate::theme::font(11.0))
                                        .color(if can_export { theme.text_secondary } else { theme.text_muted }),
                                )
                                .fill(theme.bg_pill)
                                .corner_radius(CornerRadius::same(theme.radius_sm));

                                if ui
                                    .add_enabled(can_export, btn)
                                    .on_hover_text(format!(
                                        "Export selected playlist to .{}",
                                        state.export_format.extension()
                                    ))
                                    .clicked()
                                {
                                    actions.export = Some(state.export_format);
                                }

                                egui::ComboBox::from_id_salt("playlist_export_format")
                                    .selected_text(state.export_format.label())
                                    .width(100.0)
                                    .show_ui(ui, |ui| {
                                        for format in library::PlaylistFormat::ALL {
                                            ui.selectable_value(
                                                &mut state.export_format,
                                                format,
                                                format.label(),
                                            );
                                        }
                                    });
                            });
                        });

                        ui.add_space(4.0);

                        if playlists.is_empty() {
                            let empty_frame = egui::Frame::NONE
                                .fill(theme.bg_card)
                                .corner_radius(CornerRadius::same(theme.radius_lg))
                                .inner_margin(Margin::same(theme.space_2xl as i8));
                            empty_frame.show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new("No custom playlists yet.\nType a name above and click Create.")
                                        .font(crate::theme::font(12.0))
                                        .color(theme.text_muted),
                                );
                            });
                        }

                        egui::ScrollArea::vertical()
                            .id_salt("playlist_editor_list")
                            .auto_shrink([false, false])
                            .max_height(ui.available_height() - 6.0)
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xs);
                                for pl in playlists {
                                    let is_sel = state.selected == Some(pl.id);

                                    if state.rename_id == Some(pl.id) {
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xs, 0.0);
                                            let edit = egui::TextEdit::singleline(&mut state.rename_buf)
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
                                        Vec2::new(ui.available_width(), 34.0),
                                        egui::Sense::click(),
                                    );
                                    resp.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::Role::Button,
                                            true,
                                            format!("Playlist: {}", pl.name),
                                        )
                                    });
                                    let is_hovered = resp.hovered();

                                    let rename_rect = egui::Rect::from_center_size(
                                        egui::Pos2::new(row_rect.max.x - 30.0, row_rect.center().y),
                                        Vec2::new(16.0, 16.0),
                                    );
                                    let del_rect = egui::Rect::from_center_size(
                                        egui::Pos2::new(row_rect.max.x - 12.0, row_rect.center().y),
                                        Vec2::new(16.0, 16.0),
                                    );
                                    let rename_resp = ui.interact(rename_rect, ui.make_persistent_id(format!("ren_{}", pl.id)), Sense::click())
                                        .on_hover_text("Rename playlist");
                                    let del_resp = ui.interact(del_rect, ui.make_persistent_id(format!("del_{}", pl.id)), Sense::click())
                                        .on_hover_text("Delete playlist");

                                    if del_resp.clicked() {
                                        actions.delete = Some(pl.id);
                                    } else if rename_resp.clicked() {
                                        state.rename_id = Some(pl.id);
                                        state.rename_buf = pl.name.clone();
                                    } else if resp.clicked() {
                                        state.selected = Some(pl.id);
                                        state.search.clear();
                                        state.search_cache.clear();
                                        actions.select_changed = Some(pl.id);
                                    }

                                    let p = ui.painter_at(row_rect);
                                    let bg = if is_sel {
                                        theme.accent
                                    } else if is_hovered {
                                        theme.bg_card_hover
                                    } else {
                                        theme.bg_card
                                    };
                                    p.rect_filled(row_rect, CornerRadius::same(theme.radius_md), bg);

                                    // Playlist icon
                                    let icon_r = Rect::from_center_size(
                                        Pos2::new(row_rect.min.x + 14.0, row_rect.center().y),
                                        Vec2::new(12.0, 12.0),
                                    );
                                    crate::components::icons::draw_list_icon(
                                        ui,
                                        icon_r,
                                        if is_sel { Color32::WHITE } else { theme.accent_warm },
                                    );

                                    // Name
                                    let text_col = if is_sel { Color32::WHITE } else { theme.text_primary };
                                    let max_name_w = (row_rect.width() - 32.0 - 58.0).max(30.0);
                                    let mut job = egui::text::LayoutJob::simple_singleline(
                                        pl.name.clone(),
                                        crate::theme::font(12.5),
                                        text_col,
                                    );
                                    job.wrap.max_width = max_name_w;
                                    job.wrap.overflow_character = Some('…');
                                    let galley = p.layout_job(job);
                                    p.galley(
                                        Pos2::new(row_rect.min.x + 28.0, row_rect.center().y - galley.size().y * 0.5),
                                        galley,
                                        text_col,
                                    );

                                    // Count
                                    let count_col = if is_sel { theme.text_secondary } else { theme.text_muted };
                                    p.text(
                                        egui::Pos2::new(row_rect.max.x - 44.0, row_rect.center().y),
                                        egui::Align2::RIGHT_CENTER,
                                        format!("{}", pl.track_count),
                                        crate::theme::font(11.0),
                                        count_col,
                                    );

                                    // Action icons
                                    if is_hovered || is_sel {
                                        crate::components::icons::draw_more_menu(
                                            ui,
                                            rename_rect,
                                            if rename_resp.hovered() { theme.accent_warm } else { theme.text_muted },
                                        );
                                        crate::components::icons::draw_trash(
                                            ui,
                                            del_rect,
                                            if del_resp.hovered() { theme.accent_warm } else { theme.text_muted },
                                        );
                                    }
                                }
                            });
                    },
                );

                ui.separator();

                // ── Right Column: Selected Playlist Tracks & Add Songs ─────────
                ui.allocate_ui_with_layout(
                    Vec2::new(ui.available_width(), ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        let Some(selected_id) = state.selected else {
                            let empty_card = egui::Frame::NONE
                                .fill(theme.bg_card)
                                .corner_radius(CornerRadius::same(theme.radius_lg))
                                .inner_margin(Margin::same(theme.space_5xl as i8));
                            empty_card.show(ui, |ui| {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(20.0);
                                    let (ic_rect, _) = ui.allocate_exact_size(Vec2::new(36.0, 36.0), Sense::hover());
                                    crate::components::icons::draw_list_icon(ui, ic_rect, theme.accent);
                                    ui.add_space(8.0);
                                    ui.label(
                                        egui::RichText::new("No Playlist Selected")
                                            .font(crate::theme::font(15.0))
                                            .strong()
                                            .color(theme.text_primary),
                                    );
                                    ui.label(
                                        egui::RichText::new("Select a playlist from the left, or create a new one above to add songs.")
                                            .font(crate::theme::font(12.0))
                                            .color(theme.text_muted),
                                    );
                                    ui.add_space(20.0);
                                });
                            });
                            return;
                        };

                        let sel_name = playlists
                            .iter()
                            .find(|p| p.id == selected_id)
                            .map(|p| p.name.clone())
                            .unwrap_or_else(|| "Playlist".into());

                        // Header for active playlist
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("Playlist: {sel_name}"))
                                    .font(crate::theme::font(15.0))
                                    .strong()
                                    .color(theme.accent),
                            );
                            ui.label(
                                egui::RichText::new(format!("• {} song(s)", playlist_tracks.len()))
                                    .font(crate::theme::font(12.0))
                                    .color(theme.text_muted),
                            );
                        });

                        ui.add_space(2.0);

                        // Section 1: Current tracks in playlist
                        ui.label(
                            egui::RichText::new("Tracks in Playlist")
                                .font(crate::theme::font(12.0))
                                .strong()
                                .color(theme.text_secondary),
                        );

                        egui::ScrollArea::vertical()
                            .id_salt("playlist_editor_tracks")
                            .auto_shrink([false, false])
                            .max_height(150.0)
                            .show(ui, |ui| {
                                if playlist_tracks.is_empty() {
                                    ui.label(
                                        egui::RichText::new("Playlist is empty — search and add tracks below.")
                                            .font(crate::theme::font(11.5))
                                            .color(theme.text_muted),
                                    );
                                }
                                for (idx, t) in playlist_tracks.iter().enumerate() {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing = Vec2::new(theme.space_md, 0.0);

                                        let (up_r, up_resp) = ui.allocate_exact_size(
                                            Vec2::new(16.0, 18.0),
                                            Sense::click(),
                                        );
                                        up_resp.widget_info(|| {
                                            egui::WidgetInfo::labeled(
                                                egui::Role::Button,
                                                idx > 0,
                                                "Move up in the playlist",
                                            )
                                        });
                                        let up_col = if idx > 0 && up_resp.hovered() {
                                            theme.accent_highlight
                                        } else if idx > 0 {
                                            theme.text_muted
                                        } else {
                                            theme.text_dim
                                        };
                                        ui.painter().text(
                                            up_r.center(),
                                            egui::Align2::CENTER_CENTER,
                                            "↑",
                                            crate::theme::font(12.0),
                                            up_col,
                                        );
                                        if idx > 0
                                            && up_resp.on_hover_text("Move up").clicked()
                                        {
                                            actions.move_track =
                                                Some((selected_id, idx, idx - 1));
                                        }

                                        let (down_r, down_resp) = ui.allocate_exact_size(
                                            Vec2::new(16.0, 18.0),
                                            Sense::click(),
                                        );
                                        down_resp.widget_info(|| {
                                            egui::WidgetInfo::labeled(
                                                egui::Role::Button,
                                                idx + 1 < playlist_tracks.len(),
                                                "Move down in the playlist",
                                            )
                                        });
                                        let can_down = idx + 1 < playlist_tracks.len();
                                        let down_col = if can_down && down_resp.hovered() {
                                            theme.accent_highlight
                                        } else if can_down {
                                            theme.text_muted
                                        } else {
                                            theme.text_dim
                                        };
                                        ui.painter().text(
                                            down_r.center(),
                                            egui::Align2::CENTER_CENTER,
                                            "↓",
                                            crate::theme::font(12.0),
                                            down_col,
                                        );
                                        if can_down
                                            && down_resp.on_hover_text("Move down").clicked()
                                        {
                                            actions.move_track =
                                                Some((selected_id, idx, idx + 1));
                                        }

                                        let (del_r, del_resp) = ui.allocate_exact_size(Vec2::new(18.0, 18.0), Sense::click());
                                        let del_col = if del_resp.hovered() { theme.accent_warm } else { theme.text_muted };
                                        crate::components::icons::draw_window_close(ui, del_r, del_col);
                                        if del_resp.on_hover_text("Remove from playlist").clicked() {
                                            actions.remove_track = Some((selected_id, t.id));
                                        }

                                        ui.label(
                                            egui::RichText::new(format!("{}.", idx + 1))
                                                .font(crate::theme::font(11.0))
                                                .color(theme.text_muted),
                                        );

                                        ui.label(
                                            egui::RichText::new(&t.title)
                                                .font(crate::theme::font(12.0))
                                                .color(theme.text_primary),
                                        );

                                        ui.label(
                                            egui::RichText::new(format!("— {}", t.artist))
                                                .font(crate::theme::font(11.5))
                                                .color(theme.text_muted),
                                        );
                                    });
                                }
                            });

                        ui.add_space(4.0);
                        ui.separator();
                        ui.add_space(2.0);

                        // Section 2: Add songs from library
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Add Tracks from Library")
                                    .font(crate::theme::font(12.5))
                                    .strong()
                                    .color(theme.text_primary),
                            );
                            ui.label(
                                egui::RichText::new(format!("({} total songs)", library_tracks.len()))
                                    .font(crate::theme::font(11.0))
                                    .color(theme.text_muted),
                            );
                        });

                        let search_edit = egui::TextEdit::singleline(&mut state.search)
                            .hint_text("Search library by song title, artist, or album...")
                            .desired_width(ui.available_width())
                            .font(crate::theme::font(12.0));
                        ui.add(search_edit);

                        // Recompute matches only when search text changes
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
                                    n < 16
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
                            .max_height(ui.available_height() - 4.0)
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xs);
                                for &i in &state.results {
                                    let Some(t) = library_tracks.get(i) else {
                                        continue;
                                    };
                                    let already_in = playlist_tracks.iter().any(|pt| pt.id == t.id);

                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing = Vec2::new(theme.space_md, 0.0);

                                        let add_btn = egui::Button::new(
                                            egui::RichText::new(if already_in { "✓ Added" } else { "+ Add" })
                                                .font(crate::theme::font(11.0))
                                                .color(if already_in { theme.accent } else { Color32::WHITE }),
                                        )
                                        .fill(if already_in { theme.bg_pill } else { theme.accent })
                                        .corner_radius(CornerRadius::same(theme.radius_sm))
                                        .min_size(Vec2::new(56.0, 22.0));

                                        if ui.add(add_btn).on_hover_text("Add song to this playlist").clicked() {
                                            actions.add_track = Some((selected_id, t.id));
                                        }

                                        ui.label(
                                            egui::RichText::new(&t.title)
                                                .font(crate::theme::font(12.0))
                                                .color(theme.text_primary),
                                        );

                                        ui.label(
                                            egui::RichText::new(format!("• {}", t.artist))
                                                .font(crate::theme::font(11.5))
                                                .color(theme.text_secondary),
                                        );

                                        if !t.album.is_empty() {
                                            ui.label(
                                                egui::RichText::new(format!("({})", t.album))
                                                    .font(crate::theme::font(11.0))
                                                    .color(theme.text_muted),
                                            );
                                        }
                                    });
                                }
                            });
                    },
                );
            });
        },
    );
    if closed {
        is_open = false;
    }

    // Move the focus into the dialog when it opens, so the user can start
    // typing a name straight away.
    if is_open && !state.was_open {
        ctx.memory_mut(|m| m.request_focus(new_name_field_id));
    }
    state.was_open = is_open;

    state.is_open = is_open;
    actions
}
