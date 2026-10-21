use crate::theme::Theme;
use eframe::egui::{self, Color32, CornerRadius, Margin, Pos2, Sense, Stroke, Vec2};

/// Rich structured information explaining an audiophile or player setting
#[derive(Clone, Copy, Debug)]
pub struct InfoBoxContent {
    pub title: &'static str,
    pub subtitle: &'static str,
    pub description: &'static str,
    pub technical_note: &'static str,
    pub recommendation: &'static str,
}

/// Unique persistent ID used in Context data storage for the currently open info box
const ACTIVE_INFO_BOX_KEY: &str = "shadowplayer_active_info_box_id";

/// Close the open info popup, if there is one.
///
/// Returns `true` when a popup was open and has now been closed. The app's
/// global Escape handler uses this to give the popup — the topmost transient
/// surface — the press before the layers behind it, so one Escape never
/// closes two things at once.
pub fn dismiss_open_info_box(ctx: &egui::Context) -> bool {
    let key_id = egui::Id::NULL.with(ACTIVE_INFO_BOX_KEY);
    if ctx.data(|d| d.get_temp::<u64>(key_id)).is_none() {
        return false;
    }
    ctx.data_mut(|d| {
        d.remove_temp::<u64>(key_id);
    });
    ctx.request_repaint();
    true
}

