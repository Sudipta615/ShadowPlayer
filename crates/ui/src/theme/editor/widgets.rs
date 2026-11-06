use eframe::egui::{self, Align, Color32, CornerRadius, Layout, Margin, Stroke, Vec2};

use crate::theme::tokens::{
    format_hex_color, format_hsl_color, format_rgb_color, parse_color_code, with_alpha, Theme,
};

use super::state::ThemeEditorState;

pub(super) fn section_heading(ui: &mut egui::Ui, label: &str, theme: &Theme) {
    ui.label(
        egui::RichText::new(label)
            .font(crate::theme::font(11.5))
            .strong()
            .color(theme.accent_highlight),
    );
    ui.add_space(theme.space_xxs);
}

pub(super) fn field_label(ui: &mut egui::Ui, label: &str, theme: &Theme) {
    ui.label(
        egui::RichText::new(label)
            .font(crate::theme::font(10.5))
            .color(theme.text_muted),
    );
}

/// A miniature of the real player, painted with the live editor palette.
pub(super) fn render_live_preview(
    ui: &mut egui::Ui,
    state: &ThemeEditorState,
    preview: &Theme,
    host: &Theme,
) {
    let w = 178.0;
    egui::Frame::NONE
        .fill(preview.bg_app)
        .stroke(Stroke::new(1.0, preview.border_card))
        .corner_radius(CornerRadius::same(preview.radius_lg))
        .inner_margin(Margin::same(host.space_sm as i8))
        .show(ui, |ui| {
            ui.set_width(w);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let (art, _) = ui.allocate_exact_size(Vec2::splat(40.0), egui::Sense::hover());
                    ui.painter().rect_filled(
                        art,
                        CornerRadius::same(preview.radius_md),
                        preview.artwork_placeholder,
                    );
                    ui.painter().circle_filled(
                        art.center(),
                        12.0,
                        with_alpha(preview.accent_highlight, 190),
                    );
                    ui.vertical(|ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&state.name)
                                    .font(crate::theme::font(11.0))
                                    .strong()
                                    .color(preview.text_primary),
                            )
                            .truncate(),
                        );
                        ui.label(
                            egui::RichText::new("Sample artist")
                                .font(crate::theme::font(9.5))
                                .color(preview.text_muted),
                        );
                    });
                });

                ui.add_space(host.space_xs);

                let (track, _) = ui.allocate_exact_size(Vec2::new(w, 4.0), egui::Sense::hover());
                ui.painter().rect_filled(
                    track,
                    CornerRadius::same(preview.radius_xs),
                    preview.slider_track,
                );
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(
                        track.min,
                        Vec2::new(track.width() * 0.42, track.height()),
                    ),
                    CornerRadius::same(preview.radius_xs),
                    preview.accent,
                );

                ui.add_space(host.space_xs);

                ui.horizontal(|ui| {
                    ui.push_id("editor_preview_transport", |ui| {
                        crate::components::play_button::play_button(ui, 30.0, false, preview);
                    });
                    ui.label(
                        egui::RichText::new(state.play_button_shape.label())
                            .font(crate::theme::mono_font(9.0))
                            .color(preview.text_secondary),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        for color in [
                            preview.accent,
                            preview.accent_highlight,
                            preview.accent_warm,
                            preview.info,
                        ] {
                            let (dot, _) =
                                ui.allocate_exact_size(Vec2::splat(9.0), egui::Sense::hover());
                            ui.painter().circle_filled(dot.center(), 4.0, color);
                        }
                    });
                });
            });
        });
}

pub(super) fn font_role_input(
    ui: &mut egui::Ui,
    value: &mut Option<String>,
    label: &str,
    hint: &str,
    theme: &Theme,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.space_xs;
        ui.label(
            egui::RichText::new(label)
                .font(crate::theme::font(10.0))
                .color(theme.text_muted),
        );
        let mut text = value.clone().unwrap_or_default();
        let response = ui.add(
            egui::TextEdit::singleline(&mut text)
                .hint_text(hint)
                .desired_width(106.0)
                .font(crate::theme::font(11.0)),
        );
        if response.changed() {
            *value = (!text.trim().is_empty()).then(|| text.trim().to_string());
        }
    });
}

