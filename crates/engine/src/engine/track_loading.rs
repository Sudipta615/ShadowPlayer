//! Track loading, DSD transport negotiation, and gapless handoff.
//!
//! This module owns the decode→engine integration: opening decoders,
//! negotiating native-DSD / DoP / PCM transport paths, resampler creation,
//! loudness metadata retrieval, and the state-preserving `swap_to_next_track`.

use std::path::Path;
use std::sync::Arc;

use log::{error, info, warn};

use config;

#[cfg(feature = "resample")]
use crate::dsp::resampler::GenericResampler;
use crate::{
    buffer::{PlaybackInfo, PlaybackState},
    decode::{DecodeInfo, Decoder},
    events::EngineEvent,
    output::OutputError,
    source::AudioSource,
};

use super::{dop_exclusive_reason, recovery, AudioEngine, EngineError, PlaybackStream};

impl AudioEngine {
    pub fn load_track(&mut self, path: &Path) -> Result<DecodeInfo, EngineError> {
        let decoder = match self.scratch.cached_incoming_decoder.take() {
            Some(d) if self.loudness_scan.next_track_path.as_deref() == Some(path) => {
                info!("Using cached decoder for load_track");
                d
            }
            _ => Decoder::open(path)?,
        };

        self.load_opened_decoder(AudioSource::File(path.to_path_buf()), decoder, Some(path))
    }

    /// Load in-memory byte buffer for decoding.
    pub fn load_memory(
        &mut self,
        data: Vec<u8>,
        extension_hint: Option<&str>,
    ) -> Result<DecodeInfo, EngineError> {
        let decoder = Decoder::open_memory(data.clone(), extension_hint)?;
        let source = AudioSource::Memory {
            data,
            extension_hint: extension_hint.map(String::from),
        };
        self.load_opened_decoder(source, decoder, None)
    }

