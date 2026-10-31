//! The Hardware & DAC category: backend, device, bit-perfect mode, rate
//! policy, and resampling precision.

use super::super::widgets::{
    card_header, pill_btn, segment_picker, setting_card, setting_item, setting_item_toggle,
    setting_item_with_info, toggle_switch,
};
use super::super::{SettingsExtraContext, SettingsTabActions, SettingsTabState};
use crate::theme::Theme;
use config::{
    DsdNoiseFilter, DsdOutput, FallbackPolicy, NoiseShaper, PrecisionMode, ResamplerFilter,
    ResamplerQuality, SampleRatePolicy, SdmModulator, SdmRate, UltrasonicFilter, VolumeMode,
};
use eframe::egui::{self, Color32, CornerRadius, Sense, Stroke, Vec2};

/// The concrete output rates offered in the allowed-rates whitelist. Matching
/// the engine's policy: the 44.1 kHz family and the 48 kHz family.
const ALLOWED_SAMPLE_RATE_CHOICES: [u32; 10] = [
    44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 352_800, 384_000, 705_600, 768_000,
];

/// The output-device picker.
///
/// The devices come from the *selected* backend, so the device chosen here is
/// the device the engine will open. Typing a device name was the only way to
/// choose one before, which is unusable on Windows: the exclusive WASAPI
/// backend selects an endpoint by its id, not by the name the user can see.
///
/// A free-text field is still offered, because ALSA accepts device strings
/// that no enumeration can guess (`hw:0,1`).
fn render_device_picker(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    extra: &SettingsExtraContext<'_>,
    theme: &Theme,
    actions: &mut SettingsTabActions,
) {
    let devices = extra.output_devices;

    // The label of the current selection, so the combo box shows what is
    // actually in use rather than a raw id.
    let current_label = if state.output_device.is_empty() {
        "System default".to_string()
    } else {
        devices
            .iter()
            .find(|d| d.id == state.output_device)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| state.output_device.clone())
    };

    let mut selected = state.output_device.clone();
    let mut picked = false;
    egui::ComboBox::from_id_salt("output_device_picker")
        .selected_text(current_label)
        .width(280.0)
        .show_ui(ui, |ui| {
            if ui
                .selectable_value(&mut selected, String::new(), "System default")
                .changed()
            {
                picked = true;
            }
            for device in devices {
                let mut label = device.name.clone();
                if device.is_default {
                    label.push_str("  (default)");
                }
                // The id is the stable identifier the backend selects by; the
                // name is only what the user reads.
                if ui
                    .selectable_value(&mut selected, device.id.clone(), label)
                    .changed()
                {
                    picked = true;
                }
            }
        });

    if picked && selected != state.output_device {
        state.output_device = selected.clone();
        actions.device_changed = Some(selected);
    }

    if devices.is_empty() {
        ui.label(
            egui::RichText::new(
                tr!("This backend reported no devices. It may need exclusive access, or the device may be in use by another application."),
            )
            .font(crate::theme::font(11.0))
            .color(theme.text_dim),
        );
    }

    // The advanced field, for a device string the enumeration cannot know.
    let mut custom = state.output_device.clone();
    let edit = egui::TextEdit::singleline(&mut custom)
        .hint_text(tr!(Msg::SA0C32392220AE499))
        .desired_width(280.0)
        .font(crate::theme::font(12.0));
    if ui.add(edit).changed() && custom != state.output_device {
        state.output_device = custom.clone();
        actions.device_changed = Some(custom);
    }
}

