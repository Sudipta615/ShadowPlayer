//! The AutoEQ parametric browser.
//!
//! The earlier dialog optimized a handful of hand-written measurement arrays
//! at open time. This one browses the shipped AutoEQ corpus: 8,850 parametric
//! corrections, each with its measurement source and a preamp derived from the
//! combined response. The list is virtualized and the catalog is decoded on a
//! worker, so opening the dialog never blocks a frame.
//!
//! Selecting a row is a preview only. Applying returns an [`ImportedCurve`] to
//! the shell, which installs it through the same path as a text import.

use std::sync::Arc;

use config::ImportedCurve;
use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke, TextWrapMode, Vec2};

use crate::state::AutoEqCatalog;
use crate::theme::{font, Theme};

const ROW_HEIGHT: f32 = 24.0;

#[derive(Default)]
pub struct AutoEqModalState {
    pub is_open: bool,
    pub search_query: String,
    pub selected_index: Option<usize>,
    pub catalog: Option<Arc<AutoEqCatalog>>,
    pub catalog_error: Option<String>,
    /// Currently connected audio device name for AutoEQ auto-suggestion
    pub active_device_name: String,
    /// Indices into the catalog that match `filter_query`. Recomputed only
    /// when the query changes, not on every frame.
    pub filtered: Vec<usize>,
    filter_query: String,
}

impl AutoEqModalState {
    pub fn set_catalog(&mut self, catalog: Arc<AutoEqCatalog>) {
        self.catalog = Some(catalog);
        self.catalog_error = None;
        self.selected_index = None;
        self.filter_query.clear();
        self.filtered.clear();
    }

    pub fn set_catalog_error(&mut self, error: String) {
        self.catalog_error = Some(error);
        self.catalog = None;
        self.filtered.clear();
        self.selected_index = None;
    }

    fn refresh_filter(&mut self) {
        if self.filter_query == self.search_query {
            return;
        }
        self.filter_query = self.search_query.clone();
        self.filtered = match &self.catalog {
            Some(catalog) => catalog.search(&self.filter_query),
            None => Vec::new(),
        };
    }
}

/// Browse the corpus. Returns the selected curve when Apply is pressed.
pub fn show_autoeq_modal(
    ctx: &egui::Context,
    state: &mut AutoEqModalState,
    theme: &Theme,
) -> Option<ImportedCurve> {
    if !state.is_open {
        return None;
    }

    state.refresh_filter();
    let mut applied: Option<ImportedCurve> = None;
    let mut close_requested = false;

    let closed = crate::dialog::show_modal(
        ctx,
        "autoeq",
        "AutoEQ Headphone & IEM Calibration",
        680.0,
        theme,
        |ui| {
            render_body(ui, state, theme, &mut applied, &mut close_requested);
        },
    );

    if closed || close_requested || applied.is_some() {
        state.is_open = false;
    }
    applied
}

