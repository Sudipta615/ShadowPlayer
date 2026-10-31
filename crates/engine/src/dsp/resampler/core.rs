//! Rubato-backed resampler algorithm and generic dispatch.

use super::polyphase::{PolyphaseResampler, ResamplerFilterMetadata};
use super::types::{
    ResamplerError, CHANNELS, CHUNK_SIZE, MAX_OUTPUT_BUFFER_FRAMES, MAX_REBUILD_FAILURES,
};
use config::{ResamplerFilter, ResamplerQuality};
use num_traits::Float;
use rubato::audioadapter_buffers::direct::SequentialSliceOfVecs;
use rubato::{Fft, FixedSync, Resampler, WindowFunction};

enum ResamplerInner<T: rubato::Sample + Float + Default + Send + Sync + 'static = f32> {
    /// High quality: Fft with a longer filter for better anti-aliasing
    HighQuality(Fft<T>),
    /// Ultra: Fft with the longest filter (single sub-chunk, so the
    /// filter length is derived from the full 2048-frame chunk)
    Ultra(Fft<T>),
    /// Balanced: Fft with moderate filter sizes
    Balanced(Fft<T>),
    /// Fast: Fft with minimal processing (fixed input+output chunk sizes)
    Fast(Fft<T>),
    /// The engine's own rational polyphase bank (P1.2 filter shapes).
    Polyphase(PolyphaseResampler<T>),
}

impl<T: rubato::Sample + Float + Default + Send + Sync + 'static> ResamplerInner<T> {
    fn input_frames_next(&self) -> usize {
        match self {
            Self::HighQuality(r) => r.input_frames_next(),
            Self::Ultra(r) => r.input_frames_next(),
            Self::Balanced(r) => r.input_frames_next(),
            Self::Fast(r) => r.input_frames_next(),
            Self::Polyphase(r) => r.input_frames_next(),
        }
    }

    /// Maximum possible output frames per chunk (bounds the scratch buffers).
    fn output_frames_max(&self) -> usize {
        match self {
            Self::HighQuality(r) => r.output_frames_max(),
            Self::Ultra(r) => r.output_frames_max(),
            Self::Balanced(r) => r.output_frames_max(),
            Self::Fast(r) => r.output_frames_max(),
            Self::Polyphase(r) => r.output_frames_max(),
        }
    }

    /// Output frames the next `process_into_buffer` call will write.
    fn output_frames_next(&self) -> usize {
        match self {
            Self::HighQuality(r) => r.output_frames_next(),
            Self::Ultra(r) => r.output_frames_next(),
            Self::Balanced(r) => r.output_frames_next(),
            Self::Fast(r) => r.output_frames_next(),
            Self::Polyphase(r) => r.output_frames_next(),
        }
    }

    /// Filter group delay in *output* frames (rubato's authoritative
    /// `Resampler::output_delay`).
    fn output_delay(&self) -> usize {
        match self {
            Self::HighQuality(r) => r.output_delay(),
            Self::Ultra(r) => r.output_delay(),
            Self::Balanced(r) => r.output_delay(),
            Self::Fast(r) => r.output_delay(),
            Self::Polyphase(r) => r.output_delay(),
        }
    }

    /// Resample one chunk into the pre-allocated planar scratch buffers.
    ///
    /// `buffer_in`/`buffer_out` are `audioadapter` wrappers over the
    /// pre-allocated `input_buffers`/`scratch` Vecs in `AudioResampler`;
    /// building them allocates nothing and cannot fail (the sizing is
    /// enforced by the caller before the call).
    fn process_into_buffer(
        &mut self,
        buffer_in: &dyn rubato::audioadapter::Adapter<T>,
        buffer_out: &mut dyn rubato::audioadapter::AdapterMut<T>,
    ) -> Result<(usize, usize), rubato::ResampleError> {
        match self {
            Self::HighQuality(r) => r.process_into_buffer(buffer_in, buffer_out, None),
            Self::Ultra(r) => r.process_into_buffer(buffer_in, buffer_out, None),
            Self::Balanced(r) => r.process_into_buffer(buffer_in, buffer_out, None),
            Self::Fast(r) => r.process_into_buffer(buffer_in, buffer_out, None),
            Self::Polyphase(_) => unreachable!("polyphase is processed by process_planar"),
        }
    }

    /// Resample one planar chunk. The rubato engines are wrapped in the same
    /// allocation-free adapters as before; the polyphase engine writes the
    /// planar buffers directly, so it never pays for an adapter dispatch.
    fn process_planar(
        &mut self,
        input: &[Vec<T>],
        output: &mut [Vec<T>],
        out_frames: usize,
    ) -> Result<usize, String> {
        if let Self::Polyphase(p) = self {
            return Ok(p.process(input, output, out_frames));
        }
        let in_frames = input[0].len();
        let in_adapter =
            SequentialSliceOfVecs::new(input, CHANNELS, in_frames).map_err(|e| e.to_string())?;
        let mut out_adapter = SequentialSliceOfVecs::new_mut(output, CHANNELS, out_frames)
            .map_err(|e| e.to_string())?;
        self.process_into_buffer(&in_adapter, &mut out_adapter)
            .map(|(_consumed, produced)| produced)
            .map_err(|e| e.to_string())
    }

    fn quality(&self) -> ResamplerQuality {
        match self {
            Self::HighQuality(_) => ResamplerQuality::HighQuality,
            Self::Ultra(_) => ResamplerQuality::Ultra,
            Self::Balanced(_) => ResamplerQuality::Balanced,
            Self::Fast(_) => ResamplerQuality::Fast,
            Self::Polyphase(r) => r.quality(),
        }
    }

    /// The filter family this engine actually runs.
    fn filter(&self) -> ResamplerFilter {
        match self {
            Self::HighQuality(_) | Self::Ultra(_) | Self::Balanced(_) | Self::Fast(_) => {
                ResamplerFilter::Fft
            }
            Self::Polyphase(r) => r.filter(),
        }
    }

    /// Measured metadata for the polyphase bank; `None` for the rubato engine,
    /// whose realized filter is measured by the fidelity tests instead of
    /// introspected here.
    fn metadata(&self) -> Option<ResamplerFilterMetadata> {
        match self {
            Self::Polyphase(r) => Some(r.metadata().clone()),
            _ => None,
        }
    }
}

