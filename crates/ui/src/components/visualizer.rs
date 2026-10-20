use crate::theme::Theme;
use eframe::egui::{self, CornerRadius, Pos2, Rect, Response, Sense, Vec2};

/// Compact live spectrum strip used by the Now Playing card.
///
/// The old implementation ignored `spectrum_db` entirely and drew a hard-coded
/// mock array, then requested a repaint every frame even while paused. This
/// version maps the real FFT bins logarithmically and only schedules animation
/// while audio is playing.
pub struct VisualizerWidget<'a> {
    spectrum_db: &'a [f32],
    is_playing: bool,
    theme: &'a Theme,
}

impl<'a> VisualizerWidget<'a> {
    pub fn new(spectrum_db: &'a [f32], is_playing: bool, theme: &'a Theme) -> Self {
        Self {
            spectrum_db,
            is_playing,
            theme,
        }
    }

    pub fn show(self, ui: &mut egui::Ui, size: Vec2) -> Response {
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());

        if !ui.is_rect_visible(rect) {
            return response;
        }

        let painter = ui.painter_at(rect);
        let num_bars = ((rect.width() / 9.0) as usize).clamp(24, 64);
        let spacing = 2.0_f32;
        let bar_width = ((rect.width() - spacing * (num_bars.saturating_sub(1)) as f32)
            / num_bars as f32)
            .max(1.5);
        let baseline = rect.bottom() - 2.0;

        if self.spectrum_db.is_empty() {
            // An empty library should look quiet, not broken.
            for i in 0..num_bars {
                let x = rect.left() + i as f32 * (bar_width + spacing);
                let bar = Rect::from_min_max(
                    Pos2::new(x, baseline - 2.0),
                    Pos2::new(x + bar_width, baseline),
                );
                painter.rect_filled(
                    bar,
                    CornerRadius::same(1),
                    self.theme.text_dim.gamma_multiply(0.45),
                );
            }
            return response;
        }

        let spec_len = self.spectrum_db.len();
        for i in 0..num_bars {
            let t0 = i as f32 / num_bars as f32;
            let t1 = (i + 1) as f32 / num_bars as f32;
            let bin_start =
                ((t0.powf(2.2) * (spec_len.saturating_sub(1)) as f32) as usize).min(spec_len - 1);
            let bin_end = ((t1.powf(2.2) * (spec_len.saturating_sub(1)) as f32) as usize)
                .clamp(bin_start + 1, spec_len);

            let mut max_db = -120.0_f32;
            for &db in &self.spectrum_db[bin_start..bin_end] {
                if db > max_db {
                    max_db = db;
                }
            }

            // -60 dB is the visual floor; anything quieter is a hairline.
            let norm = ((max_db + 60.0) / 60.0).clamp(0.015, 1.0);
            // Paused audio keeps a dim, frozen silhouette instead of a bright
            // spectrum that would imply something is still playing.
            let energy = if self.is_playing { 1.0 } else { 0.22 };
            let bar_height = (norm * energy * (rect.height() - 4.0)).max(2.0);
            let x = rect.left() + i as f32 * (bar_width + spacing);
            let bar = Rect::from_min_max(
                Pos2::new(x, baseline - bar_height),
                Pos2::new(x + bar_width, baseline),
            );

            // A disciplined jade/mint intensity ramp, not a rainbow.
            let color = if t0 < 0.68 {
                self.theme.accent
            } else {
                self.theme.accent_highlight
            };
            let color = if self.is_playing {
                color
            } else {
                color.gamma_multiply(0.55)
            };
            painter.rect_filled(bar, CornerRadius::same(1), color);

            if bar_height > 10.0 {
                let cap = Rect::from_min_max(
                    Pos2::new(x, bar.min.y),
                    Pos2::new(x + bar_width, bar.min.y + 1.5),
                );
                painter.rect_filled(cap, CornerRadius::same(1), color.gamma_multiply(1.3));
            }
        }

        if self.is_playing {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(33));
        }

        response
    }
}
