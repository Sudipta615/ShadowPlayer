//! The graphic-EQ frequency-response canvas: grid, nodes, spline, keyboard
//! editing, and the drawing helpers the curve needs.

use super::presets::EqPreset;
use super::{EqPanelActions, EqPanelState};
use crate::theme::Theme;
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

// ─────────────────────────────────────────────────────────────────────────────
// 5. Spline EQ Canvas (Scales to 10, 15, 31, 64 Bands)
// ─────────────────────────────────────────────────────────────────────────────

pub(super) fn render_spline_eq_canvas(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    let avail_w = ui.available_width();
    let avail_h = ui.available_height();
    // The canvas is the visual anchor of the Graphic tab, not the whole tab.
    // A hard cap keeps the preamp and the tab strip above the fold when the
    // window is tall, and stops the canvas from demanding height the user did
    // not give the panel.
    let canvas_h = (avail_h - 90.0).clamp(190.0, 280.0);

    let (canvas_rect, canvas_resp) =
        ui.allocate_exact_size(Vec2::new(avail_w, canvas_h), Sense::click_and_drag());
    canvas_resp.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Slider,
            true,
            "Equalizer curve — use the arrow keys to select and adjust bands",
        )
    });
    let painter = ui.painter_at(canvas_rect);

    // Pill Background & Subtle Border
    painter.rect_filled(
        canvas_rect,
        CornerRadius::same(theme.radius_lg),
        theme.bg_pill,
    );
    painter.rect_stroke(
        canvas_rect,
        CornerRadius::same(theme.radius_lg),
        Stroke::new(1.0, theme.border_card),
        egui::StrokeKind::Inside,
    );

    let pad_left = 46.0;
    let pad_right = 16.0;
    let pad_top = 18.0;
    let pad_bottom = 24.0;

    let plot_rect = Rect::from_min_max(
        Pos2::new(canvas_rect.left() + pad_left, canvas_rect.top() + pad_top),
        Pos2::new(
            canvas_rect.right() - pad_right,
            canvas_rect.bottom() - pad_bottom,
        ),
    );

    // Horizontal Reference dB Lines (+24, +12, 0, -12, -24 if extended; else +12, +6, 0, -6, -12)
    let max_db = if state.extended_range {
        24.0_f32
    } else {
        12.0_f32
    };
    let grid_dbs: &[f32] = if state.extended_range {
        &[24.0, 12.0, 0.0, -12.0, -24.0]
    } else {
        &[12.0, 6.0, 0.0, -6.0, -12.0]
    };
    let y_for_db = |db: f32| -> f32 {
        let norm = (db - (-max_db)) / (2.0 * max_db);
        plot_rect.bottom() - norm * plot_rect.height()
    };

    for &db in grid_dbs {
        let y = y_for_db(db);
        let is_zero = db.abs() < 0.01;

        // dB Label on the left
        let lbl_text = if is_zero {
            "0dB".to_string()
        } else {
            format!("{:+0.0}dB", db)
        };
        painter.text(
            Pos2::new(canvas_rect.left() + 10.0, y),
            egui::Align2::LEFT_CENTER,
            lbl_text,
            crate::theme::font(10.5),
            if is_zero {
                theme.text_secondary
            } else {
                theme.text_muted
            },
        );

        // Gridline across the plot
        let stroke_color = if is_zero {
            theme.meter_grid
        } else {
            theme.meter_grid.gamma_multiply(0.55)
        };
        let stroke = Stroke::new(if is_zero { 1.2 } else { 1.0 }, stroke_color);
        draw_dashed_line(
            &painter,
            Pos2::new(plot_rect.left(), y),
            Pos2::new(plot_rect.right(), y),
            stroke,
        );
    }

    let freqs = state.graphic_layout.frequencies();
    let num_bands = freqs.len().clamp(1, 64);
    let col_w = plot_rect.width() / num_bands as f32;

    // Keyboard editing: Tab focuses the graph, Left/Right choose a band,
    // Up/Down move its gain, Home resets it. Every change goes through the
    // same action the pointer path uses, so the engine hears it identically.
    let mut band = state.keyboard_band.min(num_bands.saturating_sub(1));
    if canvas_resp.has_focus() {
        let (left, right, up, down, home, fine) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::Home),
                i.modifiers.shift,
            )
        });
        if left && band > 0 {
            band -= 1;
        }
        if right && band + 1 < num_bands {
            band += 1;
        }
        let step = if fine { 0.1 } else { 0.5 };
        let mut gain = state.graphic_gains[band];
        let mut changed = false;
        if up {
            gain = (gain + step).min(max_db);
            changed = true;
        }
        if down {
            gain = (gain - step).max(-max_db);
            changed = true;
        }
        if home {
            gain = 0.0;
            changed = true;
        }
        if changed {
            state.graphic_gains[band] = gain;
            state.selected_preset = EqPreset::Custom;
            state.selected_custom_preset_idx = None;
            actions.graphic_eq_changed = true;
        }
        state.keyboard_band = band;
    }
    let keyboard_band = band;

    let mut node_positions = Vec::with_capacity(num_bands);
    let mut hovered_band: Option<(usize, f32, f32, Pos2)> = None;

    let pointer_pos = ui.input(|i| i.pointer.interact_pos());

    for i in 0..num_bands {
        let center_x = plot_rect.left() + (i as f32 + 0.5) * col_w;
        let band_rect = Rect::from_min_max(
            Pos2::new(center_x - col_w * 0.5, plot_rect.top()),
            Pos2::new(center_x + col_w * 0.5, plot_rect.bottom()),
        );

        // Interactive Drag / Click for this band
        if canvas_resp.dragged() || canvas_resp.clicked() {
            if let Some(pos) = pointer_pos {
                if band_rect.contains(Pos2::new(pos.x, plot_rect.center().y)) {
                    let norm = ((plot_rect.bottom() - pos.y) / plot_rect.height()).clamp(0.0, 1.0);
                    let new_gain = (norm * (2.0 * max_db) - max_db).clamp(-max_db, max_db);
                    let rounded_gain = (new_gain * 10.0).round() / 10.0;
                    if (state.graphic_gains[i] - rounded_gain).abs() > 0.05 {
                        state.graphic_gains[i] = rounded_gain;
                        state.selected_preset = EqPreset::Custom;
                        state.selected_custom_preset_idx = None;
                        actions.graphic_eq_changed = true;
                    }
                }
            }
        }

        // Double-click resets band to 0.0 dB
        if canvas_resp.double_clicked() {
            if let Some(pos) = pointer_pos {
                if band_rect.contains(Pos2::new(pos.x, plot_rect.center().y)) {
                    state.graphic_gains[i] = 0.0;
                    state.selected_preset = EqPreset::Custom;
                    state.selected_custom_preset_idx = None;
                    actions.graphic_eq_changed = true;
                }
            }
        }

        let gain = state.graphic_gains[i];
        let node_y = y_for_db(gain);
        let node_pos = Pos2::new(center_x, node_y);
        node_positions.push(node_pos);

        // Background vertical track line
        painter.line_segment(
            [
                Pos2::new(center_x, plot_rect.top()),
                Pos2::new(center_x, plot_rect.bottom()),
            ],
            Stroke::new(1.0, theme.border_subtle),
        );

        // Vertical stem line from node down to bottom track with vibrant gradient
        let stem_start = Pos2::new(center_x, node_y);
        let stem_end = Pos2::new(center_x, plot_rect.bottom());
        if stem_start.y < stem_end.y {
            let stem_thickness = if num_bands <= 15 {
                2.5
            } else if num_bands <= 32 {
                1.8
            } else {
                1.2
            };
            draw_vertical_gradient_line(
                &painter,
                stem_start,
                stem_end,
                theme.accent_highlight,
                theme.accent,
                stem_thickness,
            );
        }

        // Frequency Label below the plot (condensed for high band counts)
        let show_label = if num_bands <= 15 {
            true
        } else if num_bands <= 32 {
            i % 2 == 0 || i == num_bands - 1
        } else {
            i % 6 == 0 || i == num_bands - 1
        };

        if show_label {
            let lbl_y = canvas_rect.bottom() - 10.0;
            let freq_val = freqs.get(i).copied().unwrap_or(1000.0);
            let freq_str = format_frequency_label(freq_val);
            painter.text(
                Pos2::new(center_x, lbl_y),
                egui::Align2::CENTER_CENTER,
                freq_str,
                crate::theme::font(if num_bands > 15 { 9.5 } else { 10.5 }),
                theme.text_secondary,
            );
        }

        // Hover detection for tooltip
        if let Some(pos) = pointer_pos {
            let hit_radius = if num_bands <= 15 { 14.0 } else { 8.0 };
            if (pos - node_pos).length() < hit_radius || band_rect.contains(pos) {
                let freq_val = freqs.get(i).copied().unwrap_or(1000.0);
                hovered_band = Some((i, freq_val, gain, node_pos));
            }
        }
    }

    // Continuous Catmull-Rom Spline Curve passing through all nodes
    let segments_per_band = if num_bands <= 15 {
        16
    } else if num_bands <= 32 {
        8
    } else {
        4
    };
    let spline_pts = catmull_rom_spline(&node_positions, segments_per_band);

    for w in spline_pts.windows(2) {
        painter.line_segment([w[0], w[1]], Stroke::new(2.4, theme.accent_highlight));
    }

    // Draw Node Thumbs (white circular nodes with glow and border)
    let base_node_r = if num_bands <= 15 {
        5.5
    } else if num_bands <= 32 {
        4.0
    } else {
        2.8
    };
    for (i, &pos) in node_positions.iter().enumerate() {
        let is_hovered = hovered_band.map(|(b, _, _, _)| b == i).unwrap_or(false);

        // Glow halo
        let halo_r = if is_hovered {
            base_node_r + 4.5
        } else {
            base_node_r + 2.5
        };
        painter.circle_filled(
            pos,
            halo_r,
            crate::theme::with_alpha(theme.accent_highlight, if is_hovered { 90 } else { 45 }),
        );

        // Center Node
        let node_r = if is_hovered {
            base_node_r + 1.2
        } else {
            base_node_r
        };
        painter.circle_filled(pos, node_r, Color32::WHITE);
        painter.circle_stroke(pos, node_r, Stroke::new(1.6, theme.accent_highlight));

        // The keyboard's current band gets a second ring, so focus is visible
        // and the arrow keys have an obvious subject.
        if canvas_resp.has_focus() && i == keyboard_band {
            painter.circle_stroke(pos, node_r + 5.0, Stroke::new(2.0, theme.accent_highlight));
        }
    }

    // Floating Gain Tooltip on hovered node
    if let Some((_, freq, gain, pos)) = hovered_band {
        let tip_y = if pos.y > canvas_rect.top() + 35.0 {
            pos.y - 18.0
        } else {
            pos.y + 18.0
        };
        let tip_pos = Pos2::new(pos.x, tip_y);
        let tip_str = format!("{}: {:+.1} dB", format_exact_frequency(freq), gain);

        let font = crate::theme::font(11.0);
        let galley = painter.layout_no_wrap(tip_str, font, theme.text_primary);
        let tip_rect = Rect::from_center_size(tip_pos, galley.size() + Vec2::new(10.0, 4.0));

        painter.rect_filled(tip_rect, CornerRadius::same(theme.radius_sm), theme.bg_card);
        painter.rect_stroke(
            tip_rect,
            CornerRadius::same(theme.radius_sm),
            Stroke::new(1.0, theme.accent_highlight),
            egui::StrokeKind::Inside,
        );
        painter.galley(
            tip_rect.min + Vec2::new(5.0, 2.0),
            galley,
            theme.text_primary,
        );
    }

    let band_freq = freqs.get(keyboard_band).copied().unwrap_or(1000.0);
    let band_gain = state.graphic_gains[keyboard_band];
    canvas_resp.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Slider,
            true,
            format!(
                "Graphic equalizer, band {} of {}, {}, {:+.1} dB. \
                 Left and right choose a band; up and down adjust gain; Home resets.",
                keyboard_band + 1,
                num_bands,
                format_exact_frequency(band_freq),
                band_gain
            ),
        )
    });

    ui.label(
        egui::RichText::new(
            "Focus the graph and use ←/→ to choose a band, ↑/↓ to adjust it \
             (Shift for fine steps), Home to reset.",
        )
        .font(crate::theme::font(10.0))
        .color(theme.text_dim),
    );
}