/// High-quality resampler with configurable quality profiles and sample type.
pub struct AudioResampler<T: rubato::Sample + Float + Default + Send + Sync + 'static = f32> {
    /// Inner resampler using enum dispatch
    inner: ResamplerInner<T>,
    /// Source sample rate
    source_rate: usize,
    /// Output sample rate
    output_rate: usize,
    /// The quality profile the caller requested (may differ from the
    /// effective one if construction fell back, e.g. HighQuality → Fast).
    requested_quality: ResamplerQuality,
    /// The filter family the caller requested. Kept across rate/speed
    /// rebuilds; [`Self::effective_filter`] reports the family actually
    /// running when construction had to fall back.
    filter: ResamplerFilter,
    /// Low-pass cutoff as a fraction of the lower Nyquist (0.85 ..= 0.999).
    /// Only the polyphase families use it; the rubato FFT engine keeps its
    /// own fixed design.
    cutoff_ratio: f64,
    /// Playback speed multiplier (1.0 = normal)
    speed: f32,
    /// Input buffer for accumulating samples before processing
    input_buffers: [Vec<T>; CHANNELS],
    /// Write position in input buffers
    input_pos: usize,
    /// Output ring buffer for samples waiting to be consumed
    output_buffers: [Vec<T>; CHANNELS],
    /// Read position in output buffers
    output_read_pos: usize,
    /// Number of valid samples in output buffers
    output_available: usize,
    /// Whether the resampler needs to be reconfigured
    needs_rebuild: bool,
    /// Pending quality change to apply on next rebuild
    pending_quality: Option<ResamplerQuality>,
    /// After MAX_REBUILD_FAILURES consecutive failures, the resampler
    /// is disabled.
    rebuild_failures: u32,
    disabled: bool,
    /// Receiver for the background thread that builds the new resampler
    rebuild_rx: Option<crossbeam::channel::Receiver<Result<ResamplerInner<T>, ResamplerError>>>,
    /// Recent output samples for crossfade during rebuild (reduces glitches)
    crossfade_buffer: [(T, T); 64],
    /// Current read position in crossfade_buffer
    crossfade_pos: usize,
    /// Number of crossfade samples remaining to blend
    crossfade_remaining: usize,
    crossfade_blend_total: usize,
    rebuilt_effective_source: usize,
    rebuilt_output_rate: usize,
    rebuilt_quality: ResamplerQuality,
    /// Pre-allocated planar scratch buffers for process_chunk (sized for
    /// MAX_RATIO and `output_frames_max`). Using Vecs avoids large stack
    /// frames and handles extreme upsample ratios.
    scratch: [Vec<T>; CHANNELS],
    /// Frames the resampler discarded, either because a chunk could not be
    /// processed at all or because the consumer let the output backlog
    /// overflow. Non-zero means the output is not a faithful resample of the
    /// input, and it used to mean that silently.
    dropped_frames: u64,
}

pub type AudioResamplerF32 = AudioResampler<f32>;
pub type AudioResamplerF64 = AudioResampler<f64>;

