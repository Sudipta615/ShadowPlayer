use crate::theme::Theme;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

/// Dual Analog Ballistic Needle VU Meter Widget
pub struct VuMeterWidget<'a> {
    pub peak_db_l: f32,
    pub peak_db_r: f32,
    pub true_peak_dbtp: f32,
    pub theme: &'a Theme,
    pub height: f32,
}

impl<'a> VuMeterWidget<'a> {
    pub fn new(peak_db_l: f32, peak_db_r: f32, true_peak_dbtp: f32, theme: &'a Theme) -> Self {
        Self {
            peak_db_l,
            peak_db_r,
            true_peak_dbtp,
            theme,
            height: 90.0,
        }
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    pub fn show(self, ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), self.height),
            egui::Sense::hover(),
        );

        let painter = ui.painter_at(rect);

        // Dark chassis frame
        painter.rect_filled(rect, 8.0, self.theme.meter_bg);
        painter.rect_stroke(
            rect,
            8.0,
            Stroke::new(1.0_f32, self.theme.border_subtle),
            egui::StrokeKind::Inside,
        );

        let gap = 12.0_f32;
        let meter_w = ((rect.width() - 24.0 - gap) / 2.0).max(120.0);
        let meter_h = rect.height() - 16.0;

        // Render Left Meter
        let l_rect = Rect::from_min_size(
            Pos2::new(rect.left() + 12.0, rect.top() + 8.0),
            Vec2::new(meter_w, meter_h),
        );
        Self::draw_analog_dial(
            &painter,
            l_rect,
            "LEFT",
            self.peak_db_l,
            self.true_peak_dbtp > 0.0,
            self.theme,
        );

        // Render Right Meter
        let r_rect = Rect::from_min_size(
            Pos2::new(l_rect.right() + gap, rect.top() + 8.0),
            Vec2::new(meter_w, meter_h),
        );
        Self::draw_analog_dial(
            &painter,
            r_rect,
            "RIGHT",
            self.peak_db_r,
            self.true_peak_dbtp > 0.0,
            self.theme,
        );
    }

    fn draw_analog_dial(
        painter: &egui::Painter,
        rect: Rect,
        channel: &str,
        level_db: f32,
        peak_overload: bool,
        theme: &Theme,
    ) {
        // Dial Face (Warm retro incandescent glass or deep dark cyber)
        painter.rect_filled(rect, 6.0, theme.meter_gutter);
        painter.rect_stroke(
            rect,
            6.0,
            Stroke::new(1.0_f32, theme.meter_grid),
            egui::StrokeKind::Inside,
        );

        let pivot = Pos2::new(rect.center().x, rect.bottom() + 12.0);
        let dial_radius = rect.height() * 1.15;

        // Arc angles: -50 deg (left, -20 VU) to +50 deg (right, +3 VU)
        let min_angle_deg = -46.0_f32;
        let max_angle_deg = 46.0_f32;
        let zero_vu_angle_deg = 20.0_f32; // 0 VU mark position

        // Scale ticks: -20, -10, -7, -5, -3, -1, 0, +1, +2, +3
        let ticks: &[(f32, &str)] = &[
            (-20.0, "-20"),
            (-10.0, "-10"),
            (-7.0, "-7"),
            (-5.0, "-5"),
            (-3.0, "-3"),
            (-1.0, "-1"),
            (0.0, "0"),
            (1.0, "+1"),
            (2.0, "+2"),
            (3.0, "+3"),
        ];

        let db_to_angle = |db: f32| -> f32 {
            let clamped = db.clamp(-20.0, 3.0);
            if clamped <= 0.0 {
                // -20 to 0 maps from min_angle to zero_vu_angle
                let t = (clamped + 20.0) / 20.0;
                min_angle_deg + t * (zero_vu_angle_deg - min_angle_deg)
            } else {
                // 0 to +3 maps from zero_vu_angle to max_angle
                let t = clamped / 3.0;
                zero_vu_angle_deg + t * (max_angle_deg - zero_vu_angle_deg)
            }
        };

        // Draw scale ticks and numbers
        for &(db, label) in ticks {
            let deg = db_to_angle(db);
            let rad = (deg - 90.0).to_radians();

            let r_inner = dial_radius - 12.0;
            let r_outer = dial_radius - 4.0;

            let p0 = Pos2::new(pivot.x + r_inner * rad.cos(), pivot.y + r_inner * rad.sin());
            let p1 = Pos2::new(pivot.x + r_outer * rad.cos(), pivot.y + r_outer * rad.sin());

            let tick_col = if db >= 0.0 {
                theme.destructive // Red overload zone
            } else {
                theme.text_muted
            };

            painter.line_segment([p0, p1], Stroke::new(1.2_f32, tick_col));

            // Numbers on major ticks
            if db == -20.0 || db == -10.0 || db == -5.0 || db == 0.0 || db == 3.0 {
                let p_txt = Pos2::new(
                    pivot.x + (dial_radius - 20.0) * rad.cos(),
                    pivot.y + (dial_radius - 20.0) * rad.sin(),
                );
                painter.text(
                    p_txt,
                    egui::Align2::CENTER_CENTER,
                    label,
                    crate::theme::font(8.5),
                    tick_col,
                );
            }
        }

        // Needle Ballistics & Rendering
        let needle_db = (level_db + 20.0).clamp(-20.0, 3.0); // Offset to VU scale
        let needle_deg = db_to_angle(needle_db);
        let needle_rad = (needle_deg - 90.0).to_radians();
        let needle_tip = Pos2::new(
            pivot.x + (dial_radius - 6.0) * needle_rad.cos(),
            pivot.y + (dial_radius - 6.0) * needle_rad.sin(),
        );

        // Needle shadow
        let shadow_tip = Pos2::new(needle_tip.x + 1.5, needle_tip.y + 1.5);
        painter.line_segment(
            [pivot, shadow_tip],
            Stroke::new(1.5_f32, theme.border_subtle),
        );

        // Needle line
        let needle_color = if needle_db >= 0.0 {
            theme.destructive
        } else {
            theme.accent_highlight
        };
        painter.line_segment([pivot, needle_tip], Stroke::new(1.5_f32, needle_color));

        // Pivot cap
        painter.circle_filled(pivot, 4.0, theme.text_dim);

        // Channel Label & VU branding
        painter.text(
            Pos2::new(rect.left() + 8.0, rect.top() + 6.0),
            egui::Align2::LEFT_TOP,
            format!("{} VU", channel),
            crate::theme::font(9.0),
            theme.text_secondary,
        );

        // Peak Overload LED
        let led_pos = Pos2::new(rect.right() - 10.0, rect.top() + 10.0);
        let (led_fill, led_glow) = if peak_overload || needle_db >= 0.0 {
            (theme.destructive, theme.destructive.gamma_multiply(0.35))
        } else {
            (theme.destructive.gamma_multiply(0.25), Color32::TRANSPARENT)
        };

        if peak_overload || needle_db >= 0.0 {
            painter.circle_filled(led_pos, 5.5, led_glow);
        }
        painter.circle_filled(led_pos, 3.0, led_fill);
    }
}
