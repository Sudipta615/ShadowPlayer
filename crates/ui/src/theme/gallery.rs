//! The theme gallery: a responsive wall of compact, live previews.
//!
//! Cards are sized from the width the gallery actually has, so a narrow
//! Settings pane gets two columns and a wide monitor gets more columns rather
//! than five oversized cards. Each card is a small optical sample of its own
//! theme (app background, accent rail, swatches and transport silhouette)
//! rather than a wall of metadata.

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, Layout, Margin, Pos2, Rect, Sense, Stroke, Vec2,
};

use super::manager::{ThemeCardInfo, ThemeManager};
use super::tokens::{with_alpha, PlayButtonShape, Theme};

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

// Keep the wall responsive without ever measuring a card wider than the pane
// can hold: choose the largest number of columns that still respects the
// minimum card width, then cap the card at the maximum. A narrow pane may
// leave a little unused space rather than shrinking controls below their hit
// target or overflowing horizontally.
const MIN_CARD_W: f32 = 174.0;
const MAX_CARD_W: f32 = 260.0;
const MAX_COLUMNS: usize = 12;

fn gallery_columns(avail_w: f32, gap: f32) -> (usize, f32) {
    let cols = (((avail_w + gap) / (MIN_CARD_W + gap)).floor() as usize).clamp(1, MAX_COLUMNS);
    let card_w = (avail_w - gap * (cols - 1) as f32) / cols as f32;
    (cols, card_w.min(MAX_CARD_W))
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
    pub cancel_preview: bool,
    pub reload_themes: bool,
    pub import_theme: bool,
    pub open_editor: Option<String>,
}

/// Render the Theme Gallery: search, filters, an active-preview strip, and a
/// responsive card wall.
pub fn render_theme_gallery(
    ui: &mut egui::Ui,
    state: &mut ThemeGalleryState,
    manager: &ThemeManager,
    theme: &Theme,
) -> GalleryActions {
    let mut actions = GalleryActions::default();

    crate::components::search_field::search_field_with_width(
        ui,
        &mut state.search_query,
        "Search themes, authors, or tags…",
        theme,
        crate::components::search_field::SEARCH_MAX_WIDTH,
    );

    ui.add_space(theme.space_sm);

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_xs, theme.space_xs);
        for category in ThemeFilter::ALL {
            let selected = state.filter == category;
            let response = ui.add(
                egui::Button::new(
                    egui::RichText::new(category.label())
                        .font(crate::theme::font(11.0))
                        .color(if selected {
                            Color32::WHITE
                        } else {
                            theme.text_secondary
                        }),
                )
                .fill(if selected {
                    theme.accent
                } else {
                    theme.bg_pill
                })
                .stroke(Stroke::new(
                    1.0,
                    if selected {
                        theme.accent_highlight
                    } else {
                        theme.border_subtle
                    },
                ))
                .corner_radius(CornerRadius::same(theme.radius_sm))
                .min_size(Vec2::new(0.0, 24.0)),
            );
            if response.clicked() {
                state.filter = category;
            }
        }

        // Import and Reload belong to the gallery as a whole, so keep them at
        // the trailing edge of the filter row instead of scattering them
        // between the category chips. On a narrow pane the group wraps onto
        // its own right-aligned row rather than overflowing.
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xs, theme.space_xs);

            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("Reload")
                            .font(crate::theme::font(11.0))
                            .color(theme.text_secondary),
                    )
                    .fill(theme.bg_pill)
                    .stroke(Stroke::new(1.0, theme.border_subtle))
                    .corner_radius(CornerRadius::same(theme.radius_sm))
                    .min_size(Vec2::new(0.0, 24.0)),
                )
                .on_hover_text("Rescan the themes folder")
                .clicked()
            {
                actions.reload_themes = true;
            }

            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("Import…")
                            .font(crate::theme::font(11.0))
                            .color(theme.text_secondary),
                    )
                    .fill(theme.bg_pill)
                    .stroke(Stroke::new(1.0, theme.border_subtle))
                    .corner_radius(CornerRadius::same(theme.radius_sm))
                    .min_size(Vec2::new(0.0, 24.0)),
                )
                .on_hover_text("Import a .shadowtheme file")
                .clicked()
            {
                actions.import_theme = true;
            }

            if manager.is_staged()
                && ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("Clear Preview")
                                .font(crate::theme::font(11.0))
                                .strong()
                                .color(theme.accent_warm),
                        )
                        .fill(theme.bg_pill)
                        .stroke(Stroke::new(1.0, theme.accent_warm))
                        .corner_radius(CornerRadius::same(theme.radius_sm))
                        .min_size(Vec2::new(0.0, 24.0)),
                    )
                    .on_hover_text("Return to the active theme without applying anything")
                    .clicked()
            {
                actions.cancel_preview = true;
            }
        });
    });

    ui.add_space(theme.space_sm);

    // Filter cards.
    let all_cards = manager.all_theme_cards();
    let query = state.search_query.trim().to_lowercase();
    let filtered_cards: Vec<ThemeCardInfo> = all_cards
        .into_iter()
        .filter(|card| {
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

    // A staged theme gets its own strip above the wall: the global "apply" and
    // "clear" actions should not require finding the card again.
    if let Some(staged) = filtered_cards.iter().find(|card| card.is_staged) {
        render_preview_strip(ui, staged, theme, &mut actions);
        ui.add_space(theme.space_sm);
    }

    if filtered_cards.is_empty() {
        ui.add_space(theme.space_lg);
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new("No themes match the selected filter.")
                    .font(crate::theme::font(12.0))
                    .color(theme.text_muted),
            );
            let filters_active = !state.search_query.is_empty() || state.filter != ThemeFilter::All;
            if filters_active
                && ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("Reset filters")
                                .font(crate::theme::font(11.0))
                                .strong()
                                .color(theme.accent_highlight),
                        )
                        .fill(theme.bg_pill)
                        .stroke(Stroke::new(1.0, theme.border_subtle))
                        .corner_radius(CornerRadius::same(theme.radius_sm))
                        .min_size(Vec2::new(0.0, 24.0)),
                    )
                    .clicked()
            {
                state.search_query.clear();
                state.filter = ThemeFilter::All;
            }
        });
        return actions;
    }

    let gap = theme.space_sm;
    let (cols, card_w) = gallery_columns(ui.available_width(), gap);

    for row in filtered_cards.chunks(cols) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(gap, 0.0);
            for card in row {
                ui.vertical(|ui| {
                    ui.set_width(card_w);
                    render_theme_card(ui, card, card_w, theme, &mut actions);
                });
            }
        });
        ui.add_space(gap);
    }

    actions
}