impl<T: rubato::Sample + Float + Default + Send + Sync + 'static> AudioResampler<T> {
    /// Create a new resampler with the existing rubato FFT engine.
    pub fn new(
        quality: ResamplerQuality,
        source_rate: f32,
        output_rate: f32,
    ) -> Result<Self, ResamplerError> {
        Self::new_with_filter(quality, ResamplerFilter::Fft, source_rate, output_rate)
    }

    /// Create a new resampler with an explicit filter family and sample rates.
    pub fn new_with_filter(
        quality: ResamplerQuality,
        filter: ResamplerFilter,
        source_rate: f32,
        output_rate: f32,
    ) -> Result<Self, ResamplerError> {
        Self::new_with_filter_and_cutoff(quality, filter, 0.95, source_rate, output_rate)
    }

    /// Create a new resampler with an explicit filter family, cutoff ratio and
    /// sample rates. The cutoff is clamped to the polyphase design's supported
    /// range (0.85 ..= 0.999 of the lower Nyquist).
    pub fn new_with_filter_and_cutoff(
        quality: ResamplerQuality,
        filter: ResamplerFilter,
        cutoff_ratio: f64,
        source_rate: f32,
        output_rate: f32,
    ) -> Result<Self, ResamplerError> {
        // `is_finite`, not `<= 0.0`. Every NaN comparison is false, so a NaN rate
        // sailed past a `<= 0.0` guard: `NaN.round() as usize` saturates to 0,
        // `.max(1)` turned that into 1, and the result was a 1 Hz → 48 kHz
        // converter whose `allocate_buffers` sized scratch as
        // `input_frames * ratio` ≈ 98M frames — about 0.8 GB. The sibling
        // setters `set_source_rate`/`set_output_rate` already checked
        // `is_finite`; only the constructor was exposed.
        if !source_rate.is_finite() || !output_rate.is_finite() {
            return Err(ResamplerError::InvalidRates {
                source_rate: if source_rate.is_finite() {
                    source_rate.round().max(0.0) as usize
                } else {
                    0
                },
                output_rate: if output_rate.is_finite() {
                    output_rate.round().max(0.0) as usize
                } else {
                    0
                },
            });
        }
        let src = (source_rate.round() as usize).max(1);
        let out = (output_rate.round() as usize).max(1);
        if source_rate <= 0.0 || output_rate <= 0.0 {
            return Err(ResamplerError::InvalidRates {
                source_rate: src,
                output_rate: out,
            });
        }
        let cutoff_ratio = cutoff_ratio.clamp(0.85, 0.999);
        let inner = Self::create_resampler(quality, filter, cutoff_ratio, src, out)?;
        let rebuilt_quality = inner.quality();
        let mut resampler = Self {
            inner,
            source_rate: src,
            output_rate: out,
            requested_quality: quality,
            filter,
            cutoff_ratio,
            speed: 1.0,
            input_buffers: [Vec::new(), Vec::new()],
            input_pos: 0,
            output_buffers: [Vec::new(), Vec::new()],
            output_read_pos: 0,
            output_available: 0,
            needs_rebuild: false,
            pending_quality: None,
            rebuild_failures: 0,
            disabled: false,
            rebuild_rx: None,
            crossfade_buffer: [(T::zero(), T::zero()); 64],
            crossfade_pos: 0,
            crossfade_remaining: 0,
            crossfade_blend_total: 1,
            rebuilt_effective_source: src,
            rebuilt_output_rate: out,
            rebuilt_quality,
            scratch: [Vec::new(), Vec::new()],
            dropped_frames: 0,
        };
        resampler.allocate_buffers();
        Ok(resampler)
    }

    /// Create the requested engine, falling back explicitly and visibly.
    ///
    /// A polyphase family that cannot be built for this ratio (the prototype
    /// would exceed [`super::polyphase::MAX_PROTOTYPE_TAPS`]) falls back to the
    /// rubato FFT engine at the same quality tier. If rubato also fails, the
    /// existing Fast fallback applies. Both fallbacks are logged; callers can
    /// compare [`Self::requested_filter`] with [`Self::effective_filter`].
    fn create_resampler(
        quality: ResamplerQuality,
        filter: ResamplerFilter,
        cutoff_ratio: f64,
        source_rate: usize,
        output_rate: usize,
    ) -> Result<ResamplerInner<T>, ResamplerError> {
        if filter.is_polyphase() {
            match PolyphaseResampler::new_with_cutoff(
                filter,
                quality,
                source_rate as u32,
                output_rate as u32,
                cutoff_ratio,
            ) {
                Ok(p) => return Ok(ResamplerInner::Polyphase(p)),
                Err(err) => log::warn!(
                    "Polyphase {:?} resampler unavailable for {} -> {} Hz, \
                     falling back to the FFT engine: {}",
                    filter,
                    source_rate,
                    output_rate,
                    err
                ),
            }
        }
        Self::create_resampler_exact(quality, source_rate, output_rate).or_else(|orig_err| {
            if quality != ResamplerQuality::Fast {
                log::warn!(
                    "Resampler creation failed for {:?} ({} -> {} Hz), falling back to Fast: {}",
                    quality,
                    source_rate,
                    output_rate,
                    orig_err
                );
                Self::create_resampler_exact(ResamplerQuality::Fast, source_rate, output_rate)
            } else {
                Err(orig_err)
            }
        })
    }

    /// Exact constructor without fallback.
    fn create_resampler_exact(
        quality: ResamplerQuality,
        source_rate: usize,
        output_rate: usize,
    ) -> Result<ResamplerInner<T>, ResamplerError> {
        match quality {
            // rubato 5.0's `Fft::new_custom(fs_in, fs_out, chunk_size,
            // sub_chunks, channels, window, fixed)`: the anti-aliasing filter
            // length derives from `chunk_size / sub_chunks`. Measured filter
            // group delay for this tier at 44.1 <-> 48 kHz is 560 output
            // frames, i.e. a ~1120-tap filter -- see
            // `tests/fidelity/resampler_measurement.rs::
            // published_latency_matches_measurement_and_its_tap_counts`, which
            // measures every tier and pins it to the published latency table
            // and the tap counts in the tier documentation.
            //
            // An earlier comment here quoted a different set (Fast ~640,
            // Balanced ~588, HighQuality ~1029, Ultra ~2058), which was both
            // wrong and not monotonic across tiers -- Fast appeared to have a
            // *longer* filter than Balanced. The measured ladder is
            // 320 / 640 / 1120 / 2240 taps.
            ResamplerQuality::HighQuality => Fft::new_custom(
                source_rate,
                output_rate,
                CHUNK_SIZE * 2,
                2,
                CHANNELS,
                WindowFunction::BlackmanHarris2,
                FixedSync::Input,
            )
            .map(ResamplerInner::HighQuality)
            .map_err(|e| ResamplerError::CreationFailed {
                quality,
                reason: e.to_string(),
            }),
            // Single sub-chunk: the filter derives from the whole 2048-frame
            // chunk (~2× longer than HighQuality), giving the deepest
            // stopband in this engine.
            ResamplerQuality::Ultra => Fft::new_custom(
                source_rate,
                output_rate,
                CHUNK_SIZE * 2,
                1,
                CHANNELS,
                WindowFunction::BlackmanHarris2,
                FixedSync::Input,
            )
            .map(ResamplerInner::Ultra)
            .map_err(|e| ResamplerError::CreationFailed {
                quality,
                reason: e.to_string(),
            }),
            ResamplerQuality::Balanced => Fft::new_custom(
                source_rate,
                output_rate,
                CHUNK_SIZE,
                2,
                CHANNELS,
                WindowFunction::BlackmanHarris2,
                FixedSync::Input,
            )
            .map(ResamplerInner::Balanced)
            .map_err(|e| ResamplerError::CreationFailed {
                quality,
                reason: e.to_string(),
            }),
            ResamplerQuality::Fast => {
                // FixedSync::Both has no sub-chunks: its filter derives from
                // the whole chunk. A small chunk (256 frames) keeps the filter
                // shortest. Measured group delay is 160 output frames at
                // 44.1 <-> 48 kHz, i.e. ~320 taps -- the shortest tier by a
                // factor of two against Balanced, so Fast really is the
                // cheapest tier and carries the most aliasing.
                Fft::new(source_rate, output_rate, 256, CHANNELS, FixedSync::Both)
                    .map(ResamplerInner::Fast)
                    .map_err(|e| ResamplerError::CreationFailed {
                        quality,
                        reason: e.to_string(),
                    })
            }
        }
    }

    /// Pre-allocate all internal buffers.
    ///
    /// The scratch buffers are sized to handle the worst-case upsample ratio
    /// (currently `MAX_RATIO`×) plus a filter group-delay margin. This ensures
    /// `process_chunk` never overflows regardless of source/output rate combination.
    fn allocate_buffers(&mut self) {
        let input_frames = self.inner.input_frames_next();
        let input_capacity = input_frames.max(CHUNK_SIZE * 4);
        // The output buffer has to hold a *whole* chunk, and a long-filter
        // tier reads 2048-frame chunks: at the worst supported ratio that is
        // more than `MAX_OUTPUT_BUFFER_FRAMES`. Sizing to the per-chunk
        // maximum keeps `process_chunk` from having to truncate a chunk it
        // cannot fit.
        let output_capacity = MAX_OUTPUT_BUFFER_FRAMES.max(self.inner.output_frames_max());

        for ch in 0..CHANNELS {
            self.input_buffers[ch].resize(input_frames, T::zero());
            self.input_buffers[ch].reserve(input_capacity - input_frames);
            self.output_buffers[ch].resize(output_capacity, T::zero());
        }

        // Compute the scratch size from the actual ratio so that even
        // 44.1 → 768 kHz (17.4×) or 48 → 384 kHz (8×) never overflows.
        // `output_frames_max()` is the authoritative per-chunk output bound,
        // so the realtime guard in `process_chunk` can never trip.
        let ratio = self.output_rate as f64 / self.compute_effective_source_rate() as f64;
        let filter_margin = self.inner.input_frames_next().max(256);
        let scratch_needed = ((input_frames as f64 * ratio).ceil() as usize + filter_margin)
            .max(MAX_OUTPUT_BUFFER_FRAMES)
            .max(self.inner.output_frames_max());

        for ch in 0..CHANNELS {
            self.scratch[ch].clear();
            self.scratch[ch].resize(scratch_needed, T::zero());
        }

        self.input_pos = 0;
        self.output_read_pos = 0;
        self.output_available = 0;
    }

    /// Feed a stereo sample into the resampler
    #[inline]
    pub fn feed(&mut self, left: T, right: T) {
        if !self.disabled
            && self.rebuild_rx.is_none()
            && !self.needs_rebuild
            && (self.rebuilt_effective_source != self.compute_effective_source_rate()
                || self.rebuilt_output_rate != self.output_rate
                || self.rebuilt_quality != self.inner.quality()
                || self.pending_quality.is_some())
        {
            self.needs_rebuild = true;
        }

        if self.disabled || self.is_passthrough() {
            self.push_sample_direct(left, right);
            return;
        }

        if let Some(ref rx) = self.rebuild_rx {
            match rx.try_recv() {
                Ok(result) => {
                    self.rebuild_rx = None;
                    self.apply_rebuild_result(result);
                }
                Err(crossbeam::channel::TryRecvError::Empty) => {}
                Err(crossbeam::channel::TryRecvError::Disconnected) => {
                    log::error!("Resampler builder thread disconnected unexpectedly.");
                    self.rebuild_rx = None;
                    self.needs_rebuild = true;
                    self.rebuild_failures += 1;
                }
            }
        } else if self.needs_rebuild {
            if self.rebuild_failures >= MAX_REBUILD_FAILURES {
                self.needs_rebuild = false;
                self.disabled = true;
                log::error!(
                    "Resampler disabled after {} consecutive rebuild failures.",
                    MAX_REBUILD_FAILURES
                );
                self.push_sample_direct(left, right);
                return;
            } else {
                self.trigger_rebuild();
            }
        }

        if self.input_pos >= self.input_buffers[0].len() {
            self.process_chunk();
            if self.input_pos >= self.input_buffers[0].len() {
                return;
            }
        }

        self.input_buffers[0][self.input_pos] = left;
        self.input_buffers[1][self.input_pos] = right;
        self.input_pos += 1;

        let needed = self.inner.input_frames_next();
        if self.input_pos >= needed {
            self.process_chunk();
        }
    }

    /// Feed a block of stereo frames into the resampler.
    ///
    /// Batches slice copies and reduces per-sample checks, invoking `process_chunk()`
    /// only as each chunk is filled.
    pub fn feed_block(&mut self, left: &[T], right: &[T]) {
        let n = left.len().min(right.len());
        if n == 0 {
            return;
        }

        if !self.disabled
            && self.rebuild_rx.is_none()
            && !self.needs_rebuild
            && (self.rebuilt_effective_source != self.compute_effective_source_rate()
                || self.rebuilt_output_rate != self.output_rate
                || self.rebuilt_quality != self.inner.quality()
                || self.pending_quality.is_some())
        {
            self.needs_rebuild = true;
        }

        if self.disabled || self.is_passthrough() {
            for i in 0..n {
                self.push_sample_direct(left[i], right[i]);
            }
            return;
        }

        let mut offset = 0;
        while offset < n {
            if let Some(ref rx) = self.rebuild_rx {
                match rx.try_recv() {
                    Ok(result) => {
                        self.rebuild_rx = None;
                        self.apply_rebuild_result(result);
                    }
                    Err(crossbeam::channel::TryRecvError::Empty) => {}
                    Err(crossbeam::channel::TryRecvError::Disconnected) => {
                        log::error!("Resampler builder thread disconnected unexpectedly.");
                        self.rebuild_rx = None;
                        self.needs_rebuild = true;
                        self.rebuild_failures += 1;
                    }
                }
            } else if self.needs_rebuild {
                if self.rebuild_failures >= MAX_REBUILD_FAILURES {
                    self.needs_rebuild = false;
                    self.disabled = true;
                    log::error!(
                        "Resampler disabled after {} consecutive rebuild failures.",
                        MAX_REBUILD_FAILURES
                    );
                    for i in offset..n {
                        self.push_sample_direct(left[i], right[i]);
                    }
                    return;
                } else {
                    self.trigger_rebuild();
                }
            }

            let input_buf_len = self.input_buffers[0].len();
            if self.input_pos >= input_buf_len {
                self.process_chunk();
                if self.input_pos >= input_buf_len {
                    return;
                }
            }

            let needed = self.inner.input_frames_next();
            let to_fill = (needed.saturating_sub(self.input_pos))
                .min(input_buf_len - self.input_pos)
                .min(n - offset);

            if to_fill > 0 {
                self.input_buffers[0][self.input_pos..self.input_pos + to_fill]
                    .copy_from_slice(&left[offset..offset + to_fill]);
                self.input_buffers[1][self.input_pos..self.input_pos + to_fill]
                    .copy_from_slice(&right[offset..offset + to_fill]);
                self.input_pos += to_fill;
                offset += to_fill;
            }

            if self.input_pos >= needed {
                self.process_chunk();
            }
        }
    }

    /// Write a single stereo sample directly into the output buffers with
    /// no heap allocation. Used by the disabled-resampler bypass path.
    #[inline]
    fn push_sample_direct(&mut self, left: T, right: T) {
        // The buffer's own length is the authoritative capacity; it is at
        // least `MAX_OUTPUT_BUFFER_FRAMES` (see `allocate_buffers`).
        let cap = self.output_buffers[0]
            .len()
            .min(self.output_buffers[1].len());
        // When the consumer has drained the queue completely, the read cursor
        // may sit at the old end of the backing storage. Rewind it before the
        // next direct write; otherwise passthrough silently drops every sample
        // after one full output-buffer span.
        if self.output_available == 0 {
            self.output_read_pos = 0;
        }
        if cap < MAX_OUTPUT_BUFFER_FRAMES {
            log::error!(
                "Resampler output buffers are under-allocated; dropping passthrough sample"
            );
            self.dropped_frames += 1;
            return;
        }
        let write_start = self.output_read_pos + self.output_available;
        if write_start < cap {
            self.output_buffers[0][write_start] = left;
            self.output_buffers[1][write_start] = right;
            self.output_available += 1;
        } else {
            if self.output_available > 0 && self.output_read_pos > 0 {
                self.output_buffers[0].copy_within(
                    self.output_read_pos..self.output_read_pos + self.output_available,
                    0,
                );
                self.output_buffers[1].copy_within(
                    self.output_read_pos..self.output_read_pos + self.output_available,
                    0,
                );
                self.output_read_pos = 0;
            }
            let new_write_start = self.output_read_pos + self.output_available;
            if new_write_start < cap {
                self.output_buffers[0][new_write_start] = left;
                self.output_buffers[1][new_write_start] = right;
                self.output_available += 1;
            } else {
                // The consumer has stalled for a whole buffer's worth of
                // samples. The sample has to go somewhere; count it rather
                // than dropping it without a trace.
                self.dropped_frames += 1;
            }
        }
    }

    /// Process a full chunk of input samples through rubato.
    ///
    /// Uses pre-allocated planar `scratch` Vecs (sized at construction time
    /// for the worst-case ratio and `output_frames_max`) to avoid any heap
    /// allocation on this hot path. `allocate_buffers()` sizes every buffer
    /// for a whole chunk off the audio path, so the guards below are
    /// tripwires rather than routine paths; when one does fire, or when the
    /// consumer has let the output backlog overflow, the frames involved are
    /// counted in `dropped_frames()` rather than vanishing behind an error
    /// log.
    fn process_chunk(&mut self) {
        let needed = self.inner.input_frames_next();
        if self.input_pos < needed {
            for ch in 0..CHANNELS {
                self.input_buffers[ch][self.input_pos..needed].fill(T::zero());
            }
            self.input_pos = needed;
        }

        if self.output_available > 0 && self.output_read_pos > 0 {
            let rpos = self.output_read_pos;
            let avail = self.output_available;
            let safe_avail = avail.min(self.output_buffers[0].len().saturating_sub(rpos));
            for ch in 0..CHANNELS {
                self.output_buffers[ch].copy_within(rpos..rpos + safe_avail, 0);
            }
            self.output_read_pos = 0;
            self.output_available = safe_avail;
        } else if self.output_available == 0 {
            self.output_read_pos = 0;
        }

        // The exact number of output frames rubato will write for this chunk.
        let out_frames = self.inner.output_frames_next();

        // STRICT zero-allocation invariant: the scratch buffers are sized in
        // `allocate_buffers()` (which runs on every rebuild, OFF the audio
        // path) and must never be resized here. `allocate_buffers` sizes
        // them from `output_frames_max()`, so `out_frames <= scratch.len()`
        // always holds; if it does not, the buffer invariant is broken and
        // the chunk is bypassed (never truncated, never partially written).
        let capacity = self.output_buffers[0]
            .len()
            .min(self.output_buffers[1].len());
        if capacity < out_frames
            || self.scratch[0].len() < out_frames
            || self.scratch[1].len() < out_frames
        {
            log::error!(
                "Resampler buffers are under-allocated for a {out_frames}-frame chunk \
                 (output {capacity}, scratch {}); dropping the chunk",
                self.scratch[0].len()
            );
            self.dropped_frames += needed as u64;
            self.input_pos = 0;
            return;
        }

        // Room for the whole chunk. When the consumer has not drained the
        // buffer, the *oldest* output frames go: dropping the chunk that is
        // being resampled now is an audible dropout, whereas dropping part of
        // a backlog the consumer has not read is not heard as one.
        let used = self.output_available;
        if used + out_frames > capacity {
            let excess = (used + out_frames - capacity).min(used);
            log::warn!(
                "Resampler output backlog: dropping {excess} oldest frames to make room \
                 (the consumer is not draining)"
            );
            self.dropped_frames += excess as u64;
            self.output_buffers[0].copy_within(excess..used, 0);
            self.output_buffers[1].copy_within(excess..used, 0);
            self.output_available = used - excess;
        }

        let write_start = self.output_available;
        let space_available = capacity - write_start;

        let result =
            self.inner
                .process_planar(&self.input_buffers[..], &mut self.scratch[..], out_frames);

        match result {
            Ok(produced) => {
                let frames_to_add = produced.min(space_available);
                if frames_to_add < produced {
                    // Cannot happen while `capacity >= output_frames_max()`;
                    // count it rather than letting the tail disappear.
                    self.dropped_frames += (produced - frames_to_add) as u64;
                    log::error!(
                        "Resampler produced {produced} frames but only {space_available} fit; \
                         {} counted as dropped",
                        produced - frames_to_add
                    );
                }
                if frames_to_add > 0 {
                    self.output_buffers[0][write_start..write_start + frames_to_add]
                        .copy_from_slice(&self.scratch[0][..frames_to_add]);
                    self.output_buffers[1][write_start..write_start + frames_to_add]
                        .copy_from_slice(&self.scratch[1][..frames_to_add]);
                    self.output_available += frames_to_add;
                }
            }

            Err(e) => {
                // No output was produced for this chunk's input; the frames
                // are gone, so say so in the accounting.
                self.dropped_frames += needed as u64;
                log::warn!(
                    "Resampler process error ({} input frames dropped): {e}",
                    needed
                );
            }
        }

        self.input_pos = 0;
    }

    /// Read a resampled stereo sample. Returns None if no output is available.
    #[inline]
    pub fn read(&mut self) -> Option<(T, T)> {
        if self.crossfade_remaining > 0 {
            let (new_l, new_r) = if self.output_available > 0 {
                let l = self.output_buffers[0][self.output_read_pos];
                let r = self.output_buffers[1][self.output_read_pos];
                self.output_read_pos += 1;
                self.output_available -= 1;
                (l, r)
            } else {
                (T::zero(), T::zero())
            };
            let (old_l, old_r) = self.crossfade_buffer[self.crossfade_pos % 64];
            self.crossfade_pos += 1;
            self.crossfade_remaining -= 1;
            let t_f = self.crossfade_remaining as f64 / self.crossfade_blend_total as f64;
            let t = T::from(t_f).unwrap_or(T::zero());
            let one_minus_t = T::one() - t;
            return Some((
                new_l * one_minus_t + old_l * t,
                new_r * one_minus_t + old_r * t,
            ));
        }

        if self.output_available == 0 {
            return None;
        }

        let left = self.output_buffers[0][self.output_read_pos];
        let right = self.output_buffers[1][self.output_read_pos];
        self.output_read_pos += 1;
        self.output_available -= 1;

        Some((left, right))
    }

    /// Read available resampled stereo frames into destination slices.
    /// Returns the number of frames read.
    pub fn read_block(&mut self, out_l: &mut [T], out_r: &mut [T]) -> usize {
        let max_read = out_l.len().min(out_r.len());
        let mut count = 0;
        while self.crossfade_remaining > 0 && count < max_read {
            if let Some((l, r)) = self.read() {
                out_l[count] = l;
                out_r[count] = r;
                count += 1;
            } else {
                return count;
            }
        }

        let avail = self.output_available.min(max_read - count);
        if avail > 0 {
            let rpos = self.output_read_pos;
            out_l[count..count + avail]
                .copy_from_slice(&self.output_buffers[0][rpos..rpos + avail]);
            out_r[count..count + avail]
                .copy_from_slice(&self.output_buffers[1][rpos..rpos + avail]);
            self.output_read_pos += avail;
            self.output_available -= avail;
            count += avail;
        }
        count
    }

    /// Number of output samples available for reading
    pub fn available_output(&self) -> usize {
        self.output_available
    }

    /// Set playback speed (0.25 to 4.0)
    pub fn set_speed(&mut self, speed: f32) {
        let new_speed = speed.clamp(0.25, 4.0);
        if (new_speed - self.speed).abs() > 0.001 {
            self.speed = new_speed;
            self.needs_rebuild = true;
        }
    }

    /// Get current playback speed
    pub fn speed(&self) -> f32 {
        self.speed
    }

    #[cfg(test)]
    pub(crate) fn rebuild_pending(&self) -> bool {
        self.needs_rebuild || self.rebuild_rx.is_some()
    }

    /// Set the quality profile (triggers rebuild)
    pub fn set_quality(&mut self, quality: ResamplerQuality) {
        if quality != self.inner.quality() {
            self.requested_quality = quality;
            self.pending_quality = Some(quality);
            self.needs_rebuild = true;
        }
    }

    /// Set the filter family (triggers a rebuild).
    ///
    /// The running engine keeps its buffered output across the rebuild and
    /// crossfades into the new filter, so changing the family mid-stream is a
    /// control-path operation rather than a hard reset.
    pub fn set_filter(&mut self, filter: ResamplerFilter) {
        if filter != self.filter {
            self.filter = filter;
            self.needs_rebuild = true;
        }
    }

    /// Set the polyphase low-pass cutoff ratio (triggers a rebuild).
    ///
    /// The ratio is relative to the lower of the two Nyquist frequencies and
    /// is clamped to the filter design's supported range.
    pub fn set_cutoff_ratio(&mut self, cutoff_ratio: f64) {
        let cutoff_ratio = cutoff_ratio.clamp(0.85, 0.999);
        if (cutoff_ratio - self.cutoff_ratio).abs() > 1e-6 {
            self.cutoff_ratio = cutoff_ratio;
            self.needs_rebuild = true;
        }
    }

    /// The cutoff ratio the resampler is configured to use.
    pub fn cutoff_ratio(&self) -> f64 {
        self.cutoff_ratio
    }

    /// The quality profile the caller requested. If construction of the
    /// requested profile failed, the resampler silently fell back to
    /// [`Self::effective_quality`] — see [`Self::quality_fell_back`].
    pub fn requested_quality(&self) -> ResamplerQuality {
        self.requested_quality
    }

    /// The quality profile actually running. May be lower than
    /// [`Self::requested_quality`] when a rebuild fell back.
    pub fn effective_quality(&self) -> ResamplerQuality {
        self.inner.quality()
    }

    /// True when the running resampler is not the quality the caller asked
    /// for (a silent quality downgrade occurred).
    pub fn quality_fell_back(&self) -> bool {
        self.requested_quality != self.inner.quality()
    }

    /// The filter family the caller requested.
    pub fn requested_filter(&self) -> ResamplerFilter {
        self.filter
    }

    /// The filter family actually running. Differs from
    /// [`Self::requested_filter`] only after an announced fallback (an
    /// extreme ratio that cannot build the polyphase prototype).
    pub fn effective_filter(&self) -> ResamplerFilter {
        self.inner.filter()
    }

    /// True when the polyphase request fell back to the FFT engine.
    pub fn filter_fell_back(&self) -> bool {
        self.filter != self.inner.filter()
    }

    /// Measured shape of the running filter, when the engine can introspect
    /// it. The rubato FFT path returns `None`; its realized latency is pinned
    /// by the fidelity tests instead.
    pub fn filter_metadata(&self) -> Option<ResamplerFilterMetadata> {
        self.inner.metadata()
    }

    /// Set the source sample rate (triggers rebuild)
    pub fn set_source_rate(&mut self, rate: f32) {
        if !rate.is_finite() || rate <= 0.0 {
            log::warn!(
                "AudioResampler::set_source_rate: ignoring invalid rate {}",
                rate
            );
            return;
        }
        let rate_usize = (rate.round() as usize).max(1);
        if rate_usize != self.source_rate {
            self.source_rate = rate_usize;
            self.needs_rebuild = true;
        }
    }

    /// Set the output sample rate (triggers rebuild)
    pub fn set_output_rate(&mut self, rate: f32) {
        if !rate.is_finite() || rate <= 0.0 {
            log::warn!(
                "AudioResampler::set_output_rate: ignoring invalid rate {}",
                rate
            );
            return;
        }
        let rate_usize = (rate.round() as usize).max(1);
        if rate_usize != self.output_rate {
            self.output_rate = rate_usize;
            self.needs_rebuild = true;
        }
    }

    /// Rebuild the resampler with current parameters
    fn trigger_rebuild(&mut self) {
        let effective_source_f32 = self.source_rate as f32 * self.speed;
        let effective_source = (effective_source_f32.round() as usize).max(1);
        let quality = self.pending_quality.unwrap_or_else(|| self.inner.quality());
        let filter = self.filter;
        let cutoff_ratio = self.cutoff_ratio;
        let output_rate = self.output_rate;

        let (tx, rx) = crossbeam::channel::bounded(1);
        std::thread::spawn(move || {
            let result = Self::create_resampler(
                quality,
                filter,
                cutoff_ratio,
                effective_source,
                output_rate,
            );
            let _ = tx.send(result);
        });

        self.rebuild_rx = Some(rx);
    }

    fn apply_rebuild_result(&mut self, result: Result<ResamplerInner<T>, ResamplerError>) {
        if self.input_pos > 0 {
            self.process_chunk();
        }
        let save_count = self.output_available.min(64);

        self.crossfade_buffer = [(T::zero(), T::zero()); 64];
        for i in 0..save_count {
            let pos = self.output_read_pos + i;
            if pos < self.output_buffers[0].len() {
                let l = self.output_buffers[0]
                    .get(pos)
                    .copied()
                    .unwrap_or(T::zero());
                let r = self.output_buffers[1]
                    .get(pos)
                    .copied()
                    .unwrap_or(T::zero());
                self.crossfade_buffer[i] = (l, r);
            }
        }
        self.crossfade_pos = 0;
        self.crossfade_remaining = save_count;
        self.crossfade_blend_total = save_count.max(1);

        match result {
            Ok(new_inner) => {
                self.inner = new_inner;
                self.allocate_buffers();

                self.pending_quality = None;
                self.needs_rebuild = false;
                self.rebuild_failures = 0;
                self.disabled = false;

                self.rebuilt_effective_source = self.compute_effective_source_rate();
                self.rebuilt_output_rate = self.output_rate;
                self.rebuilt_quality = self.inner.quality();
            }
            Err(e) => {
                self.rebuild_failures += 1;
                log::error!(
                    "Failed to rebuild resampler ({}/{}), will retry on next feed: {}",
                    self.rebuild_failures,
                    MAX_REBUILD_FAILURES,
                    e
                );
            }
        }
    }

    fn compute_effective_source_rate(&self) -> usize {
        let effective = self.source_rate as f32 * self.speed;
        (effective.round() as usize).max(1)
    }

    /// Flush all pending samples through the resampler
    pub fn flush(&mut self) {
        if self.input_pos > 0 {
            self.process_chunk();
        }
    }

    /// Reset all state
    pub fn reset(&mut self) {
        self.input_pos = 0;
        self.output_read_pos = 0;
        self.output_available = 0;
        self.needs_rebuild = false;
        self.rebuild_rx = None;
        self.crossfade_buffer = [(T::zero(), T::zero()); 64];
        self.crossfade_pos = 0;
        self.crossfade_remaining = 0;
        for ch in 0..CHANNELS {
            self.input_buffers[ch].fill(T::zero());
            self.output_buffers[ch].fill(T::zero());
        }
        self.disabled = false;
        self.rebuild_failures = 0;
        self.dropped_frames = 0;
    }

    /// Frames this resampler has discarded since construction or the last
    /// [`Self::reset`] — the input frames of chunks it could not process, plus
    /// any output frames dropped to keep a stalled consumer's backlog within
    /// the buffer. Non-zero means the last stretch of output is not a faithful
    /// resample of the input.
    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames
    }

    /// Check if source and output rates match (passthrough possible)
    pub fn is_passthrough(&self) -> bool {
        self.source_rate == self.output_rate && (self.speed - 1.0).abs() < 0.001
    }

    /// Filter group delay in output frames (`rubato::Resampler::output_delay`).
    pub fn filter_delay_samples(&self) -> f64 {
        if self.disabled || self.is_passthrough() {
            0.0
        } else {
            self.inner.output_delay() as f64
        }
    }

    /// Buffering delay in output frames from input chunk accumulation.
    pub fn buffering_delay_samples(&self) -> f64 {
        if self.disabled || self.is_passthrough() {
            0.0
        } else {
            let ratio =
                self.output_rate as f64 / self.compute_effective_source_rate().max(1) as f64;
            self.inner.input_frames_next() as f64 * ratio
        }
    }

    /// Implementation pipeline delay in output frames.
    pub fn pipeline_delay_samples(&self) -> f64 {
        0.0
    }

    /// Authoritative total latency in output frames:
    /// filter group delay + pipeline delay + buffering delay.
    pub fn latency_samples_exact(&self) -> f64 {
        self.filter_delay_samples() + self.pipeline_delay_samples() + self.buffering_delay_samples()
    }

    /// Authoritative filter group delay in output frames, straight from the
    /// underlying rubato resampler (`Resampler::output_delay`). Zero when the
    /// resampler is disabled or in passthrough mode (no conversion is
    /// performed, so no filter delay is introduced).
    pub fn latency_samples(&self) -> usize {
        if self.disabled || self.is_passthrough() {
            0
        } else {
            self.inner.output_delay()
        }
    }

    /// Latency in samples at the specified sample rate including filter group delay
    /// and buffering delay.
    pub fn latency_samples_at(&self, sample_rate: f64) -> f64 {
        if self.disabled || self.is_passthrough() {
            return 0.0;
        }
        let out_sr = self.output_rate.max(1) as f64;
        let delay_out = self.latency_samples_exact();
        if sample_rate > 0.0 && (sample_rate - out_sr).abs() > 0.01 {
            delay_out * (sample_rate / out_sr)
        } else {
            delay_out
        }
    }

    /// Group delay in milliseconds at the output sample rate.
    pub fn latency_ms(&self) -> f32 {
        self.latency_samples() as f32 / self.output_rate.max(1) as f32 * 1000.0
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled
    }
}

