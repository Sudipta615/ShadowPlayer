//! The Parametric / AutoEQ tab and the parametric-band model it edits.
//!
//! The model is the source the AutoEQ calibration dialog writes into. The tab
//! exposes the same bank to the user: every band's enable state, filter type,
//! frequency, gain and Q, plus the shared preamp and the durable preset
//! library. A band count change is committed as one bank replacement so the
//! engine never plays a half-updated curve.

use super::controls::{card, render_footer_controls};
use super::preset_library::render_preset_library;
use super::{EqPanelActions, EqPanelState, EqTab};
use crate::theme::{font, Theme};
use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke, Vec2};
use engine::dsp::equalizer::{EqFilterType, MAX_EQ_BANDS};
use serde::{Deserialize, Serialize};

const BAND_FREQ_MIN: f32 = 10.0;
const BAND_FREQ_MAX: f32 = 24_000.0;
const BAND_GAIN_LIMIT: f32 = 24.0;
const BAND_Q_MIN: f32 = 0.1;
const BAND_Q_MAX: f32 = 18.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParametricBand {
    pub freq: f32,
    pub gain_db: f32,
    pub q: f32,
    pub filter_type: EqFilterType,
    pub enabled: bool,
}

impl Default for ParametricBand {
    fn default() -> Self {
        Self {
            freq: 1000.0,
            gain_db: 0.0,
            q: 0.707,
            filter_type: EqFilterType::Peaking,
            enabled: true,
        }
    }
}

impl ParametricBand {
    /// The next band's starting frequency: one octave above the last band,
    /// clamped to the audible range. Empty banks start at 1 kHz.
    fn next_frequency(bands: &[Self]) -> f32 {
        bands
            .last()
            .map(|band| (band.freq * 2.0).clamp(BAND_FREQ_MIN, BAND_FREQ_MAX))
            .unwrap_or(1000.0)
    }
}

pub(super) fn render_parametric_tab(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_lg);

        render_curve_card(ui, state, actions, theme);
        render_band_editor(ui, state, actions, theme);
        render_footer_controls(ui, state, actions, theme, EqTab::Parametric);
        render_preset_library(ui, state, actions, theme);
    });
}

/// The curve-level card: what the tab owns, how many bands are in it, and the
/// three entry points that can replace the bank.
fn render_curve_card(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    card(ui, theme, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
            ui.label(
                egui::RichText::new(tr!("Parametric / AutoEQ"))
                    .font(font(13.5))
                    .strong()
                    .color(theme.text_primary),
            );
            crate::components::info_box::render_info_icon_button(
                ui,
                "parametric_curve",
                &crate::components::info_box::INFO_AUTOEQ,
                theme,
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let autoeq = egui::Button::new(
                    egui::RichText::new(tr!("Open AutoEQ"))
                        .font(font(11.5))
                        .strong()
                        .color(Color32::WHITE),
                )
                .fill(theme.accent)
                .stroke(Stroke::NONE)
                .corner_radius(theme.control_radius())
                .min_size(Vec2::new(106.0, 26.0));
                if ui.add(autoeq).clicked() {
                    actions.open_autoeq_dialog = true;
                }
            });
        });

        ui.label(
            egui::RichText::new(tr!(
                "The EQ stage is driven by parametric bands. Generate or replace the curve in the AutoEQ calibration dialog."
            ))
            .font(font(11.0))
            .color(theme.text_secondary),
        );

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, theme.space_xs);
            ui.label(
                egui::RichText::new(tr_args!(
                    "{count} bands in the active parametric curve",
                    "count" => state.bands.len()
                ))
                .font(font(10.5))
                .color(theme.text_muted),
            );

            for (label, import) in [(tr!("Export"), false), (tr!("Import"), true)] {
                let button = egui::Button::new(
                    egui::RichText::new(label)
                        .font(font(11.0))
                        .color(theme.text_secondary),
                )
                .fill(theme.bg_pill)
                .stroke(Stroke::NONE)
                .corner_radius(theme.control_radius())
                .min_size(Vec2::new(68.0, 24.0));
                if ui
                    .add(button)
                    .on_hover_text(tr!("AutoEQ, Equalizer APO, REW, and GraphicEQ text"))
                    .clicked()
                {
                    if import {
                        actions.import_curve = true;
                    } else {
                        actions.export_curve = true;
                    }
                }
            }
        });
    });
}