fn render_preview_strip(
    ui: &mut egui::Ui,
    card: &ThemeCardInfo,
    theme: &Theme,
    actions: &mut GalleryActions,
) {
    egui::Frame::NONE
        .fill(with_alpha(card.accent_warm, 18))
        .stroke(Stroke::new(1.0, with_alpha(card.accent_warm, 150)))
        .corner_radius(CornerRadius::same(theme.radius_md))
        .inner_margin(Margin::symmetric(
            theme.space_md as i8,
            theme.space_xs as i8,
        ))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (dot, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                ui.painter()
                    .circle_filled(dot.center(), 4.0, card.accent_warm);
                ui.label(
                    egui::RichText::new(format!("Previewing {}", card.name))
                        .font(crate::theme::font(11.5))
                        .strong()
                        .color(theme.text_primary),
                );
                ui.label(
                    egui::RichText::new("· not applied")
                        .font(crate::theme::font(10.5))
                        .color(theme.text_muted),
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Clear Preview")
                                    .font(crate::theme::font(10.5))
                                    .color(theme.accent_warm),
                            )
                            .fill(theme.bg_pill)
                            .stroke(Stroke::new(1.0, theme.border_subtle))
                            .corner_radius(CornerRadius::same(theme.radius_sm))
                            .min_size(Vec2::new(0.0, 22.0)),
                        )
                        .clicked()
                    {
                        actions.cancel_preview = true;
                    }
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Apply")
                                    .font(crate::theme::font(10.5))
                                    .strong()
                                    .color(Color32::WHITE),
                            )
                            .fill(card.accent)
                            .corner_radius(CornerRadius::same(theme.radius_sm))
                            .min_size(Vec2::new(0.0, 22.0)),
                        )
                        .clicked()
                    {
                        actions.apply_theme = Some(card.id.clone());
                    }
                });
            });
        });
}

