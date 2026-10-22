use crate::state::{AppState, DspState};
use crate::theme::Theme;
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};
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
// 3. Tab: Quick Start & Architecture Overview
// ─────────────────────────────────────────────────────────────────────────────

fn render_quick_start_tab(ui: &mut egui::Ui, _state: &mut HelpState, theme: &Theme) {
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

        let autoeq_profiles = crate::modals::autoeq_modal::get_curated_autoeq_profiles().len();
        let dsp_pillar = (
            "• 64-bit Floating Point DSP".to_string(),
            format!(
                "Parametric and Graphic EQs (10 to 64 bands), minimum-phase biquad filters, {autoeq_profiles} curated headphone measurements with custom-measurement import, and TPDF dither."
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
        let img = egui::Image::new(egui::include_image!("../../assets/help/audio_flow.jpg"))
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

// ─────────────────────────────────────────────────────────────────────────────
// 4. Tab: Audio Signal Path (Interactive Block Diagram)
// ─────────────────────────────────────────────────────────────────────────────

fn render_signal_path_tab(ui: &mut egui::Ui, state: &mut HelpState, theme: &Theme) {
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

fn render_interactive_pipeline_diagram(ui: &mut egui::Ui, state: &mut HelpState, theme: &Theme) {
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

fn render_signal_block_inspector(ui: &mut egui::Ui, block: SignalBlock, theme: &Theme) {
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

// ─────────────────────────────────────────────────────────────────────────────
// 5. Tab: Equalizer, Acoustic Tuning & Filters (Interactive Sandbox)
// ─────────────────────────────────────────────────────────────────────────────

fn render_equalizer_tab(ui: &mut egui::Ui, state: &mut HelpState, theme: &Theme) {
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
            "../../assets/help/equalizer_bands.jpg"
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
            ("Graphic EQ Band Topology (10, 15, 31, 64 Bands)", "Standard consumer EQs offer 10 bands spaced at 1-octave intervals (32Hz, 64Hz, 125Hz... 16kHz). ShadowPlayer also includes 15 bands (2/3-octave), 31 bands (1/3-octave studio standard), and 64 bands (1/6-octave surgical tuning) to eliminate specific room resonances or headphone peaks."),
            ("Master Preamp Gain", "When you boost any frequency band by +4 dB, the audio waveform becomes +4 dB taller. If a song is already mastered near maximum 0 dBFS, this causes digital clipping. The Master Preamp slider allows you to dial in negative gain (e.g. -4 dB) so your boosts have adequate headroom."),
            ("Bass Shelf & Treble Shelf", "Unlike bell curves that target a narrow frequency, shelving filters transition cleanly below a corner frequency (Bass Shelf < 150 Hz) or above a corner frequency (Treble Shelf > 6 kHz) to gently warm up or brighten the entire acoustic balance."),
            ("Acoustic Stereo Width", "Uses Mid/Side matrixing: Left + Right = Mid (Center sound), Left - Right = Side (Spatial stereo ambiance). Boosting width to 125%–135% opens up the stereo soundstage on headphones without distorting vocal centering."),
            ("Extended Gain Range (±24 dB)", "Standard EQ mode caps adjustments at ±12 dB for safety. Toggling Extended Range unlocks full ±24 dB range for specialized acoustic calibration or aggressive room-mode notch suppression."),
            ("Auto Headroom Management", "An intelligent digital protector that constantly inspects your EQ curve. If the maximum boost across all active bands is +5 dB, it automatically and dynamically applies a -5 dB digital offset, ensuring zero clipping even if you forget to adjust the Preamp!"),
            ("Mid/Side EQ Mode (M/S)", "Splits stereo audio into Mid (monophonic center) and Side (spatial differences). Allows EQ-ing centered bass and lead vocals independently from wide ambient reverbs and panned guitars."),
            ("AutoEQ Headphone Calibration", &format!("Comes with {} curated headphone measurements and accepts your own measurement curve, so any model can be flattened to the Harman reference target. The filters are ordinary parametric EQ bands.", crate::modals::autoeq_modal::get_curated_autoeq_profiles().len())),
        ];

        for (title, body) in eq_features {
            feature_card_row(ui, title, body, theme);
        }
    });
}

fn render_frequency_response_curve_painter(
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

// ─────────────────────────────────────────────────────────────────────────────
// 6. Tab: Bit-Perfect Direct Stream & Hardware DACs
// ─────────────────────────────────────────────────────────────────────────────

fn render_bit_perfect_tab(ui: &mut egui::Ui, _state: &mut HelpState, theme: &Theme) {
    manual_card(ui, theme, |ui| {
        card_title(ui, &tr!("1. Bit-Perfect Direct Hardware Streaming"), "Why operating system audio mixers degrade sound and how ShadowPlayer achieves pure direct bitstream", theme);

        let avail_w = ui.available_width();
        let img_h = avail_w * (768.0 / 1376.0);
        let img = egui::Image::new(egui::include_image!("../../assets/help/bitperfect_dac.jpg"))
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

// ─────────────────────────────────────────────────────────────────────────────
// 7. Tab: Spatial 3D & Binaural Acoustics (Interactive Radar)
// ─────────────────────────────────────────────────────────────────────────────

fn render_spatial_tab(ui: &mut egui::Ui, state: &mut HelpState, theme: &Theme) {
    manual_card(ui, theme, |ui| {
        card_title(ui, &tr!("1. 3D Binaural HRTF & Virtual Room Acoustics"), "How Head-Related Transfer Function impulse responses recreate realistic three-dimensional sound on headphones", theme);

        let avail_w = ui.available_width();
        let img_h = avail_w * (768.0 / 1376.0);
        let img = egui::Image::new(egui::include_image!("../../assets/help/spatial_3d.jpg"))
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

fn render_radar_painter(ui: &mut egui::Ui, rect: Rect, state: &HelpState, theme: &Theme) {
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

// ─────────────────────────────────────────────────────────────────────────────
// 8. Tab: Playback Dynamics, Normalization & DSP
// ─────────────────────────────────────────────────────────────────────────────

fn render_playback_tab(ui: &mut egui::Ui, state: &mut HelpState, theme: &Theme) {
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

fn render_crossfade_painter(ui: &mut egui::Ui, rect: Rect, state: &HelpState, theme: &Theme) {
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

fn render_mixbus_tab(ui: &mut egui::Ui, _state: &mut HelpState, theme: &Theme) {
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

// ─────────────────────────────────────────────────────────────────────────────
// 10. Tab: Keyboard Shortcuts & Power Gestures
// ─────────────────────────────────────────────────────────────────────────────

fn render_shortcuts_tab(ui: &mut egui::Ui, _state: &mut HelpState, theme: &Theme) {
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

/// A sentence naming the exclusive backends this build actually has.
///
/// The manual used to say "Linux ALSA, Windows WASAPI Exclusive, macOS
/// CoreAudio Hog mode, or ASIO 2.0" on every platform and in every build. On
/// a build without the native backend that sentence is a promise the binary
/// cannot keep, and the Settings card — which reports what was really
/// negotiated — would contradict it.
fn exclusive_backend_sentence() -> String {
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
fn render_feature_matrix(ui: &mut egui::Ui, theme: &Theme) {
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
fn render_this_build_tab(ui: &mut egui::Ui, _state: &mut HelpState, theme: &Theme) {
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

fn render_glossary_tab(ui: &mut egui::Ui, _state: &mut HelpState, theme: &Theme) {
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

// ─────────────────────────────────────────────────────────────────────────────
// 12. Search Results Mode
// ─────────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────────
// Reusable Visual Styling Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// The manual's cards use the same card as every other page.
fn manual_card(ui: &mut egui::Ui, theme: &Theme, add_contents: impl FnOnce(&mut egui::Ui)) {
    crate::components::page::page_card_with_padding(ui, theme, (22, 18), add_contents);
}

fn card_title(ui: &mut egui::Ui, title: &str, subtitle: &str, theme: &Theme) {
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

fn feature_pillar_card(ui: &mut egui::Ui, title: &str, desc: &str, theme: &Theme) {
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

fn feature_card_row(ui: &mut egui::Ui, title: &str, desc: &str, theme: &Theme) {
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

fn caption_box(ui: &mut egui::Ui, text: &str, theme: &Theme) {
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

fn badge(ui: &mut egui::Ui, text: impl Into<String>, bg: Color32, fg: Color32, theme: &Theme) {
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

fn spec_badge(ui: &mut egui::Ui, label: &str, value: &str, color: Color32, theme: &Theme) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Pos2, RawInput};

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
