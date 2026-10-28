//! Room/headphone correction — the correction node: a per-channel bank of partitioned
//! [`ConvolutionEngine`]s running as an `AllChannels`-scoped plan step placed
//! **post-aux / pre-EQ** (`mix → aux → correction → eq → …`), so user EQ
//! stacks on the corrected response and the node's declared latency flows
//! into the graph's latency metadata (`position_secs_compensated`).
//!
//! The node only ever processes **pre-rendered** correction IRs
//! ([`CorrectionIrSet`], produced by the correction derivation chain on the control
//! Thread). Nothing in the measurement-to-correction measurement machinery is on the hot path:
//! the IR set is loaded once (control path — allocation is fine there) and
//! the engines then run the same allocation-free partitioned-FFT contract as
//! the existing convolution node.
//!
//! Realtime contract: disabled, depth 0, or no IR loaded = the process step
//! returns without touching a sample — bit-exact. One `ConvolutionEngine`
//! per channel (each loaded with a mono IR — the channel's own correction
//! IR), so a multichannel master corrects every plane the set covers and
//! passes the rest through untouched.

use crate::buffer::MAX_AUDIO_BLOCK_FRAMES;
use crate::dsp::{
    convolution::ConvolutionEngine,
    correction::{
        derive_correction_ir, derive_params_from_config, CorrectionIrSet, IrConditioner, PhaseMode,
    },
    graph2::prod::arena::node::DspNode,
    pipeline::{DspStageCapability, StageChannelSupport, StagePrecision},
};

/// SNR assumed for config-driven derivation (`ir_paths` at config-apply
/// time). A live measurement reports its own `snr_db`; a config file has no
/// measurement, so boosts are trusted up to the clamp.
const CONFIG_DRIVEN_SNR_DB: f64 = 60.0;

/// Derivation IR render length for config-driven IRs: the conditioned
/// measurement rounded up to a power of two, clamped to a sane correction
/// budget (128 ms – 682 ms @ 48 kHz).
const MIN_IR_LEN: usize = 1024;
const MAX_IR_LEN: usize = 32_768;

/// Telemetry snapshot of the correction node, published into
/// `PlaybackInfo.correction` on the engine's telemetry cadence.
#[derive(Debug, Clone, PartialEq)]
pub struct CorrectionNodeInfo {
    /// Enabled (live toggle / config).
    pub enabled: bool,
    /// Wet/dry depth in [0, 1].
    pub depth: f32,
    /// Phase mode the loaded IR set was rendered in (`None` = no IR).
    pub phase_mode: Option<PhaseMode>,
    /// Length of the loaded IRs (samples); 0 = none.
    pub ir_len_samples: usize,
    /// Declared correction latency (ms) — the phase mode's group delay,
    /// 0 when inactive.
    pub latency_ms: f32,
    /// Peak gain the loaded set applies (dB), from its per-channel max.
    pub max_gain_db: f32,
}

/// Telemetry snapshot of the device-specific DAC correction bank. Kept
/// separate from [`CorrectionNodeInfo`] so a UI never conflates a room /
/// headphone measurement with a converter profile.
#[derive(Debug, Clone, PartialEq)]
pub struct DacCorrectionNodeInfo {
    /// The config asked for DAC correction.
    pub enabled: bool,
    /// The bank is loaded and processing this block.
    pub active: bool,
    /// Matched profile device key, when one was selected.
    pub profile: Option<String>,
    /// Wet/dry depth in [0, 1].
    pub depth: f32,
    /// Declared latency already present in the profile IRs (ms).
    pub latency_ms: f32,
    /// Length of the loaded IRs (samples); 0 when inactive.
    pub ir_len_samples: usize,
    /// Peak gain of the loaded set (dB).
    pub max_gain_db: f32,
    /// Why DAC correction is not active, when it was requested but could not
    /// run. `None` when disabled or active.
    pub reason: Option<String>,
}