    /// Configure and load an opened decoder into the engine pipeline.
    pub(crate) fn load_opened_decoder(
        &mut self,
        source: AudioSource,
        mut decoder: Decoder,
        loudness_path: Option<&Path>,
    ) -> Result<DecodeInfo, EngineError> {
        // A native DSD output is a different transport, not merely a sample
        // rate. Leave it explicitly before loading any track whose requested
        // policy is PCM/DoP (including a DSD→PCM transition); otherwise a raw
        // decoder would be paired with a stale native render thread and the
        // PCM ring would never reach the DAC.
        // PCM→SDM is its own opt-in: a PCM source does not consult
        // `dsd_output` (which selects the transport for *DSD sources*).
        // If native DSD is unavailable the engine stays on PCM and records
        // the reason, exactly as for an explicit request.
        //
        // Native DSD, DoP and PCM→SDM all drive the same output stream, so
        // choose exactly one plan up front instead of negotiating each in
        // turn. `dsd_bypass` keeps native DSD files bit-perfect while SDM is
        // enabled; turning it off hands DSD sources to the modulator.
        let transport_plan = super::dsd_state::plan_transport(
            decoder.is_dsd(),
            self.config.dsd_output,
            &self.config.sdm,
        );
        let wants_sdm = transport_plan == super::dsd_state::DsdTransportPlan::Sdm;
        let wants_native = transport_plan == super::dsd_state::DsdTransportPlan::Native;
        // PCM→DoP synthesis also owns the output format (24-bit PCM at
        // `dsd_rate / 16`), so every load leaves it before renegotiating. It
        // is not a native DSD stream, but the rate and container are just as
        // specific and a stale one would feed raw PCM into a DoP-rate stream.
        let needs_teardown = (self.dsd.native_dsd_active && !wants_native)
            || (self.dsd.sdm_active && !wants_sdm)
            || self.dsd.dop_from_pcm;
        if needs_teardown {
            let output_rate = {
                let output = self.audio_output.as_mut().ok_or_else(|| {
                    EngineError::Output(OutputError::StreamError(
                        "native DSD active without an output transport".to_string(),
                    ))
                })?;
                output.set_native_dsd(None)?;
                output.set_dither_enabled(self.config.dither_enabled);
                output.sample_rate()
            };
            self.output_sample_rate = output_rate;
            self.graph
                .update_sample_rate(self.output_sample_rate as f32);
            self.dsd.native_dsd_active = false;
            self.dsd.dsd_wire_format = None;
            self.dsd.dsd_byte_buffer = None;
            self.dsd.sdm_active = false;
            self.dsd.sdm_output = None;
            self.dsd.sdm_plan = None;
            self.dsd.clear_sdm_dop();
            self.dsd.dop_active = false;
            self.dsd.dop_rate = 0;
            self.graph.set_dop_bypass(false);
            // The primary output is PCM again and may have a different rate
            // than when the monitor was built; its conversion ratio belongs
            // to that PCM clock.
            self.sync_talkback();
        }

        // ── DSD output mode selection (§7) ─────────────────────────────────
        let mut native_dsd_active = false;
        // A new track always starts with a clean PCM→SDM stage. The output
        // transport (if it was already native DSD) is reconfigured below.
        self.dsd.sdm_active = false;
        self.dsd.sdm_output = None;
        self.dsd.sdm_plan = None;
        self.dsd.sdm_fallback_reason = None;
        self.dsd.clear_sdm_dop();
        // The decimation filter only matters on the PCM-conversion path, but
        // selecting it here (before any decode) keeps the cascade in sync with
        // the config for the whole track.
        if decoder.is_dsd() {
            decoder.set_dsd_noise_filter(self.config.dsd_noise_filter);
        }
        let mut dsd_report = if decoder.is_dsd() {
            let requested = match self.config.dsd_output {
                config::DsdOutput::NativeDsd => crate::decode::DsdTransport::Native,
                config::DsdOutput::DoP => crate::decode::DsdTransport::Dop,
                config::DsdOutput::PcmConvert => crate::decode::DsdTransport::PcmConversion,
            };
            let mut r = crate::decode::DsdTransportReport::new(requested, requested);
            r.bit_rate = decoder.dsd_bit_rate();
            r
        } else {
            crate::decode::DsdTransportReport::default()
        };
        if wants_native && decoder.is_dsd() {
            match self.negotiate_native_dsd(&mut decoder) {
                Ok((format, buffer)) => {
                    native_dsd_active = true;
                    dsd_report.actual = crate::decode::DsdTransport::Native;
                    dsd_report.wire_format = Some(format);
                    self.dsd.dsd_wire_format = Some(format);
                    self.dsd.dsd_byte_buffer = Some(buffer);
                    let bit_rate = dsd_report.bit_rate.unwrap_or(2_822_400);
                    self.output_sample_rate = format.frame_rate_hz(bit_rate);
                    info!(
                        "DSD native active: {} at {} (DSP structurally bypassed)",
                        format.label(),
                        format.frame_rate_hz(bit_rate)
                    );
                }
                Err(e) => {
                    dsd_report.actual = crate::decode::DsdTransport::Dop;
                    dsd_report.step(format!("native DSD unavailable ({e})"));
                    dsd_report.step("fallback: DoP");
                    warn!("Native DSD unavailable ({e}); falling back to DoP, then DSD→PCM");
                }
            }
        }
        let mut sdm_active = false;
        // Output PCM rate of a synthesised DoP stream (`dsd_rate / 16`).
        let mut sdm_dop_rate: Option<u32> = None;
        if wants_sdm {
            // Native DSD first. A failure is not the end of the modulator: if
            // the host opted in, the same one-bit stream is framed as DoP
            // into a 24-bit PCM transport.
            let native = match self.negotiate_sdm(&decoder) {
                Ok((plan, format, buffer)) => {
                    let channels = decoder.info().channels;
                    match crate::dsp::sdm::SdmOutput::new(
                        plan,
                        channels,
                        format,
                        self.config.sdm.modulator.order(),
                    ) {
                        Some(stage) => {
                            sdm_active = true;
                            self.dsd.sdm_output = Some(stage);
                            self.dsd.sdm_plan = Some(plan);
                            self.dsd.dsd_wire_format = Some(format);
                            self.dsd.dsd_byte_buffer = Some(buffer);
                            // The PCM DSP chain runs at the working rate; the
                            // resampler below converts the source to it.
                            self.output_sample_rate = plan.working_rate;
                            self.graph.update_sample_rate(plan.working_rate as f32);
                            info!(
                                "PCM→SDM active: {} modulator, {} ch, {} Hz → {} bps \
                                 (working {} Hz, ×{}, {} wire)",
                                self.config.sdm.modulator.label(),
                                channels,
                                plan.pcm_rate,
                                plan.dsd_rate,
                                plan.working_rate,
                                plan.stage_factor,
                                format.label(),
                            );
                            Ok(())
                        }
                        None => {
                            if let Some(output) = self.audio_output.as_mut() {
                                let _ = output.set_native_dsd(None);
                            }
                            Err(format!(
                                "could not build the {} modulator for {} channels",
                                self.config.sdm.modulator.label(),
                                channels
                            ))
                        }
                    }
                }
                Err(e) => Err(e.to_string()),
            };

            if let Err(native_err) = native {
                if self.config.sdm.dop && decoder.info().channels == 2 {
                    match self.negotiate_sdm_dop(&decoder) {
                        Ok((plan, dop_rate)) => match crate::dsp::sdm::SdmDopOutput::new(
                            plan,
                            2,
                            self.config.sdm.modulator.order(),
                        ) {
                            Some(stage) => {
                                sdm_active = true;
                                sdm_dop_rate = Some(dop_rate);
                                self.dsd.dop_from_pcm = true;
                                self.dsd.dop_pending = Vec::with_capacity(
                                    stage.output_capacity_frames(
                                        crate::buffer::MAX_AUDIO_BLOCK_FRAMES,
                                    ) * 2,
                                );
                                self.dsd.sdm_dop_output = Some(stage);
                                self.dsd.sdm_plan = Some(plan);
                                self.output_sample_rate = plan.working_rate;
                                self.graph.update_sample_rate(plan.working_rate as f32);
                                dsd_report = crate::decode::DsdTransportReport::new(
                                    crate::decode::DsdTransport::Dop,
                                    crate::decode::DsdTransport::Dop,
                                );
                                dsd_report.bit_rate = Some(plan.dsd_rate);
                                info!(
                                    "PCM→DoP active: {} modulator, 2 ch, {} Hz → {} bps \
                                     (working {} Hz, ×{}, DoP {} Hz via 24-bit PCM)",
                                    self.config.sdm.modulator.label(),
                                    plan.pcm_rate,
                                    plan.dsd_rate,
                                    plan.working_rate,
                                    plan.stage_factor,
                                    dop_rate,
                                );
                            }
                            None => {
                                let reason = "could not build the stereo DoP packing stage";
                                warn!("PCM→DoP unavailable: {reason}; staying on PCM");
                                self.dsd.sdm_fallback_reason = Some(format!(
                                    "native DSD unavailable ({native_err}); {reason}"
                                ));
                            }
                        },
                        Err(e) => {
                            let reason = format!(
                                "native DSD unavailable ({native_err}); DoP synthesis \
                                 unavailable ({e})"
                            );
                            warn!("PCM→SDM unavailable: {reason}; staying on PCM");
                            self.dsd.sdm_fallback_reason = Some(reason);
                        }
                    }
                } else {
                    warn!("PCM→SDM unavailable ({native_err}); staying on PCM");
                    self.dsd.sdm_fallback_reason = Some(native_err);
                }
            }
        }

        // DoP is the requested transport, or the first fallback when a
        // native/SDM attempt failed. A plan that already engaged (native or
        // SDM) must never touch the output again.
        let native_failed = wants_native && !native_dsd_active;
        let sdm_failed = wants_sdm && !sdm_active;
        let dop_is_fallback = native_failed
            || (sdm_failed && self.config.dsd_output != config::DsdOutput::PcmConvert);
        let attempt_dop = decoder.is_dsd()
            && !native_dsd_active
            && !sdm_active
            && (transport_plan == super::dsd_state::DsdTransportPlan::Dop || dop_is_fallback);
        if attempt_dop {
            decoder.set_dop_mode(true);
        }
        let dop_rate = if attempt_dop {
            decoder.dop_rate()
        } else {
            None
        };
        let mut dop_active = false;
        if let Some(dr) = dop_rate {
            if let Some(ref mut output) = self.audio_output {
                let out_info = output.output_info();
                let exclusive = out_info.access_state.is_bit_perfect();
                if exclusive {
                    match output.reconfigure_sample_format(dr, cpal::SampleFormat::I32) {
                        Ok(actual)
                            if actual == dr
                                && output.sample_format() == cpal::SampleFormat::I32 =>
                        {
                            dop_active = true;
                            self.output_sample_rate = dr;
                            self.graph.update_sample_rate(dr as f32);
                        }
                        Ok(actual) => {
                            warn!(
                                "DoP requested but output settled on {} Hz without the I32 \
                                 container (need {} Hz I32); using DSD→PCM",
                                actual, dr
                            );
                            dsd_report.step(format!(
                                "DoP unavailable: output settled on {} Hz without the I32 \
                                 container (need {} Hz I32)",
                                actual, dr
                            ));
                        }
                        Err(e) => {
                            warn!(
                                "DoP requested but the output could not provide a {} Hz I32 \
                                 stream ({}); using DSD→PCM",
                                dr, e
                            );
                            dsd_report.step(format!("DoP unavailable ({e})"));
                        }
                    }
                } else {
                    let reason = dop_exclusive_reason(&out_info, self.config.output_backend);
                    warn!(
                        "DoP requires an exclusive output — {reason}. Using DSD→PCM for this track."
                    );
                    dsd_report.step(format!("DoP requires an exclusive output — {reason}"));
                }
            } else {
                warn!("DoP requested but no output device is active; using DSD→PCM");
                dsd_report.step("DoP unavailable: no output device is active");
            }
            if !dop_active && dsd_report.actual != crate::decode::DsdTransport::PcmConversion {
                dsd_report.actual = crate::decode::DsdTransport::PcmConversion;
                dsd_report.step("fallback: DSD→PCM conversion");
            }
        } else if decoder.is_dsd()
            && !native_dsd_active
            && dsd_report.requested != crate::decode::DsdTransport::PcmConversion
            && dsd_report.actual != crate::decode::DsdTransport::PcmConversion
        {
            dsd_report.actual = crate::decode::DsdTransport::PcmConversion;
            dsd_report.step("fallback: DSD→PCM conversion");
        }
        if !dop_active && !native_dsd_active {
            decoder.set_dop_mode(false);
        }
        if native_dsd_active {
            decoder.set_native_dsd_mode(true);
        }
        // The negotiated plan is the authority for the read-back report.
        // `DsdTransport` has no SDM variant: a re-modulated DSD source is
        // recorded as decimated to PCM here, while `sdm_active`/`sdm` describe
        // the one-bit output that actually leaves the engine.
        if decoder.is_dsd() && !native_dsd_active {
            if dop_active {
                dsd_report.actual = crate::decode::DsdTransport::Dop;
            } else if sdm_active {
                dsd_report.actual = crate::decode::DsdTransport::PcmConversion;
                dsd_report.wire_format = None;
                dsd_report.step("re-modulated to native DSD (PCM→SDM)");
            } else {
                dsd_report.actual = crate::decode::DsdTransport::PcmConversion;
            }
        }
        // Record the filter that actually ran: only the PCM-conversion path
        // decimates, so native/DoP transport reports no filter.
        if dsd_report.actual == crate::decode::DsdTransport::PcmConversion {
            dsd_report.noise_filter = decoder.dsd_noise_filter();
        }
        // `dop_active` covers both DoP transports: a DSD source packed by the
        // decoder (DSP bypassed) and PCM→SDM frames synthesised here (DSP
        // active at the working rate). Keep them distinguishable.
        let dop_from_pcm = sdm_dop_rate.is_some();
        self.dsd.dop_active = dop_active || dop_from_pcm;
        self.dsd.dop_rate = if dop_active {
            dop_rate.unwrap_or(0)
        } else {
            sdm_dop_rate.unwrap_or(0)
        };
        self.dsd.native_dsd_active = native_dsd_active;
        self.dsd.sdm_active = sdm_active;
        self.dsd.dsd_transport_report = dsd_report;
        let bitstream_bypass = native_dsd_active || (dop_active && !dop_from_pcm);
        self.graph.set_dop_bypass(bitstream_bypass);
        // Dither must be off for every DoP transport: it would overwrite the
        // 24-bit DoP word (marker included) with shaped quantization noise.
        if bitstream_bypass || dop_from_pcm {
            if let Some(ref output) = self.audio_output {
                output.set_dither_enabled(false);
            }
        } else if let Some(ref output) = self.audio_output {
            output.set_dither_enabled(self.config.dither_enabled);
        }
        if bitstream_bypass || dop_from_pcm {
            self.speed = 1.0;
        }
        if dop_active && !dop_from_pcm {
            info!(
                "DSD DoP active: {} Hz to DAC (DSP bypassed, dither off)",
                self.dsd.dop_rate
            );
        } else if dop_from_pcm {
            info!(
                "PCM→DoP active: {} Hz DoP frames to DAC (DSP at working rate, dither off)",
                self.dsd.dop_rate
            );
        }

        let info = decoder.info().clone();
        self.clock.reset_track(info.sample_rate);
        self.analyzer.set_sample_rate(info.sample_rate);
        self.apodizing.reset(info.sample_rate as f32);
        self.duration_secs = info.duration_secs;
        self.recovery.consecutive_decode_errors = 0;

        if !dop_active && !native_dsd_active && !sdm_active {
            if let Some(ref mut output) = self.audio_output {
                let caps = output.capabilities();
                let target_rate = caps.best_rate_for_with_allowed(
                    info.sample_rate,
                    &self.config.sample_rate_policy,
                    &self.config.allowed_sample_rates,
                );
                if let Ok(actual_rate) = output.reconfigure_sample_rate(target_rate) {
                    self.output_sample_rate = actual_rate;
                    self.graph.update_sample_rate(actual_rate as f32);
                }
            }
            // PCM transport: the monitor must follow the output clock this
            // track selected. Idempotent when the rate did not move.
            self.sync_talkback();
        }

        #[cfg(feature = "resample")]
        let resample_speed = if dop_active || native_dsd_active {
            1.0
        } else {
            self.speed
        };
        #[cfg(feature = "resample")]
        let resampler = if native_dsd_active {
            None
        } else {
            recovery::build_resampler(
                self.config.resampler_quality,
                self.config.resampler_filter,
                self.config.resampler_cutoff_ratio,
                self.clock.source_sample_rate as f32,
                self.output_sample_rate as f32,
                resample_speed,
                self.config.precision_mode,
            )
        };

        #[cfg(feature = "resample")]
        if !native_dsd_active
            && resampler.is_none()
            && (self.clock.source_sample_rate != self.output_sample_rate
                || (self.speed - 1.0).abs() > 0.001)
        {
            error!(
                "Critical: Resampler required ({} Hz -> {} Hz) but could not be initialized!",
                self.clock.source_sample_rate, self.output_sample_rate
            );
            self.write_playback_info(|pb| {
                pb.resampler_disabled = true;
                pb.resampler_failed_fatal = true;
                pb.set_engine_error(
                    crate::diagnostics::DiagnosticKind::Resampler,
                    "Resampler initialization failed; cannot play at requested rate/speed",
                );
            });
            return Err(EngineError::Resampler(format!(
                "Required resampler ({} Hz -> {} Hz) could not be initialized; \
                 refusing to play at the wrong rate/speed",
                self.clock.source_sample_rate, self.output_sample_rate
            )));
        }

        #[cfg(not(feature = "resample"))]
        let resampler: Option<()> = None;

        self.stream = Some(PlaybackStream::Single { decoder, resampler });

        if let Some(ref output) = self.audio_output {
            output.reset_buffer();
            let _ = output.take_underruns();
        } else {
            self.sample_sink.reset();
        }
        self.scratch.pending_output_frames.clear();
        self.scratch.pending_multichannel.clear();
        self.scratch.pending_multichannel_channels = 0;
        self.scratch.pending_chunk = None;
        self.scratch.pending_incoming_chunk = None;
        self.scratch.rs_out_buf.clear();
        self.scratch.rs_in_buf.clear();
        self.graph.reset();

        let current_volume = self.playback_info.load().volume;
        self.graph.set_volume(current_volume);
        self.graph.with_graph(|g| g.volume_mut().processor.snap());

        let loudness_meta = if let Some(path) = loudness_path {
            let mut meta = crate::decode::extract_loudness_metadata(path);
            if meta.ebu_r128_loudness.is_none() {
                if let Some(cached) = crate::decode::loudness_cache::lookup(path) {
                    meta.ebu_r128_loudness = cached.ebu_r128_loudness;
                    meta.ebu_r128_peak = cached.ebu_r128_peak_dbtp;
                    info!("Loaded cached loudness metadata for {}", path.display());
                }
            }
            meta
        } else {
            crate::dsp::LoudnessMetadata::default()
        };

        self.loudness_scan.current_track_path = loudness_path.map(|p| p.to_path_buf());
        self.loudness_scan.pending_loudness_metadata = Some(loudness_meta);
        self.graph
            .apply_loudness_metadata_outgoing(Some(loudness_meta));
        if loudness_path.is_some() {
            self.start_loudness_scan();
        }

        self.graph.begin_playing();

        self.stream_ended = false;
        self.current_source = Some(source.clone());
        let current_source = self.current_source.clone();
        let speed = self.speed;
        let dop_active = self.dsd.dop_active;
        let native_dsd_active = self.dsd.native_dsd_active;
        let dsd_transport = self.dsd.dsd_transport_report.actual;
        let dsd_transport_report = self.dsd.dsd_transport_report.clone();
        self.playback_info.rcu(|old| {
            Arc::new(PlaybackInfo {
                duration_secs: info.duration_secs,
                sample_rate: info.sample_rate,
                current_source: current_source.clone(),
                speed,
                dop_active,
                native_dsd_active,
                sdm_active,
                sdm: self.sdm_report(),
                dsd_transport,
                dsd_transport_report: dsd_transport_report.clone(),
                volume: old.volume,
                state: if old.state == PlaybackState::Stopped {
                    PlaybackState::Paused
                } else {
                    old.state
                },

                ..Default::default()
            })
        });

        self.emit_event(EngineEvent::SourceOpened {
            source,
            sample_rate: info.sample_rate,
            channels: info.channels,
            duration_secs: info.duration_secs,
        });
        self.emit_event(EngineEvent::FormatChanged {
            sample_rate: info.sample_rate,
            channels: info.channels,
        });

        info!(
            "Loaded source: {} Hz, {} ch, {:.1}s",
            info.sample_rate, info.channels, info.duration_secs
        );
        Ok(info)
    }

