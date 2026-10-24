//! The Mix Bus category: multitrack lanes, ducking, aux returns, bass
//! management, LFE, up/down mixing, and the CLAP insert.

use super::super::cards::render_chain_presets_card;
use super::super::widgets::{
    badge_label, card_header, dual_tone_slider, pill_btn, segment_picker, setting_card,
    setting_item, setting_item_with_info, toggle_switch,
};
use super::super::{SettingsTabActions, SettingsTabState};
use crate::theme::Theme;
use config::{ChannelMixTemplate, CrossoverFilterType, CrossoverSlope};
use eframe::egui::{self, Vec2};
use engine::AudioSource;

pub(in crate::components::settings_tab) fn render_mixbus_category(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
    actions: &mut SettingsTabActions,
) {
    // Card 0: Whole-chain presets (spans the EQ panel and the cards below).
    render_chain_presets_card(ui, state, theme, actions);

    // CLAP plugin insert, placed above the mixer strips: whether a plugin is in
    // the chain changes the chain, while the strips below are details of it. The
    // rack window owns every control, so this card only reports and routes.
    render_clap_plugin_card(ui, state, theme, actions);

    // Card 1: Multitrack Mixer Strip
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_equalizer_sliders,
            &tr!("Multitrack Lane Mixer"),
            &tr!(
                "Adjust individual input tracks, gain attenuation, stereo panning, and mute states"
            ),
            None,
            theme,
        );

        setting_item(
            ui,
            &tr!("Add Audio Source File"),
            &tr!("Ingest secondary stems, reference tracks, or voice commentary channels"),
            theme,
            |ui| {
                let edit = egui::TextEdit::singleline(&mut state.add_track_path)
                    .hint_text(tr!(Msg::S55955311908B0E1F))
                    .desired_width(180.0)
                    .font(crate::theme::font(12.0));
                ui.add(edit);
                if ui
                    .add(pill_btn(tr!(Msg::S3C92D2852EC673DE), false, theme))
                    .clicked()
                    && !state.add_track_path.is_empty()
                {
                    actions.add_track =
                        Some(AudioSource::File(state.add_track_path.clone().into()));
                }
            },
        );

        setting_item(
            ui,
            &tr!("Active Channel Slot"),
            &tr!("Select mixer channel strip to control"),
            theme,
            |ui| {
                let mut slot = state.track_slot as f32;
                if ui
                    .add(
                        egui::DragValue::new(&mut slot)
                            .range(2.0..=16.0)
                            .min_size(Vec2::new(76.0, 24.0)),
                    )
                    .changed()
                {
                    state.track_slot = slot as u8;
                }
                badge_label(ui, format!("Slot #{}", state.track_slot), theme);
                if ui
                    .add(pill_btn(tr!(Msg::SB4BA4B46A7C22BDB), false, theme))
                    .clicked()
                {
                    actions.remove_track = Some(state.track_slot);
                }
            },
        );

        setting_item(
            ui,
            &tr!("Channel Gain & Pan"),
            &tr!("Fine-tune channel volume level and stereo pan balance"),
            theme,
            |ui| {
                ui.label(tr!(Msg::S05EEC723A5F2B92C));
                let mut g = state.track_gain;
                if dual_tone_slider(ui, &mut g, 0.0..=2.0, 0.01, 0.0, theme) {
                    state.track_gain = g;
                    actions.track_gain_changed = Some((state.track_slot, g));
                }
                badge_label(ui, format!("{:.2}x", state.track_gain), theme);

                ui.label(tr!(Msg::S2DFAE419ECFC86C2));
                let mut p = state.track_pan;
                if dual_tone_slider(ui, &mut p, -1.0..=1.0, 0.02, 0.0, theme) {
                    state.track_pan = p;
                    actions.track_pan_changed = Some((state.track_slot, p));
                }
                badge_label(ui, format!("{:+.2}", state.track_pan), theme);
            },
        );

        setting_item(
            ui,
            &tr!("Channel Status"),
            &tr!("Mute and active playback enable state"),
            theme,
            |ui| {
                let mut m = state.track_muted;
                if ui.checkbox(&mut m, "Mute").changed() {
                    state.track_muted = m;
                    actions.track_mute_changed = Some((state.track_slot, m));
                }
                let mut a = state.track_active;
                if ui.checkbox(&mut a, "Active").changed() {
                    state.track_active = a;
                    actions.track_active_changed = Some((state.track_slot, a));
                }
            },
        );
    });

    // Card 2: Sidechain Ducking Compressor
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_trending_up,
            &tr!("Sidechain Ducking Compressor"),
            &tr!("Automatically reduces background music level when voice or commentary channel speaks"),
            None,
            theme,
        );

        setting_item(
            ui,
            &tr!("Trigger Source Slot"),
            &tr!("Mixer channel monitored to trigger ducking attenuation"),
            theme,
            |ui| {
                let mut slot = state.duck_source_slot as f32;
                if ui
                    .add(
                        egui::DragValue::new(&mut slot)
                            .range(0.0..=15.0)
                            .min_size(Vec2::new(76.0, 24.0)),
                    )
                    .changed()
                {
                    state.duck_source_slot = slot as u8;
                }
                badge_label(ui, format!("Source #{}", state.duck_source_slot), theme);
            },
        );

        setting_item_with_info(
            ui,
            &tr!("Threshold & Depth"),
            &tr!("Level that activates ducking (dB) and total volume reduction depth (dB)"),
            Some(&crate::components::info_box::INFO_DUCKING),
            theme,
            |ui| {
                ui.label(tr!(Msg::SC4B836D6B794CF46));
                let mut t = state.duck_threshold_db;
                if ui
                    .add(
                        egui::DragValue::new(&mut t)
                            .speed(0.5)
                            .suffix(" dB")
                            .range(-60.0..=0.0)
                            .min_size(Vec2::new(76.0, 24.0)),
                    )
                    .changed()
                {
                    state.duck_threshold_db = t;
                }
                ui.label(tr!(Msg::S2DB356921ECFD550));
                let mut d = state.duck_depth_db;
                if ui
                    .add(
                        egui::DragValue::new(&mut d)
                            .speed(0.5)
                            .suffix(" dB")
                            .range(0.0..=30.0)
                            .min_size(Vec2::new(76.0, 24.0)),
                    )
                    .changed()
                {
                    state.duck_depth_db = d;
                }
            },
        );

        setting_item(
            ui,
            &tr!("Attack & Release Envelope"),
            &tr!("Reaction speed (ms) and recovery return time (ms)"),
            theme,
            |ui| {
                ui.label(tr!(Msg::SAB0AFE53776CF2E5));
                let mut a = state.duck_attack_ms;
                if ui
                    .add(
                        egui::DragValue::new(&mut a)
                            .speed(1.0)
                            .suffix(" ms")
                            .range(1.0..=100.0)
                            .min_size(Vec2::new(76.0, 24.0)),
                    )
                    .changed()
                {
                    state.duck_attack_ms = a;
                }
                ui.label(tr!(Msg::SBE02DA81DAB96BEC));
                let mut r = state.duck_release_ms;
                if ui
                    .add(
                        egui::DragValue::new(&mut r)
                            .speed(5.0)
                            .suffix(" ms")
                            .range(10.0..=1000.0)
                            .min_size(Vec2::new(76.0, 24.0)),
                    )
                    .changed()
                {
                    state.duck_release_ms = r;
                }
                if ui
                    .add(pill_btn(tr!(Msg::S63C434143EE5AFE5), false, theme))
                    .clicked()
                {
                    actions.duck_tracks = true;
                }
            },
        );
    });

    // Card 3: Stereo Image Enhancer & Aux
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_bar_chart,
            &tr!("Auxiliary Bus & Psychoacoustic Enhancer"),
            &tr!("Stereo soundstage widening algorithm and parallel auxiliary return loop"),
            None,
            theme,
        );

        setting_item_with_info(
            ui,
            &tr!("Stereo Field Enhancer"),
            &tr!("Mid-side psychoacoustic stereo image width expander"),
            Some(&crate::components::info_box::INFO_STEREO_WIDTH),
            theme,
            |ui| {
                let mut on = state.stereo_enhancer_enabled;
                if toggle_switch(ui, &mut on, theme) {
                    state.stereo_enhancer_enabled = on;
                    actions.stereo_enhancer_changed = Some(on);
                }
            },
        );

        setting_item(
            ui,
            &tr!("Auxiliary Return Bus"),
            &tr!("Enable parallel effects return loop with dedicated return gain"),
            theme,
            |ui| {
                let mut on = state.aux_enabled;
                if toggle_switch(ui, &mut on, theme) {
                    state.aux_enabled = on;
                    actions.aux_changed = true;
                }
                if state.aux_enabled {
                    ui.label(tr!(Msg::S5C6D692771336A94));
                    let mut v = state.aux_return_gain;
                    if dual_tone_slider(ui, &mut v, 0.0..=2.0, 0.01, 0.0, theme) {
                        state.aux_return_gain = v;
                        actions.aux_changed = true;
                    }
                    badge_label(ui, format!("{:.2}x", state.aux_return_gain), theme);
                }
            },
        );
    });

    // Card 6: Bass Management (subwoofer integration)
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_equalizer_sliders,
            &tr!("Bass Management & Subwoofer Integration"),
            &tr!("Route low frequencies to a subwoofer with a proper crossover instead of relying on speaker roll-off"),
            None,
            theme,
        );

        let mut changed = false;

        setting_item(
            ui,
            &tr!("Bass Management"),
            &tr!("Enable a high-pass on the mains plus a matching low-pass for the subwoofer"),
            theme,
            |ui| {
                let mut on = state.bass_management.enabled;
                if toggle_switch(ui, &mut on, theme) {
                    state.bass_management.enabled = on;
                    changed = true;
                }
            },
        );

        if state.bass_management.enabled {
            setting_item(
                ui,
                &tr!("Crossover Frequency"),
                &tr!("Hand-off point between mains and subwoofer (60–120 Hz is the usual home-theatre range)"),
                theme,
                |ui| {
                    let mut v = state.bass_management.crossover_hz;
                    if dual_tone_slider(ui, &mut v, 40.0..=200.0, 1.0, 0.0, theme) {
                        state.bass_management.crossover_hz = v;
                        changed = true;
                    }
                    badge_label(ui, format!("{:.0} Hz", state.bass_management.crossover_hz), theme);
                },
            );

            setting_item(
                ui,
                &tr!("Crossover Slope"),
                &tr!("Steeper slopes protect the mains but add group delay"),
                theme,
                |ui| {
                    let options = [
                        (CrossoverSlope::Db12, "12 dB/oct"),
                        (CrossoverSlope::Db24, "24 dB/oct"),
                        (CrossoverSlope::Db48, "48 dB/oct"),
                    ];
                    if segment_picker(ui, &mut state.bass_management.slope, &options, theme) {
                        changed = true;
                    }
                },
            );

            setting_item(
                ui,
                &tr!("Filter Topology"),
                &tr!("Linkwitz-Riley sums flat at the crossover; Butterworth is −3 dB there"),
                theme,
                |ui| {
                    let options = [
                        (CrossoverFilterType::LinkwitzRiley, "Linkwitz-Riley"),
                        (CrossoverFilterType::Butterworth, "Butterworth"),
                    ];
                    if segment_picker(ui, &mut state.bass_management.filter_type, &options, theme) {
                        changed = true;
                    }
                },
            );

            setting_item(
                ui,
                &tr!("Filter Q"),
                &tr!("1/√2 ≈ 0.707 is the Butterworth default"),
                theme,
                |ui| {
                    let mut v = state.bass_management.q;
                    if dual_tone_slider(ui, &mut v, 0.4..=1.4, 0.01, 0.0, theme) {
                        state.bass_management.q = v;
                        changed = true;
                    }
                    badge_label(ui, format!("{:.3}", state.bass_management.q), theme);
                },
            );

            setting_item(
                ui,
                &tr!("Mains High-Pass"),
                &tr!("Also high-pass the main speakers. Disable to high-pass only the sub path"),
                theme,
                |ui| {
                    let mut on = state.bass_management.mains_highpass_enabled;
                    if toggle_switch(ui, &mut on, theme) {
                        state.bass_management.mains_highpass_enabled = on;
                        changed = true;
                    }
                },
            );

            setting_item(
                ui,
                &tr!("Subwoofer Distance/Delay"),
                &tr!("Align the sub's arrival with the mains (0–50 ms; 1 ms ≈ 34 cm)"),
                theme,
                |ui| {
                    let mut v = state.bass_management.sub_delay_ms;
                    if dual_tone_slider(ui, &mut v, 0.0..=50.0, 0.1, 0.0, theme) {
                        state.bass_management.sub_delay_ms = v;
                        changed = true;
                    }
                    badge_label(
                        ui,
                        format!("{:.1} ms", state.bass_management.sub_delay_ms),
                        theme,
                    );
                },
            );

            setting_item(
                ui,
                &tr!("Subwoofer Phase"),
                &tr!("Continuously trim the sub's phase for the smoothest crossover sum"),
                theme,
                |ui| {
                    let mut v = state.bass_management.sub_phase_degrees;
                    if dual_tone_slider(ui, &mut v, 0.0..=180.0, 1.0, 0.0, theme) {
                        state.bass_management.sub_phase_degrees = v;
                        changed = true;
                    }
                    badge_label(
                        ui,
                        format!("{:.0}°", state.bass_management.sub_phase_degrees),
                        theme,
                    );
                },
            );

            setting_item(
                ui,
                &tr!("Subwoofer Polarity"),
                &tr!("Invert the sub's polarity (use instead of, not with, coarse phase)"),
                theme,
                |ui| {
                    let mut inv = state.bass_management.sub_polarity_invert;
                    if toggle_switch(ui, &mut inv, theme) {
                        state.bass_management.sub_polarity_invert = inv;
                        changed = true;
                    }
                },
            );
        }

        if changed {
            actions.bass_management_changed = Some(state.bass_management.clone());
        }
    });

    // Card 7: LFE channel
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_equalizer_sliders,
            &tr!("LFE Channel"),
            &tr!("Gain and low-pass for content that already carries a dedicated .1 channel"),
            None,
            theme,
        );

        let mut changed = false;

        setting_item(
            ui,
            &tr!("LFE Processing"),
            &tr!("Apply gain and an optional low-pass to LFE-role channels"),
            theme,
            |ui| {
                let mut on = state.lfe.enabled;
                if toggle_switch(ui, &mut on, theme) {
                    state.lfe.enabled = on;
                    changed = true;
                }
            },
        );

        if state.lfe.enabled {
            setting_item(
                ui,
                &tr!("LFE Gain"),
                &tr!("Trim the .1 channel relative to the other speakers"),
                theme,
                |ui| {
                    let mut v = state.lfe.gain_db;
                    if dual_tone_slider(ui, &mut v, -20.0..=20.0, 0.5, 0.0, theme) {
                        state.lfe.gain_db = v;
                        changed = true;
                    }
                    badge_label(ui, format!("{:+.1} dB", state.lfe.gain_db), theme);
                },
            );

            let mut lp_on = state.lfe.crossover_hz.is_some();
            let mut lp_hz = state.lfe.crossover_hz.unwrap_or(120.0);
            setting_item(
                ui,
                &tr!("LFE Low-Pass"),
                &tr!("Standard LFE content is band-limited to 120 Hz"),
                theme,
                |ui| {
                    if toggle_switch(ui, &mut lp_on, theme) {
                        changed = true;
                    }
                    if lp_on {
                        let mut hz = lp_hz;
                        if dual_tone_slider(ui, &mut hz, 40.0..=200.0, 1.0, 0.0, theme) {
                            lp_hz = hz;
                            changed = true;
                        }
                        badge_label(ui, format!("{:.0} Hz", lp_hz), theme);
                    }
                },
            );
            if changed {
                state.lfe.crossover_hz = if lp_on { Some(lp_hz) } else { None };
            }
        }

        if changed {
            actions.lfe_changed = Some(state.lfe.clone());
        }
    });

    // Card 8: Up / Down Mixing
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_equalizer_sliders,
            &tr!("Up / Down Mixing"),
            &tr!("Convert between stereo and multichannel with a deterministic, spec-based matrix"),
            None,
            theme,
        );

        let mut changed = false;

        setting_item(
            ui,
            &tr!("Channel Mapping"),
            &tr!("Identity by default; ITU-R BS.775 folds multichannel to stereo and can build 5.1/7.1 from stereo"),
            theme,
            |ui| {
                let mut on = state.channel_mix.enabled;
                if toggle_switch(ui, &mut on, theme) {
                    state.channel_mix.enabled = on;
                    changed = true;
                }
            },
        );

        if state.channel_mix.enabled {
            setting_item(
                ui,
                &tr!("Mapping Template"),
                &tr!("Choose the up-mix or down-mix matrix applied at the output boundary"),
                theme,
                |ui| {
                    let options = [
                        (ChannelMixTemplate::ItuBs775, "ITU-R BS.775"),
                        (ChannelMixTemplate::StereoToFiveOne, "Stereo → 5.1"),
                        (ChannelMixTemplate::StereoToSevenOne, "Stereo → 7.1"),
                        (
                            ChannelMixTemplate::StereoToSevenPointOneFour,
                            "Stereo → 7.1.4",
                        ),
                        (ChannelMixTemplate::FiveOneToStereo, "5.1 → Stereo"),
                        (ChannelMixTemplate::SevenOneToStereo, "7.1 → Stereo"),
                        (
                            ChannelMixTemplate::SevenPointOneFourToStereo,
                            "7.1.4 → Stereo",
                        ),
                    ];
                    if segment_picker(ui, &mut state.channel_mix.template, &options, theme) {
                        changed = true;
                    }
                },
            );
        }

        if changed {
            actions.channel_mix_changed = Some(state.channel_mix.clone());
        }
    });
}

