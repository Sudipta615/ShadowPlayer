//! A command palette: one text box that reaches everything in the app.
//!
//! Typing filters a scored list of commands — navigation, playlists, tracks,
//! toggles and the panels that are otherwise buried in a menu. It is the
//! fastest way to reach anything in a library of ten thousand tracks, and the
//! closest thing in this app to the "quick search" every serious desktop
//! player has.

use crate::theme::Theme;

use eframe::egui::{self, CornerRadius, Key, Pos2, Sense, Stroke, Vec2};

/// What a palette entry does when it is chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteAction {
    /// Switch the sidebar to a destination.
    Nav(crate::components::sidebar::NavItem),
    /// Toggle the light/dark theme.
    ToggleTheme,
    /// Toggle random shuffle.
    ToggleShuffle,
    /// Cycle the repeat mode.
    ToggleRepeat,
    /// Toggle mute.
    ToggleMute,
    /// Switch the bit-perfect "Pure Direct" mode.
    ToggleBitPerfect,
    /// Open the DSP studio.
    OpenEq,
    /// Open the playlist studio.
    OpenPlaylistEditor,
    /// Open the AutoEQ calibration dialog.
    OpenAutoEq,
    /// Open the bit-perfect inspector.
    OpenBitPerfect,
    /// Open the duplicate-track finder.
    OpenDuplicates,
    /// Open the batch organiser.
    OpenOrganizer,
    /// Open the library manager.
    OpenFolders,
    /// Import files.
    ImportFiles,
    /// Import a folder.
    ImportFolder,
    /// Put the caret in the search box.
    FocusSearch,
    /// Dismiss the palette itself.
    ClosePalette,
    /// Play a track by id.
    PlayTrack(domain::ids::TrackId),
}

/// One row in the palette.
#[derive(Debug, Clone)]
pub struct PaletteEntry {
    /// The group the entry is listed under.
    pub category: &'static str,
    /// What the entry is called.
    pub title: String,
    /// A secondary line: an album, an artist, a key hint.
    pub hint: String,
    /// The keyboard hint, if the entry has one.
    pub shortcut: Option<String>,
    /// What happens when it is chosen.
    pub action: PaletteAction,
}

impl PaletteEntry {
    pub fn new(
        category: &'static str,
        title: impl Into<String>,
        hint: impl Into<String>,
        action: PaletteAction,
    ) -> Self {
        Self {
            category,
            title: title.into(),
            hint: hint.into(),
            shortcut: None,
            action,
        }
    }

    pub fn with_shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }
}

/// The palette's own state.
#[derive(Default)]
pub struct CommandPaletteState {
    pub query: String,
    /// Index of the highlighted row among the filtered entries.
    pub selected: usize,
}