    /// Load an explicit [`AudioSource`] (file, URI, or memory) for playback.
    pub fn load_source(&mut self, source: &AudioSource) -> Result<DecodeInfo, EngineError> {
        match source {
            AudioSource::File(path) => self.load_track(path),
            // A CUE segment plays through the same load path as its file, then
            // seeks to the segment start. Going through `load_opened_decoder`
            // (rather than a parallel implementation) is what keeps transport
            // negotiation, loudness metadata, and resampler setup identical
            // between a whole album file and one of its tracks.
            AudioSource::CueSegment(seg) => {
                // Open the underlying file and seek it, then hand the
                // positioned decoder to the same `load_opened_decoder` the
                // whole-file path uses. Going through the shared seam is what
                // keeps transport negotiation, loudness metadata, and
                // resampler setup identical between an album file and one of
                // its tracks.
                let mut decoder = Decoder::open(&seg.path)?;
                if seg.start_frame > 0 {
                    let sample_rate = decoder.info().sample_rate.max(1);
                    let secs = seg.start_frame as f64 / f64::from(sample_rate);
                    decoder.seek(secs as f32)?;
                }
                self.load_opened_decoder(source.clone(), decoder, Some(&seg.path))
            }
            AudioSource::Uri(uri) => {
                // Classified, not collapsed: a URL must never be handed to a
                // filesystem loader. See `decode::resolve_uri`.
                match crate::decode::resolve_uri(uri).map_err(EngineError::InvalidSource)? {
                    crate::decode::UriTarget::Local(path) => self.load_track(&path),
                    crate::decode::UriTarget::Remote(url) => {
                        let decoder = crate::decode::stream::open_remote(&url)?;
                        let info = decoder.info().clone();
                        self.load_opened_decoder(AudioSource::Uri(url), decoder, None)?;
                        Ok(info)
                    }
                }
            }
            AudioSource::Memory {
                data,
                extension_hint,
            } => self.load_memory(data.clone(), extension_hint.as_deref()),
            // The control plane already decoded this source. Nothing
            // is opened and nothing is decoded; the pipeline downstream is the
            // same one a file decoder feeds.
            AudioSource::SharedPcm(pcm) => self.load_shared_pcm(pcm.clone()),
            // A live AES67 stream: open the socket and join the group here, so
            // an unreachable multicast group fails `Open` with a typed error
            // rather than playing silence indefinitely. From here it is an
            // ordinary decoder feeding the ordinary pipeline.
            AudioSource::NetworkStream(cfg) => {
                let decoder = crate::decode::Aes67Decoder::new(cfg.as_ref().clone())
                    .map_err(|e| EngineError::InvalidSource(format!("AES67 stream: {e}")))?;
                let info = decoder.info().clone();
                self.load_opened_decoder(
                    AudioSource::NetworkStream(cfg.clone()),
                    Decoder::Aes67(decoder),
                    None,
                )?;
                Ok(info)
            }
        }
    }

