use super::*;

/// The `SHADOWPLAYER_EVAL_CLAP` script's progress.
///
/// The engine answers on its own thread, so the script cannot be a straight
/// line: each step asserts on what the previous one asked for and then asks for
/// the next thing. Publishing the steps rather than sleeping keeps the eval
/// honest about what the UI actually observes. Development only: see the
/// `eval-tools` feature.
#[cfg(feature = "eval-tools")]
pub(super) struct ClapEval {
    mode: String,
    library: String,
    step: u8,
    /// The state blob the preset captured, for the log.
    saved_state: Option<Vec<u8>>,
    /// Whether the diagnostic dump has been printed yet.
    logged: bool,
}

impl ShadowPlayerApp {
    /// Probe a chosen CLAP library and, when it holds a single plugin, load it.
    ///
    /// The probe has to happen even in the single-plugin case: until the file
    /// has been read there is no id to load by, and a library that holds
    /// several plugins needs the user to pick one from the rack.
    pub(super) fn open_clap_library(&mut self, path: std::path::PathBuf) {
        let display = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        self.dsp_state.clap_rack.probe_path = Some(path.to_string_lossy().to_string());
        self.dsp_state.clap_rack.last_library = path.to_string_lossy().to_string();
        self.dsp_state.clap_rack.pending_autoload_done = false;
        self.dsp_state.clap_rack.is_open = true;
        self.dsp_state
            .clap_rack
            .announce(format!("Reading {display}…"));
        self.session.probe_clap_library(&path);
    }

    /// Load the plugin a probe turned up, once that probe has answered.
    ///
    /// A library holding exactly one plugin needs no decision from the user, so
    /// it loads on its own; several leave the choice to the rack, which lists
    /// them.
    pub(super) fn autoload_probed_clap_plugin(&mut self) {
        let Some(expected) = self.dsp_state.clap_rack.probe_path.clone() else {
            return;
        };
        let view = self.session.clap_view();
        let Some(library) = &view.library else {
            return;
        };
        if library.path != expected || view.rack.is_loaded() {
            return;
        }
        // Already handled this probe; wait for the user to choose.
        if self.dsp_state.clap_rack.pending_autoload_done {
            return;
        }
        if let Some(error) = &library.error {
            self.dsp_state
                .clap_rack
                .announce(format!("Could not read {expected}: {error}"));
            self.dsp_state.clap_rack.pending_autoload_done = true;
            return;
        }
        match library.plugins.len() {
            0 => {
                self.dsp_state.clap_rack.notice =
                    Some(format!("{expected} contains no CLAP plugins"));
                self.dsp_state.clap_rack.pending_autoload_done = true;
            }
            1 => {
                let id = library.plugins[0].id.clone();
                let name = library.plugins[0].name.clone();
                self.session.load_clap_plugin(&expected, Some(&id));
                self.dsp_state
                    .clap_rack
                    .announce(format!("Loading {name}…"));
                self.dsp_state.clap_rack.pending_autoload_done = true;
            }
            count => {
                self.dsp_state.clap_rack.notice =
                    Some(format!("{count} plugins found — choose one below"));
                self.dsp_state.clap_rack.pending_autoload_done = true;
            }
        }
    }

