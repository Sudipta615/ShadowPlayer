//! Quick Start & Architecture Overview tab.

use super::widgets::{caption_box, card_title, feature_pillar_card, manual_card};
use super::HelpState;
use crate::theme::Theme;
use eframe::egui::{self, CornerRadius, Vec2};

// ─────────────────────────────────────────────────────────────────────────────
// 3. Tab: Quick Start & Architecture Overview
// ─────────────────────────────────────────────────────────────────────────────

pub(super) fn render_quick_start_tab(ui: &mut egui::Ui, _state: &mut HelpState, theme: &Theme) {
    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            &tr!("1. Welcome to ShadowPlayer"),
            "The audiophile music workstation engineered for bit-exact transparency",
            theme,
        );

        ui.label(
            egui::RichText::new(
                tr!("ShadowPlayer is not a typical consumer music player. It was engineered from the ground up in Rust to treat your digital audio library with studio-grade mathematical fidelity. Traditional operating system media players push your audio through noisy OS audio mixers, degrade dynamic range through digital bit-truncation, and force hidden sample rate conversions. ShadowPlayer provides direct bit-perfect transport to external Digital-to-Analog Converters (DACs) alongside a 64-bit floating point DSP suite.")
            )
            .font(crate::theme::font(13.5))
            .color(theme.text_secondary),
        );

        ui.add_space(8.0);

        // Core Pillars Grid.
        //
        // The first two pillars state what *this build* can do, so they are
        // rendered from the live engine. A manual that promises "Exclusive
        // ALSA, WASAPI, CoreAudio and ASIO" on a build that has none of them
        // is a promise, and the promise is the thing this project refuses to
        // make.
        let exclusive_labels: Vec<String> = engine::output::available_backends()
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

        let bit_perfect_pillar = if exclusive_labels.is_empty() {
            (
                "• Bit-Perfect Audio (not in this build)".to_string(),
                "This build has no exclusive-mode backend compiled in, so it plays through the operating system's mixer and is not bit-perfect. See “This Build” for what it can do.".to_string(),
            )
        } else {
            (
                "• Bit-Perfect Audio".to_string(),
                format!(
                    "Direct hardware streaming via {}. Bypasses the OS digital mixer and its volume algorithms.",
                    exclusive_labels.join(", ")
                ),
            )
        };

        let autoeq_profiles = crate::state::autoeq_catalog::SHIPPED_CURVE_COUNT;
        let dsp_pillar = (
            "• 64-bit Floating Point DSP".to_string(),
            format!(
                "Parametric and Graphic EQs (10 to 64 bands), minimum-phase biquad filters, {autoeq_profiles} AutoEQ parametric corrections, text-curve import and export, and TPDF dither."
            ),
        );

        ui.columns(3, |cols| {
            cols[0].vertical(|ui| {
                feature_pillar_card(ui, &bit_perfect_pillar.0, &bit_perfect_pillar.1, theme);
            });
            cols[1].vertical(|ui| {
                feature_pillar_card(ui, &dsp_pillar.0, &dsp_pillar.1, theme);
            });
            cols[2].vertical(|ui| {
                feature_pillar_card(
                    ui,
                    "• 3D Binaural HRTF Soundstage",
                    "Convolution-based Head-Related Transfer Function spatializer transforms standard stereo into a natural, out-of-head studio acoustic space.",
                    theme,
                );
            });
        });
    });

    // Technical Diagram Image Card
    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            &tr!("2. High-Level System Architecture Diagram"),
            "Complete flow from encoded audio file to analog headphone output",
            theme,
        );

        let avail_w = ui.available_width();
        let img_h = avail_w * (768.0 / 1376.0);
        let img = egui::Image::new(egui::include_image!("../../../assets/help/audio_flow.jpg"))
            .corner_radius(CornerRadius::same(theme.radius_lg))
            .fit_to_exact_size(Vec2::new(avail_w, img_h));
        ui.add(img);

        ui.add_space(6.0);
        caption_box(ui, "Figure 1: Architectural diagram of the ShadowPlayer audio pipeline. Uncompressed PCM samples flow from the multi-format decoder through lock-free ringbuffers into the optional 64-bit float DSP rack before direct handover to the hardware DAC.", theme);
    });

    // Interface Navigation Walkthrough
    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            &tr!("3. Interface Elements & Layout Walkthrough"),
            "How to navigate and control the desktop workspace",
            theme,
        );

        let layout_points = [
            ("Left Navigation Sidebar", "Quick access to your curated Library (Home, Albums, Artists, Genres, Folders), Playlists, Favorites, History, Settings & Audio Studio, and this Help Manual."),
            ("Center Stage View", "Displays tracks, rich album grids, interactive folder trees, or the Settings Studio based on your sidebar selection."),
            ("Right Sliding Drawer", "Toggle between your live Up Next Playback Queue (drag to reorder, clear queue) and Time-Synchronized LRC Lyrics that automatically scroll in harmony with the music."),
            ("Bottom Master Bar", "Global playback controls (Play, Pause, Previous, Next, Shuffle, Repeat), dual-tone scrubbing seekbar, volume control, mute, and live audio visualizer switcher."),
            ("Floating Equalizer Window", "Click the EQ icon in the top header to summon the floating studio window. You can freely drag it anywhere on screen or pin it center-stage."),
        ];

        for (name, desc) in layout_points {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_lg, 0.0);
                ui.label(egui::RichText::new(tr!("•")).color(theme.accent_warm));
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xxs);
                    ui.label(
                        egui::RichText::new(name)
                            .strong()
                            .color(theme.text_primary)
                            .font(crate::theme::font(13.0)),
                    );
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(desc)
                                .color(theme.text_secondary)
                                .font(crate::theme::font(12.5)),
                        )
                        .wrap(),
                    );
                });
            });
            ui.add_space(4.0);
        }
    });
}
