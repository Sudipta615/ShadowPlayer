//! Duplicate-track finder.
//!
//! Duplicates are identified by the content fingerprint added in v0.4.0 — files
//! whose bytes (size plus head/tail fingerprint) are identical, regardless of
//! their tags or file name. Nothing is guessed from metadata, so the list is
//! trustworthy. Removing a track only unlinks it from the library; deleting the
//! file from disk is a separate, off-by-default opt-in.

use domain::ids::TrackId;
use domain::models::Track;
use eframe::egui::{self, Color32, Vec2};

use crate::theme::Theme;

/// One set of byte-identical files.
#[derive(Debug, Clone, Default)]
pub struct DuplicateGroup {
    /// The copy kept when the group is pruned (the lowest id, i.e. the one the
    /// library has known longest) — shown as "keep" to the user.
    pub keep: Option<TrackId>,
    pub tracks: Vec<Track>,
}

impl DuplicateGroup {
    /// The copies marked for removal (every track but `keep`).
    pub fn removable(&self) -> impl Iterator<Item = &Track> {
        self.tracks.iter().filter(|t| Some(t.id) != self.keep)
    }
}

/// Persistent (per-session) UI state for the finder.
#[derive(Debug, Default)]
pub struct DuplicatesState {
    pub is_open: bool,
    pub groups: Vec<DuplicateGroup>,
    /// `true` once a scan has run, so an empty list means "no duplicates"
    /// rather than "not scanned yet".
    pub scanned: bool,
    /// Tracks the user actually wants removed (defaults to every extra copy).
    pub selected: std::collections::HashSet<TrackId>,
    /// Also delete the underlying files. Destructive, so off by default.
    pub delete_from_disk: bool,
}

impl DuplicatesState {
    /// Load fresh scan results, defaulting the selection to every extra copy.
    pub fn set_groups(&mut self, groups: Vec<DuplicateGroup>) {
        self.selected.clear();
        for g in &groups {
            for t in g.removable() {
                self.selected.insert(t.id);
            }
        }
        self.groups = groups;
        self.scanned = true;
    }

    pub fn selected_count(&self) -> usize {
        self.selected.len()
    }
}

#[derive(Default)]
pub struct DuplicatesActions {
    /// Re-run the duplicate scan (the library may have changed).
    pub rescan: bool,
    /// Remove the selected tracks (from the library; see `delete_from_disk`).
    pub remove_selected: bool,
}

pub fn show_duplicates_modal(
    ctx: &egui::Context,
    state: &mut DuplicatesState,
    theme: &Theme,
) -> DuplicatesActions {
    let mut actions = DuplicatesActions::default();
    if !state.is_open {
        return actions;
    }

    let mut is_open = state.is_open;

    let closed =
        crate::dialog::show_modal(ctx, "duplicates", "Duplicate Tracks", 620.0, theme, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_md);

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(
                        "Tracks with identical audio content, found by content fingerprint \
                         (tags and file names are ignored).",
                    )
                    .font(crate::theme::font(12.0))
                    .color(theme.text_muted),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Rescan").clicked() {
                        actions.rescan = true;
                    }
                });
            });

            ui.separator();

            if !state.scanned {
                ui.label(
                    egui::RichText::new("Press Rescan to look for duplicates.")
                        .font(crate::theme::font(12.5))
                        .color(theme.text_muted),
                );
            } else if state.groups.is_empty() {
                ui.label(
                    egui::RichText::new("No duplicates found. Your library is clean.")
                        .font(crate::theme::font(12.5))
                        .color(theme.text_muted),
                );
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("duplicates_scroll")
                    .auto_shrink([false, false])
                    .max_height(300.0)
                    .show(ui, |ui| {
                        for (gi, group) in state.groups.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Group {} · {} copies",
                                        gi + 1,
                                        group.tracks.len()
                                    ))
                                    .font(crate::theme::font(12.5))
                                    .strong()
                                    .color(theme.text_primary),
                                );
                            });
                            ui.indent(("dupe_group", gi), |ui| {
                                for t in &group.tracks {
                                    let is_keeper = Some(t.id) == group.keep;
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing =
                                            Vec2::new(theme.space_sm, 0.0);
                                        let mut checked = state.selected.contains(&t.id);
                                        let label = format!(
                                            "{} — {}  ({:.0}s)",
                                            t.title, t.artist, t.duration_secs
                                        );
                                        if is_keeper {
                                            // The keeper is not removable.
                                            ui.add_enabled(
                                                false,
                                                egui::Checkbox::new(&mut checked, label),
                                            );
                                            ui.label(
                                                egui::RichText::new("keep")
                                                    .font(crate::theme::font(11.0))
                                                    .color(theme.bit_perfect_green),
                                            );
                                        } else {
                                            if ui.checkbox(&mut checked, label).changed() {
                                                if checked {
                                                    state.selected.insert(t.id);
                                                } else {
                                                    state.selected.remove(&t.id);
                                                }
                                            }
                                        }
                                    });
                                    ui.label(
                                        egui::RichText::new(t.path.display().to_string())
                                            .font(crate::theme::font(10.5))
                                            .color(theme.text_muted),
                                    );
                                }
                            });
                            ui.add_space(6.0);
                        }
                    });
            }

            ui.separator();

            ui.horizontal(|ui| {
                let n = state.selected_count();
                let can_remove = n > 0;
                let label = if state.delete_from_disk {
                    format!("Delete {n} file(s) from disk")
                } else {
                    format!("Remove {n} from library")
                };
                let fill = if state.delete_from_disk {
                    theme.accent_highlight
                } else {
                    theme.accent
                };
                let btn =
                    egui::Button::new(egui::RichText::new(label).color(Color32::WHITE)).fill(fill);
                if ui.add_enabled(can_remove, btn).clicked() {
                    actions.remove_selected = true;
                }

                ui.checkbox(
                    &mut state.delete_from_disk,
                    "Also delete the files from disk",
                );

                if state.delete_from_disk {
                    ui.label(
                        egui::RichText::new("This cannot be undone.")
                            .font(crate::theme::font(11.0))
                            .color(theme.accent_highlight),
                    );
                }
            });
        });
    if closed {
        is_open = false;
    }

    state.is_open = is_open;
    actions
}