    /// Load an already-decoded shared payload.
    ///
    /// The handoff happens entirely on the control/decode thread: the `Arc` is
    /// cloned, a cursor is opened, and the ring/crossfade machinery downstream
    /// is untouched. The audio callback keeps pulling from the preallocated
    /// `PcmRingBuffer`, so the realtime contract is preserved exactly as it is
    /// for a file source.
    pub fn load_shared_pcm(
        &mut self,
        pcm: crate::decode::shared_pcm::SharedPcm,
    ) -> Result<DecodeInfo, EngineError> {
        let source = AudioSource::SharedPcm(pcm.clone());
        let decoder = Decoder::SharedPcm(crate::decode::shared_pcm::SharedPcmDecoder::new(pcm));
        self.load_opened_decoder(source, decoder, None)
    }

    /// Pre-open the next [`AudioSource`] for seamless gapless / crossfade transition.
    pub fn prepare_next_source(&mut self, source: &AudioSource) -> Result<DecodeInfo, EngineError> {
        match source {
            AudioSource::File(path) => self.prepare_next_track(path),
            // Preload a CUE segment the same way as its file: open, seek, and
            // stash the positioned decoder in the same cache the whole-file
            // path uses, so a gapless transition into track 7 of an album
            // behaves like any other. `prepare_next_track` cannot be reused
            // because it opens the path itself; the parts that are *not* about
            // opening (the loudness-metadata prep and the next-track bookkeeping)
            // are reached by first letting it open the file, then replacing the
            // cached decoder with the seeked one.
            AudioSource::CueSegment(seg) => {
                let info = self.prepare_next_track(&seg.path)?;
                let mut decoder = Decoder::open(&seg.path)?;
                if seg.start_frame > 0 {
                    let sample_rate = decoder.info().sample_rate.max(1);
                    let secs = seg.start_frame as f64 / f64::from(sample_rate);
                    decoder.seek(secs as f32)?;
                }
                self.scratch.cached_incoming_decoder = Some(decoder);
                Ok(info)
            }
            AudioSource::Uri(uri) => {
                match crate::decode::resolve_uri(uri).map_err(EngineError::InvalidSource)? {
                    crate::decode::UriTarget::Local(path) => self.prepare_next_track(&path),
                    crate::decode::UriTarget::Remote(url) => {
                        let decoder = crate::decode::stream::open_remote(&url)?;
                        let info = decoder.info().clone();
                        self.scratch.cached_incoming_decoder = Some(decoder);
                        self.loudness_scan.pending_incoming_loudness_metadata =
                            Some(crate::dsp::LoudnessMetadata::default());
                        Ok(info)
                    }
                }
            }
            AudioSource::Memory {
                data,
                extension_hint,
            } => {
                let decoder = Decoder::open_memory(data.clone(), extension_hint.as_deref())?;
                let info = decoder.info().clone();
                self.scratch.cached_incoming_decoder = Some(decoder);
                self.loudness_scan.pending_incoming_loudness_metadata =
                    Some(crate::dsp::LoudnessMetadata::default());
                Ok(info)
            }
            AudioSource::NetworkStream(cfg) => {
                // A network stream has no "next track" in the file sense, but a
                // crossfade partner can still be pre-armed the same way: open
                // its receiver now so the group join and jitter pre-roll happen
                // during the current track rather than mid-transition.
                let decoder = crate::decode::Aes67Decoder::new(cfg.as_ref().clone())
                    .map_err(|e| EngineError::InvalidSource(format!("AES67 stream: {e}")))?;
                let info = decoder.info().clone();
                self.scratch.cached_incoming_decoder = Some(Decoder::Aes67(decoder));
                self.loudness_scan.pending_incoming_loudness_metadata =
                    Some(crate::dsp::LoudnessMetadata::default());
                Ok(info)
            }
            AudioSource::SharedPcm(pcm) => {
                // Pre-open the cursor only; nothing is decoded and no file is
                // touched, so the "preload" is just an `Arc` clone.
                let decoder = Decoder::SharedPcm(crate::decode::shared_pcm::SharedPcmDecoder::new(
                    pcm.clone(),
                ));
                let info = decoder.info().clone();
                self.scratch.cached_incoming_decoder = Some(decoder);
                self.loudness_scan.pending_incoming_loudness_metadata =
                    Some(crate::dsp::LoudnessMetadata::default());
                Ok(info)
            }
        }
    }