/// Convert RGB (0..=255) to HSV (h: 0..=360, s: 0..=1, v: 0..=1).
pub(super) fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let delta = max - min;
    let v = max;
    let s = if max > 1e-5 { delta / max } else { 0.0 };
    let h = if delta < 1e-5 {
        0.0
    } else if (max - rf).abs() < 1e-5 {
        let mut h = (gf - bf) / delta;
        if h < 0.0 {
            h += 6.0;
        }
        h * 60.0
    } else if (max - gf).abs() < 1e-5 {
        ((bf - rf) / delta + 2.0) * 60.0
    } else {
        ((rf - gf) / delta + 4.0) * 60.0
    };
    (h, s, v)
}

/// Convert HSV (h: 0..=360, s: 0..=1, v: 0..=1) to RGB (0..=255).
pub(super) fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let h = ((h % 360.0) + 360.0) % 360.0;
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let c = v * s;
    let h_prime = h / 60.0;
    let x = c * (1.0 - ((h_prime % 2.0) - 1.0).abs());
    let (r1, g1, b1) = if h_prime < 1.0 {
        (c, x, 0.0)
    } else if h_prime < 2.0 {
        (x, c, 0.0)
    } else if h_prime < 3.0 {
        (0.0, c, x)
    } else if h_prime < 4.0 {
        (0.0, x, c)
    } else if h_prime < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    let m = v - c;
    (
        ((r1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

/// Interactive radial HSV color wheel widget with continuous hue and saturation selection.
pub(super) fn color_wheel_widget(ui: &mut egui::Ui, color: &mut Color32, theme: &Theme) -> bool {
    let size = 110.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click_and_drag());
    let center = rect.center();
    let radius = (size / 2.0 - 4.0).max(1.0);

    let (mut h, mut s, mut v) = rgb_to_hsv(color.r(), color.g(), color.b());
    let mut changed = false;

    if response.clicked() || response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let diff = pos - center;
            let dist = diff.length();
            s = (dist / radius).clamp(0.0, 1.0);
            let angle = diff.y.atan2(diff.x);
            let mut deg = angle.to_degrees();
            if deg < 0.0 {
                deg += 360.0;
            }
            h = deg;
            if v < 0.15 {
                v = 1.0;
            }
            let (r, g, b) = hsv_to_rgb(h, s, v);
            *color = Color32::from_rgba_unmultiplied(r, g, b, color.a());
            changed = true;
        }
    }

    // Paint wheel mesh using bilinear-interpolated triangles
    let mut mesh = egui::Mesh::default();
    let center_idx = 0u32;
    let center_val = (v * 255.0).round().clamp(0.0, 255.0) as u8;
    let center_color = Color32::from_rgb(center_val, center_val, center_val);
    mesh.vertices.push(egui::epaint::Vertex {
        pos: center,
        uv: egui::epaint::WHITE_UV,
        color: center_color,
    });

    let segments = 36;
    for i in 0..=segments {
        let angle = (i as f32 / segments as f32) * std::f32::consts::TAU;
        let pos = center + Vec2::new(angle.cos(), angle.sin()) * radius;
        let hue = angle.to_degrees().rem_euclid(360.0);
        let (r, g, b) = hsv_to_rgb(hue, 1.0, v);
        mesh.vertices.push(egui::epaint::Vertex {
            pos,
            uv: egui::epaint::WHITE_UV,
            color: Color32::from_rgb(r, g, b),
        });
        if i > 0 {
            mesh.add_triangle(center_idx, i as u32, (i + 1) as u32);
        }
    }
    ui.painter().add(egui::Shape::mesh(mesh));

    // Outer border ring
    ui.painter()
        .circle_stroke(center, radius, Stroke::new(1.0, theme.border_subtle));

    // Reticle at current (h, s)
    let reticle_angle = h.to_radians();
    let reticle_dist = s * radius;
    let reticle_pos = center + Vec2::new(reticle_angle.cos(), reticle_angle.sin()) * reticle_dist;
    ui.painter()
        .circle_stroke(reticle_pos, 5.0, Stroke::new(2.0, Color32::WHITE));
    ui.painter().circle_stroke(
        reticle_pos,
        6.0,
        Stroke::new(1.0, Color32::from_black_alpha(180)),
    );

    changed
}

