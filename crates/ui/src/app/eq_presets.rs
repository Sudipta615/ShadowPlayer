//! EQ preset library actions: load, save, apply, delete, bind.
//!
//! Everything here runs on the UI thread but performs only bounded SQLite
//! work (a handful of rows). The heavy lifting — file import, parsing — lives
//! in `PresetIoService`. A preset application pushes the curve with the
//! targeted `push_imported_eq`, never by reopening the output device.

use super::*;
use crate::components::eq_panel::{PresetBindRequest, PresetSaveRequest};
use crate::state::{StoredCurve, StoredPreset};

impl ShadowPlayerApp {
    /// Read every stored preset and hand the decoded rows to the panel.
    pub(super) fn reload_eq_presets(&mut self) {
        match self.eq_presets_repo.list() {
            Ok(records) => {
                self.dsp_state
                    .eq_panel
                    .library
                    .replace_from_records(&records);
            }
            Err(e) => {
                log::warn!("Could not load the EQ preset library: {e}");
                self.dsp_state.eq_panel.library.loaded = true;
                self.dsp_state.eq_panel.library.error = Some(tr_args!(
                    "EQ preset error: {error}",
                    "error" => e.to_string()
                ));
            }
        }
        self.refresh_track_preset_binding();
    }

    /// Save the curve currently in the panel as a new library preset.
    pub(super) fn save_eq_preset(&mut self, request: PresetSaveRequest) {
        let curve = StoredCurve::capture(&self.dsp_state.eq_panel);
        let json = match curve.to_json() {
            Ok(json) => json,
            Err(e) => {
                self.toasts.warning(tr_args!(
                    "EQ preset error: {error}",
                    "error" => e
                ));
                return;
            }
        };
        match self.eq_presets_repo.insert(
            &request.name,
            database::SOURCE_USER,
            &json,
            request.output_pattern.as_deref(),
            request.is_default,
        ) {
            Ok(record) => {
                self.dsp_state.eq_panel.library.saving = false;
                self.dsp_state.eq_panel.library.save_name.clear();
                self.dsp_state.eq_panel.library.save_pattern.clear();
                self.dsp_state.eq_panel.library.save_default = false;
                self.reload_eq_presets();
                self.toasts.success(tr_args!(
                    "Saved the preset “{name}”",
                    "name" => record.name
                ));
            }
            Err(e) => {
                log::warn!("Could not save an EQ preset: {e}");
                self.toasts.warning(tr_args!(
                    "EQ preset error: {error}",
                    "error" => e.to_string()
                ));
            }
        }
    }

    /// Install a stored preset and push it to the engine.
    pub(super) fn apply_eq_preset(&mut self, id: &str) {
        let record = match self.eq_presets_repo.get(id) {
            Ok(Some(record)) => record,
            Ok(None) => {
                self.reload_eq_presets();
                return;
            }
            Err(e) => {
                log::warn!("Could not read EQ preset {id}: {e}");
                self.toasts.warning(tr_args!(
                    "EQ preset error: {error}",
                    "error" => e.to_string()
                ));
                return;
            }
        };
        match StoredPreset::from_record(&record) {
            Ok(preset) => {
                self.install_stored_preset(&preset);
                self.toasts.success(tr_args!(
                    "Applied the preset “{name}”",
                    "name" => preset.name
                ));
            }
            Err(e) => {
                self.toasts.warning(tr_args!(
                    "EQ preset error: {error}",
                    "error" => e
                ));
            }
        }
    }

    pub(super) fn delete_eq_preset(&mut self, id: &str) {
        let name = self
            .dsp_state
            .eq_panel
            .library
            .presets
            .iter()
            .find(|preset| preset.id == id)
            .map(|preset| preset.name.clone())
            .unwrap_or_else(|| tr!("EQ preset"));
        match self.eq_presets_repo.delete(id) {
            Ok(true) => {
                if self.dsp_state.eq_panel.library.binding_edit.as_deref() == Some(id) {
                    self.dsp_state.eq_panel.library.binding_edit = None;
                }
                self.reload_eq_presets();
                self.toasts.success(tr_args!(
                    "Deleted the preset “{name}”",
                    "name" => name
                ));
            }
            Ok(false) => self.reload_eq_presets(),
            Err(e) => {
                log::warn!("Could not delete EQ preset {id}: {e}");
                self.toasts.warning(tr_args!(
                    "EQ preset error: {error}",
                    "error" => e.to_string()
                ));
            }
        }
    }

