//! Reference tabs: keyboard shortcuts, glossary, and this-build facts.
//!
//! Also owns the platform feature matrix, whose column geometry is guarded by
//! a layout test at the bottom of this file.

use super::widgets::{card_title, feature_card_row, manual_card};
use super::HelpState;
use crate::theme::Theme;
use eframe::egui::{self, Color32, CornerRadius, Sense, Stroke, Vec2};

// ─────────────────────────────────────────────────────────────────────────────
// 10. Tab: Keyboard Shortcuts & Power Gestures
// ─────────────────────────────────────────────────────────────────────────────

pub(super) fn render_shortcuts_tab(ui: &mut egui::Ui, _state: &mut HelpState, theme: &Theme) {
    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            &tr!("1. Audiophile Keyboard Shortcuts"),
            "Complete reference of keyboard commands for instant mouse-free operation",
            theme,
        );

        let shortcuts = crate::shortcuts::SHORTCUTS;

        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_sm);
            for shortcut in shortcuts {
                ui.horizontal_top(|ui| {
                    let (badge_rect, _) =
                        ui.allocate_exact_size(Vec2::new(120.0, 24.0), Sense::hover());
                    let p = ui.painter_at(badge_rect);
                    p.rect_filled(
                        badge_rect,
                        CornerRadius::same(theme.radius_sm),
                        theme.bg_pill,
                    );
                    p.rect_stroke(
                        badge_rect,
                        CornerRadius::same(theme.radius_sm),
                        Stroke::new(1.0, theme.border_subtle),
                        egui::StrokeKind::Inside,
                    );
                    p.text(
                        badge_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        shortcut.keys,
                        crate::theme::mono_font(12.0),
                        theme.accent_warm,
                    );

                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(shortcut.description)
                                .font(crate::theme::font(13.0))
                                .color(theme.text_secondary),
                        )
                        .wrap(),
                    );
                });
            }
        });
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// 11. Tab: Audiophile Glossary & Terminology
// ─────────────────────────────────────────────────────────────────────────────

/// Width of one platform column in the feature matrix.
const MATRIX_PLATFORM_WIDTH: f32 = 64.0;

/// Draw one fixed-size matrix cell.
///
/// `allocate_ui_with_layout` lets a child UI shrink back to its content, which
/// made the platform headings and the `Yes` cells collapse into one run of
/// adjacent text. Allocating the full cell and painting the text at its left
/// edge makes the column width part of the parent's layout, not a hint.
fn matrix_cell(
    ui: &mut egui::Ui,
    width: f32,
    height: f32,
    text: &str,
    font: egui::FontId,
    color: Color32,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    ui.painter().text(
        egui::pos2(rect.left(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        font,
        color,
    );
}

/// Render the platform feature matrix used by the Help page.
pub(super) fn render_feature_matrix(ui: &mut egui::Ui, theme: &Theme) {
    let gap = theme.space_sm;
    let label_width =
        (ui.available_width() - MATRIX_PLATFORM_WIDTH * 3.0 - gap * 3.0).clamp(150.0, 320.0);
    let header_font = crate::theme::font(11.0);
    let row_font = crate::theme::font(12.0);

    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.horizontal(|ui| {
            matrix_cell(
                ui,
                label_width,
                18.0,
                "Capability",
                header_font.clone(),
                theme.text_muted,
            );
            for platform in crate::features::Platform::ALL {
                matrix_cell(
                    ui,
                    MATRIX_PLATFORM_WIDTH,
                    18.0,
                    platform.label(),
                    header_font.clone(),
                    theme.text_muted,
                );
            }
        });

        for feature in crate::features::FEATURES {
            ui.horizontal(|ui| {
                matrix_cell(
                    ui,
                    label_width,
                    20.0,
                    feature.label,
                    row_font.clone(),
                    theme.text_primary,
                );
                for platform in crate::features::Platform::ALL {
                    let supported = feature.supports(platform);
                    matrix_cell(
                        ui,
                        MATRIX_PLATFORM_WIDTH,
                        20.0,
                        if supported { "Yes" } else { "-" },
                        row_font.clone(),
                        if supported {
                            theme.bit_perfect_green
                        } else {
                            theme.text_dim
                        },
                    );
                }
            });
            ui.label(
                egui::RichText::new(feature.note)
                    .font(crate::theme::font(11.0))
                    .color(theme.text_muted),
            );
            ui.add_space(theme.space_sm);
        }
    });
}