    /// Negotiate native-DSD transport with the output backend (§7).
    ///
    /// Returns the negotiated wire format and the byte ring the engine feeds
    /// raw DSD bytes into, or an error describing why native DSD is
    /// unavailable on this device/backend (e.g. no DSD-capable output, or
    /// the device refused every DSD format). The caller records the failure
    /// as an explicit fallback step — never a silent downgrade.
    fn negotiate_native_dsd(
        &mut self,
        decoder: &mut Decoder,
    ) -> Result<
        (
            crate::decode::dsd::DsdWireFormat,
            std::sync::Arc<crate::buffer::DsdByteBuffer>,
        ),
        OutputError,
    > {
        let bit_rate = decoder
            .dsd_bit_rate()
            .ok_or_else(|| OutputError::StreamError("not a DSD source".to_string()))?;
        let channels = decoder.info().channels as u16;
        self.negotiate_native_dsd_for(bit_rate, channels)
    }

    /// Negotiate native-DSD transport for an explicit bit rate and channel
    /// count. Shared by DSD sources (which derive the rate from the decoder)
    /// and PCM→SDM output (which derives it from the SDM rate plan).
    fn negotiate_native_dsd_for(
        &mut self,
        bit_rate: u32,
        channels: u16,
    ) -> Result<
        (
            crate::decode::dsd::DsdWireFormat,
            std::sync::Arc<crate::buffer::DsdByteBuffer>,
        ),
        OutputError,
    > {
        let out_info = {
            let Some(output) = self.audio_output.as_mut() else {
                return Err(OutputError::StreamError(
                    "no output device is active".to_string(),
                ));
            };
            let caps = output.native_dsd_capability_matrix();
            let info = output.output_info();
            (caps, info)
        };
        let (caps, out_info) = out_info;
        let output = self.audio_output.as_mut().unwrap();

        // Guard: native DSD requires an exclusive, bit-perfect transport
        // path. On shared-mode backends (cpal Auto), the DSD capability
        // matrix is always empty — but we surface a per-platform error
        // explaining which exclusive backend to select instead, matching
        // the DoP path's diagnostic contract.
        if caps.is_empty() {
            let hint = if out_info.is_fallback {
                match &out_info.fallback_reason {
                    Some(r) => {
                        format!("the exclusive backend request fell back to a shared device ({r})")
                    }
                    None => {
                        "the exclusive backend request fell back to a shared device".to_string()
                    }
                }
            } else if self.config.output_backend == config::AudioBackend::Auto {
                if cfg!(target_os = "windows") {
                    "select AudioBackend::ExclusiveAsio for native DSD transport \
                     (enable the 'asio-native' or 'asio' feature)"
                        .to_string()
                } else if cfg!(target_os = "linux") {
                    "select AudioBackend::ExclusiveAlsa with a direct hw: device \
                     for native DSD transport"
                        .to_string()
                } else {
                    "switch to an exclusive backend for native DSD transport".to_string()
                }
            } else if self.config.output_backend == config::AudioBackend::ExclusiveAsio
                && !cfg!(feature = "asio")
                && !cfg!(feature = "asio-native")
            {
                "the ASIO backend is not compiled in (enable the 'asio-native' or 'asio' feature)"
                    .to_string()
            } else {
                format!(
                    "backend {:?} does not advertise native DSD transport capabilities",
                    self.config.output_backend
                )
            };
            return Err(OutputError::StreamError(format!(
                "native DSD unavailable: {hint}"
            )));
        }

        // Capability candidates are typed by wire format, rate, and channel
        // constraints. The exact stream is still verified by set_native_dsd;
        // this prevents choosing DSD_U8 merely because it was first in a
        // legacy format list when a DSD128+ endpoint exposes another format.
        let wire_format = caps
            .iter()
            .find(|cap| cap.supports(bit_rate, channels))
            .map(|cap| cap.wire_format)
            .or_else(|| caps.first().map(|cap| cap.wire_format))
            .ok_or_else(|| {
                OutputError::StreamError(format!(
                    "no native DSD transport candidates on backend {:?}",
                    self.config.output_backend
                ))
            })?;

        // Byte ring sized for approximately one second of the source DSD
        // bitstream. Native U8/U16/U32 wire formats carry the same total byte
        // count per channel; the negotiated frame width only changes how the
        // render thread groups those bytes.
        let capacity = (bit_rate as usize)
            .checked_mul(channels as usize)
            .and_then(|bytes| bytes.checked_div(8))
            .unwrap_or(usize::MAX)
            .min(usize::MAX / 2);
        let buffer = std::sync::Arc::new(crate::buffer::DsdByteBuffer::new(capacity.max(65536)));
        let params = crate::output::NativeDsdParams {
            wire_format,
            bit_rate,
            channels,
            buffer: buffer.clone(),
        };

        info!(
            "Native DSD: requesting {} at {} bps / {} ch from backend {:?} (device \"{}\")",
            wire_format.label(),
            bit_rate,
            channels,
            out_info
                .actual_backend
                .unwrap_or(self.config.output_backend),
            out_info.device_name,
        );

        let negotiated = output.set_native_dsd(Some(params))?;
        let format = negotiated.ok_or_else(|| {
            OutputError::StreamError("backend returned no native DSD format".to_string())
        })?;

        info!(
            "Native DSD: backend negotiated {} (requested {})",
            format.label(),
            wire_format.label()
        );

        Ok((format, buffer))
    }

