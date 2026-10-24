//! The Tools category: capture, ReplayGain, recovery, diagnostics, room
//! measurement, CD ripping, export, and update checks.

use super::super::widgets::{
    badge_label, badge_label_toned, card_header, card_header_with_info, pill_btn, setting_card,
    setting_item, toggle_switch,
};
use super::super::{SettingsExtraContext, SettingsTabActions, SettingsTabState};
use crate::theme::Theme;
use eframe::egui::{self, Color32, CornerRadius};

pub(in crate::components::settings_tab) fn render_tools_category(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
    extra: &SettingsExtraContext<'_>,
    actions: &mut SettingsTabActions,
) {
    // Card 1: Direct System Audio Capture
    setting_card(ui, theme, |ui| {
        card_header_with_info(
            ui,
            |ui, r, c| crate::components::icons::draw_speaker(ui, r, c, false),
            "High-Resolution Stream Capture",
            "Record live bit-perfect audio stream directly to uncompressed PCM WAV files",
            None,
            Some(&crate::components::info_box::INFO_STREAM_CAPTURE),
            theme,
        );

        setting_item(
            ui,
            &tr!("Output Recording Path"),
            &tr!("Destination WAV file on local storage"),
            theme,
            |ui| {
                let edit = egui::TextEdit::singleline(&mut state.capture_path)
                    .hint_text(tr!(Msg::S4DB11DCD6DB5E805))
                    .desired_width(180.0)
                    .font(crate::theme::font(12.0));
                ui.add(edit);
            },
        );

        setting_item(
            ui,
            &tr!("Input / Loopback Device"),
            &tr!("Capture endpoint (leave blank for loopback master monitor)"),
            theme,
            |ui| {
                let edit = egui::TextEdit::singleline(&mut state.capture_device)
                    .hint_text(tr!(Msg::S6D98136CAD659CFD))
                    .desired_width(180.0)
                    .font(crate::theme::font(12.0));
                ui.add(edit);
            },
        );

        setting_item(
            ui,
            &tr!("Recording Controls"),
            &tr!("Start or finalize audio recording file"),
            theme,
            |ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(tr!("Start Capture"))
                                .font(crate::theme::font(12.0))
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(theme.accent)
                        .corner_radius(CornerRadius::same(theme.radius_md)),
                    )
                    .clicked()
                {
                    actions.start_capture = true;
                }

                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(tr!("Stop Capture"))
                                .font(crate::theme::font(12.0))
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(theme.destructive)
                        .corner_radius(CornerRadius::same(theme.radius_md)),
                    )
                    .clicked()
                {
                    actions.stop_capture = true;
                }
            },
        );
    });

    // Card 2: Loudness Tag Writer & ReplayGain Scanner
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_list_icon,
            &tr!("ReplayGain & Loudness Tagging Tool"),
            &tr!("Batch analyze audio files and permanently write ID3/FLAC loudness normalization tags"),
            None,
            theme,
        );

        setting_item(
            ui,
            &tr!("Target File Path"),
            &tr!("Path to audio file to measure integrated LUFS and inject tags"),
            theme,
            |ui| {
                let edit = egui::TextEdit::singleline(&mut state.loudness_tag_path)
                    .hint_text(tr!("/path/to/song.flac…"))
                    .desired_width(180.0)
                    .font(crate::theme::font(12.0));
                ui.add(edit);
                if ui
                    .add(pill_btn(tr!(Msg::S7178B1280039227D), false, theme))
                    .clicked()
                    && !state.loudness_tag_path.is_empty()
                {
                    actions.write_loudness_tags = Some(state.loudness_tag_path.clone());
                }
            },
        );

        setting_item(
            ui,
            &tr!("Library ReplayGain Scan"),
            &tr!("Analyse every track that has no loudness data (EBU R128 + ReplayGain 2.0). Results are cached for playback."),
            theme,
            |ui| {
                if extra.loudness_running {
                    let total = extra.loudness_total.max(1);
                    let frac = extra.loudness_done as f32 / total as f32;
                    ui.add(
                        egui::ProgressBar::new(frac)
                            .desired_width(150.0)
                            .text(format!("{}/{}", extra.loudness_done, extra.loudness_total)),
                    );
                    if ui.add(pill_btn(tr!(Msg::S2CEB11BE2290BB1B), true, theme)).clicked() {
                        actions.cancel_loudness_scan = true;
                    }
                } else {
                    let label = if extra.loudness_has_run {
                        "Rescan Library"
                    } else {
                        "Scan Library"
                    };
                    if ui.add(pill_btn(label, false, theme)).clicked() {
                        actions.scan_loudness_library = true;
                    }
                }
            },
        );

        setting_item(
            ui,
            &tr!("Write Tags Into Files"),
            &tr!("Also save the measured ReplayGain / EBU R128 values into each file's tags (FLAC, MP3, M4A, WAV …). Off by default — this is the only setting that modifies your audio files; DSD and raw PCM are skipped."),
            theme,
            |ui| {
                toggle_switch(ui, &mut state.loudness_write_tags, theme);
            },
        );

        if !extra.loudness_current.is_empty() {
            ui.label(
                egui::RichText::new(extra.loudness_current)
                    .font(crate::theme::font(11.0))
                    .color(theme.text_muted),
            );
        }
    });

    // Card 3: Stream Recovery & Diagnostics
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_gear,
            &tr!("Audio Engine Stream Recovery"),
            &tr!("Re-arm playback queues, drain ringbuffers, and recover from hardware USB/DAC disconnects"),
            None,
            theme,
        );

        setting_item(
            ui,
            &tr!("Stream Recovery"),
            &tr!("Force re-initializes output pipeline without restarting ShadowPlayer"),
            theme,
            |ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(tr!("Recover Stream & Re-arm"))
                                .font(crate::theme::font(12.0))
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(theme.accent)
                        .corner_radius(CornerRadius::same(theme.radius_md)),
                    )
                    .clicked()
                {
                    actions.recover_stream = true;
                }
            },
        );
    });

    // Card 4: Runtime Diagnostics
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_gear,
            &tr!("Runtime Diagnostics"),
            &tr!("Live figures measured by the audio engine's output path"),
            None,
            theme,
        );

        // These rows come from `output_facts`, the same function the
        // Bit-Perfect modal uses. Do not add a diagnostics row here directly:
        // a fact that exists on only one surface is exactly the drift this
        // shared list prevents.
        for fact in
            crate::components::output_facts::output_facts(extra.output_info, extra.latency_ms)
        {
            setting_item(ui, fact.label, fact.detail, theme, |ui| {
                badge_label_toned(ui, fact.value, fact.tone, theme);
            });
        }

        setting_item(
            ui,
            &tr!("DSP Graph Rebuilds"),
            &tr!("Mean cost of rebuilding the processing graph (codec, sample-rate or routing change)"),
            theme,
            |ui| {
                badge_label(
                    ui,
                    format!(
                        "{:.2} ms mean · {} total",
                        extra.graph_build_ms, extra.graph_builds
                    ),
                    theme,
                );
            },
        );
    });

    // Card 5: Room-correction measurement
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_bar_chart,
            "Room Correction Measurement",
            "Play a sweep, capture it with a microphone, and derive a correction you can audition",
            None,
            theme,
        );

        setting_item(
            ui,
            "Measurement Wizard",
            "Choose the input device and sweep length, watch each stage, and cancel without keeping a partial recording",
            theme,
            |ui| {
                if ui
                    .add(pill_btn("Open Measurement Wizard", true, theme))
                    .clicked()
                {
                    actions.open_room_measurement = true;
                }
            },
        );
    });

    // Card 6: CD extraction
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_disc,
            "CD Ripper",
            "Read audio CDs in paranoia mode and encode straight to FLAC, WAV or Opus",
            None,
            theme,
        );

        setting_item(
            ui,
            "Disc Extraction",
            "Probe the drive, fetch CDDB metadata when available, pick tracks, and write new files without overwriting",
            theme,
            |ui| {
                if ui
                    .add(pill_btn("Open CD Ripper", true, theme))
                    .clicked()
                {
                    actions.open_cd_ripper = true;
                }
            },
        );
    });

    // Card 7: Transcode export
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_list_icon,
            "Transcode Export",
            "Make FLAC 16/24-bit or 48 kHz Opus copies of local audio, with tags copied where possible",
            None,
            theme,
        );

        setting_item(
            ui,
            "Format Export",
            "Batch export selected files to one output folder; existing files are never replaced unless you allow it",
            theme,
            |ui| {
                if ui
                    .add(pill_btn("Open Transcode Export", true, theme))
                    .clicked()
                {
                    actions.open_transcode_export = true;
                }
            },
        );
    });

    // Card 8: Opt-in updates
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_gear,
            "Automatic Updates",
            "Opt-in GitHub release checks with mandatory SHA-256 verification; nothing installs itself",
            None,
            theme,
        );

        setting_item(
            ui,
            "Update Check",
            "Off by default. When enabled, Check contacts GitHub, compares versions, and offers a verified download for this platform",
            theme,
            |ui| {
                if ui.add(pill_btn("Open Update Check", false, theme)).clicked() {
                    actions.open_update_check = true;
                }
            },
        );
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Category 6: Library & Appearance
// ─────────────────────────────────────────────────────────────────────────────