/// One row per band. The band count is capped by the engine (64), so rendering
/// every row is bounded work; the surrounding scroll area handles the height.
fn render_band_editor(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    let mut add_band = false;
    let mut remove_band: Option<usize> = None;
    let mut band_changed: Option<usize> = None;

    card(ui, theme, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, theme.space_xs);
            ui.label(
                egui::RichText::new(tr!("Parametric bands"))
                    .font(font(12.5))
                    .strong()
                    .color(theme.text_primary),
            );

            let at_capacity = state.bands.len() >= MAX_EQ_BANDS;
            let add = egui::Button::new(
                egui::RichText::new(tr!("Add band"))
                    .font(font(11.0))
                    .color(theme.text_secondary),
            )
            .fill(theme.bg_pill)
            .stroke(Stroke::NONE)
            .corner_radius(theme.control_radius())
            .min_size(Vec2::new(84.0, 24.0));
            let response = ui.add_enabled(!at_capacity, add);
            if response
                .on_hover_text(if at_capacity {
                    tr_args!(
                        "The engine accepts at most {max} bands",
                        "max" => MAX_EQ_BANDS
                    )
                } else {
                    tr!("Append an octave above the last band")
                })
                .clicked()
            {
                add_band = true;
            }
        });

        if state.bands.is_empty() {
            ui.label(
                egui::RichText::new(tr!(
                    "No parametric bands. Add one, import an AutoEQ curve, or open AutoEQ."
                ))
                .font(font(11.0))
                .color(theme.text_muted),
            );
            return;
        }

        // `remove` is disabled when it would empty the bank; the engine can
        // represent an empty bank, but a curve with no bands is indistinguishable
        // from a bypassed EQ in the UI.
        let can_remove = state.bands.len() > 1;
        for (index, band) in state.bands.iter_mut().enumerate() {
            let row = render_band_row(ui, index, band, can_remove, theme);
            if row.changed {
                band_changed = Some(index);
            }
            if row.remove {
                remove_band = Some(index);
            }
        }
    });

    if add_band {
        state.bands.push(ParametricBand {
            freq: ParametricBand::next_frequency(&state.bands),
            ..ParametricBand::default()
        });
        actions.eq_bands_changed = true;
    }
    if let Some(index) = remove_band {
        if index < state.bands.len() {
            state.bands.remove(index);
            actions.eq_bands_changed = true;
        }
    }
    if let Some(index) = band_changed {
        actions.eq_band_changed = Some(index);
    }
}