    /// Drive `SHADOWPLAYER_EVAL_CLAP` one step per frame.
    ///
    /// `=load` exercises probe, load, enumeration and parameter plumbing;
    /// `=preset` also captures a named preset and recalls it. Either way it goes
    /// through the same code the UI uses, so what it proves is what a user gets.
    ///
    /// The library defaults to the in-repo fixture plugin, which is the only
    /// CLAP plugin this machine has. `SHADOWPLAYER_EVAL_CLAP_LIB` names another.
    #[cfg(feature = "eval-tools")]
    pub(super) fn drive_clap_eval(&mut self) {
        if self.clap_eval.is_none() {
            let Ok(mode) = std::env::var("SHADOWPLAYER_EVAL_CLAP") else {
                return;
            };
            let library = std::env::var("SHADOWPLAYER_EVAL_CLAP_LIB").unwrap_or_else(|_| {
                format!(
                    "{}/../../target/debug/libclap_fixture_plugin.so",
                    env!("CARGO_MANIFEST_DIR")
                )
            });
            log::info!("CLAP EVAL: mode {mode}, library {library}");
            self.clap_eval = Some(ClapEval {
                mode,
                library,
                step: 0,
                saved_state: None,
                logged: false,
            });
        }

        let (mode, step, library) = {
            let eval = self.clap_eval.as_ref().expect("set just above");
            (eval.mode.clone(), eval.step, eval.library.clone())
        };
        let view = self.session.clap_view();

        match step {
            0 => {
                // A settings screenshot wants the card in its page, not a window
                // over it.
                self.dsp_state.clap_rack.is_open =
                    std::env::var("SHADOWPLAYER_EVAL_CLAP_NO_WINDOW").is_err();
                self.dsp_state.clap_rack.probe_path = Some(library.clone());
                // The app asks for a few refreshes at startup so a plugin
                // restored from config becomes visible. A refresh republishes
                // the plugin's own values, which would race this script's
                // parameter step, so the eval stops it.
                self.clap_refresh_frames = 0;
                self.session.probe_clap_library(&library);
                log::info!("CLAP EVAL [0] probing {library}");
                self.clap_eval_advance(1);
            }
            1 => {
                let Some(probe) = view.library.as_ref() else {
                    return;
                };
                if probe.path != library {
                    return;
                }
                assert!(
                    probe.error.is_none(),
                    "probe of {library} failed: {:?}",
                    probe.error
                );
                let listed: Vec<String> = probe
                    .plugins
                    .iter()
                    .map(|p| format!("{} [{}]", p.name, p.id))
                    .collect();
                log::info!(
                    "CLAP EVAL [1] {} plugin(s): {}",
                    probe.plugins.len(),
                    listed.join(", ")
                );
                assert_eq!(
                    probe.plugins.len(),
                    2,
                    "the fixture library holds two plugins"
                );
                assert!(
                    probe
                        .plugins
                        .iter()
                        .any(|p| p.id == "com.shadowplayer.fixture.gain"),
                    "the gain fixture is listed"
                );
                assert!(
                    probe
                        .plugins
                        .iter()
                        .any(|p| p.id == "com.shadowplayer.fixture.trim"),
                    "the trim fixture is listed"
                );
                self.dsp_state.clap_rack.notice = Some(format!("Found {}", listed.join(", ")));
                self.session
                    .load_clap_plugin(&library, Some("com.shadowplayer.fixture.gain"));
                self.clap_eval_advance(2);
            }
            2 => {
                if !view.rack.is_loaded() {
                    return;
                }
                let plugin = view.rack.plugin.as_ref().expect("loaded implies a plugin");
                assert_eq!(plugin.id, "com.shadowplayer.fixture.gain");
                assert_eq!(plugin.name, "Fixture Gain");
                assert_eq!(plugin.vendor, "ShadowPlayer");
                log::info!(
                    "CLAP EVAL [2] loaded {} v{} by {}",
                    plugin.name,
                    plugin.version,
                    plugin.vendor
                );
                for param in &view.rack.params {
                    log::info!(
                        "CLAP EVAL [2]   #{} {}/{} = {} ({:?}) in {}..{} stepped={} bypass={}",
                        param.id,
                        param.module,
                        param.name,
                        param.value,
                        param.value_text,
                        param.min,
                        param.max,
                        param.is_stepped,
                        param.is_bypass
                    );
                }
                assert_eq!(
                    view.rack.params.len(),
                    4,
                    "the fixture Gain declares four parameters"
                );
                let by_name = |name: &str| view.rack.params.iter().find(|p| p.name == name);

                // An enumeration renders as its label, not its number: the
                // plugin's `value_to_text` is what a native editor would show.
                let mode_param = by_name("Mode").expect("a Mode parameter");
                assert_eq!(mode_param.value_text, "Clean");
                assert!(mode_param.is_stepped);
                assert_eq!(mode_param.min, 0.0);
                assert_eq!(mode_param.max, 2.0);

                let bypass = by_name("Bypass").expect("a Bypass parameter");
                assert!(bypass.is_bypass, "the fixture flags its bypass parameter");
                assert_eq!(bypass.value_text, "Off");
                assert!(view.rack.has_state, "the fixture implements state");

                let gain = by_name("Gain").expect("a Gain parameter");
                assert_eq!(gain.min, -24.0);
                assert_eq!(gain.max, 24.0);
                assert!((gain.value).abs() < 1e-9, "the default is unity");
                assert_eq!(gain.value_text, "0.00");

                self.session.set_clap_plugin_param(gain.id, 6.0);
                log::info!("CLAP EVAL [2] set Gain to +6.00 dB");
                self.clap_eval_advance(3);
            }
            3 => {
                let Some(gain) = view.rack.params.iter().find(|p| p.name == "Gain") else {
                    return;
                };
                if (gain.value - 6.0).abs() > 1e-9 {
                    return;
                }
                log::info!("CLAP EVAL [3] host reports Gain = {}", gain.value);

                if mode != "preset" {
                    log::info!("CLAP EVAL OK: probe, load, enumerate and drive a parameter");
                    self.clap_eval_advance(9);
                    return;
                }

                {
                    let rack = &mut self.dsp_state.clap_rack;
                    rack.preset_name = "Eval 6dB".to_string();
                    rack.preset_note = "captured by the eval hook".to_string();
                }
                self.dsp_state.handle_clap_rack_actions(
                    &self.session,
                    crate::state::clap_rack::ClapRackActions {
                        save_preset: true,
                        ..Default::default()
                    },
                );
                log::info!("CLAP EVAL [3] asked for a preset save");
                self.clap_eval_advance(4);
            }
            4 => {
                // The save completes on the frame the engine's state blob
                // arrives; until then there is nothing to assert.
                if self.dsp_state.clap_rack.pending_save.is_some() {
                    return;
                }
                let preset = self
                    .dsp_state
                    .clap_rack
                    .presets
                    .first()
                    .cloned()
                    .expect("the preset was saved");
                let bytes = preset
                    .state_bytes()
                    .expect("the plugin returned a state blob");
                assert!(!bytes.is_empty(), "an empty blob is not state");
                log::info!(
                    "CLAP EVAL [4] preset {:?} captured {} parameter values and {} state bytes",
                    preset.name,
                    preset.params.len(),
                    bytes.len()
                );
                assert_eq!(preset.params.len(), 4, "every parameter is captured");
                assert_eq!(preset.plugin_id, "com.shadowplayer.fixture.gain");
                if let Some(eval) = self.clap_eval.as_mut() {
                    eval.saved_state = Some(bytes);
                }

                // Move it, so the recall has something to undo.
                self.session.set_clap_plugin_param(0, -12.0);
                log::info!("CLAP EVAL [4] moved Gain to -12.00 dB");
                self.clap_eval_advance(5);
            }
            5 => {
                let Some(gain) = view.rack.params.iter().find(|p| p.name == "Gain") else {
                    return;
                };
                if (gain.value + 12.0).abs() > 1e-9 {
                    return;
                }
                log::info!("CLAP EVAL [5] host reports Gain = {}", gain.value);
                self.dsp_state.handle_clap_rack_actions(
                    &self.session,
                    crate::state::clap_rack::ClapRackActions {
                        apply_preset: Some(0),
                        ..Default::default()
                    },
                );
                log::info!("CLAP EVAL [5] recalled the preset");
                self.clap_eval_advance(6);
            }
            6 => {
                let Some(gain) = view.rack.params.iter().find(|p| p.name == "Gain") else {
                    return;
                };
                if (gain.value - 6.0).abs() > 1e-9 {
                    return;
                }
                assert!(
                    view.rack.error.is_none(),
                    "the recall reported an error: {:?}",
                    view.rack.error
                );
                log::info!(
                    "CLAP EVAL [6] recall restored Gain = {} ({:?})",
                    gain.value,
                    gain.value_text
                );

                // Re-read from the plugin rather than trusting the host's own
                // cache, so the next step proves the plugin accepted the blob.
                self.session.refresh_clap_plugin();
                self.clap_eval_advance(7);
            }
            7 => {
                if !self.clap_eval.as_ref().is_some_and(|e| e.logged) {
                    let dump: Vec<(String, f64, String)> = view
                        .rack
                        .params
                        .iter()
                        .map(|p| (p.name.clone(), p.value, p.value_text.clone()))
                        .collect();
                    log::info!(
                        "CLAP EVAL [7] plugin reports, after re-reading: plugin={:?} params={dump:?} error={:?}",
                        view.rack.plugin.as_ref().map(|p| p.name.clone()),
                        view.rack.error
                    );
                    if let Some(eval) = self.clap_eval.as_mut() {
                        eval.logged = true;
                    }
                }
                let Some(gain) = view.rack.params.iter().find(|p| p.name == "Gain") else {
                    return;
                };
                if (gain.value - 6.0).abs() > 1e-9 {
                    return;
                }
                let state_len = self
                    .clap_eval
                    .as_ref()
                    .and_then(|e| e.saved_state.as_ref())
                    .map(|b| b.len())
                    .unwrap_or(0);
                log::info!(
                    "CLAP EVAL OK: probe, load, enumerate, drive, capture ({state_len} bytes) and recall"
                );
                self.dsp_state.clap_rack.notice = Some(format!(
                    "Eval: recalled “Eval 6dB” — {state_len} bytes of state, Gain back at {} ({:?})",
                    gain.value, gain.value_text
                ));
                self.clap_eval_advance(9);
            }
            _ => {}
        }
    }

    /// Move the eval script along. A separate method so the steps above read as
    /// a sequence rather than as borrow-juggling.
    #[cfg(feature = "eval-tools")]
    pub(super) fn clap_eval_advance(&mut self, step: u8) {
        if let Some(eval) = self.clap_eval.as_mut() {
            eval.step = step;
        }
    }
}
