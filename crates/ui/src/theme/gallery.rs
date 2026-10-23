use eframe::egui::{
    self, Align, Color32, CornerRadius, Layout, Margin, Sense, Stroke, Vec2,
};

use super::manager::{ThemeCardInfo, ThemeManager};
use super::tokens::Theme;

/// Category filter options for browsing the theme gallery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeFilter {
    #[default]
    All,
    Dark,
    Light,
    Oled,
    Custom,
}

impl ThemeFilter {
    pub const ALL: [ThemeFilter; 5] = [
        ThemeFilter::All,
        ThemeFilter::Dark,
        ThemeFilter::Light,
        ThemeFilter::Oled,
        ThemeFilter::Custom,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Dark => "Dark",
            Self::Light => "Light",
            Self::Oled => "OLED",
            Self::Custom => "Custom",
        }
    }
}

/// State maintained across frames for the Theme Gallery UI.
#[derive(Debug, Clone, Default)]
pub struct ThemeGalleryState {
    pub search_query: String,
    pub filter: ThemeFilter,
}

/// Actions emitted by the Theme Gallery.
#[derive(Default, Clone, Debug)]
pub struct GalleryActions {
    pub preview_theme: Option<String>,
    pub apply_theme: Option<String>,
    pub reload_themes: bool,
    pub import_theme: bool,
    pub open_custom_themes_folder: bool,
    pub open_editor: Option<String>,
}

/// Render the Theme Gallery: filter bar, search, and responsive cards.
pub fn render_theme_gallery(
    ui: &mut egui::Ui,
    state: &mut ThemeGalleryState,
    manager: &ThemeManager,
    theme: &Theme,
) -> GalleryActions {
    let mut actions = GalleryActions::default();

    ui.vertical(|ui| {
        // 1. Controls bar: Search + Category filter chips + Action buttons
        ui.horizontal(|ui| {
            // Search input
            ui.add(
                egui::TextEdit::singleline(&mut state.search_query)
                    .hint_text(
                        egui::RichText::new("Search themes...")
                            .color(theme.text_muted)
                            .font(crate::theme::font(12.0)),
                    )
                    .desired_width(180.0)
                    .margin(Margin::symmetric(
                        theme.space_sm as i8,
                        theme.space_xs as i8,
                    )),
            );

            ui.add_space(theme.space_md);

            // Filter chips
            for category in ThemeFilter::ALL {
                let selected = state.filter == category;
                let text = egui::RichText::new(category.label()).font(crate::theme::font(11.5));
                let resp = if selected {
                    ui.add(
                        egui::Button::new(text.color(Color32::WHITE))
                            .fill(theme.accent)
                            .corner_radius(CornerRadius::same(theme.radius_pill)),
                    )
                } else {
                    ui.add(
                        egui::Button::new(text.color(theme.text_secondary))
                            .fill(theme.bg_pill)
                            .corner_radius(CornerRadius::same(theme.radius_pill)),
                    )
                };
                if resp.clicked() {
                    state.filter = category;
                }
            }

            // Action buttons on the right: Import Theme, Open Folder, Reload
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("Import Theme…")
                                .font(crate::theme::font(11.5))
                                .color(Color32::WHITE),
                        )
                        .fill(theme.accent)
                        .corner_radius(CornerRadius::same(theme.radius_sm)),
                    )
                    .on_hover_text("Open native file chooser to pick and import .shadowtheme files")
                    .clicked()
                {
                    actions.import_theme = true;
                }

                if ui
                    .button(
                        egui::RichText::new("Open Folder")
                            .font(crate::theme::font(11.5))
                            .color(theme.text_secondary),
                    )
                    .on_hover_text("Open custom-theme directory in your system file manager")
                    .clicked()
                {
                    actions.open_custom_themes_folder = true;
                }

                if ui
                    .button(
                        egui::RichText::new("Reload")
                            .font(crate::theme::font(11.5))
                            .color(theme.info),
                    )
                    .on_hover_text("Rescan themes directory")
                    .clicked()
                {
                    actions.reload_themes = true;
                }
            });
        });

        ui.add_space(theme.space_md);

        // 2. Filter theme cards
        let all_cards = manager.all_theme_cards();
        let query = state.search_query.trim().to_lowercase();

        let filtered_cards: Vec<ThemeCardInfo> = all_cards
            .into_iter()
            .filter(|card| {
                // Category filter
                let matches_filter = match state.filter {
                    ThemeFilter::All => true,
                    ThemeFilter::Dark => !card.is_light && !card.tags.iter().any(|t| t == "oled"),
                    ThemeFilter::Light => card.is_light,
                    ThemeFilter::Oled => card.tags.iter().any(|t| t == "oled"),
                    ThemeFilter::Custom => !card.is_system,
                };
                if !matches_filter {
                    return false;
                }

                // Search query
                if query.is_empty() {
                    true
                } else {
                    card.name.to_lowercase().contains(&query)
                        || card.description.to_lowercase().contains(&query)
                        || card.author.to_lowercase().contains(&query)
                        || card.tags.iter().any(|t| t.to_lowercase().contains(&query))
                }
            })
            .collect();

        if filtered_cards.is_empty() {
            ui.add_space(theme.space_lg);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("No themes match the selected filter.")
                        .font(crate::theme::font(12.0))
                        .color(theme.text_muted),
                );
                if state.filter == ThemeFilter::Custom
                    && ui
                        .button(
                            egui::RichText::new("Import a .shadowtheme file…")
                                .font(crate::theme::font(12.0))
                                .color(theme.accent),
                        )
                        .clicked()
                {
                    actions.import_theme = true;
                }
            });
            return;
        }

        // 3. Render grid of cards
        let avail_w = ui.available_width();
        let card_w = 220.0_f32;
        let spacing = theme.space_md;
        let cols = ((avail_w + spacing) / (card_w + spacing)).floor().max(1.0) as usize;

        egui::Grid::new("theme_gallery_grid")
            .spacing(Vec2::new(spacing, spacing))
            .show(ui, |ui| {
                for (idx, card) in filtered_cards.iter().enumerate() {
                    render_theme_card(ui, card, theme, &mut actions);
                    if (idx + 1) % cols == 0 {
                        ui.end_row();
                    }
                }
            });
    });

    actions
}