/// The "This Build" tab: the facts about the running binary.
///
/// Everything here is read from the live engine rather than written out, which
/// is the point. A manual that says "Windows: WASAPI Exclusive" is a claim
/// about a *build*; a manual that lists the backends the running binary
/// actually compiled in is a fact about *this* binary. The difference is the
/// difference between a promise and a report.
pub(super) fn render_this_build_tab(ui: &mut egui::Ui, _state: &mut HelpState, theme: &Theme) {
    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            &tr!("What This Build Can Do"),
            "Read from the running engine. A capability that is not listed here is not in this binary.",
            theme,
        );

        build_facts_table(
            ui,
            &[
                ("Player".to_string(), env!("CARGO_PKG_VERSION").to_string()),
                ("Audio engine".to_string(), engine::VERSION.to_string()),
                ("Platform".to_string(), std::env::consts::OS.to_string()),
                (
                    "Architecture".to_string(),
                    std::env::consts::ARCH.to_string(),
                ),
            ],
            theme,
        );
    });

    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            &tr!("Platform Feature Matrix"),
            "The same generated matrix as the README. Each platform claim is checked against the cfg gates in the code.",
            theme,
        );

        render_feature_matrix(ui, theme);
    });

    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            &tr!("Output Backends In This Build"),
            "The backends this binary compiled in. Each one enumerates its own devices, so a device chosen in Settings is a device this build can open.",
            theme,
        );

        let backends = engine::output::available_backends();
        if backends.is_empty() {
            ui.label(
                egui::RichText::new(tr!("No output backend is compiled into this build."))
                    .font(crate::theme::font(12.0))
                    .color(theme.text_dim),
            );
        } else {
            let mut rows: Vec<(String, String)> = Vec::new();
            for backend in &backends {
                let devices = engine::output::devices::available_devices(*backend);
                let detail = if devices.is_empty() {
                    "no devices reported".to_string()
                } else {
                    // A count, and the names — the value column wraps, so a
                    // long list is readable rather than a line that runs off
                    // the card.
                    let names: Vec<String> = devices.iter().map(|d| d.name.clone()).collect();
                    format!("{} device(s): {}", devices.len(), names.join(", "))
                };
                rows.push((
                    crate::components::backend_options::backend_label(*backend),
                    detail,
                ));
            }
            build_facts_table(ui, &rows, theme);
        }

        // The exclusive backends are the ones that make the audiophile claim
        // true, so their absence is worth saying out loud.
        let exclusive: Vec<String> = backends
            .iter()
            .filter(|b| {
                matches!(
                    b,
                    config::AudioBackend::ExclusiveAlsa
                        | config::AudioBackend::ExclusiveWasapi
                        | config::AudioBackend::ExclusiveAsio
                        | config::AudioBackend::ExclusiveCoreAudioHog
                )
            })
            .map(|b| crate::components::backend_options::backend_label(*b))
            .collect();
        if exclusive.is_empty() {
            ui.label(
                egui::RichText::new(
                    tr!("No exclusive-mode backend is compiled in, so this build cannot open a device without the operating system's mixer. Playback is not bit-perfect here."),
                )
                .font(crate::theme::font(12.0))
                .color(theme.warning_yellow),
            );
        } else {
            ui.label(
                egui::RichText::new(format!(
                    "Exclusive-mode backends available: {}.",
                    exclusive.join(", ")
                ))
                .font(crate::theme::font(12.0))
                .color(theme.text_muted),
            );
        }
    });

    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            &tr!("Decoders In This Build"),
            "The formats this binary can actually decode.",
            theme,
        );
        let codecs = engine::decode::codecs::decodable_codecs();
        if codecs.is_empty() {
            ui.label(
                egui::RichText::new(tr!("No decoder is compiled into this build."))
                    .font(crate::theme::font(12.0))
                    .color(theme.warning_yellow),
            );
        } else {
            let mut names: Vec<String> = codecs.iter().map(|c| format!("{c:?}")).collect();
            names.sort();
            ui.label(
                egui::RichText::new(names.join(" · "))
                    .font(crate::theme::font(12.0))
                    .color(theme.text_secondary),
            );
        }
    });
}

/// A two-column list of facts: a label on the left, a value on the right.
///
/// The value wraps: a device list is a sentence, and a sentence that runs off
/// the edge of the card is a sentence the reader cannot finish.
///
/// A `Grid` is the obvious choice here and the wrong one: a grid measures its
/// columns from the *unwrapped* content, so a long value makes the grid
/// demand more width than the card has, and the label column is squeezed
/// until it is clipped. A horizontal layout gives the value whatever the
/// label did not use, and wraps inside it.
fn build_facts_table(ui: &mut egui::Ui, rows: &[(String, String)], theme: &Theme) {
    for (label, value) in rows {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_lg, 0.0);
            ui.label(
                egui::RichText::new(label.clone())
                    .font(crate::theme::font(12.0))
                    .color(theme.text_muted),
            );
            ui.add(
                egui::Label::new(
                    egui::RichText::new(value.clone())
                        .font(crate::theme::font(12.0))
                        .color(theme.text_primary),
                )
                .wrap(),
            );
        });
    }
}