/// Score a candidate against a query.
///
/// Returns `None` when the query is not a subsequence of the candidate. The
/// score rewards the things a person would expect: a prefix match beats a
/// match in the middle of a word, a match at a word boundary beats one
/// inside a word, and consecutive characters beat scattered ones.
pub fn fuzzy_score(candidate: &str, query: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let hay: Vec<char> = candidate.chars().collect();
    let needle: Vec<char> = query.chars().collect();

    let mut score = 0i32;
    let mut hay_idx = 0usize;
    let mut last_match: Option<usize> = None;
    let mut first_match: Option<usize> = None;

    for &n in &needle {
        let lower = n.to_lowercase().next().unwrap_or(n);
        let mut found = None;
        while hay_idx < hay.len() {
            let c = hay[hay_idx];
            let cl = c.to_lowercase().next().unwrap_or(c);
            if cl == lower {
                found = Some(hay_idx);
                hay_idx += 1;
                break;
            }
            hay_idx += 1;
        }
        let at = found?;
        if first_match.is_none() {
            first_match = Some(at);
        }

        // A match at the very start, or right after a separator, is worth
        // much more than one buried inside a word.
        let at_boundary = at == 0
            || hay[at - 1].is_whitespace()
            || hay[at - 1] == '/'
            || hay[at - 1] == '-'
            || hay[at - 1] == '_'
            || hay[at - 1] == '(';
        if at_boundary {
            score += 12;
        } else {
            score += 4;
        }
        // Consecutive characters are a stronger signal than scattered ones.
        if let Some(prev) = last_match {
            if at == prev + 1 {
                score += 8;
            } else {
                score -= 1;
            }
        }
        last_match = Some(at);
    }

    // The structural bonuses, in strict order of strength:
    //
    //   1. the candidate *starts* with the whole query — "album" means the
    //      entry called "Albums", not the one called "About the album";
    //   2. the query ends on a word boundary — "the album" beats "albumen";
    //   3. nothing extra — a mid-word fragment is the weakest match.
    //
    // The first two are mutually exclusive: a candidate that starts with the
    // query is a prefix match by definition.
    let whole_query_is_prefix =
        first_match == Some(0) && last_match == Some(needle.len().saturating_sub(1));
    let ends_on_word_boundary = last_match.map(|last| {
        last + 1 == hay.len()
            || hay[last + 1].is_whitespace()
            || hay[last + 1] == '/'
            || hay[last + 1] == '-'
            || hay[last + 1] == '_'
            || hay[last + 1] == ')'
    });

    if whole_query_is_prefix {
        score += 24;
    } else if ends_on_word_boundary == Some(true) {
        score += 14;
    }

    // A shorter candidate that still matched is the better answer.
    score -= (hay.len() as i32) / 8;
    Some(score)
}

/// Filter and rank the entries against a query.
pub fn filter_entries<'a>(
    entries: &'a [PaletteEntry],
    query: &str,
) -> Vec<(i32, &'a PaletteEntry)> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return entries.iter().map(|e| (0, e)).collect();
    }
    let mut scored: Vec<(i32, &PaletteEntry)> = entries
        .iter()
        .filter_map(|e| {
            // The title is what the user is typing; the hint only breaks ties.
            fuzzy_score(&e.title, trimmed)
                .or_else(|| fuzzy_score(&e.hint, trimmed))
                .map(|s| (s, e))
        })
        .collect();
    scored.sort_by_key(|a| std::cmp::Reverse(a.0));
    scored
}

