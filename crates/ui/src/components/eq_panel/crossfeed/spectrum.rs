//! Interactive Acoustic Transfer Function Spectrum canvas.

use super::types::biquad_crossfeed_gain_db;
use crate::components::eq_panel::EqPanelState;
use crate::theme::Theme;
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

/// Interactive Acoustic Transfer Function Spectrum canvas.
pub fn render_transfer_spectrum_canvas(ui: &mut egui::Ui, state: &EqPanelState, theme: &Theme) {
    let avail_w = ui.available_width();
    let canvas_h = 160.0;
    let (canvas_rect, resp) = ui.allocate_exact_size(Vec2::new(avail_w, canvas_h), Sense::hover());
    let painter = ui.painter_at(canvas_rect);

    painter.rect_filled(
        canvas_rect,
        CornerRadius::same(theme.radius_md),
        theme.bg_pill,
    );
    painter.rect_stroke(
        canvas_rect,
        CornerRadius::same(theme.radius_md),
        Stroke::new(1.0, theme.border_card),
        egui::StrokeKind::Inside,
    );

    let pad_left = 34.0;
    let pad_right = 14.0;
    let pad_top = 18.0;
    let pad_bottom = 22.0;

    let plot = Rect::from_min_max(
        Pos2::new(canvas_rect.left() + pad_left, canvas_rect.top() + pad_top),
        Pos2::new(
            canvas_rect.right() - pad_right,
            canvas_rect.bottom() - pad_bottom,
        ),
    );

    // Frequency mapping: 100 Hz to 20 kHz (logarithmic)
    let min_freq = 100.0_f32;
    let max_freq = 20000.0_f32;
    let log_min = min_freq.log10();
    let log_max = max_freq.log10();

    let x_for_freq = |f: f32| -> f32 {
        let t = (f.log10() - log_min) / (log_max - log_min);
        plot.left() + t.clamp(0.0, 1.0) * plot.width()
    };

    // dB mapping: -30 dB to +6 dB
    let min_db = -30.0_f32;
    let max_db = 6.0_f32;

    let y_for_db = |db: f32| -> f32 {
        let t = (db - min_db) / (max_db - min_db);
        plot.bottom() - t.clamp(0.0, 1.0) * plot.height()
    };

    // 1. Grid dB lines
    for &db in &[0.0, -6.0, -12.0, -18.0, -24.0] {
        let y = y_for_db(db);
        painter.line_segment(
            [Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
            Stroke::new(0.8, Color32::from_white_alpha(15)),
        );
        painter.text(
            Pos2::new(plot.left() - 4.0, y),
            egui::Align2::RIGHT_CENTER,
            format!("{db:+.0}"),
            crate::theme::font(9.0),
            theme.text_muted,
        );
    }

    // 2. Grid Frequency lines
    for &(freq, label) in &[
        (100.0, "100"),
        (250.0, "250"),
        (500.0, "500"),
        (1000.0, "1k"),
        (2500.0, "2.5k"),
        (5000.0, "5k"),
        (10000.0, "10k"),
        (20000.0, "20k"),
    ] {
        let x = x_for_freq(freq);
        painter.line_segment(
            [Pos2::new(x, plot.top()), Pos2::new(x, plot.bottom())],
            Stroke::new(0.8, Color32::from_white_alpha(15)),
        );
        painter.text(
            Pos2::new(x, plot.bottom() + 4.0),
            egui::Align2::CENTER_TOP,
            label,
            crate::theme::font(9.0),
            theme.text_muted,
        );
    }

    // 3. Direct Channel Flat Response Line (0 dB)
    let direct_y = y_for_db(0.0);
    painter.line_segment(
        [
            Pos2::new(plot.left(), direct_y),
            Pos2::new(plot.right(), direct_y),
        ],
        Stroke::new(1.5, theme.bit_perfect_green),
    );

    // 4. Contralateral Crossfeed Transfer Curve
    let steps = 64;
    let mut curve_pts: Vec<Pos2> = Vec::with_capacity(steps + 1);

    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let freq = 10.0_f32.powf(log_min + t * (log_max - log_min));
        let gain_db = biquad_crossfeed_gain_db(freq, state.crossfeed_freq, state.crossfeed_q);
        let x = x_for_freq(freq);
        let y = y_for_db(gain_db);
        curve_pts.push(Pos2::new(x, y));
    }

    // Draw shaded area under crossfeed curve
    for pair in curve_pts.windows(2) {
        let p0 = pair[0];
        let p1 = pair[1];
        let b0 = Pos2::new(p0.x, plot.bottom());
        let b1 = Pos2::new(p1.x, plot.bottom());

        let fill_color = Color32::from_rgba_unmultiplied(
            theme.accent.r(),
            theme.accent.g(),
            theme.accent.b(),
            30,
        );
        painter.add(egui::Shape::convex_polygon(
            vec![p0, p1, b1, b0],
            fill_color,
            Stroke::NONE,
        ));
        painter.line_segment([p0, p1], Stroke::new(2.0, theme.accent));
    }

    // 5. Cutoff Frequency Marker line
    let fc_x = x_for_freq(state.crossfeed_freq);
    painter.line_segment(
        [Pos2::new(fc_x, plot.top()), Pos2::new(fc_x, plot.bottom())],
        Stroke::new(1.0, theme.accent_warm),
    );
    painter.text(
        Pos2::new(fc_x, plot.top() + 4.0),
        egui::Align2::CENTER_TOP,
        format!("fc: {:.0} Hz", state.crossfeed_freq),
        crate::theme::font(9.5),
        theme.accent_warm,
    );

    // Legend in top-right
    painter.text(
        Pos2::new(plot.right() - 6.0, plot.top() + 4.0),
        egui::Align2::RIGHT_TOP,
        tr!("— Direct (0 dB)   — Crossfeed (Lowpass -12 dB/oct)"),
        crate::theme::font(9.0),
        theme.text_secondary,
    );

    if resp.hovered() {
        resp.on_hover_text(tr!(
            "Binaural transfer function: shows low frequencies diffracted across head (-6 dB blend) and steep -12 dB/oct attenuation of high frequencies above fc."
        ));
    }
}
