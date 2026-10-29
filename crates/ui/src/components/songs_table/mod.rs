//! The virtualized songs table.
//!
//! The table is split into focused modules:
//!
//! - [`prefs`] — persisted preferences and the quality/format chips.
//! - [`columns`] — column definitions, saved presets, geometry, and the
//!   interactive header row.
//! - [`toolbar`] — the title bar, view/sort controls, and the bulk-action bar.
//! - [`row`] — per-row and per-card rendering plus the virtualized loops.
//! - [`context_menu`] — the track context menu.
//!
//! `mod.rs` owns the public table shell ([`SongsTable`]) and its action sink
//! ([`SongsTableAction`]); the submodules are implementation detail re-exported
//! through this façade.

use crate::services::ArtworkService;
use crate::theme::Theme;
use domain::ids::{PlaylistId, TrackId};
use domain::models::{Playlist, SortField, SortOrder, Track};
use eframe::egui::{self, Color32, CornerRadius, Sense, Ui, Vec2};
use std::collections::HashSet;
use std::path::PathBuf;

mod columns;
mod context_menu;
mod prefs;
mod row;
mod toolbar;

pub use columns::{ColumnPreset, SongColumn};
pub use prefs::{ColumnWidths, QuickFilter, RowDensity, SongTablePrefs};
pub(crate) use row::paint_rating_stars;

/// Maximum number of new cover textures decoded per frame. Caps the work done in
/// a single frame so fast scrolling on a slow CPU can't cause a stutter.
const ART_LOADS_PER_FRAME: usize = 8;

/// The call to action offered by the empty state.
enum EmptyStateAction {
    OpenFolders,
    ClearSearch,
    AnalyzeRecommendations,
}

/// Explain an empty list and offer the next step.
///
/// The table used to render its header and then nothing at all, so an empty
/// library, a search that matched nothing and an empty playlist were all
/// indistinguishable from a broken app.
#[allow(clippy::too_many_arguments)]
fn render_empty_state(
    ui: &mut egui::Ui,
    theme: &Theme,
    search_query: &str,
    library_track_count: usize,
    quick_filter: QuickFilter,
    recommendations_enabled: bool,
    recommendation_loading: bool,
    action: &mut SongsTableAction,
) {
    let (headline, body, cta): (String, String, Option<(String, EmptyStateAction)>) =
        if library_track_count == 0 {
            (
                tr!("Your library is empty"),
                tr!(
                    "Add a music folder and ShadowPlayer will scan it, read the tags \
                     and build your library."
                ),
                Some((tr!("Add a music folder"), EmptyStateAction::OpenFolders)),
            )
        } else if quick_filter == QuickFilter::ForYou
            && recommendations_enabled
            && search_query.trim().is_empty()
        {
            if recommendation_loading {
                (
                    tr!("Building your For You mix"),
                    tr!(
                        "The recommendation engine is ranking your library. This stays off the \
                         audio thread, so playback keeps running."
                    ),
                    None,
                )
            } else {
                (
                    tr!("No recommendations yet"),
                    tr!(
                        "Play a few songs so the engine can learn your taste, or analyze the \
                         library to give it audio features to work with."
                    ),
                    Some((
                        tr!("Analyze library"),
                        EmptyStateAction::AnalyzeRecommendations,
                    )),
                )
            }
        } else if !search_query.trim().is_empty() {
            (
                tr!("Nothing matched your search"),
                tr_args!(
                    "No tracks, albums, artists or playlists match “{search_query}”.",
                    "search_query" => search_query
                ),
                Some((tr!("Clear search"), EmptyStateAction::ClearSearch)),
            )
        } else {
            (
                tr!("No tracks here yet"),
                tr!("This view is empty. Try another one, or add more music to your library."),
                None,
            )
        };

    // In a short window the illustration and the explanation are dropped, so
    // the message and its button still fit. The empty state used to be cut
    // off at the app's own minimum window height.
    let tight = ui.available_height() < 220.0;

    let top_space = if tight {
        2.0
    } else {
        (ui.available_height() * 0.16).clamp(6.0, 48.0)
    };
    ui.add_space(top_space);
    ui.vertical_centered(|ui| {
        if !tight {
            let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
            crate::components::icons::draw_music_note(ui, icon_rect, theme.text_dim);
            ui.add_space(10.0);
        }
        ui.label(
            egui::RichText::new(headline)
                .font(crate::theme::font(16.0))
                .strong()
                .color(theme.text_primary),
        );
        if !tight {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(body)
                    .font(crate::theme::font(12.5))
                    .color(theme.text_muted),
            );
        }

        if let Some((label, cta_action)) = cta {
            ui.add_space(if tight { 8.0 } else { 14.0 });
            let resp = ui.add(
                egui::Button::new(
                    egui::RichText::new(&label)
                        .font(crate::theme::font(13.0))
                        .color(Color32::WHITE),
                )
                .fill(theme.accent)
                .corner_radius(CornerRadius::same(theme.radius_md))
                .min_size(Vec2::new(158.0, 32.0)),
            );
            resp.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label.as_str())
            });
            if resp.clicked() {
                match cta_action {
                    EmptyStateAction::OpenFolders => action.open_folders = true,
                    EmptyStateAction::ClearSearch => action.clear_search = true,
                    EmptyStateAction::AnalyzeRecommendations => {
                        action.analyze_recommendations = true;
                    }
                }
            }
        }
    });
}