const EDITABLE_TOKENS: &[(&str, &str)] = &[
    ("accent", "Primary Accent"),
    ("accent_highlight", "Highlight Accent"),
    ("accent_warm", "Warm Accent"),
    ("bg_app", "App Background"),
    ("bg_sidebar", "Sidebar Background"),
    ("bg_card", "Cards & Dialogs"),
    ("bg_pill", "Pills & Chips"),
    ("bg_input", "Text Input"),
    ("border_card", "Card Border"),
    ("border_subtle", "Subtle Border"),
    ("text_primary", "Primary Text"),
    ("text_secondary", "Secondary Text"),
    ("text_muted", "Muted Text"),
    ("text_dim", "Dim Text"),
    ("info", "Info Blue"),
    ("bit_perfect_green", "Bit-Perfect Green"),
    ("warning_yellow", "Warning Yellow"),
    ("destructive", "Destructive Red"),
    ("slider_track", "Slider Track"),
    ("artwork_placeholder", "Artwork Placeholder"),
];

const QUICK_PALETTE: &[(&str, Color32)] = &[
    ("Ruby", Color32::from_rgb(239, 68, 68)),
    ("Coral", Color32::from_rgb(249, 115, 22)),
    ("Amber", Color32::from_rgb(245, 158, 11)),
    ("Emerald", Color32::from_rgb(16, 185, 129)),
    ("Mint", Color32::from_rgb(20, 184, 166)),
    ("Cyan", Color32::from_rgb(6, 182, 212)),
    ("Sky", Color32::from_rgb(56, 189, 248)),
    ("Blue", Color32::from_rgb(59, 130, 246)),
    ("Indigo", Color32::from_rgb(99, 102, 241)),
    ("Violet", Color32::from_rgb(139, 92, 246)),
    ("Purple", Color32::from_rgb(168, 85, 247)),
    ("Fuchsia", Color32::from_rgb(217, 70, 239)),
    ("Pink", Color32::from_rgb(236, 72, 153)),
    ("Rose", Color32::from_rgb(244, 63, 94)),
    ("Slate", Color32::from_rgb(100, 116, 139)),
    ("Dark", Color32::from_rgb(15, 23, 42)),
    ("White", Color32::from_rgb(255, 255, 255)),
];

