//! Library statistics.
//!
//! The library already knows everything this needs — how many tracks it
//! holds, what they are made of, how loud they are, when they arrived and
//! how often they are played — but it never showed any of it. A player that
//! can tell you that half your library is MP3 and that you have not played
//! anything in three months feels like it knows your collection.

use crate::theme::Theme;
use domain::models::Track;
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

/// Everything the statistics view shows, computed once when it opens.
#[derive(Debug, Clone, Default)]
pub struct LibraryStats {
    pub tracks: usize,
    pub albums: usize,
    pub artists: usize,
    pub genres: usize,
    pub total_bytes: u64,
    pub total_secs: f32,
    /// Track count per format, most common first.
    pub formats: Vec<(String, usize)>,
    /// Track count per sample rate, ascending.
    pub sample_rates: Vec<(u32, usize)>,
    pub lossless: usize,
    pub hires: usize,
    /// Tracks added within the last 30 days.
    pub added_recently: usize,
    /// The most-played tracks, most played first.
    pub top_played: Vec<(String, String, u32)>,
    /// How many tracks have never been played.
    pub never_played: usize,
    /// The total number of plays.
    pub total_plays: u32,
    /// The tracks that are skipped most often.
    pub most_skipped: Vec<(String, u32)>,
}

impl LibraryStats {
    /// Summarise a slice of tracks.
    pub fn from_tracks(tracks: &[Track]) -> Self {
        use std::collections::{BTreeMap, HashSet};

        let mut stats = LibraryStats {
            tracks: tracks.len(),
            ..Default::default()
        };

        let mut albums: HashSet<(String, String)> = HashSet::new();
        let mut artists: HashSet<String> = HashSet::new();
        let mut genres: HashSet<String> = HashSet::new();
        let mut formats: BTreeMap<String, usize> = BTreeMap::new();
        let mut rates: BTreeMap<u32, usize> = BTreeMap::new();

        // 30 days before now, in the same units as `added_at`.
        let cutoff = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
            - 30 * 24 * 60 * 60;

        for track in tracks {
            stats.total_bytes += track.file_size_bytes;
            stats.total_secs += track.duration_secs;
            stats.total_plays += track.play_count;
            if track.play_count == 0 {
                stats.never_played += 1;
            }
            if track.added_at >= cutoff {
                stats.added_recently += 1;
            }
            if !track.album.is_empty() {
                albums.insert((track.album.clone(), track.artist.clone()));
            }
            if !track.artist.is_empty() {
                artists.insert(track.artist.clone());
            }
            if let Some(g) = &track.genre {
                if !g.is_empty() {
                    genres.insert(g.clone());
                }
            }
            *formats.entry(track.format.clone()).or_default() += 1;
            *rates.entry(track.sample_rate).or_default() += 1;
            if track.is_lossless {
                stats.lossless += 1;
            }
            if track.is_hires() {
                stats.hires += 1;
            }
        }

        stats.albums = albums.len();
        stats.artists = artists.len();
        stats.genres = genres.len();

        let mut format_counts: Vec<(String, usize)> = formats.into_iter().collect();
        format_counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        stats.formats = format_counts;
        stats.sample_rates = rates.into_iter().collect();

        let mut played: Vec<(String, String, u32)> = tracks
            .iter()
            .filter(|t| t.play_count > 0)
            .map(|t| (t.title.clone(), t.artist.clone(), t.play_count))
            .collect();
        played.sort_by_key(|a| std::cmp::Reverse(a.2));
        played.truncate(5);
        stats.top_played = played;

        let mut skipped: Vec<(String, u32)> = tracks
            .iter()
            .filter(|t| t.skip_count > 0)
            .map(|t| (t.title.clone(), t.skip_count))
            .collect();
        skipped.sort_by_key(|a| std::cmp::Reverse(a.1));
        skipped.truncate(5);
        stats.most_skipped = skipped;

        stats
    }

    /// The library's total running time, in words.
    pub fn total_duration_text(&self) -> String {
        let total = self.total_secs as u64;
        let days = total / 86_400;
        let hours = (total % 86_400) / 3_600;
        let minutes = (total % 3_600) / 60;
        if days > 0 {
            format!("{days} d {hours} h")
        } else if hours > 0 {
            format!("{hours} h {minutes} min")
        } else {
            format!("{minutes} min")
        }
    }
}

/// The statistics view's state.
#[derive(Default)]
pub struct LibraryStatsModalState {
    pub is_open: bool,
    /// The summary, captured when the view opens so the library is not walked
    /// on every frame.
    pub stats: LibraryStats,
}

