//! The Hardware & DAC category: backend, device, bit-perfect mode, rate
//! policy, and resampling precision.

use super::super::widgets::{
    card_header, pill_btn, segment_picker, setting_card, setting_item, setting_item_with_info,
    toggle_switch,
};
use super::super::{SettingsExtraContext, SettingsTabActions, SettingsTabState};
use crate::theme::Theme;
use config::{FallbackPolicy, PrecisionMode, ResamplerQuality, SampleRatePolicy, VolumeMode};
use eframe::egui::{self, Color32, CornerRadius, Sense, Stroke, Vec2};

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
                "Software volume scaling vs Fixed 100% line-out for external preamp/headphone amp"
            ),
            Some(&crate::components::info_box::INFO_VOLUME_MODE),
            theme,
            |ui| {
                let options = [
                    (VolumeMode::SoftwareOnly, "Software Controlled"),
                    (VolumeMode::HardwarePreferred, "Hardware Preferred"),
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
}