impl<T: rubato::Sample + Float + Default + Send + Sync + 'static> super::LatencyProvider
    for AudioResampler<T>
{
    fn latency_samples(&self, sample_rate: f64) -> f64 {
        self.latency_samples_at(sample_rate)
    }
}

/// Unified resampler container supporting both f32 (Performance) and f64 (Quality) modes.
// The f64 variant carries the full double-precision state; the size gap is
// intrinsic to supporting both precisions in one handle.
#[allow(clippy::large_enum_variant)]
pub enum GenericResampler {
    F32(AudioResamplerF32),
    F64(AudioResamplerF64),
}

impl GenericResampler {
    #[inline]
    pub fn is_passthrough(&self) -> bool {
        match self {
            Self::F32(r) => r.is_passthrough(),
            Self::F64(r) => r.is_passthrough(),
        }
    }

    #[inline]
    pub fn is_disabled(&self) -> bool {
        match self {
            Self::F32(r) => r.is_disabled(),
            Self::F64(r) => r.is_disabled(),
        }
    }

    #[inline]
    pub fn set_speed(&mut self, speed: f32) {
        match self {
            Self::F32(r) => r.set_speed(speed),
            Self::F64(r) => r.set_speed(speed),
        }
    }

    #[inline]
    pub fn speed(&self) -> f32 {
        match self {
            Self::F32(r) => r.speed(),
            Self::F64(r) => r.speed(),
        }
    }