fn render_body(
    ui: &mut egui::Ui,
    state: &mut AutoEqModalState,
    theme: &Theme,
    applied: &mut Option<ImportedCurve>,
    close_requested: &mut bool,
) {
    ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_md);

    ui.label(
        egui::RichText::new(tr!(
            "Auto-compensate frequency response using the AutoEQ database"
        ))
        .font(font(12.5))
        .color(theme.text_secondary),
    );

    render_search(ui, state, theme);
    ui.separator();

    match (&state.catalog, &state.catalog_error) {
        (_, Some(error)) => {
            ui.label(
                egui::RichText::new(tr_args!(
                    "Could not load the AutoEQ corpus: {error}",
                    "error" => error
                ))
                .font(font(11.5))
                .color(theme.destructive),
            );
        }
        (None, None) => {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(16.0));
                ui.label(
                    egui::RichText::new(tr!("Loading AutoEQ curves…"))
                        .font(font(12.0))
                        .color(theme.text_muted),
                );
            });
        }
        (Some(catalog), None) => {
            let catalog = Arc::clone(catalog);
            if !state.active_device_name.is_empty() {
                if let Some(suggestion) = catalog.find_suggestion(&state.active_device_name) {
                    egui::Frame::NONE
                        .fill(theme.accent.linear_multiply(0.12))
                        .stroke(Stroke::new(1.0, theme.accent.linear_multiply(0.4)))
                        .corner_radius(CornerRadius::same(theme.radius_md))
                        .inner_margin(Margin::same(theme.space_md as i8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (icon_rect, _) = ui.allocate_exact_size(
                                    egui::Vec2::splat(16.0),
                                    egui::Sense::hover(),
                                );
                                crate::components::icons::draw_headphones(
                                    ui,
                                    icon_rect,
                                    theme.accent,
                                );
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new(tr_args!(
                                            "AutoEQ Match for {device}:",
                                            "device" => state.active_device_name.clone()
                                        ))
                                        .font(font(10.5))
                                        .color(theme.text_secondary),
                                    );
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{} ({})",
                                            suggestion.name, suggestion.source
                                        ))
                                        .font(font(11.5))
                                        .strong()
                                        .color(theme.text_primary),
                                    );
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let btn = egui::Button::new(
                                            egui::RichText::new(tr!("Apply Match"))
                                                .font(font(11.0))
                                                .strong()
                                                .color(Color32::WHITE),
                                        )
                                        .fill(theme.accent)
                                        .corner_radius(CornerRadius::same(theme.radius_sm));
                                        if ui.add(btn).clicked() {
                                            *applied = Some(suggestion.to_imported_curve());
                                            *close_requested = true;
                                        }
                                    },
                                );
                            });
                        });
                    ui.add_space(4.0);
                }
            }
            render_list(ui, state, &catalog, theme);
            ui.separator();
            render_selection(ui, state, &catalog, theme, applied, close_requested);
        }
    }
}

fn render_search(ui: &mut egui::Ui, state: &mut AutoEqModalState, theme: &Theme) {
    crate::components::search_field::search_field_with_width(
        ui,
        &mut state.search_query,
        tr!("Headphone or IEM model, source"),
        theme,
        f32::INFINITY,
    );
}

fn render_list(
    ui: &mut egui::Ui,
    state: &mut AutoEqModalState,
    catalog: &Arc<AutoEqCatalog>,
    theme: &Theme,
) {
    ui.horizontal(|ui| {
        let count = catalog.len();
        ui.label(
            egui::RichText::new(if state.filter_query.trim().is_empty() {
                tr_args!("{count} curves", "count" => count)
            } else {
                tr_args!(
                    "{shown} of {count} curves",
                    "shown" => state.filtered.len(),
                    "count" => count
                )
            })
            .font(font(10.5))
            .color(theme.text_muted),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.hyperlink_to(
                egui::RichText::new(tr!("Curves derived from AutoEQ (MIT)")).font(font(10.5)),
                "https://github.com/jaakkopasanen/AutoEq",
            );
        });
    });

    if state.filtered.is_empty() {
        ui.label(
            egui::RichText::new(tr!("No matching headphones or IEMs"))
                .font(font(11.5))
                .color(theme.text_muted),
        );
        return;
    }

    let mut clicked: Option<usize> = None;
    let width = ui.available_width();
    egui::ScrollArea::vertical()
        .id_salt("autoeq_list")
        .max_height(220.0)
        .auto_shrink([false, true])
        .show_rows(ui, ROW_HEIGHT, state.filtered.len(), |ui, range| {
            for row in range {
                let index = state.filtered[row];
                let Some(entry) = catalog.get(index) else {
                    continue;
                };
                let selected = state.selected_index == Some(index);
                let response = ui.add_sized(
                    Vec2::new(width, ROW_HEIGHT),
                    egui::Button::new(
                        egui::RichText::new(format!("{}  ·  {}", entry.name, entry.source))
                            .font(font(11.0))
                            .color(if selected {
                                Color32::WHITE
                            } else {
                                theme.text_primary
                            }),
                    )
                    .selected(selected)
                    .fill(if selected {
                        theme.accent
                    } else {
                        theme.bg_pill
                    })
                    .stroke(Stroke::NONE)
                    .corner_radius(CornerRadius::same(theme.radius_sm))
                    .wrap_mode(TextWrapMode::Truncate),
                );
                if response.clicked() {
                    clicked = Some(index);
                }
            }
        });
    if let Some(index) = clicked {
        state.selected_index = Some(index);
    }
}

