//! The per-track DSP editor.
//!
//! A track can carry its own version of the whole chain — a graphic curve, a
//! preamp, a different crossover, a crossfeed profile — applied when that
//! track plays. The profile is stored as a **diff** against the user's global
//! chain: it names only the parameters it changes and inherits everything
//! else, so a global change still reaches the tracks that did not override it.

use crate::state::chain_override::{self, OverrideLine};
use crate::state::ChainPreset;
use crate::theme::Theme;
use database::TrackDspRepo;
use domain::ids::TrackId;
use domain::models::Track;
use eframe::egui::{self, Color32, CornerRadius, Vec2};

/// What the editor did.
#[derive(Debug, Clone, Copy, Default)]
pub struct TrackDspActions {
    /// The current chain was saved as this track's override.
    pub save_profile: bool,
    /// The track's profile was removed.
    pub clear_profile: bool,
}

/// The editor's state.
#[derive(Default)]
pub struct TrackDspModalState {
    pub is_open: bool,
    /// The track the editor is about.
    pub track_id: Option<TrackId>,
}

/// The chain context a per-track save is authored in.
pub struct TrackDspContext<'a> {
    /// The chain as the panels hold it right now.
    pub current_chain: &'a ChainPreset,
    /// The chain the save is diffed against: the user's global chain, or the
    /// chain this track started with while it is unprofiled.
    pub base_chain: &'a ChainPreset,
    /// Whether the edited track is the one playing, which decides whether a
    /// save can also update the engine and the snapshots.
    pub is_current_track: bool,
}

