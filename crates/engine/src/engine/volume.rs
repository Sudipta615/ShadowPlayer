//! Volume management — linear gain, dB conversion, hardware/software routing.
//!
//! One routing function serves every caller: the public methods here, the
//! `SetVolume`/`SetVolumeDb` commands and the C FFI all land in
//! [`AudioEngine::set_volume`] and [`AudioEngine::set_volume_db`]. That is
//! deliberate — when the routing lived in two places, `HardwarePreferred`
//! behaved differently depending on which door the caller came through.

use log::{error, warn};

use super::AudioEngine;
use crate::dsp::pipeline::VolumePath;

impl AudioEngine {
    /// Whether the active volume path is the endpoint's hardware control.
    pub(crate) fn volume_uses_hardware(&self) -> bool {
        match self.config.volume_mode {
            config::VolumeMode::HardwarePreferred => self
                .audio_output
                .as_ref()
                .is_some_and(|o| o.supports_hardware_volume()),
            config::VolumeMode::HardwareOnly => true,
            _ => false,
        }
    }

    /// Set the volume as a linear gain (0.0-1.0).
    ///
    /// The active [`VolumeMode`](config::VolumeMode) decides whether the
    /// endpoint owns the level or the software gain does. An embedder (or the
    /// C FFI) therefore never has to special-case `HardwarePreferred`: when the
    /// active backend reports no hardware volume, the gain is applied in
    /// software and `PlaybackInfo.volume_error` says so. Read
    /// `PlaybackInfo.volume_path` to see where the level actually went.
    pub fn set_volume(&mut self, vol: f32) {
        if !vol.is_finite() {
            warn!("set_volume ignored: non-finite value {vol}");
            return;
        }
        let clamped = vol.clamp(0.0, 1.0);
        if self.graph.is_bit_perfect() && !self.volume_uses_hardware() {
            self.report_bit_perfect_volume();
            return;
        }
        if !self.volume_uses_hardware() {
            self.graph.set_volume(clamped);
            self.write_playback_info(|pb| {
                pb.volume = clamped;
                pb.volume_error = None;
                pb.volume_path = Some(VolumePath::Software);
            });
        } else {
            self.apply_hardware_volume_linear(clamped);
        }
    }

    /// Set the volume in dB. Range: [-60.0, 0.0].
    ///
    /// Routes exactly like [`set_volume`](Self::set_volume), including the
    /// hardware/software decision.
    pub fn set_volume_db(&mut self, db: f32) {
        if !db.is_finite() {
            warn!("set_volume_db ignored: non-finite value {db}");
            return;
        }
        let linear = if db <= -60.0 {
            0.0
        } else {
            10.0_f32.powf(db.clamp(-60.0, 0.0) / 20.0)
        };
        if self.graph.is_bit_perfect() && !self.volume_uses_hardware() {
            self.report_bit_perfect_volume();
            return;
        }
        if !self.volume_uses_hardware() {
            self.graph.set_volume_db(db);
            self.write_playback_info(|pb| {
                pb.volume = linear;
                pb.volume_error = None;
                pb.volume_path = Some(VolumePath::Software);
            });
        } else {
            self.apply_hardware_volume_db(db, linear);
        }
    }

    /// Report why a bit-perfect graph refused a software volume change.
    fn report_bit_perfect_volume(&mut self) {
        let message = "Bit-Perfect mode: software volume is disabled; use hardware volume or disable Bit-Perfect mode"
            .to_string();
        warn!("{message}");
        self.write_playback_info(|pb| {
            pb.volume_error = Some(message.clone());
            pb.volume_path = None;
        });
    }

    /// Apply a hardware endpoint volume change from a linear gain value.
    fn apply_hardware_volume_linear(&mut self, clamped: f32) {
        let db = if clamped <= 0.0001 {
            -96.0
        } else {
            20.0 * clamped.log10()
        };
        self.apply_hardware_volume_db(db, clamped);
    }

    /// Shared path for hardware volume: converts to dB (if needed), sends to
    /// the output backend, and falls back to software gain when appropriate.
    ///
    /// The fallback is never silent: `PlaybackInfo.volume_error` carries the
    /// reason and `volume_path` says which path owns the level now. In
    /// `HardwareOnly` the failing set never introduces software gain.
    fn apply_hardware_volume_db(&mut self, db: f32, linear: f32) {
        match self
            .audio_output
            .as_ref()
            .map(|o| o.set_hardware_volume_db(db))
        {
            Some(Ok(())) => {
                self.graph.set_volume(1.0);
                self.write_playback_info(|pb| {
                    pb.volume = linear;
                    pb.volume_error = None;
                    pb.volume_path = Some(VolumePath::Hardware);
                });
            }
            Some(Err(e)) => {
                error!("Hardware volume set failed: {e}");
                if self.config.volume_mode == config::VolumeMode::HardwareOnly {
                    self.graph.set_volume(1.0);
                    self.write_playback_info(|pb| {
                        pb.volume = linear;
                        pb.volume_error = Some(format!(
                            "HardwareOnly: endpoint volume set failed ({e}); \
                             software volume NOT applied"
                        ));
                        pb.volume_path = None;
                    });
                } else {
                    self.graph.set_volume(linear);
                    self.write_playback_info(|pb| {
                        pb.volume = linear;
                        pb.volume_error = Some(format!(
                            "Hardware volume set failed ({e}); using software gain"
                        ));
                        pb.volume_path = Some(VolumePath::Software);
                    });
                }
            }
            None => {
                self.graph.set_volume(1.0);
                self.write_playback_info(|pb| {
                    pb.volume = linear;
                    pb.volume_error = None;
                    pb.volume_path = if self.config.volume_mode == config::VolumeMode::HardwareOnly
                    {
                        None
                    } else {
                        Some(VolumePath::Hardware)
                    };
                });
            }
        }
    }

    /// Read the current volume in dB.
    pub fn volume_db(&self) -> f32 {
        self.graph.volume_db()
    }

    /// Convert a UI percentage (0.0–100.0) to a dB value suitable for
    /// `set_volume_db`.
    pub fn volume_percent_to_db(percent: f32) -> f32 {
        crate::dsp::pipeline::DspPipeline::volume_percent_to_db(percent)
    }

    /// Reset the clip and NaN counters in PlaybackInfo.
    pub fn reset_dsp_diagnostics(&mut self) {
        self.write_playback_info(|pb| {
            pb.clip_count = 0;
            pb.nan_count = 0;
        });
    }

    /// Enable or disable true-peak detection on the limiter.
    pub fn set_limiter_true_peak(&mut self, enabled: bool) {
        self.graph.set_limiter_true_peak(enabled);
    }

    /// Whether true-peak detection is currently active on the limiter.
    pub fn limiter_true_peak_enabled(&self) -> bool {
        self.graph.limiter_true_peak_enabled()
    }
}
