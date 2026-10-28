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

    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            &tr!("4. DSD Transport, Filters & Noise Shaping"),
            "How one-bit sources reach the DAC, and which filters run on the way",
            theme,
        );

        let dsd_features = [
            ("DSD Transport", "PCM Conversion is the compatible default: DSD is decimated to PCM and processed like any other source. DSD over PCM (DoP) carries the native bitstream inside a 24-bit PCM stream and needs a PCM container the DAC recognizes. Native DSD requires a direct hardware endpoint; ShadowPlayer only claims it when the device accepts it, and reports the fallback reason when it does not."),
            ("DSD Decimation Filter", "Used only when DSD is converted to PCM. Standard, Low, High Order, and Brickwall trade audio-band flatness against ultrasonic noise rejection; Slow Linear Phase and Slow Minimum Phase trade sharpness for a gentler time response. The filter is constructed with the decoder, so a change applies to the next DSD track."),
            ("Ultrasonic Filter", "An output-domain low-pass that removes shaped quantization noise above the audio band. Off is a bit-exact bypass. The 20 kHz setting is the sharp metering filter; the 30–50 kHz settings and the 2x/4x/8x rate-relative settings are progressively gentler. This stage can change live during playback."),
            ("Resampler Filter Family", "FFT is the historical rubato converter. Kaiser and Blackman-Harris are windowed-sinc polyphase designs; Minimum Phase removes pre-ringing at the cost of phase linearity; Apodizing rejects images just above the source Nyquist; Sinc-M is the reference-length design. The prototype is designed in f64, but the runtime bank follows the precision mode: f32 in Performance and f64 in Quality."),
            ("Noise Shaper", "Applied when integer output quantization would otherwise add distortion. TPDF is unshaped white noise; Shibata, NS5, NS9, and LNS15 are error-feedback designs that move noise out of the audible band. The quantization arithmetic runs in f64, while the shaper's error history and coefficients are f32. A new shaper applies when the output stream is opened."),
        ];

        for (title, body) in dsd_features {
            feature_card_row(ui, title, body, theme);
        }
    });

    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            &tr!("5. Native DSD, DAC Correction & Matrix FIR"),
            "One-bit modulation, device correction, and multichannel convolution routing",
            theme,
        );

        let advanced_features = [
            ("SDM Conversion (PCM to DSD)", "Converts PCM sources to a one-bit SDM bitstream for native DSD output. The fifth-order modulator suits converters with a simple analog reconstruction filter; seventh order shapes more in-band noise but needs an analog stage that tolerates more out-of-band energy. The modulator runs in f64 internally. The rate is Auto (the highest multiple the device reports) or a fixed DSD64/128/256/512 request, and only the negotiated rate is reported as active."),
            ("Apodizing Guidance", "A read-only analysis of the decoded source. It counts near-Nyquist energy bursts, normalises them to incidents per minute, and compares that count with a threshold. A high count suggests using an apodizing resampler for that track. The indicator is guidance: it never switches a filter silently, and it never modifies the audio."),
            ("DAC Correction", "A device- and rate-keyed correction bank, separate from room/headphone correction. Each profile matches the output device by a case-insensitive substring (empty or * matches every device) and a sample rate (0 matches any rate). FIR paths are one file per channel or one multichannel WAV. Latency states the delay already present in the profile; depth is the wet/dry amount. The first matching profile wins, and the engine reports which profile ran or why the bank stayed inactive. The bank reloads on the control path, so it applies live."),
            ("Matrix FIR Routing", "Sparse per-route convolution for multichannel output. Each route sends a source channel through a mono impulse response into a destination channel, with per-route gain and delay. Up to 16 routes and IRs up to 262,144 samples are accepted. The routing stage only runs on the >2-channel path; stereo uses balance and crossfeed instead. The partition block adds latency, which the engine reports. Matrix FIR runs in f32 even in Quality mode."),
            ("Precision Mode", "Performance (f32) is the default. Quality promotes the stereo mix graph to f64 and demotes back to f32 before output conversion. More than two channels stay on the f32 path regardless of the mode. The DSD decimator, ultrasonic stage, apodizing detector, and matrix FIR are f32; SDM, dither quantization, DAC correction, and the resampler have f64 execution paths."),
        ];

        for (title, body) in advanced_features {
            feature_card_row(ui, title, body, theme);
        }
    });
}