/// The correction stage: per-channel partitioned convolution bank.
pub struct CorrectionNode {
    /// One engine per channel of the loaded IR set. Each holds a mono IR
    /// (the channel's own correction IR), so the node corrects every plane
    /// the set covers.
    engines: Vec<ConvolutionEngine>,
    /// Enabled (live toggle / config). Disabled = the step is skipped,
    /// bit-exact.
    enabled: bool,
    /// Wet/dry depth in [0, 1] (1.0 = fully corrected).
    depth: f32,
    /// Declared group delay of the loaded set (samples) — the phase mode's
    /// latency. 0 when inactive.
    delay_samples: usize,
    /// Length of the loaded IRs (samples).
    ir_len_samples: usize,
    /// Phase mode of the loaded set.
    phase_mode: Option<PhaseMode>,
    /// Peak gain of the loaded set (dB).
    max_gain_db: f32,
    /// Device-specific DAC correction, deliberately a second bank: room
    /// correction and converter correction can both be active and are
    /// reported separately.
    dac_engines: Vec<ConvolutionEngine>,
    dac_enabled: bool,
    dac_depth: f32,
    dac_delay_samples: usize,
    dac_ir_len_samples: usize,
    dac_phase_mode: Option<PhaseMode>,
    dac_max_gain_db: f32,
    dac_profile: Option<String>,
    dac_reason: Option<String>,
    /// Discard plane for the mono per-channel convolution (the engine writes
    /// both L and R; R is thrown away).
    scratch: Vec<f32>,
    scratch_f64: Vec<f64>,
    sample_rate: f32,
}

impl CorrectionNode {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            engines: Vec::new(),
            enabled: false,
            depth: 1.0,
            delay_samples: 0,
            ir_len_samples: 0,
            phase_mode: None,
            max_gain_db: 0.0,
            dac_engines: Vec::new(),
            dac_enabled: false,
            dac_depth: 1.0,
            dac_delay_samples: 0,
            dac_ir_len_samples: 0,
            dac_phase_mode: None,
            dac_max_gain_db: 0.0,
            dac_profile: None,
            dac_reason: None,
            scratch: vec![0.0; MAX_AUDIO_BLOCK_FRAMES],
            scratch_f64: vec![0.0; MAX_AUDIO_BLOCK_FRAMES],
            sample_rate,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn depth(&self) -> f32 {
        self.depth
    }

    /// Whether the node will process this block (enabled + an IR set
    /// loaded + no stale-rate reload pending).
    pub fn processing(&self) -> bool {
        self.enabled
            && !self.engines.is_empty()
            && self
                .engines
                .iter()
                .all(|e| e.is_ir_loaded() && !e.ir_needs_reload())
    }

    /// Whether the DAC bank will process this block.
    pub fn dac_processing(&self) -> bool {
        self.dac_enabled
            && !self.dac_engines.is_empty()
            && self
                .dac_engines
                .iter()
                .all(|e| e.is_ir_loaded() && !e.ir_needs_reload())
    }

    /// Telemetry for the DAC bank, distinct from the room/headphone bank.
    pub fn dac_info(&self) -> DacCorrectionNodeInfo {
        DacCorrectionNodeInfo {
            enabled: self.dac_enabled,
            active: self.dac_processing(),
            profile: self.dac_profile.clone(),
            depth: self.dac_depth,
            latency_ms: if self.dac_processing() {
                self.dac_latency_ms(self.sample_rate)
            } else {
                0.0
            },
            ir_len_samples: if self.dac_processing() {
                self.dac_ir_len_samples
            } else {
                0
            },
            max_gain_db: if self.dac_processing() {
                self.dac_max_gain_db
            } else {
                0.0
            },
            reason: self.dac_reason.clone(),
        }
    }

