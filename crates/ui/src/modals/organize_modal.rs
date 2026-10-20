//! Batch organize / rename.
//!
//! Files are moved according to a naming pattern, but nothing touches the disk
//! until the user has seen the plan and pressed the apply button. The plan
//! itself comes from `library::organize`, which also refuses to overwrite an
//! existing file or let two tracks claim the same destination — so the dangerous
//! cases are visible before they can happen rather than reported after.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use domain::models::Track;
use eframe::egui::{self, Color32, Vec2};
use library::organize::{
    build_plan, OrganizeEntry, OrganizePlan, DEFAULT_IN_PLACE_PATTERN, DEFAULT_TREE_PATTERN, TOKENS,
};
use serde::{Deserialize, Serialize};

use crate::theme::Theme;

/// Rows rendered in the preview list before it is truncated.
const PREVIEW_ROWS: usize = 300;
/// Minimum gap between plan rebuilds while the user is typing.
const REBUILD_THROTTLE: Duration = Duration::from_millis(300);

/// The organizer's last settings, remembered across runs so a user who sorts
/// their library regularly does not retype the same pattern every time.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OrganizePrefs {
    pub pattern: String,
    pub destination: Option<PathBuf>,
}

#[derive(Debug, Default)]
pub struct OrganizeState {
    pub is_open: bool,
    /// The tracks in scope, captured when the modal opened. Recomputing the
    /// scope mid-flight would change what "apply" means halfway through.
    pub tracks: Vec<Track>,
    /// Where the scope came from, for the header ("Whole library" / "12 selected").
    pub scope_label: String,
    pub pattern: String,
    /// `None` renames in place; `Some(root)` moves into a tree under `root`.
    pub destination: Option<PathBuf>,
    pub plan: OrganizePlan,
    /// The pattern or destination changed and the plan needs rebuilding.
    pub plan_dirty: bool,
    last_built: Option<Instant>,
    /// Outcome of the last apply, shown until the next change.
    pub result: Option<String>,
    /// True once an apply has run in this session, so the app knows to refresh
    /// the library and rebuild its watchers.
    pub applied: bool,
}

impl OrganizeState {
    /// Open the organizer for a set of tracks.
    pub fn open(&mut self, tracks: Vec<Track>, scope_label: String) {
        self.is_open = true;
        self.tracks = tracks;
        self.scope_label = scope_label;
        self.result = None;
        self.applied = false;
        if self.pattern.trim().is_empty() {
            self.pattern = self.default_pattern();
        }
        self.plan_dirty = true;
        self.last_built = None;
    }

    /// The pattern that matches the current destination mode.
    fn default_pattern(&self) -> String {
        if self.destination.is_some() {
            DEFAULT_TREE_PATTERN.to_string()
        } else {
            DEFAULT_IN_PLACE_PATTERN.to_string()
        }
    }

    pub fn reset_pattern(&mut self) {
        self.pattern = self.default_pattern();
        self.plan_dirty = true;
    }

    /// Mark the plan stale and bypass the rebuild throttle.
    pub fn force_rebuild(&mut self) {
        self.plan_dirty = true;
        self.last_built = None;
    }

    pub fn set_destination(&mut self, destination: Option<PathBuf>) {
        let had_folders = self.pattern.contains('/');
        self.destination = destination;
        // Keep the pattern sensible: a name-only pattern cannot fill a tree, and
        // a tree pattern cannot rename in place.
        if self.destination.is_some() && !had_folders {
            self.pattern = DEFAULT_TREE_PATTERN.to_string();
        } else if self.destination.is_none() && had_folders {
            self.pattern = DEFAULT_IN_PLACE_PATTERN.to_string();
        }
        self.plan_dirty = true;
    }

    /// Rebuild the plan, at most once every [`REBUILD_THROTTLE`].
    ///
    /// The plan stats every destination on disk, so a large selection must not
    /// re-run on every keystroke. Returns the time until the next attempt when
    /// a rebuild is still pending, so the caller can keep repainting.
    pub fn rebuild_if_due(&mut self) -> Option<Duration> {
        if !self.plan_dirty {
            return None;
        }
        if let Some(last) = self.last_built {
            let elapsed = last.elapsed();
            if elapsed < REBUILD_THROTTLE {
                return Some(REBUILD_THROTTLE - elapsed);
            }
        }
        self.plan = build_plan(&self.tracks, &self.pattern, self.destination.as_deref());
        self.plan_dirty = false;
        self.last_built = Some(Instant::now());
        None
    }
}

