use crate::state::{AppState, DspState};
use crate::theme::Theme;
use eframe::egui::{self, Color32, CornerRadius, Sense, Stroke, Vec2};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HelpTab {
    #[default]
    QuickStart,
    SignalPath,
    EqualizerAcoustics,
    BitPerfectDac,
    SpatialBinaural,
    PlaybackDsp,
    MixBusStudio,
    KeyboardShortcuts,
    Glossary,
    /// What *this* build can actually do.
    ///
    /// Every capability claim in this manual is rendered from the live engine
    /// rather than written out by hand, and this tab is where the raw facts
    /// are: the compiled backends, the compiled codecs, the negotiated output
    /// device, the versions. It is the answer to "is that true on my
    /// machine?".
    ThisBuild,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalBlock {
    Decoder,
    RingBuffer,
    Resampler,
    Equalizer,
    LimiterHeadroom,
    Dither,
    AudioBackend,
    HardwareDac,
}

pub struct HelpState {
    pub active_tab: HelpTab,
    pub search_query: String,
    pub selected_signal_block: Option<SignalBlock>,
    pub expanded_deep_dives: HashSet<String>,

    // Interactive Sandbox states
    pub demo_shelf_gain: f32,
    pub demo_peak_gain: f32,
    pub demo_treble_gain: f32,
    pub demo_peak_q: f32,

    pub demo_azimuth_deg: f32,
    pub demo_distance_m: f32,

    pub demo_speed: f32,
    pub demo_crossfade_curve: usize, // 0 = Constant Power, 1 = Linear, 2 = S-Curve
    pub demo_duck_threshold: f32,
    pub demo_duck_depth: f32,
}

impl Default for HelpState {
    fn default() -> Self {
        Self {
            active_tab: HelpTab::QuickStart,
            search_query: String::new(),
            selected_signal_block: Some(SignalBlock::Decoder),
            expanded_deep_dives: HashSet::new(),

            demo_shelf_gain: 3.5,
            demo_peak_gain: -2.0,
            demo_treble_gain: 2.5,
            demo_peak_q: 1.4,

            demo_azimuth_deg: 35.0,
            demo_distance_m: 2.0,

            demo_speed: 1.0,
            demo_crossfade_curve: 0,
            demo_duck_threshold: -18.0,
            demo_duck_depth: 12.0,
        }
    }
}

pub fn render_help_view(
    ui: &mut egui::Ui,
    state: &mut HelpState,
    theme: &Theme,
    _app_state: &mut AppState,
    _dsp_state: &mut DspState,
) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_lg);

            // 1. The lead line. The page's title is the topbar's, which is
            // sticky; the 230px hero that used to sit here was the same
            // information twice, and it pushed the manual below the fold.
            crate::components::page::page_lead(
                ui,
                theme,
                "Every stage of the audio pipeline, what it does, and why. Search it, or pick a section.",
            );

            // 2. The search field
            render_manual_search(ui, state, theme);
            ui.add_space(theme.space_md);

            // 3. The section strip — the app's one tab-strip component
            render_category_nav_bar(ui, state, theme);
            ui.add_space(theme.space_lg);

            // 3. Search Results Overlay or Active Category Tab
            if !state.search_query.trim().is_empty() {
                render_search_results(ui, state, theme);
            } else {
                match state.active_tab {
                    HelpTab::QuickStart => render_quick_start_tab(ui, state, theme),
                    HelpTab::SignalPath => render_signal_path_tab(ui, state, theme),
                    HelpTab::EqualizerAcoustics => render_equalizer_tab(ui, state, theme),
                    HelpTab::BitPerfectDac => render_bit_perfect_tab(ui, state, theme),
                    HelpTab::SpatialBinaural => render_spatial_tab(ui, state, theme),
                    HelpTab::PlaybackDsp => render_playback_tab(ui, state, theme),
                    HelpTab::MixBusStudio => render_mixbus_tab(ui, state, theme),
                    HelpTab::KeyboardShortcuts => render_shortcuts_tab(ui, state, theme),
                    HelpTab::Glossary => render_glossary_tab(ui, state, theme),
                    HelpTab::ThisBuild => render_this_build_tab(ui, state, theme),
                }
            }

            ui.add_space(40.0);
        });
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. The manual's search field
// ─────────────────────────────────────────────────────────────────────────────