/// Renders a clickable circular 'i' icon that opens a floating, glassmorphic info box
pub fn render_info_icon_button(
    ui: &mut egui::Ui,
    id_source: impl std::hash::Hash + std::fmt::Debug,
    content: &InfoBoxContent,
    theme: &Theme,
) -> egui::Response {
    let popup_id = ui.make_persistent_id(id_source);
    let key_id = egui::Id::NULL.with(ACTIVE_INFO_BOX_KEY);

    let current_open: Option<u64> = ui.ctx().data(|d| d.get_temp(key_id));
    let eval_match = if let Ok(target) = std::env::var("SHADOWPLAYER_EVAL_INFO_NAME") {
        content
            .title
            .to_lowercase()
            .contains(&target.to_lowercase())
    } else {
        false
    };
    let is_open = current_open == Some(popup_id.value()) || eval_match;

    // Allocate 16x16 interactive area
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(16.0, 16.0), Sense::click());
    let is_hovered = resp.hovered();

    // Visual rendering of 'i' icon
    let p = ui.painter_at(rect);
    let center = rect.center();
    let radius = 7.0;

    let (bg_col, stroke_col, fg_col) = if is_open {
        (
            theme.accent.linear_multiply(0.3),
            theme.accent_warm,
            theme.accent_warm,
        )
    } else if is_hovered {
        (theme.bg_card_hover, theme.accent, theme.text_primary)
    } else {
        (Color32::TRANSPARENT, theme.border_subtle, theme.text_muted)
    };

    if bg_col != Color32::TRANSPARENT {
        p.circle_filled(center, radius + 1.0, bg_col);
    }
    p.circle_stroke(center, radius, Stroke::new(1.0, stroke_col));

    // Dot of the 'i'
    p.circle_filled(Pos2::new(center.x, center.y - 3.0), 1.0, fg_col);
    // Vertical stem of the 'i'
    p.line_segment(
        [
            Pos2::new(center.x, center.y - 1.0),
            Pos2::new(center.x, center.y + 3.2),
        ],
        Stroke::new(1.2, fg_col),
    );

    // On hover, show a prompt tooltip.
    //
    // `on_hover_text` and not `on_hover_ui`: egui only turns the former into
    // the control's accessible name, so the button used to be an unnamed node
    // in the accessibility tree.
    resp.clone()
        .on_hover_text(format!("Details on {}", content.title));
    resp.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            true,
            format!("Details on {}", content.title),
        )
    });

    // Toggle logic on click
    if resp.clicked() {
        ui.ctx().data_mut(|d| {
            if is_open {
                d.remove_temp::<u64>(key_id);
            } else {
                d.insert_temp(key_id, popup_id.value());
            }
        });
        ui.ctx().request_repaint();
    }

    // Dismissal with Escape is handled centrally, in the app's global
    // shortcut handler, so that exactly one owner acts on the key press. See
    // [`dismiss_open_info_box`].

    // If active, render the floating info card in the Foreground layer
    if is_open {
        let screen_rect =
            ui.ctx()
                .input(|i| i.raw.screen_rect)
                .unwrap_or(eframe::egui::Rect::from_min_size(
                    Pos2::ZERO,
                    Vec2::new(1280.0, 830.0),
                ));
        let box_width = 330.0_f32.min(screen_rect.width() - 40.0);

        // Position: prefer below-left of icon, clamped safely inside viewport
        let mut target_x = rect.left() - 10.0;
        if target_x + box_width > screen_rect.right() - 20.0 {
            target_x = screen_rect.right() - box_width - 20.0;
        }
        if target_x < 20.0 {
            target_x = 20.0;
        }

        let mut target_y = rect.bottom() + 6.0;
        if target_y > screen_rect.bottom() - 260.0 {
            // Flip above the icon if near the bottom of viewport
            target_y = (rect.top() - 240.0).max(20.0);
        }

        let anchor = Pos2::new(target_x, target_y);
        let mut close_requested = false;

        egui::Area::new(popup_id.with("info_popover"))
            .order(egui::Order::Foreground)
            .fixed_pos(anchor)
            .show(ui.ctx(), |ui| {
                let frame = egui::Frame::NONE
                    .fill(theme.bg_card)
                    .stroke(Stroke::new(1.0, theme.accent.linear_multiply(0.6)))
                    .corner_radius(CornerRadius::same(theme.radius_lg))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 8],
                        blur: 20,
                        spread: 0,
                        color: theme.shadow,
                    })
                    .inner_margin(Margin::symmetric(
                        theme.space_2xl as i8,
                        theme.space_xl as i8,
                    ));

                let frame_resp = frame.show(ui, |ui| {
                    ui.set_width(box_width);
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_sm);

                    // 1. Header: Icon + Title + Subtitle + Close Button (✕)
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);

                        // Mini badge icon
                        let (badge_r, _) =
                            ui.allocate_exact_size(Vec2::new(16.0, 16.0), Sense::hover());
                        let bp = ui.painter_at(badge_r);
                        bp.circle_filled(
                            badge_r.center(),
                            7.0,
                            theme.accent_warm.linear_multiply(0.2),
                        );
                        bp.circle_stroke(
                            badge_r.center(),
                            7.0,
                            Stroke::new(1.0, theme.accent_warm),
                        );
                        bp.circle_filled(
                            Pos2::new(badge_r.center().x, badge_r.center().y - 3.0),
                            0.9,
                            theme.accent_warm,
                        );
                        bp.line_segment(
                            [
                                Pos2::new(badge_r.center().x, badge_r.center().y - 1.0),
                                Pos2::new(badge_r.center().x, badge_r.center().y + 3.0),
                            ],
                            Stroke::new(1.1, theme.accent_warm),
                        );

                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xxs);
                            ui.label(
                                egui::RichText::new(content.title)
                                    .font(crate::theme::font(13.0))
                                    .strong()
                                    .color(theme.text_primary),
                            );
                            if !content.subtitle.is_empty() {
                                ui.label(
                                    egui::RichText::new(content.subtitle)
                                        .font(crate::theme::font(10.5))
                                        .color(theme.accent_warm),
                                );
                            }
                        });

                        // Close button on right
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (c_rect, c_resp) =
                                ui.allocate_exact_size(Vec2::new(20.0, 20.0), Sense::click());
                            let cp = ui.painter_at(c_rect);
                            let bg = if c_resp.hovered() {
                                theme.destructive.gamma_multiply(0.2)
                            } else {
                                theme.bg_pill
                            };
                            cp.rect_filled(c_rect, CornerRadius::same(theme.radius_sm), bg);
                            let cross_col = if c_resp.hovered() {
                                theme.destructive
                            } else {
                                theme.text_muted
                            };
                            crate::components::icons::draw_window_close(
                                ui,
                                c_rect.shrink2(Vec2::splat(5.0)),
                                cross_col,
                            );
                            if c_resp.on_hover_text("Close info box").clicked() {
                                close_requested = true;
                            }
                        });
                    });

                    // Divider
                    let sep_y = ui.cursor().top() + 2.0;
                    ui.painter().line_segment(
                        [
                            Pos2::new(ui.min_rect().left() + 2.0, sep_y),
                            Pos2::new(ui.min_rect().right() - 2.0, sep_y),
                        ],
                        Stroke::new(1.0, theme.border_subtle),
                    );
                    ui.add_space(5.0);

                    // 2. Main Explanation Description
                    ui.label(
                        egui::RichText::new(content.description)
                            .font(crate::theme::font(11.5))
                            .color(theme.text_secondary),
                    );

                    // 3. Technical Detail Box
                    if !content.technical_note.is_empty() {
                        egui::Frame::NONE
                            .fill(theme.bg_card_hover)
                            .stroke(Stroke::new(1.0, theme.border_subtle))
                            .corner_radius(CornerRadius::same(theme.radius_md))
                            .inner_margin(Margin::symmetric(
                                theme.space_lg as i8,
                                theme.space_sm as i8,
                            ))
                            .show(ui, |ui| {
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xxs);
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("ENGINE & DSP DYNAMICS")
                                                .font(crate::theme::font(9.5))
                                                .strong()
                                                .color(theme.accent),
                                        );
                                    });
                                    ui.label(
                                        egui::RichText::new(content.technical_note)
                                            .font(crate::theme::font(10.5))
                                            .color(theme.text_muted),
                                    );
                                });
                            });
                    }

                    // 4. Audiophile Pro-Tip / Recommendation Pill
                    if !content.recommendation.is_empty() {
                        egui::Frame::NONE
                            .fill(theme.accent.linear_multiply(0.12))
                            .stroke(Stroke::new(1.0, theme.accent.linear_multiply(0.3)))
                            .corner_radius(CornerRadius::same(theme.radius_md))
                            .inner_margin(Margin::symmetric(
                                theme.space_lg as i8,
                                theme.space_sm as i8,
                            ))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_xs, 0.0);
                                    ui.label(
                                        egui::RichText::new("RECOMMENDED:")
                                            .font(crate::theme::font(9.5))
                                            .strong()
                                            .color(theme.accent_warm),
                                    );
                                    ui.label(
                                        egui::RichText::new(content.recommendation)
                                            .font(crate::theme::font(10.5))
                                            .color(theme.text_primary),
                                    );
                                });
                            });
                    }
                });

                // Detect clicking outside both the info box and the trigger button to dismiss
                if ui.input(|i| i.pointer.any_pressed()) {
                    if let Some(pos) = ui.input(|i| i.pointer.latest_pos()) {
                        if !frame_resp.response.rect.contains(pos) && !rect.contains(pos) {
                            close_requested = true;
                        }
                    }
                }
            });

        if close_requested {
            ui.ctx().data_mut(|d| {
                d.remove_temp::<u64>(key_id);
            });
            ui.ctx().request_repaint();
        }
    }

    resp
}