/// Show the palette and return the chosen action.
///
/// Returns `None` when it is closed without a choice.
pub fn show_command_palette(
    ctx: &egui::Context,
    state: &mut CommandPaletteState,
    entries: &[PaletteEntry],
    theme: &Theme,
) -> Option<PaletteAction> {
    let mut chosen = None;

    let backdrop_clicked =
        crate::dialog::show_modal_bare(ctx, "command_palette", 520.0, theme, |ui| {
            let filtered = filter_entries(entries, &state.query);

            // ── The query field ─────────────────────────────────────────────
            let field = egui::Frame::NONE
                .fill(theme.bg_input)
                .stroke(Stroke::new(1.0, theme.border_input))
                .corner_radius(CornerRadius::same(theme.radius_md))
                .inner_margin(egui::Margin::symmetric(12, 7));
            field.show(ui, |ui| {
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut state.query)
                        .id(egui::Id::unique("command_palette_query"))
                        .hint_text("Type a command, a playlist or a track…")
                        .font(crate::theme::font(14.0))
                        .desired_width(ui.available_width()),
                );
                resp.request_focus();
            });

            // Arrow keys and Enter drive the list, so the palette is usable
            // without a mouse.
            let mut move_up = false;
            let mut move_down = false;
            let mut activate = false;
            ctx.input(|i| {
                if i.key_pressed(Key::ArrowDown) {
                    move_down = true;
                }
                if i.key_pressed(Key::ArrowUp) {
                    move_up = true;
                }
                if i.key_pressed(Key::Enter) {
                    activate = true;
                }
            });

            if move_down && !filtered.is_empty() {
                state.selected = (state.selected + 1) % filtered.len();
            }
            if move_up && !filtered.is_empty() {
                state.selected = (state.selected + filtered.len() - 1) % filtered.len();
            }
            if state.selected >= filtered.len() {
                state.selected = 0;
            }

            ui.add_space(6.0);

            if filtered.is_empty() {
                ui.add_space(18.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("Nothing matches that")
                            .font(crate::theme::font(13.0))
                            .color(theme.text_muted),
                    );
                });
                ui.add_space(18.0);
            } else {
                // ── The results ───────────────────────────────────────────────
                let row_h = 40.0;
                let max_rows = 9usize;
                // The row count comes from the viewport, not from the modal's
                // own available height: that keeps the dialog the same size
                // every time it opens, and the list always ends on a whole
                // row instead of a half one.
                let screen_h = crate::dialog::viewport_rect(ctx).height();
                let rows = (((screen_h * 0.42) / row_h) as usize).clamp(3, max_rows);
                let shown = filtered.len().min(rows);
                egui::ScrollArea::vertical()
                    .id_salt("command_palette_results")
                    .auto_shrink([false, false])
                    .max_height(row_h * shown as f32)
                    .show_rows(ui, row_h, filtered.len(), |ui, row_range| {
                        ui.spacing_mut().item_spacing = Vec2::new(0.0, 4.0);
                        for row in row_range {
                            let (score, entry) = filtered[row];
                            let is_selected = row == state.selected;

                            let (rect, resp) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), row_h - 4.0),
                                Sense::click(),
                            );
                            let painter = ui.painter_at(rect);

                            if is_selected {
                                painter.rect_filled(
                                    rect,
                                    CornerRadius::same(theme.radius_md),
                                    theme.bg_card_hover,
                                );
                                painter.rect_stroke(
                                    rect,
                                    CornerRadius::same(theme.radius_md),
                                    Stroke::new(1.0, theme.accent_highlight),
                                    egui::StrokeKind::Inside,
                                );
                            }

                            // The category, the title, and the hint.
                            painter.text(
                                Pos2::new(rect.min.x + 10.0, rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                entry.category,
                                crate::theme::font(10.0),
                                theme.text_dim,
                            );
                            let title_x = rect.min.x + 76.0;
                            let title_galley = painter.layout_no_wrap(
                                entry.title.clone(),
                                crate::theme::font(13.0),
                                theme.text_primary,
                            );
                            painter.galley(
                                Pos2::new(title_x, rect.center().y - title_galley.size().y * 0.5),
                                title_galley.clone(),
                                theme.text_primary,
                            );
                            if !entry.hint.is_empty() {
                                // The hint is truncated with an ellipsis
                                // rather than clipped mid-word.
                                let shortcut_w = if entry.shortcut.is_some() { 58.0 } else { 12.0 };
                                let hint_room = (rect.max.x
                                    - 76.0
                                    - title_x
                                    - title_galley.size().x
                                    - 12.0
                                    - shortcut_w)
                                    .max(0.0);
                                if hint_room > 20.0 {
                                    let mut job = egui::text::LayoutJob::simple_singleline(
                                        entry.hint.clone(),
                                        crate::theme::font(11.0),
                                        theme.text_muted,
                                    );
                                    job.wrap.max_width = hint_room;
                                    job.wrap.break_anywhere = false;
                                    job.wrap.overflow_character = Some('…');
                                    let hint_galley = painter.layout_job(job);
                                    painter.galley(
                                        Pos2::new(
                                            title_x + title_galley.size().x + 10.0,
                                            rect.center().y - hint_galley.size().y * 0.5,
                                        ),
                                        hint_galley,
                                        theme.text_muted,
                                    );
                                }
                            }
                            if let Some(shortcut) = &entry.shortcut {
                                painter.text(
                                    Pos2::new(rect.max.x - 10.0, rect.center().y),
                                    egui::Align2::RIGHT_CENTER,
                                    shortcut,
                                    crate::theme::mono_font(10.5),
                                    theme.text_dim,
                                );
                            }

                            resp.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::Role::Button,
                                    true,
                                    format!("{}: {}", entry.category, entry.title),
                                )
                            });

                            if resp.clicked() {
                                chosen = Some(entry.action.clone());
                            }
                            if activate && row == state.selected {
                                chosen = Some(entry.action.clone());
                            }
                            let _ = score;
                        }
                    });
            }
        });

    if backdrop_clicked {
        chosen = Some(PaletteAction::ClosePalette);
    }

    chosen
}

