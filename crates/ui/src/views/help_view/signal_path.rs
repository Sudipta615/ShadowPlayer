//! Audio signal path tab: the interactive block diagram and inspector.

use super::widgets::{badge, card_title, exclusive_backend_sentence, manual_card, spec_badge};
use super::{HelpState, SignalBlock};
use crate::theme::Theme;
use eframe::egui::{self, CornerRadius, Pos2, Sense, Stroke, Vec2};

// ─────────────────────────────────────────────────────────────────────────────
// 4. Tab: Audio Signal Path (Interactive Block Diagram)
// ─────────────────────────────────────────────────────────────────────────────

pub(super) fn render_signal_path_tab(ui: &mut egui::Ui, state: &mut HelpState, theme: &Theme) {
    manual_card(ui, theme, |ui| {
        card_title(ui, &tr!("1. Interactive Audio Signal Flow"), "Click any node in the digital processing chain below to inspect its sample mathematics", theme);

        // Clickable interactive block diagram
        render_interactive_pipeline_diagram(ui, state, theme);

        ui.add_space(10.0);

        // Detail Inspector for selected block
        if let Some(block) = state.selected_signal_block {
            render_signal_block_inspector(ui, block, theme);
        }
    });

    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            &tr!("2. Internal Mathematical Precision & Buffer Health"),
            "64-bit double precision IEEE 754 vs 32-bit single precision",
            theme,
        );

        ui.label(
            egui::RichText::new(
                tr!("When audio processing modifies samples (such as applying +3 dB of bass or cutting 1 kHz), rounding errors inevitably accumulate in digital arithmetic. Standard 16-bit processing causes audible quantization distortion. 32-bit float offers a theoretical noise floor of -144 dBFS, while ShadowPlayer's internal 64-bit double precision engine provides a staggering -300+ dBFS dynamic range. This means you can chain multiple biquad filters, shelving curves, spatializers, and gain offsets without losing a single micro-fraction of resolution.")
            )
            .font(crate::theme::font(13.0))
            .color(theme.text_secondary),
        );

        ui.add_space(8.0);

        ui.horizontal(|ui| {
            spec_badge(ui, "PRECISION", "64-bit IEEE Float", theme.accent, theme);
            spec_badge(
                ui,
                "BUFFER JITTER",
                "< 0.05 ms",
                theme.bit_perfect_green,
                theme,
            );
            spec_badge(
                ui,
                "INTERNAL NOISE FLOOR",
                "-310 dBFS",
                theme.accent_warm,
                theme,
            );
            spec_badge(
                ui,
                "THREAD LOCKS",
                "0 (Lock-Free Atomics)",
                theme.accent_highlight,
                theme,
            );
        });
    });
}

pub(super) fn render_interactive_pipeline_diagram(
    ui: &mut egui::Ui,
    state: &mut HelpState,
    theme: &Theme,
) {
    let blocks = [
        (SignalBlock::Decoder, "1. Decoder", "Symphonia PCM"),
        (SignalBlock::RingBuffer, "2. RingBuffer", "Lock-Free Atomic"),
        (SignalBlock::Resampler, "3. Resampler", "Rubato Sinc FIR"),
        (SignalBlock::Equalizer, "4. DSP / EQ Rack", "64-bit Biquads"),
        (
            SignalBlock::LimiterHeadroom,
            "5. Auto Headroom",
            "Dynamic Limiter",
        ),
        (SignalBlock::Dither, "6. TPDF Dither", "Noise Shaping"),
        (
            SignalBlock::AudioBackend,
            "7. Driver Output",
            "ALSA / WASAPI",
        ),
        (SignalBlock::HardwareDac, "8. DAC Unit", "Bit-Exact Analog"),
    ];

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_lg, theme.space_lg);

        for (block, title, subtitle) in blocks {
            let is_selected = state.selected_signal_block == Some(block);
            let (bg, border, fg) = if is_selected {
                (
                    theme.accent.linear_multiply(0.3),
                    theme.accent_highlight,
                    theme.accent_warm,
                )
            } else {
                (theme.bg_card_hover, theme.border_card, theme.text_primary)
            };

            let (rect, resp) = ui.allocate_exact_size(Vec2::new(135.0, 56.0), Sense::click());
            resp.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::Role::Button,
                    true,
                    is_selected,
                    format!("{title}: {subtitle}"),
                )
            });
            let painter = ui.painter_at(rect);

            painter.rect_filled(rect, CornerRadius::same(theme.radius_lg), bg);
            painter.rect_stroke(
                rect,
                CornerRadius::same(theme.radius_lg),
                Stroke::new(if is_selected { 2.0 } else { 1.0 }, border),
                egui::StrokeKind::Inside,
            );

            painter.text(
                Pos2::new(rect.center().x, rect.top() + 16.0),
                egui::Align2::CENTER_CENTER,
                title,
                crate::theme::font(12.5),
                fg,
            );
            painter.text(
                Pos2::new(rect.center().x, rect.top() + 36.0),
                egui::Align2::CENTER_CENTER,
                subtitle,
                crate::theme::font(10.5),
                theme.text_muted,
            );

            if resp.clicked() {
                state.selected_signal_block = Some(block);
            }
        }
    });
}