pub struct SongsTableAction {
    pub play_track: Option<TrackId>,
    pub queue_next_track: Option<TrackId>,
    pub add_to_queue: Option<TrackId>,
    pub add_to_playlist: Option<(TrackId, PlaylistId)>,
    pub toggle_favorite: Option<TrackId>,
    /// Set one track's rating to 0–5 stars.
    pub set_rating: Option<(TrackId, u8)>,
    pub sort_field_changed: Option<SortField>,
    /// User tapped the ascending/descending toggle.
    pub sort_order_changed: Option<SortOrder>,
    /// User tapped a quality/format chip.
    pub quick_filter_changed: Option<QuickFilter>,
    /// The user dragged a separator to a new set of shares.
    pub column_preset_changed: Option<ColumnPreset>,
    /// A column was added, removed or reordered from the menu. Unlike a drag,
    /// this is a completed edit and should be persisted immediately.
    pub column_preset_edited: bool,
    /// The user chose another saved preset.
    pub select_column_preset: Option<usize>,
    /// Store this layout under the name in `preset_name_draft`.
    pub save_column_preset: Option<ColumnPreset>,
    /// Remove the active preset.
    pub delete_column_preset: bool,
    /// The rating column was dragged to a new pixel width.
    pub rating_width_changed: Option<f32>,
    /// The drag ended; this is the moment to persist the widths.
    pub column_drag_finished: bool,
    /// The density button was tapped.
    pub toggle_density: bool,
    /// "Reset columns" was chosen from the context menu.
    pub reset_columns: bool,
    pub toggle_view_mode: bool,
    pub open_folder: Option<PathBuf>,
    pub show_properties: Option<TrackId>,
    pub remove_track: Option<TrackId>,
    /// User chose "Edit Song Info" from the context menu.
    pub edit_track: Option<TrackId>,
    /// User chose "Per-track DSP" from the context menu.
    pub open_dsp: Option<TrackId>,
    /// User chose "Auto-Tag with MusicBrainz" from the context menu.
    pub autotag_track: Option<TrackId>,
    /// Seed a local radio station from this track.
    pub start_radio: Option<TrackId>,

    // ── Bulk (multi-select) actions ─────────────────────────────────────────
    // The selected ids themselves live in `LibraryState`; these flags only say
    // *what* to do with them.
    pub play_selected: bool,
    pub queue_next_selected: bool,
    pub add_selected_to_queue: bool,
    pub add_selected_to_playlist: Option<PlaylistId>,
    /// Add every selected track to Favorites.
    pub favorite_selected: bool,
    /// Set every selected track to this rating (0 clears).
    pub rate_selected: Option<u8>,
    /// Hand the selected tracks to the batch organizer.
    pub organize_selected: bool,
    /// Bulk auto-tag selected tracks.
    pub autotag_selected: bool,
    /// Bulk artwork fetch for selected tracks.
    pub fetch_artwork_selected: bool,
    pub remove_selected: bool,
    pub clear_selection: bool,
    /// The empty state asked the user to add a music folder.
    pub open_folders: bool,
    /// The empty state asked the user to clear the search query.
    pub clear_search: bool,
    /// The For You empty state asked to analyze the library.
    pub analyze_recommendations: bool,
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
    /// The active column layout.
    pub column_preset: &'a ColumnPreset,
    /// Names of every saved preset, in menu order.
    pub column_preset_names: &'a [String],
    /// Index of `column_preset` in the saved list.
    pub active_preset: usize,
    /// Work-in-progress name for "Save as preset".
    pub preset_name_draft: &'a mut String,
    /// Persisted width of the fixed rating column.
    pub rating_width: f32,
    /// Whether rows are comfortable or compact.
    pub density: RowDensity,
    /// Multi-selection, shared with the app so it survives tab changes.
    pub selected_track_ids: &'a mut HashSet<TrackId>,
    /// Anchor for shift-range selection.
    pub last_clicked_index: &'a mut Option<usize>,
    /// Artwork service used to lazily fetch textures for visible rows only.
    pub artwork: &'a mut ArtworkService,
    pub theme: &'a Theme,
    /// Hide the large "Songs" title and toolbar in short windows, leaving the
    /// table header and rows. Sorting stays available from the header cells.
    pub show_header: bool,
    /// The current search query, so the empty state can say what it found
    /// nothing for.
    pub search_query: &'a str,
    /// How many tracks the library holds in total. Zero means the library
    /// itself is empty, which is a different situation from a filtered view
    /// that happens to be empty.
    pub library_track_count: usize,
    /// Whether the recommendation feature is enabled in settings.
    pub recommendations_enabled: bool,
    /// A ranking request is in flight; the For You empty state says so.
    pub recommendation_loading: bool,
}

