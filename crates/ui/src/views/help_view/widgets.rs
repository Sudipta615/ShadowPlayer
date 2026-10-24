//! Shared visual helpers for the manual.
//!
//! These are the manual's card, badge, and row primitives. They stay in one
//! place so a change to the reading surface is made once.

use crate::theme::Theme;
use eframe::egui::{self, Color32, CornerRadius, Stroke, Vec2};

/// A sentence naming the exclusive backends this build actually has.
///
/// The manual used to say "Linux ALSA, Windows WASAPI Exclusive, macOS
/// CoreAudio Hog mode, or ASIO 2.0" on every platform and in every build. On
/// a build without the native backend that sentence is a promise the binary
/// cannot keep, and the Settings card — which reports what was really
/// negotiated — would contradict it.
pub(super) fn exclusive_backend_sentence() -> String {
    let mut parts: Vec<String> = Vec::new();
    for backend in engine::output::available_backends() {
        let label = crate::components::backend_options::backend_label(backend);
        let detail = match backend {
            config::AudioBackend::ExclusiveAlsa => {
                "locks the Linux ALSA hardware device node (`hw:X,Y` direct MMAP)"
            }
            config::AudioBackend::ExclusiveWasapi => {
                "locks the Windows endpoint in event-driven pull mode"
            }
            config::AudioBackend::ExclusiveAsio => {
                "interfaces with professional studio drivers through the ASIO COM API"
            }
            config::AudioBackend::ExclusiveCoreAudioHog => {
                "claims the macOS device in hog mode and renders through a direct IO proc"
            }
            _ => continue,
        };
        parts.push(format!("{label} {detail}"));
    }
    if parts.is_empty() {
        "This build has no exclusive-mode backend, so playback goes through the operating system's mixer.".to_string()
    } else {
        parts.join("; ")
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Reusable Visual Styling Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// The manual's cards use the same card as every other page.
pub(super) fn manual_card(
    ui: &mut egui::Ui,
    theme: &Theme,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    crate::components::page::page_card_with_padding(ui, theme, (22, 18), add_contents);
}

pub(super) fn card_title(ui: &mut egui::Ui, title: &str, subtitle: &str, theme: &Theme) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xxs);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
            // The interactive sections say so, so a reader knows the card
            // below is something to use and not another paragraph.
            //
            // The comparison is against the *translated* word, because the
            // title arrives already translated: comparing against the
            // English text would silently stop marking the card once the
            // interface speaks German.
            let interactive = tr!("Interactive");
            let simulator = tr!("Simulator");
            if title.contains(interactive.as_str()) || title.contains(simulator.as_str()) {
                egui::Frame::NONE
                    .fill(theme.accent.linear_multiply(0.18))
                    .corner_radius(CornerRadius::same(theme.radius_sm))
                    .inner_margin(egui::Margin::symmetric(
                        theme.space_md as i8,
                        theme.space_xxs as i8,
                    ))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(tr!("TRY IT"))
                                .font(crate::theme::font(9.5))
                                .strong()
                                .color(theme.accent_highlight),
                        );
                    });
            }
            ui.label(
                egui::RichText::new(title)
                    .font(crate::theme::font(16.0))
                    .strong()
                    .color(theme.text_primary),
            );
        });
        if !subtitle.is_empty() {
            ui.label(
                egui::RichText::new(subtitle)
                    .font(crate::theme::font(12.0))
                    .color(theme.text_muted),
            );
        }
    });
    ui.add_space(2.0);
    ui.separator();
    ui.add_space(2.0);
}

pub(super) fn feature_pillar_card(ui: &mut egui::Ui, title: &str, desc: &str, theme: &Theme) {
    let frame = egui::Frame::NONE
        .fill(theme.bg_pill.linear_multiply(0.7))
        .stroke(Stroke::new(1.0, theme.border_subtle))
        .corner_radius(CornerRadius::same(theme.radius_lg))
        .inner_margin(egui::Margin::symmetric(
            theme.space_2xl as i8,
            theme.space_xl as i8,
        ));

    frame.show(ui, |ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xs);
            ui.label(
                egui::RichText::new(title)
                    .font(crate::theme::font(13.5))
                    .strong()
                    .color(theme.accent_warm),
            );
            ui.add(
                egui::Label::new(
                    egui::RichText::new(desc)
                        .font(crate::theme::font(12.0))
                        .color(theme.text_secondary),
                )
                .wrap(),
            );
        });
    });
}

pub(super) fn feature_card_row(ui: &mut egui::Ui, title: &str, desc: &str, theme: &Theme) {
    let avail_w = ui.available_width();
    if avail_w < 540.0 {
        // Vertical layout for narrow windows: title on top, description below
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xs);
            ui.label(
                egui::RichText::new(title)
                    .font(crate::theme::font(13.5))
                    .strong()
                    .color(theme.accent_warm),
            );
            ui.add(
                egui::Label::new(
                    egui::RichText::new(desc)
                        .font(crate::theme::font(12.5))
                        .color(theme.text_secondary),
                )
                .wrap(),
            );
        });
    } else {
        // Side-by-side two column layout with auto-wrapping on both columns
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_3xl, 0.0);
            let label_w = (avail_w * 0.32).clamp(180.0, 280.0);
            let desc_w = (avail_w - label_w - 20.0).max(100.0);

            ui.allocate_ui_with_layout(
                Vec2::new(label_w, 20.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(title)
                                .font(crate::theme::font(13.0))
                                .strong()
                                .color(theme.text_primary),
                        )
                        .wrap(),
                    );
                },
            );

            ui.allocate_ui_with_layout(
                Vec2::new(desc_w, 20.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(desc)
                                .font(crate::theme::font(12.5))
                                .color(theme.text_secondary),
                        )
                        .wrap(),
                    );
                },
            );
        });
    }
    ui.separator();
}

pub(super) fn caption_box(ui: &mut egui::Ui, text: &str, theme: &Theme) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_md, 0.0);
        badge(ui, "DIAGRAM", theme.bg_pill, theme.accent_warm, theme);
        ui.add(
            egui::Label::new(
                egui::RichText::new(text)
                    .font(crate::theme::font(11.5))
                    .italics()
                    .color(theme.text_muted),
            )
            .wrap(),
        );
    });
}

pub(super) fn badge(
    ui: &mut egui::Ui,
    text: impl Into<String>,
    bg: Color32,
    fg: Color32,
    theme: &Theme,
) {
    egui::Frame::NONE
        .fill(bg)
        .corner_radius(CornerRadius::same(theme.radius_sm))
        .inner_margin(egui::Margin::symmetric(
            theme.space_md as i8,
            theme.space_xxs as i8,
        ))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text.into())
                    .font(crate::theme::font(10.5))
                    .strong()
                    .color(fg),
            );
        });
}

pub(super) fn spec_badge(
    ui: &mut egui::Ui,
    label: &str,
    value: &str,
    color: Color32,
    theme: &Theme,
) {
    egui::Frame::NONE
        .fill(color.linear_multiply(0.15))
        .stroke(Stroke::new(1.0, color.linear_multiply(0.4)))
        .corner_radius(CornerRadius::same(theme.radius_md))
        .inner_margin(egui::Margin::symmetric(
            theme.space_lg as i8,
            theme.space_sm as i8,
        ))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_xs, 0.0);
                ui.label(
                    egui::RichText::new(label)
                        .font(crate::theme::font(10.0))
                        .strong()
                        .color(color),
                );
                ui.label(
                    egui::RichText::new(value)
                        .font(crate::theme::font(11.0))
                        .strong()
                        .color(Color32::WHITE),
                );
            });
        });
}
