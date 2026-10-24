//! Playback dynamics/DSP and mix-bus studio tabs.

use super::widgets::{badge, card_title, manual_card};
use super::HelpState;
use crate::theme::Theme;
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

// ─────────────────────────────────────────────────────────────────────────────
// 8. Tab: Playback Dynamics, Normalization & DSP
// ─────────────────────────────────────────────────────────────────────────────

pub(super) fn render_playback_tab(ui: &mut egui::Ui, state: &mut HelpState, theme: &Theme) {
    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            &tr!("1. Processing Engine: Varispeed vs TimeStretch"),
            "Analog tape pitch shift vs modern decoupled granular time-stretching",
            theme,
        );

        ui.label(
            egui::RichText::new(
                tr!("When you speed up audio, two physical approaches exist. Varispeed acts like speeding up the motor of an analog tape machine or vinyl record: playing 1.5x faster shifts all frequencies up proportionally. This preserves 100% micro-phase coherency with zero digital artifacts. In contrast, TimeStretch uses a phase vocoder to keep musical pitch fixed while altering speed.")
            )
            .font(crate::theme::font(13.0))
            .color(theme.text_secondary),
        );

        ui.add_space(8.0);

        // Interactive Speed Demo
        ui.horizontal(|ui| {
            ui.label(tr!(Msg::S9EEBF2448150F573));
            ui.add(egui::Slider::new(&mut state.demo_speed, 0.25..=4.0).suffix("x"));
            let semitones = 12.0 * state.demo_speed.log2();
            badge(
                ui,
                format!("Varispeed Pitch: {:+.2} semitones", semitones),
                theme.accent,
                Color32::WHITE,
                theme,
            );
        });
    });

    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            &tr!("2. Track Transitions: Gapless Direct vs Crossfades"),
            "Why Constant Power crossfading prevents volume drops",
            theme,
        );

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(tr!("Crossfade Curve Type:"))
                    .strong()
                    .color(theme.accent_warm),
            );

            let curves = ["Constant Power (Equal Energy)", "Linear", "S-Curve"];
            for (idx, name) in curves.iter().enumerate() {
                let is_active = state.demo_crossfade_curve == idx;
                if ui.selectable_label(is_active, *name).clicked() {
                    state.demo_crossfade_curve = idx;
                }
            }
        });

        ui.add_space(4.0);
        ui.label(egui::RichText::new(match state.demo_crossfade_curve {
            0 => "Constant Power (cos² + sin² = 1): Maintains constant acoustic energy across the entire blend. No dip in perceived volume.",
            1 => "Linear Crossfade: Summing two linear ramps creates a -3 dB volume dip in the middle of the transition.",
            _ => "S-Curve: Smooth ease-in and ease-out transitions, ideal for ambient soundscapes.",
        }).font(crate::theme::font(12.0)).color(theme.text_secondary));

        ui.add_space(8.0);
        // Painter for crossfade curves spanning full card width
        let (curve_rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 110.0), Sense::hover());
        render_crossfade_painter(ui, curve_rect, state, theme);
    });

    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            &tr!("3. Loudness Normalization: EBU R128 vs ReplayGain"),
            "Eliminate volume jumps without compressing dynamic range",
            theme,
        );

        ui.label(
            egui::RichText::new(
                tr!("Different songs and albums are mastered at radically different volume levels. Switching from an older 1980s jazz album to modern pop music forces you to reach for the volume dial. ShadowPlayer implements the international broadcast standard EBU R128 (-14 LUFS) alongside ReplayGain 2.0. Unlike consumer peak normalizers that cause harsh digital clipping, EBU R128 uses K-weighted human perceptual loudness filters to calculate integrated loudness and apply transparent gain offsets.")
            )
            .font(crate::theme::font(13.0))
            .color(theme.text_secondary),
        );
    });
}

