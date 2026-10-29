//! The songs-table toolbar: title, view controls, sorting, filter chips,
//! column-preset menu, and the multi-select action bar.

use super::columns::{ColumnPreset, SongColumn};
use super::prefs::{QuickFilter, RowDensity};
use super::SongsTableAction;
use crate::theme::Theme;
use domain::models::{Playlist, SortField, SortOrder};
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Ui, Vec2};

/// Everything the toolbar needs, borrowed from the table for one frame.
pub(super) struct TableToolbar<'a, 'b> {
    pub track_count: usize,
    pub quick_filter: QuickFilter,
    /// The feature is opt-out; hide its chip entirely when disabled.
    pub recommendations_enabled: bool,
    pub is_grid_view: bool,
    pub column_preset: &'a ColumnPreset,
    pub column_preset_names: &'a [String],
    pub active_preset: usize,
    pub preset_name_draft: &'a mut String,
    pub density: RowDensity,
    pub current_sort: SortField,
    pub sort_order: SortOrder,
    pub theme: &'a Theme,
    pub action: &'b mut SongsTableAction,
}

pub(super) fn render_toolbar(ui: &mut Ui, toolbar: &mut TableToolbar<'_, '_>) {
    let track_count = toolbar.track_count;
    let quick_filter = toolbar.quick_filter;
    let is_grid_view = toolbar.is_grid_view;
    let column_preset = toolbar.column_preset;
    let column_preset_names = toolbar.column_preset_names;
    let active_preset = toolbar.active_preset;
    let preset_name_draft = &mut *toolbar.preset_name_draft;
    let density = toolbar.density;
    let current_sort = toolbar.current_sort;
    let sort_order = toolbar.sort_order;
    let theme = toolbar.theme;
    let action = &mut *toolbar.action;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);

        ui.label(
            egui::RichText::new(tr!("Songs"))
                .font(crate::theme::font(22.0))
                .strong()
                .color(theme.text_primary),
        );

        ui.label(
            egui::RichText::new(if quick_filter.is_active_filter() {
                format!("{} songs · {} filter", track_count, quick_filter.label())
            } else {
                format!("{} songs", track_count)
            })
            .font(crate::theme::font(13.0))
            .color(theme.text_muted),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_md, 0.0);

            if !is_grid_view {
                render_column_preset_menu(
                    ui,
                    column_preset,
                    column_preset_names,
                    active_preset,
                    preset_name_draft,
                    theme,
                    action,
                );
            }

            // View Mode Toggles (Grid vs List). The active button's
            // fill fades in rather than snapping, and the animation
            // ids are fixed, so the state stays bounded.
            let grid_t = ui.ctx().animate_bool_with_time(
                ui.make_persistent_id("view_mode_grid"),
                is_grid_view,
                theme.motion_time(0.16),
            );
            let list_t = ui.ctx().animate_bool_with_time(
                ui.make_persistent_id("view_mode_list"),
                !is_grid_view,
                theme.motion_time(0.16),
            );
            let grid_bg = crate::theme::lerp_color(theme.bg_pill, theme.accent, grid_t);
            let list_bg = crate::theme::lerp_color(theme.bg_pill, theme.accent, list_t);

            let (grid_rect, grid_resp) =
                ui.allocate_exact_size(Vec2::new(30.0, 30.0), Sense::click());
            ui.painter()
                .rect_filled(grid_rect, CornerRadius::same(theme.radius_md), grid_bg);
            if !is_grid_view {
                ui.painter().rect_stroke(
                    grid_rect,
                    CornerRadius::same(theme.radius_md),
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
            grid_resp.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::Role::Button,
                    true,
                    is_grid_view,
                    "Show covers as a grid",
                )
            });
            let _ = grid_resp.clone().on_hover_text(tr!(Msg::S315382037EF4E6A9));
            if grid_resp.clicked() && !is_grid_view {
                action.toggle_view_mode = true;
            }

            let (list_rect, list_resp) =
                ui.allocate_exact_size(Vec2::new(30.0, 30.0), Sense::click());
            ui.painter()
                .rect_filled(list_rect, CornerRadius::same(theme.radius_md), list_bg);
            crate::components::icons::draw_list_icon(
                ui,
                list_rect.shrink2(Vec2::new(7.0, 7.0)),
                if !is_grid_view {
                    Color32::WHITE
                } else {
                    theme.text_muted
                },
            );
            list_resp.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::Role::Button,
                    true,
                    !is_grid_view,
                    "Show tracks as a list",
                )
            });
            let _ = list_resp.clone().on_hover_text(tr!(Msg::S432E9931F963515D));
            if list_resp.clicked() && is_grid_view {
                action.toggle_view_mode = true;
            }

            // Row density: three lines that get closer together in
            // compact mode. The icon is painted rather than typed so
            // it does not depend on a font glyph.
            let (density_rect, density_resp) =
                ui.allocate_exact_size(Vec2::new(30.0, 30.0), Sense::click());
            let density_bg = if matches!(density, RowDensity::Compact) {
                theme.accent
            } else if density_resp.hovered() {
                theme.bg_card_hover
            } else {
                theme.bg_pill
            };
            ui.painter().rect_filled(
                density_rect,
                CornerRadius::same(theme.radius_md),
                density_bg,
            );
            if !matches!(density, RowDensity::Compact) {
                ui.painter().rect_stroke(
                    density_rect,
                    CornerRadius::same(theme.radius_md),
                    Stroke::new(1.0_f32, theme.border_subtle),
                    egui::StrokeKind::Inside,
                );
            }
            let line_col = if matches!(density, RowDensity::Compact) {
                Color32::WHITE
            } else {
                theme.text_muted
            };
            let line_gap = if matches!(density, RowDensity::Compact) {
                4.0
            } else {
                6.0
            };
            for i in -1..=1 {
                let y = density_rect.center().y + i as f32 * line_gap;
                ui.painter().line_segment(
                    [
                        Pos2::new(density_rect.center().x - 6.0, y),
                        Pos2::new(density_rect.center().x + 6.0, y),
                    ],
                    Stroke::new(1.5_f32, line_col),
                );
            }
            density_resp.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::Role::Button,
                    true,
                    matches!(density, RowDensity::Compact),
                    "Compact rows",
                )
            });
            if density_resp
                .on_hover_text(format!(
                    "{} — click for {}",
                    density.label(),
                    density.toggled().label().to_lowercase()
                ))
                .clicked()
            {
                action.toggle_density = true;
            }

            ui.add_space(6.0);

            // Sort Dropdown Pill
            let sort_label = format!("Sort by: {}", sort_field_label(current_sort));

            let (sort_rect, sort_resp) =
                ui.allocate_exact_size(Vec2::new(124.0, 30.0), Sense::click());
            sort_resp.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, true, sort_label.clone())
            });
            let sp = ui.painter_at(sort_rect);
            let sort_bg = if sort_resp.hovered() {
                theme.bg_card_hover
            } else {
                theme.bg_pill
            };
            sp.rect_filled(sort_rect, theme.control_radius(), sort_bg);
            sp.rect_stroke(
                sort_rect,
                theme.control_radius(),
                Stroke::new(1.0_f32, theme.border_subtle),
                egui::StrokeKind::Inside,
            );
            sp.text(
                Pos2::new(sort_rect.min.x + 14.0, sort_rect.center().y),
                egui::Align2::LEFT_CENTER,
                &sort_label,
                crate::theme::font(12.5),
                theme.text_primary,
            );
            let chev_rect = Rect::from_center_size(
                Pos2::new(sort_rect.max.x - 14.0, sort_rect.center().y),
                Vec2::new(12.0, 12.0),
            );
            crate::components::icons::draw_chevron_down(ui, chev_rect, theme.text_muted);

            // The pill carries a chevron, so it has to open a menu.
            // It used to cycle through the fields one click at a
            // time, which read as a broken dropdown.
            let sort_menu_id = ui.make_persistent_id("sort_menu");
            let mut sort_menu_open: bool =
                ui.ctx().data(|d| d.get_temp(sort_menu_id).unwrap_or(false));
            if sort_resp
                .clone()
                .on_hover_text(tr!(Msg::SA67DAD356DD3A456))
                .clicked()
            {
                sort_menu_open = !sort_menu_open;
                ui.ctx()
                    .data_mut(|d| d.insert_temp(sort_menu_id, sort_menu_open));
            }
            if sort_menu_open {
                let popup_pos = sort_rect.left_bottom() + Vec2::new(0.0, 4.0);
                let mut close = false;
                let mut popup_rect: Option<Rect> = None;
                egui::Area::new(sort_menu_id.with("popup"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(popup_pos)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.set_min_width(124.0);
                            for (field, name) in SORT_FIELDS {
                                let selected = current_sort == *field;
                                if ui
                                    .selectable_label(
                                        selected,
                                        egui::RichText::new(*name)
                                            .font(crate::theme::font(12.5))
                                            .color(if selected {
                                                theme.accent_highlight
                                            } else {
                                                theme.text_secondary
                                            }),
                                    )
                                    .clicked()
                                {
                                    action.sort_field_changed = Some(*field);
                                    close = true;
                                }
                            }
                        });
                        popup_rect = Some(ui.min_rect());
                    });

                let clicked_elsewhere = ui.input(|i| i.pointer.any_click())
                    && !sort_resp.hovered()
                    && !popup_rect
                        .map(|r| ui.rect_contains_pointer(r))
                        .unwrap_or(false);
                if close || clicked_elsewhere {
                    ui.ctx().data_mut(|d| d.insert_temp(sort_menu_id, false));
                }
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
            dp.rect_filled(dir_rect, CornerRadius::same(theme.radius_md), dir_bg);
            dp.rect_stroke(
                dir_rect,
                CornerRadius::same(theme.radius_md),
                Stroke::new(1.0_f32, theme.border_subtle),
                egui::StrokeKind::Inside,
            );
            let ascending = sort_order == SortOrder::Ascending;
            let arrow_col = if dir_resp.hovered() {
                theme.accent_highlight
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
            dir_resp.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::Role::Button,
                    true,
                    if ascending {
                        "Sort ascending — activate for descending"
                    } else {
                        "Sort descending — activate for ascending"
                    },
                )
            });
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

    // Quick-filter chips. They narrow the list without leaving the
    // view, so the user does not have to hunt for a collection view
    // to find the lossless or Hi-Res tracks.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
        for filter in QuickFilter::ALL {
            if filter == QuickFilter::ForYou && !toolbar.recommendations_enabled {
                continue;
            }
            let is_active = quick_filter == filter;
            let id_label = filter.label();
            // "For You" is the one chip whose name is part of the new feature,
            // so it goes through the catalogue; the older chips keep their
            // static labels until they are localised as a group.
            let label = if filter == QuickFilter::ForYou {
                tr!("For You")
            } else {
                id_label.to_string()
            };
            // One animation id per chip, so the state stays bounded.
            let chip_t = ui.ctx().animate_bool_with_time(
                ui.make_persistent_id(("chip", id_label)),
                is_active,
                theme.motion_time(0.16),
            );
            let resp = ui.add(
                egui::Button::new(
                    egui::RichText::new(&label)
                        .font(crate::theme::font(11.5))
                        .color(crate::theme::lerp_color(
                            theme.text_secondary,
                            Color32::WHITE,
                            chip_t,
                        )),
                )
                .fill(crate::theme::lerp_color(
                    theme.bg_pill,
                    theme.accent,
                    chip_t,
                ))
                .stroke(Stroke::new(
                    1.0,
                    if is_active {
                        theme.accent_highlight
                    } else {
                        theme.border_subtle
                    },
                ))
                .corner_radius(CornerRadius::same(theme.radius_sm))
                .min_size(Vec2::new(0.0, 24.0)),
            );
            resp.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::Role::Button,
                    ui.is_enabled(),
                    is_active,
                    format!("Filter: {label}"),
                )
            });
            if resp.on_hover_text(quick_filter_tooltip(filter)).clicked() {
                action.quick_filter_changed = Some(filter);
            }
        }
    });

    ui.add_space(6.0);
}

