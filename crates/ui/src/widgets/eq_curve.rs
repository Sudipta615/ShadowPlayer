use eframe::egui::{self, Color32, FontId, Pos2, Stroke, Vec2};
use engine::dsp::equalizer::EqFilterType;
use crate::components::eq_panel::ParametricBand;
use crate::theme::Theme;

/// Interactive 64-band Parametric Spline EQ curve widget
pub struct EqCurveWidget<'a> {
    pub bands: &'a mut [ParametricBand],
    pub preamp_db: f32,
    pub selected_band: Option<usize>,
    pub theme: &'a Theme,
    pub height: f32,
}

pub struct EqCurveResponse {
    pub selected_band: Option<usize>,
    pub band_changed: Option<usize>,
    pub changed: bool,
}

impl<'a> EqCurveWidget<'a> {
    pub fn new(bands: &'a mut [ParametricBand], preamp_db: f32, theme: &'a Theme) -> Self {
        Self {
            bands,
            preamp_db,
            selected_band: None,
            theme,
            height: 220.0,
        }
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    pub fn selected_band(mut self, sel: Option<usize>) -> Self {
        self.selected_band = sel;
        self
    }

    /// Render interactive EQ frequency response canvas
    pub fn show(self, ui: &mut egui::Ui) -> EqCurveResponse {
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), self.height),
            egui::Sense::click_and_drag(),
        );

        let painter = ui.painter_at(rect);
        let mut response_out = EqCurveResponse {
            selected_band: self.selected_band,
            band_changed: None,
            changed: false,
        };

        // Background card
        painter.rect_filled(
            rect,
            8.0,
            Color32::from_rgba_premultiplied(10, 11, 20, 240),
        );
        painter.rect_stroke(
            rect,
            8.0,
            Stroke::new(1.0_f32, self.theme.border_subtle),
            egui::StrokeKind::Inside,
        );

        // Coordinate transforms:
        // Frequency: 20 Hz (left) to 20,000 Hz (right) on logarithmic scale
        let f_min: f32 = 20.0;
        let f_max: f32 = 20_000.0;
        let log_f_min = f_min.ln();
        let log_f_max = f_max.ln();

        let freq_to_x = |f: f32| -> f32 {
            let clamped = f.clamp(f_min, f_max);
            let t = (clamped.ln() - log_f_min) / (log_f_max - log_f_min);
            rect.left() + t * rect.width()
        };

        let x_to_freq = |x: f32| -> f32 {
            let t = ((x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            (log_f_min + t * (log_f_max - log_f_min)).exp()
        };

        // Decibels: +18 dB (top) to -18 dB (bottom)
        let db_max: f32 = 18.0;
        let db_min: f32 = -18.0;

        let db_to_y = |db: f32| -> f32 {
            let clamped = db.clamp(db_min, db_max);
            let t = (db_max - clamped) / (db_max - db_min);
            rect.top() + t * rect.height()
        };

        let y_to_db = |y: f32| -> f32 {
            let t = ((y - rect.top()) / rect.height()).clamp(0.0, 1.0);
            db_max - t * (db_max - db_min)
        };

        // 1. Draw Grid Lines
        let grid_stroke = Stroke::new(1.0_f32, Color32::from_rgba_premultiplied(255, 255, 255, 12));
        let center_stroke = Stroke::new(1.2_f32, Color32::from_rgba_premultiplied(255, 255, 255, 30));

        // Horizontal dB lines
        for &db in &[-12.0, -6.0, 0.0, 6.0, 12.0] {
            let y = db_to_y(db);
            let stroke = if db == 0.0 { center_stroke } else { grid_stroke };
            painter.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], stroke);
            painter.text(
                Pos2::new(rect.left() + 6.0, y - 2.0),
                egui::Align2::LEFT_BOTTOM,
                format!("{:+0.0} dB", db),
                FontId::proportional(10.0),
                self.theme.text_muted,
            );
        }

        // Vertical Frequency lines
        let freq_marks: &[(f32, &str)] = &[
            (50.0, "50"),
            (100.0, "100"),
            (250.0, "250"),
            (500.0, "500"),
            (1000.0, "1k"),
            (2500.0, "2.5k"),
            (5000.0, "5k"),
            (10000.0, "10k"),
            (20000.0, "20k"),
        ];

        for &(f, label) in freq_marks {
            let x = freq_to_x(f);
            painter.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], grid_stroke);
            painter.text(
                Pos2::new(x, rect.bottom() - 4.0),
                egui::Align2::CENTER_BOTTOM,
                label,
                FontId::proportional(10.0),
                self.theme.text_muted,
            );
        }

        // 2. Compute Composite Frequency Response Curve (200 sample points)
        let num_eval_points = 200;
        let mut curve_points: Vec<Pos2> = Vec::with_capacity(num_eval_points);
        let mut fill_polygon: Vec<Pos2> = Vec::with_capacity(num_eval_points + 2);
        let zero_y = db_to_y(0.0);

        fill_polygon.push(Pos2::new(rect.left(), zero_y));

        for i in 0..num_eval_points {
            let t = i as f32 / (num_eval_points - 1) as f32;
            let f = (log_f_min + t * (log_f_max - log_f_min)).exp();
            let mut total_db = self.preamp_db;

            for band in self.bands.iter() {
                if band.enabled {
                    total_db += evaluate_band_gain_db(band, f, 48_000.0);
                }
            }

            let x = rect.left() + t * rect.width();
            let y = db_to_y(total_db);
            let pos = Pos2::new(x, y);
            curve_points.push(pos);
            fill_polygon.push(pos);
        }

        fill_polygon.push(Pos2::new(rect.right(), zero_y));

        // Fill area under curve
        painter.add(egui::Shape::convex_polygon(
            fill_polygon,
            Color32::from_rgba_premultiplied(168, 85, 247, 30),
            Stroke::NONE,
        ));

        // Draw glowing spline curve
        for window in curve_points.windows(2) {
            painter.line_segment([window[0], window[1]], Stroke::new(2.5_f32, self.theme.accent_violet));
        }

        // 3. Handle Drag & Interaction on Nodes
        let mouse_pos = ui.input(|i| i.pointer.hover_pos());
        let mut hovered_band: Option<usize> = None;

        // Check node hits
        if let Some(mpos) = mouse_pos {
            for (i, band) in self.bands.iter().enumerate() {
                let node_pos = Pos2::new(freq_to_x(band.freq), db_to_y(band.gain_db));
                if (mpos - node_pos).length() < 14.0 {
                    hovered_band = Some(i);
                    break;
                }
            }
        }

        // Active dragging
        if response.dragged() {
            if let Some(mpos) = mouse_pos {
                if let Some(sel) = response_out.selected_band.or(hovered_band) {
                    let new_freq = x_to_freq(mpos.x);
                    let new_gain = y_to_db(mpos.y);

                    self.bands[sel].freq = new_freq.clamp(20.0, 20_000.0);
                    self.bands[sel].gain_db = ((new_gain * 2.0).round() / 2.0).clamp(-18.0, 18.0);
                    self.bands[sel].enabled = true;

                    response_out.selected_band = Some(sel);
                    response_out.band_changed = Some(sel);
                    response_out.changed = true;
                }
            }
        } else if response.clicked() {
            if let Some(h) = hovered_band {
                response_out.selected_band = Some(h);
            }
        }

        // Scroll wheel adjusts Q on hovered/selected band
        let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll_delta.abs() > 0.1 {
            if let Some(target) = hovered_band.or(response_out.selected_band) {
                let q_change = if scroll_delta > 0.0 { 0.1 } else { -0.1 };
                self.bands[target].q = (self.bands[target].q + q_change).clamp(0.1, 10.0);
                response_out.band_changed = Some(target);
                response_out.changed = true;
            }
        }

        // 4. Draw Band Nodes & Badges
        for (i, band) in self.bands.iter().enumerate() {
            let node_x = freq_to_x(band.freq);
            let node_y = db_to_y(band.gain_db);
            let center = Pos2::new(node_x, node_y);

            let is_sel = response_out.selected_band == Some(i);
            let is_hov = hovered_band == Some(i);

            let node_radius = if is_sel { 8.5 } else if is_hov { 7.5 } else { 6.0 };
            let fill_color = if !band.enabled {
                Color32::from_rgb(60, 60, 75)
            } else if is_sel {
                self.theme.accent_magenta
            } else if is_hov {
                self.theme.accent_violet
            } else {
                Color32::from_rgb(180, 120, 255)
            };

            // Outer glow circle
            if is_sel || is_hov {
                painter.circle_filled(
                    center,
                    node_radius + 4.0,
                    Color32::from_rgba_premultiplied(168, 85, 247, 80),
                );
            }

            // Solid inner circle
            painter.circle_filled(center, node_radius, fill_color);
            painter.circle_stroke(center, node_radius, Stroke::new(1.5_f32, Color32::WHITE));

            // Band index text inside node
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                format!("{}", i + 1),
                FontId::proportional(9.0),
                Color32::BLACK,
            );

            // Tooltip when hovered
            if is_hov || is_sel {
                let tooltip_text = format!(
                    "Band {}: {:.0} Hz  {:+0.1} dB  Q={:.2}",
                    i + 1,
                    band.freq,
                    band.gain_db,
                    band.q
                );
                painter.text(
                    Pos2::new(node_x, (node_y - 18.0).max(rect.top() + 12.0)),
                    egui::Align2::CENTER_BOTTOM,
                    tooltip_text,
                    FontId::proportional(11.0),
                    self.theme.text_primary,
                );
            }
        }

        response_out
    }
}

