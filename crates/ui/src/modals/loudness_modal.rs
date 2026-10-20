//! The loudness inspector.
//!
//! Answers the question an audiophile actually asks about a file: *how
//! loud is it, and how does that compare with the rest of the album?* It
//! shows the track's integrated loudness against the album's spread, the
//! EBU R128 and ReplayGain targets, and the true peak — the numbers that
//! decide whether a track will sound quiet next to its neighbours or clip
//! the DAC.

use crate::theme::Theme;
use domain::models::Track;
use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

/// The lowest integrated loudness the scale shows.
const SCALE_MIN: f32 = -36.0;
/// The highest integrated loudness the scale shows.
const SCALE_MAX: f32 = 0.0;
/// EBU R128's broadcast target.
const TARGET_R128: f32 = -14.0;
/// ReplayGain 2.0's reference level.
const TARGET_REPLAYGAIN: f32 = -18.0;

/// The loudness inspector's state.
#[derive(Default)]
pub struct LoudnessModalState {
    pub is_open: bool,
    /// The tracks of the current track's album, captured when the inspector
    /// opens, so the album's spread can be shown.
    pub album_tracks: Vec<Track>,
}

/// Show the loudness inspector.
pub fn show_loudness_modal(
    ctx: &egui::Context,
    state: &mut LoudnessModalState,
    track: Option<&Track>,
    theme: &Theme,
) {
    if !state.is_open {
        return;
    }
    let Some(track) = track else {
        state.is_open = false;
        return;
    };

    let closed = crate::dialog::show_modal(
        ctx,
        "loudness_inspector",
        "Loudness Inspector",
        520.0,
        theme,
        |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_lg);

            ui.label(
                egui::RichText::new(track.title.clone())
                    .font(crate::theme::font(15.0))
                    .strong()
                    .color(theme.text_primary),
            );
            ui.label(
                egui::RichText::new(if track.album.is_empty() {
                    track.artist.clone()
                } else {
                    format!("{} — {}", track.artist, track.album)
                })
                .font(crate::theme::font(12.0))
                .color(theme.text_muted),
            );

            // ── The scale ──────────────────────────────────────────────────
            let lufs = track.r128_loudness_lufs;
            let album_lufs: Vec<f32> = state
                .album_tracks
                .iter()
                .filter_map(|t| t.r128_loudness_lufs)
                .collect();

            let scale_h = 54.0;
            let (scale_rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), scale_h), Sense::hover());
            let painter = ui.painter_at(scale_rect);

            // The bar itself.
            let bar = Rect::from_min_size(
                Pos2::new(scale_rect.min.x, scale_rect.center().y - 7.0),
                Vec2::new(scale_rect.width(), 14.0),
            );
            painter.rect_filled(bar, CornerRadius::same(theme.radius_sm), theme.bg_pill);
            painter.rect_stroke(
                bar,
                CornerRadius::same(theme.radius_sm),
                Stroke::new(1.0, theme.border_subtle),
                egui::StrokeKind::Inside,
            );

            let to_x = |db: f32| -> f32 {
                let t = ((db - SCALE_MIN) / (SCALE_MAX - SCALE_MIN)).clamp(0.0, 1.0);
                bar.min.x + t * bar.width()
            };

            // The album's spread, if it is known.
            if album_lufs.len() > 1 {
                let lo = album_lufs.iter().cloned().fold(f32::MAX, f32::min);
                let hi = album_lufs.iter().cloned().fold(f32::MIN, f32::max);
                let band = Rect::from_min_max(
                    Pos2::new(to_x(lo), bar.min.y + 1.0),
                    Pos2::new(to_x(hi), bar.max.y - 1.0),
                );
                painter.rect_filled(band, CornerRadius::same(3), theme.info.gamma_multiply(0.3));
            }

            // The two reference targets.
            for (target, label, color) in [
                (TARGET_REPLAYGAIN, "RG", theme.info),
                (TARGET_R128, "R128", theme.accent_highlight),
            ] {
                let x = to_x(target);
                painter.vline(
                    x,
                    bar.min.y - 5.0..=bar.max.y + 5.0,
                    Stroke::new(1.5, color.gamma_multiply(0.8)),
                );
                painter.text(
                    Pos2::new(x, bar.max.y + 12.0),
                    egui::Align2::CENTER_TOP,
                    label,
                    crate::theme::mono_font(9.0),
                    color,
                );
            }

            // The track itself.
            if let Some(db) = lufs {
                let x = to_x(db);
                painter.circle_filled(Pos2::new(x, bar.center().y), 6.0, theme.accent);
                painter.circle_stroke(
                    Pos2::new(x, bar.center().y),
                    6.0,
                    Stroke::new(1.5, theme.bg_card),
                );
            }

            // The scale's own ends.
            for (db, label) in [(SCALE_MIN, "-36"), (SCALE_MAX, "0")] {
                painter.text(
                    Pos2::new(to_x(db), bar.max.y + 12.0),
                    egui::Align2::CENTER_TOP,
                    label,
                    crate::theme::mono_font(9.0),
                    theme.text_dim,
                );
            }
            painter.text(
                Pos2::new(bar.min.x, bar.min.y - 14.0),
                egui::Align2::LEFT_BOTTOM,
                "integrated loudness, LUFS",
                crate::theme::font(10.0),
                theme.text_dim,
            );

            // ── The numbers ────────────────────────────────────────────────
            ui.add_space(4.0);
            egui::Grid::new("loudness_grid")
                .num_columns(2)
                .spacing(Vec2::new(theme.space_2xl, theme.space_sm))
                .show(ui, |ui| {
                    let mut row = |label: &str, value: String| {
                        ui.label(
                            egui::RichText::new(label)
                                .font(crate::theme::font(12.0))
                                .color(theme.text_muted),
                        );
                        ui.label(
                            egui::RichText::new(value)
                                .font(crate::theme::mono_font(12.0))
                                .color(theme.text_primary),
                        );
                        ui.end_row();
                    };
                    row(
                        "Integrated loudness",
                        lufs.map(|v| format!("{v:.1} LUFS"))
                            .unwrap_or_else(|| "not measured".into()),
                    );
                    row(
                        "True peak",
                        track
                            .replaygain_track_peak
                            .map(|v| format!("{:.4} ({:.2} dBTP)", v, 20.0 * v.max(1e-9).log10()))
                            .unwrap_or_else(|| "not measured".into()),
                    );
                    row(
                        "ReplayGain, track",
                        track
                            .replaygain_track_db
                            .map(|v| format!("{v:+.2} dB"))
                            .unwrap_or_else(|| "not measured".into()),
                    );
                    row(
                        "ReplayGain, album",
                        track
                            .replaygain_album_db
                            .map(|v| format!("{v:+.2} dB"))
                            .unwrap_or_else(|| "not measured".into()),
                    );
                    if album_lufs.len() > 1 {
                        let lo = album_lufs.iter().cloned().fold(f32::MAX, f32::min);
                        let hi = album_lufs.iter().cloned().fold(f32::MIN, f32::max);
                        row(
                            "Album spread",
                            format!("{lo:.1} to {hi:.1} LUFS ({} tracks)", album_lufs.len()),
                        );
                    }
                });

            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(
                    "The band is the rest of the album. A track far to the left of \
                     its neighbours will sound quiet; one far to the right will be \
                     the loud thing in the room.",
                )
                .font(crate::theme::font(11.0))
                .color(theme.text_dim),
            );
        },
    );
    if closed {
        state.is_open = false;
    }
}