/// Show the library statistics.
pub fn show_library_stats_modal(
    ctx: &egui::Context,
    state: &mut LibraryStatsModalState,
    theme: &Theme,
) {
    if !state.is_open {
        return;
    }
    let stats = state.stats.clone();

    let closed = crate::dialog::show_modal_scrollable(
        ctx,
        "library_stats",
        "Library Statistics",
        560.0,
        theme,
        |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_lg);

            // ── The headline numbers ───────────────────────────────────────
            let tiles: [(&str, String); 6] = [
                ("Tracks", stats.tracks.to_string()),
                ("Albums", stats.albums.to_string()),
                ("Artists", stats.artists.to_string()),
                ("Genres", stats.genres.to_string()),
                ("Running time", stats.total_duration_text()),
                (
                    "On disk",
                    format!(
                        "{:.1} GiB",
                        stats.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
                    ),
                ),
            ];
            // A wrapped row of fixed-width tiles, rather than a grid: the
            // grid squeezed its columns to nothing when the dialog was narrow.
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, theme.space_sm);
                for (label, value) in tiles {
                    stat_tile(ui, label, &value, theme, 152.0);
                }
            });

            // ── Formats ────────────────────────────────────────────────────
            section_heading(ui, "Formats", theme);
            if stats.formats.is_empty() {
                ui.label(
                    egui::RichText::new("No tracks yet.")
                        .font(crate::theme::font(12.0))
                        .color(theme.text_muted),
                );
            } else {
                // A stacked bar, then the breakdown.
                let total = stats.tracks.max(1) as f32;
                let (bar_rect, _) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 14.0), Sense::hover());
                let painter = ui.painter_at(bar_rect);
                let mut x = bar_rect.min.x;
                let palette = [
                    theme.accent,
                    theme.accent_highlight,
                    theme.info,
                    theme.accent_warm,
                    theme.bit_perfect_green,
                    theme.text_muted,
                ];
                for (i, (_, count)) in stats.formats.iter().enumerate() {
                    let count = *count;
                    let w = bar_rect.width() * (count as f32 / total);
                    let color = palette[i % palette.len()];
                    painter.rect_filled(
                        Rect::from_min_size(Pos2::new(x, bar_rect.min.y), Vec2::new(w, 14.0)),
                        CornerRadius::same(theme.radius_xs),
                        color,
                    );
                    x += w;
                }
                ui.add_space(4.0);
                for (i, (format, count)) in stats.formats.iter().enumerate() {
                    let color = palette[i % palette.len()];
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                        let (dot, _) = ui.allocate_exact_size(Vec2::splat(9.0), Sense::hover());
                        ui.painter_at(dot).circle_filled(dot.center(), 4.0, color);
                        ui.label(
                            egui::RichText::new(format.clone())
                                .font(crate::theme::font(12.0))
                                .color(theme.text_secondary),
                        );
                        ui.label(
                            egui::RichText::new(format!("{count}"))
                                .font(crate::theme::mono_font(11.5))
                                .color(theme.text_muted),
                        );
                    });
                }
            }

            // ── Resolution ─────────────────────────────────────────────────
            section_heading(ui, "Sample rate", theme);
            for (rate, count) in &stats.sample_rates {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                    let label = if *rate % 1000 == 0 {
                        format!("{} kHz", rate / 1000)
                    } else {
                        format!("{:.1} kHz", *rate as f32 / 1000.0)
                    };
                    let is_hires = *rate > 48_000;
                    ui.label(
                        egui::RichText::new(label)
                            .font(crate::theme::font(12.0))
                            .color(if is_hires {
                                theme.bit_perfect_green
                            } else {
                                theme.text_secondary
                            }),
                    );
                    ui.label(
                        egui::RichText::new(format!("{count}"))
                            .font(crate::theme::mono_font(11.5))
                            .color(theme.text_muted),
                    );
                    if is_hires {
                        ui.label(
                            egui::RichText::new("Hi-Res")
                                .font(crate::theme::font(10.0))
                                .strong()
                                .color(theme.bit_perfect_green),
                        );
                    }
                });
            }

            // ── Listening ──────────────────────────────────────────────────
            section_heading(ui, "Listening", theme);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);
                for (label, value) in [
                    ("Plays", stats.total_plays.to_string()),
                    ("Never played", stats.never_played.to_string()),
                    ("Added this month", stats.added_recently.to_string()),
                ] {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(value)
                                .font(crate::theme::font(15.0))
                                .strong()
                                .color(theme.text_primary),
                        );
                        ui.label(
                            egui::RichText::new(label)
                                .font(crate::theme::font(11.0))
                                .color(theme.text_muted),
                        );
                    });
                }
            });

            // ── The most skipped ────────────────────────────────────────────
            if !stats.most_skipped.is_empty() {
                section_heading(ui, "Most skipped", theme);
                for (title, count) in &stats.most_skipped {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                        ui.label(
                            egui::RichText::new(title.clone())
                                .font(crate::theme::font(12.0))
                                .color(theme.text_primary),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(format!("{count}× skipped"))
                                    .font(crate::theme::mono_font(11.5))
                                    .color(theme.accent_warm),
                            );
                        });
                    });
                }
            }

            // ── The most played ────────────────────────────────────────────
            if !stats.top_played.is_empty() {
                section_heading(ui, "Most played", theme);
                for (title, artist, plays) in &stats.top_played {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                        ui.label(
                            egui::RichText::new(title.clone())
                                .font(crate::theme::font(12.0))
                                .color(theme.text_primary),
                        );
                        ui.label(
                            egui::RichText::new(artist.clone())
                                .font(crate::theme::font(11.5))
                                .color(theme.text_dim),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(format!("{plays}×"))
                                    .font(crate::theme::mono_font(11.5))
                                    .color(theme.text_muted),
                            );
                        });
                    });
                }
            }
        },
    );
    if closed {
        state.is_open = false;
    }
}

