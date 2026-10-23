//! Auto-Tag modal — presents AcoustID / MusicBrainz lookup results and lets the
//! user confirm or reject each match before anything is written.
//!
//! The modal has two phases:
//!
//! 1. **Progress phase** — shown while the background worker is still running.
//!    Displays a progress bar and the file currently being fingerprinted.
//!
//! 2. **Review phase** — shown when results arrive. Each result gets its own
//!    expandable card:
//!    - Top candidate is pre-selected; the user can pick an alternative.
//!    - A diff view shows which fields would change vs. the current tag.
//!    - The user can check/uncheck individual fields (title, artist, album,
//!      year, track/disc number, genre) independently.
//!    - "Apply Selected" writes only the checked, confirmed matches.
//!    - "Skip All" dismisses without writing anything.

use crate::services::autotag_service::{
    AutotagMatch, AutotagOutcome, AutotagProgress, AutotagResult,
};
use crate::theme::Theme;
use domain::models::Track;
use eframe::egui::{self, CornerRadius, Stroke, Vec2};

// ── State ─────────────────────────────────────────────────────────────────────

/// Whether the user wants to apply a specific field from the auto-tag result.
#[derive(Clone, Debug)]
pub struct FieldSelection {
    pub title: bool,
    pub artist: bool,
    pub album: bool,
    pub year: bool,
    pub track_number: bool,
    pub disc_number: bool,
    pub genre: bool,
}

impl Default for FieldSelection {
    fn default() -> Self {
        Self {
            title: true,
            artist: true,
            album: true,
            year: true,
            track_number: true,
            disc_number: true,
            genre: false, // Genre write-back is off by default to avoid clobbering user genre.
        }
    }
}

/// UI state for one completed auto-tag result that needs review.
#[derive(Clone, Debug)]
pub struct ReviewEntry {
    pub result: AutotagResult,
    /// The track's current tags (from the library).
    pub current: Track,
    /// Index into `result.outcome.Matches` vec, i.e. which candidate is selected.
    pub selected_match: usize,
    /// Which fields the user wants to apply.
    pub fields: FieldSelection,
    /// Whether the card is expanded to show all candidates.
    pub expanded: bool,
    /// Dismissed = skip this one.
    pub dismissed: bool,
    /// Applied = confirmed and written.
    pub applied: bool,
}

impl ReviewEntry {
    pub fn new(result: AutotagResult, current: Track) -> Self {
        Self {
            result,
            current,
            selected_match: 0,
            fields: FieldSelection::default(),
            expanded: false,
            dismissed: false,
            applied: false,
        }
    }

    /// The currently selected `AutotagMatch`, if any.
    pub fn selected(&self) -> Option<&AutotagMatch> {
        if let AutotagOutcome::Matches(ref matches) = self.result.outcome {
            matches.get(self.selected_match)
        } else {
            None
        }
    }
}

/// Full auto-tag modal state, owned by the app.
#[derive(Default)]
pub struct AutotagModalState {
    pub is_open: bool,
    /// Phase 1: worker is running.
    pub progress: AutotagProgress,
    /// Phase 2: review queue.
    pub review: Vec<ReviewEntry>,
}

impl AutotagModalState {
    pub fn open_progress(&mut self, progress: AutotagProgress) {
        self.is_open = true;
        self.progress = progress;
        self.review.clear();
    }

    pub fn push_result(&mut self, result: AutotagResult, current: Track) {
        self.review.push(ReviewEntry::new(result, current));
    }

    pub fn mark_progress(&mut self, progress: AutotagProgress) {
        self.progress = progress;
    }

    pub fn close(&mut self) {
        self.is_open = false;
        self.review.clear();
    }
}

// ── Actions ───────────────────────────────────────────────────────────────────

/// What the modal wants the app to do this frame.
#[derive(Default)]
pub struct AutotagModalActions {
    /// Tracks the user confirmed, with the chosen match and field selections.
    pub apply: Vec<(ReviewEntry, AutotagMatch)>,
    /// The user cancelled / closed the modal.
    pub cancelled: bool,
}

pub type AutotagActions = AutotagModalActions;

// ── Render ────────────────────────────────────────────────────────────────────