    /// Select and load a device-specific pre-rendered DAC profile.
    ///
    /// Control path only: this reads the profile WAVs and rebuilds the
    /// partitioned convolvers. A profile is matched by a case-insensitive
    /// substring of the device name and by output rate (`0` = any). A missing
    /// file, a rate mismatch, or no matching profile leaves the bank inactive
    /// and publishes a reason in [`DacCorrectionNodeInfo::reason`] — it never
    /// silently passes the dry signal while claiming to correct the DAC.
    pub fn apply_dac_config(
        &mut self,
        cfg: &config::DacCorrectionConfig,
        device: Option<&str>,
        sample_rate: f32,
    ) {
        self.sample_rate = sample_rate;
        self.dac_enabled = cfg.enabled;
        if !cfg.enabled {
            self.clear_dac();
            self.dac_reason = None;
            return;
        }

        let device = device.unwrap_or("");
        let device_lower = device.to_lowercase();
        let profile = cfg.profiles.iter().find(|profile| {
            let rate_matches =
                profile.sample_rate == 0 || profile.sample_rate == sample_rate.round() as u32;
            let device_matches = profile.dac.is_empty()
                || profile.dac == "*"
                || device_lower.contains(&profile.dac.to_lowercase());
            rate_matches && device_matches
        });

        let Some(profile) = profile else {
            self.clear_dac();
            self.set_dac_reason(format!(
                "no DAC correction profile matched device {device:?} at {sample_rate:.0} Hz"
            ));
            return;
        };
        if profile.ir_paths.is_empty() {
            self.clear_dac();
            self.set_dac_reason(format!("DAC profile {:?} has no IR paths", profile.dac));
            return;
        }

        match load_dac_profile_ir(profile, sample_rate) {
            Ok(set) => match self.load_dac_set(&profile.dac, &set, sample_rate) {
                Ok(()) => {
                    self.dac_depth = profile.depth.clamp(0.0, 1.0);
                    for engine in &mut self.dac_engines {
                        engine.set_wet_mix(self.dac_depth);
                    }
                    self.dac_reason = None;
                    log::info!(
                        "DAC correction: loaded profile {:?} ({} IR(s), {} samples, {} ms declared latency)",
                        profile.dac,
                        self.dac_engines.len(),
                        self.dac_ir_len_samples,
                        profile.latency_ms
                    );
                }
                Err(e) => {
                    self.clear_dac();
                    self.set_dac_reason(e);
                }
            },
            Err(e) => {
                self.clear_dac();
                self.set_dac_reason(e);
            }
        }
    }

    /// Load a pre-rendered DAC correction set into the second bank.
    pub fn load_dac_set(
        &mut self,
        profile: &str,
        set: &CorrectionIrSet,
        sample_rate: f32,
    ) -> Result<(), String> {
        self.sample_rate = sample_rate;
        if set.channels.is_empty() {
            return Err("DAC correction IR set has no channels".to_string());
        }
        let mut engines = Vec::with_capacity(set.channels.len());
        for ch in &set.channels {
            if ch.len() > MAX_IR_LEN {
                return Err(format!(
                    "DAC correction IR of {} samples exceeds the {} limit",
                    ch.len(),
                    MAX_IR_LEN
                ));
            }
            let mut engine = ConvolutionEngine::new(sample_rate, MAX_IR_LEN.min(ch.len()).max(64));
            engine.set_wet_mix(self.dac_depth);
            let pairs: Vec<(f64, f64)> = ch.iter().map(|&s| (s, s)).collect();
            engine
                .load_ir_from_samples_f64(&pairs)
                .map_err(|e| format!("DAC partitioned-convolution load failed: {e}"))?;
            engine.set_enabled(true);
            engines.push(engine);
        }
        self.dac_engines = engines;
        self.dac_delay_samples = set.delay_samples.round().max(0.0) as usize;
        self.dac_ir_len_samples = set.channels[0].len();
        self.dac_phase_mode = Some(set.phase_mode);
        self.dac_max_gain_db = set
            .channels
            .iter()
            .map(|ch| {
                ch.iter()
                    .map(|s| 20.0 * s.abs().max(1e-12).log10())
                    .fold(f64::NEG_INFINITY, f64::max)
            })
            .fold(f64::NEG_INFINITY, f64::max)
            .max(0.0) as f32;
        self.dac_profile = Some(profile.to_string());
        Ok(())
    }

    fn dac_latency_ms(&self, sample_rate: f32) -> f32 {
        if !self.dac_processing() {
            return 0.0;
        }
        let engine_latency = self
            .dac_engines
            .iter()
            .map(|e| e.latency_samples())
            .max()
            .unwrap_or(0);
        ((self.dac_delay_samples + engine_latency) as f32) * 1_000.0 / sample_rate.max(1.0)
    }

    fn clear_dac(&mut self) {
        self.dac_engines.clear();
        self.dac_delay_samples = 0;
        self.dac_ir_len_samples = 0;
        self.dac_phase_mode = None;
        self.dac_max_gain_db = 0.0;
        self.dac_profile = None;
    }

    fn set_dac_reason(&mut self, reason: String) {
        if self.dac_reason.as_deref() != Some(reason.as_str()) {
            log::warn!("DAC correction: {reason}");
        }
        self.dac_reason = Some(reason);
    }

