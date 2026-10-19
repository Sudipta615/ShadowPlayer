//! Output command handlers — backend, device, sample rate policy, volume mode,
//! fallback policy, output profiles, hardware/software volume, ASIO control panel.

use log::{error, info, warn};

use super::super::AudioEngine;
use crate::dsp::pipeline::VolumePath;

impl AudioEngine {
    pub(super) fn handle_set_output_backend(&mut self, backend: config::AudioBackend) {
        if self.config.output_backend != backend {
            self.config.output_backend = backend;
            info!("Output backend set to {:?}, recovering stream...", backend);
            if let Err(e) = self.recover_output_stream() {
                error!("Failed to recover stream after backend change: {}", e);
            }
        }
    }

    #[cfg(feature = "audio-output")]
    pub(super) fn handle_set_endpoints(&mut self, configs: Vec<config::EndpointConfig>) {
        let previous = std::mem::replace(&mut self.endpoint_configs, configs.clone());
        if self.audio_output.is_some() {
            if let Err(e) = self.reopen_configured_endpoints() {
                self.endpoint_configs = previous;
                error!("Failed to apply endpoint configuration: {e}");
                return;
            }
        }
        self.write_playback_info(|info| {
            info.endpoint_dropped_frames = 0;
            info.endpoints.clear();
        });
    }

    #[cfg(feature = "audio-output")]
    pub(super) fn handle_upsert_endpoint(&mut self, endpoint: config::EndpointConfig) {
        let mut configs = self.endpoint_configs.clone();
        if let Some(existing) = configs.iter_mut().find(|item| item.id == endpoint.id) {
            *existing = endpoint;
        } else {
            configs.push(endpoint);
        }
        self.handle_set_endpoints(configs);
    }

    #[cfg(feature = "audio-output")]
    pub(super) fn handle_remove_endpoint(&mut self, id: String) {
        let configs = self
            .endpoint_configs
            .iter()
            .filter(|endpoint| endpoint.id != id)
            .cloned()
            .collect();
        self.handle_set_endpoints(configs);
    }

    pub(super) fn handle_set_output_device(&mut self, device: Option<String>) {
        if self.config.output_device != device {
            self.config.output_device = device.clone();
            info!("Output device set to {:?}, recovering stream...", device);
            if let Err(e) = self.recover_output_stream() {
                error!("Failed to recover stream after device change: {}", e);
            }
            #[cfg(feature = "audio-output")]
            self.emit_output_event(crate::events::OutputEvent::OutputDeviceChanged { device });
        }
    }

    pub(super) fn handle_set_sample_rate_policy(&mut self, policy: config::SampleRatePolicy) {
        info!("Sample rate policy set to: {}", policy.display_name());
        self.config.sample_rate_policy = policy.clone();
        if let Some(ref mut output) = self.audio_output {
            let caps = output.capabilities();
            let target = caps.best_rate_for(self.clock.source_sample_rate, &policy);
            if let Ok(actual) = output.reconfigure_sample_rate(target) {
                self.output_sample_rate = actual;
                self.graph.update_sample_rate(actual as f32);
            }
        }
    }

    pub(super) fn handle_set_volume_mode(&mut self, mode: config::VolumeMode) {
        info!("Volume mode set to {:?}", mode);
        self.config.volume_mode = mode;
        let has_hw = self
            .audio_output
            .as_ref()
            .is_some_and(|o| o.supports_hardware_volume());
        match mode {
            config::VolumeMode::HardwarePreferred | config::VolumeMode::HardwareOnly if has_hw => {
                self.graph.set_volume(1.0);
                info!("{mode:?} active: software pipeline set to unity gain (1.0).");
                self.write_playback_info(|pb| {
                    pb.volume_path = Some(VolumePath::Hardware);
                    pb.volume_error = None;
                });
            }
            config::VolumeMode::HardwarePreferred => {
                let message = "HardwarePreferred: endpoint volume unavailable for the active output; using software gain"
                    .to_string();
                warn!("{message}");
                let vol = self.playback_info.load().volume;
                self.graph.set_volume(vol);
                self.write_playback_info(|pb| {
                    pb.volume_error = Some(message.clone());
                    pb.volume_path = Some(VolumePath::Software);
                });
            }
            config::VolumeMode::HardwareOnly => {
                let message = "HardwareOnly: endpoint volume unavailable for the active output; software volume NOT applied"
                    .to_string();
                warn!("{message}");
                self.graph.set_volume(1.0);
                self.write_playback_info(|pb| {
                    pb.volume_error = Some(message.clone());
                    pb.volume_path = None;
                });
            }
            config::VolumeMode::SoftwareOnly | config::VolumeMode::SoftwareAllowed => {
                let vol = self.playback_info.load().volume;
                self.graph.set_volume(vol);
                self.write_playback_info(|pb| {
                    pb.volume_path = Some(VolumePath::Software);
                    pb.volume_error = None;
                });
            }
        }
    }

    pub(super) fn handle_set_fallback_policy(&mut self, policy: config::FallbackPolicy) {
        info!("Fallback policy set to {:?}", policy);
        self.config.fallback_policy = policy;
    }

    pub(super) fn handle_set_output_profile(&mut self, profile: crate::output::OutputProfile) {
        let backend_changed = profile
            .backend_preference
            .is_some_and(|b| Some(b) != self.profile_backend());
        self.output_profile = Some(profile.clone());
        self.config.output_backend = profile
            .backend_preference
            .unwrap_or(self.config.output_backend);
        if self.audio_output.is_some() && backend_changed {
            info!(
                "Output profile '{}': backend preference changed, recovering stream",
                profile.id
            );
            if let Err(e) = self.recover_output_stream() {
                error!("Stream recovery after profile change failed: {}", e);
            }
        } else {
            self.apply_output_profile(&profile);
        }
        info!("Output profile '{}' selected", profile.id);
    }

    pub(super) fn handle_clear_output_profile(&mut self) {
        self.output_profile = None;
        self.write_playback_info(|pb| pb.active_output_profile = None);
        info!("Output profile cleared");
    }

    pub(super) fn handle_open_asio_control_panel(&mut self) {
        if let Some(ref output) = self.audio_output {
            match output.open_control_panel() {
                Ok(()) => info!("ASIO control panel opened"),
                Err(e) => warn!("Could not open ASIO control panel: {e}"),
            }
        } else {
            warn!("OpenAsioControlPanel: no output device is active");
        }
    }
}
