use crate::services::ArtworkService;
use crate::theme::Theme;
use domain::ids::{PlaylistId, TrackId};
use domain::models::{Playlist, SortField, SortOrder, Track};
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, TextureHandle, Ui, Vec2,
};
use std::collections::HashSet;
use std::path::PathBuf;

/// Maximum number of new cover textures decoded per frame. Caps the work done in
/// a single frame so fast scrolling on a slow CPU can't cause a stutter.
const ART_LOADS_PER_FRAME: usize = 8;

/// Quality/format chips that narrow whatever view is open, so the user can
/// filter the current list without navigating away. Lives here (next to
/// [`SortField`]) because this is where the chips are rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QuickFilter {
    #[default]
    All,
    Lossless,
    HiRes,
    Favorites,
    Played,
}

impl QuickFilter {
    pub const ALL: [QuickFilter; 5] = [
        QuickFilter::All,
        QuickFilter::Lossless,
        QuickFilter::HiRes,
        QuickFilter::Favorites,
        QuickFilter::Played,
    ];

    pub fn label(self) -> &'static str {
        match self {
            QuickFilter::All => "All",
            QuickFilter::Lossless => "Lossless",
            QuickFilter::HiRes => "Hi-Res",
            QuickFilter::Favorites => "Favourites",
            QuickFilter::Played => "Played",
        }
    }

    /// Whether this chip narrows the list (i.e. is not the pass-through).
    pub fn is_active_filter(self) -> bool {
        self != QuickFilter::All
    }
}

pub struct SongsTableAction {
    pub play_track: Option<TrackId>,
    pub queue_next_track: Option<TrackId>,
    pub add_to_queue: Option<TrackId>,
    pub add_to_playlist: Option<(TrackId, PlaylistId)>,
    pub toggle_favorite: Option<TrackId>,
    pub sort_field_changed: Option<SortField>,
    /// User tapped the ascending/descending toggle.
    pub sort_order_changed: Option<SortOrder>,
    /// User tapped a quality/format chip.
    pub quick_filter_changed: Option<QuickFilter>,
    pub toggle_view_mode: bool,
    pub open_folder: Option<PathBuf>,
    pub show_properties: Option<TrackId>,
    pub remove_track: Option<TrackId>,
    /// User chose "Edit Song Info" from the context menu.
    pub edit_track: Option<TrackId>,

    // ── Bulk (multi-select) actions ─────────────────────────────────────────
    // The selected ids themselves live in `LibraryState`; these flags only say
    // *what* to do with them.
    pub play_selected: bool,
    pub queue_next_selected: bool,
    pub add_selected_to_queue: bool,
    pub add_selected_to_playlist: Option<PlaylistId>,
    /// Add every selected track to Favorites.
    pub favorite_selected: bool,
    /// Hand the selected tracks to the batch organizer.
    pub organize_selected: bool,
    pub remove_selected: bool,
    pub clear_selection: bool,
}

pub struct SongsTable<'a> {
    pub tracks: &'a [Track],
    pub active_track_id: Option<TrackId>,
    pub is_playing: bool,
    pub favorite_track_ids: &'a HashSet<TrackId>,
    pub playlists: &'a [Playlist],
    pub current_sort: SortField,
    /// Whether the current sort runs ascending or descending.
    pub sort_order: SortOrder,
    pub is_grid_view: bool,
    /// Active quality/format chip.
    pub quick_filter: QuickFilter,
    /// Multi-selection, shared with the app so it survives tab changes.
    pub selected_track_ids: &'a mut HashSet<TrackId>,
    /// Anchor for shift-range selection.
    pub last_clicked_index: &'a mut Option<usize>,
    /// Artwork service used to lazily fetch textures for visible rows only.
    pub artwork: &'a mut ArtworkService,
    pub theme: &'a Theme,
}

