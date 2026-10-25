//! Equalizer, acoustic tuning, and filter tab with the response sandbox.

use super::widgets::{caption_box, card_title, feature_card_row, manual_card};
use super::HelpState;
use crate::theme::Theme;
use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

// ─────────────────────────────────────────────────────────────────────────────
// 5. Tab: Equalizer, Acoustic Tuning & Filters (Interactive Sandbox)
// ─────────────────────────────────────────────────────────────────────────────

pub(super) fn render_equalizer_tab(ui: &mut egui::Ui, state: &mut HelpState, theme: &Theme) {
    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            &tr!("1. Equalizer Spectrum & Filter Topologies"),
            "Understanding graphic bands, shelves, Q factors, and frequency bands",
            theme,
        );

        let avail_w = ui.available_width();
        let img_h = avail_w * (768.0 / 1376.0);
        let img = egui::Image::new(egui::include_image!(
            "../../../assets/help/equalizer_bands.jpg"
        ))
        .corner_radius(CornerRadius::same(theme.radius_lg))
        .fit_to_exact_size(Vec2::new(avail_w, img_h));
        ui.add(img);

        ui.add_space(6.0);
        caption_box(ui, "Figure 2: Audio Spectrum Analyzer showing the division of the human audible spectrum (20 Hz to 20 kHz) into Sub-Bass, Bass, Low Mids, Midrange, High Mids, Treble, and Air, illustrating Low Shelf, Bell/Peaking, and High Shelf filter responses.", theme);
    });

    // Interactive Filter Response Sandbox
    manual_card(ui, theme, |ui| {
        card_title(ui, &tr!("2. Interactive Filter Sandbox"), "Experiment with simulated filter gains below to see how frequency response curves shape audio in real time", theme);

        let avail_w = ui.available_width();
        let controls_w = (avail_w * 0.42).clamp(240.0, 320.0);
        let canvas_w = (avail_w - controls_w - 20.0).max(180.0);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_3xl, 0.0);

            // Column 1: Controls
            ui.allocate_ui_with_layout(
                Vec2::new(controls_w, 160.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_md);
                    ui.label(
                        egui::RichText::new(tr!("Simulated Controls:"))
                            .strong()
                            .color(theme.accent_warm),
                    );

                    ui.horizontal(|ui| {
                        ui.label(tr!(Msg::S117ABC28FA94C587));
                        let resp = ui.add(
                            egui::Slider::new(&mut state.demo_shelf_gain, -12.0..=12.0)
                                .suffix(" dB"),
                        );
                        resp.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::Role::Slider,
                                true,
                                "Bass Shelf (80 Hz)",
                            )
                        });
                    });

                    ui.horizontal(|ui| {
                        ui.label(tr!(Msg::S13647365883E2FF6));
                        let resp = ui.add(
                            egui::Slider::new(&mut state.demo_peak_gain, -12.0..=12.0)
                                .suffix(" dB"),
                        );
                        resp.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::Role::Slider,
                                true,
                                "Mid Peak (1.5 kHz)",
                            )
                        });
                    });

                    ui.horizontal(|ui| {
                        ui.label(tr!(Msg::SEA1E5DECF132858A));
                        let resp = ui.add(egui::Slider::new(&mut state.demo_peak_q, 0.5..=5.0));
                        resp.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::Role::Slider,
                                true,
                                "Mid Peak Q (Width)",
                            )
                        });
                    });

                    ui.horizontal(|ui| {
                        ui.label(tr!(Msg::SB01A7376DDC5505E));
                        let resp = ui.add(
                            egui::Slider::new(&mut state.demo_treble_gain, -12.0..=12.0)
                                .suffix(" dB"),
                        );
                        resp.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::Role::Slider,
                                true,
                                "Treble Shelf (10 kHz)",
                            )
                        });
                    });

                    if ui.button(tr!(Msg::SD13B7F04122611D4)).clicked() {
                        state.demo_shelf_gain = 0.0;
                        state.demo_peak_gain = 0.0;
                        state.demo_treble_gain = 0.0;
                        state.demo_peak_q = 1.4;
                    }
                },
            );

            // Column 2: Live Drawn Frequency Response Curve
            let (canvas_rect, _) =
                ui.allocate_exact_size(Vec2::new(canvas_w, 160.0), Sense::hover());
            render_frequency_response_curve_painter(ui, canvas_rect, state, theme);
        });
    });

    // Detailed Section Breakdown
    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            &tr!("3. Complete Equalizer Settings Guide"),
            "Every slider, toggle, and acoustic mode explained",
            theme,
        );

        let eq_features = [
            ("Graphic vs Parametric / AutoEQ Mode", "The EQ stage plays exactly one curve at a time. Graphic mode uses the 10, 15, 31 or 64 band sliders. Parametric / AutoEQ mode plays the imported measurement curve. Both models stay in memory, so switching modes restores the other curve instead of flattening it. The master switch and the preamp are shared by both modes."),
            ("Graphic EQ Band Topology (10, 15, 31, 64 Bands)", "Standard consumer EQs offer 10 bands spaced at 1-octave intervals (32Hz, 64Hz, 125Hz... 16kHz). ShadowPlayer also includes 15 bands (2/3-octave), 31 bands (1/3-octave studio standard), and 64 bands (1/6-octave surgical tuning) to eliminate specific room resonances or headphone peaks. Changing the layout interpolates the visible curve onto the new ladder."),
            ("Master Preamp Gain", "The single preamp shared by graphic and parametric modes. When you boost any frequency band by +4 dB, the audio waveform becomes +4 dB taller. If a song is already mastered near maximum 0 dBFS, this causes digital clipping. The Master Preamp slider allows you to dial in negative gain (e.g. -4 dB) so your boosts have adequate headroom. A custom preset stores and restores its own preamp."),
            ("Bass Shelf & Treble Shelf", "Unlike bell curves that target a narrow frequency, shelving filters transition cleanly below a corner frequency (Bass Shelf < 150 Hz) or above a corner frequency (Treble Shelf > 6 kHz) to gently warm up or brighten the entire acoustic balance."),
            ("Acoustic Stereo Width", "Uses Mid/Side matrixing: Left + Right = Mid (Center sound), Left - Right = Side (Spatial stereo ambiance). Boosting width to 125%–135% opens up the stereo soundstage on headphones without distorting vocal centering."),
            ("Extended Gain Range (±24 dB)", "Standard EQ mode caps adjustments at ±12 dB for safety. Toggling Extended Range unlocks full ±24 dB range for specialized acoustic calibration or aggressive room-mode notch suppression. Pushing a slider all the way up really does apply that boost; it is not silently capped by the headroom stage."),
            ("Static Auto Headroom (Optional, Off by Default)", "When enabled, measures the curve's combined peak boost and applies exactly that much attenuation before the filters. A +12 dB bass boost therefore plays 0 dB at the bass and −12 dB everywhere else: the shape is preserved, but the whole track is quieter. It is a static offset, not a dynamic processor. It is off by default so the boost you ask for is the boost you hear."),
            ("Output Limiter & Compressor", "The true-peak look-ahead limiter is the output safety net and is on by default at a −1.0 dBFS ceiling. It catches peaks that would clip after EQ, preamp and compressor gain without changing the curve. The three-band compressor is off by default; its makeup gain runs before the limiter."),
            ("Crossfeed & Correction", "Crossfeed blends a delayed, filtered copy of each channel into the other to reduce the hard left/right separation of headphones. Correction convolves playback with a measured impulse response and blends it with the dry signal. Both are optional and live in the Filters tab."),
            ("Mid/Side EQ Mode (M/S)", "Splits stereo audio into Mid (monophonic center) and Side (spatial differences). Allows EQ-ing centered bass and lead vocals independently from wide ambient reverbs and panned guitars."),
            ("AutoEQ Headphone Calibration", &format!("Comes with {} AutoEQ parametric corrections, each carrying its measurement source and a preamp derived from the curve's combined peak response. Search by model, preview the ten filters, then apply: the correction enters Parametric mode and replaces the current parametric curve. ShadowPlayer does not import raw measurement files; to use your own correction, import a ParametricEQ or GraphicEQ text file from the Equalizer panel.", crate::state::autoeq_catalog::SHIPPED_CURVE_COUNT)),
        ];

        for (title, body) in eq_features {
            feature_card_row(ui, title, body, theme);
        }
    });

    // The corpus is compiled into the binary. The MIT license therefore has
    // to travel with the binary, not only with the repository.
    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            "AutoEQ Attribution",
            "Third-party correction data and its license",
            theme,
        );
        ui.label(
            egui::RichText::new(
                "The parametric AutoEQ browser ships correction curves created by Jaakko Pasanen and the AutoEQ project (github.com/jaakkopasanen/AutoEq), used under the MIT license. Poweramp is not affiliated with ShadowPlayer and does not endorse it; the curves were extracted from the AutoEQ-derived preset database that Poweramp bundles.",
            )
            .font(crate::theme::font(11.5))
            .color(theme.text_secondary),
        );
        ui.add_space(6.0);
        egui::CollapsingHeader::new(
            egui::RichText::new("AutoEQ MIT license")
                .font(crate::theme::font(11.5))
                .color(theme.text_primary),
        )
        .id_salt("autoeq_license")
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(crate::state::autoeq_catalog::AUTO_EQ_LICENSE)
                    .font(crate::theme::mono_font(9.5))
                    .color(theme.text_muted),
            );
        });
    });
}

