use super::*;

impl ShadowPlayerApp {
    pub(super) fn render_modals(&mut self, ctx: &egui::Context, current_track: Option<&Track>) {
        // 10. Modals Execution
        self.render_dsp_and_tool_modals(ctx, current_track);
        self.render_editor_modals(ctx);
        self.render_history_modals(ctx);
        self.render_library_modals(ctx);
        self.render_autotag_modals(ctx);
        self.render_command_palette(ctx);
    }

    pub(super) fn render_dsp_and_tool_modals(
        &mut self,
        ctx: &egui::Context,
        current_track: Option<&Track>,
    ) {
        let mut eq_actions = show_eq_panel(ctx, &mut self.dsp_state.eq_panel, &self.theme);
        let want_import = eq_actions.import_curve;
        let want_export = eq_actions.export_curve;
        let preset_save = eq_actions.preset_save.take();
        let preset_apply = eq_actions.preset_apply.take();
        let preset_delete = eq_actions.preset_delete.take();
        let preset_default = eq_actions.preset_default.take();
        let preset_bind = eq_actions.preset_bind.take();
        let preset_bind_track = eq_actions.preset_bind_track.take();
        let preset_unbind_track = eq_actions.preset_unbind_track;
        self.dsp_state
            .handle_eq_panel_actions(&self.session, eq_actions);
        if want_import {
            self.preset_io.request_import();
        }
        if want_export {
            let (name, text) = self.dsp_state.eq_panel.export_curve_text();
            self.preset_io.request_export(name, text);
        }
        if let Some(request) = preset_save {
            self.save_eq_preset(request);
        }
        if let Some(id) = preset_apply {
            self.apply_eq_preset(&id);
        }
        if let Some(id) = preset_delete {
            self.delete_eq_preset(&id);
        }
        if let Some(id) = preset_default {
            self.set_default_eq_preset(&id);
        }
        if let Some(request) = preset_bind {
            self.bind_eq_preset(request);
        }
        if let Some(id) = preset_bind_track {
            self.bind_eq_preset_to_track(&id);
        }
        if preset_unbind_track {
            self.unbind_eq_preset_from_track();
        }
        if self.dsp_state.eq_panel.is_open {
            self.settings_dirty = true;
        }

        // The plugin rack. The view is a snapshot the engine publishes, so the
        // window never touches the audio thread.
        let clap_view = self.session.clap_view();
        let clap_actions =
            show_clap_rack_panel(ctx, &mut self.dsp_state.clap_rack, &clap_view, &self.theme);
        if clap_actions.choose_library {
            self.request_import(ImportKind::ClapLibrary);
        }
        self.dsp_state
            .handle_clap_rack_actions(&self.session, clap_actions);
        if self.dsp_state.clap_rack.is_open {
            self.settings_dirty = true;
        }

        // The room-correction measurement wizard. The engine plays the sweep
        // and publishes progress; the modal only renders the state the app
        // already drained from the coordinator.
        let measurement_actions =
            show_room_measurement_modal(ctx, &mut self.room_measurement, &self.theme);
        if measurement_actions.start {
            self.room_measurement.mark_started();
            self.coordinator.measure_room(
                self.room_measurement.seconds,
                self.room_measurement.pre_emphasis,
                self.room_measurement.requested_device(),
            );
        }
        if measurement_actions.cancel {
            self.room_measurement.stage = "Cancelling measurement…".to_string();
            self.coordinator.cancel_room_measurement();
        }
        if measurement_actions.restart {
            self.room_measurement.phase =
                crate::modals::room_measurement_modal::MeasurementPhase::Setup;
            self.room_measurement.stage.clear();
            self.room_measurement.result = None;
            self.room_measurement.error = None;
            self.room_measurement.needs_devices = true;
        }
        if measurement_actions.close {
            self.room_measurement.close();
        }
        if measurement_actions.save_ir {
            if let Some(result) = self.room_measurement.result.as_ref() {
                let source = result.path.clone();
                if let Some(dest) = rfd::FileDialog::new()
                    .set_title("Save Measurement WAV")
                    .set_file_name("room-measurement.wav")
                    .add_filter("WAV audio", &["wav"])
                    .save_file()
                {
                    match std::fs::copy(&source, &dest) {
                        Ok(_) => self
                            .toasts
                            .info(format!("Measurement saved to {}", dest.display())),
                        Err(e) => self
                            .toasts
                            .warning(format!("Could not save the measurement: {e}")),
                    }
                }
            }
        }
        if measurement_actions.load_ir {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Load Impulse Response")
                .add_filter("WAV impulse response", &["wav"])
                .pick_file()
            {
                self.session.load_correction_ir(&path);
                self.room_measurement.close();
                self.toasts
                    .info(format!("Loading correction from {}", path.display()));
            }
        }

        // The Tools workshop: CD extraction, transcode export and updates.
        let tool_actions = show_media_tools_modal(ctx, &mut self.media_tools, &self.theme);
        if tool_actions.close {
            self.media_tools.is_open = false;
        }
        if tool_actions.cancel {
            self.media_tools_service.cancel();
        }
        if tool_actions.pick_cd_output {
            if let Some(dir) = rfd::FileDialog::new()
                .set_title("Choose a folder for ripped tracks")
                .pick_folder()
            {
                self.media_tools.cd_output_dir = dir;
            }
        }
        if tool_actions.pick_export_files {
            if let Some(files) = rfd::FileDialog::new()
                .set_title("Choose audio files to export")
                .add_filter(
                    "Audio",
                    &[
                        "flac", "wav", "aif", "aiff", "mp3", "m4a", "aac", "ogg", "opus", "dsf",
                        "dff",
                    ],
                )
                .pick_files()
            {
                for file in files {
                    if !self.media_tools.export_inputs.contains(&file) {
                        self.media_tools.export_inputs.push(file);
                    }
                }
            }
        }
        if tool_actions.pick_export_output {
            if let Some(dir) = rfd::FileDialog::new()
                .set_title("Choose an export folder")
                .pick_folder()
            {
                self.media_tools.export_output_dir = dir;
            }
        }
        if tool_actions.clear_export_files {
            self.media_tools.export_inputs.clear();
            self.media_tools.export_message = None;
            self.media_tools.export_error = None;
        }
        if tool_actions.probe_disc {
            self.media_tools.cd_error = None;
            self.media_tools.cd_progress = None;
            self.media_tools_service.probe_disc(
                self.media_tools.cd_device.clone(),
                self.media_tools.cd_fetch_metadata,
            );
        }
        if tool_actions.rip_disc {
            self.media_tools.cd_error = None;
            self.media_tools.cd_progress = None;
            self.media_tools_service.rip_disc(
                self.media_tools.cd_device.clone(),
                self.media_tools.cd_output_dir.clone(),
                self.media_tools.cd_format,
                self.media_tools.selected_tracks(),
                self.media_tools.cd_fetch_metadata,
            );
        }
        if tool_actions.start_export {
            self.media_tools.export_error = None;
            self.media_tools.export_progress = None;
            self.media_tools_service.transcode_files(
                self.media_tools.export_inputs.clone(),
                self.media_tools.export_output_dir.clone(),
                self.media_tools.update_format(),
                self.media_tools.export_copy_tags,
                self.media_tools.export_overwrite,
            );
        }
        if tool_actions.pick_update_dir {
            if let Some(dir) = rfd::FileDialog::new()
                .set_title("Choose a folder for downloaded updates")
                .pick_folder()
            {
                self.media_tools.update_dir = dir;
            }
        }
        if let Some(enabled) = tool_actions.update_enabled_changed {
            self.dsp_state.settings_tab.update_check_enabled = enabled;
            self.settings_dirty = true;
        }
        if tool_actions.check_update {
            self.media_tools.update_error = None;
            self.media_tools.update_check = None;
            self.media_tools.update_download = None;
            self.media_tools_service.check_for_update(
                self.media_tools.update_enabled,
                env!("CARGO_PKG_VERSION").to_string(),
            );
        }
        if tool_actions.download_update {
            if let Some(check) = self.media_tools.update_check.clone() {
                if let Some(asset) = check.recommended_asset.clone() {
                    self.media_tools.update_error = None;
                    self.media_tools.update_download = None;
                    self.media_tools_service.download_update(
                        self.media_tools.update_enabled,
                        check.release,
                        asset,
                        self.media_tools.update_dir.clone(),
                        false,
                    );
                }
            }
        }
        if tool_actions.open_release_page {
            if let Some(check) = &self.media_tools.update_check {
                ctx.open_url(egui::OpenUrl::new_tab(check.release.page_url.clone()));
            }
        }

        if let Some(curve) = show_autoeq_modal(ctx, &mut self.dsp_state.autoeq, &self.theme) {
            self.dsp_state.eq_panel.apply_imported_curve(&curve);
            self.dsp_state.push_imported_eq(&self.session);
            self.settings_dirty = true;
        }

        show_bit_perfect_modal(
            ctx,
            &mut self.app_state.bit_perfect_modal_open,
            current_track,
            &self.session,
            &mut self.dsp_state,
            &self.theme,
        );

        show_track_properties_modal(
            ctx,
            self.app_state.track_properties_track.as_ref(),
            &mut self.app_state.track_properties_open,
            &self.theme,
        );
    }

    pub(super) fn render_library_modals(&mut self, ctx: &egui::Context) {
        // The library statistics.
        crate::modals::library_stats_modal::show_library_stats_modal(
            ctx,
            &mut self.app_state.library_stats_modal,
            &self.theme,
        );

        // The loudness inspector.
        crate::modals::loudness_modal::show_loudness_modal(
            ctx,
            &mut self.app_state.loudness_modal,
            self.coordinator.current_track(),
            &self.theme,
        );
    }
}
