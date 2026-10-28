//! DSP command handlers — stereo width, balance, dither, crossfeed,
//! compressor, limiter, bit-perfect, precision, crossfade, resampler quality.

use log::info;

use super::super::{AudioEngine, PlaybackStream};
use crate::dsp::pipeline::VolumePath;

impl AudioEngine {
    pub(super) fn handle_set_stereo_width(&mut self, width: f32) {
        self.graph.set_stereo_width(width);
    }

    /// Wet/dry mix for the canonical chain's convolution insert.
    ///
    /// The engine's own `config.convolution.wet_mix` is updated too, so a
    /// later generation rebuild (track change, sample-rate change) inherits
    /// the live value rather than snapping back to the value the config file
    /// was loaded with. That inheritance is the same contract the volume /
    /// balance / speed user-state replay uses.
    pub(super) fn handle_set_convolution_wet_mix(&mut self, wet_mix: f32) {
        if !wet_mix.is_finite() {
            log::warn!("Convolution wet mix ignored: non-finite value");
            return;
        }
        let clamped = wet_mix.clamp(0.0, 1.0);
        self.config.convolution.wet_mix = clamped;
        self.graph.set_convolution_wet_mix(clamped);
    }

    pub(super) fn handle_set_balance(&mut self, balance: f32) {
        self.graph.set_balance(balance);
    }

    pub(super) fn handle_set_dither_enabled(&mut self, enabled: bool) {
        if let Some(ref output) = self.audio_output {
            output.set_dither_enabled(enabled && !self.dsd.dop_active);
        }
        self.config.dither_enabled = enabled;
    }

    pub(super) fn handle_set_crossfeed_enabled(&mut self, enabled: bool) {
        self.graph.set_crossfeed_enabled(enabled);
    }

    pub(super) fn handle_set_crossfeed_profile(&mut self, profile: config::CrossfeedProfile) {
        self.graph.set_crossfeed_profile(profile);
    }

    /// Apply the creative-effects insert's settings.
    pub(super) fn handle_set_fx_config(&mut self, config: config::FxConfig) {
        self.graph.set_fx_config(config);
    }

    pub(super) fn handle_set_crossfeed_custom_params(
        &mut self,
        frequency_hz: f32,
        q: f32,
        delay_ms: f32,
    ) {
        self.graph
            .set_crossfeed_custom_params(frequency_hz, q, delay_ms);
    }

    pub(super) fn handle_set_compressor_enabled(&mut self, enabled: bool) {
        self.graph.set_compressor_enabled(enabled);
    }

    pub(super) fn handle_set_compressor_band_params(
        &mut self,
        band: usize,
        threshold_db: f32,
        ratio: f32,
        attack_ms: f32,
        release_ms: f32,
        makeup_gain_db: f32,
    ) {
        self.graph.set_compressor_band_params(
            band,
            threshold_db,
            ratio,
            attack_ms,
            release_ms,
            makeup_gain_db,
        );
    }

    pub(super) fn handle_set_precision_mode(&mut self, mode: crate::dsp::pipeline::PrecisionMode) {
        info!("DSP precision mode set to {:?}", mode);
        self.graph.set_precision_mode(mode);
    }

    pub(super) fn handle_set_bit_perfect(&mut self, enabled: bool) {
        info!("Bit-perfect mode: {}", if enabled { "on" } else { "off" });
        self.graph.set_bit_perfect(enabled);
        if enabled {
            self.graph.set_volume(1.0);
            self.graph.with_graph(|g| g.seek_fade_mut().fade.reset());
            let uses_hardware = self.volume_uses_hardware();
            self.write_playback_info(|pb| {
                pb.volume_path = if uses_hardware {
                    Some(VolumePath::Hardware)
                } else {
                    None
                };
                pb.volume_error = if uses_hardware {
                    None
                } else {
                    Some(
                        "Bit-Perfect mode: software volume disabled; hardware volume unavailable"
                            .to_string(),
                    )
                };
                pb.bit_perfect = true;
            });
        } else {
            self.write_playback_info(|pb| {
                pb.bit_perfect = false;
                pb.volume_path = None;
                pb.volume_error = None;
            });
        }
    }

    pub(super) fn handle_set_limiter_mode(&mut self, mode: crate::dsp::limiter::LimiterMode) {
        info!("Limiter mode set to {:?}", mode);
        self.graph.set_limiter_mode(mode);
    }

    pub(super) fn handle_set_limiter_true_peak(&mut self, enabled: bool) {
        info!(
            "Limiter true-peak FIR: {}",
            if enabled { "enabled" } else { "disabled" }
        );
        self.graph.set_limiter_true_peak(enabled);
    }

    pub(super) fn handle_set_resampler_quality(&mut self, quality: config::ResamplerQuality) {
        self.config.resampler_quality = quality;
        #[cfg(feature = "resample")]
        match &mut self.stream {
            Some(PlaybackStream::Single {
                resampler: Some(ref mut r),
                ..
            }) => {
                r.set_quality(quality);
            }
            Some(PlaybackStream::Transitioning {
                outgoing_resampler,
                incoming_resampler,
                ..
            }) => {
                if let Some(ref mut r) = outgoing_resampler {
                    r.set_quality(quality);
                }
                if let Some(ref mut r) = incoming_resampler {
                    r.set_quality(quality);
                }
            }
            _ => {}
        }
        info!("Resampler quality set to {:?}", quality);
    }

