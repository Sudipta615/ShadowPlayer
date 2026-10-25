//! Spatial 3D and binaural acoustics tab with the interactive radar.

use super::widgets::{caption_box, card_title, feature_card_row, manual_card};
use super::HelpState;
use crate::theme::Theme;
use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

// ─────────────────────────────────────────────────────────────────────────────
// 7. Tab: Spatial 3D & Binaural Acoustics (Interactive Radar)
// ─────────────────────────────────────────────────────────────────────────────

pub(super) fn render_spatial_tab(ui: &mut egui::Ui, state: &mut HelpState, theme: &Theme) {
    manual_card(ui, theme, |ui| {
        card_title(ui, &tr!("1. 3D Binaural HRTF & Virtual Room Acoustics"), "How Head-Related Transfer Function impulse responses recreate realistic three-dimensional sound on headphones", theme);

        let avail_w = ui.available_width();
        let img_h = avail_w * (768.0 / 1376.0);
        let img = egui::Image::new(egui::include_image!("../../../assets/help/spatial_3d.jpg"))
            .corner_radius(CornerRadius::same(theme.radius_lg))
            .fit_to_exact_size(Vec2::new(avail_w, img_h));
        ui.add(img);

        ui.add_space(6.0);
        caption_box(ui, "Figure 4: 3D Binaural Soundstage diagram illustrating acoustic wavefront propagation, ear pinna reflection arcs, interaural time delays, and virtual listening room wall reflections.", theme);
    });

    // Interactive Spatial Radar Simulator
    manual_card(ui, theme, |ui| {
        card_title(ui, &tr!("2. Interactive Binaural Soundstage Simulator"), "Drag the azimuth angle and distance sliders to inspect simulated ear delay and shadowing", theme);

        let avail_w = ui.available_width();
        let radar_size = 180.0_f32;
        let controls_w = (avail_w - radar_size - 24.0).max(200.0);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);

            // Controls
            ui.allocate_ui_with_layout(
                Vec2::new(controls_w, radar_size),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_sm);
                    ui.label(
                        egui::RichText::new(tr!("Acoustic Coordinates:"))
                            .strong()
                            .color(theme.accent_warm),
                    );

                    ui.horizontal(|ui| {
                        ui.label(tr!(Msg::S52F4CACF3D8016A3));
                        ui.add(
                            egui::Slider::new(&mut state.demo_azimuth_deg, -180.0..=180.0)
                                .suffix("°"),
                        );
                    });

                    ui.horizontal(|ui| {
                        ui.label(tr!(Msg::S9D797C69369C407F));
                        ui.add(
                            egui::Slider::new(&mut state.demo_distance_m, 0.5..=10.0).suffix(" m"),
                        );
                    });

                    // Calculated psychoacoustic readouts
                    let rad = state.demo_azimuth_deg.to_radians();
                    let ear_dist_m = 0.215_f32; // average ear separation
                    let speed_of_sound = 343.0_f32; // m/s
                    let delta_dist = ear_dist_m * rad.sin();
                    let itd_us = (delta_dist / speed_of_sound * 1_000_000.0).round();
                    let ild_db = (rad.sin() * 7.5).round();

                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(tr!("Calculated Psychoacoustic Cues:"))
                            .strong()
                            .color(theme.bit_perfect_green),
                    );
                    ui.label(format!("• Interaural Time Difference (ITD): {} µs", itd_us));
                    ui.label(format!(
                        "• Interaural Level Difference (ILD): {} dB",
                        ild_db
                    ));
                    ui.label(format!(
                        "• Inverse-Square Sound Attenuation: -{:.1} dB",
                        20.0 * state.demo_distance_m.log10()
                    ));
                },
            );

            // Radar Visualizer Painter
            let (radar_rect, _) =
                ui.allocate_exact_size(Vec2::new(radar_size, radar_size), Sense::hover());
            render_radar_painter(ui, radar_rect, state, theme);
        });
    });

    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            &tr!("3. Spatial & Acoustics Settings Guide"),
            "Understanding SOFA profiles, listener tracking smoothing, and RT60 room decay",
            theme,
        );

        let spatial_features = [
            ("Spatial Master Enable", "One switch owns the whole spatial stage. With it off, HRTF, room, head-tracking and listener settings are bypassed and the remaining spatial controls are disabled. Settings stay saved, so re-enabling restores the same soundstage. Crossfeed and correction are separate stages in the DSP studio's Filters tab."),
            ("Spatial Synthesis Quality (Low, Medium, High, Ultra)", "Controls the order of spherical harmonics and the convolution FIR filter length. 'High' (recommended) provides 512-point convolution for pristine localization without excessive CPU consumption. 'Ultra' expands to 2048-point convolution."),
            ("HRTF Profile ID (SOFA Datasets)", "ShadowPlayer supports Spatially Oriented Format for Acoustics (SOFA) standardized files. Default profiles are derived from the KEMAR acoustic head mannequin and IRCAM 1002 studio datasets."),
            ("Listener Motion Smoothing (ms)", "When head tracking sensors or mouse azimuth adjustments occur, instant jumps cause clicks. This low-pass interpolation filter smoothly ramps virtual listener position over a 15–50 ms window."),
            ("Virtual Room Acoustics Simulation", "Headphones sound unnaturally 'in-your-head' because real rooms have wall reflections. Enabling Room Acoustics models a physical room with adjustable dimensions (width, depth, height) and reflections."),
            ("RT60 Reverberation Decay Time", "The time in milliseconds for acoustic reflections to decay by 60 dB. Small listening rooms have an RT60 of 150–250 ms, while large concert halls have an RT60 of 1500–2500 ms."),
        ];

        for (title, body) in spatial_features {
            feature_card_row(ui, title, body, theme);
        }
    });
}