#[derive(Default)]
pub struct OrganizeActions {
    /// Open the native folder picker for the destination.
    pub pick_destination: bool,
    /// Go back to renaming in place.
    pub clear_destination: bool,
    pub reset_pattern: bool,
    /// Apply the plan shown on screen.
    pub apply: bool,
}

pub fn show_organize_modal(
    ctx: &egui::Context,
    state: &mut OrganizeState,
    theme: &Theme,
) -> OrganizeActions {
    let mut actions = OrganizeActions::default();
    if !state.is_open {
        return actions;
    }

    // Keep the preview live while typing, without rebuilding on every frame.
    if let Some(remaining) = state.rebuild_if_due() {
        ctx.request_repaint_after(remaining);
    }

    let mut is_open = state.is_open;
    // The window's own title-bar close button writes through `is_open`, so the
    // in-body Close button needs its own flag to avoid borrowing twice.
    let mut close_requested = false;

    let closed = crate::dialog::show_modal(ctx, "organize", "Organize Files", 860.0, theme, |ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, 8.0);

        ui.label(
                egui::RichText::new(format!(
                    "{} · nothing is moved until you press Apply, and existing files are never overwritten.",
                    state.scope_label
                ))
                .font(crate::theme::font(12.0))
                .color(theme.text_muted),
            );

        ui.separator();

        // ── Pattern ────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Naming pattern")
                    .font(crate::theme::font(12.5))
                    .strong()
                    .color(theme.text_primary),
            );
            let response = ui.add(
                egui::TextEdit::singleline(&mut state.pattern)
                    .desired_width(380.0)
                    .font(crate::theme::font(12.5)),
            );
            if response.changed() {
                state.plan_dirty = true;
            }
            if ui.button("Reset").clicked() {
                actions.reset_pattern = true;
            }
        });

        // Token palette: click to append, so the syntax is discoverable.
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
            ui.label(
                egui::RichText::new("Tokens:")
                    .font(crate::theme::font(11.0))
                    .color(theme.text_muted),
            );
            for (token, description) in TOKENS {
                if ui
                    .small_button(*token)
                    .on_hover_text(*description)
                    .clicked()
                {
                    state.pattern.push_str(token);
                    state.plan_dirty = true;
                }
            }
        });

        // ── Destination ────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Destination")
                    .font(crate::theme::font(12.5))
                    .strong()
                    .color(theme.text_primary),
            );
            let in_place = state.destination.is_none();
            if ui
                .selectable_label(in_place, "Keep in current folder")
                .clicked()
            {
                actions.clear_destination = true;
            }
            if ui.selectable_label(!in_place, "Move into folder").clicked() && in_place {
                actions.pick_destination = true;
            }
            if let Some(ref dest) = state.destination {
                ui.label(
                    egui::RichText::new(dest.display().to_string())
                        .font(crate::theme::font(11.5))
                        .color(theme.text_muted),
                );
                if ui.button("Change...").clicked() {
                    actions.pick_destination = true;
                }
            }
        });

        ui.separator();

        // ── Preview ────────────────────────────────────────────────
        ui.horizontal(|ui| {
            let pending = state.plan.pending_count();
            let conflicts = state.plan.conflict_count();
            let unchanged = state.plan.unchanged_count();
            ui.label(
                egui::RichText::new(format!(
                    "{pending} to move · {conflicts} conflict(s) · {unchanged} already in place"
                ))
                .font(crate::theme::font(12.5))
                .strong()
                .color(if conflicts > 0 {
                    theme.accent_highlight
                } else {
                    theme.text_primary
                }),
            );
            if pending > 0 {
                ui.label(
                    egui::RichText::new(format!(
                        "· {} folder(s) will be used",
                        state.plan.destination_folders()
                    ))
                    .font(crate::theme::font(11.5))
                    .color(theme.text_muted),
                );
            }
        });

        if let Some(ref result) = state.result {
            ui.label(
                egui::RichText::new(result)
                    .font(crate::theme::font(11.5))
                    .color(theme.bit_perfect_green),
            );
        }

        egui::ScrollArea::vertical()
            .id_salt("organize_preview")
            .auto_shrink([false, false])
            .max_height(340.0)
            .show(ui, |ui| {
                if state.tracks.is_empty() {
                    ui.label(
                        egui::RichText::new("Nothing in scope.")
                            .font(crate::theme::font(12.0))
                            .color(theme.text_muted),
                    );
                    return;
                }
                for (i, entry) in state.plan.entries.iter().take(PREVIEW_ROWS).enumerate() {
                    render_preview_row(ui, i, entry, theme);
                }
                if state.plan.entries.len() > PREVIEW_ROWS {
                    ui.label(
                        egui::RichText::new(format!(
                            "... and {} more (all of them will be moved)",
                            state.plan.entries.len() - PREVIEW_ROWS
                        ))
                        .font(crate::theme::font(11.0))
                        .color(theme.text_muted),
                    );
                }
            });

        ui.separator();

        // ── Apply ──────────────────────────────────────────────────
        ui.horizontal(|ui| {
            let pending = state.plan.pending_count();
            let plan_ready = !state.plan_dirty;
            let can_apply = pending > 0 && plan_ready;
            let label = if pending == 1 {
                "Move 1 file".to_string()
            } else {
                format!("Move {pending} files")
            };
            let btn = egui::Button::new(egui::RichText::new(label).color(Color32::WHITE))
                .fill(theme.accent);
            let response = ui.add_enabled(can_apply, btn);
            if !plan_ready {
                response
                    .clone()
                    .on_hover_text("Waiting for the preview to finish");
            }
            if response.clicked() {
                actions.apply = true;
            }
            if pending == 0 && !state.plan_dirty {
                ui.label(
                    egui::RichText::new("Nothing to do with this pattern.")
                        .font(crate::theme::font(11.5))
                        .color(theme.text_muted),
                );
            }
            if ui.button("Close").clicked() {
                close_requested = true;
            }
        });
    });
    if closed {
        is_open = false;
    }

    if close_requested {
        is_open = false;
    }
    state.is_open = is_open;
    actions
}