pub(in crate::components::settings_tab) fn render_output_category(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
    extra: &SettingsExtraContext<'_>,
    actions: &mut SettingsTabActions,
) {
    // Card 1: Bit-Perfect Audiophile Mode Banner Card
    let bp_border = if state.bit_perfect {
        theme.bit_perfect_green
    } else {
        theme.border_card
    };
    let bp_fill = if state.bit_perfect {
        theme.bit_perfect_green.gamma_multiply(0.12)
    } else {
        theme.bg_card
    };

    let bp_frame = egui::Frame::NONE
        .fill(bp_fill)
        .stroke(Stroke::new(1.2, bp_border))
        .corner_radius(CornerRadius::same(theme.radius_xl))
        .inner_margin(egui::Margin::symmetric(
            theme.space_4xl as i8,
            theme.space_3xl as i8,
        ));

    bp_frame.show(ui, |ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_lg);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);
            let (icon_rect, _) = ui.allocate_exact_size(Vec2::new(32.0, 32.0), Sense::hover());
            ui.painter_at(icon_rect).rect_filled(
                icon_rect,
                CornerRadius::same(theme.radius_md),
                if state.bit_perfect { theme.bit_perfect_green } else { theme.bg_pill },
            );
            crate::components::icons::draw_speaker(
                ui,
                icon_rect.shrink2(Vec2::splat(6.0)),
                if state.bit_perfect { Color32::WHITE } else { theme.accent_highlight },
                false,
            );

            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
                    ui.label(
                        egui::RichText::new(tr!("Audiophile Bit-Perfect Direct Stream"))
                            .font(crate::theme::font(15.0))
                            .strong()
                            .color(if state.bit_perfect { theme.bit_perfect_green } else { theme.text_primary }),
                    );
                    crate::components::info_box::render_info_icon_button(
                        ui,
                        "info_bit_perfect_banner",
                        &crate::components::info_box::INFO_BIT_PERFECT,
                        theme,
                    );
                });
                ui.label(
                    egui::RichText::new(
                        if state.bit_perfect {
                            "ACTIVE: Bypassing all OS audio mixers, EQ, resampling, and digital attenuation. Audio stream delivered byte-for-byte to your DAC."
                        } else {
                            "Enable to guarantee 100% bit-exact transmission to external DACs (disables DSP, EQ, and software volume)."
                        }
                    )
                    .font(crate::theme::font(11.5))
                    .color(theme.text_muted),
                );
            });

            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), 32.0),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    let mut bp = state.bit_perfect;
                    if toggle_switch(ui, &mut bp, theme) {
                        state.bit_perfect = bp;
                        actions.bit_perfect_changed = Some(bp);
                    }
                },
            );
        });
    });

    // Card 2: Driver Backend & Device
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            |ui, r, c| crate::components::icons::draw_speaker(ui, r, c, false),
            "Audio Driver Backend & Hardware Device",
            "Select hardware driver architecture and specific digital-to-analog converter",
            None,
            theme,
        );

        setting_item_with_info(
            ui,
            &tr!("Driver Architecture"),
            &tr!("Exclusive ALSA, WASAPI Exclusive, or cross-platform automatic detection"),
            Some(&crate::components::info_box::INFO_AUDIO_BACKEND),
            theme,
            |ui| {
                crate::components::backend_options::backend_picker(
                    ui,
                    &mut state.output_backend,
                    theme,
                    &mut actions.backend_changed,
                );
            },
        );

        setting_item(
            ui,
            &tr!("Processing Profile"),
            &tr!("Engine preset: resampler quality, dither, EQ, crossfeed and buffer sizes move together. Consumer keeps the lightest chain, Fidelity the strictest, Low Power the cheapest"),
            theme,
            |ui| {
                let options = [
                    (domain::models::AudioProfileChoice::Normal, "Consumer"),
                    (domain::models::AudioProfileChoice::Fidelity, "Fidelity"),
                    (domain::models::AudioProfileChoice::LegacyLowPower, "Low Power"),
                ];
                if segment_picker(ui, &mut state.audio_profile, &options, theme) {
                    actions.audio_profile_changed = Some(state.audio_profile);
                }
            },
        );

        setting_item(
            ui,
            &tr!("Output Endpoint"),
            &tr!("The device the engine opens. Leave it on the system default unless you are routing to a specific DAC."),
            theme,
            |ui| {
                render_device_picker(ui, state, extra, theme, actions);
            },
        );
        setting_item(
            ui,
            &tr!("DAC Warmup on Start"),
            &tr!("Sends a short burst of zero frames (0–500 ms) upon stream start to allow external DAC clock PLLs to stabilize, eliminating startup pops and clicks."),
            theme,
            |ui| {
                let mut ms = state.dac_warmup_ms;
                if ui
                    .add(egui::Slider::new(&mut ms, 0..=500).suffix(" ms").step_by(10.0))
                    .changed()
                {
                    state.dac_warmup_ms = ms;
                    actions.dac_warmup_ms_changed = Some(ms);
                }
            },
        );
        setting_item(
            ui,
            &tr!("Fallback Policy"),
            &tr!("Action to take if exclusive mode device acquisition is rejected by the OS"),
            theme,
            |ui| {
                let options = [
                    (FallbackPolicy::Allow, "Allow Shared Fallback"),
                    (FallbackPolicy::Strict, "Strict (Halt if Exclusive Fails)"),
                ];
                if segment_picker(ui, &mut state.fallback_policy, &options, theme) {
                    actions.fallback_policy_changed = Some(state.fallback_policy);
                }
            },
        );

        setting_item(
            ui,
            &tr!("Hardware Actions"),
            &tr!("Open external driver control panels and reset device configuration profiles"),
            theme,
            |ui| {
                if ui
                    .add(pill_btn(tr!(Msg::SE39B424FF4863C4D), false, theme))
                    .clicked()
                {
                    actions.open_asio_panel = true;
                }
                if ui
                    .add(pill_btn(tr!(Msg::S575E2028F7A67363), false, theme))
                    .clicked()
                {
                    actions.clear_output_profile = true;
                }
            },
        );
    });

    // Card 3: Sample Rate Policy & Resampling
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_equalizer_sliders,
            &tr!("Sample Rate Policies & Resampling Precision"),
            &tr!("Lock output sample rate or dynamically match input track sample rate"),
            None,
            theme,
        );

        setting_item_with_info(
            ui,
            &tr!("Sample Rate Target"),
            &tr!("Follow Track switches your DAC rate on song change; Fixed resamples everything"),
            Some(&crate::components::info_box::INFO_SAMPLE_RATE_POLICY),
            theme,
            |ui| {
                let options = [
                    (SampleRatePolicy::FollowTrack, "Follow Track (Bit-Exact)"),
                    (SampleRatePolicy::DeviceDefault, "Device Native"),
                    (SampleRatePolicy::Fixed(44100), "44.1 kHz (CD)"),
                    (SampleRatePolicy::Fixed(48000), "48 kHz"),
                    (SampleRatePolicy::Fixed(96000), "96 kHz (Hi-Res)"),
                    (SampleRatePolicy::Fixed(192000), "192 kHz"),
                ];
                if segment_picker(ui, &mut state.sample_rate_policy, &options, theme) {
                    actions.sample_rate_policy_changed = Some(state.sample_rate_policy.clone());
                }
            },
        );

        setting_item_with_info(
            ui,
            &tr!("Allowed Sample Rates"),
            &tr!(
                "Restricts which DAC rates the engine may negotiate. Disabled chips leave every hardware-supported rate available"
            ),
            None,
            theme,
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = theme.space_sm;
                    for rate in ALLOWED_SAMPLE_RATE_CHOICES {
                        let mut on = state.allowed_sample_rates.contains(&rate);
                        let label = if rate % 1000 == 0 {
                            format!("{} kHz", rate / 1000)
                        } else {
                            format!("{:.1} kHz", rate as f32 / 1000.0)
                        };
                        if ui.toggle_value(&mut on, label).changed() {
                            if on {
                                state.allowed_sample_rates.push(rate);
                                state.allowed_sample_rates.sort_unstable();
                                state.allowed_sample_rates.dedup();
                            } else {
                                state
                                    .allowed_sample_rates
                                    .retain(|&r| r != rate);
                            }
                            actions.allowed_sample_rates_changed =
                                Some(state.allowed_sample_rates.clone());
                        }
                    }
                });
            },
        );

        setting_item_with_info(
            ui,
            &tr!("Resampler Engine"),
            &tr!("Polyphase FIR filter quality used when rate conversion is required"),
            Some(&crate::components::info_box::INFO_RESAMPLER_QUALITY),
            theme,
            |ui| {
                let options = [
                    (ResamplerQuality::Fast, "Fast (Low CPU)"),
                    (ResamplerQuality::Balanced, "Balanced"),
                    (ResamplerQuality::HighQuality, "High Quality (Linear Phase)"),
                    (ResamplerQuality::Ultra, "Ultra (Audiophile Sinc)"),
                ];
                if segment_picker(ui, &mut state.resampler_quality, &options, theme) {
                    actions.resampler_quality_changed = Some(state.resampler_quality);
                }
            },
        );

        setting_item(
            ui,
            &tr!("DSP Precision Mode"),
            &tr!("Internal processing precision. Quality routes the mix chain through the f64 core for extra headroom at a small CPU cost."),
            theme,
            |ui| {
                let mut mode = state.precision_mode.unwrap_or_default();
                let options = [
                    (PrecisionMode::Performance, "Performance (f32)"),
                    (PrecisionMode::Quality, "Quality (f64 core)"),
                ];
                if segment_picker(ui, &mut mode, &options, theme) {
                    state.precision_mode = Some(mode);
                    actions.precision_mode_changed = Some(mode);
                }
            },
        );

        setting_item_with_info(
            ui,
            &tr!("Volume Attenuation Mode"),
            &tr!(
                "Software Controlled vs Hardware Synchronized (DVC Mode): keeps digital DSP and EQ at full 0 dBFS float scale with maximum dynamic range, delegating attenuation to the DAC hardware"
            ),
            Some(&crate::components::info_box::INFO_VOLUME_MODE),
            theme,
            |ui| {
                let options = [
                    (VolumeMode::SoftwareOnly, "Software Controlled"),
                    (VolumeMode::HardwarePreferred, "Hardware Synchronized (DVC Mode)"),
                ];
                if segment_picker(ui, &mut state.volume_mode, &options, theme) {
                    actions.volume_mode_changed = Some(state.volume_mode);
                }
            },
        );

        setting_item_with_info(
            ui,
            &tr!("TPDF Dither"),
            &tr!("Triangular PDF dithering eliminates quantization distortion when converting bit depth"),
            Some(&crate::components::info_box::INFO_DITHER),
            theme,
            |ui| {
                let mut d = state.dither_enabled;
                if toggle_switch(ui, &mut d, theme) {
                    state.dither_enabled = d;
                    actions.dither_changed = Some(d);
                }
            },
        );
    });

    // Card 4: DSD transport and decoding filters.
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_disc,
            &tr!("DSD Transport & Decimation"),
            &tr!("How DSD sources are carried to the DAC, and how they are filtered when converted to PCM"),
            None,
            theme,
        );

        setting_item(
            ui,
            &tr!("DSD Transport"),
            &tr!("PCM conversion is the safe default. DoP works over an I32 PCM stream. Native DSD needs a direct hardware endpoint and is negotiated down with a reported reason when unavailable."),
            theme,
            |ui| {
                let options = [
                    (DsdOutput::PcmConvert, "PCM Conversion"),
                    (DsdOutput::DoP, "DSD over PCM (DoP)"),
                    (DsdOutput::NativeDsd, "Native DSD (Direct)"),
                ];
                if segment_picker(ui, &mut state.dsd_output, &options, theme) {
                    actions.dsd_output_changed = Some(state.dsd_output);
                }
            },
        );

        setting_item(
            ui,
            &tr!("DSD Decimation Filter"),
            &tr!("Filter used when a DSD stream is converted to PCM. It trades audio-band flatness against ultrasonic noise rejection; applies to the next DSD track."),
            theme,
            |ui| {
                let options = [
                    (DsdNoiseFilter::Standard, "Standard (63-tap)"),
                    (DsdNoiseFilter::Low, "Low (95-tap)"),
                    (DsdNoiseFilter::HighOrder, "High Order (127-tap)"),
                    (DsdNoiseFilter::Brickwall, "Brickwall (191-tap)"),
                    (DsdNoiseFilter::SlowLp, "Slow Linear Phase"),
                    (DsdNoiseFilter::SlowMp, "Slow Minimum Phase"),
                ];
                if segment_picker(ui, &mut state.dsd_noise_filter, &options, theme) {
                    actions.dsd_noise_filter_changed = Some(state.dsd_noise_filter);
                }
            },
        );

        setting_item(
            ui,
            &tr!("Ultrasonic Filter"),
            &tr!("Output-domain low-pass that removes shaped noise above the audio band. Off is a bit-exact bypass; the 20 kHz setting is the sharp metering filter."),
            theme,
            |ui| {
                let options = [
                    (UltrasonicFilter::Off, "Off (Bypass)"),
                    (UltrasonicFilter::Cut20k, "20 kHz"),
                    (UltrasonicFilter::Cut30k, "30 kHz"),
                    (UltrasonicFilter::Cut40k, "40 kHz"),
                    (UltrasonicFilter::Cut50k, "50 kHz"),
                    (UltrasonicFilter::X2, "2x Rate"),
                    (UltrasonicFilter::X4, "4x Rate"),
                    (UltrasonicFilter::X8, "8x Rate"),
                ];
                if segment_picker(ui, &mut state.ultrasonic_filter, &options, theme) {
                    actions.ultrasonic_filter_changed = Some(state.ultrasonic_filter);
                }
            },
        );
    });

    // Card 5: Resampler family and noise shaping.
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_equalizer_sliders,
            &tr!("Resampling Filter & Noise Shaping"),
            &tr!(
                "Choose the rate-conversion filter family and the final quantization noise shaper"
            ),
            None,
            theme,
        );

        setting_item(
            ui,
            &tr!("Resampler Filter Family"),
            &tr!("FFT is the historical rubato engine. The polyphase designs trade length and latency: Kaiser and Blackman-Harris are linear phase, Minimum Phase removes pre-ringing, Apodizing rejects images just above the source Nyquist, and Sinc-M is the reference-length converter."),
            theme,
            |ui| {
                let options = [
                    (ResamplerFilter::Fft, "FFT (Rubato)"),
                    (ResamplerFilter::Kaiser, "Kaiser Polyphase"),
                    (ResamplerFilter::BlackmanHarris, "Blackman-Harris"),
                    (ResamplerFilter::MinimumPhase, "Minimum Phase"),
                    (ResamplerFilter::Apodizing, "Apodizing"),
                    (ResamplerFilter::SincM, "Sinc-M (Reference)"),
                ];
                if segment_picker(ui, &mut state.resampler_filter, &options, theme) {
                    actions.resampler_filter_changed = Some(state.resampler_filter);
                }
            },
        );

        setting_item(
            ui,
            &tr!("Resampler Cutoff Ratio"),
            &tr!("Cutoff frequency ratio for anti-aliasing low-pass filter (85% to 99.9%, default 95% of Nyquist). Higher preserves more ultrasonic bandwidth; lower reduces filter ringing."),
            theme,
            |ui| {
                let mut ratio = state.resampler_cutoff_ratio * 100.0;
                if ui
                    .add(
                        egui::Slider::new(&mut ratio, 85.0..=99.9)
                            .suffix("%")
                            .step_by(0.5),
                    )
                    .changed()
                {
                    state.resampler_cutoff_ratio = (ratio / 100.0).clamp(0.85, 0.999);
                    actions.resampler_cutoff_ratio_changed = Some(state.resampler_cutoff_ratio);
                }
            },
        );

        setting_item(
            ui,
            &tr!("Noise Shaper"),
            &tr!("Error-feedback design applied when the output is quantized to an integer format. TPDF is unshaped; Shibata and the NS/LNS designs move noise out of the audible band. Applies to the next stream open."),
            theme,
            |ui| {
                let options = [
                    (NoiseShaper::Tpdf, "TPDF (Unshaped)"),
                    (NoiseShaper::Shibata, "Shibata"),
                    (NoiseShaper::Ns5, "NS5"),
                    (NoiseShaper::Ns9, "NS9"),
                    (NoiseShaper::Lns15, "LNS15"),
                ];
                if segment_picker(ui, &mut state.noise_shaper, &options, theme) {
                    actions.noise_shaper_changed = Some(state.noise_shaper);
                }
            },
        );
    });

    // Card 6: Native DSD modulation and apodizing guidance.
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            |ui, r, c| crate::components::icons::draw_speaker(ui, r, c, false),
            &tr!("Native DSD Modulation & Apodizing"),
            &tr!("PCM to one-bit SDM conversion, and the per-track apodizing recommendation"),
            None,
            theme,
        );

        let sdm_changed = setting_item_toggle(
            ui,
            &tr!("SDM Conversion"),
            &tr!("Convert PCM sources to a one-bit SDM bitstream for native DSD output. Requires a transport the DAC accepts; otherwise the engine reports the negotiated fallback."),
            theme,
            &mut state.sdm.enabled,
        );
        if sdm_changed {
            actions.sdm_config_changed = Some(state.sdm.clone());
        }

        let dsd_bypass_changed = setting_item_toggle(
            ui,
            &tr!("Bypass for Native DSD"),
            &tr!("When SDM conversion is enabled, native DSD tracks (DSF/DFF) bypass delta-sigma re-modulation and play bit-perfect directly to the DAC."),
            theme,
            &mut state.sdm.dsd_bypass,
        );
        if dsd_bypass_changed {
            actions.sdm_config_changed = Some(state.sdm.clone());
        }

        setting_item(
            ui,
            &tr!("SDM Modulator"),
            &tr!("Fifth order suits converters with a simple analog reconstruction filter. Seventh order shapes more in-band noise but needs an analog stage that tolerates more out-of-band energy."),
            theme,
            |ui| {
                let options = [
                    (SdmModulator::FifthOrder, "5th Order"),
                    (SdmModulator::SeventhOrder, "7th Order"),
                ];
                if segment_picker(ui, &mut state.sdm.modulator, &options, theme) {
                    actions.sdm_config_changed = Some(state.sdm.clone());
                }
            },
        );

        setting_item(
            ui,
            &tr!("SDM Rate"),
            &tr!("Auto asks the device for the highest multiple it reports. A fixed selection is only used when the device accepts it; the negotiated rate is reported by the engine."),
            theme,
            |ui| {
                let options = [
                    (SdmRate::Auto, "Auto"),
                    (SdmRate::Dsd64, "DSD64"),
                    (SdmRate::Dsd128, "DSD128"),
                    (SdmRate::Dsd256, "DSD256"),
                    (SdmRate::Dsd512, "DSD512"),
                ];
                if segment_picker(ui, &mut state.sdm.rate, &options, theme) {
                    actions.sdm_config_changed = Some(state.sdm.clone());
                }
            },
        );

        setting_item(
            ui,
            &tr!("Apodizing Guidance"),
            &tr!("Read-only measurement from the decoded source. A high near-Nyquist burst count suggests an apodizing resampler; it is guidance, never a silent filter switch."),
            theme,
            |ui| {
                let Some(report) = extra.apodizing else {
                    ui.label(
                        egui::RichText::new(tr!("No stream analysed yet"))
                            .font(crate::theme::font(11.0))
                            .color(theme.text_dim),
                    );
                    return;
                };
                let recommendation = match report.recommendation {
                    engine::dsp::apodizing::ApodizingRecommendation::InsufficientData => {
                        tr!("Insufficient data")
                    }
                    engine::dsp::apodizing::ApodizingRecommendation::NotNeeded => {
                        tr!("Not needed")
                    }
                    engine::dsp::apodizing::ApodizingRecommendation::Recommended => {
                        tr!("Apodizing recommended")
                    }
                };
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(recommendation)
                            .font(crate::theme::font(11.5))
                            .color(theme.text_primary),
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "{:.1} incidents/min · {} · {:.0} s analysed · threshold {}",
                            report.incidents_per_minute,
                            if report.sample_rate > 0 {
                                format!("{} Hz", report.sample_rate)
                            } else {
                                tr!("unknown rate").to_string()
                            },
                            report.analyzed_seconds,
                            report.threshold
                        ))
                        .font(crate::theme::font(10.5))
                        .color(theme.text_muted),
                    );
                });
            },
        );
    });

    // Card 7: DAC correction (device/rate-specific, distinct from room correction).
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            |ui, r, c| crate::components::icons::draw_speaker(ui, r, c, false),
            &tr!("DAC Correction Profiles"),
            &tr!("Device- and rate-specific correction supplied as pre-rendered FIR profiles. This is a separate bank from room/headphone correction, so both can run and both are reported distinctly."),
            None,
            theme,
        );

        let toggle_changed = setting_item_toggle(
            ui,
            &tr!("DAC Correction"),
            &tr!("Enable the DAC correction bank. The engine matches a profile by output-device name and session rate, loads its FIRs on the control path, and reports the selected profile or the reason it stayed inactive."),
            theme,
            &mut state.dac_correction.enabled,
        );
        let mut correction_changed = toggle_changed;

        setting_item(
            ui,
            &tr!("DAC Correction Profiles"),
            &tr!("One row per profile. Device is a case-insensitive substring (empty or * matches every device); rate 0 matches any rate. Paths are separated by semicolons: one FIR per channel, or one multichannel WAV. Latency is the delay already present in the profile."),
            theme,
            |ui| {
                let mut remove = None;
                ui.vertical(|ui| {
                    if state.dac_correction.profiles.is_empty() {
                        ui.label(
                            egui::RichText::new(tr!("No DAC correction profiles"))
                                .font(crate::theme::font(11.0))
                                .color(theme.text_dim),
                        );
                    }
                    for (index, profile) in state.dac_correction.profiles.iter_mut().enumerate() {
                        let mut row_changed = false;
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("{}", index + 1))
                                    .font(crate::theme::font(10.5))
                                    .color(theme.text_dim),
                            );
                            row_changed |= ui
                                .add(
                                    egui::TextEdit::singleline(&mut profile.dac)
                                        .hint_text(tr!("Device match (e.g. RME ADI-2)"))
                                        .desired_width(220.0)
                                        .font(crate::theme::font(10.5)),
                                )
                                .changed();
                            row_changed |= ui
                                .add(
                                    egui::DragValue::new(&mut profile.sample_rate)
                                        .range(0..=768_000)
                                        .speed(100.0)
                                        .suffix(" Hz"),
                                )
                                .changed();
                            if ui.small_button("×").clicked() {
                                remove = Some(index);
                            }
                        });
                        ui.horizontal(|ui| {
                            row_changed |= ui
                                .add(
                                    egui::DragValue::new(&mut profile.latency_ms)
                                        .range(0.0..=1000.0)
                                        .speed(0.05)
                                        .suffix(" ms"),
                                )
                                .changed();
                            ui.label(
                                egui::RichText::new(tr!("depth"))
                                    .font(crate::theme::font(10.5))
                                    .color(theme.text_muted),
                            );
                            row_changed |= ui
                                .add(
                                    egui::Slider::new(&mut profile.depth, 0.0..=1.0)
                                        .show_value(true),
                                )
                                .changed();
                        });
                        let mut paths = profile.ir_paths.join("; ");
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut paths)
                                    .hint_text(tr!("FIR paths, separated by semicolons"))
                                    .desired_width(f32::INFINITY)
                                    .font(crate::theme::font(10.5)),
                            )
                            .changed()
                        {
                            profile.ir_paths = paths
                                .split(';')
                                .map(str::trim)
                                .filter(|path| !path.is_empty())
                                .map(str::to_string)
                                .collect();
                            row_changed = true;
                        }
                        correction_changed |= row_changed;
                    }
                });
                if let Some(index) = remove {
                    state.dac_correction.profiles.remove(index);
                    correction_changed = true;
                }
            },
        );

        setting_item(
            ui,
            &tr!("Add DAC Profile"),
            &tr!("Add another device/rate profile. The first matching profile wins; keep the device match specific when several DACs share an output rate."),
            theme,
            |ui| {
                if ui
                    .add(pill_btn(tr!("Add Profile"), false, theme))
                    .clicked()
                {
                    state
                        .dac_correction
                        .profiles
                        .push(config::DacCorrectionProfileConfig::default());
                    state.dac_correction.enabled = true;
                    correction_changed = true;
                }
            },
        );

        if correction_changed {
            actions.dac_correction_changed = Some(state.dac_correction.clone());
        }
    });
}
