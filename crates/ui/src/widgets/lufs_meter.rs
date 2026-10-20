use crate::theme::Theme;
use eframe::egui::{self, CornerRadius, Pos2, Rect, Stroke, Vec2};
use engine::dsp::ProfessionalMeterSnapshot;

/// Broadcast-style loudness & true-peak meter.
///
/// The LUFS blocks are the engine's BS.1770-5-gated loudness. The dBTP block
/// is the engine's 4× FIR *estimate* of the Annex 2 true-peak definition —
/// a different prototype filter from the 48-tap table the recommendation
/// publishes (`engine::dsp::true_peak`) — and the widget says so on hover.
pub struct LufsMeterWidget<'a> {
    pub meters: Option<&'a ProfessionalMeterSnapshot>,
    pub theme: &'a Theme,
    pub height: f32,
}

impl<'a> LufsMeterWidget<'a> {
    pub fn new(meters: Option<&'a ProfessionalMeterSnapshot>, theme: &'a Theme) -> Self {
        Self {
            meters,
            theme,
            height: 90.0,
        }
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    pub fn show(self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), self.height),
            egui::Sense::hover(),
        );

        let painter = ui.painter_at(rect);

        // Frame
        painter.rect_filled(rect, 8.0, self.theme.meter_bg);
        painter.rect_stroke(
            rect,
            8.0,
            Stroke::new(1.0_f32, self.theme.border_subtle),
            egui::StrokeKind::Inside,
        );

        let default_meters = ProfessionalMeterSnapshot::default();
        let m = self.meters.unwrap_or(&default_meters);

        // Split into 4 metric blocks: Momentary, Short-Term, Integrated, True-Peak
        let block_w = (rect.width() - 32.0) / 4.0;
        let block_h = rect.height() - 16.0;
        let y_top = rect.top() + 8.0;

        let metrics = [
            ("MOMENTARY", m.lufs_momentary, "LUFS", -14.0_f32),
            ("SHORT-TERM", m.lufs_short_term, "LUFS", -14.0_f32),
            ("INTEGRATED", m.lufs_integrated, "LUFS", -14.0_f32),
            ("TRUE PEAK", m.max_true_peak_dbtp, "dBTP", 0.0_f32),
        ];

        for (idx, (label, val, unit, target)) in metrics.iter().enumerate() {
            let x_left = rect.left() + 16.0 + (idx as f32) * block_w;
            let b_rect =
                Rect::from_min_size(Pos2::new(x_left, y_top), Vec2::new(block_w - 8.0, block_h));

            // Metric container card
            painter.rect_filled(
                b_rect,
                CornerRadius::same(self.theme.radius_sm),
                self.theme.meter_gutter,
            );
            painter.rect_stroke(
                b_rect,
                CornerRadius::same(self.theme.radius_sm),
                Stroke::new(1.0_f32, self.theme.meter_grid),
                egui::StrokeKind::Inside,
            );

            // Label
            painter.text(
                Pos2::new(b_rect.left() + 8.0, b_rect.top() + 6.0),
                egui::Align2::LEFT_TOP,
                label,
                crate::theme::font(8.5),
                self.theme.text_muted,
            );

            // Large numeric readout
            let display_val = if *val <= -100.0 {
                "-∞".to_string()
            } else {
                format!("{:+.1}", val)
            };

            let val_color = if *label == "TRUE PEAK" {
                if *val > 0.0 {
                    self.theme.destructive
                } else {
                    self.theme.bit_perfect_green
                }
            } else if *val > *target {
                self.theme.accent_highlight
            } else {
                self.theme.info
            };

            painter.text(
                Pos2::new(b_rect.left() + 8.0, b_rect.top() + 20.0),
                egui::Align2::LEFT_TOP,
                display_val,
                crate::theme::font(15.0),
                val_color,
            );

            painter.text(
                Pos2::new(b_rect.right() - 8.0, b_rect.top() + 24.0),
                egui::Align2::RIGHT_TOP,
                unit,
                crate::theme::font(9.0),
                self.theme.text_secondary,
            );

            // Miniature horizontal level bar at the bottom of the card
            let bar_y = b_rect.bottom() - 10.0;
            let bar_w = b_rect.width() - 16.0;
            let bar_bg =
                Rect::from_min_size(Pos2::new(b_rect.left() + 8.0, bar_y), Vec2::new(bar_w, 4.0));
            painter.rect_filled(
                bar_bg,
                CornerRadius::same(self.theme.radius_xs),
                self.theme.meter_gutter,
            );

            // Level normalization (-40 to 0 dB)
            let norm = ((*val + 40.0) / 40.0).clamp(0.0, 1.0);
            let fill_w = bar_w * norm;
            let bar_fill = Rect::from_min_size(
                Pos2::new(b_rect.left() + 8.0, bar_y),
                Vec2::new(fill_w, 4.0),
            );
            painter.rect_filled(
                bar_fill,
                CornerRadius::same(self.theme.radius_xs),
                val_color,
            );
        }

        let _ = response.on_hover_text(
            "LUFS blocks: ITU-R BS.1770-5 gated loudness. dBTP: the engine's 4× FIR \
             estimate of the Annex 2 true-peak definition (engine prototype filter, \
             not the published 48-tap table).",
        );
    }
}