pub(super) fn render_signal_block_inspector(ui: &mut egui::Ui, block: SignalBlock, theme: &Theme) {
    let inspect_frame = egui::Frame::NONE
        .fill(theme.bg_pill.linear_multiply(0.8))
        .stroke(Stroke::new(1.2, theme.accent))
        .corner_radius(CornerRadius::same(theme.radius_lg))
        .inner_margin(egui::Margin::symmetric(
            theme.space_4xl as i8,
            theme.space_2xl as i8,
        ));

    inspect_frame.show(ui, |ui| {
        let backend_sentence = exclusive_backend_sentence();
        let (title, input_type, output_type, plain_desc, tech_desc) = match block {
            SignalBlock::Decoder => (
                "Stage 1: Multi-Format Audio Decoder (Symphonia Engine)",
                "Compressed/Lossless bytes from disk (FLAC, ALAC, WAV, AIFF, DSD, MP3, Opus)",
                "Uncompressed 32-bit/64-bit IEEE floating-point PCM sample frames",
                "Reads your music file from the hard drive and unpacks the compressed data into pure, raw sound wave numbers.",
                "Uses pure Rust Symphonia decoders. Zero unsafe native C libraries. Decodes high-resolution PCM (up to 384 kHz / 32-bit) and 1-bit Direct Stream Digital (DSD) without rounding or clipping.".to_string(),
            ),
            SignalBlock::RingBuffer => (
                "Stage 2: Lock-Free Atomic Inter-Thread RingBuffer",
                "Decoded PCM audio frames from worker thread",
                "Synchronized real-time playback buffer for DSP engine",
                "A high-speed waiting room for music samples that prevents music from stuttering even when your CPU is busy with other apps.",
                "Implemented with lock-free crossbeam atomic primitives. Decouples the variable-latency file I/O thread from the ultra-strict real-time audio thread to guarantee zero jitter and zero thread-locking priority inversions.".to_string(),
            ),
            SignalBlock::Resampler => (
                "Stage 3: Polyphase FIR Sinc Resampler (Rubato)",
                "Input track sample rate (e.g. 44.1 kHz CD audio, 96 kHz Hi-Res)",
                "Hardware DAC target sample rate (or Follow Track exact rate)",
                "Changes the speed of the digital clock if your soundcard doesn't match the song's native recording rate.",
                "Employs Rubato's band-limited Sinc polyphase interpolation with windowed Chebyshev filter designs. Delivers >160 dB Signal-to-Noise Ratio (SNR) with zero aliasing artifacts into the audible band.".to_string(),
            ),
            SignalBlock::Equalizer => (
                "Stage 4: 64-bit DSP Filter Rack & Acoustic Shaping",
                "Clean PCM stereo samples",
                "Acoustically compensated audio (Graphic EQ, Shelves, Stereo Width, HRTF)",
                "The tone control studio: allows you to boost bass punch, smooth vocal harshness, widen the stereo stage, or tune headphones.",
                "Executes minimum-phase 2nd-order Direct Form II Transposed biquad filter topologies. Implements Robert Bristow-Johnson Audio EQ Cookbook algorithms recalculated on every parameter tick.".to_string(),
            ),
            SignalBlock::LimiterHeadroom => (
                "Stage 5: Master Limiter & Auto Headroom Management",
                "Audio samples potentially exceeding 0.0 dBFS due to EQ boosts",
                "Guaranteed clean samples bounded strictly at or below 0.0 dBFS",
                "A smart volume protector that automatically dials back the master volume when you boost the bass, so your speakers never crackle or distort.",
                "Monitors inter-sample peaks using 4x oversampling. When EQ boosts push signal amplitude beyond 1.0 peak float, Auto Headroom applies an exact inverse attenuation offset, completely preventing harsh DAC digital overs.".to_string(),
            ),
            SignalBlock::Dither => (
                "Stage 6: TPDF Triangular Probability Dithering",
                "High-precision 64-bit floating point audio",
                "Bit-depth matched PCM samples (16-bit or 24-bit integer)",
                "Adds an ultra-quiet, completely inaudible microscopic hiss that prevents digital distortion when music fades into quiet silence.",
                "Generates high-pass filtered Triangular Probability Density Function (TPDF) noise. Eliminates quantization harmonic distortion and truncation artifacts when down-converting from 64-bit float to 16/24-bit DAC depth.".to_string(),
            ),
            SignalBlock::AudioBackend => (
                "Stage 7: Exclusive Kernel-Level Driver Interface",
                "Processed PCM sample frames",
                "Direct DMA hardware endpoint buffer",
                "Hands the audio directly to your physical soundcard, completely ignoring Windows/Mac/Linux volume mixers.",
                format!(
                    "Interfaces directly via {backend_sentence}. Completely bypasses the OS shared mixer (PulseAudio/PipeWire/Windows Audio Engine)."
                ),
            ),
            SignalBlock::HardwareDac => (
                "Stage 8: Digital-to-Analog Converter (DAC) & Line Output",
                "Pristine bit-exact binary stream",
                "Continuous smooth analog voltage wave for your headphones / speakers",
                "The physical chip that turns digital numbers into real analog sound waves that vibrate your headphone drivers.",
                "Your external DAC (e.g. ESS Sabre, AKM, R-2R ladder) reconstructs the analog wave using its native crystal clock without jitter or operating-system interference.".to_string(),
            ),
        };

        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_sm);
            ui.label(egui::RichText::new(title).font(crate::theme::font(15.0)).strong().color(theme.accent_warm));
            ui.separator();

            ui.horizontal_top(|ui| {
                ui.label(egui::RichText::new(tr!("Input Format:")).strong().color(theme.text_primary));
                ui.add(egui::Label::new(egui::RichText::new(input_type).color(theme.text_secondary)).wrap());
            });
            ui.horizontal_top(|ui| {
                ui.label(egui::RichText::new(tr!("Output Format:")).strong().color(theme.text_primary));
                ui.add(egui::Label::new(egui::RichText::new(output_type).color(theme.text_secondary)).wrap());
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                badge(
                    ui,
                    "PLAIN ENGLISH",
                    theme.bit_perfect_green.linear_multiply(0.2),
                    theme.bit_perfect_green,
                    theme,
                );
                ui.label(egui::RichText::new(tr!("Function Overview:")).strong().color(theme.bit_perfect_green));
            });
            ui.add(egui::Label::new(egui::RichText::new(plain_desc).font(crate::theme::font(12.5)).color(theme.text_secondary)).wrap());

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                badge(
                    ui,
                    "DSP SPECS",
                    theme.accent.linear_multiply(0.2),
                    theme.accent_warm,
                    theme,
                );
                ui.label(egui::RichText::new(tr!("Engineering & Mathematics:")).strong().color(theme.accent_warm));
            });
            ui.add(egui::Label::new(egui::RichText::new(tech_desc).font(crate::theme::font(12.0)).color(theme.text_muted)).wrap());
        });
    });
}