/// The sort fields offered by the sort menu, in the order they are listed.
const SORT_FIELDS: &[(SortField, &str)] = &[
    (SortField::TrackNumber, "Track"),
    (SortField::Title, "Title"),
    (SortField::Artist, "Artist"),
    (SortField::Album, "Album"),
    (SortField::Duration, "Duration"),
    (SortField::DateAdded, "Added"),
    (SortField::Favorite, "Favorites"),
    (SortField::Rating, "Rating"),
];

/// The short name of a sort field, as shown on the sort pill.
fn sort_field_label(field: SortField) -> &'static str {
    SORT_FIELDS
        .iter()
        .find(|(f, _)| *f == field)
        .map(|(_, label)| *label)
        .unwrap_or("Title")
}

/// A one-line explanation of what a quick-filter chip does.
fn quick_filter_tooltip(filter: QuickFilter) -> String {
    match filter {
        QuickFilter::All => "Show every track".to_string(),
        QuickFilter::ForYou => {
            tr!("A local mix that learns from what you play, skip, like and rate")
        }
        QuickFilter::Lossless => "Show only lossless tracks (FLAC, ALAC, WAV, DSD…)".to_string(),
        QuickFilter::HiRes => "Show only Hi-Res tracks (above 48 kHz or 1000 kbps)".to_string(),
        QuickFilter::Favorites => "Show only tracks you have favorited".to_string(),
        QuickFilter::Played => "Show only tracks you have played before".to_string(),
        QuickFilter::Rating => "Show only tracks with at least one star".to_string(),
    }
}

