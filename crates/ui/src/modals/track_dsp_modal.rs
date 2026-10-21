//! The per-track DSP editor.
//!
//! A track can carry its own graphic EQ and preamp, applied when that track
//! plays. This is the setting every serious local player has: the track that
//! was mastered too loud, or the one that needs a corrective curve, gets its
//! own values instead of changing the whole library.

use crate::theme::Theme;
use database::TrackDspRepo;
use domain::ids::TrackId;
use domain::models::Track;
use eframe::egui::{self, Color32, CornerRadius, Vec2};

/// What the editor did.
#[derive(Debug, Clone, Copy, Default)]
pub struct TrackDspActions {
    /// The current EQ was saved as this track's profile.
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

/// Show the per-track DSP editor for a track.
pub fn show_track_dsp_modal(
    ctx: &egui::Context,
    state: &mut TrackDspModalState,
    repo: &TrackDspRepo,
    track: Option<&Track>,
    // The graphic EQ as it stands right now, so it can be saved as the
    // track's profile.
    current_eq: (bool, Vec<f32>, f32),
    theme: &Theme,
) -> TrackDspActions {
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
    let (eq_enabled, eq_gains, _eq_preamp) = current_eq;
    let has_eq = eq_enabled && eq_gains.iter().any(|g| g.abs() > 0.01);

    let closed = crate::dialog::show_modal(ctx, "track_dsp", "Per-track DSP", 480.0, theme, |ui| {
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
                        "This track has its own EQ profile."
                    } else {
                        "This track plays with the global settings."
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

        ui.add_space(theme.space_xxs);
        ui.label(
            egui::RichText::new(
                "A per-track profile overrides the graphic EQ and its preamp for \
                     this track only. Everything else — the compressor, the \
                     crossfeed, the spatial stage — stays as you have it set.",
            )
            .font(crate::theme::font(11.5))
            .color(theme.text_dim),
        );

        ui.add_space(theme.space_sm);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);

            let save = ui
                .add_enabled(
                    has_eq,
                    egui::Button::new(
                        egui::RichText::new("Save the current EQ as this track's profile")
                            .font(crate::theme::font(12.5))
                            .color(theme.text_primary),
                    )
                    .fill(theme.bg_pill)
                    .corner_radius(CornerRadius::same(theme.radius_md)),
                )
                .on_hover_text(
                    "Store the graphic EQ as it stands now, and apply it whenever \
                         this track plays",
                );
            if save.clicked() {
                actions.save_profile = true;
            }

            if has_profile {
                let clear = ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("Remove the profile")
                                .font(crate::theme::font(12.5))
                                .color(theme.destructive),
                        )
                        .fill(Color32::TRANSPARENT)
                        .corner_radius(CornerRadius::same(theme.radius_md)),
                    )
                    .on_hover_text("Play this track with the global settings again");
                if clear.clicked() {
                    actions.clear_profile = true;
                }
            }
        });

        if !has_eq {
            ui.label(
                egui::RichText::new(
                    "The graphic EQ is off or flat, so there is nothing to save yet.",
                )
                .font(crate::theme::font(11.0))
                .color(theme.text_dim),
            );
        }
    });

    if closed {
        state.is_open = false;
    }
    actions
}

/// The profile for the graphic EQ as it stands.
pub fn profile_from_eq(
    enabled: bool,
    gains: &[f32],
    preamp: f32,
) -> domain::models::TrackDspProfile {
    domain::models::TrackDspProfile {
        graphic_gains: gains.to_vec(),
        graphic_enabled: enabled,
        graphic_preamp_db: preamp,
    }
}
