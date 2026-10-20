use crate::theme::Theme;
use eframe::egui::{self, CornerRadius, Pos2, Rect, Stroke, Vec2};

/// High-Resolution 60fps FFT Spectrum Visualizer Widget
pub struct SpectrumVisualizerWidget<'a> {
    pub spectrum_db: &'a [f32],
    pub height: f32,
    pub bar_count: usize,
    pub peak_decay: bool,
    pub theme: &'a Theme,
}

impl<'a> SpectrumVisualizerWidget<'a> {
    pub fn new(spectrum_db: &'a [f32], theme: &'a Theme) -> Self {
        Self {
            spectrum_db,
            height: 90.0,
            bar_count: 64,
            peak_decay: true,
            theme,
        }
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    pub fn bar_count(mut self, count: usize) -> Self {
        self.bar_count = count;
        self
    }

    pub fn show(self, ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), self.height),
            egui::Sense::hover(),
        );

        let painter = ui.painter_at(rect);

        // Dark background
        painter.rect_filled(rect, 8.0, self.theme.bg_card);
        painter.rect_stroke(
            rect,
            8.0,
            Stroke::new(1.0_f32, self.theme.border_subtle),
            egui::StrokeKind::Inside,
        );

        if self.spectrum_db.is_empty() {
            // Idle placeholder line
            let mid_y = rect.center().y;
            painter.line_segment(
                [
                    Pos2::new(rect.left() + 10.0, mid_y),
                    Pos2::new(rect.right() - 10.0, mid_y),
                ],
                Stroke::new(1.0_f32, self.theme.meter_grid),
            );
            return;
        }

        let num_bars = self.bar_count.clamp(16, 128);
        let bar_spacing = 2.0_f32;
        let total_spacing = bar_spacing * (num_bars as f32 - 1.0);
        let bar_width = ((rect.width() - 16.0 - total_spacing) / num_bars as f32).max(1.5);

        let spec_len = self.spectrum_db.len();

        for i in 0..num_bars {
            // Map bar index logarithmic frequency to spectrum bin index
            let t = (i as f32) / (num_bars as f32);
            // Logarithmic mapping: lower bars get fewer FFT bins, higher bars group more bins
            let bin_start = ((t.powf(2.2) * (spec_len as f32 - 1.0)) as usize).min(spec_len - 1);
            let bin_end = ((((i + 1) as f32 / num_bars as f32).powf(2.2) * (spec_len as f32 - 1.0))
                as usize)
                .clamp(bin_start + 1, spec_len);

            let mut max_db = -120.0_f32;
            for b in bin_start..bin_end {
                if b < spec_len && self.spectrum_db[b] > max_db {
                    max_db = self.spectrum_db[b];
                }
            }

            // Normalize dB: from -60 dB (floor) to 0 dB (ceiling)
            let norm = ((max_db + 60.0) / 60.0).clamp(0.02, 1.0);
            let bar_h = (norm * (rect.height() - 12.0)).max(2.0);

            let x0 = rect.left() + 8.0 + (i as f32) * (bar_width + bar_spacing);
            let x1 = x0 + bar_width;
            let y1 = rect.bottom() - 6.0;
            let y0 = y1 - bar_h;

            let bar_rect = Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1));

            // Jade at the low/mid bands, mint at the highs. No violet ramp.
            let color = if t < 0.4 {
                self.theme.info
            } else if t < 0.75 {
                self.theme.accent
            } else {
                self.theme.accent_highlight
            };

            // Glow cap
            painter.rect_filled(bar_rect, CornerRadius::same(2), color);

            // Subtle peak cap dot
            let peak_y = (y0 - 2.5).max(rect.top() + 4.0);
            painter.line_segment(
                [Pos2::new(x0, peak_y), Pos2::new(x1, peak_y)],
                Stroke::new(1.2_f32, self.theme.text_primary),
            );
        }
    }
}
