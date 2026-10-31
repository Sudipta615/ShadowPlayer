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

    /// Select how DSD sources reach the DAC. Applies on the next stream open.
    pub fn set_dsd_output(&self, output: config::DsdOutput) {
        self.send(EngineCommand::SetDsdOutput(output));
    }

    /// Select the DSD→PCM decimation filter. Applies on the next DSD stream.
    pub fn set_dsd_noise_filter(&self, filter: config::DsdNoiseFilter) {
        self.send(EngineCommand::SetDsdNoiseFilter(filter));
    }

    /// Select the output-domain ultrasonic low-pass. Applies live.
    pub fn set_ultrasonic_filter(&self, filter: config::UltrasonicFilter) {
        self.send(EngineCommand::SetUltrasonicFilter(filter));
    }

    /// Select the polyphase resampler filter family. Rebuilds active
    /// resamplers on the control path.
    pub fn set_resampler_filter(&self, filter: config::ResamplerFilter) {
        self.send(EngineCommand::SetResamplerFilter(filter));
    }

    /// Select the polyphase low-pass cutoff ratio. Rebuilds active resamplers
    /// on the control path; the FFT families keep their fixed design.
    pub fn set_resampler_cutoff_ratio(&self, ratio: f32) {
        self.send(EngineCommand::SetResamplerCutoffRatio(ratio));
    }

    /// Replace the sample-rate whitelist used by the next rate selection.
    pub fn set_allowed_sample_rates(&self, rates: Vec<u32>) {
        self.send(EngineCommand::SetAllowedSampleRates(rates));
    }

    /// Set the DAC warmup silence inserted at the next stream open.
    pub fn set_dac_warmup_ms(&self, ms: u32) {
        self.send(EngineCommand::SetDacWarmupMs(ms));
    }

    /// Select the final quantization noise shaper. Applies on the next stream.
    pub fn set_noise_shaper(&self, shaper: config::NoiseShaper) {
        self.send(EngineCommand::SetNoiseShaper(shaper));
    }

    /// Configure PCM→SDM native-DSD output. Applies on the next stream open.
    pub fn set_sdm_config(&self, cfg: config::SdmConfig) {
        self.send(EngineCommand::SetSdmConfig(cfg));
    }

    /// Replace the device/rate-keyed DAC correction profiles. Applies live.
    pub fn set_dac_correction(&self, cfg: config::DacCorrectionConfig) {
        self.send(EngineCommand::SetDacCorrection(cfg));
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