fn render_preview_row(ui: &mut egui::Ui, index: usize, entry: &OrganizeEntry, theme: &Theme) {
    let (marker, marker_color) = match (&entry.issue, entry.unchanged) {
        (Some(_), _) => ("!", theme.accent_highlight),
        (None, true) => ("=", theme.text_muted),
        (None, false) => ("->", theme.bit_perfect_green),
    };

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
        ui.label(
            egui::RichText::new(marker)
                .font(crate::theme::font(11.0))
                .color(marker_color),
        );
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 1.0);
            ui.label(
                egui::RichText::new(entry.from.display().to_string())
                    .font(crate::theme::font(11.0))
                    .color(theme.text_muted),
            );
            let target = match &entry.issue {
                None => entry.to.display().to_string(),
                Some(issue) => format!("{}  ({})", entry.to.display(), issue.label()),
            };
            ui.label(
                egui::RichText::new(target)
                    .font(crate::theme::font(11.5))
                    .color(if entry.issue.is_some() {
                        theme.accent_highlight
                    } else if entry.unchanged {
                        theme.text_muted
                    } else {
                        theme.text_primary
                    }),
            );
        });
        // Keep ids unique per row so egui does not mix up hover state.
        ui.push_id(("organize_row", index), |_ui| {});
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefs_round_trip_and_tolerate_a_partial_payload() {
        let prefs = OrganizePrefs {
            pattern: "{artist}/{album}/{track} - {title}".to_string(),
            destination: Some(PathBuf::from("/music/library")),
        };
        let json = serde_json::to_string(&prefs).unwrap();
        let back: OrganizePrefs = serde_json::from_str(&json).unwrap();
        assert_eq!(back.pattern, prefs.pattern);
        assert_eq!(back.destination, prefs.destination);

        // A payload written before a field existed must still load.
        let legacy: OrganizePrefs = serde_json::from_str("{}").unwrap();
        assert!(legacy.pattern.is_empty());
        assert!(legacy.destination.is_none());
    }

    #[test]
    fn opening_with_a_saved_pattern_keeps_it() {
        let mut state = OrganizeState {
            pattern: "{artist}/{album}/{title}".to_string(),
            destination: Some(PathBuf::from("/music/library")),
            ..Default::default()
        };
        state.open(Vec::new(), "test".to_string());
        assert_eq!(state.pattern, "{artist}/{album}/{title}");
        assert!(state.is_open);
    }

    #[test]
    fn choosing_a_destination_switches_between_name_and_tree_patterns() {
        let mut state = OrganizeState {
            pattern: DEFAULT_IN_PLACE_PATTERN.to_string(),
            ..Default::default()
        };
        state.set_destination(Some(PathBuf::from("/music/library")));
        assert_eq!(state.pattern, DEFAULT_TREE_PATTERN);

        state.set_destination(None);
        assert_eq!(state.pattern, DEFAULT_IN_PLACE_PATTERN);
    }

    #[test]
    fn an_explicit_pattern_is_not_clobbered_by_a_destination_change() {
        let mut state = OrganizeState {
            pattern: "{album}/{title}".to_string(),
            ..Default::default()
        };
        state.set_destination(Some(PathBuf::from("/music/library")));
        assert_eq!(state.pattern, "{album}/{title}", "user text wins");
    }
}