pub fn show_autotag_modal(
    ctx: &egui::Context,
    state: &mut AutotagModalState,
    theme: &Theme,
) -> AutotagModalActions {
    let mut actions = AutotagModalActions::default();
    if !state.is_open {
        return actions;
    }

    let modal = egui::Modal::new(egui::Id::unique("autotag_modal"));
    modal.show(ctx, |ui| {
        ui.set_min_width(620.0);
        ui.set_max_width(820.0);

        // ── Title bar ────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Auto-Tag from MusicBrainz")
                    .color(theme.text_primary)
                    .size(18.0)
                    .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(24.0), egui::Sense::click());
                let color = if resp.hovered() {
                    theme.text_primary
                } else {
                    theme.text_muted
                };
                crate::components::icons::draw_window_close(ui, rect.shrink(5.0), color);
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Close"));
                if resp.clicked() {
                    actions.cancelled = true;
                    state.close();
                }
            });
        });

        ui.add_space(4.0);
        ui.separator();
        ui.add_space(8.0);

        if state.progress.running {
            show_progress_phase(ui, &state.progress, theme);
        } else {
            show_review_phase(ui, &mut state.review, theme, &mut actions);
        }

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(6.0);

        // ── Footer buttons ───────────────────────────────────────────────
        ui.horizontal(|ui| {
            if !state.progress.running {
                let apply_btn = egui::Button::new(
                    egui::RichText::new("Apply Selected").color(egui::Color32::WHITE),
                )
                .fill(theme.accent)
                .corner_radius(CornerRadius::same(theme.radius_md));
                if ui.add(apply_btn).clicked() {
                    // Collect all non-dismissed, non-applied entries with a selected match.
                    for entry in state.review.iter() {
                        if entry.dismissed || entry.applied {
                            continue;
                        }
                        if let Some(m) = entry.selected() {
                            actions.apply.push((entry.clone(), m.clone()));
                        }
                    }
                    state.close();
                }
            }

            let label = if state.progress.running {
                "Cancel"
            } else {
                "Skip All"
            };
            if ui
                .add(
                    egui::Button::new(egui::RichText::new(label).color(theme.text_secondary))
                        .fill(theme.bg_pill)
                        .corner_radius(CornerRadius::same(theme.radius_md)),
                )
                .clicked()
            {
                actions.cancelled = true;
                state.close();
            }

            if !state.progress.running {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let pending = state
                        .review
                        .iter()
                        .filter(|e| !e.dismissed && !e.applied)
                        .count();
                    ui.label(
                        egui::RichText::new(format!("{pending} pending"))
                            .color(theme.text_muted)
                            .size(11.0),
                    );
                });
            }
        });
    });

    actions
}

fn show_progress_phase(ui: &mut egui::Ui, progress: &AutotagProgress, theme: &Theme) {
    ui.label(
        egui::RichText::new("Fingerprinting tracks and querying MusicBrainz…")
            .color(theme.text_secondary),
    );
    ui.add_space(8.0);

    let fraction = if progress.queued > 0 {
        progress.done as f32 / progress.queued as f32
    } else {
        0.0
    };
    let bar_size = Vec2::new(ui.available_width(), 6.0);
    let (bar_rect, _) = ui.allocate_exact_size(bar_size, egui::Sense::hover());
    ui.painter().rect_filled(
        bar_rect,
        CornerRadius::same(theme.radius_xs),
        theme.bg_card_hover,
    );
    let filled = egui::Rect::from_min_size(
        bar_rect.min,
        Vec2::new(bar_rect.width() * fraction, bar_rect.height()),
    );
    ui.painter()
        .rect_filled(filled, CornerRadius::same(theme.radius_xs), theme.accent);

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{}/{}", progress.done, progress.queued))
                .color(theme.text_muted)
                .size(11.0),
        );
        if !progress.current.is_empty() {
            ui.label(
                egui::RichText::new(format!("— {}", progress.current))
                    .color(theme.text_dim)
                    .size(11.0),
            );
        }
        if progress.failed > 0 {
            ui.label(
                egui::RichText::new(format!("{} failed", progress.failed))
                    .color(theme.destructive)
                    .size(11.0),
            );
        }
    });

    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "Network access: api.acoustid.org · musicbrainz.org\n\
             No personal data is transmitted.",
        )
        .color(theme.text_dim)
        .size(10.5),
    );
}