    #[inline]
    pub fn set_quality(&mut self, quality: ResamplerQuality) {
        match self {
            Self::F32(r) => r.set_quality(quality),
            Self::F64(r) => r.set_quality(quality),
        }
    }

    /// Set the filter family on either precision path (triggers a rebuild).
    #[inline]
    pub fn set_filter(&mut self, filter: ResamplerFilter) {
        match self {
            Self::F32(r) => r.set_filter(filter),
            Self::F64(r) => r.set_filter(filter),
        }
    }

    /// Set the polyphase cutoff ratio on either precision path.
    #[inline]
    pub fn set_cutoff_ratio(&mut self, cutoff_ratio: f64) {
        match self {
            Self::F32(r) => r.set_cutoff_ratio(cutoff_ratio),
            Self::F64(r) => r.set_cutoff_ratio(cutoff_ratio),
        }
    }

    /// The configured polyphase cutoff ratio.
    #[inline]
    pub fn cutoff_ratio(&self) -> f64 {
        match self {
            Self::F32(r) => r.cutoff_ratio(),
            Self::F64(r) => r.cutoff_ratio(),
        }
    }

    /// Requested quality profile (the one the caller asked for).
    #[inline]
    pub fn requested_quality(&self) -> ResamplerQuality {
        match self {
            Self::F32(r) => r.requested_quality(),
            Self::F64(r) => r.requested_quality(),
        }
    }

