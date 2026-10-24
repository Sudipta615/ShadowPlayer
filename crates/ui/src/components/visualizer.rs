use crate::theme::Theme;
use eframe::egui::{self, CornerRadius, Pos2, Rect, Response, Sense, Vec2};

/// Curated static soundwave equalizer silhouette.
/// Models a natural audio waveform with acoustic peaks in the bass, vocal mids,
/// and upper registers, with elegant valleys in between.
const STATIC_WAVE_SILHOUETTE: [f32; 48] = [
    0.20, 0.28, 0.42, 0.58, 0.72, 0.82, 0.65, 0.48, 0.38, 0.52, 0.68, 0.84, 0.94, 0.88, 0.70, 0.54,
    0.40, 0.35, 0.50, 0.68, 0.85, 0.98, 0.92, 0.78, 0.62, 0.48, 0.42, 0.56, 0.74, 0.88, 0.90, 0.76,
    0.58, 0.44, 0.38, 0.52, 0.66, 0.78, 0.70, 0.55, 0.42, 0.34, 0.46, 0.58, 0.48, 0.36, 0.26, 0.18,
];

/// Completely static, zero-CPU soundwave visualizer used by the Now Playing card.
///
/// Rather than running expensive real-time FFT frequency binning or per-frame
/// continuous animations (which consume CPU and battery on lower-end devices),
/// this widget paints a static, balanced soundwave silhouette.
///
/// It schedules ZERO repaints and performs zero runtime allocations, strictly
/// guaranteeing 0% CPU consumption during playback and idle.
pub struct VisualizerWidget<'a> {
    is_playing: bool,
    theme: &'a Theme,
}

impl<'a> VisualizerWidget<'a> {
    pub fn new(is_playing: bool, theme: &'a Theme) -> Self {
        Self { is_playing, theme }
    }

    pub fn show(self, ui: &mut egui::Ui, size: Vec2) -> Response {
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());

        if !ui.is_rect_visible(rect) {
            return response;
        }

        let painter = ui.painter_at(rect);

        // Layout parameters
        let bar_width = 3.5_f32;
        let spacing = 2.5_f32;
        let step = bar_width + spacing;
        let num_bars = ((rect.width() / step) as usize).clamp(16, 48);

        let total_width =
            num_bars as f32 * bar_width + (num_bars.saturating_sub(1)) as f32 * spacing;
        let start_x = rect.left() + (rect.width() - total_width).max(0.0) * 0.5;
        let baseline = rect.bottom() - 2.0;
        let max_height = (rect.height() - 4.0).max(6.0);
        let min_height = 2.5_f32;
        let corner = CornerRadius::same(self.theme.radius_xs);

        if !self.is_playing {
            // Idle / Paused state: Resting low-profile pills in muted color.
            let rest_color = self.theme.text_dim.gamma_multiply(0.35);
            for i in 0..num_bars {
                let norm_idx = (i * STATIC_WAVE_SILHOUETTE.len()) / num_bars.max(1);
                let factor = STATIC_WAVE_SILHOUETTE[norm_idx.min(STATIC_WAVE_SILHOUETTE.len() - 1)];
                let bar_h = min_height + factor * 0.25 * (max_height - min_height);
                let x = start_x + i as f32 * step;
                let bar = Rect::from_min_max(
                    Pos2::new(x, baseline - bar_h),
                    Pos2::new(x + bar_width, baseline),
                );
                painter.rect_filled(bar, corner, rest_color);
            }
            return response;
        }

        // Active Playing state: Static, crisp soundwave silhouette in theme accent colors.
        // No animation, no timer math, no repaint requests -> strictly 0% CPU.
        for i in 0..num_bars {
            let pos = i as f32 / num_bars.max(1) as f32; // normalized position [0.0, 1.0]
            let norm_idx = (i * STATIC_WAVE_SILHOUETTE.len()) / num_bars.max(1);
            let factor = STATIC_WAVE_SILHOUETTE[norm_idx.min(STATIC_WAVE_SILHOUETTE.len() - 1)];

            let bar_height = min_height + factor * (max_height - min_height);
            let x = start_x + i as f32 * step;
            let bar = Rect::from_min_max(
                Pos2::new(x, baseline - bar_height),
                Pos2::new(x + bar_width, baseline),
            );

            let color = if pos < 0.68 {
                self.theme.accent
            } else {
                self.theme.accent_highlight
            };

            painter.rect_filled(bar, corner, color);

            // Subtle top cap highlight on peak bars for visual depth
            if bar_height > 10.0 {
                let cap_h = (bar_width * 0.6).min(bar_height * 0.25);
                let cap = Rect::from_min_max(
                    Pos2::new(x, bar.min.y),
                    Pos2::new(x + bar_width, bar.min.y + cap_h),
                );
                painter.rect_filled(cap, corner, color.gamma_multiply(1.25));
            }
        }

        // IMPORTANT: No ui.ctx().request_repaint() or request_repaint_after()!
        // This ensures strictly 0% CPU consumption.

        response
    }
}