    /// Negotiate a PCM→SDM native-DSD transport.
    ///
    /// Tries every rate the configuration and device allow, highest first
    /// for `Auto`, and returns the first transport the backend actually
    /// accepts. A failure is explicit and leaves the caller free to fall
    /// back to PCM; the backend is returned to PCM mode before the error is
    /// propagated so a failed attempt cannot strand it in DSD mode.
    fn negotiate_sdm(
        &mut self,
        decoder: &Decoder,
    ) -> Result<
        (
            crate::dsp::sdm::SdmRatePlan,
            crate::decode::dsd::DsdWireFormat,
            std::sync::Arc<crate::buffer::DsdByteBuffer>,
        ),
        OutputError,
    > {
        let info = decoder.info();
        let channels = info.channels as u16;
        let supported = {
            let output = self.audio_output.as_ref().ok_or_else(|| {
                OutputError::StreamError("no output device is active".to_string())
            })?;
            let mut rates = Vec::new();
            for cap in output.native_dsd_capability_matrix() {
                rates.extend(cap.bit_rates);
            }
            rates
        };
        let plans = crate::dsp::sdm::candidates(info.sample_rate, self.config.sdm.rate, &supported);
        if plans.is_empty() {
            return Err(OutputError::StreamError(format!(
                "native DSD has no supported rate for a {} Hz PCM source (requested {})",
                info.sample_rate,
                self.config.sdm.rate.label()
            )));
        }

        let mut last_error = None;
        for plan in plans {
            match self.negotiate_native_dsd_for(plan.dsd_rate, channels) {
                Ok((wire_format, buffer)) => return Ok((plan, wire_format, buffer)),
                Err(e) => {
                    warn!(
                        "PCM→SDM: backend refused {} bps ({}); trying the next rate",
                        plan.dsd_rate, e
                    );
                    last_error = Some(e);
                }
            }
        }

        // Do not leave the backend holding a half-opened DSD stream.
        if let Some(output) = self.audio_output.as_mut() {
            let _ = output.set_native_dsd(None);
        }
        Err(last_error.unwrap_or_else(|| {
            OutputError::StreamError("no native DSD rate could be negotiated".to_string())
        }))
    }

    /// Negotiate a **PCM→SDM→DoP** transport: an exclusive 24-bit PCM stream
    /// at `dsd_rate / 16`, into which the modulator's one-bit output is
    /// framed.
    ///
    /// DoP is a stereo framing (one marker shared by the two channels of a
    /// stereo word), and a shared-mode mixer would rewrite the 24-bit words,
    /// so both are hard requirements. Candidate rates are the same clock-family
    /// multiples the native path uses; each is filtered against the device's
    /// advertised PCM rates and then verified by actually reconfiguring the
    /// stream, because a device that lists a rate may still refuse it.
    fn negotiate_sdm_dop(
        &mut self,
        decoder: &Decoder,
    ) -> Result<(crate::dsp::sdm::SdmRatePlan, u32), OutputError> {
        let info = decoder.info();
        if info.channels != 2 {
            return Err(OutputError::StreamError(
                "DoP synthesis is stereo-only".to_string(),
            ));
        }
        let plans = crate::dsp::sdm::candidates(info.sample_rate, self.config.sdm.rate, &[]);
        if plans.is_empty() {
            return Err(OutputError::StreamError(format!(
                "no PCM→DoP rate plan for a {} Hz source (requested {})",
                info.sample_rate,
                self.config.sdm.rate.label()
            )));
        }
        let supported_rates = {
            let output = self.audio_output.as_ref().ok_or_else(|| {
                OutputError::StreamError("no output device is active".to_string())
            })?;
            let out_info = output.output_info();
            if !out_info.access_state.is_bit_perfect() {
                return Err(OutputError::StreamError(format!(
                    "DoP requires an exclusive output — {}",
                    dop_exclusive_reason(&out_info, self.config.output_backend)
                )));
            }
            output.capabilities().sample_rates
        };

        let mut last_error = None;
        for plan in plans {
            let dop_rate = (plan.dsd_rate / 16).max(1);
            if dop_rate < 44_100 {
                continue;
            }
            if !supported_rates.is_empty() && !supported_rates.contains(&dop_rate) {
                last_error = Some(format!(
                    "the output does not advertise the {} Hz DoP container rate",
                    dop_rate
                ));
                continue;
            }
            let Some(output) = self.audio_output.as_mut() else {
                return Err(OutputError::StreamError(
                    "no output device is active".to_string(),
                ));
            };
            match output.reconfigure_sample_format(dop_rate, cpal::SampleFormat::I32) {
                Ok(actual)
                    if actual == dop_rate
                        && output.sample_format() == cpal::SampleFormat::I32
                        && output.output_info().access_state.is_bit_perfect() =>
                {
                    return Ok((plan, dop_rate));
                }
                Ok(actual) => {
                    last_error = Some(format!(
                        "the output settled on {} Hz/{:?} (need {} Hz I32 exclusive)",
                        actual,
                        output.sample_format(),
                        dop_rate
                    ));
                }
                Err(e) => last_error = Some(e.to_string()),
            }
        }
        Err(OutputError::StreamError(last_error.unwrap_or_else(|| {
            "no DoP container rate could be negotiated".to_string()
        })))
    }

