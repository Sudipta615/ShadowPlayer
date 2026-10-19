#[cfg(target_os = "linux")]
pub mod alsa_output;
// Native Steinberg ASIO backend in pure Rust — bypasses cpal entirely.
// Gated behind the `asio-native` feature; hosts that don't need pro-audio
// ASIO don't pay the compile cost of the COM vtable, IASIO wrappers, and
// the `windows` crate's registry enumeration.
#[cfg(feature = "asio-native")]
pub mod asio_output;
pub mod calibration;
pub mod capabilities;
/// Portable capture: input devices everywhere via cpal, plus the Windows
/// system-mix loopback. This is the seam `CaptureStart` / `CaptureStartInput`
/// and `MeasureRoom` are written against.
pub mod capture;
#[cfg(target_os = "macos")]
pub mod coreaudio_output;
pub mod cpal_callbacks;
pub mod cpal_devices;
pub mod cpal_output;
pub mod device_match;
pub mod device_monitor;
pub mod drift;
pub mod endpoint;
pub mod format_converter;
// Hardware-free sink ("null" backend). Cross-platform; needs no OS device, so
// the headless suites can exercise the engine's output matrix end to end.
pub mod null_output;
// The directory `src/output/` holds the `output.rs` core trait + factory
// alongside the backend modules; the naming is intentional.
#[allow(clippy::module_inception)]
pub mod output;
pub mod output_info;
pub mod output_profile;
pub mod rate_policy;
pub mod recovery;
pub mod rt_thread;
pub mod wav_writer;
// Native WASAPI exclusive-mode backend (no cpal). Windows-only, opt-in via
// the `wasapi-native` feature.
#[cfg(all(target_os = "windows", feature = "wasapi-native"))]
pub mod wasapi_output;
// WASAPI loopback capture — records the system mix. Windows-only; same
// feature gate as the native WASAPI backend (it reuses its COM plumbing).
#[cfg(all(target_os = "windows", feature = "wasapi-native"))]
pub mod wasapi_loopback;
#[cfg(all(target_os = "windows", feature = "wasapi-native"))]
pub use wasapi_loopback::WasapiLoopbackCapture;

#[cfg(feature = "asio-native")]
pub use asio_output::AsioOutput;
pub use calibration::{CorrectionIr, OutputCalibration, TargetLayoutKind};
pub use capabilities::{OutputAccessMode, OutputCapabilities, OutputValidationError};
pub use cpal_output::{CpalOutput, OutputError, OutputVolume};
pub use device_match::{classify_device_name_match, DeviceNameMatch};
pub use device_monitor::{DeviceDelta, DeviceMonitor};
pub use drift::{DriftController, MAX_DRIFT_RATIO};
pub use endpoint::{
    EndpointConfig, EndpointId, EndpointRegistry, EndpointRing, EndpointStats, EndpointWorker,
    VirtualEndpoint,
};
pub use format_converter::{AudioFormatConverter, TargetFormat};
pub use null_output::NullOutput;
pub use output::{
    create_output, NativeDsdCapability, NativeDsdParams, Output, StreamErrorBatch,
    StreamErrorEvent, StreamErrorKind, StreamErrorState,
};

/// The output backends this build can actually try, in menu order.
///
/// Compile-time truth rather than a fixed list: a backend whose native
/// implementation is not compiled in, or which belongs to another operating
/// system, is not offered. Asking for one of those used to be possible from the
/// picker and silently degraded to shared cpal output — the exact opposite of
/// what "exclusive" promises. Which backend was *actually* negotiated is
/// reported separately by [`OutputInfo::actual_backend`].
pub fn available_backends() -> Vec<config::AudioBackend> {
    // `Auto` is always first: it is the only entry that is meaningful
    // everywhere.
    let mut backends = vec![config::AudioBackend::Auto];

    #[cfg(target_os = "linux")]
    {
        backends.push(config::AudioBackend::ExclusiveAlsa);
    }
    #[cfg(all(target_os = "windows", feature = "wasapi-native"))]
    {
        backends.push(config::AudioBackend::ExclusiveWasapi);
    }
    #[cfg(all(target_os = "windows", feature = "asio-native"))]
    {
        backends.push(config::AudioBackend::ExclusiveAsio);
    }
    #[cfg(target_os = "macos")]
    {
        backends.push(config::AudioBackend::ExclusiveCoreAudioHog);
    }

    let _ = &mut backends;
    backends
}

pub use output_info::OutputInfo;
pub use output_profile::{OutputProfile, OutputProfileLibrary};
pub use rate_policy::SampleRatePolicy;
pub use recovery::rescale_clock_frames;
pub use rt_thread::{
    promote_current_thread_to_realtime, promote_current_thread_to_realtime_silent,
};

#[cfg(test)]
mod available_backends_tests {
    use super::available_backends;
    use config::AudioBackend;

    /// The picker must never offer a backend this build cannot open, and `Auto`
    /// must always be offered.
    #[test]
    fn only_backends_this_build_can_open_are_listed() {
        let backends = available_backends();
        assert_eq!(backends.first(), Some(&AudioBackend::Auto));

        #[cfg(target_os = "linux")]
        assert!(backends.contains(&AudioBackend::ExclusiveAlsa));

        // Never offered: no implementation exists for them on any platform.
        assert!(!backends.contains(&AudioBackend::Null));
        // Declared for the settings format only; there is no PipeWire/JACK
        // integration in any configuration, so the picker must not offer them
        // even though selecting them is possible from a config file.
        assert!(!backends.contains(&AudioBackend::PipeWire));
        assert!(!backends.contains(&AudioBackend::Jack));
    }

    /// Backends gated on a native feature must not appear without it.
    #[cfg(not(all(target_os = "windows", feature = "wasapi-native")))]
    #[test]
    fn exclusive_wasapi_is_hidden_without_the_native_backend() {
        assert!(!available_backends().contains(&AudioBackend::ExclusiveWasapi));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn coreaudio_hog_is_hidden_off_macos() {
        assert!(!available_backends().contains(&AudioBackend::ExclusiveCoreAudioHog));
    }
}