    /// Apply the derivation config: enabled / depth plus the
    /// Optional measured IR paths, which run the full IR conditioning to derivation chain
    /// (condition → smooth → SNR-weighted regularized inverse → phase
    /// render) on the control path. A missing/unreadable IR leaves the node
    /// inactive — bit-exact passthrough.
    pub fn apply_config(&mut self, cfg: &config::CorrectionConfig, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.depth = cfg.depth.clamp(0.0, 1.0);
        self.enabled = cfg.enabled;
        if cfg.ir_paths.is_empty() {
            self.clear_ir();
            return;
        }
        match derive_from_config(cfg, sample_rate as f64) {
            Ok(set) => {
                if let Err(e) = self.load_set(&set, sample_rate) {
                    log::warn!("correction: failed to load derived IRs: {e}");
                    self.clear_ir();
                }
            }
            Err(e) => {
                log::warn!(
                    "correction: failed to derive from configured IRs {}: {e}",
                    cfg.ir_paths.join(", ")
                );
                self.clear_ir();
            }
        }
    }

    /// Live runtime toggle (control-queue drain): enabled + depth only — the
    /// loaded IR stays exactly as it was.
    pub fn set_runtime(&mut self, enabled: bool, depth: f32) {
        self.enabled = enabled;
        self.depth = depth.clamp(0.0, 1.0);
        for engine in &mut self.engines {
            engine.set_wet_mix(self.depth);
        }
    }

    /// Load a pre-rendered correction IR set (the `LoadCorrectionIr` /
    /// `MeasureRoom` control-path land, or generation replay). Control path
    /// only — the engines are rebuilt here (allocation is fine).
    pub fn load_set(&mut self, set: &CorrectionIrSet, sample_rate: f32) -> Result<(), String> {
        self.sample_rate = sample_rate;
        if set.channels.is_empty() {
            return Err("correction IR set has no channels".to_string());
        }
        let mut engines = Vec::with_capacity(set.channels.len());
        for ch in &set.channels {
            if ch.len() > MAX_IR_LEN {
                return Err(format!(
                    "correction IR of {} samples exceeds the {} limit",
                    ch.len(),
                    MAX_IR_LEN
                ));
            }
            let mut engine = ConvolutionEngine::new(sample_rate, MAX_IR_LEN.min(ch.len()).max(64));
            engine.set_wet_mix(self.depth);
            let pairs: Vec<(f64, f64)> = ch.iter().map(|&s| (s, s)).collect();
            engine
                .load_ir_from_samples_f64(&pairs)
                .map_err(|e| format!("partitioned-convolution load failed: {e}"))?;
            engine.set_enabled(true);
            engines.push(engine);
        }
        self.engines = engines;
        self.delay_samples = set.delay_samples.round().max(0.0) as usize;
        self.ir_len_samples = set.channels[0].len();
        self.phase_mode = Some(set.phase_mode);
        self.max_gain_db = set
            .channels
            .iter()
            .map(|ch| {
                ch.iter()
                    .map(|s| 20.0 * s.abs().max(1e-12).log10())
                    .fold(f64::NEG_INFINITY, f64::max)
            })
            .fold(f64::NEG_INFINITY, f64::max)
            .max(0.0) as f32;
        log::info!(
            "correction: loaded {} IR(s) of {} samples ({} phase, {} ms latency)",
            self.engines.len(),
            self.ir_len_samples,
            format!("{:?}", set.phase_mode).to_lowercase(),
            self.latency_ms(self.sample_rate)
        );
        Ok(())
    }

    /// Drop the loaded IR set (config with no paths / failed derive).
    fn clear_ir(&mut self) {
        self.engines.clear();
        self.delay_samples = 0;
        self.ir_len_samples = 0;
        self.phase_mode = None;
        self.max_gain_db = 0.0;
    }

    /// Telemetry snapshot.
    pub fn info(&self) -> CorrectionNodeInfo {
        CorrectionNodeInfo {
            enabled: self.enabled,
            depth: self.depth,
            phase_mode: self.phase_mode,
            ir_len_samples: if self.processing() {
                self.ir_len_samples
            } else {
                0
            },
            latency_ms: self.latency_ms(self.sample_rate),
            max_gain_db: if self.processing() {
                self.max_gain_db
            } else {
                0.0
            },
        }
    }
}