impl<'a> SongsTable<'a> {
    pub fn show(self, ui: &mut Ui) -> SongsTableAction {
        let mut action = SongsTableAction {
            play_track: None,
            queue_next_track: None,
            add_to_queue: None,
            add_to_playlist: None,
            toggle_favorite: None,
            sort_field_changed: None,
            sort_order_changed: None,
            quick_filter_changed: None,
            toggle_view_mode: false,
            open_folder: None,
            show_properties: None,
            remove_track: None,
            edit_track: None,
            play_selected: false,
            queue_next_selected: false,
            add_selected_to_queue: false,
            add_selected_to_playlist: None,
            favorite_selected: false,
            organize_selected: false,
            remove_selected: false,
            clear_selection: false,
        };

        // Split the borrows so the scroll-area closures can mutably borrow
        // `artwork`/`action` while reading the other fields.
        let SongsTable {
            tracks,
            active_track_id,
            is_playing,
            favorite_track_ids,
            playlists,
            current_sort,
            sort_order,
            is_grid_view,
            quick_filter,
            selected_track_ids,
            last_clicked_index,
            artwork,
            theme,
        } = self;

        let ctx = ui.ctx().clone();
        let mut art_budget = ART_LOADS_PER_FRAME;

        // 1. Section Header
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(12.0, 0.0);

            ui.label(
                egui::RichText::new("Songs")
                    .font(FontId::proportional(22.0))
                    .strong()
                    .color(theme.text_primary),
            );

            ui.label(
                egui::RichText::new(if quick_filter.is_active_filter() {
                    format!("{} songs · {} filter", tracks.len(), quick_filter.label())
                } else {
                    format!("{} songs", tracks.len())
                })
                .font(FontId::proportional(13.0))
                .color(theme.text_muted),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);

                // View Mode Toggles (Grid vs List)
                let grid_bg = if is_grid_view {
                    theme.accent_violet
                } else {
                    theme.bg_pill
                };
                let list_bg = if !is_grid_view {
                    theme.accent_violet
                } else {
                    theme.bg_pill
                };

                let (grid_rect, grid_resp) =
                    ui.allocate_exact_size(Vec2::new(30.0, 30.0), Sense::click());
                ui.painter()
                    .rect_filled(grid_rect, CornerRadius::same(8), grid_bg);
                if !is_grid_view {
                    ui.painter().rect_stroke(
                        grid_rect,
                        CornerRadius::same(8),
                        Stroke::new(1.0_f32, theme.border_subtle),
                        egui::StrokeKind::Inside,
                    );
                }
                crate::components::icons::draw_grid_icon(
                    ui,
                    grid_rect.shrink2(Vec2::new(7.0, 7.0)),
                    if is_grid_view {
                        Color32::WHITE
                    } else {
                        theme.text_muted
                    },
                );
                if grid_resp.clicked() && !is_grid_view {
                    action.toggle_view_mode = true;
                }

                let (list_rect, list_resp) =
                    ui.allocate_exact_size(Vec2::new(30.0, 30.0), Sense::click());
                ui.painter()
                    .rect_filled(list_rect, CornerRadius::same(8), list_bg);
                crate::components::icons::draw_list_icon(
                    ui,
                    list_rect.shrink2(Vec2::new(7.0, 7.0)),
                    if !is_grid_view {
                        Color32::WHITE
                    } else {
                        theme.text_muted
                    },
                );
                if list_resp.clicked() && is_grid_view {
                    action.toggle_view_mode = true;
                }

                ui.add_space(6.0);

                // Sort Dropdown Pill
                let sort_name = match current_sort {
                    SortField::Artist => "Artist",
                    SortField::Album => "Album",
                    SortField::Duration => "Duration",
                    SortField::DateAdded => "Added",
                    SortField::Favorite => "Favourites",
                    SortField::Title => "Title",
                    SortField::TrackNumber => "Track",
                };
                let sort_label = format!("Sort by: {}", sort_name);

                let (sort_rect, sort_resp) =
                    ui.allocate_exact_size(Vec2::new(124.0, 30.0), Sense::click());
                let sp = ui.painter_at(sort_rect);
                let sort_bg = if sort_resp.hovered() {
                    theme.bg_card_hover
                } else {
                    theme.bg_pill
                };
                sp.rect_filled(sort_rect, CornerRadius::same(15), sort_bg);
                sp.rect_stroke(
                    sort_rect,
                    CornerRadius::same(15),
                    Stroke::new(1.0_f32, theme.border_subtle),
                    egui::StrokeKind::Inside,
                );
                sp.text(
                    Pos2::new(sort_rect.min.x + 14.0, sort_rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    &sort_label,
                    FontId::proportional(12.5),
                    theme.text_primary,
                );
                let chev_rect = Rect::from_center_size(
                    Pos2::new(sort_rect.max.x - 14.0, sort_rect.center().y),
                    Vec2::new(12.0, 12.0),
                );
                crate::components::icons::draw_chevron_down(ui, chev_rect, theme.text_muted);

                if sort_resp.on_hover_text("Change the sort field").clicked() {
                    // A deliberate cycle rather than a menu: one click walks
                    // the fields, and the direction is a separate button so the
                    // two never fight each other.
                    let next = match current_sort {
                        SortField::TrackNumber => SortField::Title,
                        SortField::Title => SortField::Artist,
                        SortField::Artist => SortField::Album,
                        SortField::Album => SortField::Duration,
                        SortField::Duration => SortField::DateAdded,
                        SortField::DateAdded => SortField::Favorite,
                        SortField::Favorite => SortField::TrackNumber,
                    };
                    action.sort_field_changed = Some(next);
                }

                // Sort direction toggle
                let (dir_rect, dir_resp) =
                    ui.allocate_exact_size(Vec2::new(30.0, 30.0), Sense::click());
                let dp = ui.painter_at(dir_rect);
                let dir_bg = if dir_resp.hovered() {
                    theme.bg_card_hover
                } else {
                    theme.bg_pill
                };
                dp.rect_filled(dir_rect, CornerRadius::same(8), dir_bg);
                dp.rect_stroke(
                    dir_rect,
                    CornerRadius::same(8),
                    Stroke::new(1.0_f32, theme.border_subtle),
                    egui::StrokeKind::Inside,
                );
                let ascending = sort_order == SortOrder::Ascending;
                let arrow_col = if dir_resp.hovered() {
                    theme.accent_pink
                } else {
                    theme.text_secondary
                };
                let cy = dir_rect.center().y;
                let cx = dir_rect.center().x;
                if ascending {
                    // ↑ : tip at the top, barbs below it
                    dp.add(egui::Shape::line(
                        vec![Pos2::new(cx, cy - 6.0), Pos2::new(cx, cy + 6.0)],
                        Stroke::new(1.8_f32, arrow_col),
                    ));
                    dp.add(egui::Shape::line(
                        vec![
                            Pos2::new(cx - 4.5, cy - 1.5),
                            Pos2::new(cx, cy - 6.0),
                            Pos2::new(cx + 4.5, cy - 1.5),
                        ],
                        Stroke::new(1.8_f32, arrow_col),
                    ));
                } else {
                    dp.add(egui::Shape::line(
                        vec![Pos2::new(cx, cy - 6.0), Pos2::new(cx, cy + 6.0)],
                        Stroke::new(1.8_f32, arrow_col),
                    ));
                    dp.add(egui::Shape::line(
                        vec![
                            Pos2::new(cx - 4.5, cy + 1.5),
                            Pos2::new(cx, cy + 6.0),
                            Pos2::new(cx + 4.5, cy + 1.5),
                        ],
                        Stroke::new(1.8_f32, arrow_col),
                    ));
                }
                if dir_resp
                    .on_hover_text(if ascending {
                        "Ascending — click for descending"
                    } else {
                        "Descending — click for ascending"
                    })
                    .clicked()
                {
                    action.sort_order_changed = Some(if ascending {
                        SortOrder::Descending
                    } else {
                        SortOrder::Ascending
                    });
                }
            });
        });

        ui.add_space(6.0);

        // Quality/format chips: narrow the current view in place.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
            for filter in QuickFilter::ALL {
                let selected = quick_filter == filter;
                let (fill, text_color) = if selected {
                    (theme.accent_violet, Color32::WHITE)
                } else {
                    (theme.bg_pill, theme.text_secondary)
                };
                let btn = egui::Button::new(
                    egui::RichText::new(filter.label())
                        .font(FontId::proportional(12.0))
                        .color(text_color),
                )
                .fill(fill)
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius::same(12))
                .min_size(Vec2::new(0.0, 24.0));
                if ui.add(btn).clicked() && !selected {
                    action.quick_filter_changed = Some(filter);
                }
            }

            if quick_filter.is_active_filter() {
                ui.add_space(8.0);
                let clear = egui::Button::new(
                    egui::RichText::new("Clear filter")
                        .font(FontId::proportional(11.5))
                        .color(theme.text_muted),
                )
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::NONE);
                if ui.add(clear).clicked() {
                    action.quick_filter_changed = Some(QuickFilter::All);
                }
            }
        });

        ui.add_space(8.0);

        // Bulk action bar — only shown while a selection exists.
        if !selected_track_ids.is_empty() {
            render_bulk_bar(ui, selected_track_ids.len(), playlists, theme, &mut action);
            ui.add_space(6.0);
        }

        if is_grid_view {
            // ── Grid View (virtualized in horizontal bands) ─────────────────
            let card_w = 140.0;
            let card_h = 180.0;
            let gap = 12.0;
            let avail_w = ui.available_width();
            let cols = (((avail_w + gap) / (card_w + gap)).floor()).max(1.0) as usize;
            let total_rows = tracks.len().div_ceil(cols.max(1));
            let band_h = card_h + gap;

            egui::ScrollArea::vertical()
                .id_salt("songs_grid_scroll")
                .auto_shrink([false, false])
                .show_rows(ui, band_h, total_rows, |ui, row_range| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
                    for row in row_range {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(gap, 0.0);
                            for col in 0..cols {
                                let idx = row * cols + col;
                                if idx >= tracks.len() {
                                    break;
                                }
                                let track = &tracks[idx];
                                render_grid_card(
                                    ui,
                                    track,
                                    idx,
                                    card_w,
                                    card_h,
                                    active_track_id,
                                    favorite_track_ids,
                                    playlists,
                                    selected_track_ids,
                                    last_clicked_index,
                                    artwork,
                                    &ctx,
                                    &mut art_budget,
                                    theme,
                                    &mut action,
                                );
                            }
                        });
                    }
                });
        } else {
            // ── List View (virtualized, renders only visible rows) ──────────
            let col_num_w = 54.0;
            let col_time_w = 52.0;
            let col_fav_w = 32.0;
            let col_menu_w = 26.0;
            let row_h = 48.0;

            // Static column headers
            let full_width = ui.available_width();
            let fixed_w = col_num_w + col_time_w + col_fav_w + col_menu_w + 40.0;
            let remaining_w = (full_width - fixed_w).max(200.0);
            let title_w = remaining_w * 0.38;
            let artist_w = remaining_w * 0.31;
            let album_w = remaining_w * 0.31;

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);

                let (num_h_rect, num_h_resp) =
                    ui.allocate_exact_size(Vec2::new(col_num_w, 20.0), Sense::click());
                // Select-all checkbox, independent of the column sort target.
                let all_selected =
                    !tracks.is_empty() && tracks.iter().all(|t| selected_track_ids.contains(&t.id));
                let any_selected = tracks.iter().any(|t| selected_track_ids.contains(&t.id));
                let h_painter = ui.painter_at(num_h_rect);
                let cb = Rect::from_center_size(
                    Pos2::new(num_h_rect.min.x + 16.0, num_h_rect.center().y),
                    Vec2::new(15.0, 15.0),
                );
                paint_checkbox(
                    &h_painter,
                    cb,
                    all_selected,
                    any_selected && !all_selected,
                    theme,
                );
                h_painter.text(
                    Pos2::new(num_h_rect.min.x + 42.0, num_h_rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    "#",
                    FontId::proportional(11.8),
                    theme.text_muted,
                );
                if num_h_resp
                    .on_hover_text("Click the box to select all · click # to sort")
                    .clicked()
                {
                    let pointer = ui.input(|i| i.pointer.interact_pos());
                    let over_box = pointer
                        .map(|p| p.x < num_h_rect.min.x + 30.0)
                        .unwrap_or(false);
                    if over_box {
                        if all_selected {
                            selected_track_ids.clear();
                        } else {
                            for t in tracks.iter() {
                                selected_track_ids.insert(t.id);
                            }
                        }
                        *last_clicked_index = None;
                    } else {
                        action.sort_field_changed = Some(SortField::TrackNumber);
                    }
                }

                let (title_h_rect, title_h_resp) =
                    ui.allocate_exact_size(Vec2::new(title_w, 20.0), Sense::click());
                let title_col = if current_sort == SortField::Title {
                    theme.accent_pink
                } else {
                    theme.text_muted
                };
                ui.painter_at(title_h_rect).text(
                    Pos2::new(title_h_rect.min.x + 40.0, title_h_rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    "TITLE",
                    FontId::proportional(11.8),
                    title_col,
                );
                if title_h_resp.on_hover_text("Sort by Title").clicked() {
                    action.sort_field_changed = Some(SortField::Title);
                }

                let (artist_h_rect, artist_h_resp) =
                    ui.allocate_exact_size(Vec2::new(artist_w, 20.0), Sense::click());
                let artist_col = if current_sort == SortField::Artist {
                    theme.accent_pink
                } else {
                    theme.text_muted
                };
                ui.painter_at(artist_h_rect).text(
                    artist_h_rect.left_center(),
                    egui::Align2::LEFT_CENTER,
                    "ARTIST",
                    FontId::proportional(11.8),
                    artist_col,
                );
                if artist_h_resp.on_hover_text("Sort by Artist").clicked() {
                    action.sort_field_changed = Some(SortField::Artist);
                }

                let (album_h_rect, album_h_resp) =
                    ui.allocate_exact_size(Vec2::new(album_w, 20.0), Sense::click());
                let album_col = if current_sort == SortField::Album {
                    theme.accent_pink
                } else {
                    theme.text_muted
                };
                ui.painter_at(album_h_rect).text(
                    album_h_rect.left_center(),
                    egui::Align2::LEFT_CENTER,
                    "ALBUM",
                    FontId::proportional(11.8),
                    album_col,
                );
                if album_h_resp.on_hover_text("Sort by Album").clicked() {
                    action.sort_field_changed = Some(SortField::Album);
                }

                let (clock_rect, clock_resp) =
                    ui.allocate_exact_size(Vec2::new(col_time_w, 20.0), Sense::click());
                let clock_col = if current_sort == SortField::Duration {
                    theme.accent_pink
                } else {
                    theme.text_muted
                };
                let clock_icon_rect =
                    Rect::from_center_size(clock_rect.center(), Vec2::new(14.0, 14.0));
                crate::components::icons::draw_clock(ui, clock_icon_rect, clock_col);
                if clock_resp.on_hover_text("Sort by Duration").clicked() {
                    action.sort_field_changed = Some(SortField::Duration);
                }

                let (heart_rect, heart_resp) =
                    ui.allocate_exact_size(Vec2::new(col_fav_w, 20.0), Sense::click());
                let heart_icon_rect =
                    Rect::from_center_size(heart_rect.center(), Vec2::new(14.0, 14.0));
                crate::components::icons::draw_heart(ui, heart_icon_rect, theme.text_muted, false);
                if heart_resp.on_hover_text("Sort by Favourites").clicked() {
                    action.sort_field_changed = Some(SortField::Favorite);
                }

                ui.add_sized([col_menu_w, 20.0], egui::Label::new(""));
            });

            ui.add_space(4.0);

            egui::ScrollArea::vertical()
                .id_salt("songs_table_scroll")
                .auto_shrink([false, false])
                .show_rows(ui, row_h, tracks.len(), |ui, row_range| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);

                    for idx in row_range {
                        let track = &tracks[idx];
                        let is_active = active_track_id == Some(track.id);
                        let is_favorite = favorite_track_ids.contains(&track.id);
                        let is_selected = selected_track_ids.contains(&track.id);
                        let full_width = ui.available_width();

                        let (row_rect, row_resp) =
                            ui.allocate_exact_size(Vec2::new(full_width, row_h), Sense::click());
                        // Visual row is 46px tall inside the 48px slot (2px gap).
                        let vis_rect = Rect::from_min_max(
                            Pos2::new(row_rect.min.x, row_rect.min.y + 1.0),
                            Pos2::new(row_rect.max.x, row_rect.max.y - 1.0),
                        );
                        let cy = vis_rect.center().y;

                        let painter = ui.painter_at(vis_rect);
                        let is_hovered =
                            row_resp.hovered() && std::env::var("SHADOWPLAYER_EVAL").is_err();

                        if is_active {
                            painter.rect_filled(
                                vis_rect,
                                CornerRadius::same(8),
                                theme.bg_card_active,
                            );
                            painter.rect_stroke(
                                vis_rect,
                                CornerRadius::same(8),
                                Stroke::new(1.0_f32, theme.accent_violet),
                                egui::StrokeKind::Inside,
                            );
                        } else if is_selected {
                            painter.rect_filled(
                                vis_rect,
                                CornerRadius::same(8),
                                theme.bg_card_hover,
                            );
                            painter.rect_stroke(
                                vis_rect,
                                CornerRadius::same(8),
                                Stroke::new(1.0_f32, theme.accent_blue),
                                egui::StrokeKind::Inside,
                            );
                        } else if is_hovered {
                            painter.rect_filled(
                                vis_rect,
                                CornerRadius::same(8),
                                theme.bg_card_hover,
                            );
                        }

                        // Interaction zones that must NOT trigger playback.
                        let fav_rect = Rect::from_center_size(
                            Pos2::new(vis_rect.max.x - col_menu_w - 16.0, cy),
                            Vec2::new(24.0, 24.0),
                        );
                        let menu_rect = Rect::from_center_size(
                            Pos2::new(vis_rect.max.x - 12.0, cy),
                            Vec2::new(20.0, 20.0),
                        );
                        // Selection checkbox lives in the first column.
                        let check_rect = Rect::from_center_size(
                            Pos2::new(vis_rect.min.x + 16.0, cy),
                            Vec2::new(20.0, 20.0),
                        );
                        let pointer = ui.input(|i| i.pointer.interact_pos());
                        let over_widget = pointer
                            .map(|p| fav_rect.contains(p) || menu_rect.contains(p))
                            .unwrap_or(false);
                        let over_check = pointer.map(|p| check_rect.contains(p)).unwrap_or(false);

                        // Click behaviour: plain click plays (matching the queue),
                        // the checkbox toggles, Ctrl/Cmd toggles, Shift extends.
                        if row_resp.clicked() && !over_widget {
                            let modifiers = ui.input(|i| i.modifiers);
                            if modifiers.shift && last_clicked_index.is_some() {
                                let anchor = last_clicked_index.unwrap_or(idx);
                                let (lo, hi) = if anchor <= idx {
                                    (anchor, idx)
                                } else {
                                    (idx, anchor)
                                };
                                for i in lo..=hi {
                                    if let Some(t) = tracks.get(i) {
                                        selected_track_ids.insert(t.id);
                                    }
                                }
                                *last_clicked_index = Some(idx);
                            } else if modifiers.command || modifiers.ctrl || over_check {
                                if !selected_track_ids.remove(&track.id) {
                                    selected_track_ids.insert(track.id);
                                }
                                *last_clicked_index = Some(idx);
                            } else {
                                action.play_track = Some(track.id);
                                *last_clicked_index = Some(idx);
                            }
                        }

                        // Column 1: selection box, else hover play button, else index.
                        if is_selected {
                            paint_checkbox(&painter, check_rect.shrink(3.0), true, false, theme);
                        } else if is_hovered {
                            paint_checkbox(&painter, check_rect.shrink(3.0), false, false, theme);
                            let play_icon_rect = Rect::from_center_size(
                                Pos2::new(vis_rect.min.x + 40.0, cy),
                                Vec2::new(14.0, 14.0),
                            );
                            let t1 =
                                Pos2::new(play_icon_rect.min.x + 11.0, play_icon_rect.center().y);
                            let t2 = Pos2::new(
                                play_icon_rect.min.x + 3.0,
                                play_icon_rect.center().y - 5.5,
                            );
                            let t3 = Pos2::new(
                                play_icon_rect.min.x + 3.0,
                                play_icon_rect.center().y + 5.5,
                            );
                            painter.add(egui::Shape::convex_polygon(
                                vec![t1, t2, t3],
                                Color32::WHITE,
                                Stroke::NONE,
                            ));
                        } else {
                            let num_text = format!("{}", idx + 1);
                            let num_col = if is_active {
                                theme.accent_magenta
                            } else {
                                theme.text_muted
                            };
                            painter.text(
                                Pos2::new(vis_rect.min.x + 34.0, cy),
                                egui::Align2::LEFT_CENTER,
                                &num_text,
                                FontId::proportional(12.8),
                                num_col,
                            );
                        }

                        // Column 2: Thumbnail + Title + Equalizer Indicator
                        let thumb_rect = Rect::from_min_size(
                            Pos2::new(vis_rect.min.x + col_num_w, cy - 18.0),
                            Vec2::new(36.0, 36.0),
                        );

                        if let Some(tex) = art_for(artwork, &ctx, &mut art_budget, track) {
                            egui::Image::new(&tex)
                                .corner_radius(CornerRadius::same(6))
                                .fit_to_exact_size(Vec2::new(36.0, 36.0))
                                .paint_at(ui, thumb_rect);
                        } else {
                            painter.rect_filled(thumb_rect, CornerRadius::same(6), theme.bg_card);
                            crate::components::icons::draw_disc(
                                ui,
                                thumb_rect.shrink2(Vec2::new(8.0, 8.0)),
                                theme.accent_pink,
                            );
                        }

                        // The playing track's title is tinted as well as its row
                        // being highlighted: the two branches here used to be
                        // identical, so the highlight was a no-op.
                        let active_text = if is_active {
                            theme.accent_violet
                        } else {
                            theme.text_primary
                        };
                        let title_x = vis_rect.min.x + col_num_w + 44.0;
                        let title_galley = painter.layout_no_wrap(
                            track.title.clone(),
                            FontId::proportional(13.8),
                            active_text,
                        );
                        let title_w_actual = title_galley.size().x;
                        painter.galley(
                            Pos2::new(title_x, cy - title_galley.size().y * 0.5),
                            title_galley,
                            active_text,
                        );

                        // Equalizer Indicator if currently playing
                        if is_active && is_playing {
                            let eq_x = title_x + title_w_actual + 10.0;
                            let bar_heights = [6.0, 14.0, 11.0, 7.0];
                            for (b, &bh) in bar_heights.iter().enumerate() {
                                let bx = eq_x + b as f32 * 3.5;
                                let bar_r = Rect::from_min_max(
                                    Pos2::new(bx, cy + 6.0 - bh),
                                    Pos2::new(bx + 2.2, cy + 6.0),
                                );
                                painter.rect_filled(
                                    bar_r,
                                    CornerRadius::same(1),
                                    theme.accent_magenta,
                                );
                            }
                        }

                        // Column 3: Artist
                        let artist_x = vis_rect.min.x + col_num_w + title_w + 8.0;
                        painter.text(
                            Pos2::new(artist_x, cy),
                            egui::Align2::LEFT_CENTER,
                            &track.artist,
                            FontId::proportional(12.8),
                            if is_active {
                                theme.text_primary
                            } else {
                                theme.text_secondary
                            },
                        );

                        // Column 4: Album
                        let album_x = vis_rect.min.x + col_num_w + title_w + artist_w + 12.0;
                        painter.text(
                            Pos2::new(album_x, cy),
                            egui::Align2::LEFT_CENTER,
                            if track.album.is_empty() {
                                "—"
                            } else {
                                &track.album
                            },
                            FontId::proportional(12.5),
                            if is_active {
                                theme.text_secondary
                            } else {
                                theme.text_muted
                            },
                        );

                        // Column 5: Duration
                        let dur_x = vis_rect.max.x - col_menu_w - col_fav_w - col_time_w + 4.0;
                        let dur_str = format_duration(track.duration_secs as f64);
                        painter.text(
                            Pos2::new(dur_x, cy),
                            egui::Align2::LEFT_CENTER,
                            &dur_str,
                            FontId::proportional(12.5),
                            if is_active {
                                theme.text_secondary
                            } else {
                                theme.text_muted
                            },
                        );

                        // Column 6: Heart Favorite Toggle
                        let fav_hover = ui.rect_contains_pointer(fav_rect);
                        let heart_col = if is_favorite {
                            theme.accent_pink
                        } else if fav_hover {
                            theme.text_primary
                        } else {
                            theme.text_muted
                        };
                        crate::components::icons::draw_heart(ui, fav_rect, heart_col, is_favorite);
                        if fav_hover && ui.input(|i| i.pointer.primary_clicked()) {
                            action.toggle_favorite = Some(track.id);
                        }

                        // Column 7: Overflow Menu "⋮"
                        let menu_hover = ui.rect_contains_pointer(menu_rect);
                        let menu_resp = ui.interact(
                            menu_rect,
                            ui.scope_id().with(("row_menu", idx)),
                            Sense::click(),
                        );
                        crate::components::icons::draw_more_menu(
                            ui,
                            menu_rect,
                            if menu_hover || is_active {
                                theme.text_primary
                            } else {
                                theme.text_secondary
                            },
                        );
                        // The ⋮ used to be painted with no handler at all, so
                        // it looked clickable and did nothing. It now opens the
                        // same menu as a right-click on the row.
                        egui::Popup::menu(&menu_resp)
                            .id(menu_resp.id.with("popup"))
                            .show(|ui| {
                                render_track_context_menu(
                                    ui,
                                    track,
                                    is_favorite,
                                    playlists,
                                    &mut action,
                                    theme,
                                );
                            });

                        // Right-Click Context Menu on Row
                        row_resp.context_menu(|ui| {
                            render_track_context_menu(
                                ui,
                                track,
                                is_favorite,
                                playlists,
                                &mut action,
                                theme,
                            );
                        });
                    }
                });
        }

        action
    }
}