fn render_selection(
    ui: &mut egui::Ui,
    state: &AutoEqModalState,
    catalog: &Arc<AutoEqCatalog>,
    theme: &Theme,
    applied: &mut Option<ImportedCurve>,
    close_requested: &mut bool,
) {
    let Some(index) = state.selected_index else {
        ui.label(
            egui::RichText::new(tr!("Select a model to see its correction curve"))
                .font(font(11.5))
                .color(theme.text_muted),
        );
        return;
    };
    let Some(entry) = catalog.get(index) else {
        return;
    };

    egui::Frame::NONE
        .fill(theme.bg_card_hover)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(theme.radius_md))
        .inner_margin(egui::Margin::same(theme.space_md as i8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(&entry.name)
                        .font(font(13.5))
                        .strong()
                        .color(theme.text_primary),
                );
                ui.label(
                    egui::RichText::new(if entry.is_iem {
                        tr!("In-ear")
                    } else {
                        tr!("Over-ear")
                    })
                    .font(font(9.5))
                    .color(theme.text_secondary),
                );
            });
            ui.label(
                egui::RichText::new(&entry.source)
                    .font(font(10.5))
                    .color(theme.text_muted),
            );
            ui.add_space(theme.space_xs);
            ui.label(
                egui::RichText::new(tr_args!(
                    "Bands: {bands} · Preamp: {preamp} dB",
                    "bands" => entry.bands.len(),
                    "preamp" => format!("{:+.1}", entry.preamp_db)
                ))
                .font(font(10.5))
                .color(theme.text_secondary),
            );

            egui::ScrollArea::vertical()
                .id_salt("autoeq_bands")
                .max_height(148.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    egui::Grid::new("autoeq_band_grid")
                        .num_columns(4)
                        .spacing(Vec2::new(theme.space_lg, 2.0))
                        .striped(true)
                        .show(ui, |ui| {
                            for header in [tr!("Type"), tr!("Frequency"), tr!("Gain"), tr!("Q")] {
                                ui.label(
                                    egui::RichText::new(header)
                                        .font(font(9.5))
                                        .color(theme.text_muted),
                                );
                            }
                            ui.end_row();
                            for band in &entry.bands {
                                ui.label(
                                    egui::RichText::new(filter_label(band.filter_type))
                                        .font(font(10.5))
                                        .color(theme.text_secondary),
                                );
                                ui.label(
                                    egui::RichText::new(format!("{:.0} Hz", band.frequency))
                                        .font(font(10.5))
                                        .color(theme.text_primary),
                                );
                                ui.label(
                                    egui::RichText::new(format!("{:+.1} dB", band.gain_db))
                                        .font(font(10.5))
                                        .color(theme.text_primary),
                                );
                                ui.label(
                                    egui::RichText::new(format!("{:.2}", band.q))
                                        .font(font(10.5))
                                        .color(theme.text_secondary),
                                );
                                ui.end_row();
                            }
                        });
                });

            ui.add_space(theme.space_xs);
            ui.label(
                egui::RichText::new(tr!(
                    "Applying a correction replaces the current parametric curve"
                ))
                .font(font(10.0))
                .color(theme.text_muted),
            );
            ui.add_space(theme.space_sm);
            ui.horizontal(|ui| {
                let apply = egui::Button::new(
                    egui::RichText::new(tr!("Apply Calibration to Audio Engine"))
                        .font(font(12.5))
                        .strong()
                        .color(Color32::WHITE),
                )
                .fill(theme.accent)
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius::same(theme.radius_md))
                .min_size(Vec2::new((ui.available_width() - 90.0).max(160.0), 30.0));
                if ui.add(apply).clicked() {
                    *applied = Some(entry.to_imported_curve());
                }
                if ui.button(tr!("Close")).clicked() {
                    *close_requested = true;
                }
            });
        });
}

fn filter_label(filter_type: config::FilterType) -> String {
    match filter_type {
        config::FilterType::LowShelf => tr!("Low shelf"),
        config::FilterType::HighShelf => tr!("High shelf"),
        config::FilterType::Peaking => tr!("Peaking"),
        other => format!("{other:?}"),
    }
}