/// Read a pre-rendered DAC profile into a [`CorrectionIrSet`].
///
/// Unlike the room-correction path this does **not** condition or invert the
/// files: they are already the rendered correction. It only enforces the
/// sample-rate contract (a profile rendered for another rate is rejected, not
/// silently resampled) and the same IR length budget as the correction node.
fn load_dac_profile_ir(
    profile: &config::DacCorrectionProfileConfig,
    sample_rate: f32,
) -> Result<CorrectionIrSet, String> {
    use crate::dsp::correction::read_wav_ir;
    use std::path::Path;

    let read = |path: &str| {
        read_wav_ir(Path::new(path))
            .map_err(|e| format!("failed to read DAC correction IR {path:?}: {e}"))
    };

    let (channels, file_rate) = if profile.ir_paths.len() == 1 {
        let wav = read(&profile.ir_paths[0])?;
        (wav.channels, wav.sample_rate)
    } else {
        let mut merged = Vec::new();
        let mut rate = 0.0;
        for path in &profile.ir_paths {
            let wav = read(path)?;
            if let Some(ch0) = wav.channels.first() {
                merged.push(ch0.clone());
            }
            rate = wav.sample_rate;
        }
        if merged.is_empty() {
            return Err("DAC correction profile has no readable channels".to_string());
        }
        (merged, rate)
    };

    if channels.is_empty() {
        return Err("DAC correction profile has no channels".to_string());
    }
    if (file_rate - sample_rate as f64).abs() > 1.0 {
        return Err(format!(
            "DAC correction profile is rendered at {file_rate:.0} Hz, output is {sample_rate:.0} Hz"
        ));
    }
    for channel in &channels {
        if channel.len() > MAX_IR_LEN {
            return Err(format!(
                "DAC correction IR of {} samples exceeds the {} limit",
                channel.len(),
                MAX_IR_LEN
            ));
        }
    }
    let latency_ms = if profile.latency_ms.is_finite() {
        profile.latency_ms.max(0.0)
    } else {
        0.0
    };
    Ok(CorrectionIrSet {
        channels,
        sample_rate: sample_rate as f64,
        delay_samples: (latency_ms as f64 * sample_rate as f64 / 1_000.0).round(),
        phase_mode: PhaseMode::Minimum,
        peak_scale: 1.0,
    })
}

/// Run the IR conditioning to derivation chain over the configured measured IR paths (control
/// path). One path = a multichannel WAV; several = per-channel files (each
/// file's channel 0 is that channel's measurement).
fn derive_from_config(
    cfg: &config::CorrectionConfig,
    session_rate: f64,
) -> Result<CorrectionIrSet, crate::dsp::correction::CorrectionError> {
    use crate::dsp::correction::read_wav_ir;
    use std::path::Path;

    let (channels, file_rate) = if cfg.ir_paths.len() == 1 {
        let wav = read_wav_ir(Path::new(&cfg.ir_paths[0]))?;
        (wav.channels, wav.sample_rate)
    } else {
        let mut merged = Vec::new();
        let mut rate = 0.0;
        for path in &cfg.ir_paths {
            let wav = read_wav_ir(Path::new(path))?;
            if let Some(ch0) = wav.channels.first() {
                merged.push(ch0.clone());
            }
            rate = wav.sample_rate;
        }
        if merged.is_empty() {
            return Err(crate::dsp::correction::CorrectionError::InvalidConfig {
                what: "IR paths",
                message: "no channels could be read".into(),
            });
        }
        (merged, rate)
    };

    // The conditioner is strict about rate alignment: a file whose rate
    // differs from the session must be resampled before conditioning (the
    // engine's rate machinery owns that; here the mismatch surfaces as a
    // logged warning and the node stays bit-exact).
    let conditioner = IrConditioner::default();
    let measured = conditioner.condition(
        &crate::dsp::correction::WavIr {
            channels,
            sample_rate: file_rate,
        },
        session_rate,
    )?;

    let meas_len = measured.channels[0].len();
    let ir_len = meas_len
        .next_power_of_two()
        .clamp(MIN_IR_LEN, MAX_IR_LEN)
        .max(MIN_IR_LEN);
    let params = derive_params_from_config(cfg, session_rate, ir_len, CONFIG_DRIVEN_SNR_DB);
    derive_correction_ir(&measured, &params)
}