    pub(super) fn set_default_eq_preset(&mut self, id: &str) {
        match self.eq_presets_repo.set_default(id) {
            Ok(()) => self.reload_eq_presets(),
            Err(e) => {
                log::warn!("Could not set the default EQ preset: {e}");
                self.toasts.warning(tr_args!(
                    "EQ preset error: {error}",
                    "error" => e.to_string()
                ));
            }
        }
    }

    /// Change (or clear) the output-device pattern of a preset.
    pub(super) fn bind_eq_preset(&mut self, request: PresetBindRequest) {
        let mut record = match self.eq_presets_repo.get(&request.id) {
            Ok(Some(record)) => record,
            Ok(None) => {
                self.reload_eq_presets();
                return;
            }
            Err(e) => {
                log::warn!("Could not read EQ preset {}: {e}", request.id);
                self.toasts.warning(tr_args!(
                    "EQ preset error: {error}",
                    "error" => e.to_string()
                ));
                return;
            }
        };
        record.output_pattern = request
            .output_pattern
            .map(|pattern| pattern.trim().to_string())
            .filter(|pattern| !pattern.is_empty());

        match self.eq_presets_repo.update(&record) {
            Ok(()) => {
                self.dsp_state.eq_panel.library.binding_edit = None;
                self.reload_eq_presets();
                self.toasts.success(tr_args!(
                    "Updated the device binding for “{name}”",
                    "name" => record.name
                ));
            }
            Err(e) => {
                log::warn!("Could not bind EQ preset {}: {e}", request.id);
                self.toasts.warning(tr_args!(
                    "EQ preset error: {error}",
                    "error" => e.to_string()
                ));
            }
        }
    }

    /// Mirror the playing track's preset pointer into the library view.
    ///
    /// Called when the track changes and after a bind/unbind. The database is
    /// authoritative; this is only what the panel renders.
    pub(super) fn refresh_track_preset_binding(&mut self) {
        let Some(track_id) = self.coordinator.current_track().map(|track| track.id) else {
            self.dsp_state.eq_panel.library.track_binding = None;
            return;
        };
        let binding = match self.eq_presets_repo.preset_for_track(&track_id.to_string()) {
            Ok(record) => record.map(|record| record.id),
            Err(e) => {
                log::warn!("Could not read the track's EQ preset binding: {e}");
                None
            }
        };
        self.dsp_state.eq_panel.library.track_binding = binding;
    }

    /// Point the playing track at a library preset.
    pub(super) fn bind_eq_preset_to_track(&mut self, id: &str) {
        let Some(track_id) = self.coordinator.current_track().map(|track| track.id) else {
            self.toasts.warning(tr!("No track is playing"));
            return;
        };
        let name = self
            .dsp_state
            .eq_panel
            .library
            .presets
            .iter()
            .find(|preset| preset.id == id)
            .map(|preset| preset.name.clone())
            .unwrap_or_else(|| tr!("EQ preset"));

        match self.eq_presets_repo.bind_track(&track_id.to_string(), id) {
            Ok(()) => {
                self.refresh_track_preset_binding();
                // Make the pointer audible now when the track is not also
                // carrying a full profile (a profile outranks the pointer).
                let profile_active = matches!(
                    self.track_dsp_repo.get(&track_id),
                    Ok(Some(profile)) if !profile.is_neutral()
                );
                if !profile_active {
                    if let Ok(Some(record)) = self.eq_presets_repo.get(id) {
                        if let Ok(preset) = StoredPreset::from_record(&record) {
                            self.install_stored_preset(&preset);
                        }
                    }
                }
                self.toasts.success(tr_args!(
                    "Bound “{name}” to this track",
                    "name" => name
                ));
            }
            Err(e) => {
                log::warn!("Could not bind EQ preset {id} to {track_id}: {e}");
                self.toasts.warning(tr_args!(
                    "EQ preset error: {error}",
                    "error" => e.to_string()
                ));
            }
        }
    }