/// The "Columns" menu: load a saved layout, reorder columns, and save the
/// current arrangement as a new preset.
fn render_column_preset_menu(
    ui: &mut Ui,
    column_preset: &ColumnPreset,
    column_preset_names: &[String],
    active_preset: usize,
    preset_name_draft: &mut String,
    theme: &Theme,
    action: &mut SongsTableAction,
) {
    ui.scope(|ui| {
        ui.visuals_mut().widgets.inactive.weak_bg_fill = theme.bg_pill;
        ui.visuals_mut().widgets.inactive.bg_fill = theme.bg_pill;
        ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::new(1.0, theme.border_subtle);
        ui.visuals_mut().widgets.inactive.corner_radius = CornerRadius::same(theme.radius_md);
        ui.visuals_mut().widgets.hovered.weak_bg_fill = theme.bg_card_hover;
        ui.visuals_mut().widgets.hovered.bg_fill = theme.bg_card_hover;
        ui.visuals_mut().widgets.hovered.bg_stroke = egui::Stroke::new(1.0, theme.border_subtle);
        ui.visuals_mut().widgets.hovered.corner_radius = CornerRadius::same(theme.radius_md);
        ui.visuals_mut().widgets.open.weak_bg_fill = theme.bg_card_active;
        ui.visuals_mut().widgets.open.bg_fill = theme.bg_card_active;
        ui.visuals_mut().widgets.open.bg_stroke = egui::Stroke::new(1.0, theme.accent);
        ui.visuals_mut().widgets.open.corner_radius = CornerRadius::same(theme.radius_md);

        ui.menu_button(
            egui::RichText::new(tr!("Columns"))
                .font(crate::theme::font(11.5))
                .color(theme.text_secondary),
            |ui| {
                ui.set_min_width(240.0);
                for (idx, name) in column_preset_names.iter().enumerate() {
                    if ui.selectable_label(idx == active_preset, name).clicked() {
                        action.select_column_preset = Some(idx);
                        ui.close();
                    }
                }

                ui.separator();
                ui.label(
                    egui::RichText::new(tr!("Column order"))
                        .font(crate::theme::font(11.0))
                        .strong()
                        .color(theme.text_muted),
                );

                let mut edited = column_preset.clone();
                let mut changed = false;
                let count = edited.columns.len();
                for (idx, column) in edited.columns.clone().iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(column.label())
                                .font(crate::theme::font(11.5))
                                .color(theme.text_primary),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if idx > 0 && ui.small_button("×").clicked() {
                                edited.columns.remove(idx);
                                edited.weights.remove(idx);
                                changed = true;
                            }
                            if idx + 1 < count && ui.small_button("↓").clicked() {
                                edited.move_column(idx, idx + 1);
                                changed = true;
                            }
                            if idx > 0 && ui.small_button("↑").clicked() {
                                edited.move_column(idx, idx - 1);
                                changed = true;
                            }
                        });
                    });
                }

                let missing: Vec<SongColumn> = SongColumn::OPTIONAL
                    .iter()
                    .copied()
                    .filter(|column| !edited.columns.contains(column))
                    .collect();
                if !missing.is_empty() {
                    ui.menu_button(tr!("Add column"), |ui| {
                        for column in missing {
                            if ui.button(column.label()).clicked() {
                                edited.columns.push(column);
                                edited.weights.push(column.default_weight());
                                changed = true;
                                ui.close();
                            }
                        }
                    });
                }

                if changed {
                    action.column_preset_changed = Some(edited.clone().normalized());
                    action.column_preset_edited = true;
                }

                ui.separator();
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(preset_name_draft)
                            .hint_text(tr!("Preset name"))
                            .desired_width(130.0),
                    );
                    if ui.button(tr!("Save as preset")).clicked() {
                        action.save_column_preset = Some(edited.clone().normalized());
                        ui.close();
                    }
                });
                if ui.button(tr!("Reset this preset")).clicked() {
                    let reset = ColumnPreset::factory(&column_preset.name).unwrap_or_else(|| {
                        ColumnPreset {
                            name: column_preset.name.clone(),
                            ..ColumnPreset::default()
                        }
                    });
                    action.column_preset_changed = Some(reset);
                    ui.close();
                }
                if column_preset_names.len() > 1 && ui.button(tr!("Delete preset")).clicked() {
                    action.delete_column_preset = true;
                    ui.close();
                }
            },
        );
    });
}

