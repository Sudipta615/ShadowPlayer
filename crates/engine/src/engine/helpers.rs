//! Helper methods for AudioEngine — utility functions for playback info,
//! state management, event emission, and URI decoding.

use std::sync::Arc;

use super::AudioEngine;
use crate::buffer::{PlaybackInfo, PlaybackState};
use crate::events::EngineEvent;

impl AudioEngine {
    /// Read just the current PlaybackState without cloning the entire
    /// PlaybackInfo struct.
    pub fn current_state(&self) -> PlaybackState {
        self.playback_info.load().state
    }

    /// Emit an asynchronous discrete engine event to all subscribed handles.
    pub(crate) fn emit_event(&self, event: EngineEvent) {
        let _ = self.event_tx.try_send(event);
    }

    /// Emit an output device event on the dedicated output event channel.
    /// Only compiled when the `audio-output` feature is enabled.
    #[cfg(feature = "audio-output")]
    pub(crate) fn emit_output_event(&self, event: crate::events::OutputEvent) {
        let _ = self.output_event_tx.try_send(event);
    }

    /// Update playback state and publish an event if the state changed.
    pub(crate) fn update_playback_state(&self, state: PlaybackState) {
        let prev_state = self.current_state();
        if prev_state != state {
            match state {
                PlaybackState::Playing => {
                    if let Some(ref output) = self.audio_output {
                        output.resume();
                    }
                    self.emit_event(EngineEvent::PlaybackStarted);
                }
                PlaybackState::Paused => {
                    if let Some(ref output) = self.audio_output {
                        output.pause();
                    }
                    self.emit_event(EngineEvent::PlaybackPaused);
                }
                PlaybackState::Stopped => {
                    if let Some(ref output) = self.audio_output {
                        output.pause();
                    }
                    self.emit_event(EngineEvent::PlaybackStopped);
                }
                PlaybackState::Buffering => {}
            }
        }
        self.playback_info.rcu(|old| {
            Arc::new(PlaybackInfo {
                state,
                ..old.as_ref().clone()
            })
        });
    }

    /// Build the PCM→SDM telemetry snapshot from the live stage.
    ///
    /// Reads the modulator counters and the negotiated plan; never invents a
    /// number when the stage is inactive. Control path only.
    pub(super) fn sdm_report(&self) -> crate::playback_info::SdmReport {
        let mut report = crate::playback_info::SdmReport {
            active: self.dsd.sdm_active,
            modulator: Some(self.config.sdm.modulator),
            requested_rate: Some(self.config.sdm.rate),
            fallback_reason: self.dsd.sdm_fallback_reason.clone(),
            ..Default::default()
        };
        if let Some(plan) = self.dsd.sdm_plan {
            report.dsd_rate = plan.dsd_rate;
            report.working_rate = plan.working_rate;
            report.source_rate = plan.pcm_rate;
            report.stage_factor = plan.stage_factor;
            report.multiple = plan.multiple;
        }
        if let Some(stage) = self.dsd.sdm_output.as_ref() {
            let t = stage.telemetry();
            report.clips = t.clips;
            report.state_recoveries = t.state_recoveries;
            report.input_peak = t.input_peak;
            report.ones_ratio = t.ones_ratio;
            report.interpolator_delay_ms = t.interpolator_delay_ms;
            report.modulation_depth = t.modulation_depth;
        }
        report
    }

    /// Helper for short one-field writes to PlaybackInfo.
    pub(super) fn write_playback_info<F: FnMut(&mut PlaybackInfo)>(&self, mut f: F) {
        self.playback_info.rcu(|old| {
            let mut next = old.as_ref().clone();
            f(&mut next);
            Arc::new(next)
        });
    }
}