pub(super) fn render_frequency_response_curve_painter(
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

    // Grid lines for 0 dB, +12 dB, -12 dB
    let y_zero = rect.center().y;
    let y_plus12 = rect.top() + 20.0;
    let y_minus12 = rect.bottom() - 20.0;

    p.line_segment(
        [
            Pos2::new(rect.left(), y_zero),
            Pos2::new(rect.right(), y_zero),
        ],
        Stroke::new(1.0, theme.border_subtle),
    );
    p.text(
        Pos2::new(rect.left() + 8.0, y_zero - 6.0),
        egui::Align2::LEFT_BOTTOM,
        "0 dB",
        crate::theme::mono_font(9.5),
        theme.text_muted,
    );
    p.text(
        Pos2::new(rect.left() + 8.0, y_plus12 + 2.0),
        egui::Align2::LEFT_TOP,
        "+12 dB",
        crate::theme::mono_font(9.5),
        theme.text_muted,
    );
    p.text(
        Pos2::new(rect.left() + 8.0, y_minus12 - 2.0),
        egui::Align2::LEFT_BOTTOM,
        "-12 dB",
        crate::theme::mono_font(9.5),
        theme.text_muted,
    );

    // Compute curve points across logarithmic frequency range (20 Hz to 20,000 Hz)
    let steps = 120;
    let mut points = Vec::with_capacity(steps);

    for i in 0..steps {
        let t = i as f32 / (steps - 1) as f32;
        let freq = 20.0 * (1000.0_f32).powf(t); // log range 20 to 20,000

        // Simulated filter evaluation
        let shelf_factor = (80.0 / freq).min(1.0).powf(1.5);
        let shelf_gain = state.demo_shelf_gain * shelf_factor;

        let treble_factor = (freq / 10000.0).min(1.0).powf(1.5);
        let treble_gain = state.demo_treble_gain * treble_factor;

        // Peaking bell around 1500 Hz
        let oct_diff = (freq / 1500.0).log2().abs();
        let bell_factor = (-oct_diff * state.demo_peak_q * 1.8).exp().max(0.0);
        let peak_gain = state.demo_peak_gain * bell_factor;

        let total_db = shelf_gain + treble_gain + peak_gain;
        let clamped_db = total_db.clamp(-14.0, 14.0);

        let x = rect.left() + t * rect.width();
        let y = y_zero - (clamped_db / 12.0) * (y_zero - y_plus12);
        points.push(Pos2::new(x, y));
    }

    // Draw shaded area
    let mut mesh_points = points.clone();
    mesh_points.push(Pos2::new(rect.right(), y_zero));
    mesh_points.push(Pos2::new(rect.left(), y_zero));

    for i in 0..points.len() - 1 {
        p.line_segment(
            [points[i], points[i + 1]],
            Stroke::new(2.5, theme.accent_warm),
        );
    }
}
