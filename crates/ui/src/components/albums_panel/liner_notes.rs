// Markdown liner-notes card for the album detail view.
//
// Moved from the albums panel so the panel stays under the size budget;
// it only depends on the notes state, the repository, and Markdown parsing.
use database::AlbumNotesRepo;
use domain::ids::AlbumId;
use eframe::egui::{self, Color32, CornerRadius, Stroke, Ui};
use media_tools::markdown::{Block, Inline};

use crate::state::AlbumNotesState;
use crate::theme::Theme;

/// The Markdown liner-notes card in the album detail view.
pub(crate) fn render_liner_notes(
    ui: &mut Ui,
    notes: &mut AlbumNotesState,
    repo: &AlbumNotesRepo,
    title: &str,
    artist: &str,
    theme: &Theme,
) {
    let album_id = AlbumId::from_identity(title, artist);
    notes.sync(album_id, title, artist, repo);

    egui::Frame::NONE
        .fill(theme.bg_card)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(theme.radius_lg))
        .inner_margin(egui::Margin::same(theme.space_3xl as i8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Liner Notes")
                        .font(crate::theme::font(14.0))
                        .strong()
                        .color(theme.text_primary),
                );
                if !notes.editing && notes.has_saved_notes() {
                    ui.label(
                        egui::RichText::new("Markdown")
                            .font(crate::theme::font(10.0))
                            .color(theme.text_dim),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if notes.editing {
                        if ui.button("Cancel").clicked() {
                            notes.cancel();
                        }
                        if ui
                            .add_enabled(
                                notes.is_dirty(),
                                egui::Button::new(
                                    egui::RichText::new("Save Notes")
                                        .color(Color32::WHITE)
                                        .strong(),
                                )
                                .fill(theme.accent),
                            )
                            .clicked()
                        {
                            notes.save(repo);
                        }
                    } else if ui
                        .button(if notes.has_saved_notes() {
                            "Edit Notes"
                        } else {
                            "Add Notes"
                        })
                        .clicked()
                    {
                        notes.begin_edit();
                    }
                });
            });

            ui.add_space(theme.space_sm);

            if let Some(error) = &notes.error {
                ui.label(
                    egui::RichText::new(error)
                        .font(crate::theme::font(11.0))
                        .color(theme.destructive),
                );
                ui.add_space(theme.space_xs);
            } else if notes.just_saved {
                ui.label(
                    egui::RichText::new("Liner notes saved")
                        .font(crate::theme::font(10.5))
                        .color(theme.bit_perfect_green),
                );
                ui.add_space(theme.space_xs);
            }

            if notes.editing {
                ui.add(
                    egui::TextEdit::multiline(&mut notes.draft)
                        .desired_rows(7)
                        .desired_width(f32::INFINITY)
                        .hint_text("Write about this album… Markdown: # heading, **bold**, *italic*, - list, [link](https://…)")
                        .font(crate::theme::font(12.0)),
                );
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} / {} characters",
                            notes.draft.chars().count(),
                            media_tools::markdown::MAX_NOTE_BYTES
                        ))
                        .font(crate::theme::font(10.0))
                        .color(theme.text_dim),
                    );
                    if notes.draft.trim().is_empty() {
                        ui.label(
                            egui::RichText::new("Saving an empty note deletes it")
                                .font(crate::theme::font(10.0))
                                .color(theme.text_muted),
                        );
                    }
                });
            } else if notes.draft.trim().is_empty() {
                ui.label(
                    egui::RichText::new("No liner notes yet for this album.")
                        .font(crate::theme::font(11.5))
                        .color(theme.text_muted),
                );
            } else {
                let blocks = media_tools::markdown::parse(&notes.draft);
                render_markdown_blocks(ui, &blocks, theme);
            }
        });
}

/// Paint the Markdown subset the liner-notes parser understands.
fn render_markdown_blocks(ui: &mut Ui, blocks: &[Block], theme: &Theme) {
    for block in blocks {
        match block {
            Block::Heading { level, content } => {
                let size = match level {
                    1 => 18.0,
                    2 => 15.5,
                    3 => 14.0,
                    _ => 12.5,
                };
                render_inline(ui, content, theme, size, true, theme.text_primary);
            }
            Block::Paragraph { content } => {
                render_inline(ui, content, theme, 12.0, false, theme.text_secondary);
            }
            Block::Quote { blocks } => {
                egui::Frame::NONE
                    .fill(theme.bg_pill)
                    .corner_radius(CornerRadius::same(theme.radius_sm))
                    .inner_margin(egui::Margin {
                        left: theme.space_lg as i8,
                        right: theme.space_lg as i8,
                        top: theme.space_sm as i8,
                        bottom: theme.space_sm as i8,
                    })
                    .show(ui, |ui| {
                        render_markdown_blocks(ui, blocks, theme);
                    });
            }
            Block::List {
                ordered,
                start,
                items,
            } => {
                for (index, item) in items.iter().enumerate() {
                    ui.horizontal_wrapped(|ui| {
                        let marker = if *ordered {
                            format!("{}.", start + index as u64)
                        } else {
                            "•".to_string()
                        };
                        ui.label(
                            egui::RichText::new(marker)
                                .font(crate::theme::font(12.0))
                                .color(theme.text_muted),
                        );
                        render_inline(ui, item, theme, 12.0, false, theme.text_secondary);
                    });
                }
            }
            Block::Code { language, text } => {
                egui::Frame::NONE
                    .fill(theme.bg_pill)
                    .corner_radius(CornerRadius::same(theme.radius_sm))
                    .inner_margin(egui::Margin::same(theme.space_lg as i8))
                    .show(ui, |ui| {
                        if let Some(language) = language {
                            ui.label(
                                egui::RichText::new(language)
                                    .font(crate::theme::font(9.5))
                                    .color(theme.text_dim),
                            );
                        }
                        ui.label(
                            egui::RichText::new(text)
                                .font(crate::theme::mono_font(11.0))
                                .color(theme.text_primary),
                        );
                    });
            }
            Block::Rule => {
                ui.separator();
            }
        }
        ui.add_space(theme.space_sm);
    }
}

fn render_inline(
    ui: &mut Ui,
    content: &[Inline],
    theme: &Theme,
    size: f32,
    heading: bool,
    base_color: Color32,
) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for inline in content {
            match inline {
                Inline::Text { text } => {
                    let mut text = egui::RichText::new(text)
                        .font(crate::theme::font(size))
                        .color(base_color);
                    if heading {
                        text = text.strong();
                    }
                    ui.label(text);
                }
                Inline::Emphasis { text } => {
                    ui.label(
                        egui::RichText::new(text)
                            .font(crate::theme::font(size))
                            .italics()
                            .color(base_color),
                    );
                }
                Inline::Strong { text } => {
                    ui.label(
                        egui::RichText::new(text)
                            .font(crate::theme::font(size))
                            .strong()
                            .color(if heading {
                                theme.accent_highlight
                            } else {
                                base_color
                            }),
                    );
                }
                Inline::Code { text } => {
                    ui.label(
                        egui::RichText::new(text)
                            .font(crate::theme::mono_font(size - 0.5))
                            .background_color(theme.bg_pill)
                            .color(theme.text_primary),
                    );
                }
                Inline::Link { text, url } => {
                    ui.hyperlink_to(
                        egui::RichText::new(text)
                            .font(crate::theme::font(size))
                            .color(theme.accent_highlight),
                        url,
                    );
                }
            }
        }
    });
}