/// One headline number, in a small card of a fixed width.
fn stat_tile(ui: &mut egui::Ui, label: &str, value: &str, theme: &Theme, width: f32) {
    egui::Frame::NONE
        .fill(theme.bg_card_hover)
        .corner_radius(CornerRadius::same(theme.radius_md))
        .inner_margin(egui::Margin::symmetric(
            theme.space_xl as i8,
            theme.space_md as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(width - 24.0);
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
        });
}

/// A section heading.
fn section_heading(ui: &mut egui::Ui, title: &str, theme: &Theme) {
    ui.add_space(theme.space_xs);
    ui.label(
        egui::RichText::new(title)
            .font(crate::theme::font(12.5))
            .strong()
            .color(theme.text_secondary),
    );
    ui.add_space(theme.space_xxs);
}

/// A colour for one of the format slices.
pub fn format_color(index: usize, theme: &Theme) -> Color32 {
    const PALETTE_LEN: usize = 6;
    let palette = [
        theme.accent,
        theme.accent_highlight,
        theme.info,
        theme.accent_warm,
        theme.bit_perfect_green,
        theme.text_muted,
    ];
    palette[index % PALETTE_LEN]
}

/// A hairline for the statistics view's own separators.
pub fn hairline(theme: &Theme) -> Stroke {
    Stroke::new(1.0, theme.border_subtle)
}

#[cfg(test)]
mod tests {
    use super::LibraryStats;
    use domain::ids::TrackId;
    use domain::models::Track;
    use std::path::PathBuf;

    /// The fields of a fixture track that vary between the cases.
    struct Spec {
        title: &'static str,
        artist: &'static str,
        album: &'static str,
        format: &'static str,
        rate: u32,
        lossless: bool,
        plays: u32,
        added: i64,
    }

    fn track(s: &Spec) -> Track {
        let Spec {
            title,
            artist,
            album,
            format,
            rate,
            lossless,
            plays,
            added,
        } = *s;
        Track {
            id: TrackId::new(),
            path: PathBuf::from(format!("/m/{title}.{format}")),
            title: title.into(),
            artist: artist.into(),
            album: album.into(),
            genre: Some("Ambient".into()),
            year: Some(2020),
            track_number: Some(1),
            disc_number: None,
            duration_secs: 240.0,
            sample_rate: rate,
            channels: 2,
            format: format.into(),
            bitrate_kbps: Some(1000),
            bit_depth: Some(24),
            is_lossless: lossless,
            replaygain_track_db: None,
            replaygain_track_peak: None,
            replaygain_album_db: None,
            replaygain_album_peak: None,
            r128_loudness_lufs: None,
            file_size_bytes: 50_000_000,
            mtime_unix_secs: 0,
            mtime_nanos: 0,
            content_hash: None,
            file_id: None,
            is_favorite: false,
            rating: 0,
            has_lyrics: false,
            play_count: plays,
            skip_count: 0,
            last_played_at: None,
            added_at: added,
        }
    }

    #[test]
    fn counts_the_library() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let tracks = vec![
            track(&Spec {
                title: "A",
                artist: "One",
                album: "Album X",
                format: "FLAC",
                rate: 96000,
                lossless: true,
                plays: 5,
                added: now - 10,
            }),
            track(&Spec {
                title: "B",
                artist: "One",
                album: "Album X",
                format: "FLAC",
                rate: 44100,
                lossless: true,
                plays: 0,
                added: now - 10_000_000,
            }),
            track(&Spec {
                title: "C",
                artist: "Two",
                album: "Album Y",
                format: "MP3",
                rate: 44100,
                lossless: false,
                plays: 3,
                added: now - 20,
            }),
        ];

        let stats = LibraryStats::from_tracks(&tracks);
        assert_eq!(stats.tracks, 3);
        assert_eq!(stats.albums, 2);
        assert_eq!(stats.artists, 2);
        assert_eq!(stats.genres, 1);
        assert_eq!(stats.lossless, 2);
        assert_eq!(stats.hires, 1, "only the 96 kHz track is Hi-Res");
        assert_eq!(stats.never_played, 1);
        assert_eq!(stats.total_plays, 8);
        assert_eq!(stats.added_recently, 2, "the two recent additions");
        assert_eq!(
            stats.formats[0],
            ("FLAC".to_string(), 2),
            "the most common format first"
        );
        assert_eq!(stats.top_played[0].2, 5, "the most-played track first");
        assert_eq!(stats.total_duration_text(), "12 min");
    }

    #[test]
    fn an_empty_library_does_not_divide_by_zero() {
        let stats = LibraryStats::from_tracks(&[]);
        assert_eq!(stats.tracks, 0);
        assert_eq!(stats.total_duration_text(), "0 min");
        assert!(stats.formats.is_empty());
    }
}
