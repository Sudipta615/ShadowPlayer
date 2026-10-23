//! Podcast subscriptions, separate from the music library.
//!
//! The view is a thin projection of [`PodcastState`]: feeds on the left, the
//! selected feed's episodes on the right. Network and download work happen in
//! [`crate::services::PodcastService`]; every action here is a request the app
//! carries out after the frame.

use crate::state::PodcastState;
use crate::theme::Theme;
use domain::ids::{PodcastEpisodeId, PodcastFeedId};
use domain::models::PodcastEpisode;
use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Stroke, Ui, Vec2};

#[derive(Default)]
pub struct PodcastsViewAction {
    pub add_feed: Option<String>,
    pub select_feed: Option<PodcastFeedId>,
    pub refresh_feed: Option<PodcastFeedId>,
    pub delete_feed: Option<PodcastFeedId>,
    pub download_episode: Option<PodcastEpisodeId>,
    pub play_episode: Option<PodcastEpisodeId>,
    /// Resume or pause the episode already loaded in the transport.
    pub toggle_playback: bool,
    /// Seek the podcast transport, in seconds.
    pub seek_to: Option<f32>,
}

pub struct PodcastsView<'a> {
    pub state: &'a mut PodcastState,
    /// A worker is fetching or downloading.
    pub busy: bool,
    /// Human-readable worker status, empty when idle.
    pub status: &'a str,
    pub theme: &'a Theme,
}