/// Calculate frequency response in dB of a parametric biquad filter
fn evaluate_band_gain_db(band: &ParametricBand, f: f32, sample_rate: f32) -> f32 {
    let f0 = band.freq.clamp(10.0, sample_rate * 0.49);
    let q = band.q.max(0.01);
    let gain_db = band.gain_db;

    if gain_db.abs() < 0.01 && matches!(band.filter_type, EqFilterType::Peaking | EqFilterType::LowShelf | EqFilterType::HighShelf) {
        return 0.0;
    }

    let a = 10.0_f32.powf(gain_db / 40.0);
    let omega = 2.0 * std::f32::consts::PI * f0 / sample_rate;
    let cos_omega = omega.cos();
    let sin_omega = omega.sin();
    let alpha = sin_omega / (2.0 * q);

    let (b0, b1, b2, a0, a1, a2) = match band.filter_type {
        EqFilterType::Peaking => (
            1.0 + alpha * a,
            -2.0 * cos_omega,
            1.0 - alpha * a,
            1.0 + alpha / a,
            -2.0 * cos_omega,
            1.0 - alpha / a,
        ),
        EqFilterType::LowShelf => {
            let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
            (
                a * ((a + 1.0) - (a - 1.0) * cos_omega + two_sqrt_a_alpha),
                2.0 * a * ((a - 1.0) - (a + 1.0) * cos_omega),
                a * ((a + 1.0) - (a - 1.0) * cos_omega - two_sqrt_a_alpha),
                (a + 1.0) + (a - 1.0) * cos_omega + two_sqrt_a_alpha,
                -2.0 * ((a - 1.0) + (a + 1.0) * cos_omega),
                (a + 1.0) + (a - 1.0) * cos_omega - two_sqrt_a_alpha,
            )
        }
        EqFilterType::HighShelf => {
            let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
            (
                a * ((a + 1.0) + (a - 1.0) * cos_omega + two_sqrt_a_alpha),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_omega),
                a * ((a + 1.0) + (a - 1.0) * cos_omega - two_sqrt_a_alpha),
                (a + 1.0) - (a - 1.0) * cos_omega + two_sqrt_a_alpha,
                2.0 * ((a - 1.0) - (a + 1.0) * cos_omega),
                (a + 1.0) - (a - 1.0) * cos_omega - two_sqrt_a_alpha,
            )
        }
        EqFilterType::LowPass => (
            (1.0 - cos_omega) / 2.0,
            1.0 - cos_omega,
            (1.0 - cos_omega) / 2.0,
            1.0 + alpha,
            -2.0 * cos_omega,
            1.0 - alpha,
        ),
        EqFilterType::HighPass => (
            (1.0 + cos_omega) / 2.0,
            -(1.0 + cos_omega),
            (1.0 + cos_omega) / 2.0,
            1.0 + alpha,
            -2.0 * cos_omega,
            1.0 - alpha,
        ),
        EqFilterType::Notch => (
            1.0,
            -2.0 * cos_omega,
            1.0,
            1.0 + alpha,
            -2.0 * cos_omega,
            1.0 - alpha,
        ),
        EqFilterType::Bandpass => (
            alpha,
            0.0,
            -alpha,
            1.0 + alpha,
            -2.0 * cos_omega,
            1.0 - alpha,
        ),
        _ => return 0.0,
    };

    let w = 2.0 * std::f32::consts::PI * f / sample_rate;
    let cos_w = w.cos();
    let sin_w = w.sin();
    let cos_2w = (2.0 * w).cos();
    let sin_2w = (2.0 * w).sin();

    let num_re = b0 + b1 * cos_w + b2 * cos_2w;
    let num_im = -b1 * sin_w - b2 * sin_2w;
    let den_re = a0 + a1 * cos_w + a2 * cos_2w;
    let den_im = -a1 * sin_w - a2 * sin_2w;

    let num_mag2 = num_re * num_re + num_im * num_im;
    let den_mag2 = den_re * den_re + den_im * den_im;

    if den_mag2 <= 0.0 {
        return 0.0;
    }

    10.0 * (num_mag2 / den_mag2).max(1e-12).log10()
}