/// A single-line field, the same height as the library's search box.
///
/// The hero it replaces was 230px tall: a 44px icon block, a two-line title,
/// a version badge and a second row for the field. A manual is for reading,
/// so the chrome gets out of the way and the field stays where the reader
/// left it.
fn render_manual_search(ui: &mut egui::Ui, state: &mut HelpState, theme: &Theme) {
    egui::Frame::NONE
        .fill(theme.bg_input)
        .stroke(Stroke::new(1.0, theme.border_input))
        .corner_radius(CornerRadius::same(theme.radius_lg))
        .inner_margin(egui::Margin::symmetric(
            theme.space_xl as i8,
            theme.space_md as i8,
        ))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);

                let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                crate::components::icons::draw_search(ui, icon_rect, theme.text_muted);

                let has_query = !state.search_query.is_empty();
                let clear_w = if has_query { 28.0 } else { 0.0 };
                let input_w = (ui.available_width() - clear_w - 6.0).max(80.0);

                ui.add(
                    egui::TextEdit::singleline(&mut state.search_query)
                        .hint_text(
                            egui::RichText::new(tr!(
                                "Search the manual — LUFS, ASIO, HRTF, Biquad, Dither…"
                            ))
                            .color(theme.text_muted)
                            .font(crate::theme::font(12.5)),
                        )
                        .text_color(theme.text_primary)
                        .font(crate::theme::font(13.0))
                        .desired_width(input_w)
                        .frame(egui::Frame::NONE),
                );

                if has_query
                    && ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(tr!("×"))
                                    .font(crate::theme::font(14.0))
                                    .color(theme.text_muted),
                            )
                            .fill(Color32::TRANSPARENT)
                            .frame(false),
                        )
                        .clicked()
                {
                    state.search_query.clear();
                }
            });
        });
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. Category Tab Switcher Bar
// ─────────────────────────────────────────────────────────────────────────────

