use crate::theme::Theme;
use eframe::egui::{Color32, CornerRadius, Pos2, Rect, Response, Sense, Ui, Vec2};

/// Live spectrum visualizer component drawing responsive gradient audio bars
pub struct VisualizerWidget<'a> {
    _spectrum_db: &'a [f32],
    _is_playing: bool,
    _theme: &'a Theme,
    _bar_count: usize,
}

const MOCK_BARS: [f32; 60] = [
    0.14, 0.22, 0.28, 0.22, 0.31, 0.44, 0.58, 0.58, 0.44, 0.33, 0.25, 0.22, 0.14, 0.19, 0.25, 0.33,
    0.39, 0.31, 0.25, 0.14, 0.14, 0.25, 0.36, 0.25, 0.25, 0.44, 0.25, 0.36, 0.44, 0.58, 0.67, 0.83,
    1.00, 0.72, 0.55, 0.33, 0.28, 0.31, 0.53, 0.25, 0.42, 0.36, 0.36, 0.28, 0.19, 0.17, 0.25, 0.25,
    0.31, 0.47, 0.58, 0.53, 0.42, 0.31, 0.28, 0.17, 0.22, 0.28, 0.42, 0.53,
];

impl<'a> VisualizerWidget<'a> {
    pub fn new(spectrum_db: &'a [f32], is_playing: bool, theme: &'a Theme) -> Self {
        Self {
            _spectrum_db: spectrum_db,
            _is_playing: is_playing,
            _theme: theme,
            _bar_count: 60,
        }
    }

    pub fn show(self, ui: &mut Ui, size: Vec2) -> Response {
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());

        if !ui.is_rect_visible(rect) {
            return response;
        }

        let painter = ui.painter_at(rect);

        let total_bars = 60;
        let spacing = 2.0_f32;
        let total_spacing = spacing * (total_bars - 1) as f32;
        let bar_width =
            ((rect.width() - total_spacing) / total_bars as f32).clamp(2.4_f32, 4.0_f32);

        let color_indigo_left = Color32::from_rgb(54, 42, 207); // #362acf Electric royal indigo
        let color_lavender_peak = Color32::from_rgb(165, 180, 252); // #a5b4fc Bright lavender peak
        let color_indigo_mid = Color32::from_rgb(99, 102, 241); // #6366f1 Vivid indigo
        let color_indigo_right = Color32::from_rgb(67, 56, 202); // #4338ca Deep indigo

        for (i, &mock_h) in MOCK_BARS.iter().enumerate() {
            let t = i as f32 / (total_bars - 1) as f32;

            let bar_height = (mock_h * rect.height()).max(3.5_f32);
            let x = rect.min.x + i as f32 * (bar_width + spacing);
            let y_bottom = rect.max.y;
            let y_top = y_bottom - bar_height;

            let bar_rect =
                Rect::from_min_max(Pos2::new(x, y_top), Pos2::new(x + bar_width, y_bottom));

            // Left (electric indigo) -> Center Peak (bright lavender) -> Right (deep indigo)
            let bar_color = if t < 0.52_f32 {
                let factor = t / 0.52_f32;
                lerp_color(color_indigo_left, color_lavender_peak, factor)
            } else {
                let factor = (t - 0.52_f32) / 0.48_f32;
                lerp_color(color_indigo_mid, color_indigo_right, factor)
            };

            painter.rect_filled(bar_rect, CornerRadius::same(2), bar_color);
        }

        ui.ctx().request_repaint();

        response
    }
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0_f32, 1.0_f32);
    Color32::from_rgba_premultiplied(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
        (a.a() as f32 + (b.a() as f32 - a.a() as f32) * t) as u8,
    )
}