    /// Effective (actually running) quality profile. May be lower than
    /// `requested_quality` if construction fell back.
    #[inline]
    pub fn effective_quality(&self) -> ResamplerQuality {
        match self {
            Self::F32(r) => r.effective_quality(),
            Self::F64(r) => r.effective_quality(),
        }
    }

    /// True when the running resampler is not the quality the caller asked
    /// for (silent downgrade after a construction failure).
    #[inline]
    pub fn quality_fell_back(&self) -> bool {
        match self {
            Self::F32(r) => r.quality_fell_back(),
            Self::F64(r) => r.quality_fell_back(),
        }
    }

    /// Filter family the caller requested.
    #[inline]
    pub fn requested_filter(&self) -> ResamplerFilter {
        match self {
            Self::F32(r) => r.requested_filter(),
            Self::F64(r) => r.requested_filter(),
        }
    }

    /// Filter family actually running.
    #[inline]
    pub fn effective_filter(&self) -> ResamplerFilter {
        match self {
            Self::F32(r) => r.effective_filter(),
            Self::F64(r) => r.effective_filter(),
        }
    }

    /// True when a polyphase request fell back to the FFT engine.
    #[inline]
    pub fn filter_fell_back(&self) -> bool {
        match self {
            Self::F32(r) => r.filter_fell_back(),
            Self::F64(r) => r.filter_fell_back(),
        }
    }