/// The interactive Color Wheel Studio for custom palette authoring.
pub(super) fn render_color_studio(ui: &mut egui::Ui, state: &mut ThemeEditorState, theme: &Theme) {
    if state.selected_token.is_none() {
        state.select_token("accent");
    }
    let current_token = state
        .selected_token
        .clone()
        .unwrap_or_else(|| "accent".to_string());
    let mut current_color = state
        .colors
        .get(&current_token)
        .copied()
        .unwrap_or(theme.accent);

    egui::Frame::NONE
        .fill(theme.bg_card)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(theme.radius_md))
        .inner_margin(Margin::same(theme.space_sm as i8))
        .show(ui, |ui| {
            // Header: Studio label + Token Selector + Live Swatch
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Color Wheel Studio")
                        .font(crate::theme::font(11.5))
                        .strong()
                        .color(theme.accent_highlight),
                );
                ui.add_space(theme.space_xs);
                ui.label(
                    egui::RichText::new("Editing:")
                        .font(crate::theme::font(10.5))
                        .color(theme.text_muted),
                );
                let selected_label = EDITABLE_TOKENS
                    .iter()
                    .find(|(k, _)| *k == current_token)
                    .map(|(_, l)| *l)
                    .unwrap_or(current_token.as_str());
                egui::ComboBox::from_id_salt("color_studio_token_select")
                    .selected_text(selected_label)
                    .show_ui(ui, |ui| {
                        for (key, label) in EDITABLE_TOKENS {
                            let is_sel = current_token == *key;
                            if ui.selectable_label(is_sel, *label).clicked() {
                                state.select_token(key);
                            }
                        }
                    });

                // Active token color swatch badge
                let (swatch_rect, _) =
                    ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
                ui.painter().rect_filled(
                    swatch_rect,
                    CornerRadius::same(theme.radius_xs),
                    current_color,
                );
                ui.painter().rect_stroke(
                    swatch_rect,
                    CornerRadius::same(theme.radius_xs),
                    Stroke::new(1.0, theme.border_subtle),
                    egui::StrokeKind::Inside,
                );
            });

            ui.add_space(theme.space_xs);

            let mut color_changed = false;
            ui.horizontal(|ui| {
                // Left Column: Radial Color Wheel
                ui.vertical(|ui| {
                    if color_wheel_widget(ui, &mut current_color, theme) {
                        color_changed = true;
                    }
                });

                ui.add_space(theme.space_sm);

                // Right Column: Value, Alpha, Code Input, Formats, and Presets
                ui.vertical(|ui| {
                    // Brightness / Value slider
                    let (h, s, mut v) =
                        rgb_to_hsv(current_color.r(), current_color.g(), current_color.b());
                    let mut v_pct = (v * 100.0).round();
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Brightness:")
                                .font(crate::theme::font(10.5))
                                .color(theme.text_secondary),
                        );
                        if ui
                            .add(
                                egui::Slider::new(&mut v_pct, 0.0..=100.0)
                                    .show_value(true)
                                    .suffix("%"),
                            )
                            .changed()
                        {
                            v = (v_pct / 100.0).clamp(0.0, 1.0);
                            let (r, g, b) = hsv_to_rgb(h, s, v);
                            current_color =
                                Color32::from_rgba_unmultiplied(r, g, b, current_color.a());
                            color_changed = true;
                        }
                    });

                    // Alpha / Opacity slider
                    let mut alpha = current_color.a();
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Opacity:")
                                .font(crate::theme::font(10.5))
                                .color(theme.text_secondary),
                        );
                        if ui
                            .add(egui::Slider::new(&mut alpha, 0..=255).show_value(true))
                            .changed()
                        {
                            current_color = Color32::from_rgba_unmultiplied(
                                current_color.r(),
                                current_color.g(),
                                current_color.b(),
                                alpha,
                            );
                            color_changed = true;
                        }
                    });

                    // Color Code Input (Supports Hex, RGB, HSL)
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Color code:")
                                .font(crate::theme::font(10.5))
                                .color(theme.text_secondary),
                        );
                        let text_resp = ui.add(
                            egui::TextEdit::singleline(&mut state.color_code_text)
                                .hint_text("#hex, rgb(...), hsl(...)")
                                .desired_width(170.0)
                                .font(crate::theme::mono_font(10.0)),
                        );
                        if text_resp.changed() {
                            if let Some(parsed) = parse_color_code(&state.color_code_text) {
                                current_color = parsed;
                                color_changed = true;
                            }
                        }
                    });

                    // Format Chips (Click to copy & switch display format)
                    ui.horizontal_wrapped(|ui| {
                        let hex_str = format_hex_color(current_color);
                        let rgb_str = format_rgb_color(current_color);
                        let hsl_str = format_hsl_color(current_color);

                        for (fmt_name, fmt_val) in
                            [("HEX", &hex_str), ("RGB", &rgb_str), ("HSL", &hsl_str)]
                        {
                            let chip_text = format!("{}: {}", fmt_name, fmt_val);
                            let chip_resp = ui.add(
                                egui::Button::new(
                                    egui::RichText::new(chip_text)
                                        .font(crate::theme::mono_font(9.0))
                                        .color(theme.text_secondary),
                                )
                                .fill(theme.bg_pill)
                                .stroke(Stroke::new(1.0, theme.border_subtle))
                                .corner_radius(CornerRadius::same(theme.radius_xs))
                                .min_size(Vec2::new(0.0, 18.0)),
                            );
                            if chip_resp
                                .on_hover_text("Click to copy to clipboard and set input")
                                .clicked()
                            {
                                ui.ctx().copy_text(fmt_val.to_string());
                                state.color_code_text = fmt_val.to_string();
                            }
                        }
                    });

                    // Quick Spectrum Presets
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new("Presets:")
                                .font(crate::theme::font(10.0))
                                .color(theme.text_muted),
                        );
                        for &(name, swatch_col) in QUICK_PALETTE {
                            let (sw_rect, sw_resp) =
                                ui.allocate_exact_size(Vec2::splat(15.0), egui::Sense::click());
                            ui.painter().rect_filled(
                                sw_rect,
                                CornerRadius::same(theme.radius_xs),
                                swatch_col,
                            );
                            ui.painter().rect_stroke(
                                sw_rect,
                                CornerRadius::same(theme.radius_xs),
                                Stroke::new(1.0, theme.border_subtle),
                                egui::StrokeKind::Inside,
                            );
                            if sw_resp.on_hover_text(name).clicked() {
                                current_color = swatch_col;
                                color_changed = true;
                            }
                        }
                    });
                });
            });

            if color_changed {
                state.set_selected_token_color(current_color);
                state.color_code_text = format_hex_color(current_color);
            }
        });
}

