//! Engine construction, configuration detection, command dispatch, and handle creation.

use std::{
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use arc_swap::ArcSwap;
use crossbeam::channel::{self, Sender};
use log::{info, warn};

use config::EngineConfig;

#[cfg(feature = "audio-output")]
use crate::output::DeviceMonitor;
use crate::{
    buffer::{EngineCommand, FixedFrameBuffer, PlaybackInfo, DEFAULT_SAMPLE_RATE},
    dsp::{graph2::prod::Graph2Engine, GraphicEq},
    playlist::Playlist,
    sink::SampleSink,
};

use super::{
    AudioClock, AudioEngine, DsdTransportState, EngineError, EngineHandle, EngineScratch,
    EngineTelemetry, LoudnessScanState, RecoveryState,
};

impl AudioEngine {
    /// Create a new audio engine with the default (DAC) sample sink.
    pub fn new(config: EngineConfig) -> Result<Self, EngineError> {
        let buffer_frames = config.effective_output_buffer_frames();
        let output_buffer = Arc::new(
            FixedFrameBuffer::new(buffer_frames)
                .map_err(|e| EngineError::Config(format!("Output buffer: {}", e)))?,
        );
        let sample_sink = Box::new(crate::sink::DacSink::new(Arc::clone(&output_buffer)));
        Self::new_inner(config, output_buffer, sample_sink)
    }

    /// Create an engine with a custom [`SampleSink`].
    ///
    /// Use [`NoopSink`](crate::sink::NoopSink) for headless/analyzer mode
    /// (no output device needed). Use [`VecSink`](crate::sink::VecSink) to
    /// collect samples for off-line analysis. Implement your own for
    /// network broadcast, shared-memory visualization, or disk writing.
    ///
    /// When the sink is not a [`DacSink`](crate::sink::DacSink), no output
    /// backend can be started — the engine runs decode-and-analyze without
    /// driving a DAC.
    pub fn with_sink(config: EngineConfig, sink: Box<dyn SampleSink>) -> Result<Self, EngineError> {
        let buffer_frames = config.effective_output_buffer_frames();
        let output_buffer = Arc::new(
            FixedFrameBuffer::new(buffer_frames)
                .map_err(|e| EngineError::Config(format!("Output buffer: {}", e)))?,
        );
        Self::new_inner(config, output_buffer, sink)
    }

    /// Shared constructor body.
    fn new_inner(
        mut config: EngineConfig,
        output_buffer: Arc<FixedFrameBuffer>,
        sample_sink: Box<dyn SampleSink>,
    ) -> Result<Self, EngineError> {
        let _ = crate::buffer::enable_flush_zero_denormals_on_current_thread();
        // Validate before anything is built. A config error is a value the
        // engine cannot honour at all — `mix_slots < 2` has no valid graph,
        // a non-finite trim gain would be clamped into something the caller
        // never asked for — so the honest response is to refuse the config
        // rather than build a graph whose behaviour differs from the file
        // that produced it. Warnings are logged and retained on the engine
        // for the host to read back via `EngineHandle::config_validation`.
        let config_validation = config.validate();
        for issue in &config_validation.issues {
            match issue.severity {
                config::ConfigSeverity::Error => {
                    return Err(EngineError::Config(format!(
                        "config validation [{}]: {}",
                        issue.kind.code(),
                        issue.message
                    )));
                }
                config::ConfigSeverity::Warning => {
                    log::warn!(
                        "config validation [{}]: {}",
                        issue.kind.code(),
                        issue.message
                    );
                }
            }
        }
        // The output converters are built on their own threads at stream open;
        // publish the configured shaper before any of them can be constructed.
        crate::dsp::dither::set_configured_noise_shaper(config.noise_shaper);
        let (cmd_tx, cmd_rx) = channel::bounded(256);
        let (event_tx, event_rx) = channel::bounded(256);
        #[cfg(feature = "audio-output")]
        let (output_event_tx, output_event_rx) = channel::bounded(64);
        let output_sample_rate = DEFAULT_SAMPLE_RATE;
        let configured_endpoints = config.endpoints.clone();
        // Restore the hand-tuned DSP state (limiter, EQ preset library, last
        // output device/backend) over the configured defaults BEFORE the graph
        // is built, so the restored values are the ones the plan is compiled
        // from rather than a later mutation. Added in 0.9.0 — the store existed
        // and was exported since 0.2.0 but nothing ever constructed one, so
        // these settings were lost on every restart.
        //
        // Precedence is explicit-beats-remembered, field by field: a value the
        // host (or a `--config` file) actually chose wins, and a remembered
        // value only fills in where the caller left the default. Comparing
        // against the *default* rather than "is it set" is what makes that
        // work for the limiter, whose config carries a full struct of values
        // rather than an `Option`.
        let mut dsp_persistence = crate::engine::dsp_persistence::DspStateStore::new();
        if let Some(state) = dsp_persistence.restore() {
            if config.output_device.is_none() {
                config.output_device = state.output_device.clone();
            }
            if config.output_backend == config::AudioBackend::Auto {
                config.output_backend = state.output_backend;
            }
            if config.limiter == config::LimiterConfig::default() {
                config.limiter = state.limiter.clone();
            }
            if config.eq.presets.is_empty() {
                config.eq.presets = state.eq_presets.clone();
            }
        }
        let mut graph = Graph2Engine::from_config(&config, output_sample_rate as f32);
        // Restore the last session's active spatial scene (screen,
        // room, listener, enable) over the configured defaults. Best-effort
        // — a missing/corrupt auto-save simply keeps the config defaults.
        let mut spatial_persistence =
            crate::engine::spatial_persistence::SpatialPersistence::from_config(&config);
        spatial_persistence.restore(&mut graph, output_sample_rate as f32);
        let graphic_eq = GraphicEq::from_config(&config.graphic_eq);
        let info = PlaybackInfo {
            sample_rate: output_sample_rate,
            ..Default::default()
        };
        let clock = AudioClock::new(DEFAULT_SAMPLE_RATE);
        #[cfg(feature = "audio-output")]
        let device_monitor = DeviceMonitor::new(config.output_backend, Duration::from_millis(1500));

        let mut engine = Self {
            output_buffer,
            sample_sink,
            cmd_tx,
            cmd_rx,
            wake: crate::engine::EngineWake::new(),
            playback_info: Arc::new(ArcSwap::new(Arc::new(info))),
            running: Arc::new(AtomicBool::new(false)),
            audio_output: None,
            graph,
            // No authority until a host installs one. A standalone integrator
            // gets exactly the behaviour this library had before governance
            // existed; an integrator that has a budget installs one.
            governor: None,
            graphic_eq,
            output_profile: None,
            stream: None,
            playlist: Playlist::new(),
            analyzer: Arc::new(crate::dsp::AudioAnalyzer::new_default()),
            apodizing: crate::dsp::apodizing::ApodizingDetector::new(output_sample_rate as f32),
            config,
            config_validation,
            duration_secs: 0.0,
            output_sample_rate,
            speed: 1.0,
            clock,
            current_source: None,
            stream_ended: false,
            event_tx,
            event_rx,
            #[cfg(feature = "audio-output")]
            output_event_tx,
            #[cfg(feature = "audio-output")]
            output_event_rx,
            #[cfg(feature = "audio-output")]
            device_monitor,
            capture: None,
            measurement: None,

            telemetry: EngineTelemetry::default(),
            dsd: DsdTransportState::default(),
            loudness_scan: LoudnessScanState::default(),
            recovery: RecoveryState::default(),
            dac_warmup_frames_remaining: 0,
            scratch: EngineScratch::default(),
            spatial_persistence,
            dsp_persistence,
            lanes: Vec::new(),
            #[cfg(feature = "audio-output")]
            endpoints: Vec::new(),
            #[cfg(feature = "audio-output")]
            endpoint_configs: configured_endpoints,
            #[cfg(feature = "audio-output")]
            endpoint_dropped_frames: std::sync::atomic::AtomicU64::new(0),
            preload: super::PreloadManager::new(),
            track_cache: crate::track_cache::TrackCache::default(),
            meters: Arc::new(crate::dsp::meters::ProfessionalMeters::new(
                output_sample_rate,
                2,
            )),
            graph_build: Arc::new(super::GraphBuildStats::default()),
            clap_view: Arc::new(ArcSwap::from_pointee(
                crate::dsp::clap_rack::ClapRackView::default(),
            )),
            clap_state: Arc::new(std::sync::Mutex::new(None)),
        };
        // Let the graph mirror its rebuild cost into the block the handles
        // read. Done after construction because the graph is built inside the
        // struct literal.
        let build = Arc::clone(&engine.graph_build);
        engine
            .graph
            .with_graph(|g| g.set_build_stats(Arc::clone(&build)));
        Ok(engine.with_initial_settings())
    }

    /// Publish a correct settings snapshot immediately, before the first tick.
    ///
    /// The telemetry cadence that normally refreshes `PlaybackInfo::settings`
    /// is gated at two seconds, which would leave a freshly-constructed engine
    /// advertising empty EQ and compressor band lists — and a UI would render
    /// that as "this engine has no EQ bands", not "this engine has not polled
    /// yet". Seeding the snapshot here means the very first read a host makes
    /// is already truthful.
    fn with_initial_settings(self) -> Self {
        // `ArcSwap::rcu` may re-invoke the closure if the compare-exchange
        // loses a race, so the closure must be idempotent: clone inside it
        // rather than moving out of a captured binding. This runs exactly
        // once per engine, so the clone is free in practice.
        let settings = Arc::new(self.snapshot_settings());
        self.playback_info.rcu(|old| {
            let mut next: crate::buffer::PlaybackInfo = old.as_ref().clone();
            next.settings = Arc::clone(&settings);
            Arc::new(next)
        });
        self
    }

    /// Convenience constructor using the default configuration.
    pub fn new_default() -> Result<Self, EngineError> {
        Self::new(EngineConfig::default())
    }

    #[allow(dead_code)]
    pub(super) fn detect_output_sample_rate() -> Option<u32> {
        #[cfg(test)]
        {
            // Avoid querying OS audio drivers during unit tests to prevent WASAPI/COM access violations on Windows CI runners.
            None
        }
        #[cfg(not(test))]
        {
            #[cfg(feature = "audio-output")]
            {
                use cpal::traits::{DeviceTrait, HostTrait};
                let host = cpal::default_host();
                let device = host.default_output_device()?;
                let default_config = device.default_output_config().ok()?;
                Some(default_config.sample_rate())
            }
            #[cfg(not(feature = "audio-output"))]
            {
                None
            }
        }
    }

    /// Deliver one processed interleaved block to the sample sink.
    ///
    /// Returns the number of **frames** accepted. The engine retries
    /// unwritten tail frames on the next tick. This is the only write
    /// path from the decode loop to the output — every stereo interleaved
    /// and multichannel interleaved push routes through here. It also feeds
    /// the real-time analyzer and professional meters.
    #[inline]
    pub(crate) fn push_to_sink(&mut self, samples: &[f32], channels: usize) -> usize {
        self.analyzer.update(samples, channels);
        if self.meters.is_enabled() {
            self.meters.process_interleaved(samples, channels);
        }
        if self.dsd.sdm_active {
            return self.push_sdm(samples, channels);
        }
        self.sample_sink.push_interleaved(samples, channels)
    }

    /// Arm the configured DAC warmup for the stream that is opening.
    ///
    /// Native DSD, DoP and PCM→SDM bypass the PCM sink path, so there is no
    /// PCM warmup to send; the counter stays at zero for those transports.
    pub(crate) fn arm_dac_warmup(&mut self) {
        let bypassed = self.dsd.native_dsd_active || self.dsd.dop_active || self.dsd.sdm_active;
        if self.config.dac_warmup_ms == 0 || bypassed {
            self.dac_warmup_frames_remaining = 0;
            return;
        }
        let rate = u64::from(self.output_sample_rate.max(1));
        self.dac_warmup_frames_remaining =
            (u64::from(self.config.dac_warmup_ms) * rate).div_ceil(1000);
        if self.dac_warmup_frames_remaining > 0 {
            info!(
                "DAC warmup armed: {} ms ({} frames at {} Hz)",
                self.config.dac_warmup_ms, self.dac_warmup_frames_remaining, rate
            );
        }
    }

    /// Send the next slice of DAC warmup silence.
    ///
    /// Returns `true` while the warmup is still active; the caller must not
    /// decode into the sink until it returns `false`, otherwise music would
    /// interleave with the warmup zeros.
    pub(crate) fn pump_dac_warmup(&mut self) -> bool {
        if self.dac_warmup_frames_remaining == 0 {
            return false;
        }
        if self.dsd.native_dsd_active || self.dsd.dop_active || self.dsd.sdm_active {
            // A DSD transport owns the sink now: the PCM warmup cannot run.
            self.dac_warmup_frames_remaining = 0;
            return false;
        }
        let channels = self
            .audio_output
            .as_ref()
            .map(|output| output.output_info().channels as usize)
            .unwrap_or(2)
            .max(1);
        let rate = u64::from(self.output_sample_rate.max(1));
        // At most ~20 ms per tick; sink/ring backpressure paces the rest.
        let max_frames = (rate / 50).max(64);
        let frames = self.dac_warmup_frames_remaining.min(max_frames) as usize;
        let zeros = vec![0.0f32; frames * channels];
        let written = self.push_to_sink(&zeros, channels);
        self.dac_warmup_frames_remaining = self
            .dac_warmup_frames_remaining
            .saturating_sub(written as u64);
        self.dac_warmup_frames_remaining > 0
    }

    /// Route a working-rate PCM block through the PCM→SDM stage and into the
    /// native-DSD byte ring.
    ///
    /// Backpressure is honoured in input frames: the stage only advances for
    /// frames whose packed DSD words fit in the ring, and the caller retries
    /// the rest. The free-space check reserves one whole wire frame so a
    /// partial word carried over from the previous block cannot overflow.
    #[inline]
    fn push_sdm(&mut self, samples: &[f32], channels: usize) -> usize {
        let Some(stage) = self.dsd.sdm_output.as_mut() else {
            return 0;
        };
        let Some(buffer) = self.dsd.dsd_byte_buffer.as_ref() else {
            return 0;
        };
        if channels == 0 || channels != stage.channels() {
            return 0;
        }
        let frames = samples.len() / channels;
        if frames == 0 {
            return 0;
        }
        let bytes_per_frame = stage.bytes_per_input_frame();
        if bytes_per_frame == 0 {
            return 0;
        }
        let frame_width = channels * stage.wire().bytes_per_word();
        let free = buffer.free_bytes().saturating_sub(frame_width);
        let max_frames = free / bytes_per_frame;
        let n = frames.min(max_frames);
        if n == 0 {
            return 0;
        }
        let block = stage.process_block(&samples[..n * channels], &mut self.dsd.dsd_pack_scratch);
        let pushed = buffer.push_frames(
            &self.dsd.dsd_pack_scratch[..block.output_bytes],
            frame_width,
        );
        // The free-space pre-check reserves a whole frame, so the consumer
        // (which only drains) cannot make this fail. If it ever does, the
        // stage has advanced for bytes that were dropped — surface it in
        // debug builds rather than hiding it.
        debug_assert!(
            block.output_bytes == 0 || pushed > 0,
            "native DSD ring rejected a pre-checked block"
        );
        n
    }

    #[cfg(feature = "audio-output")]
    pub fn endpoint_dropped_frames(&self) -> u64 {
        self.endpoint_dropped_frames
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Fan out a processed block to all configured physical endpoint workers.
    /// Each endpoint receives its own copy and applies its own gain/enabled
    /// policy before its private output ring is written.
    #[cfg(feature = "audio-output")]
    /// Deliver one processed block independently to every enabled endpoint.
    /// Endpoint backpressure is local: a short write increments only that
    /// endpoint's drop counter and never changes primary-sink delivery.
    pub(crate) fn push_to_endpoints(&mut self, samples: &[f32], channels: usize) {
        let frames = samples.len() / channels.max(1);
        for endpoint in &mut self.endpoints {
            if !endpoint.config().enabled {
                continue;
            }
            // Each worker applies its own gain and (for rate-mismatched
            // endpoints) resamples the master block into its rate domain,
            // with optional clock-drift correction of the ratio.
            let accepted = endpoint.push_interleaved(samples, channels);
            self.endpoint_dropped_frames.fetch_add(
                frames.saturating_sub(accepted) as u64,
                std::sync::atomic::Ordering::Relaxed,
            );
        }
    }

    #[cfg(feature = "audio-output")]
    pub(crate) fn reset_endpoints(&self) {
        for endpoint in &self.endpoints {
            endpoint.reset();
        }
    }

    /// Access the shared real-time analyzer (levels + spectrum).
    pub fn analyzer(&self) -> Arc<crate::dsp::AudioAnalyzer> {
        Arc::clone(&self.analyzer)
    }

    /// The wake handle a tick pump should idle on.
    ///
    /// Deliberately obtainable *without* locking the engine: the pump needs it
    /// precisely when it must **not** hold the engine's lock. Pair it with
    /// [`AudioEngine::tick`]:
    ///
    /// ```no_run
    /// # use std::sync::{Arc, Mutex};
    /// # use std::time::Duration;
    /// # fn drive(engine: Arc<Mutex<Option<engine::engine::AudioEngine>>>) {
    /// let wake = engine.lock().unwrap().as_ref().unwrap().wake_handle();
    /// loop {
    ///     // Scoped: the guard is released before the wait.
    ///     if let Some(engine) = engine.lock().unwrap().as_mut() {
    ///         engine.tick();
    ///     }
    ///     wake.wait(Duration::from_millis(5));
    /// }
    /// # }
    /// ```
    pub fn wake_handle(&self) -> Arc<crate::engine::EngineWake> {
        Arc::clone(&self.wake)
    }

    pub fn send_command(&self, cmd: EngineCommand) {
        let is_critical = matches!(
            cmd,
            EngineCommand::Play
                | EngineCommand::Pause
                | EngineCommand::Stop
                | EngineCommand::Shutdown
        );
        let timeout = if is_critical {
            std::time::Duration::from_secs(5)
        } else {
            std::time::Duration::from_millis(100)
        };
        match self.cmd_tx.send_timeout(cmd, timeout) {
            Ok(()) => {}
            Err(crossbeam::channel::SendTimeoutError::Timeout(cmd)) => {
                // For idempotent state-setting commands (volume, seek, eq, balance),
                // attempt a fallback non-blocking send before giving up.
                if self.cmd_tx.try_send(cmd.clone()).is_err() {
                    warn!(
                        "Engine command channel saturated; dropped non-critical command: {:?}",
                        cmd
                    );
                }
            }
            Err(crossbeam::channel::SendTimeoutError::Disconnected(_)) => {
                warn!("Engine command channel disconnected; command dropped");
            }
        }
        // Notify unconditionally rather than only on the `Ok` arm: the fallback
        // send above may have succeeded, and a wake for a command that was
        // dropped costs one idle tick while a missing wake costs a full period
        // of latency on every command.
        self.wake.notify();
    }

    /// A bare command sender, for callers that want the channel itself.
    ///
    /// Commands sent here do **not** wake a [`EngineWake`](crate::engine::EngineWake)
    /// pump, so they are applied on the pump's next idle timeout rather than
    /// immediately. Use [`AudioEngine::send_command`] or
    /// [`EngineHandle::send_command`](crate::engine::EngineHandle::send_command)
    /// when latency matters — the channel's convenience is precisely what it
    /// gives up.
    pub fn send_command_channel(&mut self) -> Sender<EngineCommand> {
        self.cmd_tx.clone()
    }

    /// Create a safe, decoupled [`EngineHandle`] for host applications and controllers.
    pub fn handle(&self) -> EngineHandle {
        EngineHandle::new(
            self.cmd_tx.clone(),
            Arc::clone(&self.playback_info),
            self.event_rx.clone(),
            #[cfg(feature = "audio-output")]
            self.output_event_rx.clone(),
            Arc::clone(&self.analyzer),
            Arc::clone(&self.meters),
            Arc::clone(&self.wake),
            Arc::clone(&self.graph_build),
            Arc::clone(&self.clap_view),
            Arc::clone(&self.clap_state),
        )
    }
}