/// Paint a small checkbox: filled with a tick when checked, a dash when
/// indeterminate, otherwise an empty outline.
fn paint_checkbox(
    painter: &egui::Painter,
    rect: Rect,
    checked: bool,
    indeterminate: bool,
    theme: &Theme,
) {
    let radius = CornerRadius::same(4);
    if checked || indeterminate {
        painter.rect_filled(rect, radius, theme.accent_blue);
        let c = rect.center();
        if checked {
            let s = rect.width() * 0.26;
            painter.add(egui::Shape::line(
                vec![
                    Pos2::new(c.x - s, c.y + s * 0.05),
                    Pos2::new(c.x - s * 0.25, c.y + s * 0.75),
                    Pos2::new(c.x + s, c.y - s * 0.7),
                ],
                Stroke::new(1.7_f32, Color32::WHITE),
            ));
        } else {
            painter.line_segment(
                [
                    Pos2::new(rect.min.x + rect.width() * 0.28, c.y),
                    Pos2::new(rect.max.x - rect.width() * 0.28, c.y),
                ],
                Stroke::new(1.7_f32, Color32::WHITE),
            );
        }
    } else {
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(1.2_f32, theme.text_muted),
            egui::StrokeKind::Inside,
        );
    }
}

/// The context-sensitive action strip shown while tracks are multi-selected.
fn render_bulk_bar(
    ui: &mut Ui,
    count: usize,
    playlists: &[Playlist],
    theme: &Theme,
    action: &mut SongsTableAction,
) {
    egui::Frame::NONE
        .fill(theme.bg_card)
        .corner_radius(CornerRadius::same(8))
        .stroke(Stroke::new(1.0_f32, theme.accent_blue))
        .inner_margin(egui::Margin {
            left: 10,
            right: 10,
            top: 6,
            bottom: 6,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                ui.label(
                    egui::RichText::new(format!("{count} selected"))
                        .font(FontId::proportional(12.5))
                        .strong()
                        .color(theme.accent_blue),
                );
                ui.add_space(4.0);

                if ui
                    .button(egui::RichText::new("Play").font(FontId::proportional(12.0)))
                    .clicked()
                {
                    action.play_selected = true;
                }
                if ui
                    .button(egui::RichText::new("Play Next").font(FontId::proportional(12.0)))
                    .clicked()
                {
                    action.queue_next_selected = true;
                }
                if ui
                    .button(egui::RichText::new("Add to Queue").font(FontId::proportional(12.0)))
                    .clicked()
                {
                    action.add_selected_to_queue = true;
                }
                if ui
                    .button(egui::RichText::new("Favorite").font(FontId::proportional(12.0)))
                    .clicked()
                {
                    action.favorite_selected = true;
                }
                if ui
                    .button(egui::RichText::new("Organize").font(FontId::proportional(12.0)))
                    .clicked()
                {
                    action.organize_selected = true;
                }
                if !playlists.is_empty() {
                    ui.menu_button(
                        egui::RichText::new("Add to Playlist").font(FontId::proportional(12.0)),
                        |ui| {
                            for pl in playlists {
                                if ui.button(&pl.name).clicked() {
                                    action.add_selected_to_playlist = Some(pl.id);
                                    ui.close();
                                }
                            }
                        },
                    );
                }
                if ui
                    .button(
                        egui::RichText::new("Remove")
                            .font(FontId::proportional(12.0))
                            .color(theme.destructive),
                    )
                    .clicked()
                {
                    action.remove_selected = true;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(egui::RichText::new("Clear").font(FontId::proportional(12.0)))
                        .clicked()
                    {
                        action.clear_selection = true;
                    }
                });
            });
        });
}