impl<'a> PodcastsView<'a> {
    pub fn show(self, ui: &mut Ui) -> PodcastsViewAction {
        let mut action = PodcastsViewAction::default();
        let PodcastsView {
            state,
            busy,
            status,
            theme,
        } = self;

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(tr!("Podcasts"))
                        .font(crate::theme::font(22.0))
                        .strong()
                        .color(theme.text_primary),
                );
                ui.label(
                    egui::RichText::new(tr!(
                        "Subscriptions live outside the music library. Downloads and resume positions are kept per episode."
                    ))
                    .font(crate::theme::font(11.5))
                    .color(theme.text_muted),
                );
            });
        });
        ui.add_space(6.0);

        // ── Add a feed ────────────────────────────────────────────────────
        egui::Frame::NONE
            .fill(theme.bg_card)
            .stroke(Stroke::new(1.0_f32, theme.border_card))
            .corner_radius(CornerRadius::same(theme.radius_lg))
            .inner_margin(egui::Margin::symmetric(
                theme.space_2xl as i8,
                theme.space_lg as i8,
            ))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(tr!("Add by RSS or Atom URL"))
                            .font(crate::theme::font(12.0))
                            .color(theme.text_secondary),
                    );
                    let field_w = (ui.available_width() - 120.0).max(160.0);
                    ui.add_sized(
                        Vec2::new(field_w, 28.0),
                        egui::TextEdit::singleline(&mut state.new_feed_url)
                            .hint_text("https://example.com/feed.xml")
                            .font(crate::theme::font(12.0)),
                    );
                    let add = ui.add_enabled(
                        !busy && !state.new_feed_url.trim().is_empty(),
                        egui::Button::new(tr!("Subscribe"))
                            .fill(theme.accent)
                            .corner_radius(CornerRadius::same(theme.radius_md)),
                    );
                    if add.clicked() {
                        action.add_feed = Some(state.new_feed_url.trim().to_string());
                    }
                });
                if !status.is_empty() {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(status)
                            .font(crate::theme::font(11.0))
                            .color(theme.info),
                    );
                }
                if let Some(error) = &state.error {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(error)
                            .font(crate::theme::font(11.0))
                            .color(theme.destructive),
                    );
                }
            });

        ui.add_space(8.0);

        if state.feeds.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);
                ui.label(
                    egui::RichText::new(tr!("No podcast subscriptions yet"))
                        .font(crate::theme::font(14.0))
                        .color(theme.text_muted),
                );
                ui.label(
                    egui::RichText::new(tr!(
                        "Paste a feed URL above. Episodes can be streamed after download and resume where you stopped."
                    ))
                    .font(crate::theme::font(11.5))
                    .color(theme.text_dim),
                );
            });
            return action;
        }

        ui.horizontal_top(|ui| {
            // ── Feed list ─────────────────────────────────────────────────
            let list_w = ui.available_width().min(240.0);
            egui::Frame::NONE
                .fill(theme.bg_card)
                .stroke(Stroke::new(1.0_f32, theme.border_card))
                .corner_radius(CornerRadius::same(theme.radius_lg))
                .inner_margin(egui::Margin::same(theme.space_sm as i8))
                .show(ui, |ui| {
                    ui.set_width(list_w - theme.space_sm * 2.0);
                    egui::ScrollArea::vertical()
                        .id_salt("podcast_feed_list")
                        .max_height(ui.available_height().max(160.0))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for feed in &state.feeds {
                                let selected = state.selected_feed == Some(feed.id);
                                let fill = if selected {
                                    theme.bg_card_active
                                } else {
                                    theme.bg_card
                                };
                                egui::Frame::NONE
                                    .fill(fill)
                                    .corner_radius(CornerRadius::same(theme.radius_md))
                                    .inner_margin(egui::Margin::symmetric(6, 5))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        let resp = ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(&feed.title)
                                                    .font(crate::theme::font(12.5))
                                                    .strong()
                                                    .color(if selected {
                                                        theme.accent_highlight
                                                    } else {
                                                        theme.text_primary
                                                    }),
                                            )
                                            .sense(Sense::click())
                                            .truncate(),
                                        );
                                        if resp.clicked() {
                                            action.select_feed = Some(feed.id);
                                        }
                                        let count = if feed.id
                                            == state.selected_feed.unwrap_or(feed.id)
                                            && !state.episodes.is_empty()
                                        {
                                            Some(state.episodes.len())
                                        } else {
                                            None
                                        };
                                        ui.horizontal(|ui| {
                                            if let Some(count) = count {
                                                ui.label(
                                                    egui::RichText::new(tr_args!(
                                                        "{count} episodes",
                                                        "count" => count
                                                    ))
                                                    .font(crate::theme::font(10.0))
                                                    .color(theme.text_dim),
                                                );
                                            }
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    if ui
                                                        .small_button("×")
                                                        .on_hover_text(tr!(
                                                            "Unsubscribe and forget this feed"
                                                        ))
                                                        .clicked()
                                                    {
                                                        action.delete_feed = Some(feed.id);
                                                    }
                                                    if ui
                                                        .small_button("↻")
                                                        .on_hover_text(tr!("Refresh feed"))
                                                        .clicked()
                                                    {
                                                        action.refresh_feed = Some(feed.id);
                                                    }
                                                },
                                            );
                                        });
                                    });
                                ui.add_space(2.0);
                            }
                        });
                });

            ui.add_space(10.0);

            // ── Selected feed ─────────────────────────────────────────────
            ui.vertical(|ui| {
                if let Some(feed) = state.selected_feed() {
                    let title = feed.title.clone();
                    let description = feed.description.clone();
                    let url = feed.url.clone();
                    ui.label(
                        egui::RichText::new(title)
                            .font(crate::theme::font(17.0))
                            .strong()
                            .color(theme.text_primary),
                    );
                    ui.label(
                        egui::RichText::new(url)
                            .font(crate::theme::font(10.5))
                            .color(theme.text_dim),
                    );
                    if let Some(description) = description.filter(|d| !d.trim().is_empty()) {
                        ui.add_space(2.0);
                        ui.label(
                            egui::RichText::new(description)
                                .font(crate::theme::font(11.5))
                                .color(theme.text_secondary),
                        );
                    }
                }

                ui.add_space(6.0);

                // Transport strip for the episode currently loaded.
                if let Some(id) = state.playing_episode {
                    let episode = state
                        .episodes
                        .iter()
                        .find(|episode| episode.id == id)
                        .cloned()
                        .or_else(|| state.episode(id).cloned());
                    if let Some(episode) = episode {
                        egui::Frame::NONE
                            .fill(theme.bg_card)
                            .stroke(Stroke::new(1.0_f32, theme.accent))
                            .corner_radius(CornerRadius::same(theme.radius_md))
                            .inner_margin(egui::Margin::symmetric(10, 6))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(if state.is_playing {
                                            tr!("Playing")
                                        } else {
                                            tr!("Paused")
                                        })
                                        .font(crate::theme::font(10.5))
                                        .strong()
                                        .color(theme.accent_highlight),
                                    );
                                    ui.label(
                                        egui::RichText::new(&episode.title)
                                            .font(crate::theme::font(12.0))
                                            .color(theme.text_primary),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui
                                                .button(if state.is_playing {
                                                    tr!("Pause")
                                                } else {
                                                    tr!("Resume")
                                                })
                                                .clicked()
                                            {
                                                action.toggle_playback = true;
                                            }
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "{} / {}",
                                                    format_clock(state.position_secs),
                                                    format_clock(state.duration_secs)
                                                ))
                                                .font(crate::theme::font(10.5))
                                                .color(theme.text_muted),
                                            );
                                        },
                                    );
                                });
                                if state.duration_secs > 0.0 {
                                    let mut position = state.position_secs;
                                    let width = ui.available_width();
                                    let resp = ui.add_sized(
                                        Vec2::new(width, 14.0),
                                        egui::Slider::new(&mut position, 0.0..=state.duration_secs)
                                            .show_value(false),
                                    );
                                    if resp.changed() {
                                        action.seek_to = Some(position);
                                    }
                                }
                            });
                        ui.add_space(8.0);
                    }
                }

                egui::ScrollArea::vertical()
                    .id_salt("podcast_episode_list")
                    .auto_shrink([false, false])
                    .max_height(ui.available_height().max(200.0))
                    .show(ui, |ui| {
                        if state.episodes.is_empty() {
                            ui.add_space(24.0);
                            ui.label(
                                egui::RichText::new(tr!("This feed has no episodes."))
                                    .font(crate::theme::font(12.5))
                                    .color(theme.text_muted),
                            );
                            if ui.button(tr!("Refresh feed")).clicked() {
                                if let Some(feed) = state.selected_feed {
                                    action.refresh_feed = Some(feed);
                                }
                            }
                            return;
                        }

                        for episode in &state.episodes {
                            let is_current = state.playing_episode == Some(episode.id);
                            let progress = if is_current && state.duration_secs > 0.0 {
                                (state.position_secs / state.duration_secs).clamp(0.0, 1.0)
                            } else {
                                episode.progress_fraction()
                            };
                            egui::Frame::NONE
                                .fill(if is_current {
                                    theme.bg_card_active
                                } else {
                                    theme.bg_card
                                })
                                .stroke(Stroke::new(
                                    1.0_f32,
                                    if is_current {
                                        theme.accent
                                    } else {
                                        theme.border_card
                                    },
                                ))
                                .corner_radius(CornerRadius::same(theme.radius_md))
                                .inner_margin(egui::Margin::symmetric(10, 7))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label(
                                                egui::RichText::new(&episode.title)
                                                    .font(crate::theme::font(12.8))
                                                    .strong()
                                                    .color(if is_current {
                                                        theme.accent_highlight
                                                    } else {
                                                        theme.text_primary
                                                    }),
                                            );
                                            ui.label(
                                                egui::RichText::new(episode_meta(episode))
                                                    .font(crate::theme::font(10.5))
                                                    .color(theme.text_dim),
                                            );
                                        });
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if is_current {
                                                    if ui
                                                        .button(if state.is_playing {
                                                            tr!("Pause")
                                                        } else {
                                                            tr!("Resume")
                                                        })
                                                        .clicked()
                                                    {
                                                        action.toggle_playback = true;
                                                    }
                                                } else if episode.is_downloaded() {
                                                    if ui.button(tr!("Play")).clicked() {
                                                        action.play_episode = Some(episode.id);
                                                    }
                                                } else {
                                                    let enabled = !busy;
                                                    if ui
                                                        .add_enabled(
                                                            enabled,
                                                            egui::Button::new(tr!("Download")),
                                                        )
                                                        .clicked()
                                                    {
                                                        action.download_episode = Some(episode.id);
                                                    }
                                                }
                                            },
                                        );
                                    });

                                    if let Some(description) = episode
                                        .description
                                        .as_deref()
                                        .filter(|d| !d.trim().is_empty())
                                    {
                                        let clean = description.replace('\n', " ");
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(clean)
                                                    .font(crate::theme::font(10.8))
                                                    .color(theme.text_muted),
                                            )
                                            .truncate(),
                                        );
                                    }

                                    if progress > 0.0 {
                                        let (bar_rect, _) = ui.allocate_exact_size(
                                            Vec2::new(ui.available_width(), 4.0),
                                            Sense::hover(),
                                        );
                                        let p = ui.painter_at(bar_rect);
                                        p.rect_filled(
                                            bar_rect,
                                            CornerRadius::same(2),
                                            theme.bg_pill,
                                        );
                                        p.rect_filled(
                                            Rect::from_min_max(
                                                bar_rect.min,
                                                Pos2::new(
                                                    bar_rect.min.x + bar_rect.width() * progress,
                                                    bar_rect.max.y,
                                                ),
                                            ),
                                            CornerRadius::same(2),
                                            if is_current {
                                                theme.accent_highlight
                                            } else {
                                                theme.info
                                            },
                                        );
                                        ui.label(
                                            egui::RichText::new(if episode.completed {
                                                tr!("Completed · plays from the start")
                                            } else {
                                                tr_args!(
                                                    "{position} of {duration}",
                                                    "position" => format_clock(episode.position_secs),
                                                    "duration" => format_clock(
                                                        episode.duration_secs.unwrap_or(0.0)
                                                    )
                                                )
                                            })
                                            .font(crate::theme::font(9.8))
                                            .color(theme.text_dim),
                                        );
                                    }
                                });
                            ui.add_space(4.0);
                        }
                    });
            });
        });

        action
    }
}

