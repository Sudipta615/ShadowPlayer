//! Artist and album biography pane.
//!
//! The pane is deliberately small: it shows what the local cache or the
//! user's notes folder has, and turns into a text editor in place. Nothing is
//! fetched from the network; the source is the folder chosen in Settings.

use crate::state::BiographyEditorState;
use crate::theme::Theme;
use domain::models::Biography;
use eframe::egui::{self, CornerRadius, Stroke, Ui};

/// A change the pane wants the view owner to persist.
pub enum BiographyPaneRequest {
    Save {
        kind: String,
        name: String,
        text: String,
    },
    Delete {
        kind: String,
        name: String,
    },
}

pub struct BiographyPane<'a> {
    pub kind: &'a str,
    pub name: &'a str,
    pub biography: Option<&'a Biography>,
    pub editor: &'a mut BiographyEditorState,
    pub theme: &'a Theme,
}

impl<'a> BiographyPane<'a> {
    pub fn show(self, ui: &mut Ui) -> Option<BiographyPaneRequest> {
        let mut request = None;
        let editing = self.editor.is_editing(self.kind, self.name);

        egui::Frame::NONE
            .fill(self.theme.bg_card)
            .stroke(Stroke::new(1.0_f32, self.theme.border_card))
            .corner_radius(CornerRadius::same(self.theme.radius_lg))
            .inner_margin(egui::Margin::symmetric(
                self.theme.space_2xl as i8,
                self.theme.space_lg as i8,
            ))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(tr!("Biography"))
                            .font(crate::theme::font(11.0))
                            .strong()
                            .color(self.theme.accent_highlight),
                    );
                    if let Some(bio) = self.biography {
                        let source = if bio.source == "user" {
                            tr!("your notes")
                        } else if let Some(path) = bio.source.strip_prefix("local-file: ") {
                            path.to_string()
                        } else {
                            bio.source.clone()
                        };
                        ui.label(
                            egui::RichText::new(source)
                                .font(crate::theme::font(10.5))
                                .color(self.theme.text_dim),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if editing {
                            if ui
                                .add(pane_button(tr!("Save"), true, self.theme))
                                .clicked()
                            {
                                request = Some(BiographyPaneRequest::Save {
                                    kind: self.kind.to_string(),
                                    name: self.name.to_string(),
                                    text: self.editor.draft.clone(),
                                });
                            }
                            if ui
                                .add(pane_button(tr!("Cancel"), false, self.theme))
                                .clicked()
                            {
                                self.editor.close();
                            }
                        } else {
                            if ui
                                .add(pane_button(tr!("Edit"), false, self.theme))
                                .clicked()
                            {
                                self.editor.begin(
                                    self.kind,
                                    self.name,
                                    self.biography.map(|b| b.text.as_str()).unwrap_or(""),
                                );
                            }
                            if self.biography.is_some()
                                && ui
                                    .add(pane_button(tr!("Clear"), false, self.theme))
                                    .clicked()
                            {
                                request = Some(BiographyPaneRequest::Delete {
                                    kind: self.kind.to_string(),
                                    name: self.name.to_string(),
                                });
                            }
                        }
                    });
                });

                ui.add_space(4.0);

                if editing {
                    let response = ui.add(
                        egui::TextEdit::multiline(&mut self.editor.draft)
                            .desired_rows(5)
                            .desired_width(f32::INFINITY)
                            .hint_text(tr!(
                                "Notes about this artist or album. Saved locally, never uploaded."
                            ))
                            .font(crate::theme::font(12.5)),
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::TextInput,
                            true,
                            "Biography text",
                        )
                    });
                } else if let Some(bio) = self.biography {
                    egui::ScrollArea::vertical()
                        .id_salt(("biography", self.kind, self.name))
                        .max_height(120.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(&bio.text)
                                    .font(crate::theme::font(12.5))
                                    .color(self.theme.text_secondary),
                            );
                        });
                } else {
                    ui.label(
                        egui::RichText::new(tr!(
                            "No biography yet. Choose a notes folder in Settings › Library, or write one here."
                        ))
                        .font(crate::theme::font(12.0))
                        .color(self.theme.text_muted),
                    );
                }

                if let Some(error) = &self.editor.error {
                    if self.editor.is_editing(self.kind, self.name) || !editing {
                        ui.label(
                            egui::RichText::new(error)
                                .font(crate::theme::font(11.0))
                                .color(self.theme.destructive),
                        );
                    }
                }
            });

        request
    }
}

/// A compact text button matching the settings cards.
fn pane_button(label: impl Into<String>, selected: bool, theme: &Theme) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(label.into())
            .font(crate::theme::font(11.0))
            .strong()
            .color(if selected {
                egui::Color32::WHITE
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
        1.0_f32,
        if selected {
            theme.accent_highlight
        } else {
            theme.border_subtle
        },
    ))
    .corner_radius(CornerRadius::same(theme.radius_sm))
}