// ─────────────────────────────────────────────────────────────────────────────
// Predefined Curated Audiophile Info Content Catalog
// ─────────────────────────────────────────────────────────────────────────────

pub const INFO_GRAPHIC_EQ_BANDS: InfoBoxContent = InfoBoxContent {
    title: "Graphic EQ Band Topology",
    subtitle: "Constant-Q Minimum-Phase Filter Bank",
    description: "Splits audible audio into precision frequency bands across the 20 Hz to 20 kHz spectrum for surgical acoustic tailoring.",
    technical_note: "10 bands = 1-octave ISO spacing (standard consumer); 15 bands = 2/3-octave; 31 bands = 1/3-octave studio grade; 64 bands = 1/6-octave surgical acoustic tuning. Filters use 2nd-order biquad peaking designs.",
    recommendation: "Use 10 or 15 bands for music tonal shaping; 31 or 64 bands for room mode notch cuts.",
};

pub const INFO_MASTER_PREAMP: InfoBoxContent = InfoBoxContent {
    title: "Master Digital Preamp",
    subtitle: "Input/Output Headroom Gain Stage",
    description: "Applies digital gain before or after equalizer filtering to optimize dynamic range and level matching.",
    technical_note: "Scaling digital gain in 32-bit floating point prevents inter-sample peaks and DAC overs when large positive filter gains are introduced without Auto Headroom.",
    recommendation: "Keep at 0.0 dB if Auto Headroom is active. Lower manually by the maximum boost if Auto Headroom is bypassed.",
};

pub const INFO_EQ_PRESETS: InfoBoxContent = InfoBoxContent {
    title: "Equalizer Target Curves",
    subtitle: "Genre & Acoustic Tuning Profiles",
    description: "Instant acoustic curve presets tailored for different musical genres, headphones, and listening environments.",
    technical_note: "Preset gains are mathematically interpolated across whatever band count is active (10, 15, 31, or 64 bands) using logarithmic frequency spline calculations.",
    recommendation: "Start with Flat for neutral studio mastering. Try Pop or Rock for lively bass/treble presence.",
};