/// Show the per-track DSP editor for a track.
///
/// The difference between `context.current_chain` and `context.base_chain` is
/// the override the save button writes.
pub fn show_track_dsp_modal(
    ctx: &egui::Context,
    state: &mut TrackDspModalState,
    repo: &TrackDspRepo,
    track: Option<&Track>,
    context: TrackDspContext<'_>,
    theme: &Theme,
) -> TrackDspActions {
    let current_chain = context.current_chain;
    let global_chain = context.base_chain;
    let is_current_track = context.is_current_track;
    let mut actions = TrackDspActions::default();
    if !state.is_open {
        return actions;
    }
    let Some(track) = track else {
        state.is_open = false;
        return actions;
    };
    let _ = state.track_id;

    let profile = repo.get(&track.id).ok().flatten();
    let has_profile = profile.as_ref().map(|p| !p.is_neutral()).unwrap_or(false);
    let stored_lines: Vec<OverrideLine> = profile
        .as_ref()
        .and_then(|p| chain_override::legacy_patch(p, global_chain))
        .map(|patch| chain_override::describe_overrides(&patch))
        .unwrap_or_default();
    let pending = chain_override::diff_chain(global_chain, current_chain);
    let pending_count = pending
        .as_ref()
        .map(|p| chain_override::describe_overrides(p).len())
        .unwrap_or(0);

    let closed = crate::dialog::show_modal(ctx, "track_dsp", "Per-track DSP", 520.0, theme, |ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_lg);

        ui.label(
            egui::RichText::new(track.title.clone())
                .font(crate::theme::font(15.0))
                .strong()
                .color(theme.text_primary),
        );
        ui.label(
            egui::RichText::new(if track.artist.is_empty() {
                "Unknown artist".to_string()
            } else {
                track.artist.clone()
            })
            .font(crate::theme::font(12.0))
            .color(theme.text_muted),
        );

        // The state.
        let state_card = egui::Frame::NONE
            .fill(theme.bg_card_hover)
            .corner_radius(CornerRadius::same(theme.radius_md))
            .inner_margin(egui::Margin::symmetric(12, 10));
        state_card.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                ui.label(
                    egui::RichText::new(if has_profile {
                        format!(
                            "This track overrides {} chain parameter{}.",
                            stored_lines.len(),
                            if stored_lines.len() == 1 { "" } else { "s" }
                        )
                    } else {
                        "This track plays with your global chain.".to_string()
                    })
                    .font(crate::theme::font(12.0))
                    .color(if has_profile {
                        theme.accent_highlight
                    } else {
                        theme.text_secondary
                    }),
                );
            });
        });

        // The parameters the profile changes, with the value it sets. This is
        // the whole point of the diff: it is a short list, not a copy of the
        // chain.
        if has_profile {
            let card = egui::Frame::NONE
                .fill(theme.bg_card_hover)
                .corner_radius(CornerRadius::same(theme.radius_md))
                .inner_margin(egui::Margin::symmetric(12, 8));
            card.show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("track_dsp_overrides")
                    .max_height(300.0)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xs);
                        for line in &stored_lines {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                                ui.allocate_ui_with_layout(
                                    Vec2::new(92.0, 16.0),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(line.section)
                                                    .font(crate::theme::font(10.5))
                                                    .color(theme.text_muted),
                                            )
                                            .truncate(),
                                        );
                                    },
                                );
                                // Reserve the value column so a long label
                                // truncates instead of running under it.
                                let label_w = (ui.available_width() - 100.0).max(80.0);
                                ui.allocate_ui_with_layout(
                                    Vec2::new(label_w, 16.0),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(line.label.clone())
                                                    .font(crate::theme::font(12.0))
                                                    .color(theme.text_primary),
                                            )
                                            .truncate(),
                                        );
                                    },
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            egui::RichText::new(line.value.clone())
                                                .font(crate::theme::mono_font(10.5))
                                                .color(theme.accent_highlight),
                                        );
                                    },
                                );
                            });
                        }
                    });
            });
        }

        let explanation = if has_profile {
            "Only the parameters named above are stored. Everything else — \
             crossfeed, the mix bus, anything you have not touched — is \
             inherited from your global chain, including changes you make to \
             it later."
        } else {
            "Only the parameters you override are stored. Everything else — \
             crossfeed, the mix bus, anything you have not touched — stays \
             inherited from your global chain, including changes you make to \
             it later."
        };
        ui.label(
            egui::RichText::new(explanation)
                .font(crate::theme::font(11.5))
                .color(theme.text_dim),
        );

        ui.add_space(theme.space_sm);
        // A per-track chain is authored against what the track sounds like
        // while it plays. For any other track, the panels show the current
        // track's chain, so a diff there would attribute the wrong changes to
        // this track.
        let can_save = pending.is_some() && is_current_track;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);

            let save = ui
                .add_enabled(
                    can_save,
                    egui::Button::new(
                        egui::RichText::new("Save the current chain for this track")
                            .font(crate::theme::font(12.5))
                            .color(theme.text_primary),
                    )
                    .fill(theme.bg_pill)
                    .corner_radius(CornerRadius::same(theme.radius_md)),
                )
                .on_hover_text(
                    "Store the difference between your global chain and the chain \
                     as it stands now, and apply it whenever this track plays",
                );
            if save.clicked() {
                actions.save_profile = true;
            }

            if has_profile {
                let clear = ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("Remove the override")
                                .font(crate::theme::font(12.5))
                                .color(theme.destructive),
                        )
                        .fill(Color32::TRANSPARENT)
                        .corner_radius(CornerRadius::same(theme.radius_md)),
                    )
                    .on_hover_text("Play this track with the global chain again");
                if clear.clicked() {
                    actions.clear_profile = true;
                }
            }
        });

        if can_save {
            ui.label(
                egui::RichText::new(format!(
                    "The chain as it stands differs from your global chain in \
                     {pending_count} parameter{}.",
                    if pending_count == 1 { "" } else { "s" }
                ))
                .font(crate::theme::font(11.0))
                .color(theme.accent_highlight),
            );
        } else if is_current_track {
            ui.label(
                egui::RichText::new(
                    "The chain as it stands matches the chain this track started with. \
                     Change the EQ, tone, dynamics, filters or mix bus while it plays, \
                     then save.",
                )
                .font(crate::theme::font(11.0))
                .color(theme.text_dim),
            );
        } else {
            ui.label(
                egui::RichText::new(
                    "Play this track to shape its own chain: the editor compares what \
                     you hear with the chain the track started with.",
                )
                .font(crate::theme::font(11.0))
                .color(theme.text_dim),
            );
        }

        if let Some(note) = &profile {
            if note.overrides.is_none() && has_profile {
                ui.label(
                    egui::RichText::new(
                        "This override was saved by an older version as a graphic EQ \
                         curve; saving it again stores it as a full chain diff.",
                    )
                    .font(crate::theme::font(10.5))
                    .color(theme.text_dim),
                );
            }
        }
    });

    if closed {
        state.is_open = false;
    }
    actions
}

/// The profile for the chain as it stands, relative to the global chain.
pub fn profile_from_chain(
    global: &ChainPreset,
    current: &ChainPreset,
) -> domain::models::TrackDspProfile {
    domain::models::TrackDspProfile {
        graphic_gains: Vec::new(),
        graphic_enabled: false,
        graphic_preamp_db: 0.0,
        overrides: chain_override::diff_chain(global, current),
    }
}