fn format_frequency_label(f: f32) -> String {
    if f >= 10000.0 {
        format!("{:.0}k", f / 1000.0)
    } else if f >= 1000.0 {
        if (f % 1000.0).abs() < 10.0 {
            format!("{:.0}k", f / 1000.0)
        } else {
            format!("{:.1}k", f / 1000.0)
        }
    } else {
        format!("{:.0}", f)
    }
}

fn format_exact_frequency(f: f32) -> String {
    if f >= 1000.0 {
        format!("{:.1} kHz", f / 1000.0)
    } else {
        format!("{:.0} Hz", f)
    }
}

fn draw_dashed_line(painter: &egui::Painter, start: Pos2, end: Pos2, stroke: Stroke) {
    let total_len = (end - start).length();
    let dash_len = 5.0;
    let gap_len = 4.0;
    let dir = (end - start) / total_len;

    let mut cur = 0.0;
    while cur < total_len {
        let p1 = start + dir * cur;
        let p2 = start + dir * (cur + dash_len).min(total_len);
        painter.line_segment([p1, p2], stroke);
        cur += dash_len + gap_len;
    }
}

fn draw_vertical_gradient_line(
    painter: &egui::Painter,
    start: Pos2,
    end: Pos2,
    top_col: Color32,
    bottom_col: Color32,
    thickness: f32,
) {
    let steps = 14;
    let total_h = end.y - start.y;
    if total_h <= 0.0 {
        return;
    }

    for i in 0..steps {
        let t0 = i as f32 / steps as f32;
        let t1 = (i + 1) as f32 / steps as f32;
        let y0 = start.y + t0 * total_h;
        let y1 = start.y + t1 * total_h;

        let col = crate::theme::lerp_color(top_col, bottom_col, t0);

        painter.line_segment(
            [Pos2::new(start.x, y0), Pos2::new(start.x, y1)],
            Stroke::new(thickness, col),
        );
    }
}

fn catmull_rom_spline(points: &[Pos2], segments: usize) -> Vec<Pos2> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let n = points.len();
    let mut result = Vec::with_capacity(n * segments);

    for i in 0..n - 1 {
        let p0 = if i == 0 { points[0] } else { points[i - 1] };
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = if i + 2 < n {
            points[i + 2]
        } else {
            points[i + 1]
        };

        for step in 0..segments {
            let t = step as f32 / segments as f32;
            let t2 = t * t;
            let t3 = t2 * t;

            let x = 0.5
                * ((2.0 * p1.x)
                    + (-p0.x + p2.x) * t
                    + (2.0 * p0.x - 5.0 * p1.x + 4.0 * p2.x - p3.x) * t2
                    + (-p0.x + 3.0 * p1.x - 3.0 * p2.x + p3.x) * t3);

            let y = 0.5
                * ((2.0 * p1.y)
                    + (-p0.y + p2.y) * t
                    + (2.0 * p0.y - 5.0 * p1.y + 4.0 * p2.y - p3.y) * t2
                    + (-p0.y + 3.0 * p1.y - 3.0 * p2.y + p3.y) * t3);

            result.push(Pos2::new(x, y));
        }
    }
    result.push(*points.last().unwrap());
    result
}
