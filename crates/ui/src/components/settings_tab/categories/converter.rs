//! The File Converter category: multi-format batch conversion, quality presets,
//! sample rate and channel downmixing, destination templates, and queue management.

use super::super::widgets::{card_header, pill_btn, setting_card, setting_item, toggle_switch};
use super::super::{SettingsExtraContext, SettingsTabActions, SettingsTabState};
use crate::theme::Theme;
use eframe::egui::{self, Color32, CornerRadius, Vec2};
use media_tools::transcode::OutputFormat;

pub(in crate::components::settings_tab) fn render_converter_category(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
    extra: &SettingsExtraContext<'_>,
    actions: &mut SettingsTabActions,
) {
    // Card 1: Converter Format & Target Quality
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_reset,
            "Converter Format & Quality",
            "Target encoding format, bit depth, resampler rate, and channel routing",
            None,
            theme,
        );

        setting_item(
            ui,
            &tr!("Target Audio Format"),
            &tr!("Select output format (FLAC, WAV, MP3, Opus, Ogg Vorbis, AIFF, Apple CAF)"),
            theme,
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                    let formats = [
                        (OutputFormat::Flac { bit_depth: 24 }, "FLAC 24-bit"),
                        (OutputFormat::Flac { bit_depth: 16 }, "FLAC 16-bit"),
                        (OutputFormat::Wav { bit_depth: 24 }, "WAV 24-bit"),
                        (OutputFormat::Wav { bit_depth: 16 }, "WAV 16-bit"),
                        (OutputFormat::WavFloat, "WAV 32-bit Float"),
                        (OutputFormat::Mp3, "MP3"),
                        (OutputFormat::Opus, "Opus 48k"),
                        (OutputFormat::Vorbis, "Ogg Vorbis"),
                        (OutputFormat::Aiff { bit_depth: 24 }, "AIFF 24-bit"),
                        (OutputFormat::CafAlac { bit_depth: 24 }, "Apple CAF ALAC"),
                    ];
                    for (fmt, label) in formats {
                        let selected = state.converter_format == fmt;
                        if ui.add(pill_btn(label, selected, theme)).clicked() {
                            state.converter_format = fmt;
                        }
                    }
                });
            },
        );

        setting_item(
            ui,
            &tr!("Target Sample Rate"),
            &tr!("Resample using windowed-sinc filter, or keep source rate untouched"),
            theme,
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                    let rates = [
                        (0u32, "Keep Source"),
                        (44100u32, "44.1 kHz (CD)"),
                        (48000u32, "48 kHz"),
                        (88200u32, "88.2 kHz"),
                        (96000u32, "96 kHz (Hi-Res)"),
                        (192000u32, "192 kHz (Master)"),
                    ];
                    for (rate, label) in rates {
                        let selected = state.converter_sample_rate == rate;
                        if ui.add(pill_btn(label, selected, theme)).clicked() {
                            state.converter_sample_rate = rate;
                        }
                    }
                });
            },
        );

        setting_item(
            ui,
            &tr!("Channel Configuration"),
            &tr!("Downmix surround sound to stereo, fold to mono, or keep source channels"),
            theme,
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                    let channels = [
                        (0u8, "Keep Source"),
                        (2u8, "Stereo (BS.775)"),
                        (1u8, "Mono Downmix"),
                    ];
                    for (ch, label) in channels {
                        let selected = state.converter_channels == ch;
                        if ui.add(pill_btn(label, selected, theme)).clicked() {
                            state.converter_channels = ch;
                        }
                    }
                });
            },
        );
    });

    // Card 2: Destination & Naming
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_folder,
            "Destination & Naming",
            "Output directory location, path formatting tokens, and overwrite rules",
            None,
            theme,
        );

        setting_item(
            ui,
            &tr!("Output Folder"),
            &tr!("Directory where converted audio files will be written"),
            theme,
            |ui| {
                ui.horizontal(|ui| {
                    let mut dir_str = state.converter_output_dir.to_string_lossy().into_owned();
                    let edit = egui::TextEdit::singleline(&mut dir_str)
                        .hint_text("Default (source file directory)")
                        .desired_width(180.0)
                        .font(crate::theme::font(12.0));
                    if ui.add(edit).changed() {
                        state.converter_output_dir = std::path::PathBuf::from(dir_str);
                    }
                    if ui.add(pill_btn("Browse…", false, theme)).clicked() {
                        actions.pick_converter_dir = true;
                    }
                });
            },
        );

        setting_item(
            ui,
            &tr!("File Naming Pattern"),
            &tr!("Custom naming template with tokens: {artist}, {album}, {track}, {title}, etc."),
            theme,
            |ui| {
                ui.vertical(|ui| {
                    let edit = egui::TextEdit::singleline(&mut state.converter_naming_pattern)
                        .hint_text("{artist}/{album}/{track} - {title}")
                        .desired_width(240.0)
                        .font(crate::theme::font(12.0));
                    ui.add(edit);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(2.0, 2.0);
                        for token in ["{artist}", "{album}", "{track}", "{title}", "{format}"] {
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(token)
                                            .font(crate::theme::mono_font(10.0))
                                            .color(theme.text_dim),
                                    )
                                    .fill(theme.bg_card_hover)
                                    .corner_radius(CornerRadius::same(theme.radius_xs)),
                                )
                                .clicked()
                            {
                                state.converter_naming_pattern.push_str(token);
                            }
                        }
                    });
                });
            },
        );

        setting_item(
            ui,
            &tr!("Overwrite Existing Files"),
            &tr!("Off: append index (1) on conflict. On: replace existing destination file."),
            theme,
            |ui| {
                toggle_switch(ui, &mut state.converter_overwrite, theme);
            },
        );
    });

    // Card 3: Metadata & Tag Preservation
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_disc,
            "Metadata & Tag Preservation",
            "Preserve artist, album, title, year, genre, and track numbering across formats",
            None,
            theme,
        );

        setting_item(
            ui,
            &tr!("Preserve Metadata Tags"),
            &tr!("Copy title, artist, album, genre, year, and track numbering to converted files"),
            theme,
            |ui| {
                toggle_switch(ui, &mut state.converter_copy_tags, theme);
            },
        );
    });

    // Card 4: Batch File Conversion Queue
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_list_icon,
            "Batch File Conversion Queue",
            "Queue audio files to convert with the configured parameters above",
            None,
            theme,
        );

        setting_item(
            ui,
            &tr!("Queue Source Files"),
            &tr!("Select audio files from disk to batch convert into the destination folder"),
            theme,
            |ui| {
                ui.horizontal(|ui| {
                    let count = state.converter_input_files.len();
                    ui.label(
                        egui::RichText::new(format!("{count} file(s) queued"))
                            .font(crate::theme::font(12.0))
                            .color(if count > 0 {
                                theme.text_primary
                            } else {
                                theme.text_muted
                            }),
                    );
                    if ui.add(pill_btn("Add Files…", false, theme)).clicked() {
                        actions.pick_converter_files = true;
                    }
                    if count > 0 && ui.add(pill_btn("Clear", false, theme)).clicked() {
                        actions.clear_converter_queue = true;
                    }
                });
            },
        );

        setting_item(
            ui,
            &tr!("Batch Conversion Controls"),
            &tr!("Start background multi-threaded conversion worker or abort running job"),
            theme,
            |ui| {
                if extra.converter_busy {
                    ui.horizontal(|ui| {
                        if let Some((done, total, ref current)) = extra.converter_progress {
                            let frac = if total > 0 {
                                done as f32 / total as f32
                            } else {
                                0.0
                            };
                            ui.add(
                                egui::ProgressBar::new(frac)
                                    .text(format!("{done}/{total}: {current}"))
                                    .desired_width(180.0),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new(extra.converter_activity)
                                    .font(crate::theme::font(12.0))
                                    .color(theme.accent_warm),
                            );
                        }
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Cancel")
                                        .font(crate::theme::font(12.0))
                                        .strong()
                                        .color(Color32::WHITE),
                                )
                                .fill(theme.destructive)
                                .corner_radius(CornerRadius::same(theme.radius_md)),
                            )
                            .clicked()
                        {
                            actions.cancel_file_conversion = true;
                        }
                    });
                } else {
                    let can_start = !state.converter_input_files.is_empty();
                    if ui
                        .add_enabled(
                            can_start,
                            egui::Button::new(
                                egui::RichText::new("Start Conversion")
                                    .font(crate::theme::font(12.0))
                                    .strong()
                                    .color(if can_start {
                                        Color32::WHITE
                                    } else {
                                        theme.text_dim
                                    }),
                            )
                            .fill(if can_start {
                                theme.accent
                            } else {
                                theme.bg_pill
                            })
                            .corner_radius(CornerRadius::same(theme.radius_md)),
                        )
                        .clicked()
                    {
                        actions.start_file_conversion = true;
                    }

                    if let Some(ref msg) = state.converter_status_message {
                        ui.label(
                            egui::RichText::new(msg)
                                .font(crate::theme::font(11.5))
                                .color(theme.accent_highlight),
                        );
                    }
                }
            },
        );
    });
}