    /// Select how DSD sources reach the DAC.
    ///
    /// The transport is negotiated when the stream is opened, so this updates
    /// the engine config and takes effect on the next DSD stream. The current
    /// stream is deliberately left alone: switching transport mid-bitstream
    /// would tear down the device while the UI claims it is still playing.
    pub(super) fn handle_set_dsd_output(&mut self, output: config::DsdOutput) {
        self.config.dsd_output = output;
        info!("DSD output transport set to {:?} (next stream)", output);
    }

    /// Select the DSD→PCM decimation filter.
    ///
    /// The decimator is constructed with the decoder, so this applies to the
    /// next DSD stream. The UI labels it accordingly.
    pub(super) fn handle_set_dsd_noise_filter(&mut self, filter: config::DsdNoiseFilter) {
        self.config.dsd_noise_filter = filter;
        info!(
            "DSD decimation filter set to {:?} (next DSD stream)",
            filter
        );
    }

    /// Select the output-domain ultrasonic low-pass. The stage is outside the
    /// generation plan, so this can apply live without a graph rebuild.
    pub(super) fn handle_set_ultrasonic_filter(&mut self, filter: config::UltrasonicFilter) {
        self.config.ultrasonic_filter = filter;
        self.graph
            .with_graph(|graph| graph.set_ultrasonic_filter(filter));
        info!("Ultrasonic filter set to {:?}", filter);
    }

    /// Select the polyphase resampler filter family.
    ///
    /// Every active resampler (decode-domain and per-endpoint) rebuilds on its
    /// own control path; the rebuild keeps the buffered tail and crossfades,
    /// so this is safe while playing.
    pub(super) fn handle_set_resampler_filter(&mut self, filter: config::ResamplerFilter) {
        self.config.resampler_filter = filter;
        #[cfg(feature = "resample")]
        match &mut self.stream {
            Some(PlaybackStream::Single {
                resampler: Some(ref mut r),
                ..
            }) => r.set_filter(filter),
            Some(PlaybackStream::Transitioning {
                outgoing_resampler,
                incoming_resampler,
                ..
            }) => {
                if let Some(ref mut r) = outgoing_resampler {
                    r.set_filter(filter);
                }
                if let Some(ref mut r) = incoming_resampler {
                    r.set_filter(filter);
                }
            }
            _ => {}
        }
        #[cfg(feature = "audio-output")]
        for endpoint in &mut self.endpoints {
            endpoint.set_resampler_filter(filter);
        }
        info!("Resampler filter family set to {:?}", filter);
    }

    /// Select the final quantization noise shaper.
    ///
    /// A converter's dither design is built when the device stream opens, so
    /// this arms the next stream. Updating the process-wide construction
    /// default keeps host-created converters consistent with this choice.
    pub(super) fn handle_set_noise_shaper(&mut self, shaper: config::NoiseShaper) {
        self.config.noise_shaper = shaper;
        crate::dsp::dither::set_configured_noise_shaper(shaper);
        info!("Noise shaper set to {:?} (next stream)", shaper);
    }

    /// Configure PCM→SDM native-DSD output. The modulator and rate are part of
    /// device negotiation, so this applies on the next stream open.
    pub(super) fn handle_set_sdm_config(&mut self, cfg: config::SdmConfig) {
        self.config.sdm = cfg;
        info!("SDM config updated (next stream)");
    }

    /// Replace the DAC correction profile table and reload the matching bank.
    pub(super) fn handle_set_dac_correction(&mut self, cfg: config::DacCorrectionConfig) {
        self.config.dac_correction = cfg.clone();
        self.graph.with_graph(|graph| graph.set_dac_correction(cfg));
        info!("DAC correction profiles updated");
    }

    pub(super) fn handle_set_crossfade_config(&mut self, cfg: config::CrossfadeConfig) {
        info!(
            "Crossfade config updated: enabled={}, duration={}ms, curve={:?}",
            cfg.enabled, cfg.duration_ms, cfg.curve
        );
        self.config.crossfade = cfg.clone();
        self.graph.set_crossfade_curve(cfg.curve);
        self.graph.set_crossfade_duration_ms(cfg.duration_ms);
        self.graph.set_crossfade_enabled(cfg.enabled);
    }

    pub(super) fn handle_set_crossfade_curve(&mut self, curve: config::CrossfadeCurve) {
        info!("Crossfade curve set to {:?}", curve);
        self.config.crossfade.curve = curve;
        self.graph.set_crossfade_curve(curve);
    }

    pub(super) fn handle_set_transition_mode(&mut self, mode: config::TransitionMode) {
        info!("Transition mode set to {:?}", mode);
        self.config.transition_mode = mode;
    }

    pub(super) fn handle_set_speed_mode(&mut self, mode: config::SpeedMode) {
        info!("Speed mode set to {:?}", mode);
        self.config.speed_mode = mode;
        let current_speed = self.speed;
        self.handle_set_speed(current_speed);
    }
}
