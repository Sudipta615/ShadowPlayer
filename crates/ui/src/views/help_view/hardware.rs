//! Bit-perfect streaming and hardware DAC tab.

use super::widgets::{
    caption_box, card_title, exclusive_backend_sentence, feature_card_row, feature_pillar_card,
    manual_card,
};
use super::HelpState;
use crate::theme::Theme;
use eframe::egui::{self, CornerRadius, Vec2};

// ─────────────────────────────────────────────────────────────────────────────
// 6. Tab: Bit-Perfect Direct Stream & Hardware DACs
// ─────────────────────────────────────────────────────────────────────────────

pub(super) fn render_bit_perfect_tab(ui: &mut egui::Ui, _state: &mut HelpState, theme: &Theme) {
    manual_card(ui, theme, |ui| {
        card_title(ui, &tr!("1. Bit-Perfect Direct Hardware Streaming"), "Why operating system audio mixers degrade sound and how ShadowPlayer achieves pure direct bitstream", theme);

        let avail_w = ui.available_width();
        let img_h = avail_w * (768.0 / 1376.0);
        let img = egui::Image::new(egui::include_image!(
            "../../../assets/help/bitperfect_dac.jpg"
        ))
        .corner_radius(CornerRadius::same(theme.radius_lg))
        .fit_to_exact_size(Vec2::new(avail_w, img_h));
        ui.add(img);

        ui.add_space(6.0);
        caption_box(ui, "Figure 3: Comparison of standard operating system audio pipeline vs ShadowPlayer's Bit-Perfect Direct Stream. Bypassing software digital mixers and sample rate converters delivers 100% byte-for-byte mathematical purity directly to your DAC.", theme);
    });

    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            &tr!("2. Why the Operating System Degrades Sound"),
            "Three critical flaws in standard OS audio engines",
            theme,
        );

        ui.columns(3, |cols| {
            cols[0].vertical(|ui| {
                feature_pillar_card(
                    ui,
                    "• Forced Resampling",
                    "Windows and PipeWire force-resample all audio to a single system rate (typically 48 kHz). Resampling 44.1 kHz CD audio creates harmonic distortion and blurred transients.",
                    theme,
                );
            });
            cols[1].vertical(|ui| {
                feature_pillar_card(
                    ui,
                    "• Bit-Depth Truncation",
                    "Digital software volume controls attenuate audio by multiplying samples by fractions. Every 6 dB of software attenuation permanently discards 1 full bit of audio resolution.",
                    theme,
                );
            });
            cols[2].vertical(|ui| {
                feature_pillar_card(
                    ui,
                    "• Hidden OS Limiters",
                    "OS mixers mix your music with system notifications, browser dings, and discord pings. To prevent digital clipping, they engage hidden brickwall limiters that squash dynamic range.",
                    theme,
                );
            });
        });
    });

    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            &tr!("3. Hardware Output Settings Reference"),
            "Configuring drivers, fallback policies, and sample rates",
            theme,
        );

        let driver_sentence = exclusive_backend_sentence();
        let output_features = [
            ("Audiophile Bit-Perfect Direct Stream", "When enabled, completely disengages all software volume attenuation, equalizer biquads, spatializers, and dithering. Hands the unaltered FLAC/WAV PCM bits straight to your external USB DAC."),
            ("Driver Architecture", &format!("Selects the low-level operating system audio subsystem. {driver_sentence} Each backend enumerates its own devices, so the device you pick in Settings is the device the engine opens.")),
            ("Fallback Policy (Allow vs Strict)", "If another application is locking your DAC when ShadowPlayer starts: 'Allow' falls back to shared mode gracefully; 'Strict' halts playback until the exclusive device can be acquired, guaranteeing bit-perfect fidelity."),
            ("Sample Rate Target (Follow Track vs Fixed)", "'Follow Track' dynamically re-clocks your DAC on song change (switching from 44.1 kHz to 96 kHz to 192 kHz without resampling). 'Fixed' locks your DAC clock to a chosen frequency and uses the Rubato polyphase sinc engine."),
            ("Volume Attenuation Mode (Software Controlled vs Hardware Preferred)", "Software mode scales digital float samples before driver handover. Hardware mode locks ShadowPlayer's output at 100% full-scale line level so you adjust physical volume on your external headphone amp or DAC dial."),
        ];

        for (title, body) in output_features {
            feature_card_row(ui, title, body, theme);
        }
    });
}
