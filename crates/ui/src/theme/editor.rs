use std::collections::HashMap;

use eframe::egui::{self, Align, Color32, CornerRadius, Layout, Margin, Stroke, Vec2};

use super::contrast::{auto_adjust_contrast, validate_contrast};
use super::fonts::{bundled_font_family_names, ThemeFonts};
use super::manager::ThemeManager;
use super::schema::{for_each_required_color_token, ThemeFile};
use super::tokens::{format_hex_color, Theme};

/// State for the in-app Theme Editor.
#[derive(Debug, Clone)]
pub struct ThemeEditorState {
    pub is_open: bool,
    pub id: String,
    pub name: String,
    pub author: String,
    pub description: String,
    pub is_light: bool,
    pub colors: HashMap<String, Color32>,
    pub radii: HashMap<String, u8>,
    pub fonts: ThemeFonts,
    pub status_message: Option<(String, bool)>, // (message, is_error)
}

impl Default for ThemeEditorState {
    fn default() -> Self {
        let mut state = Self {
            is_open: false,
            id: "my_custom_theme".to_string(),
            name: "My Custom Theme".to_string(),
            author: "Me".to_string(),
            description: "Custom ShadowPlayer theme".to_string(),
            is_light: false,
            colors: HashMap::new(),
            radii: HashMap::new(),
            fonts: ThemeFonts::default(),
            status_message: None,
        };
        state.populate_from_theme(&Theme::dark());
        state
    }
}

impl ThemeEditorState {
    /// Populate editor values from an existing Theme.
    pub fn populate_from_theme(&mut self, theme: &Theme) {
        self.is_light = theme.is_light;

        macro_rules! set_editor_color {
            ($($field:ident),* $(,)?) => {
                $(
                    self.colors.insert(stringify!($field).to_string(), theme.$field);
                )*
            };
        }

        for_each_required_color_token!(set_editor_color);

        self.radii.insert("radius_xs".into(), theme.radius_xs);
        self.radii.insert("radius_sm".into(), theme.radius_sm);
        self.radii.insert("radius_md".into(), theme.radius_md);
        self.radii.insert("radius_lg".into(), theme.radius_lg);
        self.radii.insert("radius_xl".into(), theme.radius_xl);
        self.radii.insert("radius_pill".into(), theme.radius_pill);
    }

    /// Automatically adjust all text tokens so they satisfy WCAG 2.1 AA against host surfaces.
    pub fn auto_fix_contrast(&mut self) {
        let bg_app = self.colors.get("bg_app").copied().unwrap_or(Color32::BLACK);
        let bg_card = self
            .colors
            .get("bg_card")
            .copied()
            .unwrap_or(Color32::BLACK);
        let bg_input = self
            .colors
            .get("bg_input")
            .copied()
            .unwrap_or(Color32::BLACK);

        let text_tokens = [
            "text_primary",
            "text_secondary",
            "text_muted",
            "text_dim",
            "accent_highlight",
            "accent_warm",
        ];

        for token in text_tokens {
            if let Some(mut fg) = self.colors.get(token).copied() {
                fg = auto_adjust_contrast(fg, bg_app, 4.5);
                fg = auto_adjust_contrast(fg, bg_card, 4.5);
                self.colors.insert(token.to_string(), fg);
            }
        }

        if let Some(border_input) = self.colors.get("border_input").copied() {
            let adjusted = auto_adjust_contrast(border_input, bg_input, 3.0);
            self.colors.insert("border_input".to_string(), adjusted);
        }

        self.status_message = Some((
            "All contrast values successfully auto-adjusted!".into(),
            false,
        ));
    }
}

/// Actions emitted by the Theme Editor.
#[derive(Default, Clone, Debug)]
pub struct EditorActions {
    pub theme_saved: Option<String>,
}

