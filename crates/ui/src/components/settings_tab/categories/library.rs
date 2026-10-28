//! The Library & App category: storage, scan roots, imports, duplicates,
//! language, smart playlists, instances, and file organisation.

use super::super::widgets::{
    badge_label, card_header, language_label, pill_btn, setting_card, setting_item,
    setting_item_toggle, toggle_switch,
};
use super::super::{SettingsCategory, SettingsExtraContext, SettingsTabActions, SettingsTabState};
use crate::theme::Theme;
use eframe::egui::{self, Color32, CornerRadius, Sense, Vec2};
use std::path::PathBuf;

pub(in crate::components::settings_tab) fn render_library_category(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
    extra: &SettingsExtraContext<'_>,
    actions: &mut SettingsTabActions,
) {
    // Card 1: Interface & Language
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_logo,
            &tr!("Interface & Language"),
            &tr!("The language the interface speaks; the system locale decides when unset"),
            None,
            theme,
        );

        setting_item(
            ui,
            &tr!("Language"),
            &tr!("Follow the system locale, or choose a language explicitly"),
            theme,
            |ui| {
                let current = extra.locale;
                egui::ComboBox::from_id_salt("settings_language")
                    .selected_text(language_label(current))
                    .show_ui(ui, |ui| {
                        for (code, name) in [
                            ("", tr!("Auto (system)").as_str()),
                            ("en", "English"),
                            ("de", "Deutsch"),
                        ] {
                            if ui.selectable_label(current == code, name).clicked() {
                                actions.locale_changed = Some(code.to_string());
                            }
                        }
                    });
            },
        );

        if setting_item_toggle(
            ui,
            &tr!("Reduce Motion"),
            &tr!("Disable hover and transition animations across the interface"),
            theme,
            &mut state.reduce_motion,
        ) {
            actions.reduce_motion_changed = Some(state.reduce_motion);
        }
    });

    // Card 2: Smart Playlists
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_list_icon,
            &tr!("Smart Playlists"),
            &tr!("When the saved queries over your library are run again"),
            None,
            theme,
        );

        setting_item(
            ui,
            &tr!("Smart Playlist Refresh"),
            &tr!("Re-evaluate the saved queries after every scan, on a timer, or only on request"),
            theme,
            |ui| {
                let policy = state.smart_refresh.policy;
                egui::ComboBox::from_id_salt("settings_smart_refresh")
                    .selected_text(policy.label())
                    .show_ui(ui, |ui| {
                        for option in domain::smart::SmartRefreshPolicy::ALL {
                            if ui
                                .selectable_label(policy == option, option.label())
                                .on_hover_text(option.description())
                                .clicked()
                            {
                                state.smart_refresh.policy = option;
                            }
                        }
                    });
            },
        );

        setting_item(
            ui,
            &tr!("Refresh Interval"),
            &tr!("How many minutes pass between two refreshes under the timer policy"),
            theme,
            |ui| {
                let mut minutes = state.smart_refresh.interval_minutes as f32;
                let enabled =
                    state.smart_refresh.policy == domain::smart::SmartRefreshPolicy::Interval;
                let response = ui.add_enabled(
                    enabled,
                    egui::Slider::new(&mut minutes, 1.0..=120.0)
                        .suffix(" min")
                        .step_by(1.0),
                );
                if enabled && response.changed() {
                    state.smart_refresh.interval_minutes = minutes.round() as u32;
                }
            },
        );
    });

    // Card 3: Theme & Visual Aesthetics
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_moon,
            &tr!("Theme & Visual Aesthetics"),
            &tr!("Obsidian surfaces, jade highlights, and typography for the player shell"),
            Some((
                if extra.is_dark_theme {
                    "OBSIDIAN"
                } else {
                    "LIGHT MODE"
                },
                theme.bg_pill,
                theme.accent,
            )),
            theme,
        );

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_md, 0.0);
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("Open Themes & Appearance →")
                            .font(crate::theme::font(12.0))
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(theme.accent)
                    .corner_radius(CornerRadius::same(theme.radius_md))
                    .min_size(Vec2::new(180.0, 30.0)),
                )
                .clicked()
            {
                state.active_category = SettingsCategory::Appearance;
            }

            let lbl = if extra.is_dark_theme {
                "Switch to Light Mode"
            } else {
                "Switch to Dark Mode"
            };
            if ui
                .button(
                    egui::RichText::new(lbl)
                        .font(crate::theme::font(12.0))
                        .color(theme.accent_highlight),
                )
                .clicked()
            {
                actions.toggle_theme = true;
            }
        });
    });

    // Card 4: Music Library & Monitored Directories
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_folder,
            &tr!("Music Library Storage & Directories"),
            &tr!("Manage monitored folders and scan for new audio files, albums, and embedded artwork"),
            Some((
                if extra.is_scanning {
                    "SCANNING..."
                } else {
                    "READY"
                },
                if extra.is_scanning {
                    theme.accent_highlight.linear_multiply(0.4)
                } else {
                    theme.bg_pill
                },
                if extra.is_scanning {
                    Color32::WHITE
                } else {
                    theme.text_secondary
                },
            )),
            theme,
        );

        setting_item(
            ui,
            &tr!("Cataloged Tracks"),
            &tr!("Indexed audio files in local SQLite library database"),
            theme,
            |ui| {
                badge_label(ui, format!("{} tracks indexed", extra.track_count), theme);
            },
        );

        setting_item(
            ui,
            &tr!("Import Music"),
            &tr!("Use the system file picker to add songs or whole folders to the library"),
            theme,
            |ui| {
                ui.add_enabled_ui(!extra.import_active, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(theme.space_md, 0.0);

                        if ui
                            .add(pill_btn(tr!(Msg::S3D973E682A0E8DCA), true, theme))
                            .clicked()
                        {
                            actions.import_files = true;
                        }
                        if ui
                            .add(pill_btn(tr!(Msg::SA26A7865D8B6F054), false, theme))
                            .clicked()
                        {
                            actions.import_folder = true;
                        }
                        if ui
                            .add(pill_btn(tr!(Msg::S927FD885EC2CFC42), false, theme))
                            .clicked()
                        {
                            actions.import_playlist = true;
                        }
                        if extra.import_active {
                            ui.label(
                                egui::RichText::new(tr!("waiting for file picker…"))
                                    .font(crate::theme::font(11.5))
                                    .color(theme.text_muted),
                            );
                        }
                    });
                });
            },
        );

        setting_item(
            ui,
            &tr!("Find Duplicate Tracks"),
            &tr!("Group tracks with identical audio content (matched by fingerprint, not tags) and remove the extra copies"),
            theme,
            |ui| {
                ui.horizontal(|ui| {
                    if ui.add(pill_btn(tr!(Msg::SCD03664EB9D51130), false, theme)).clicked() {
                        actions.find_duplicates = true;
                    }
                    if ui.add(pill_btn("Fetch Missing Artwork…", false, theme)).clicked() {
                        actions.open_artwork_fetch = true;
                    }
                });
            },
        );

        setting_item(
            ui,
            &tr!("Biography Source Folder"),
            &tr!("Folder of artist and album notes. Reads <Artist>.txt/.md and <Artist> - <Album>.txt/.md. The first read is cached; edits in the pane are stored separately."),
            theme,
            |ui| {
                let edit = egui::TextEdit::singleline(&mut state.biography_source_dir)
                    .hint_text(tr!("/home/user/Music/biographies"))
                    .desired_width(240.0)
                    .font(crate::theme::font(12.0));
                ui.add(edit);
            },
        );

        setting_item(
            ui,
            &tr!("Or Type a Directory"),
            &tr!("Advanced: index a folder by typing its absolute path"),
            theme,
            |ui| {
                let edit = egui::TextEdit::singleline(&mut state.new_scan_path)
                    .hint_text(tr!("/home/user/Music..."))
                    .desired_width(200.0)
                    .font(crate::theme::font(12.0));
                ui.add(edit);
                if ui
                    .add(pill_btn(tr!(Msg::S58F5A4DBDD4BD584), false, theme))
                    .clicked()
                    && !state.new_scan_path.is_empty()
                {
                    let path = PathBuf::from(state.new_scan_path.trim());
                    if path.exists() {
                        actions.trigger_scan = Some(path);
                        state.new_scan_path.clear();
                    }
                }
            },
        );

        if !extra.imported_dirs.is_empty() {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(tr!("Imported File Locations:"))
                    .font(crate::theme::font(12.0))
                    .strong()
                    .color(theme.text_secondary),
            );
            for dir in extra.imported_dirs {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_md, 0.0);
                    let (folder_rect, _) =
                        ui.allocate_exact_size(Vec2::new(16.0, 16.0), Sense::hover());
                    crate::components::icons::draw_folder(ui, folder_rect, theme.accent_warm);
                    ui.label(
                        egui::RichText::new(dir.to_string_lossy())
                            .font(crate::theme::font(12.0))
                            .color(theme.text_primary),
                    );
                });
            }
        }

        if !extra.scan_roots.is_empty() {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(tr!("Monitored Music Folders:"))
                    .font(crate::theme::font(12.0))
                    .strong()
                    .color(theme.text_secondary),
            );

            for root in extra.scan_roots {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_md, 0.0);
                    let (folder_rect, _) =
                        ui.allocate_exact_size(Vec2::new(16.0, 16.0), Sense::hover());
                    crate::components::icons::draw_folder(ui, folder_rect, theme.accent);
                    ui.label(
                        egui::RichText::new(root.path.to_string_lossy())
                            .font(crate::theme::font(12.0))
                            .color(theme.text_primary),
                    );

                    let path_clone = root.path.clone();
                    let id_clone = root.id;
                    if ui
                        .add(pill_btn(tr!(Msg::S0095E23ABF1B93A9), false, theme))
                        .clicked()
                    {
                        actions.trigger_scan = Some(path_clone);
                    }
                    if ui
                        .add(pill_btn(tr!(Msg::SEA4F08110BB8F15D), false, theme))
                        .clicked()
                    {
                        actions.remove_scan_root = Some(id_clone);
                    }
                });
            }
        }
    });

    // Card 5: Organize Files
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_list_icon,
            &tr!("Organize Files"),
            &tr!("Rename and re-sort files on disk to a naming scheme, with a full preview before anything moves"),
            None,
            theme,
        );

        setting_item(
            ui,
            &tr!("Batch Rename & Sort"),
            &tr!("Build names from tags — e.g. {artist}/{album}/{track} - {title}. Existing files are never overwritten."),
            theme,
            |ui| {
                if ui.add(pill_btn(tr!(Msg::S8E7A2EB1BB92D5DF), false, theme)).clicked() {
                    state.request_organize = true;
                }
            },
        );
    });

    // Card 6: Application Instances
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_puzzle,
            &tr!("Application Instances"),
            &tr!("How a second launch behaves while this window is open"),
            None,
            theme,
        );

        setting_item(
            ui,
            &tr!("Allow multiple instances"),
            &tr!("Off (default): a second launch brings this window forward and exits. On: it opens another window; the SQLite library is WAL-mode, so concurrent windows are safe. Takes effect next launch."),
            theme,
            |ui| {
                toggle_switch(ui, &mut state.allow_multiple_instances, theme);
            },
        );
    });

    // Card 7: About ShadowPlayer
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_logo,
            &tr!("About ShadowPlayer"),
            &tr!("Audiophile Local Desktop Music Player"),
            Some((
                concat!("v", env!("CARGO_PKG_VERSION")),
                theme.bg_pill,
                theme.accent,
            )),
            theme,
        );

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_3xl, 0.0);
            let (logo_rect, _) = ui.allocate_exact_size(Vec2::new(48.0, 48.0), Sense::hover());
            ui.painter_at(logo_rect).rect_filled(
                logo_rect,
                CornerRadius::same(theme.radius_lg),
                theme.bg_pill,
            );
            crate::components::icons::draw_logo(
                ui,
                logo_rect.shrink2(Vec2::splat(6.0)),
                theme.accent_warm,
            );

            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xs);
                ui.label(
                    egui::RichText::new(tr!("ShadowPlayer"))
                        .font(crate::theme::font(15.0))
                        .strong()
                        .color(theme.text_primary),
                );
                ui.label(
                    egui::RichText::new(tr!(
                        "Music for a better you - Audiophile Local Desktop Music Player"
                    ))
                    .font(crate::theme::font(12.0))
                    .color(theme.text_secondary),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "Engine: {} v{} | Framework: egui v0.37.0 / eframe | SQLite FTS5",
                        engine::NAME,
                        engine::VERSION
                    ))
                    .font(crate::theme::font(11.0))
                    .color(theme.text_muted),
                );
            });
        });
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Category 7: Themes & Appearance
// ─────────────────────────────────────────────────────────────────────────────