    /// Sample-accurate gapless handoff to the next track.
    ///
    /// Unlike [`Self::load_track`] — which rebuilds the resampler and resets
    /// every DSP stage — this preserves the whole pipeline state, most
    /// importantly the final safety limiter's lookahead delay line, so the
    /// outgoing track's last samples flow into the incoming track with no
    /// gap, no drop, and no click. Only the decoder, the playback clock, and
    /// the loudness metadata are swapped.
    ///
    /// When the next track's source rate matches the current one, the running
    /// resampler instance is moved into the new stream unchanged (filter
    /// state and partially-filled input block included). When the rate
    /// differs, the old resampler cannot accept the new track's samples, so
    /// its tail is flushed through the shared limiter — completing the
    /// outgoing track — and a fresh resampler is built for the new ratio.
    ///
    /// Falls back to [`Self::load_track`] only when DSD/DoP handling is
    /// involved (decoder mode selection and output-rate reconfiguration are
    /// too invasive for a state-preserving handoff).
    /// Perform a seamless, gapless handoff using a preloaded [`PreparedTrack`].
    /// Reuses the existing decoder, DSP pipeline, resampler (if matching sample rate),
    /// and limiter lookahead delay buffer without any teardown or audio discontinuity.
    pub(super) fn swap_to_prepared_track(
        &mut self,
        prepared: crate::engine::PreparedTrack,
        #[cfg(feature = "resample")] resampler: &mut Option<GenericResampler>,
        #[cfg(not(feature = "resample"))] _resampler: &mut Option<()>,
    ) -> Result<DecodeInfo, EngineError> {
        let decoder = prepared.decoder;
        let info = decoder.info().clone();
        let source = prepared.source;

        if decoder.is_dsd()
            || self.dsd.dop_active
            || self.dsd.native_dsd_active
            || self.dsd.sdm_active
        {
            return self.load_source(&source);
        }

        #[cfg(feature = "resample")]
        let next_resampler = if info.sample_rate == self.clock.source_sample_rate {
            resampler.take()
        } else {
            self.flush_resampler_tail(resampler);
            let r = recovery::build_resampler(
                self.config.resampler_quality,
                self.config.resampler_filter,
                self.config.resampler_cutoff_ratio,
                info.sample_rate as f32,
                self.output_sample_rate as f32,
                self.speed,
                self.config.precision_mode,
            );
            if r.is_none()
                && (info.sample_rate != self.output_sample_rate || (self.speed - 1.0).abs() > 0.001)
            {
                error!(
                    "Critical: Resampler required ({} Hz -> {} Hz) for gapless handoff \
                     but could not be initialized!",
                    info.sample_rate, self.output_sample_rate
                );
                self.write_playback_info(|pb| {
                    pb.resampler_disabled = true;
                    pb.resampler_failed_fatal = true;
                    pb.set_engine_error(
                        crate::diagnostics::DiagnosticKind::Resampler,
                        "Resampler initialization failed during gapless handoff",
                    );
                });
                return Err(EngineError::Resampler(format!(
                    "Required resampler ({} Hz -> {} Hz) for gapless handoff could \
                     not be initialized; refusing to play at the wrong rate/speed",
                    info.sample_rate, self.output_sample_rate
                )));
            }
            r
        };
        #[cfg(not(feature = "resample"))]
        let next_resampler: Option<()> = _resampler.take();

        self.dsd.dop_active = false;
        self.dsd.dop_rate = 0;
        self.graph.set_dop_bypass(false);
        // DoP bypass ends here: the primary output is PCM again, at the DoP
        // container rate, so the monitor's ratio is reconciled with it.
        self.sync_talkback();

        self.clock.reset_track(info.sample_rate);
        self.analyzer.set_sample_rate(info.sample_rate);
        self.apodizing.reset(info.sample_rate as f32);
        self.duration_secs = info.duration_secs;
        self.recovery.consecutive_decode_errors = 0;
        self.scratch.crossfade_triggered = false;
        self.scratch.pending_chunk = None;
        self.scratch.pending_incoming_chunk = None;

        self.stream = Some(PlaybackStream::Single {
            decoder,
            resampler: next_resampler,
        });

        if let AudioSource::File(ref path) = source {
            let mut loudness_meta = prepared
                .loudness
                .unwrap_or_else(|| crate::decode::extract_loudness_metadata(path));
            if loudness_meta.ebu_r128_loudness.is_none() {
                if let Some(cached) = crate::decode::loudness_cache::lookup(path) {
                    loudness_meta.ebu_r128_loudness = cached.ebu_r128_loudness;
                    loudness_meta.ebu_r128_peak = cached.ebu_r128_peak_dbtp;
                    info!("Loaded cached loudness metadata for {}", path.display());
                }
            }
            self.loudness_scan.current_track_path = Some(path.to_path_buf());
            self.loudness_scan.pending_loudness_metadata = Some(loudness_meta);
            self.graph
                .apply_loudness_metadata_outgoing(Some(loudness_meta));
            self.start_loudness_scan();
        } else {
            self.loudness_scan.current_track_path = None;
            self.loudness_scan.pending_loudness_metadata = prepared.loudness;
            if let Some(meta) = prepared.loudness {
                self.graph.apply_loudness_metadata_outgoing(Some(meta));
            }
        }

        self.stream_ended = false;
        self.current_source = Some(source.clone());
        let current_source = self.current_source.clone();
        let speed = self.speed;
        let dop_active = self.dsd.dop_active;
        let native_dsd_active = self.dsd.native_dsd_active;
        let dsd_transport = self.dsd.dsd_transport_report.actual;
        let dsd_transport_report = self.dsd.dsd_transport_report.clone();

        self.playback_info.rcu(|old| {
            Arc::new(PlaybackInfo {
                duration_secs: info.duration_secs,
                sample_rate: info.sample_rate,
                current_source: current_source.clone(),
                speed,
                dop_active,
                native_dsd_active,
                dsd_transport,
                dsd_transport_report: dsd_transport_report.clone(),
                volume: old.volume,
                state: if old.state == PlaybackState::Stopped {
                    PlaybackState::Paused
                } else {
                    old.state
                },
                ..Default::default()
            })
        });

        self.emit_event(EngineEvent::SourceOpened {
            source: source.clone(),
            sample_rate: info.sample_rate,
            channels: info.channels,
            duration_secs: info.duration_secs,
        });
        self.emit_event(EngineEvent::FormatChanged {
            sample_rate: info.sample_rate,
            channels: info.channels,
        });

        info!(
            "Gapless handoff (prepared): {} Hz, {} ch, {:.1}s (resampler + DSP state preserved)",
            info.sample_rate, info.channels, info.duration_secs
        );
        Ok(info)
    }