fn render_category_nav_bar(ui: &mut egui::Ui, state: &mut HelpState, theme: &Theme) {
    let tabs = [
        (HelpTab::QuickStart, "Overview"),
        (HelpTab::SignalPath, "Signal Chain"),
        (HelpTab::EqualizerAcoustics, "EQ & Filters"),
        (HelpTab::BitPerfectDac, "Bit-Perfect & DACs"),
        (HelpTab::SpatialBinaural, "Spatial & HRTF"),
        (HelpTab::PlaybackDsp, "Playback & DSP"),
        (HelpTab::MixBusStudio, "Mix Bus"),
        (HelpTab::KeyboardShortcuts, "Shortcuts"),
        (HelpTab::Glossary, "Glossary"),
        (HelpTab::ThisBuild, "This Build"),
    ];

    let page_tabs: Vec<crate::components::page::PageTab<'_>> = tabs
        .iter()
        .map(|(_, label)| crate::components::page::PageTab::new(label))
        .collect();
    let selected = tabs
        .iter()
        .position(|(tab, _)| *tab == state.active_tab)
        .unwrap_or(0);

    // Nine sections do not fit across a narrow window, so the strip wraps
    // rather than overflowing.
    if let Some(picked) = crate::components::page::tab_strip_wrapping(
        ui,
        theme,
        "help_sections",
        &page_tabs,
        selected,
        false,
        true,
    ) {
        state.active_tab = tabs[picked].0;
        state.search_query.clear();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tabs
// ─────────────────────────────────────────────────────────────────────────────

mod acoustics;
mod dsp_studio;
mod hardware;
mod quick_start;
mod reference;
mod signal_path;
mod spatial;
mod widgets;

use acoustics::render_equalizer_tab;
use dsp_studio::{render_mixbus_tab, render_playback_tab};
use hardware::render_bit_perfect_tab;
use quick_start::render_quick_start_tab;
use reference::{render_glossary_tab, render_shortcuts_tab, render_this_build_tab};
use signal_path::render_signal_path_tab;
use spatial::render_spatial_tab;
use widgets::{card_title, manual_card};

fn render_search_results(ui: &mut egui::Ui, state: &mut HelpState, theme: &Theme) {
    let query = state.search_query.trim().to_lowercase();

    manual_card(ui, theme, |ui| {
        card_title(
            ui,
            &format!("Search Results for \"{}\"", state.search_query.trim()),
            "Direct matches found across the ShadowPlayer user manual and technical glossary",
            theme,
        );

        let searchable_topics = [
            ("Bit-Perfect Pure Direct Stream", "Bypasses all operating system audio mixers, soft volume, EQ, and resampling for 100% bit-exact DAC delivery.", HelpTab::BitPerfectDac),
            ("Auto Headroom Management", "Dynamic digital offset ensuring zero inter-sample peak distortion when boosting equalizer bands.", HelpTab::EqualizerAcoustics),
            ("EBU R128 Loudness Normalization", "Automated broadcast standard -14 LUFS gain matching preventing sudden jumps between tracks.", HelpTab::PlaybackDsp),
            ("Varispeed Engine", "Analog tape pitch shift mode scaling tempo and musical key simultaneously without phase artifacts.", HelpTab::PlaybackDsp),
            ("Binaural HRTF Spatialization", "Convolves stereo audio through Head-Related Transfer Function impulse responses for expansive 3D headphone sound.", HelpTab::SpatialBinaural),
            ("Graphic EQ Band Topology (10, 15, 31, 64 Bands)", "Constant-Q minimum-phase filter banks for octave-spaced acoustic frequency tuning.", HelpTab::EqualizerAcoustics),
            ("TPDF Triangular Dithering", "High-resolution noise shaping eliminating quantization distortion when converting bit depth.", HelpTab::EqualizerAcoustics),
            ("Sidechain Ducking Compressor", "Automatically reduces background music level when voice or commentary channel speaks.", HelpTab::MixBusStudio),
            ("High-Resolution Stream Capture", "Record live bit-perfect audio stream directly to uncompressed PCM WAV files.", HelpTab::MixBusStudio),
            ("Keyboard Shortcuts & Gestures", "Space to play/pause, J/L to skip tracks, M for mute, S for shuffle, R for repeat, Ctrl+B for bit-perfect.", HelpTab::KeyboardShortcuts),
            ("Audio Driver Backend (ALSA / WASAPI / ASIO)", "Exclusive hardware driver architecture for direct DAC ringbuffer access.", HelpTab::BitPerfectDac),
            ("Sinc Resampling (Rubato Polyphase)", "Band-limited Whittaker-Shannon interpolation for pristine sample rate conversion without aliasing.", HelpTab::SignalPath),
            ("Lock-Free Atomic RingBuffer", "Inter-thread buffer decoupling file I/O from strict real-time audio thread without thread locks.", HelpTab::SignalPath),
            ("Multi-Format Audio Decoder (Symphonia)", "Decodes FLAC, ALAC, WAV, AIFF, DSD, MP3, and Opus into 64-bit float PCM frames.", HelpTab::SignalPath),
            ("DAC Master Clock Synchronization", "Physical crystal oscillators switching frequencies dynamically to match 44.1k and 48k families.", HelpTab::BitPerfectDac),
            ("Virtual Room Acoustics & RT60 Decay", "Acoustic reverberation modeling simulating reflections from virtual listening room walls.", HelpTab::SpatialBinaural),
            ("dBFS (Decibels relative to Full Scale)", "Digital volume unit where 0 dBFS is the mathematical ceiling; normal audio plays below 0 dBFS.", HelpTab::Glossary),
            ("LUFS / LKFS Perceptual Loudness", "ITU-R BS.1770 / EBU R128 psychoacoustic volume standard measuring human hearing response.", HelpTab::Glossary),
            ("Biquad Filter Topologies", "Second-order recursive digital IIR filters forming the building blocks of parametric and graphic EQs.", HelpTab::Glossary),
            ("Inter-Sample Peak (ISP)", "Waveform overshoots between discrete digital samples during DAC reconstruction.", HelpTab::Glossary),
            ("Crossfade Transitions (Constant Power)", "Trigonometric equal-energy blending (cos² + sin² = 1) preventing volume dips between songs.", HelpTab::PlaybackDsp),
        ];

        let mut matches_found = 0;
        for (title, snippet, tab) in searchable_topics {
            if title.to_lowercase().contains(&query) || snippet.to_lowercase().contains(&query) {
                matches_found += 1;
                ui.horizontal_top(|ui| {
                    let avail_w = ui.available_width();
                    let btn_w = 150.0;
                    let text_w = (avail_w - btn_w - 20.0).max(100.0);

                    ui.allocate_ui_with_layout(
                        Vec2::new(text_w, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xs);
                            ui.label(
                                egui::RichText::new(title)
                                    .font(crate::theme::font(14.0))
                                    .strong()
                                    .color(theme.accent_warm),
                            );
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(snippet)
                                        .font(crate::theme::font(12.5))
                                        .color(theme.text_secondary),
                                )
                                .wrap(),
                            );
                        },
                    );

                    ui.allocate_ui_with_layout(
                        Vec2::new(btn_w, 32.0),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(tr!("Jump to Section >>"))
                                            .font(crate::theme::font(11.5))
                                            .strong()
                                            .color(Color32::WHITE),
                                    )
                                    .fill(theme.accent)
                                    .corner_radius(CornerRadius::same(theme.radius_sm))
                                    .min_size(Vec2::new(135.0, 26.0)),
                                )
                                .clicked()
                            {
                                state.active_tab = tab;
                                state.search_query.clear();
                            }
                        },
                    );
                });
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);
            }
        }

        if matches_found == 0 {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(tr!("No direct matches found. Try searching for 'LUFS', 'DAC', 'EQ', 'Dither', 'ASIO', 'Crossfade', 'Biquad', or 'Varispeed'.")).color(theme.text_muted));
            if ui.button(tr!(Msg::SFB12F7530586615E)).clicked() {
                state.search_query.clear();
            }
        }
    });
}