/// Render the in-app Theme Editor.
pub fn render_theme_editor(
    ui: &mut egui::Ui,
    state: &mut ThemeEditorState,
    manager: &ThemeManager,
    theme: &Theme,
) -> EditorActions {
    let mut actions = EditorActions::default();

    egui::Frame::NONE
        .fill(theme.bg_card)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(theme.radius_lg))
        .inner_margin(Margin::same(theme.space_xl as i8))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                // Header with Collapse/Close toggle
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Theme Editor & Customizer")
                            .font(crate::theme::font(14.0))
                            .strong()
                            .color(theme.text_primary),
                    );

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let toggle_lbl = if state.is_open { "Hide Editor ▲" } else { "Open Editor ▼" };
                        if ui
                            .button(
                                egui::RichText::new(toggle_lbl)
                                    .font(crate::theme::font(11.5))
                                    .color(theme.accent_highlight),
                            )
                            .clicked()
                        {
                            state.is_open = !state.is_open;
                        }
                    });
                });

                if !state.is_open {
                    return;
                }

                ui.add_space(theme.space_md);

                // Status message banner
                if let Some((ref msg, is_err)) = state.status_message {
                    let color = if is_err { theme.destructive } else { theme.bit_perfect_green };
                    ui.label(
                        egui::RichText::new(msg)
                            .font(crate::theme::font(12.0))
                            .color(color),
                    );
                    ui.add_space(theme.space_xs);
                }

                // Metadata inputs
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Theme Name:")
                            .font(crate::theme::font(12.0))
                            .color(theme.text_secondary),
                    );
                    ui.add(egui::TextEdit::singleline(&mut state.name).desired_width(140.0));

                    ui.add_space(theme.space_sm);

                    ui.label(
                        egui::RichText::new("File ID:")
                            .font(crate::theme::font(12.0))
                            .color(theme.text_secondary),
                    );
                    ui.add(egui::TextEdit::singleline(&mut state.id).desired_width(120.0));

                    ui.add_space(theme.space_sm);

                    ui.label(
                        egui::RichText::new("Author:")
                            .font(crate::theme::font(12.0))
                            .color(theme.text_secondary),
                    );
                    ui.add(egui::TextEdit::singleline(&mut state.author).desired_width(100.0));
                });

                ui.add_space(theme.space_md);

                // Per-role typefaces. An empty field means Inter for that role.
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new("Fonts:")
                            .font(crate::theme::font(12.0))
                            .color(theme.text_secondary),
                    );
                    font_role_input(ui, &mut state.fonts.display, "Display", "Inter", theme);
                    font_role_input(ui, &mut state.fonts.body, "Body", "Inter", theme);
                    font_role_input(
                        ui,
                        &mut state.fonts.mono,
                        "Mono",
                        "JetBrains Mono",
                        theme,
                    );
                });
                ui.horizontal(|ui| {
                    let bundled = bundled_font_family_names().collect::<Vec<_>>().join(", ");
                    ui.label(
                        egui::RichText::new("Bundled families")
                            .font(crate::theme::font(10.0))
                            .color(theme.text_dim),
                    )
                    .on_hover_text(bundled);
                    let missing = state.fonts.missing_fonts();
                    if !missing.is_empty() {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} unavailable · using Inter",
                                missing.join(", ")
                            ))
                            .font(crate::theme::font(10.0))
                            .color(theme.warning_yellow),
                        );
                    }
                });

                ui.add_space(theme.space_md);

                // Quick Preset Base Loader
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Clone Base Colors From:")
                            .font(crate::theme::font(12.0))
                            .color(theme.text_muted),
                    );
                    if ui.button("Obsidian Dark").clicked() {
                        state.populate_from_theme(&Theme::dark());
                    }
                    if ui.button("Studio Light").clicked() {
                        state.populate_from_theme(&Theme::light());
                    }
                    if ui.button("OLED Midnight").clicked() {
                        state.populate_from_theme(&Theme::oled());
                    }
                });

                ui.add_space(theme.space_lg);

                // Grouped Color Pickers
                ui.columns(3, |columns| {
                    // Column 1: Surfaces & Backgrounds
                    columns[0].vertical(|ui| {
                        ui.label(
                            egui::RichText::new("Surfaces")
                                .font(crate::theme::font(12.5))
                                .strong()
                                .color(theme.accent_highlight),
                        );
                        ui.add_space(theme.space_xs);
                        color_picker_row(ui, state, "bg_app", "App Background", theme);
                        color_picker_row(ui, state, "bg_sidebar", "Sidebar", theme);
                        color_picker_row(ui, state, "bg_card", "Cards & Dialogs", theme);
                        color_picker_row(ui, state, "bg_pill", "Pills & Chips", theme);
                        color_picker_row(ui, state, "bg_input", "Text Input", theme);
                        color_picker_row(ui, state, "border_card", "Card Border", theme);
                    });

                    // Column 2: Accents & Status
                    columns[1].vertical(|ui| {
                        ui.label(
                            egui::RichText::new("Accents & Status")
                                .font(crate::theme::font(12.5))
                                .strong()
                                .color(theme.accent_highlight),
                        );
                        ui.add_space(theme.space_xs);
                        color_picker_row(ui, state, "accent", "Primary Accent", theme);
                        color_picker_row(ui, state, "accent_highlight", "Highlight Mint", theme);
                        color_picker_row(ui, state, "accent_warm", "Amber Secondary", theme);
                        color_picker_row(ui, state, "info", "Info Blue", theme);
                        color_picker_row(ui, state, "destructive", "Destructive", theme);
                        color_picker_row(ui, state, "bit_perfect_green", "Bit-Perfect Green", theme);
                    });

                    // Column 3: Text & Contrast Check
                    columns[2].vertical(|ui| {
                        ui.label(
                            egui::RichText::new("Typography & Contrast")
                                .font(crate::theme::font(12.5))
                                .strong()
                                .color(theme.accent_highlight),
                        );
                        ui.add_space(theme.space_xs);
                        color_picker_row(ui, state, "text_primary", "Primary Text", theme);
                        color_picker_row(ui, state, "text_secondary", "Secondary Text", theme);
                        color_picker_row(ui, state, "text_muted", "Muted Text", theme);
                        color_picker_row(ui, state, "text_dim", "Dim Text", theme);

                        ui.add_space(theme.space_sm);

                        // Live Contrast Evaluation
                        let valid = validate_contrast(&state.colors);
                        match valid {
                            Ok(()) => {
                                ui.label(
                                    egui::RichText::new("PASS: All Text Clears WCAG AA (>= 4.5:1)")
                                        .font(crate::theme::font(11.0))
                                        .color(theme.bit_perfect_green),
                                );
                            }
                            Err(ref err) => {
                                ui.label(
                                    egui::RichText::new(format!("Warning: {}", err))
                                        .font(crate::theme::font(11.0))
                                        .color(theme.warning_yellow),
                                );
                            }
                        }

                        ui.add_space(theme.space_xs);

                        if ui
                            .button(
                                egui::RichText::new("Auto-Fix Contrast")
                                    .font(crate::theme::font(11.5))
                                    .color(theme.info),
                            )
                            .on_hover_text("Automatically adjusts luminance so all text tokens pass WCAG AA standards")
                            .clicked()
                        {
                            state.auto_fix_contrast();
                        }
                    });
                });

                ui.add_space(theme.space_lg);

                // Save / Export Actions
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Save as Custom Theme")
                                    .font(crate::theme::font(12.0))
                                    .strong()
                                    .color(Color32::WHITE),
                            )
                            .fill(theme.accent)
                            .corner_radius(CornerRadius::same(theme.radius_md))
                            .min_size(Vec2::new(160.0, 32.0)),
                        )
                        .clicked()
                    {
                        if let Err(e) = validate_contrast(&state.colors) {
                            state.status_message = Some((format!("Cannot save: {e}"), true));
                        } else {
                            // Serialize to ThemeFile
                            let filename = format!("{}.shadowtheme", state.id.trim());
                            let _ = std::fs::create_dir_all(&manager.custom_themes_dir);
                            let target_path = manager.custom_themes_dir.join(&filename);

                            let hex_colors: HashMap<String, String> = state
                                .colors
                                .iter()
                                .map(|(k, v)| (k.clone(), format_hex_color(*v)))
                                .collect();

                            let theme_file = ThemeFile {
                                schema_version: 2,
                                metadata: Some(super::schema::ThemeMetadata {
                                    id: state.id.trim().to_string(),
                                    name: state.name.trim().to_string(),
                                    author: state.author.trim().to_string(),
                                    description: state.description.trim().to_string(),
                                    version: "2.0.0".to_string(),
                                    is_light: state.is_light,
                                    tags: vec!["custom".to_string()],
                                }),
                                id: Some(state.id.trim().to_string()),
                                name: Some(state.name.clone()),
                                author: Some(state.author.clone()),
                                description: Some(state.description.clone()),
                                is_light: Some(state.is_light),
                                colors: hex_colors,
                                radii: state.radii.clone(),
                                fonts: state.fonts.clone(),
                            };

                            match serde_json::to_string_pretty(&theme_file) {
                                Ok(json) => match std::fs::write(&target_path, json) {
                                    Ok(_) => {
                                        state.status_message = Some((
                                            format!("Saved successfully to {:?}", target_path),
                                            false,
                                        ));
                                        actions.theme_saved = Some(state.id.clone());
                                    }
                                    Err(e) => {
                                        state.status_message =
                                            Some((format!("Write error: {e}"), true));
                                    }
                                },
                                Err(e) => {
                                    state.status_message =
                                        Some((format!("JSON serialization error: {e}"), true));
                                }
                            }
                        }
                    }
                });
            });
        });

    actions
}

