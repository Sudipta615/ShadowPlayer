//! A seek bar that draws the track's cached waveform.
//!
//! The bar is the same control whether or not peaks have arrived: before the
//! first decode it falls back to the plain progress line, then paints the
//! envelope in place once [`crate::services::WaveformService`] has it. Clicking
//! and dragging work identically in both states.

use crate::theme::Theme;
use eframe::egui::{self, CornerRadius, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

pub struct WaveformSeek<'a> {
    /// Normalised 0.0–1.0 peaks. Empty while the decode is in flight.
    pub peaks: Option<&'a [f32]>,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub height: f32,
    /// Fixed width. `None` fills the remaining row.
    pub width: Option<f32>,
    pub theme: &'a Theme,
}

impl<'a> WaveformSeek<'a> {
    /// Show the bar. The returned fraction is `Some` only on the frame the
    /// user clicked or dragged it.
    pub fn show(self, ui: &mut Ui) -> (Response, Option<f32>) {
        let width = self.width.unwrap_or_else(|| ui.available_width()).max(40.0);
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(width, self.height), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        let mid = rect.center().y;

        let fraction = if self.duration_secs > 0.0 {
            (self.position_secs / self.duration_secs).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        response.widget_info(|| {
            egui::WidgetInfo::slider(true, f64::from(fraction), "Playback position")
        });

        // A quiet baseline, so an empty/loading waveform still reads as a
        // control rather than dead space.
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(rect.min.x, mid - 1.0),
                Pos2::new(rect.max.x, mid + 1.0),
            ),
            CornerRadius::same(1),
            self.theme.slider_track,
        );

        if let Some(peaks) = self.peaks.filter(|peaks| !peaks.is_empty()) {
            let columns = ((rect.width() / 3.0).floor() as usize).max(1);
            let bar_w = rect.width() / columns as f32;
            let progress_x = rect.min.x + rect.width() * fraction;
            for column in 0..columns {
                let from = column * peaks.len() / columns;
                let to = ((column + 1) * peaks.len() / columns).max(from + 1);
                let peak = peaks[from..to.min(peaks.len())]
                    .iter()
                    .copied()
                    .fold(0.0f32, f32::max)
                    .clamp(0.0, 1.0);
                // Keep a visible floor: a silent passage should look quiet,
                // not vanish into the baseline.
                let half = (peak * (self.height * 0.46)).max(0.8);
                let x = rect.min.x + column as f32 * bar_w;
                let colour = if x <= progress_x {
                    self.theme.accent_highlight
                } else {
                    self.theme.text_dim.gamma_multiply(0.55)
                };
                painter.rect_filled(
                    Rect::from_min_max(
                        Pos2::new(x, mid - half),
                        Pos2::new(x + (bar_w - 1.0).max(1.0), mid + half),
                    ),
                    CornerRadius::same(1),
                    colour,
                );
            }

            // Played overlay: a thin line above the envelope, so the position
            // is readable even where the waveform is tall.
            painter.line_segment(
                [
                    Pos2::new(rect.min.x, rect.min.y + 1.5),
                    Pos2::new(progress_x, rect.min.y + 1.5),
                ],
                Stroke::new(1.5_f32, self.theme.accent),
            );
        } else {
            let progress_x = rect.min.x + rect.width() * fraction;
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(rect.min.x, mid - 2.0),
                    Pos2::new(progress_x, mid + 2.0),
                ),
                CornerRadius::same(2),
                self.theme.accent_highlight,
            );
        }

        if response.hovered() || response.dragged() {
            let thumb_x = rect.min.x + rect.width() * fraction;
            painter.circle_filled(Pos2::new(thumb_x, mid), 5.0, self.theme.text_primary);
            painter.circle_stroke(
                Pos2::new(thumb_x, mid),
                5.0,
                Stroke::new(1.5_f32, self.theme.bg_card),
            );
        }

        let seek = if response.dragged() || response.clicked() {
            response
                .interact_pointer_pos()
                .map(|pointer| ((pointer.x - rect.min.x) / rect.width().max(1.0)).clamp(0.0, 1.0))
        } else {
            None
        };
        response
            .widget_info(|| egui::WidgetInfo::slider(true, fraction as f64, "Playback position"));
        (response, seek)
    }
}

/// A small helper for the panels that only need a coloured waveform strip.
pub fn paint_peaks(painter: &egui::Painter, rect: Rect, peaks: &[f32], played: f32, theme: &Theme) {
    if peaks.is_empty() || rect.width() <= 0.0 {
        return;
    }
    let mid = rect.center().y;
    let columns = ((rect.width() / 3.0).floor() as usize).max(1);
    let bar_w = rect.width() / columns as f32;
    for column in 0..columns {
        let from = column * peaks.len() / columns;
        let to = ((column + 1) * peaks.len() / columns).max(from + 1);
        let peak = peaks[from..to.min(peaks.len())]
            .iter()
            .copied()
            .fold(0.0f32, f32::max)
            .clamp(0.0, 1.0);
        let half = (peak * (rect.height() * 0.45)).max(0.6);
        let x = rect.min.x + column as f32 * bar_w;
        let colour = if column as f32 / columns as f32 <= played {
            theme.accent_highlight
        } else {
            theme.text_dim.gamma_multiply(0.5)
        };
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(x, mid - half),
                Pos2::new(x + (bar_w - 1.0).max(1.0), mid + half),
            ),
            CornerRadius::same(1),
            colour,
        );
    }
}