/// The CLAP plugin insert: what is loaded, and the way into its rack.
fn render_clap_plugin_card(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
    actions: &mut SettingsTabActions,
) {
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_puzzle,
            &tr!("CLAP Plugin Insert"),
            &tr!(
                "Host a standard CLAP audio plugin as the master insert, driven from its own rack"
            ),
            None,
            theme,
        );

        setting_item(
            ui,
            &tr!("Hosted Plugin"),
            &tr!("A plugin's own editor opens as a floating native window; the rack also exposes every parameter and named presets"),
            theme,
            |ui| {
                ui.label(
                    egui::RichText::new(state.clap.status_line())
                        .font(crate::theme::font(11.5))
                        .color(if state.clap.error.is_some() {
                            theme.destructive
                        } else if state.clap.loaded {
                            theme.text_primary
                        } else {
                            theme.text_secondary
                        }),
                );
            },
        );

        setting_item(
            ui,
            &tr!("Plugin Rack"),
            &tr!("Parameters, host bypass, and named presets that capture the plugin's own state"),
            theme,
            |ui| {
                if ui
                    .add(pill_btn(tr!(Msg::S3C7D232062305680), true, theme))
                    .clicked()
                {
                    actions.clap_open_rack = true;
                }
                if ui
                    .add(pill_btn(tr!(Msg::S6B56819702CBBA69), false, theme))
                    .clicked()
                {
                    actions.clap_choose_library = true;
                }
                if state.clap.loaded
                    && ui
                        .add(pill_btn(tr!(Msg::S5D3714BC882DF1B2), false, theme))
                        .clicked()
                {
                    actions.clap_unload = true;
                }
            },
        );
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Category 5: Capture & Tools
// ─────────────────────────────────────────────────────────────────────────────
