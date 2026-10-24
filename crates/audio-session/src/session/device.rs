//! Output device, backend, endpoint, and capture control.
//!
//! These calls report or request hardware state. They do not claim success:
//! the negotiated truth arrives through the engine's output events.

use super::AudioSession;
use engine::EngineCommand;

impl AudioSession {
    /// Select the output device (`None` = system default).
    ///
    /// Returns `false` when the engine did not receive the command.
    pub fn set_output_device(&self, device: Option<String>) -> bool {
        self.send(EngineCommand::SetOutputDevice(device))
    }

    pub fn available_devices(&self) -> Vec<String> {
        self.handle.available_devices()
    }

    /// The output devices a given backend can open.
    ///
    /// Each backend enumerates its own devices, so the list is the one the
    /// backend will actually open — an endpoint id for WASAPI, a device UID
    /// for CoreAudio, a driver's CLSID for ASIO.
    pub fn available_devices_for(
        &self,
        backend: config::AudioBackend,
    ) -> Vec<engine::output::devices::OutputDeviceInfo> {
        self.handle.available_devices_for(backend)
    }

    /// Returns `false` when the engine did not receive the command.
    pub fn set_output_backend(&self, backend: config::AudioBackend) -> bool {
        self.send(EngineCommand::SetOutputBackend(backend))
    }

    pub fn set_sample_rate_policy(&self, policy: config::SampleRatePolicy) {
        self.send(EngineCommand::SetSampleRatePolicy(policy));
    }

    pub fn set_volume_mode(&self, mode: config::VolumeMode) {
        self.send(EngineCommand::SetVolumeMode(mode));
    }

    pub fn set_bit_perfect(&self, enabled: bool) {
        self.send(EngineCommand::SetBitPerfect(enabled));
    }

    pub fn set_dither_enabled(&self, enabled: bool) {
        self.send(EngineCommand::SetDitherEnabled(enabled));
    }

    pub fn set_resampler_quality(&self, quality: config::ResamplerQuality) {
        self.handle.set_resampler_quality(quality);
    }

    pub fn set_precision_mode(&self, mode: engine::dsp::pipeline::PrecisionMode) {
        self.send(EngineCommand::SetPrecisionMode(mode));
    }

    /// Round-trip output latency reported by the backend, in milliseconds.
    pub fn latency_ms(&self) -> f32 {
        self.handle.latency_ms()
    }

    /// Mean DSP graph-rebuild cost (ms) and the number of rebuilds since startup.
    ///
    /// Each rebuild also allocates and re-arms buffers, so this is the figure to
    /// watch when a settings change causes audible gaps.
    pub fn graph_build_stats(&self) -> (f64, u64) {
        self.handle.graph_build_stats()
    }

    /// Ask the engine to report its input devices via `EngineEvent::InputDeviceList`.
    pub fn enumerate_input_devices(&self) {
        self.handle.enumerate_input_devices();
    }

    // ── Output & Device (Advanced) ────────────────────────────────────────

    /// Replace the full list of physical output endpoints.
    pub fn set_endpoints(&self, endpoints: Vec<config::EndpointConfig>) {
        self.handle.set_endpoints(endpoints);
    }

    /// Add or update a single output endpoint by ID.
    pub fn upsert_endpoint(&self, endpoint: config::EndpointConfig) {
        self.handle.set_endpoint(endpoint);
    }

    /// Remove an output endpoint by its stable identifier.
    pub fn remove_endpoint(&self, id: impl Into<String>) {
        self.handle.remove_endpoint(id);
    }

    /// Set fallback policy for exclusive-mode failures (Strict vs Allow).
    pub fn set_fallback_policy(&self, policy: config::FallbackPolicy) {
        self.send(EngineCommand::SetFallbackPolicy(policy));
    }

    /// Install a per-output profile and apply it to the active device.
    pub fn set_output_profile(&self, profile: engine::output::OutputProfile) {
        self.send(EngineCommand::SetOutputProfile(profile));
    }

    /// Remove the explicit output profile and resume auto-selection.
    pub fn clear_output_profile(&self) {
        self.send(EngineCommand::ClearOutputProfile);
    }

    /// Open the active ASIO driver's manufacturer settings dialog.
    /// No-op when the current backend is not ASIO or `asio-native` is not compiled in.
    pub fn open_asio_control_panel(&self) {
        self.handle.open_asio_control_panel();
    }

    // ── System Capture ────────────────────────────────────────────────────

    /// Start capturing the system mix (WASAPI loopback on Windows) to a WAV file.
    /// `path`: output path (`None` → `capture.wav`). `device`: render endpoint (`None` = default).
    /// No-op on platforms without `wasapi-native`.
    pub fn start_capture(&self, path: Option<std::path::PathBuf>, device: Option<String>) {
        self.handle.start_capture(path, device);
    }

    /// Stop the active system-audio capture and finalize its WAV file.
    pub fn stop_capture(&self) {
        self.handle.stop_capture();
    }

    // ── Stream Recovery ───────────────────────────────────────────────────

    /// Request manual stream recovery after a device disconnection or error.
    pub fn recover_stream(&self) {
        self.send(EngineCommand::RecoverStream);
    }
}