    /// Measured filter metadata, if the running engine can introspect it.
    #[inline]
    pub fn filter_metadata(&self) -> Option<ResamplerFilterMetadata> {
        match self {
            Self::F32(r) => r.filter_metadata(),
            Self::F64(r) => r.filter_metadata(),
        }
    }

    #[inline]
    pub fn set_source_rate(&mut self, rate: f32) {
        match self {
            Self::F32(r) => r.set_source_rate(rate),
            Self::F64(r) => r.set_source_rate(rate),
        }
    }

    #[inline]
    pub fn set_output_rate(&mut self, rate: f32) {
        match self {
            Self::F32(r) => r.set_output_rate(rate),
            Self::F64(r) => r.set_output_rate(rate),
        }
    }

    #[inline]
    pub fn reset(&mut self) {
        match self {
            Self::F32(r) => r.reset(),
            Self::F64(r) => r.reset(),
        }
    }

    #[inline]
    pub fn flush(&mut self) {
        match self {
            Self::F32(r) => r.flush(),
            Self::F64(r) => r.flush(),
        }
    }

    #[inline]
    pub fn available_output(&self) -> usize {
        match self {
            Self::F32(r) => r.available_output(),
            Self::F64(r) => r.available_output(),
        }
    }

    /// Authoritative filter group delay in output frames.
    #[inline]
    pub fn latency_samples(&self) -> usize {
        match self {
            Self::F32(r) => r.latency_samples(),
            Self::F64(r) => r.latency_samples(),
        }
    }

