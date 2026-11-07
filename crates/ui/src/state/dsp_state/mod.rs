use crate::components::eq_panel::{EqPanelActions, EqPanelState};
use crate::components::settings_tab::{SettingsTabActions, SettingsTabState};
use crate::state::chain_preset::{self, ChainPreset};
use crate::state::clap_rack::{ClapRackActions, ClapRackState};
use audio_session::engine::output::devices::OutputDeviceInfo;
use audio_session::AudioSession;
use config::{AudioBackend, CrossfadeConfig, TransitionMode};

/// The stages Pure Direct neutralises, kept so leaving it restores the sound
/// the user had before the bypass instead of leaving every stage off.
#[derive(Clone, Copy)]
struct PureDirectSaved {
    eq_enabled: bool,
    eq_mode: config::EqMode,
    preamp_db: f32,
    speed: f32,
    volume: f32,
    limiter_enabled: bool,
    compressor_enabled: bool,
    spatial_enabled: bool,
    stereo_enhancer_enabled: bool,
}

/// Central DSP state container and command dispatcher
#[derive(Default)]
pub struct DspState {
    pub eq_panel: EqPanelState,
    pub settings_tab: SettingsTabState,
    pub autoeq: crate::modals::AutoEqModalState,
    /// Set while Pure Direct has neutralised the chain; see
    /// [`Self::enter_pure_direct`].
    pure_direct_saved: Option<PureDirectSaved>,
    /// The CLAP plugin rack: its window state and the named preset library.
    pub clap_rack: ClapRackState,
    /// The output devices of the selected backend.
    ///
    /// Cached, because enumerating devices is not a per-frame operation: on
    /// Windows it initialises COM and walks the endpoint collection, and on
    /// macOS it asks the HAL for the device list. The cache is refreshed when
    /// the backend changes and when the user asks for it.
    pub output_devices: Vec<OutputDeviceInfo>,
    /// The backend the cached device list belongs to.
    pub output_devices_backend: Option<AudioBackend>,
}

mod clap;
mod eq_actions;
mod settings_actions;
mod sync;

#[cfg(test)]
mod device_name_tests;
#[cfg(test)]
mod tests;