fn show_review_phase(
    ui: &mut egui::Ui,
    review: &mut [ReviewEntry],
    theme: &Theme,
    actions: &mut AutotagModalActions,
) {
    if review.is_empty() {
        ui.label(
            egui::RichText::new("No matches found for the selected tracks.")
                .color(theme.text_muted),
        );
        return;
    }

    egui::ScrollArea::vertical()
        .max_height(480.0)
        .show(ui, |ui| {
            for entry in review.iter_mut() {
                if entry.dismissed || entry.applied {
                    continue;
                }
                show_review_card(ui, entry, theme, actions);
                ui.add_space(8.0);
            }
        });
}

fn show_review_card(
    ui: &mut egui::Ui,
    entry: &mut ReviewEntry,
    theme: &Theme,
    _actions: &mut AutotagModalActions,
) {
    let file_name = entry
        .result
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?")
        .to_string();

    let outcome = entry.result.outcome.clone();
    let card_bg = theme.bg_card;
    let card_border = theme.border_card;

    egui::Frame::new()
        .fill(card_bg)
        .stroke(Stroke::new(1.0, card_border))
        .corner_radius(CornerRadius::same(theme.radius_lg))
        .inner_margin(egui::Margin::same(theme.space_lg as i8))
        .show(ui, |ui| {
            // ── Card header ───────────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(&file_name)
                        .color(theme.text_primary)
                        .strong()
                        .size(13.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button(egui::RichText::new("Skip").color(theme.text_muted))
                        .clicked()
                    {
                        entry.dismissed = true;
                    }
                });
            });

            match &outcome {
                AutotagOutcome::Matches(matches) => {
                    let selected = entry.selected_match.min(matches.len().saturating_sub(1));
                    let m = &matches[selected];

                    ui.add_space(6.0);

                    // ── Confidence badge ──────────────────────────────────
                    let score_pct = (m.score * 100.0) as u32;
                    let score_color = if m.score >= 0.85 {
                        theme.bit_perfect_green
                    } else if m.score >= 0.70 {
                        theme.warning_yellow
                    } else {
                        theme.text_muted
                    };
                    ui.label(
                        egui::RichText::new(format!(
                            "Confidence: {score_pct}%  ·  {}  ·  MB: {}",
                            match m.release_rank {
                                0 => "Album",
                                1 => "EP",
                                2 => "Single",
                                _ => "Other",
                            },
                            &m.recording_id[..8.min(m.recording_id.len())]
                        ))
                        .color(score_color)
                        .size(11.0),
                    );

                    ui.add_space(6.0);

                    // ── Field diff table ──────────────────────────────────
                    show_field_diff(ui, entry, m, theme);

                    // ── Alternative candidates ────────────────────────────
                    if matches.len() > 1 {
                        ui.add_space(4.0);
                        let expand_label = if entry.expanded {
                            format!("▲ Hide {} alternatives", matches.len() - 1)
                        } else {
                            format!("▼ Show {} alternatives", matches.len() - 1)
                        };
                        if ui
                            .small_button(egui::RichText::new(expand_label).color(theme.info))
                            .clicked()
                        {
                            entry.expanded = !entry.expanded;
                        }

                        if entry.expanded {
                            ui.add_space(4.0);
                            for (i, alt) in matches.iter().enumerate() {
                                if i == entry.selected_match {
                                    continue;
                                }
                                let alt_label = format!(
                                    "{}. {} – {} / {} ({:?}) [{:.0}%]",
                                    i + 1,
                                    alt.title,
                                    alt.artist,
                                    alt.album,
                                    match alt.release_rank {
                                        0 => "Album",
                                        1 => "EP",
                                        2 => "Single",
                                        _ => "Other",
                                    },
                                    alt.score * 100.0,
                                );
                                if ui
                                    .small_button(
                                        egui::RichText::new(&alt_label).color(theme.text_secondary),
                                    )
                                    .clicked()
                                {
                                    entry.selected_match = i;
                                }
                            }
                        }
                    }
                }
                AutotagOutcome::LowConfidence => {
                    ui.label(
                        egui::RichText::new("No high-confidence match found.")
                            .color(theme.text_muted),
                    );
                }
                AutotagOutcome::NoRecording => {
                    ui.label(
                        egui::RichText::new("AcoustID found results but no MusicBrainz recording.")
                            .color(theme.text_muted),
                    );
                }
                AutotagOutcome::Error(e) => {
                    ui.label(egui::RichText::new(format!("Error: {e}")).color(theme.destructive));
                }
            }
        });
}