/// Draw one band row.
fn render_band_row(
    ui: &mut egui::Ui,
    index: usize,
    band: &mut ParametricBand,
    can_remove: bool,
    theme: &Theme,
) -> BandRowResult {
    let mut result = BandRowResult::default();
    egui::Frame::NONE
        .fill(if band.enabled {
            theme.bg_card
        } else {
            theme.bg_card_hover
        })
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(theme.control_radius())
        .inner_margin(Margin::symmetric(
            theme.space_lg as i8,
            theme.space_sm as i8,
        ))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, theme.space_xs);

                let mut enabled = band.enabled;
                if ui
                    .checkbox(&mut enabled, "")
                    .on_hover_text(tr!("Enable this band"))
                    .changed()
                {
                    band.enabled = enabled;
                    result.changed = true;
                }

                ui.label(
                    egui::RichText::new(tr_args!("Band {number}", "number" => index + 1))
                        .font(font(11.5))
                        .strong()
                        .color(if band.enabled {
                            theme.text_primary
                        } else {
                            theme.text_muted
                        }),
                );

                egui::ComboBox::from_id_salt(("parametric_filter_type", index))
                    .selected_text(
                        egui::RichText::new(filter_label(band.filter_type))
                            .font(font(10.5))
                            .color(theme.text_secondary),
                    )
                    .width(110.0)
                    .show_ui(ui, |ui| {
                        for filter_type in [
                            EqFilterType::Peaking,
                            EqFilterType::LowShelf,
                            EqFilterType::HighShelf,
                            EqFilterType::LowPass,
                            EqFilterType::HighPass,
                            EqFilterType::Bandpass,
                            EqFilterType::Notch,
                            EqFilterType::AllPass,
                        ] {
                            let selected = band.filter_type == filter_type;
                            if ui
                                .selectable_label(selected, filter_label(filter_type))
                                .clicked()
                                && !selected
                            {
                                band.filter_type = filter_type;
                                result.changed = true;
                            }
                        }
                    });

                result.changed |= value_editor(
                    ui,
                    tr!("Frequency"),
                    &mut band.freq,
                    BAND_FREQ_MIN..=BAND_FREQ_MAX,
                    1.0,
                    0,
                    " Hz",
                    theme,
                );
                result.changed |= value_editor(
                    ui,
                    tr!("Gain"),
                    &mut band.gain_db,
                    -BAND_GAIN_LIMIT..=BAND_GAIN_LIMIT,
                    0.1,
                    1,
                    " dB",
                    theme,
                );
                result.changed |= value_editor(
                    ui,
                    tr!("Filter Q"),
                    &mut band.q,
                    BAND_Q_MIN..=BAND_Q_MAX,
                    0.01,
                    2,
                    "",
                    theme,
                );

                let remove = egui::Button::new(
                    egui::RichText::new("×")
                        .font(font(12.0))
                        .color(theme.text_muted),
                )
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius::same(theme.radius_sm))
                .min_size(Vec2::new(22.0, 22.0));
                let response = ui.add_enabled(can_remove, remove);
                if response.on_hover_text(tr!("Remove this band")).clicked() {
                    result.remove = true;
                }
            });
        });

    result
}

#[derive(Default)]
struct BandRowResult {
    changed: bool,
    remove: bool,
}

/// A labelled drag editor for one numeric band field.
#[allow(clippy::too_many_arguments)]
fn value_editor(
    ui: &mut egui::Ui,
    label: String,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    speed: f64,
    decimals: usize,
    suffix: &str,
    theme: &Theme,
) -> bool {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_xxs, 0.0);
        ui.label(
            egui::RichText::new(label)
                .font(font(9.5))
                .color(theme.text_muted),
        );
        ui.add(
            egui::DragValue::new(value)
                .range(range)
                .speed(speed)
                .fixed_decimals(decimals)
                .suffix(suffix),
        )
        .changed()
    })
    .inner
}

fn filter_label(filter_type: EqFilterType) -> String {
    match filter_type {
        EqFilterType::Peaking => tr!("Peaking"),
        EqFilterType::LowShelf => tr!("Low shelf"),
        EqFilterType::HighShelf => tr!("High shelf"),
        EqFilterType::LowPass => tr!("Low pass"),
        EqFilterType::HighPass => tr!("High pass"),
        EqFilterType::Bandpass => tr!("Bandpass"),
        EqFilterType::Notch => tr!("Notch"),
        EqFilterType::AllPass => tr!("All pass"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_band_starts_one_octave_above_the_last_and_stays_audible() {
        assert_eq!(ParametricBand::next_frequency(&[]), 1000.0);
        let band = ParametricBand {
            freq: 2000.0,
            ..ParametricBand::default()
        };
        assert_eq!(
            ParametricBand::next_frequency(std::slice::from_ref(&band)),
            4000.0
        );
        let band = ParametricBand {
            freq: 20_000.0,
            ..ParametricBand::default()
        };
        assert_eq!(
            ParametricBand::next_frequency(&[band]),
            BAND_FREQ_MAX,
            "a band above the audible range would be inaudible"
        );
    }

    #[test]
    fn filter_labels_cover_every_engine_variant() {
        for filter_type in [
            EqFilterType::Peaking,
            EqFilterType::LowShelf,
            EqFilterType::HighShelf,
            EqFilterType::LowPass,
            EqFilterType::HighPass,
            EqFilterType::Bandpass,
            EqFilterType::Notch,
            EqFilterType::AllPass,
        ] {
            assert!(!filter_label(filter_type).is_empty());
        }
    }
}