fn render_theme_card(
    ui: &mut egui::Ui,
    card: &ThemeCardInfo,
    card_w: f32,
    host: &Theme,
    actions: &mut GalleryActions,
) {
    let border = if card.is_active {
        card.accent
    } else if card.is_staged {
        card.accent_warm
    } else {
        card.border_card
    };
    let fill = card.bg_card;

    let frame = egui::Frame::NONE
        .fill(fill)
        .stroke(Stroke::new(if card.is_active { 1.6 } else { 1.0 }, border))
        .corner_radius(CornerRadius::same(host.radius_md))
        .shadow(egui::epaint::Shadow {
            offset: [0, 2],
            blur: 8,
            spread: 0,
            color: with_alpha(host.shadow, 46),
        })
        .inner_margin(Margin::same(host.space_sm as i8));

    let response = frame.show(ui, |ui| {
        ui.set_width(card_w - host.space_sm * 2.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, host.space_xxs);

            render_card_preview(ui, card, host);

            ui.add_space(host.space_xs);

            // Name + state badge.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(host.space_xs, 0.0);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&card.name)
                            .font(crate::theme::font(12.5))
                            .strong()
                            .color(card.text_primary),
                    )
                    .truncate(),
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if card.is_active {
                        status_badge(ui, "ACTIVE", card.accent, host);
                    } else if card.is_staged {
                        status_badge(ui, "PREVIEW", card.accent_warm, host);
                    } else {
                        let mode = if card.tags.iter().any(|t| t == "oled") {
                            "OLED"
                        } else if card.is_light {
                            "LIGHT"
                        } else {
                            "DARK"
                        };
                        status_badge(ui, mode, card.bg_pill, host);
                    }
                });
            });

            // One compact provenance line.
            let mut meta = String::new();
            if !card.author.is_empty() {
                meta.push_str(&card.author);
            }
            if !card.version.is_empty() && card.version != "built-in" {
                if !meta.is_empty() {
                    meta.push_str(" · ");
                }
                meta.push_str(&card.version);
            }
            if meta.is_empty() {
                meta.push_str("Built-in");
            } else {
                meta.push_str(" · ");
                meta.push_str(card.play_button_shape.label());
            }
            ui.add(
                egui::Label::new(
                    egui::RichText::new(meta)
                        .font(crate::theme::font(10.5))
                        .color(card.text_muted),
                )
                .truncate(),
            );

            if !card.description.is_empty() {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&card.description)
                            .font(crate::theme::font(10.5))
                            .color(card.text_secondary),
                    )
                    .truncate(),
                );
            }

            ui.add_space(host.space_xs);

            // Actions: preview / clear and apply on the left, edit on the
            // right. The active card already carries the ACTIVE badge; a
            // second "✓ Active" label and a no-op Preview only crowded the
            // row, so an active card offers Edit alone.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(host.space_xs, 0.0);
                ui.spacing_mut().button_padding = Vec2::new(6.0, 3.0);

                if !card.is_active {
                    let preview_label = if card.is_staged { "Clear" } else { "Preview" };
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(preview_label)
                                    .font(crate::theme::font(10.5))
                                    .color(if card.is_staged {
                                        card.accent_warm
                                    } else {
                                        card.text_secondary
                                    }),
                            )
                            .fill(card.bg_pill)
                            .stroke(Stroke::new(1.0, card.border_subtle))
                            .corner_radius(CornerRadius::same(host.radius_sm))
                            .min_size(Vec2::new(0.0, 22.0)),
                        )
                        .on_hover_text(if card.is_staged {
                            "Return to the active theme"
                        } else {
                            "Preview without applying"
                        })
                        .clicked()
                    {
                        if card.is_staged {
                            actions.cancel_preview = true;
                        } else {
                            actions.preview_theme = Some(card.id.clone());
                        }
                    }

                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Apply")
                                    .font(crate::theme::font(10.5))
                                    .strong()
                                    .color(contrast_text_for(card.accent)),
                            )
                            .fill(card.accent)
                            .corner_radius(CornerRadius::same(host.radius_sm))
                            .min_size(Vec2::new(0.0, 22.0)),
                        )
                        .on_hover_text("Apply this theme")
                        .clicked()
                    {
                        actions.apply_theme = Some(card.id.clone());
                    }
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Edit")
                                    .font(crate::theme::font(10.5))
                                    .color(card.text_secondary),
                            )
                            .fill(Color32::TRANSPARENT)
                            .frame(false)
                            .min_size(Vec2::new(0.0, 22.0)),
                        )
                        .on_hover_text("Clone this theme into the editor")
                        .clicked()
                    {
                        actions.open_editor = Some(card.id.clone());
                    }
                });
            });
        });
    });

    let card_rect = response.response.rect;
    if !card.is_active {
        let hover_t = ui.ctx().animate_bool_with_time(
            response.response.id.with("theme_card_hover"),
            response.response.hovered(),
            host.motion_time(0.12),
        );
        if hover_t > 0.0 {
            ui.painter().rect_filled(
                card_rect,
                CornerRadius::same(host.radius_md),
                with_alpha(Color32::WHITE, (hover_t * 7.0) as u8),
            );
            ui.painter().rect_stroke(
                card_rect,
                CornerRadius::same(host.radius_md),
                Stroke::new(1.0, with_alpha(card.accent, (hover_t * 110.0) as u8)),
                egui::StrokeKind::Inside,
            );
        }
    }

    if host.frosted {
        crate::theme::glass::paint_glass_sheen(
            ui,
            card_rect,
            CornerRadius::same(host.radius_md),
            host,
        );
    }
}

