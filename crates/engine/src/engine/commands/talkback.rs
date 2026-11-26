//! Talkback / duplex monitor commands.
//!
//! These are thin control-side handlers: the mixer and capture endpoint live
//! in [`super::super::talkback`](crate::engine::talkback) (private to the
//! engine) and the actual open/close policy in
//! `AudioEngine::sync_talkback`. The handlers only record the requested
//! state and ask the engine to reconcile it.
//!
//! A failed open is not a command failure — `EngineCommand` has no reply
//! channel. It is logged and published as `EngineSettings::talkback_error`
//! so a push-to-talk button can show why the microphone did not come up.

use super::AudioEngine;

impl AudioEngine {
    /// Apply a complete talkback configuration (device and/or gain may be
    /// changed while the monitor is live).
    pub(super) fn handle_set_talkback(&mut self, config: config::TalkbackConfig) {
        self.config.talkback = config;
        self.sync_talkback();
    }

    /// Push-to-talk key-down/key-up without resetting the selected device or
    /// gain.
    pub(super) fn handle_set_talkback_enabled(&mut self, enabled: bool) {
        self.config.talkback.enabled = enabled;
        self.sync_talkback();
    }
}