    pub(super) fn swap_to_next_track(
        &mut self,
        path: &Path,
        #[cfg(feature = "resample")] resampler: &mut Option<GenericResampler>,
        #[cfg(not(feature = "resample"))] _resampler: &mut Option<()>,
    ) -> Result<DecodeInfo, EngineError> {
        // If the preloader has already prepared this exact file track, use it directly!
        let target_source = AudioSource::File(path.to_path_buf());
        if self.preload.has_prepared_matching(&target_source) {
            if let Some(prepared) = self.preload.take_prepared() {
                return self.swap_to_prepared_track(
                    prepared,
                    #[cfg(feature = "resample")]
                    resampler,
                    #[cfg(not(feature = "resample"))]
                    _resampler,
                );
            }
        }

        // Reuse the decoder pre-opened by `prepare_next_track` if there is
        // one; it was opened for exactly this path.
        let decoder = match self.scratch.cached_incoming_decoder.take() {
            Some(d) => d,
            None => Decoder::open(path)?,
        };
        let info = decoder.info().clone();

        // DSD/DoP needs the full load path, as does a handoff away from an
        // active DoP or native-DSD stream (the output device must be
        // reconfigured back to PCM or re-negotiated for DSD).
        if decoder.is_dsd()
            || self.dsd.dop_active
            || self.dsd.native_dsd_active
            || self.dsd.sdm_active
        {
            return self.load_track(path);
        }

        // Build the next stream's resampler. Same rate: keep the running
        // instance. Different rate: flush its tail (through the shared
        // limiter, so the outgoing track completes) and build a fresh
        // resampler for the new ratio. Everything else — limiter, filters,
        // pending frames, ring buffer — is preserved in both cases.
        #[cfg(feature = "resample")]
        let next_resampler = if info.sample_rate == self.clock.source_sample_rate {
            resampler.take()
        } else {
            self.flush_resampler_tail(resampler);
            let r = recovery::build_resampler(
                self.config.resampler_quality,
                self.config.resampler_filter,
                self.config.resampler_cutoff_ratio,
                info.sample_rate as f32,
                self.output_sample_rate as f32,
                self.speed,
                self.config.precision_mode,
            );
            if r.is_none()
                && (info.sample_rate != self.output_sample_rate || (self.speed - 1.0).abs() > 0.001)
            {
                // Continuing the handoff without the required resampler would
                // play the next track at the wrong rate/pitch (passthrough).
                // Fail the handoff instead — the caller halts playback.
                error!(
                    "Critical: Resampler required ({} Hz -> {} Hz) for gapless handoff \
                     but could not be initialized!",
                    info.sample_rate, self.output_sample_rate
                );
                self.write_playback_info(|pb| {
                    pb.resampler_disabled = true;
                    pb.resampler_failed_fatal = true;
                    pb.set_engine_error(
                        crate::diagnostics::DiagnosticKind::Resampler,
                        "Resampler initialization failed during gapless handoff",
                    );
                });
                return Err(EngineError::Resampler(format!(
                    "Required resampler ({} Hz -> {} Hz) for gapless handoff could \
                     not be initialized; refusing to play at the wrong rate/speed",
                    info.sample_rate, self.output_sample_rate
                )));
            }
            r
        };
        #[cfg(not(feature = "resample"))]
        let next_resampler: Option<()> = _resampler.take();

        self.dsd.dop_active = false;
        self.dsd.dop_rate = 0;
        self.graph.set_dop_bypass(false);
        // DoP bypass ends here: the primary output is PCM again, at the DoP
        // container rate, so the monitor's ratio is reconciled with it.
        self.sync_talkback();

        self.clock.reset_track(info.sample_rate);
        self.analyzer.set_sample_rate(info.sample_rate);
        self.apodizing.reset(info.sample_rate as f32);
        self.duration_secs = info.duration_secs;
        self.recovery.consecutive_decode_errors = 0;
        self.scratch.crossfade_triggered = false;
        self.scratch.pending_chunk = None;
        self.scratch.pending_incoming_chunk = None;

        self.stream = Some(PlaybackStream::Single {
            decoder,
            resampler: next_resampler,
        });

        // Loudness for the incoming track: reuse what `prepare_next_track`
        // prepared (tags + cached scan + background scan result), falling
        // back to extracting from the file.
        let mut loudness_meta = self
            .loudness_scan
            .pending_incoming_loudness_metadata
            .take()
            .unwrap_or_else(|| crate::decode::extract_loudness_metadata(path));
        if loudness_meta.ebu_r128_loudness.is_none() {
            if let Some(cached) = crate::decode::loudness_cache::lookup(path) {
                loudness_meta.ebu_r128_loudness = cached.ebu_r128_loudness;
                loudness_meta.ebu_r128_peak = cached.ebu_r128_peak_dbtp;
                info!("Loaded cached loudness metadata for {}", path.display());
            }
        }
        self.loudness_scan.current_track_path = Some(path.to_path_buf());
        self.loudness_scan.pending_loudness_metadata = Some(loudness_meta);
        self.graph
            .apply_loudness_metadata_outgoing(Some(loudness_meta));
        self.start_loudness_scan();

        self.stream_ended = false;

        self.current_source = Some(AudioSource::File(path.to_path_buf()));
        let current_source = self.current_source.clone();
        let speed = self.speed;
        let dop_active = self.dsd.dop_active;
        let native_dsd_active = self.dsd.native_dsd_active;
        let dsd_transport = self.dsd.dsd_transport_report.actual;
        let dsd_transport_report = self.dsd.dsd_transport_report.clone();
        self.playback_info.rcu(|old| {
            Arc::new(PlaybackInfo {
                duration_secs: info.duration_secs,
                sample_rate: info.sample_rate,
                current_source: current_source.clone(),
                speed,
                dop_active,
                native_dsd_active,
                dsd_transport,
                dsd_transport_report: dsd_transport_report.clone(),
                // Preserve fields that survive a track load
                volume: old.volume,
                state: if old.state == PlaybackState::Stopped {
                    PlaybackState::Paused
                } else {
                    old.state
                },

                ..Default::default()
            })
        });

        self.emit_event(EngineEvent::SourceOpened {
            source: AudioSource::File(path.to_path_buf()),
            sample_rate: info.sample_rate,
            channels: info.channels,
            duration_secs: info.duration_secs,
        });
        self.emit_event(EngineEvent::FormatChanged {
            sample_rate: info.sample_rate,
            channels: info.channels,
        });

        info!(
            "Gapless handoff: {} Hz, {} ch, {:.1}s (resampler + DSP state preserved)",
            info.sample_rate, info.channels, info.duration_secs
        );
        Ok(info)
    }
}