pub const INFO_BASS_SHELF: InfoBoxContent = InfoBoxContent {
    title: "Bass Shelf Filter",
    subtitle: "Low-Frequency Acoustic Warmth",
    description: "Smooth shelving filter boosting or attenuating low-end frequencies below the 120 Hz transition frequency.",
    technical_note: "Applies uniform boost/cut across sub-bass (20–60 Hz) and mid-bass (60–120 Hz) without resonant center peaking, avoiding boomy mud.",
    recommendation: "+1.5 to +3.0 dB for headphones needing low-end punch; -2.0 dB to tame boomy room resonances.",
};

pub const INFO_TREBLE_SHELF: InfoBoxContent = InfoBoxContent {
    title: "Treble Shelf Filter",
    subtitle: "High-Frequency Air & Detail",
    description: "Smooth shelving filter adjusting brightness, acoustic presence, and air above 7.5 kHz.",
    technical_note: "Operates above the sensitive ear canal resonance region (2–5 kHz), avoiding sibilance while shaping acoustic brilliance and cymbal shimmer.",
    recommendation: "+1.0 to +2.0 dB adds crisp modern detail; -1.5 dB smooths fatiguing or overly bright recordings.",
};

pub const INFO_STEREO_WIDTH: InfoBoxContent = InfoBoxContent {
    title: "Acoustic Stereo Width",
    subtitle: "Mid/Side Soundstage Expander",
    description: "Acoustically widens or narrows the perceived stereo soundstage without altering mono frequency response.",
    technical_note: "Decomposes Left/Right audio into Mid (L+R sum, center channel) and Side (L-R difference, stereo ambient field), scaling the Side energy.",
    recommendation: "100% preserves original mix balance; 115%–135% adds expansive immersion on headphones.",
};

pub const INFO_BALANCE: InfoBoxContent = InfoBoxContent {
    title: "Stereo Channel Balance",
    subtitle: "Left / Right Acoustic Panning",
    description: "Calibrates the sound pressure level between Left and Right stereo channels.",
    technical_note: "Implements a -3.0 dB sinusoidal constant-power panning law to ensure overall perceived loudness remains identical during pan adjustments.",
    recommendation: "Keep at Center (0.00). Use to correct asymmetrical speaker placement or unequal ear sensitivity.",
};

pub const INFO_EXTENDED_RANGE: InfoBoxContent = InfoBoxContent {
    title: "Extended Gain Range",
    subtitle: "±12 dB vs ±24 dB Maximum Scale",
    description: "Unlocks maximum ±24.0 dB slider limits across all Graphic EQ bands, tone shelves, and preamp.",
    technical_note: "Standard ±12.0 dB provides fine-grained 0.1 dB resolution for audiophile mastering; ±24.0 dB enables extreme surgical acoustic notch cuts.",
    recommendation: "Keep OFF (±12 dB Standard) for music listening to prevent sudden volume jumps.",
};

pub const INFO_AUTO_HEADROOM: InfoBoxContent = InfoBoxContent {
    title: "Auto Headroom Management",
    subtitle: "Digital Anti-Clipping Protection",
    description: "Automatically and dynamically lowers master digital gain when positive EQ boosts exceed 0 dBFS.",
    technical_note: "Calculates the maximum boost across all active bands and applies an equivalent negative digital offset. Completely transparent (0.0 dB attenuation) when EQ is flat.",
    recommendation: "Keep ON to guarantee zero inter-sample peak distortion and prevent harsh DAC clipping.",
};

pub const INFO_MIDSIDE_EQ: InfoBoxContent = InfoBoxContent {
    title: "Mid/Side Matrix Processing",
    subtitle: "M/S Channel Decomposition",
    description: "Splits stereo audio into Mid (center vocal/bass) and Side (wide reverb/ambiance) before DSP filtering.",
    technical_note: "Encodes L/R into Mid/Side components before EQ biquads and decodes back to L/R after filtering. Allows independent acoustic treatment of center vs sides.",
    recommendation: "Keep in Stereo L/R Mode for standard music playback. Use M/S for specialized remastering.",
};