impl DspNode for CorrectionNode {
    fn capability(&self) -> DspStageCapability {
        DspStageCapability {
            name: "correction",
            channel_support: StageChannelSupport::AllChannels,
            position: "post-aux, pre-EQ",
            stateful: true,
            realtime_safe: true,
            bit_perfect_compatible: false,
            sample_rate_sensitive: true,
            precision: StagePrecision::Any,
        }
    }

    fn is_active(&self) -> bool {
        self.processing() || self.dac_processing()
    }

    /// The per-channel convolution bank plus the node's own scratch planes.
    ///
    /// This is the graph's convolution memory: one partitioned engine per
    /// channel of the loaded IR set, each with every buffer it preallocates
    /// counted at its allocated length.
    fn persistent_bytes(&self) -> usize {
        std::mem::size_of_val(self)
            + self.engines.iter().map(|e| e.heap_bytes()).sum::<usize>()
            + self
                .dac_engines
                .iter()
                .map(|e| e.heap_bytes())
                .sum::<usize>()
            + self.scratch.capacity() * std::mem::size_of::<f32>()
            + self.scratch_f64.capacity() * std::mem::size_of::<f64>()
    }

    fn latency_samples(&self) -> usize {
        // TWO contributions per active bank, not one. `delay_samples` is only
        // the loaded IR's own group delay (zero under the default
        // `PhaseMode::Minimum`). Each bank also runs a partitioned
        // `ConvolutionEngine`, whose algorithmic latency is its FFT block size
        // (512 samples ≈ 10.7 ms at 48 kHz). Reporting only the former claimed
        // zero latency for a stage that delays by a full partition, which fed
        // straight into the engine's playhead compensation.
        //
        // Both banks are sequential, so their latencies add. A bank that is
        // not processing contributes zero.
        let room = if self.processing() {
            let engine_latency: usize = self
                .engines
                .iter()
                .map(|e| e.latency_samples())
                .max()
                .unwrap_or(0);
            self.delay_samples + engine_latency
        } else {
            0
        };
        let dac = if self.dac_processing() {
            let engine_latency: usize = self
                .dac_engines
                .iter()
                .map(|e| e.latency_samples())
                .max()
                .unwrap_or(0);
            self.dac_delay_samples + engine_latency
        } else {
            0
        };
        room + dac
    }

    fn tail_samples(&self) -> usize {
        let room = if self.processing() {
            self.ir_len_samples.saturating_sub(self.delay_samples)
        } else {
            0
        };
        let dac = if self.dac_processing() {
            self.dac_ir_len_samples
                .saturating_sub(self.dac_delay_samples)
        } else {
            0
        };
        room + dac
    }

    fn reset(&mut self) {
        for engine in &mut self.engines {
            engine.reset();
        }
        for engine in &mut self.dac_engines {
            engine.reset();
        }
    }

    fn prepare(&mut self, sample_rate: f32, _max_channels: usize) {
        self.sample_rate = sample_rate;
        for engine in &mut self.engines {
            engine.set_sample_rate(sample_rate);
        }
        for engine in &mut self.dac_engines {
            engine.set_sample_rate(sample_rate);
        }
    }

    fn process_block_f32(&mut self, planes: &mut [&mut [f32]]) {
        if planes.is_empty() {
            return;
        }
        let frames = planes[0].len().min(MAX_AUDIO_BLOCK_FRAMES);
        if self.processing() {
            let n = self.engines.len().min(planes.len());
            for (ch, plane) in planes.iter_mut().take(n).enumerate() {
                let engine = &mut self.engines[ch];
                let scratch = &mut self.scratch[..frames];
                engine.process_block(plane, scratch);
            }
        }
        if self.dac_processing() {
            let n = self.dac_engines.len().min(planes.len());
            for (ch, plane) in planes.iter_mut().take(n).enumerate() {
                let engine = &mut self.dac_engines[ch];
                let scratch = &mut self.scratch[..frames];
                engine.process_block(plane, scratch);
            }
        }
    }

