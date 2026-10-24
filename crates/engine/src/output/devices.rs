//! The output devices this build can actually open.
//!
//! The device list a player shows must come from the backend that will open
//! it. A list built from cpal is wrong for every native backend: on Windows,
//! cpal's WASAPI backend opens every stream in *shared* mode, so a device
//! chosen from cpal's list is not the device the exclusive backend will open,
//! and the ASIO drivers — which are not devices at all, but installed
//! drivers — are missing from it entirely.
//!
//! So each backend enumerates its own devices, and the picker asks the backend
//! it is going to use. A backend that cannot answer returns an empty list
//! rather than a list borrowed from somewhere else, because an empty list is
//! honest and a borrowed one is a lie the user finds out about at playback
//! time.

use config::AudioBackend;

/// One output device the engine can open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputDeviceInfo {
    /// The backend this device belongs to.
    pub backend: AudioBackend,
    /// The stable identifier the backend selects the device by.
    ///
    /// This is what must be handed back to the backend to open *this* device.
    /// It is not always the friendly name: an endpoint id, a device UID and an
    /// ASIO driver's CLSID are all stable where a display name is not.
    pub id: String,
    /// The name to show the user.
    pub name: String,
    /// Whether this is the backend's default device.
    pub is_default: bool,
}

/// The output devices available for `backend`.
///
/// The list is empty when the backend has no devices to offer, or when the
/// platform's APIs could not be reached. It is never a list borrowed from
/// another backend.
pub fn available_devices(backend: AudioBackend) -> Vec<OutputDeviceInfo> {
    match backend {
        #[cfg(all(target_os = "windows", feature = "wasapi-native"))]
        AudioBackend::ExclusiveWasapi => wasapi_devices(),

        #[cfg(all(target_os = "windows", feature = "asio-native"))]
        AudioBackend::ExclusiveAsio => asio_devices(),

        #[cfg(target_os = "macos")]
        AudioBackend::ExclusiveCoreAudioHog => coreaudio_devices(),

        // The null sink has no device: it is the hardware-free sink the
        // headless suites use. It must not borrow the machine's device list.
        AudioBackend::Null => Vec::new(),

        // `Auto` means "whatever the platform would open by default", and
        // that is the shared-mode cpal host. The exclusive backends above
        // are listed separately, so `Auto` lists the cpal devices.
        AudioBackend::Auto => cpal_devices(backend),

        _ => cpal_devices(backend),
    }
}

/// The devices a cpal-backed backend offers.
fn cpal_devices(backend: AudioBackend) -> Vec<OutputDeviceInfo> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let default_name = cpal::default_host()
        .default_output_device()
        .and_then(|d| d.description().ok().map(|desc| desc.name().to_string()));
    super::cpal_devices::enumerate_devices(backend)
        .into_iter()
        .map(|name| OutputDeviceInfo {
            backend,
            // The cpal backends are selected by name, so the name is the id.
            id: name.clone(),
            is_default: default_name.as_deref() == Some(name.as_str()),
            name,
        })
        .collect()
}

/// The active WASAPI render endpoints.
#[cfg(all(target_os = "windows", feature = "wasapi-native"))]
fn wasapi_devices() -> Vec<OutputDeviceInfo> {
    let endpoints = super::wasapi_output::enumerate_render_endpoints();
    let default_id = super::wasapi_output::default_render_endpoint_id();
    endpoints
        .into_iter()
        .map(|(id, name)| OutputDeviceInfo {
            backend: AudioBackend::ExclusiveWasapi,
            is_default: default_id.as_deref() == Some(id.as_str()),
            id,
            name,
        })
        .collect()
}

/// The installed ASIO drivers.
#[cfg(all(target_os = "windows", feature = "asio-native"))]
fn asio_devices() -> Vec<OutputDeviceInfo> {
    super::asio_output::installed_drivers()
        .into_iter()
        .map(|(name, description, clsid)| OutputDeviceInfo {
            backend: AudioBackend::ExclusiveAsio,
            // The CLSID is what the driver is instantiated by; the name is
            // what the user recognises. The backend accepts either.
            id: clsid,
            name: if description.is_empty() {
                name
            } else {
                description
            },
            is_default: false,
        })
        .collect()
}

/// The macOS output devices.
#[cfg(target_os = "macos")]
fn coreaudio_devices() -> Vec<OutputDeviceInfo> {
    let devices = super::coreaudio_output::enumerate_output_devices();
    let default_uid = super::coreaudio_output::default_output_device_uid();
    devices
        .into_iter()
        .map(|(uid, name)| OutputDeviceInfo {
            backend: AudioBackend::ExclusiveCoreAudioHog,
            is_default: default_uid.as_deref() == Some(uid.as_str()),
            id: uid,
            name,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_backend_with_no_devices_returns_an_empty_list_rather_than_a_borrowed_one() {
        // `Null` has no devices, and the list must be empty — not the cpal
        // list of the machine the test happens to run on.
        assert!(available_devices(AudioBackend::Null).is_empty());
    }

    #[test]
    fn every_device_knows_which_backend_it_belongs_to() {
        for backend in [
            AudioBackend::Auto,
            AudioBackend::ExclusiveAlsa,
            AudioBackend::Null,
        ] {
            for device in available_devices(backend) {
                assert_eq!(device.backend, backend);
                assert!(!device.id.is_empty(), "a device must be selectable");
                assert!(!device.name.is_empty(), "a device must be nameable");
            }
        }
    }

    #[test]
    fn the_default_device_is_the_only_one_marked_default() {
        for backend in [
            AudioBackend::Auto,
            AudioBackend::ExclusiveAlsa,
            AudioBackend::Null,
        ] {
            let devices = available_devices(backend);
            let defaults = devices.iter().filter(|d| d.is_default).count();
            assert!(defaults <= 1, "at most one default device per backend");
        }
    }
}
