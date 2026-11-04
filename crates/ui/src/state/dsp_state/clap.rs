//! CLAP rack summaries, actions, presets, and state capture.

use super::*;

impl DspState {
    /// Mirror the engine's published rack view into the settings card.
    ///
    /// Called once a frame by the app rather than read by the card itself: the
    /// summary is also wanted while the settings page is closed, and one place
    /// fetching it keeps the two settings surfaces (drawer and full page)
    /// describing the same plugin.
    pub fn refresh_clap_summary(&mut self, session: &AudioSession) {
        self.settings_tab.clap =
            crate::state::clap_rack::ClapCardSummary::from_view(&session.clap_view());
    }

    /// Dispatch the plugin rack's window actions.
    pub fn handle_clap_rack_actions(&mut self, session: &AudioSession, a: ClapRackActions) {
        if a.close {
            self.clap_rack.is_open = false;
        }
        if let Some(path) = a.probe_library {
            session.probe_clap_library(&path);
            self.clap_rack.probe_path = Some(path);
        }
        if let Some((path, id)) = a.load_plugin {
            session.load_clap_plugin(&path, Some(&id));
            self.clap_rack.last_library = path;
            self.clap_rack.notice = None;
        }
        if a.unload {
            session.unload_clap_plugin();
            self.clap_rack.announce("Plugin unloaded".to_string());
        }
        if let Some(enabled) = a.set_enabled {
            session.set_clap_plugin_enabled(enabled);
        }
        if let Some((id, value)) = a.set_param {
            session.set_clap_plugin_param(id, value);
        }
        if a.refresh {
            session.refresh_clap_plugin();
        }
        if a.open_gui {
            session.open_clap_gui();
        }
        if a.close_gui {
            session.close_clap_gui();
        }
        if let Some((width, height)) = a.resize_gui {
            session.resize_clap_gui(width.max(1), height.max(1));
        }

        if a.save_preset {
            let view = session.clap_view();
            if self.clap_rack.begin_save(&view) {
                if view.rack.has_state {
                    // The plugin's state only the engine can fetch. Arm the
                    // save and let `poll_clap_state_capture` complete it.
                    session.save_clap_state();
                } else {
                    self.clap_rack.save_without_state(&view);
                }
            }
        }

        if let Some(index) = a.apply_preset {
            self.apply_clap_preset(session, index);
        }
        if let Some(index) = a.delete_preset {
            if index < self.clap_rack.presets.len() {
                let removed = self.clap_rack.presets.remove(index);
                self.clap_rack.selected_preset = None;
                self.clap_rack.notice =
                    Some(format!("Deleted preset \u{201c}{}\u{201d}", removed.name));
            }
        }
    }

    /// Push a preset's values and state into the live plugin.
    ///
    /// A preset is only meaningful against the plugin it came from: parameter
    /// ids are the plugin's own numbering, so recalling one into a different
    /// plugin would set arbitrary controls.
    pub fn apply_clap_preset(&mut self, session: &AudioSession, index: usize) {
        let Some(preset) = self.clap_rack.presets.get(index).cloned() else {
            return;
        };
        let view = session.clap_view();

        if !view.rack.is_loaded() {
            self.clap_rack.announce(format!(
                "Load {} before recalling \u{201c}{}\u{201d}",
                if preset.plugin_name.is_empty() {
                    "the plugin"
                } else {
                    &preset.plugin_name
                },
                preset.name
            ));
            return;
        }

        let loaded_id = view
            .rack
            .plugin
            .as_ref()
            .map(|p| p.id.clone())
            .unwrap_or_default();
        if !preset.matches_plugin(&loaded_id) {
            self.clap_rack.announce(format!(
                "\u{201c}{}\u{201d} was captured from {}, not {}",
                preset.name,
                preset.plugin_name,
                view.rack.display_name()
            ));
            return;
        }

        // The plugin's own state first, then the explicit values: loading a
        // state blob may reset parameters, and the values the user set are the
        // ones that should win.
        let restored_state = match preset.state_bytes() {
            Some(bytes) => {
                session.load_clap_state(bytes);
                true
            }
            None => false,
        };
        for (id, value) in &preset.params {
            session.set_clap_plugin_param(*id, *value);
        }

        self.clap_rack.selected_preset = Some(index);
        self.clap_rack.announce(format!(
            "Recalled \u{201c}{}\u{201d}{}",
            preset.name,
            if restored_state {
                ""
            } else {
                " (parameter values only)"
            }
        ));
    }

    /// Complete a preset save once the engine hands over the plugin's state.
    ///
    /// The deadline matters: a plugin that refuses `state.save` leaves the save
    /// half-armed, and storing the parameter values alone is a better outcome
    /// than storing nothing at all.
    pub fn poll_clap_state_capture(&mut self, session: &AudioSession) {
        if self.clap_rack.pending_save.is_none() {
            // A blob nobody asked for must not be mistaken for the next save's.
            let _ = session.take_clap_state();
            return;
        }
        let view = session.clap_view();
        if let Some(bytes) = session.take_clap_state() {
            self.clap_rack.finish_save(&view, Some(bytes));
        } else if self
            .clap_rack
            .pending_save
            .as_ref()
            .is_some_and(|p| p.expired())
        {
            self.clap_rack.save_without_state(&view);
        }
    }
}