#[allow(clippy::too_many_arguments)]
fn render_grid_card(
    ui: &mut Ui,
    track: &Track,
    _idx: usize,
    card_w: f32,
    card_h: f32,
    active_track_id: Option<TrackId>,
    favorite_track_ids: &HashSet<TrackId>,
    playlists: &[Playlist],
    selected_track_ids: &mut HashSet<TrackId>,
    last_clicked_index: &mut Option<usize>,
    artwork: &mut ArtworkService,
    ctx: &egui::Context,
    art_budget: &mut usize,
    theme: &Theme,
    action: &mut SongsTableAction,
) {
    let is_cur = active_track_id == Some(track.id);
    let is_fav = favorite_track_ids.contains(&track.id);
    let is_selected = selected_track_ids.contains(&track.id);

    let (rect, resp) = ui.allocate_exact_size(Vec2::new(card_w, card_h), Sense::click());
    if resp.clicked() {
        let modifiers = ui.input(|i| i.modifiers);
        if modifiers.command || modifiers.ctrl || modifiers.shift {
            if !selected_track_ids.remove(&track.id) {
                selected_track_ids.insert(track.id);
            }
            *last_clicked_index = None;
        } else {
            action.play_track = Some(track.id);
        }
    }

    let p = ui.painter_at(rect);
    let bg = if is_cur {
        theme.bg_card_active
    } else if resp.hovered() || is_selected {
        theme.bg_card_hover
    } else {
        theme.bg_card
    };
    p.rect_filled(rect, CornerRadius::same(10), bg);
    let stroke_col = if is_cur {
        theme.accent_violet
    } else if is_selected {
        theme.accent_blue
    } else {
        theme.border_card
    };
    p.rect_stroke(
        rect,
        CornerRadius::same(10),
        Stroke::new(1.0_f32, stroke_col),
        egui::StrokeKind::Inside,
    );

    if is_selected {
        let cb = Rect::from_center_size(
            Pos2::new(rect.min.x + 18.0, rect.min.y + 18.0),
            Vec2::new(16.0, 16.0),
        );
        paint_checkbox(&p, cb, true, false, theme);
    }

    let art_size = Vec2::new(card_w - 16.0, card_w - 16.0);
    let art_rect = Rect::from_min_size(Pos2::new(rect.min.x + 8.0, rect.min.y + 8.0), art_size);

    if let Some(tex) = art_for(artwork, ctx, art_budget, track) {
        egui::Image::new(&tex)
            .corner_radius(CornerRadius::same(8))
            .fit_to_exact_size(art_size)
            .paint_at(ui, art_rect);
    } else {
        p.rect_filled(art_rect, CornerRadius::same(8), theme.bg_pill);
        crate::components::icons::draw_disc(
            ui,
            art_rect.shrink2(Vec2::new(30.0, 30.0)),
            theme.accent_pink,
        );
    }

    if resp.hovered() {
        p.circle_filled(
            art_rect.center(),
            18.0,
            Color32::from_rgba_premultiplied(0, 0, 0, 160),
        );
        let play_icon_rect = Rect::from_center_size(art_rect.center(), Vec2::new(16.0, 16.0));
        crate::components::icons::draw_play(ui, play_icon_rect, Color32::WHITE, true);
    }

    let title_g = p.layout_no_wrap(
        track.title.clone(),
        FontId::proportional(12.8),
        theme.text_primary,
    );
    p.galley(
        Pos2::new(rect.min.x + 8.0, art_rect.max.y + 6.0),
        title_g,
        theme.text_primary,
    );

    let artist_g = p.layout_no_wrap(
        track.artist.clone(),
        FontId::proportional(11.2),
        theme.text_muted,
    );
    p.galley(
        Pos2::new(rect.min.x + 8.0, art_rect.max.y + 22.0),
        artist_g,
        theme.text_muted,
    );

    resp.context_menu(|ui| {
        render_track_context_menu(ui, track, is_fav, playlists, action, theme);
    });
}