#[cfg(test)]
mod tests {
    use super::{filter_entries, fuzzy_score, PaletteAction, PaletteEntry};

    #[test]
    fn a_prefix_match_beats_a_scattered_one() {
        let prefix = fuzzy_score("Recently added", "rec").unwrap();
        let scattered = fuzzy_score("Record collection archive", "rec").unwrap();
        assert!(
            prefix > scattered,
            "prefix {prefix} should beat scattered {scattered}"
        );
    }

    #[test]
    fn a_match_at_a_word_boundary_beats_one_inside_a_word() {
        let boundary = fuzzy_score("Recently added", "add").unwrap();
        let inside = fuzzy_score("Radiohead album", "add").unwrap();
        assert!(boundary > inside);
    }

    #[test]
    fn a_query_that_is_not_a_subsequence_does_not_match() {
        assert!(fuzzy_score("Recently added", "zzz").is_none());
        assert!(fuzzy_score("Recently added", "recentlyy").is_none());
    }

    #[test]
    fn an_empty_query_matches_everything() {
        assert_eq!(fuzzy_score("anything", ""), Some(0));
    }

    #[test]
    fn the_matching_is_case_insensitive() {
        assert!(fuzzy_score("Recently Added", "recently").is_some());
        assert!(fuzzy_score("recently added", "RECENTLY").is_some());
    }

    #[test]
    fn filtering_keeps_the_best_matches_first() {
        let entries = vec![
            PaletteEntry::new("Go to", "Albums", "", PaletteAction::ToggleTheme),
            PaletteEntry::new("Go to", "About the album", "", PaletteAction::ToggleTheme),
            PaletteEntry::new("Go to", "Genres", "", PaletteAction::ToggleTheme),
        ];
        let filtered = filter_entries(&entries, "album");
        assert_eq!(filtered.len(), 2);
        // "Albums" *is* the query, so it wins over a phrase that merely
        // contains the word.
        assert_eq!(filtered[0].1.title, "Albums");
    }

    #[test]
    fn a_candidate_that_starts_with_the_query_wins() {
        // "About the album" only matches the first letter at the start; the
        // whole query is not a prefix of it.
        let prefix = fuzzy_score("Albums", "album").unwrap();
        let not_prefix = fuzzy_score("About the album", "album").unwrap();
        assert!(
            prefix > not_prefix,
            "prefix {prefix} should beat not-prefix {not_prefix}"
        );
    }

    #[test]
    fn a_prefix_match_beats_a_whole_word_inside_a_phrase() {
        // The rule the scorer implements: a candidate that starts with the
        // query beats a phrase that merely contains the word.
        let prefix = fuzzy_score("Albums", "album").unwrap();
        let phrase = fuzzy_score("About the album", "album").unwrap();
        assert!(
            prefix > phrase,
            "prefix {prefix} should beat phrase {phrase}"
        );
    }

    #[test]
    fn a_whole_word_beats_a_mid_word_fragment() {
        // Neither is a prefix, so the word boundary decides. "albumen" would
        // not do: it *is* a prefix match, and a prefix is the stronger signal.
        let whole = fuzzy_score("the album", "album").unwrap();
        let mid = fuzzy_score("subalbumx", "album").unwrap();
        assert!(whole > mid, "whole {whole} should beat mid {mid}");
    }

    #[test]
    fn a_prefix_match_beats_a_mid_word_fragment() {
        let prefix = fuzzy_score("albumen", "album").unwrap();
        let mid = fuzzy_score("subalbumx", "album").unwrap();
        assert!(prefix > mid, "prefix {prefix} should beat mid {mid}");
    }

    #[test]
    fn filtering_also_matches_the_hint() {
        let entries = vec![PaletteEntry::new(
            "Track",
            "Weightless",
            "Marconi Union — Ambient Transmissions",
            PaletteAction::ToggleTheme,
        )];
        assert_eq!(filter_entries(&entries, "marconi").len(), 1);
        assert_eq!(filter_entries(&entries, "nothing").len(), 0);
    }
}
