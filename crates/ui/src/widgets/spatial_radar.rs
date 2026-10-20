use crate::theme::Theme;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

/// Interactive 2D/3D Soundstage and Spatial Radar Canvas
pub struct SpatialRadarWidget<'a> {
    /// Azimuth of the virtual centre, in degrees. Dragging the source node
    /// writes this: it is the only spatial placement the engine exposes.
    pub center_azimuth_deg: &'a mut f32,
    /// Where the listener's head is facing, in degrees. Independent of the
    /// centre.
    pub listener_yaw_deg: f32,
    pub room_width_m: f32,
    pub room_depth_m: f32,
    pub theme: &'a Theme,
    pub size: Vec2,
}

pub struct SpatialRadarResponse {
    pub changed: bool,
}

impl<'a> SpatialRadarWidget<'a> {
    pub fn new(center_azimuth_deg: &'a mut f32, listener_yaw_deg: f32, theme: &'a Theme) -> Self {
        Self {
            center_azimuth_deg,
            listener_yaw_deg,
            room_width_m: 6.0,
            room_depth_m: 8.0,
            theme,
            size: Vec2::new(320.0, 260.0),
        }
    }

    pub fn room_dimensions(mut self, w: f32, d: f32) -> Self {
        self.room_width_m = w;
        self.room_depth_m = d;
        self
    }

    pub fn size(mut self, s: Vec2) -> Self {
        self.size = s;
        self
    }

    pub fn show(self, ui: &mut egui::Ui) -> SpatialRadarResponse {
        let (rect, response) = ui.allocate_exact_size(self.size, egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        let center = rect.center();
        let mut changed = false;

        // Background radar card
        painter.rect_filled(
            rect,
            12.0,
            Color32::from_rgba_premultiplied(10, 12, 22, 240),
        );
        painter.rect_stroke(
            rect,
            12.0,
            Stroke::new(1.0_f32, self.theme.border_subtle),
            egui::StrokeKind::Inside,
        );

        // Max distance represented by radar edge (e.g. 5 meters)
        let max_dist = 5.0_f32;
        let radar_radius = (rect.width().min(rect.height()) / 2.0) - 20.0;
        let scale = radar_radius / max_dist;

        // 1. Draw Virtual Room Outline
        let room_w_px = (self.room_width_m * scale).min(rect.width() - 20.0);
        let room_d_px = (self.room_depth_m * scale).min(rect.height() - 20.0);
        let room_rect = Rect::from_center_size(center, Vec2::new(room_w_px, room_d_px));
        painter.rect_stroke(
            room_rect,
            4.0,
            Stroke::new(1.0_f32, Color32::from_rgba_premultiplied(100, 116, 139, 40)),
            egui::StrokeKind::Inside,
        );
        painter.text(
            Pos2::new(room_rect.left() + 6.0, room_rect.top() + 4.0),
            egui::Align2::LEFT_TOP,
            format!(
                "Virtual Room: {:.1}m × {:.1}m",
                self.room_width_m, self.room_depth_m
            ),
            crate::theme::font(9.5),
            self.theme.text_muted,
        );

        // 2. Draw Distance Concentric Rings
        let grid_stroke = Stroke::new(1.0_f32, Color32::from_rgba_premultiplied(255, 255, 255, 15));
        for &dist in &[1.0, 2.0, 3.0, 4.0] {
            let r = dist * scale;
            if r <= radar_radius {
                painter.circle_stroke(center, r, grid_stroke);
                painter.text(
                    Pos2::new(center.x + 4.0, center.y - r + 2.0),
                    egui::Align2::LEFT_TOP,
                    format!("{:.0}m", dist),
                    crate::theme::font(8.5),
                    self.theme.text_muted,
                );
            }
        }

        // 3. Draw Crosshair / Angle Lines (Ahead, Left, Right, Behind)
        painter.line_segment(
            [
                Pos2::new(center.x, rect.top() + 15.0),
                Pos2::new(center.x, rect.bottom() - 15.0),
            ],
            grid_stroke,
        );
        painter.line_segment(
            [
                Pos2::new(rect.left() + 15.0, center.y),
                Pos2::new(rect.right() - 15.0, center.y),
            ],
            grid_stroke,
        );

        // 4. Draw Listener Head (Center)
        // Listener body circle
        painter.circle_filled(center, 9.0, Color32::from_rgb(45, 55, 75));
        painter.circle_stroke(center, 9.0, Stroke::new(1.5_f32, self.theme.accent));

        // Listener nose / direction pointer
        let yaw_rad = (self.listener_yaw_deg - 90.0).to_radians();
        let nose_tip = Pos2::new(
            center.x + 14.0 * yaw_rad.cos(),
            center.y + 14.0 * yaw_rad.sin(),
        );
        painter.line_segment([center, nose_tip], Stroke::new(2.5_f32, self.theme.accent));

        // 5. Handle Dragging of the Virtual Centre
        let mouse_pos = ui.input(|i| i.pointer.hover_pos());
        if response.dragged() {
            if let Some(mpos) = mouse_pos {
                let dx = mpos.x - center.x;
                let dy = mpos.y - center.y;

                // Azimuth in degrees: 0° is ahead (negative Y), 90° right, -90° left
                let angle_rad = dy.atan2(dx);
                let mut deg = angle_rad.to_degrees() + 90.0;
                if deg > 180.0 {
                    deg -= 360.0;
                }

                *self.center_azimuth_deg = deg;
                changed = true;
            }
        }

        // 6. Draw the Virtual Centre Node
        let src_az_rad = (*self.center_azimuth_deg - 90.0).to_radians();
        let src_dist_px = 3.0 * scale;
        let source_pos = Pos2::new(
            center.x + src_dist_px * src_az_rad.cos(),
            center.y + src_dist_px * src_az_rad.sin(),
        );

        // Ray line from listener to source
        painter.line_segment(
            [center, source_pos],
            Stroke::new(
                1.0_f32,
                Color32::from_rgba_unmultiplied(
                    self.theme.accent_highlight.r(),
                    self.theme.accent_highlight.g(),
                    self.theme.accent_highlight.b(),
                    80,
                ),
            ),
        );

        // Pulsing glow around sound source
        painter.circle_filled(
            source_pos,
            12.0,
            Color32::from_rgba_unmultiplied(
                self.theme.accent_highlight.r(),
                self.theme.accent_highlight.g(),
                self.theme.accent_highlight.b(),
                40,
            ),
        );
        painter.circle_filled(source_pos, 7.0, self.theme.accent_highlight);
        painter.circle_stroke(source_pos, 7.0, Stroke::new(1.5_f32, Color32::WHITE));

        // Source Label & Telemetry
        let source_info = format!("Centre {:+.0}°", *self.center_azimuth_deg);
        painter.text(
            Pos2::new(source_pos.x, source_pos.y - 12.0),
            egui::Align2::CENTER_BOTTOM,
            source_info,
            crate::theme::font(10.0),
            self.theme.text_primary,
        );

        SpatialRadarResponse { changed }
    }
}