/// Fetch (or lazily decode, within budget) a cover texture for `track`.
fn art_for(
    artwork: &mut ArtworkService,
    ctx: &egui::Context,
    budget: &mut usize,
    track: &Track,
) -> Option<TextureHandle> {
    if artwork.get_texture(&track.id).is_none() && *budget > 0 && !artwork.is_missing(&track.id) {
        *budget -= 1;
        artwork.ensure_loaded(ctx, track);
    }
    let tex = artwork.get_texture(&track.id).cloned();
    if tex.is_some() {
        artwork.touch(&track.id);
    }
    tex
}

fn render_track_context_menu(
    ui: &mut Ui,
    track: &Track,
    is_fav: bool,
    playlists: &[Playlist],
    action: &mut SongsTableAction,
    theme: &Theme,
) {
    if ui.button("Play Now").clicked() {
        action.play_track = Some(track.id);
        ui.close();
    }
    if ui.button("Play Next").clicked() {
        action.queue_next_track = Some(track.id);
        ui.close();
    }
    if ui.button("Add to Queue").clicked() {
        action.add_to_queue = Some(track.id);
        ui.close();
    }

    ui.separator();

    let fav_text = if is_fav {
        "Remove from Favorites"
    } else {
        "Add to Favorites"
    };
    if ui.button(fav_text).clicked() {
        action.toggle_favorite = Some(track.id);
        ui.close();
    }

    if !playlists.is_empty() {
        ui.menu_button("Add to Playlist", |ui| {
            for pl in playlists {
                if ui.button(&pl.name).clicked() {
                    action.add_to_playlist = Some((track.id, pl.id));
                    ui.close();
                }
            }
        });
    }

    ui.separator();

    if ui.button("Open Containing Folder").clicked() {
        if let Some(parent) = track.path.parent() {
            action.open_folder = Some(parent.to_path_buf());
        }
        ui.close();
    }

    if ui.button("Track Properties").clicked() {
        action.show_properties = Some(track.id);
        ui.close();
    }

    if ui.button("Edit Song Info").clicked() {
        action.edit_track = Some(track.id);
        ui.close();
    }

    ui.separator();

    if ui
        .button(egui::RichText::new("Remove from Library").color(theme.destructive))
        .clicked()
    {
        action.remove_track = Some(track.id);
        ui.close();
    }
}

fn format_duration(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    let mins = s / 60;
    let secs = s % 60;
    format!("{}:{:02}", mins, secs)
}