pub const INFO_DITHER: InfoBoxContent = InfoBoxContent {
    title: "TPDF Triangular Dithering",
    subtitle: "Quantization Noise Shaping",
    description: "Applies high-resolution Triangular Probability Density Function dither noise to prevent low-level quantization distortion.",
    technical_note: "Eliminates harmonic distortion when truncating 32-bit float audio to 16-bit or 24-bit fixed point. Shibata noise shaping selects coefficients for the stream's sample rate, using the published 44.1/48 kHz designs; other rates reuse the nearer table and log one warning.",
    recommendation: "Keep OFF for 24-bit/32-bit DACs for a zero-noise-floor bit-accurate signal. Enable for 16-bit soundcards.",
};

pub const INFO_AUTOEQ: InfoBoxContent = InfoBoxContent {
    title: "AutoEQ Headphone Target Calibration",
    subtitle: "Measurement-Based Frequency Equalization",
    description: "Compensates for headphone acoustic deviations using calibrated lab measurements from over 4,000 headphones and IEMs.",
    technical_note: "Applies multi-band biquad parametric corrections from Oratory1990, RTINGS, and Crinacle to match the Harman 2020 Target Curve.",
    recommendation: "Search for your exact headphone model in the AutoEQ dialog for flat studio-reference sound.",
};

pub const INFO_SPEED_PITCH: InfoBoxContent = InfoBoxContent {
    title: "Speed, Pitch & Varispeed",
    subtitle: "Audiophile Tempo Engine",
    description: "Adjusts playback speed and musical pitch using professional time-stretching or analog-style varispeed.",
    technical_note: "Varispeed links speed and pitch simultaneously (like analog magnetic tape or vinyl speed changes), preserving zero DSP phase artifacts. TimeStretch decouples tempo.",
    recommendation: "Use Varispeed for 100% artifact-free analog tape listening; 1.0x with 0.0 semitones for standard playback.",
};

pub const INFO_TRANSITION_CROSSFADE: InfoBoxContent = InfoBoxContent {
    title: "Gapless & Transition Engine",
    subtitle: "Inter-Track Playback Transitions",
    description: "Determines how consecutive tracks in your playlist transition from one track into the next.",
    technical_note: "Gapless guarantees 0 sample delay or clicks between album tracks (live albums, concept albums). Constant Power crossfade preserves acoustic perceived loudness.",
    recommendation: "Gapless (Audiophile) is recommended for album listening. Constant Power crossfade for party mixes.",
};

pub const INFO_LOUDNESS_NORMALIZATION: InfoBoxContent = InfoBoxContent {
    title: "Loudness Normalization (EBU R128)",
    subtitle: "Automated LUFS Volume Matching",
    description: "Eliminates sudden volume spikes between quiet acoustic recordings and loud modern masters.",
    technical_note: "Measures loudness according to ITU-R BS.1770 / EBU R128 (-14 LUFS / -18 LUFS) or ReplayGain 2.0 tags. Applies a clean static digital gain offset without dynamic range compression.",
    recommendation: "Track ReplayGain for shuffled playlists; Album ReplayGain to preserve artist-intended dynamic album flow.",
};

pub const INFO_AUDIO_BACKEND: InfoBoxContent = InfoBoxContent {
    title: "Audio Output Backend API",
    subtitle: "Operating System Driver Interface",
    description: "The low-level operating system audio engine driver used to stream digital samples to your audio device.",
    technical_note: "Auto selects the lowest-latency driver; ALSA accesses Linux audio hardware directly; PulseAudio/PipeWire integrates with system audio; WASAPI Exclusive/ASIO bypasses Windows mixing.",
    recommendation: "Auto is recommended for universal compatibility. ALSA or WASAPI Exclusive for direct DAC bit-perfect streaming.",
};

pub const INFO_SAMPLE_RATE_POLICY: InfoBoxContent = InfoBoxContent {
    title: "Sample Rate Clock Policy",
    subtitle: "DAC Clock Synchronizer",
    description: "Configures how track sample rates (44.1, 48, 88.2, 96, 192 kHz) are fed to your digital-to-analog converter.",
    technical_note: "Follow Track dynamically switches hardware DAC clock to match track rate without software resampling. Fixed Rate resamples all streams to a single constant clock rate.",
    recommendation: "Follow Track for external audiophile DACs for bit-perfect native playback.",
};