pub(super) fn color_picker_row(
    ui: &mut egui::Ui,
    state: &mut ThemeEditorState,
    token: &str,
    label: &str,
    theme: &Theme,
) {
    if let Some(mut color) = state.colors.get(token).copied() {
        let is_selected = state.selected_token.as_deref() == Some(token);
        let mut color_changed = false;
        let mut select_requested = false;
        let mut code_update: Option<String> = None;

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme.space_xs;
            let mut srgba = [color.r(), color.g(), color.b(), color.a()];
            let picker_resp = ui.color_edit_button_srgba_unmultiplied(&mut srgba);
            if picker_resp.changed() {
                color = Color32::from_rgba_unmultiplied(srgba[0], srgba[1], srgba[2], srgba[3]);
                color_changed = true;
            }
            if picker_resp.clicked() && !is_selected {
                select_requested = true;
            }

            // Clickable label to select token in Color Studio
            let label_resp = ui.allocate_ui_with_layout(
                Vec2::new(104.0, 18.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(label)
                                .font(crate::theme::font(10.5))
                                .color(if is_selected {
                                    theme.accent_highlight
                                } else {
                                    theme.text_secondary
                                }),
                        )
                        .sense(egui::Sense::click())
                        .truncate(),
                    )
                },
            );
            if label_resp.inner.clicked() {
                select_requested = true;
            }

            // Inline editable color code input (supports hex, rgb, hsl)
            let text_id = ui.make_persistent_id(token).with("code_input");
            let mut current_text = ui
                .data(|d| d.get_temp::<String>(text_id))
                .unwrap_or_else(|| format_hex_color(color));
            if !is_selected
                && ui.data(|d| !d.get_temp::<bool>(text_id.with("focus")).unwrap_or(false))
            {
                current_text = format_hex_color(color);
            }
            let text_resp = ui.add(
                egui::TextEdit::singleline(&mut current_text)
                    .desired_width(70.0)
                    .font(crate::theme::mono_font(9.5)),
            );
            if text_resp.gained_focus() {
                select_requested = true;
                ui.data_mut(|d| d.insert_temp(text_id.with("focus"), true));
            }
            if text_resp.lost_focus() {
                ui.data_mut(|d| d.insert_temp(text_id.with("focus"), false));
                current_text = format_hex_color(color);
            }
            if text_resp.changed() {
                if let Some(parsed) = parse_color_code(&current_text) {
                    color = parsed;
                    color_changed = true;
                    code_update = Some(current_text.clone());
                }
            }
            ui.data_mut(|d| d.insert_temp(text_id, current_text));
        });

        if select_requested {
            state.select_token(token);
        }
        if color_changed {
            if let Some(c) = state.colors.get_mut(token) {
                *c = color;
            }
            if is_selected || select_requested {
                state.color_code_text = code_update.unwrap_or_else(|| format_hex_color(color));
            }
        }
    }
}