fn render_card_preview(ui: &mut egui::Ui, card: &ThemeCardInfo, host: &Theme) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 46.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(host.radius_sm), card.bg_app);

    // An accent rail and the surface sample: the two colours that carry most
    // of a theme's identity.
    let rail = Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height()));
    painter.rect_filled(rail, CornerRadius::same(host.radius_xs), card.accent);
    let inner = Rect::from_min_max(
        Pos2::new(rect.left() + 9.0, rect.top() + 6.0),
        Pos2::new(rect.right() - 7.0, rect.bottom() - 6.0),
    );
    painter.rect_filled(
        inner,
        CornerRadius::same(host.radius_xs),
        with_alpha(card.bg_card, 235),
    );
    painter.rect_stroke(
        inner,
        CornerRadius::same(host.radius_xs),
        Stroke::new(1.0, with_alpha(card.border_card, 180)),
        egui::StrokeKind::Inside,
    );

    // Swatches along the left of the sample.
    for (index, color) in [card.accent, card.accent_highlight, card.accent_warm]
        .into_iter()
        .enumerate()
    {
        painter.circle_filled(
            Pos2::new(inner.left() + 12.0, inner.top() + 8.0 + index as f32 * 9.0),
            3.3,
            color,
        );
    }

    // "Aa" type sample.
    painter.text(
        Pos2::new(inner.left() + 23.0, inner.center().y),
        Align2::LEFT_CENTER,
        "Aa",
        crate::theme::font(16.0),
        card.text_primary,
    );

    // Transport silhouette, so the card says something about personality.
    draw_shape_swatch(
        &painter,
        Rect::from_center_size(
            Pos2::new(inner.right() - 22.0, inner.center().y),
            Vec2::new(30.0, 20.0),
        ),
        card.play_button_shape,
        card.accent,
        card.accent_highlight,
    );

    // A hairline top sheen makes the sample read as a surface, not a diagram.
    painter.line_segment(
        [
            Pos2::new(inner.left() + 2.0, inner.top() + 1.0),
            Pos2::new(inner.right() - 2.0, inner.top() + 1.0),
        ],
        Stroke::new(1.0, with_alpha(Color32::WHITE, 32)),
    );
}

fn draw_shape_swatch(
    painter: &egui::Painter,
    rect: Rect,
    shape: PlayButtonShape,
    fill: Color32,
    edge: Color32,
) {
    let radius = match shape {
        PlayButtonShape::Circle | PlayButtonShape::Pill => CornerRadius::same(10),
        PlayButtonShape::Rounded => CornerRadius::same(6),
        PlayButtonShape::Square => CornerRadius::same(1),
    };
    let button = match shape {
        PlayButtonShape::Circle => Rect::from_center_size(rect.center(), Vec2::splat(18.0)),
        PlayButtonShape::Pill => Rect::from_center_size(rect.center(), Vec2::new(30.0, 18.0)),
        PlayButtonShape::Rounded => Rect::from_center_size(rect.center(), Vec2::new(26.0, 18.0)),
        PlayButtonShape::Square => Rect::from_center_size(rect.center(), Vec2::new(24.0, 18.0)),
    };
    painter.rect_filled(button, radius, fill);
    painter.rect_stroke(
        button,
        radius,
        Stroke::new(1.0, with_alpha(edge, 180)),
        egui::StrokeKind::Inside,
    );
    painter.circle_filled(button.center(), 2.6, Color32::WHITE);
}

fn status_badge(ui: &mut egui::Ui, text: &str, fill: Color32, host: &Theme) {
    egui::Frame::NONE
        .fill(fill)
        .corner_radius(CornerRadius::same(host.radius_xs))
        .inner_margin(Margin::symmetric(4, 1))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text)
                    .font(crate::theme::mono_font(8.5))
                    .strong()
                    .color(contrast_text_for(fill)),
            );
        });
}

fn contrast_text_for(bg: Color32) -> Color32 {
    if super::contrast::luminance(bg) > 0.35 {
        Color32::from_rgb(15, 23, 42)
    } else {
        Color32::WHITE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responsive_columns_never_exceed_the_pane() {
        for avail in [180.0, 240.0, 270.0, 300.0, 360.0, 610.0, 1430.0, 2900.0] {
            let (cols, card_w) = gallery_columns(avail, 8.0);
            let used = card_w * cols as f32 + 8.0 * (cols - 1) as f32;
            assert!(
                used <= avail + 0.01,
                "{cols} columns of {card_w} px overflow a {avail} px pane"
            );
            assert!(card_w <= MAX_CARD_W);
        }
    }
}