/// The context-sensitive action strip shown while tracks are multi-selected.
pub(super) fn render_bulk_bar(
    ui: &mut Ui,
    count: usize,
    playlists: &[Playlist],
    theme: &Theme,
    action: &mut SongsTableAction,
) {
    egui::Frame::NONE
        .fill(theme.bg_card)
        .corner_radius(CornerRadius::same(theme.radius_md))
        .stroke(Stroke::new(1.0_f32, theme.info))
        .inner_margin(egui::Margin {
            left: 10,
            right: 10,
            top: 6,
            bottom: 6,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_md, 0.0);
                ui.label(
                    egui::RichText::new(format!("{count} selected"))
                        .font(crate::theme::font(12.5))
                        .strong()
                        .color(theme.info),
                );
                ui.add_space(4.0);

                if ui
                    .button(egui::RichText::new(tr!("Play")).font(crate::theme::font(12.0)))
                    .clicked()
                {
                    action.play_selected = true;
                }
                if ui
                    .button(egui::RichText::new(tr!("Play Next")).font(crate::theme::font(12.0)))
                    .clicked()
                {
                    action.queue_next_selected = true;
                }
                if ui
                    .button(egui::RichText::new(tr!("Add to Queue")).font(crate::theme::font(12.0)))
                    .clicked()
                {
                    action.add_selected_to_queue = true;
                }
                if ui
                    .button(egui::RichText::new(tr!("Favorite")).font(crate::theme::font(12.0)))
                    .clicked()
                {
                    action.favorite_selected = true;
                }
                ui.menu_button(
                    egui::RichText::new(tr!("Rate")).font(crate::theme::font(12.0)),
                    |ui| {
                        for stars in 1..=5u8 {
                            let label = match stars {
                                1 => "1 star".to_string(),
                                _ => format!("{stars} stars"),
                            };
                            if ui.button(label).clicked() {
                                action.rate_selected = Some(stars);
                                ui.close();
                            }
                        }
                        if ui.button(tr!(Msg::SDA396F7DB3F6E8D7)).clicked() {
                            action.rate_selected = Some(0);
                            ui.close();
                        }
                    },
                );
                if ui
                    .button(egui::RichText::new(tr!("Organize")).font(crate::theme::font(12.0)))
                    .clicked()
                {
                    action.organize_selected = true;
                }
                if ui
                    .button(egui::RichText::new("Auto-Tag").font(crate::theme::font(12.0)))
                    .clicked()
                {
                    action.autotag_selected = true;
                }
                if ui
                    .button(egui::RichText::new("Fetch Artwork").font(crate::theme::font(12.0)))
                    .clicked()
                {
                    action.fetch_artwork_selected = true;
                }
                if !playlists.is_empty() {
                    ui.menu_button(
                        egui::RichText::new(tr!("Add to Playlist")).font(crate::theme::font(12.0)),
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
                        egui::RichText::new(tr!("Remove"))
                            .font(crate::theme::font(12.0))
                            .color(theme.destructive),
                    )
                    .clicked()
                {
                    action.remove_selected = true;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(egui::RichText::new(tr!("Clear")).font(crate::theme::font(12.0)))
                        .clicked()
                    {
                        action.clear_selection = true;
                    }
                });
            });
        });
}