pub(super) fn render_radar_painter(
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

    let center = rect.center();
    let max_radius = rect.width() * 0.42;

    // Range rings
    p.circle_stroke(
        center,
        max_radius * 0.33,
        Stroke::new(1.0, theme.border_subtle),
    );
    p.circle_stroke(
        center,
        max_radius * 0.66,
        Stroke::new(1.0, theme.border_subtle),
    );
    p.circle_stroke(center, max_radius, Stroke::new(1.0, theme.border_subtle));

    // Crosshairs
    p.line_segment(
        [
            Pos2::new(rect.left() + 10.0, center.y),
            Pos2::new(rect.right() - 10.0, center.y),
        ],
        Stroke::new(1.0, theme.border_subtle),
    );
    p.line_segment(
        [
            Pos2::new(center.x, rect.top() + 10.0),
            Pos2::new(center.x, rect.bottom() - 10.0),
        ],
        Stroke::new(1.0, theme.border_subtle),
    );

    // Listener Head in center
    p.circle_filled(center, 9.0, theme.accent);
    // Ears
    p.circle_filled(Pos2::new(center.x - 11.0, center.y), 3.0, theme.accent_warm);
    p.circle_filled(Pos2::new(center.x + 11.0, center.y), 3.0, theme.accent_warm);
    // Nose pointer (facing North)
    p.line_segment(
        [center, Pos2::new(center.x, center.y - 14.0)],
        Stroke::new(2.0, theme.accent_highlight),
    );

    // Source position
    let rad = (state.demo_azimuth_deg - 90.0).to_radians();
    let dist_norm = (state.demo_distance_m / 10.0).clamp(0.2, 1.0);
    let src_x = center.x + rad.cos() * (max_radius * dist_norm);
    let src_y = center.y + rad.sin() * (max_radius * dist_norm);
    let src_pos = Pos2::new(src_x, src_y);

    // Connecting line & speaker dot
    p.line_segment(
        [center, src_pos],
        Stroke::new(1.5, theme.accent_highlight.linear_multiply(0.5)),
    );
    p.circle_filled(src_pos, 7.0, theme.bit_perfect_green);
    p.circle_stroke(src_pos, 11.0, Stroke::new(1.2, theme.bit_perfect_green));
}