impl<'a> SongsTable<'a> {
    pub fn show(self, ui: &mut Ui) -> SongsTableAction {
        let mut action = SongsTableAction {
            play_track: None,
            queue_next_track: None,
            add_to_queue: None,
            add_to_playlist: None,
            toggle_favorite: None,
            set_rating: None,
            sort_field_changed: None,
            sort_order_changed: None,
            quick_filter_changed: None,
            column_preset_changed: None,
            column_preset_edited: false,
            select_column_preset: None,
            save_column_preset: None,
            delete_column_preset: false,
            rating_width_changed: None,
            column_drag_finished: false,
            toggle_density: false,
            reset_columns: false,
            toggle_view_mode: false,
            open_folder: None,
            show_properties: None,
            remove_track: None,
            edit_track: None,
            open_dsp: None,
            autotag_track: None,
            start_radio: None,
            play_selected: false,
            queue_next_selected: false,
            add_selected_to_queue: false,
            add_selected_to_playlist: None,
            favorite_selected: false,
            rate_selected: None,
            organize_selected: false,
            autotag_selected: false,
            fetch_artwork_selected: false,
            remove_selected: false,
            clear_selection: false,
            open_folders: false,
            clear_search: false,
            analyze_recommendations: false,
        };

        // Split the borrows so the row loops can mutably borrow
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
            column_preset,
            column_preset_names,
            active_preset,
            preset_name_draft,
            rating_width,
            density,
            selected_track_ids,
            last_clicked_index,
            artwork,
            theme,
            show_header,
            search_query,
            library_track_count,
            recommendations_enabled,
            recommendation_loading,
        } = self;

        let ctx = ui.ctx().clone();
        let mut art_budget = ART_LOADS_PER_FRAME;

        if show_header {
            toolbar::render_toolbar(
                ui,
                &mut toolbar::TableToolbar {
                    track_count: tracks.len(),
                    quick_filter,
                    recommendations_enabled,
                    is_grid_view,
                    column_preset,
                    column_preset_names,
                    active_preset,
                    preset_name_draft,
                    density,
                    current_sort,
                    sort_order,
                    theme,
                    action: &mut action,
                },
            );
        }

        // Bulk action bar — only shown while a selection exists.
        if !selected_track_ids.is_empty() {
            toolbar::render_bulk_bar(ui, selected_track_ids.len(), playlists, theme, &mut action);
            ui.add_space(6.0);
        }

        // An empty list gets an explanation and a way out, not a blank area.
        if tracks.is_empty() {
            render_empty_state(
                ui,
                theme,
                search_query,
                library_track_count,
                quick_filter,
                recommendations_enabled,
                recommendation_loading,
                &mut action,
            );
            return action;
        }

        if is_grid_view {
            let mut rows = row::RowEnv {
                tracks,
                active_track_id,
                is_playing,
                favorite_track_ids,
                playlists,
                selected_track_ids,
                last_clicked_index,
                artwork,
                theme,
                ctx: &ctx,
                density,
                art_budget: &mut art_budget,
            };
            row::render_grid(ui, &mut rows, &mut action);
        } else {
            // List View (virtualized, renders only visible rows).
            let row_h = density.row_height();
            let full_width = ui.available_width();
            let cols = columns::TableColumns::new(full_width, column_preset, density, rating_width);

            // Static column headers, painted on the same column model the rows
            // use, so the two can never drift apart.
            columns::render_header(
                ui,
                &cols,
                &mut columns::HeaderContext {
                    tracks,
                    sort: (current_sort, sort_order),
                    column_preset,
                    theme,
                    selected_track_ids,
                    last_clicked_index,
                },
                &mut action,
            );

            ui.add_space(4.0);
            let mut rows = row::RowEnv {
                tracks,
                active_track_id,
                is_playing,
                favorite_track_ids,
                playlists,
                selected_track_ids,
                last_clicked_index,
                artwork,
                theme,
                ctx: &ctx,
                density,
                art_budget: &mut art_budget,
            };
            row::render_list(ui, &mut rows, &cols, row_h, &mut action);
        }

        action
    }
}