pub(super) fn render_crossfade_painter(
    ui: &mut egui::Ui,
    rect: Rect,
    state: &HelpState,
    theme: &Theme,
) {
    let p = ui.painter_at(rect);
    p.rect_filled(rect, CornerRadius::same(theme.radius_lg), theme.bg_card);
    p.rect_stroke(
        rect,
        CornerRadius::same(theme.radius_lg),
        Stroke::new(1.0, theme.border_card),
        egui::StrokeKind::Inside,
    );

    let steps = 60;
    let mut out_points = Vec::with_capacity(steps);
    let mut in_points = Vec::with_capacity(steps);
    let mut sum_points = Vec::with_capacity(steps);

    let y_top = rect.top() + 16.0;
    let y_bot = rect.bottom() - 16.0;
    let h = y_bot - y_top;

    for i in 0..steps {
        let t = i as f32 / (steps - 1) as f32;
        let (fade_out, fade_in) = match state.demo_crossfade_curve {
            0 => {
                let rad = t * std::f32::consts::FRAC_PI_2;
                (rad.cos(), rad.sin())
            }
            1 => (1.0 - t, t),
            _ => {
                let s_in = t * t * (3.0 - 2.0 * t);
                (1.0 - s_in, s_in)
            }
        };

        let energy_sum = (fade_out * fade_out + fade_in * fade_in).sqrt();

        let x = rect.left() + t * rect.width();
        out_points.push(Pos2::new(x, y_bot - fade_out * h));
        in_points.push(Pos2::new(x, y_bot - fade_in * h));
        sum_points.push(Pos2::new(x, y_bot - energy_sum * h));
    }

    for i in 0..steps - 1 {
        p.line_segment(
            [out_points[i], out_points[i + 1]],
            Stroke::new(2.0, theme.accent_warm),
        );
        p.line_segment(
            [in_points[i], in_points[i + 1]],
            Stroke::new(2.0, theme.accent),
        );
        p.line_segment(
            [sum_points[i], sum_points[i + 1]],
            Stroke::new(1.5, theme.bit_perfect_green),
        );
    }

    p.text(
        Pos2::new(rect.left() + 10.0, y_top),
        egui::Align2::LEFT_TOP,
        "Fade Out (Track A)",
        crate::theme::mono_font(10.0),
        theme.accent_warm,
    );
    p.text(
        Pos2::new(rect.right() - 10.0, y_top),
        egui::Align2::RIGHT_TOP,
        "Fade In (Track B)",
        crate::theme::mono_font(10.0),
        theme.accent,
    );
    p.text(
        Pos2::new(rect.center().x, y_top),
        egui::Align2::CENTER_TOP,
        "Summed Perceived Energy",
        crate::theme::mono_font(10.0),
        theme.bit_perfect_green,
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 9. Tab: Mix Bus & Studio Recording Tools
// ─────────────────────────────────────────────────────────────────────────────

pub(super) fn render_mixbus_tab(ui: &mut egui::Ui, _state: &mut HelpState, theme: &Theme) {
    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            &tr!("1. Multi-Track Mixer & Audio Stem Ingestion"),
            "Control gain, stereo pan, and mute states for secondary audio sources",
            theme,
        );

        ui.label(
            egui::RichText::new(
                tr!("ShadowPlayer includes a multi-track mixing engine allowing you to inject auxiliary stems, click tracks, or commentary channels alongside your primary music. Each channel slot provides independent float gain (0.0x to 2.0x), true stereo pan balancing (-1.0 Left to +1.0 Right), and instantaneous mute toggling.")
            )
            .font(crate::theme::font(13.0))
            .color(theme.text_secondary),
        );
    });

    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            &tr!("2. Sidechain Ducking Compressor"),
            "Automatically dip background music when speech is detected",
            theme,
        );

        ui.label(
            egui::RichText::new(
                tr!("When broadcasting or listening with a voice channel active, manual volume fading is cumbersome. The Sidechain Ducking Compressor monitors a designated trigger slot (such as microphone input or voice stems). When audio exceeds the Threshold (dB), it smoothly attenuates the music by the Duck Depth (dB) at the specified Attack speed, restoring full volume over the Release envelope once speech stops.")
            )
            .font(crate::theme::font(13.0))
            .color(theme.text_secondary),
        );
    });

    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            &tr!("3. High-Resolution Stream Capture"),
            "Direct live bit-perfect loopback recording to uncompressed PCM WAV",
            theme,
        );

        ui.label(
            egui::RichText::new(
                tr!("Need to capture an uncompressed reference recording of live audio? The Stream Capture engine taps directly into the post-DSP master ringbuffer, writing pure 32-bit floating point PCM audio to a `.wav` file on your local storage with zero generation loss.")
            )
            .font(crate::theme::font(13.0))
            .color(theme.text_secondary),
        );
    });
}
