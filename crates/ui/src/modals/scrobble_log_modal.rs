//! The scrobble log.
//!
//! A scrobble is a play that counted: thirty seconds, or half the track,
//! whichever came first. The log is the record of what you actually listened
//! to, as opposed to what you skipped past, and it is what the listening
//! statistics and the skip rules are built on.

use crate::theme::Theme;
use database::HistoryRepo;
use eframe::egui::{self, CornerRadius, Sense, Vec2};

/// Show the scrobble log.
pub fn show_scrobble_log_modal(
    ctx: &egui::Context,
    history_repo: &HistoryRepo,
    theme: &Theme,
) -> bool {
    let scrobbles = history_repo.list_scrobbles(200).unwrap_or_default();
    let total = history_repo.count_scrobbles().unwrap_or(0);

    let closed = crate::dialog::show_modal_scrollable(
        ctx,
        "scrobble_log",
        "Scrobble Log",
        520.0,
        theme,
        |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_md);

            // The headline.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);
                for (label, value) in [
                    ("Scrobbles", total.to_string()),
                    ("In the log", scrobbles.len().to_string()),
                ] {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(value)
                                .font(crate::theme::font(16.0))
                                .strong()
                                .color(theme.text_primary),
                        );
                        ui.label(
                            egui::RichText::new(label)
                                .font(crate::theme::font(10.5))
                                .color(theme.text_muted),
                        );
                    });
                }
            });

            ui.label(
                egui::RichText::new(
                    "A play counts as a scrobble once it has been heard for thirty \
                     seconds, or half the track, whichever comes first. A play that \
                     was skipped is not a scrobble.",
                )
                .font(crate::theme::font(11.5))
                .color(theme.text_dim),
            );

            if scrobbles.is_empty() {
                ui.add_space(theme.space_lg);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("Nothing scrobbled yet")
                            .font(crate::theme::font(13.0))
                            .color(theme.text_muted),
                    );
                });
                return;
            }

            // The log itself.
            let card = egui::Frame::NONE
                .fill(theme.bg_card_hover)
                .corner_radius(CornerRadius::same(theme.radius_md))
                .inner_margin(egui::Margin::symmetric(12, 8));
            card.show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xs);
                for (track, played_at, listened) in &scrobbles {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                        ui.label(
                            egui::RichText::new(track.title.clone())
                                .font(crate::theme::font(12.0))
                                .color(theme.text_primary),
                        );
                        ui.label(
                            egui::RichText::new(track.artist.clone())
                                .font(crate::theme::font(11.0))
                                .color(theme.text_dim),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let stamp = format!(
                                "{} · {}",
                                relative_time(*played_at),
                                format_time(*listened)
                            );
                            ui.label(
                                egui::RichText::new(stamp)
                                    .font(crate::theme::mono_font(10.5))
                                    .color(theme.text_muted),
                            );
                        });
                    });
                }
            });
        },
    );
    !closed
}

/// The most-skipped tracks, which is the other half of the story.
pub fn show_most_skipped(ui: &mut egui::Ui, tracks: &[domain::models::Track], theme: &Theme) {
    if tracks.is_empty() {
        return;
    }
    ui.label(
        egui::RichText::new("Most skipped")
            .font(crate::theme::font(12.5))
            .strong()
            .color(theme.text_secondary),
    );
    ui.add_space(theme.space_xxs);
    for track in tracks.iter().take(8) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
            ui.label(
                egui::RichText::new(track.title.clone())
                    .font(crate::theme::font(12.0))
                    .color(theme.text_primary),
            );
            ui.label(
                egui::RichText::new(track.artist.clone())
                    .font(crate::theme::font(11.0))
                    .color(theme.text_dim),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{}× skipped", track.skip_count))
                        .font(crate::theme::mono_font(10.5))
                        .color(theme.accent_warm),
                );
            });
        });
    }
}

/// "3 minutes ago", "yesterday", or a date.
fn relative_time(unix_secs: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let ago = now - unix_secs;
    if ago < 60 {
        "just now".to_string()
    } else if ago < 3600 {
        format!("{} min ago", ago / 60)
    } else if ago < 86_400 {
        format!("{} h ago", ago / 3600)
    } else if ago < 172_800 {
        "yesterday".to_string()
    } else {
        format!("{} days ago", ago / 86_400)
    }
}

/// `m:ss` of listening time.
fn format_time(secs: f32) -> String {
    let total = secs.max(0.0) as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

/// A hairline for the log's separators.
pub fn hairline(theme: &Theme) -> egui::Stroke {
    egui::Stroke::new(1.0, theme.border_subtle)
}

/// Unused-import guard.
const _: fn() = || {
    let _ = Sense::hover();
};