pub(super) fn render_glossary_tab(ui: &mut egui::Ui, _state: &mut HelpState, theme: &Theme) {
    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            &tr!("Complete Technical Glossary & Definitions"),
            "Clear definitions of every acoustic, DSP, and engineering term in ShadowPlayer",
            theme,
        );

        let glossary = [
            ("dBFS (Decibels relative to Full Scale)", "The standard unit of volume in digital audio systems. 0.0 dBFS is the maximum possible volume a digital file can store without clipping. All normal music levels are expressed in negative decibels (-14 dBFS, -6 dBFS)."),
            ("LUFS / LKFS (Loudness Units Full Scale)", "An international psychoacoustic measurement of perceived volume defined in ITU-R BS.1770 and EBU R128. Unlike peak decibels, LUFS accounts for how human ears perceive loudness across different frequencies over time."),
            ("TPDF (Triangular Probability Density Function)", "The mathematically optimal dithering noise profile. By adding tiny triangularly distributed noise prior to quantization, it converts harsh truncation distortion into completely inaudible random white noise."),
            ("HRTF (Head-Related Transfer Function)", "A mathematical filter pair describing how sound from a given coordinate in 3D space diffracts around human shoulders, head, and outer ear cartilage (pinna) before reaching the eardrums."),
            ("SOFA (Spatially Oriented Format for Acoustics)", "An international AES standard file format storing measured 3D impulse responses from acoustic mannequins (such as KEMAR) for binaural spatial rendering."),
            ("Biquad Filter", "A second-order recursive digital IIR (Infinite Impulse Response) filter containing two poles and two zeros. The fundamental mathematical building block for all parametric EQs, shelves, and peaking filters."),
            ("Inter-Sample Peak (ISP)", "When digital samples near 0 dBFS are converted into an analog wave by a DAC, the smooth reconstructed wave between two discrete sample points can overshoot above 0 dBFS, causing DAC clipping unless Auto-Headroom is engaged."),
            ("Constant-Q Filter", "An equalizer filter whose bandwidth narrows as frequency increases, maintaining an exact proportional ratio (Q = Center Frequency / Bandwidth). This matches human logarithmic pitch perception."),
            ("Sinc Resampling (Whittaker-Shannon)", "The mathematically ideal anti-aliased interpolation formula. Convolving samples with a sinc function allows changing sample rates without altering high-frequency phase or introducing aliasing artifacts."),
            ("ASIO (Audio Stream Input/Output)", "A proprietary low-latency driver protocol created by Steinberg that connects audio software directly to soundcard hardware, bypassing the Windows operating system audio stack."),
        ];

        // Alphabetical, so the glossary can be scanned rather than read, and
        // in two columns so a long definition does not turn the page into a
        // single ribbon of text.
        let mut glossary = glossary;
        glossary.sort_by_key(|(term, _)| term.to_ascii_lowercase());

        ui.columns(2, |columns| {
            let half = glossary.len().div_ceil(2);
            for (index, (term, definition)) in glossary.iter().enumerate() {
                let column = if index < half { 0 } else { 1 };
                feature_card_row(&mut columns[column], term, definition, theme);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Pos2, RawInput, Rect};

    #[test]
    fn feature_matrix_columns_do_not_collapse_into_each_other() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let theme = Theme::dark();
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 2000.0))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            ui.set_width(1000.0);
            render_feature_matrix(ui, &theme);
        });

        let mut positions = std::collections::HashMap::new();
        for clipped in &output.shapes {
            if let egui::Shape::Text(text) = &clipped.shape {
                positions.insert(text.galley.text().to_string(), text.pos.x);
            }
        }
        let linux = positions["Linux"];
        let windows = positions["Windows"];
        let macos = positions["macOS"];
        assert!(
            windows - linux >= MATRIX_PLATFORM_WIDTH,
            "Linux and Windows columns overlap: {linux} vs {windows}"
        );
        assert!(
            macos - windows >= MATRIX_PLATFORM_WIDTH,
            "Windows and macOS columns overlap: {windows} vs {macos}"
        );

        // Acknowledge the font atlas so epaint's drop check does not fire;
        // these tests have no renderer to upload it to.
        output.textures_delta.clear();
    }
}