fn font_role_input(
    ui: &mut egui::Ui,
    value: &mut Option<String>,
    label: &str,
    hint: &str,
    theme: &Theme,
) {
    ui.add_space(theme.space_sm);
    ui.label(
        egui::RichText::new(label)
            .font(crate::theme::font(10.5))
            .color(theme.text_muted),
    );
    let mut text = value.clone().unwrap_or_default();
    let response = ui.add(
        egui::TextEdit::singleline(&mut text)
            .hint_text(hint)
            .desired_width(112.0),
    );
    if response.changed() {
        *value = (!text.trim().is_empty()).then(|| text.trim().to_string());
    }
}

fn color_picker_row(
    ui: &mut egui::Ui,
    state: &mut ThemeEditorState,
    token: &str,
    label: &str,
    theme: &Theme,
) {
    if let Some(color) = state.colors.get_mut(token) {
        ui.horizontal(|ui| {
            let mut srgba = [color.r(), color.g(), color.b(), color.a()];
            if ui
                .color_edit_button_srgba_unmultiplied(&mut srgba)
                .changed()
            {
                *color = Color32::from_rgba_unmultiplied(srgba[0], srgba[1], srgba[2], srgba[3]);
            }
            ui.label(
                egui::RichText::new(label)
                    .font(crate::theme::font(11.0))
                    .color(theme.text_secondary),
            );
        });
    }
}