pub const INFO_VOLUME_MODE: InfoBoxContent = InfoBoxContent {
    title: "Volume Attenuation Architecture",
    subtitle: "Software vs Hardware Volume Control",
    description: "Controls where audio volume is attenuated in the signal chain.",
    technical_note: "Software Only attenuates in 32-bit floating point; Hardware Mixer adjusts the analog/digital attenuation registers inside the DAC chip; Bit-Perfect Direct locks volume at 0 dBFS.",
    recommendation: "Bit-Perfect (Direct 100%) when volume is controlled via external analog amplifier or headphone amp.",
};

pub const INFO_BIT_PERFECT: InfoBoxContent = InfoBoxContent {
    title: "Bit-Perfect Pure Direct Mode",
    subtitle: "100% Lossless DAC Stream Path",
    description: "Bypasses all software DSP processing (equalizer, volume scaling, spatializers, limiter, dithering).",
    technical_note: "Feeds unadulterated PCM samples directly from audio decoder into the output device ringbuffer with bit-for-bit mathematical purity.",
    recommendation: "Enable for reference listening when you have an external headphone amp or DAC volume knob.",
};

pub const INFO_RESAMPLER_QUALITY: InfoBoxContent = InfoBoxContent {
    title: "Polyphase Sinc Resampler",
    subtitle: "Sample Rate Conversion Quality",
    description: "The interpolation algorithm used whenever audio must be converted between different sample rates.",
    technical_note: "High Quality uses a 32-tap Kaiser-windowed sinc with >120 dB stopband attenuation. Ultra Quality uses 64-tap windowed sinc with >144 dB stopband attenuation and linear phase.",
    recommendation: "High Quality for efficient low-latency playback; Ultra Quality for reference studio mastering accuracy.",
};

pub const INFO_SPATIAL_HRTF: InfoBoxContent = InfoBoxContent {
    title: "Binaural HRTF Spatialization",
    subtitle: "3D Acoustic Soundstage Simulation",
    description: "Recreates natural human pinna, head, and torso acoustic diffraction for expansive 3D headphone listening.",
    technical_note: "Convolves stereo audio through Head-Related Transfer Function impulse responses, placing virtual speakers in front of you and eliminating headphone 'in-the-head' fatigue.",
    recommendation: "Enable on headphones for a natural speaker-like studio listening experience.",
};

pub const INFO_ROOM_ACOUSTICS: InfoBoxContent = InfoBoxContent {
    title: "Virtual Room Acoustics Simulation",
    subtitle: "Physical Space RT60 Decay Engine",
    description: "Simulates early acoustic reflections and diffuse late reverberation of a physical acoustic mastering room.",
    technical_note: "RT60 controls reverberation decay time (time to decay by 60 dB); Absorption determines wall acoustic damping; Room Dimensions set modal echo density.",
    recommendation: "Subtle RT60 of 300–400 ms with 10–15% Wet Mix creates an authentic mastering studio listening environment.",
};

pub const INFO_LISTENER_TRACKING: InfoBoxContent = InfoBoxContent {
    title: "Spatial Listener Head Tracking",
    subtitle: "Smooth Dynamic Acoustic Kinematics",
    description: "Smoothes rotational coordinates when using head-tracking sensors or virtual camera movement.",
    technical_note: "Applies 1st-order low-pass smoothing and an angular velocity slew limiter to prevent phase clicks and acoustic tearing during sudden head movements.",
    recommendation: "50 ms smoothing provides natural head movement without perceptible lag.",
};

pub const INFO_DUCKING: InfoBoxContent = InfoBoxContent {
    title: "Sidechain Audio Ducking",
    subtitle: "Automated Level Attenuation",
    description: "Automatically attenuates background music whenever secondary audio (voice commentary or microphone) is detected.",
    technical_note: "Fast lookahead peak detector triggers a smooth exponential gain reduction curve (Threshold & Depth) with configurable Attack and Release times.",
    recommendation: "Useful for podcasting, streaming, or background music mixing.",
};

pub const INFO_STREAM_CAPTURE: InfoBoxContent = InfoBoxContent {
    title: "Direct Lossless Stream Capture",
    subtitle: "Bit-Accurate WAV Output Recorder",
    description: "Captures and writes the live post-engine audio stream directly to an uncompressed 32-bit float WAV file on disk.",
    technical_note: "Streams PCM frames through a ringbuffer directly to a non-blocking background disk writer with zero audio thread latency.",
    recommendation: "Ideal for recording processed audio, EQ sessions, or live spatial mixes.",
};