    fn process_block_f64(&mut self, planes: &mut [&mut [f64]]) {
        if planes.is_empty() {
            return;
        }
        let frames = planes[0].len().min(MAX_AUDIO_BLOCK_FRAMES);
        if self.processing() {
            let n = self.engines.len().min(planes.len());
            for (ch, plane) in planes.iter_mut().take(n).enumerate() {
                let engine = &mut self.engines[ch];
                let scratch = &mut self.scratch_f64[..frames];
                engine.process_block_f64(plane, scratch);
            }
        }
        if self.dac_processing() {
            let n = self.dac_engines.len().min(planes.len());
            for (ch, plane) in planes.iter_mut().take(n).enumerate() {
                let engine = &mut self.dac_engines[ch];
                let scratch = &mut self.scratch_f64[..frames];
                engine.process_block_f64(plane, scratch);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_mono_wav(path: &std::path::Path, rate: u32, samples: &[f32]) {
        let mut data = Vec::with_capacity(samples.len() * 2);
        for &sample in samples {
            let value = (sample.clamp(-1.0, 1.0) * 32_767.0).round() as i16;
            data.extend_from_slice(&value.to_le_bytes());
        }
        let mut bytes = Vec::with_capacity(44 + data.len());
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&((36 + data.len()) as u32).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&rate.to_le_bytes());
        bytes.extend_from_slice(&(rate * 2).to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&data);
        std::fs::write(path, bytes).unwrap();
    }

    fn dac_config(path: &std::path::Path, rate: u32) -> config::DacCorrectionConfig {
        config::DacCorrectionConfig {
            enabled: true,
            profiles: vec![config::DacCorrectionProfileConfig {
                dac: "RME ADI-2".to_string(),
                sample_rate: rate,
                ir_paths: vec![path.display().to_string()],
                latency_ms: 1.0,
                depth: 0.75,
            }],
        }
    }

    #[test]
    fn dac_profile_matches_device_and_rate_and_runs_in_its_own_bank() {
        let path = std::env::temp_dir().join(format!(
            "shadowplayer_dac_correction_{}.wav",
            std::process::id()
        ));
        write_mono_wav(&path, 48_000, &[0.5, 0.0, 0.0, 0.0]);

        let mut node = CorrectionNode::new(48_000.0);
        node.apply_dac_config(&dac_config(&path, 48_000), Some("RME ADI-2 DAC"), 48_000.0);
        let info = node.dac_info();
        assert!(info.enabled);
        assert!(info.active, "{info:?}");
        assert_eq!(info.profile.as_deref(), Some("RME ADI-2"));
        assert!((info.depth - 0.75).abs() < 1e-6);
        assert!(info.ir_len_samples > 0);
        assert!(info.latency_ms >= 1.0, "declared latency missing: {info:?}");
        assert!(!node.processing(), "the room bank must stay empty");
        assert!(!node.info().enabled);

        // Feed the impulse in block 0 and let the partitioned convolver
        // reach its output in block 1: the DAC bank, not the room bank, must
        // produce the correction.
        let mut first = vec![0.0f32; 512];
        first[0] = 1.0;
        let mut second = vec![0.0f32; 512];
        {
            let mut planes = [&mut first[..]];
            node.process_block_f32(&mut planes);
        }
        {
            let mut planes = [&mut second[..]];
            node.process_block_f32(&mut planes);
        }
        assert!(
            first
                .iter()
                .chain(second.iter())
                .any(|sample| sample.abs() > 1e-4),
            "DAC correction bank produced no output"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn dac_profile_rate_mismatch_is_inactive_with_a_reason() {
        let path =
            std::env::temp_dir().join(format!("shadowplayer_dac_rate_{}.wav", std::process::id()));
        write_mono_wav(&path, 44_100, &[0.5, 0.0, 0.0, 0.0]);

        let mut node = CorrectionNode::new(48_000.0);
        node.apply_dac_config(&dac_config(&path, 48_000), Some("RME ADI-2"), 48_000.0);
        let info = node.dac_info();
        assert!(info.enabled);
        assert!(!info.active);
        assert!(info.reason.is_some(), "{info:?}");
        assert!(!node.is_active());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn dac_profile_with_a_different_device_does_not_load() {
        let path = std::env::temp_dir().join(format!(
            "shadowplayer_dac_device_{}.wav",
            std::process::id()
        ));
        write_mono_wav(&path, 48_000, &[0.5, 0.0, 0.0, 0.0]);

        let mut node = CorrectionNode::new(48_000.0);
        node.apply_dac_config(&dac_config(&path, 48_000), Some("Some Other DAC"), 48_000.0);
        let info = node.dac_info();
        assert!(info.enabled);
        assert!(!info.active);
        assert!(info.reason.is_some(), "{info:?}");
        let _ = std::fs::remove_file(path);
    }
}