fn episode_meta(episode: &PodcastEpisode) -> String {
    let mut parts = Vec::new();
    if let Some(ts) = episode.published_at {
        parts.push(format_date(ts));
    }
    if let Some(duration) = episode.duration_secs.filter(|d| *d > 0.0) {
        parts.push(format_clock(duration));
    }
    if episode.is_downloaded() {
        parts.push(tr!("Downloaded"));
    } else {
        parts.push(tr!("Not downloaded"));
    }
    parts.join(" · ")
}

fn format_clock(seconds: f32) -> String {
    let s = seconds.max(0.0) as u64;
    let hours = s / 3600;
    let mins = (s % 3600) / 60;
    let secs = s % 60;
    if hours > 0 {
        format!("{hours}:{mins:02}:{secs:02}")
    } else {
        format!("{mins}:{secs:02}")
    }
}

/// Unix seconds to `YYYY-MM-DD`, without pulling in a calendar crate.
fn format_date(timestamp: i64) -> String {
    let days = timestamp.div_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::{format_clock, format_date};

    #[test]
    fn dates_are_rendered_without_a_calendar_dependency() {
        assert_eq!(format_date(0), "1970-01-01");
        assert_eq!(format_date(1_735_732_800), "2025-01-01");
    }

    #[test]
    fn clocks_switch_to_hours_for_long_episodes() {
        assert_eq!(format_clock(59.0), "0:59");
        assert_eq!(format_clock(3_600.0), "1:00:00");
    }
}