    /// Remove the playing track's preset pointer.
    pub(super) fn unbind_eq_preset_from_track(&mut self) {
        let Some(track_id) = self.coordinator.current_track().map(|track| track.id) else {
            self.toasts.warning(tr!("No track is playing"));
            return;
        };
        match self.eq_presets_repo.unbind_track(&track_id.to_string()) {
            Ok(_) => {
                self.refresh_track_preset_binding();
                // Put the device/default curve back so the track does not
                // keep playing a preset the user just unbound.
                let device = self.dsp_state.settings_tab.output_device.clone();
                if !device.is_empty() {
                    self.apply_device_bound_preset(&device);
                }
                self.toasts
                    .success(tr!("Removed the preset binding from this track"));
            }
            Err(e) => {
                log::warn!("Could not clear the EQ preset binding for {track_id}: {e}");
                self.toasts.warning(tr_args!(
                    "EQ preset error: {error}",
                    "error" => e.to_string()
                ));
            }
        }
    }

    /// A device arrived or the selected output changed: apply the most
    /// specific preset bound to that name.    ///
    /// Precedence is deliberate. A per-track profile wins because it encodes
    /// an explicit decision for this track; a per-track preset pointer wins
    /// next; only then does the device binding apply. Pure Direct is a hard
    /// bypass and is never overridden by an automatic preset.
    pub(super) fn apply_device_bound_preset(&mut self, device: &str) {
        if self.dsp_state.is_pure_direct() {
            return;
        }
        if self.current_track_override_wins() {
            return;
        }
        let record = match self.eq_presets_repo.find_for_output(device) {
            Ok(Some(record)) => record,
            Ok(None) => {
                // No device pattern matched. A preset explicitly marked as
                // the default is the global fallback; without one, the
                // restored manual chain stays in place.
                match self.eq_presets_repo.default_preset() {
                    Ok(Some(record)) => record,
                    Ok(None) => return,
                    Err(e) => {
                        log::warn!("Could not read the default EQ preset: {e}");
                        return;
                    }
                }
            }
            Err(e) => {
                log::warn!("Could not resolve an EQ preset for {device}: {e}");
                return;
            }
        };
        match StoredPreset::from_record(&record) {
            Ok(preset) => {
                let bound_by_pattern = preset
                    .output_pattern
                    .as_deref()
                    .is_some_and(|pattern| device.to_lowercase().contains(&pattern.to_lowercase()));
                self.install_stored_preset(&preset);
                let message = if bound_by_pattern {
                    tr_args!(
                        "Applied the preset “{name}” for {device}",
                        "name" => preset.name,
                        "device" => device
                    )
                } else {
                    tr_args!("Applied the preset “{name}”", "name" => preset.name)
                };
                self.toasts.info(message);
            }
            Err(e) => {
                log::warn!("EQ preset {} could not be decoded: {e}", record.name);
            }
        }
    }

    fn install_stored_preset(&mut self, preset: &StoredPreset) {
        preset.curve.apply(&mut self.dsp_state.eq_panel);
        self.dsp_state.push_imported_eq(&self.session);
        self.settings_dirty = true;
    }

    /// True when the current track carries a full DSP profile or a preset
    /// pointer; either is more specific than a device binding.
    fn current_track_override_wins(&self) -> bool {
        let Some(track_id) = self.coordinator.current_track().map(|track| track.id) else {
            return false;
        };
        if matches!(
            self.track_dsp_repo.get(&track_id),
            Ok(Some(profile)) if !profile.is_neutral()
        ) {
            return true;
        }
        matches!(
            self.eq_presets_repo.preset_for_track(&track_id.to_string()),
            Ok(Some(_))
        )
    }
}