    /// Filter group delay in milliseconds at the output sample rate.
    #[inline]
    pub fn latency_ms(&self) -> f32 {
        match self {
            Self::F32(r) => r.latency_ms(),
            Self::F64(r) => r.latency_ms(),
        }
    }

    #[inline]
    pub fn feed_f32(&mut self, left: f32, right: f32) {
        match self {
            Self::F32(r) => r.feed(left, right),
            Self::F64(r) => r.feed(left as f64, right as f64),
        }
    }

    #[inline]
    pub fn feed_f64(&mut self, left: f64, right: f64) {
        match self {
            Self::F32(r) => r.feed(left as f32, right as f32),
            Self::F64(r) => r.feed(left, right),
        }
    }

    #[inline]
    pub fn read(&mut self) -> Option<(f32, f32)> {
        self.read_f32()
    }

    #[inline]
    pub fn read_f32(&mut self) -> Option<(f32, f32)> {
        match self {
            Self::F32(r) => r.read(),
            Self::F64(r) => r.read().map(|(l, r)| (l as f32, r as f32)),
        }
    }

    /// Latency in samples at the specified sample rate.
    #[inline]
    pub fn latency_samples_at(&self, sample_rate: f64) -> f64 {
        match self {
            Self::F32(r) => r.latency_samples_at(sample_rate),
            Self::F64(r) => r.latency_samples_at(sample_rate),
        }
    }

    #[inline]
    pub fn read_f64(&mut self) -> Option<(f64, f64)> {
        match self {
            Self::F32(r) => r.read().map(|(l, r)| (l as f64, r as f64)),
            Self::F64(r) => r.read(),
        }
    }
}

impl super::LatencyProvider for GenericResampler {
    fn latency_samples(&self, sample_rate: f64) -> f64 {
        self.latency_samples_at(sample_rate)
    }
}