/// Render a two-column diff (Current | Proposed) for each metadata field.
fn show_field_diff(ui: &mut egui::Ui, entry: &mut ReviewEntry, m: &AutotagMatch, theme: &Theme) {
    let current = &entry.current;
    let fields = [
        (
            "Title",
            current.title.as_str(),
            m.title.as_str(),
            &mut entry.fields.title,
        ),
        (
            "Artist",
            current.artist.as_str(),
            m.artist.as_str(),
            &mut entry.fields.artist,
        ),
        (
            "Album",
            current.album.as_str(),
            m.album.as_str(),
            &mut entry.fields.album,
        ),
    ];

    egui::Grid::new(format!("atag_diff_{:?}", entry.result.track_id))
        .num_columns(4)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            // Header row
            ui.label(
                egui::RichText::new("Apply")
                    .color(theme.text_dim)
                    .size(11.0),
            );
            ui.label(
                egui::RichText::new("Field")
                    .color(theme.text_dim)
                    .size(11.0),
            );
            ui.label(
                egui::RichText::new("Current")
                    .color(theme.text_dim)
                    .size(11.0),
            );
            ui.label(
                egui::RichText::new("Proposed")
                    .color(theme.text_dim)
                    .size(11.0),
            );
            ui.end_row();

            for (label, old, new, apply) in fields {
                ui.checkbox(apply, "");
                ui.label(
                    egui::RichText::new(label)
                        .color(theme.text_secondary)
                        .size(12.0),
                );
                let old_color = if old == new {
                    theme.text_dim
                } else {
                    theme.text_muted
                };
                ui.label(
                    egui::RichText::new(if old.is_empty() { "—" } else { old })
                        .color(old_color)
                        .size(12.0),
                );
                let new_color = if old == new {
                    theme.text_dim
                } else {
                    theme.accent_highlight
                };
                ui.label(
                    egui::RichText::new(if new.is_empty() { "—" } else { new })
                        .color(new_color)
                        .size(12.0),
                );
                ui.end_row();
            }

            // Year
            let old_year = current.year.map(|y| y.to_string()).unwrap_or_default();
            let new_year = m.year.map(|y| y.to_string()).unwrap_or_default();
            ui.checkbox(&mut entry.fields.year, "");
            ui.label(
                egui::RichText::new("Year")
                    .color(theme.text_secondary)
                    .size(12.0),
            );
            ui.label(
                egui::RichText::new(if old_year.is_empty() {
                    "—"
                } else {
                    &old_year
                })
                .color(theme.text_dim)
                .size(12.0),
            );
            ui.label(
                egui::RichText::new(if new_year.is_empty() {
                    "—"
                } else {
                    &new_year
                })
                .color(if old_year == new_year {
                    theme.text_dim
                } else {
                    theme.accent_highlight
                })
                .size(12.0),
            );
            ui.end_row();

            // Genre
            let old_genre = current.genre.as_deref().unwrap_or_default();
            let new_genre = m.genre.as_deref().unwrap_or_default();
            ui.checkbox(&mut entry.fields.genre, "");
            ui.label(
                egui::RichText::new("Genre")
                    .color(theme.text_secondary)
                    .size(12.0),
            );
            ui.label(
                egui::RichText::new(if old_genre.is_empty() {
                    "—"
                } else {
                    old_genre
                })
                .color(theme.text_dim)
                .size(12.0),
            );
            ui.label(
                egui::RichText::new(if new_genre.is_empty() {
                    "—"
                } else {
                    new_genre
                })
                .color(theme.text_dim)
                .size(12.0),
            );
            ui.end_row();
        });
}