fn render_theme_card(
    ui: &mut egui::Ui,
    card: &ThemeCardInfo,
    theme: &Theme,
    actions: &mut GalleryActions,
) {
    let card_w = 220.0_f32;
    let card_bg = if card.is_active {
        theme.bg_card_active
    } else {
        theme.bg_card
    };
    let card_border = if card.is_active {
        theme.border_active_row
    } else if card.is_staged {
        theme.accent_highlight
    } else {
        theme.border_card
    };

    egui::Frame::NONE
        .fill(card_bg)
        .stroke(Stroke::new(1.0, card_border))
        .corner_radius(CornerRadius::same(theme.radius_md))
        .inner_margin(Margin::same(theme.space_md as i8))
        .show(ui, |ui| {
            ui.set_width(card_w);
            ui.vertical(|ui| {
                // Header: Name + Badge
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(&card.name)
                            .font(crate::theme::font(13.0))
                            .strong()
                            .color(theme.text_primary),
                    );

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if card.is_active {
                            let badge = egui::Frame::NONE
                                .fill(theme.accent)
                                .corner_radius(CornerRadius::same(theme.radius_xs))
                                .inner_margin(Margin::symmetric(
                                    theme.space_xs as i8,
                                    theme.space_xxs as i8,
                                ));
                            badge.show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new("ACTIVE")
                                        .font(crate::theme::mono_font(8.5))
                                        .strong()
                                        .color(Color32::WHITE),
                                );
                            });
                        } else if card.is_staged {
                            let badge = egui::Frame::NONE
                                .fill(theme.accent_warm)
                                .corner_radius(CornerRadius::same(theme.radius_xs))
                                .inner_margin(Margin::symmetric(
                                    theme.space_xs as i8,
                                    theme.space_xxs as i8,
                                ));
                            badge.show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new("STAGED")
                                        .font(crate::theme::mono_font(8.5))
                                        .strong()
                                        .color(Color32::WHITE),
                                );
                            });
                        }
                    });
                });

                // Author & Version
                let mut meta_text = String::new();
                if !card.author.is_empty() {
                    meta_text.push_str(&format!("by {}", card.author));
                }
                if !card.version.is_empty() {
                    let v_str = if card.version == "built-in" {
                        "built-in".to_string()
                    } else if card.version.starts_with('v') {
                        card.version.clone()
                    } else {
                        format!("v{}", card.version)
                    };
                    if !meta_text.is_empty() {
                        meta_text.push_str(" • ");
                    }
                    meta_text.push_str(&v_str);
                }
                if !meta_text.is_empty() {
                    ui.label(
                        egui::RichText::new(meta_text)
                            .font(crate::theme::font(10.5))
                            .color(theme.text_dim),
                    );
                }

                ui.add_space(theme.space_xs);

                // Palette Swatches (4 dots)
                ui.horizontal(|ui| {
                    for color in card.preview_colors {
                        let (dot_rect, _) =
                            ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                        let p = ui.painter_at(dot_rect);
                        p.circle_filled(dot_rect.center(), 6.0, color);
                        p.circle_stroke(
                            dot_rect.center(),
                            6.0,
                            Stroke::new(1.0, theme.border_subtle),
                        );
                    }
                });

                ui.add_space(theme.space_xs);

                // Description
                if !card.description.is_empty() {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&card.description)
                                .font(crate::theme::font(10.5))
                                .color(theme.text_secondary),
                        )
                        .truncate(),
                    );
                }

                // Tags
                if !card.tags.is_empty() {
                    ui.add_space(theme.space_xxs);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(theme.space_xs, theme.space_xxs);
                        for tag in card.tags.iter().take(3) {
                            let badge = egui::Frame::NONE
                                .fill(theme.bg_pill)
                                .corner_radius(CornerRadius::same(theme.radius_pill))
                                .inner_margin(Margin::symmetric(
                                    theme.space_xs as i8,
                                    theme.space_xxs as i8,
                                ));
                            badge.show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(format!("#{tag}"))
                                        .font(crate::theme::mono_font(9.0))
                                        .color(theme.text_muted),
                                );
                            });
                        }
                    });
                }

                ui.add_space(theme.space_sm);

                // Action buttons: Preview, Apply
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Preview")
                                    .font(crate::theme::font(11.0))
                                    .color(theme.accent_highlight),
                            )
                            .fill(theme.bg_pill)
                            .corner_radius(CornerRadius::same(theme.radius_sm)),
                        )
                        .clicked()
                    {
                        actions.preview_theme = Some(card.id.clone());
                    }

                    if !card.is_active
                        && ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Apply")
                                        .font(crate::theme::font(11.0))
                                        .color(Color32::WHITE),
                                )
                                .fill(theme.accent)
                                .corner_radius(CornerRadius::same(theme.radius_sm)),
                            )
                            .clicked()
                    {
                        actions.apply_theme = Some(card.id.clone());
                    }
                });
            });
        });
}
