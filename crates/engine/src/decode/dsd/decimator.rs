//! DSD-to-PCM decimation: a five-stage 2:1 FIR cascade.

use config::DsdNoiseFilter;
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

/// Window shape used to taper a linear-phase prototype filter.
#[derive(Clone, Copy)]
enum WindowShape {
    Blackman,
    Hann,
    /// Kaiser window with the given β (higher β = deeper stopband, wider
    /// transition).
    Kaiser(f32),
}

/// One selectable decimation design: a prototype low-pass shape applied to
/// every stage of the 2:1 cascade.
///
/// `cutoff` is a fraction of the stage's *input* rate; 0.25 is the half-band
/// design (the output Nyquist). Lower values trade audio bandwidth for a
/// flatter/earlier ultrasonic roll-off.
#[derive(Clone, Copy)]
struct FilterDesign {
    taps: usize,
    cutoff: f32,
    window: WindowShape,
    min_phase: bool,
}

impl FilterDesign {
    fn for_filter(filter: DsdNoiseFilter) -> Self {
        match filter {
            DsdNoiseFilter::Standard => Self {
                taps: 63,
                cutoff: 0.25,
                window: WindowShape::Blackman,
                min_phase: false,
            },
            DsdNoiseFilter::Low => Self {
                taps: 95,
                cutoff: 0.21,
                window: WindowShape::Blackman,
                min_phase: false,
            },
            DsdNoiseFilter::HighOrder => Self {
                taps: 127,
                cutoff: 0.24,
                window: WindowShape::Kaiser(9.0),
                min_phase: false,
            },
            DsdNoiseFilter::Brickwall => Self {
                taps: 191,
                cutoff: 0.249,
                window: WindowShape::Kaiser(10.0),
                min_phase: false,
            },
            DsdNoiseFilter::SlowLp => Self {
                taps: 31,
                cutoff: 0.25,
                window: WindowShape::Hann,
                min_phase: false,
            },
            DsdNoiseFilter::SlowMp => Self {
                taps: 31,
                cutoff: 0.25,
                window: WindowShape::Hann,
                min_phase: true,
            },
        }
    }

    /// Build the normalized prototype coefficients.
    fn coefficients(self) -> Vec<f32> {
        let n = self.taps;
        let center = (n - 1) as f32 * 0.5;
        let mut coeffs = Vec::with_capacity(n);
        for i in 0..n {
            let x = i as f32 - center;
            // Ideal low-pass: h[n] = 2·fc·sinc(2·fc·n).
            let arg = 2.0 * self.cutoff * x;
            let sinc = if arg.abs() < 1e-6 {
                1.0
            } else {
                (std::f32::consts::PI * arg).sin() / (std::f32::consts::PI * arg)
            };
            let window = match self.window {
                WindowShape::Blackman => {
                    let phase = i as f32 / (n - 1) as f32;
                    0.42 - 0.5 * (2.0 * std::f32::consts::PI * phase).cos()
                        + 0.08 * (4.0 * std::f32::consts::PI * phase).cos()
                }
                WindowShape::Hann => {
                    let phase = i as f32 / (n - 1) as f32;
                    0.5 - 0.5 * (2.0 * std::f32::consts::PI * phase).cos()
                }
                WindowShape::Kaiser(beta) => {
                    let alpha = center;
                    let r = (i as f32 - alpha) / alpha;
                    let arg = 1.0 - r * r;
                    let arg = if arg > 0.0 { arg.sqrt() } else { 0.0 };
                    bessel_i0(beta * arg) / bessel_i0(beta)
                }
            };
            coeffs.push(2.0 * self.cutoff * sinc * window);
        }
        normalize_dc(&mut coeffs);
        if self.min_phase {
            coeffs = minimum_phase(&coeffs);
        }
        coeffs
    }
}

/// Modified Bessel function of the first kind, order zero (Numerical Recipes
/// polynomial approximation; accurate to ~1e-6 for the β range windows use).
fn bessel_i0(x: f32) -> f32 {
    let ax = x.abs();
    if ax < 3.75 {
        let y = (x / 3.75) * (x / 3.75);
        1.0 + y
            * (3.515_622_9
                + y * (3.089_942_4
                    + y * (1.206_749_2 + y * (0.265_973_2 + y * (0.036_076_8 + y * 0.004_581_3)))))
    } else {
        let y = 3.75 / ax;
        (ax.exp() / ax.sqrt())
            * (0.398_942_3
                + y * (0.013_285_92
                    + y * (0.002_253_19
                        + y * (-0.001_575_65
                            + y * (0.009_162_81
                                + y * (-0.020_577_06
                                    + y * (0.026_355_37
                                        + y * (-0.016_476_33 + y * 0.003_923_77))))))))
    }
}

/// Scale an FIR to exactly unity DC gain.
fn normalize_dc(coeffs: &mut [f32]) {
    let sum: f32 = coeffs.iter().sum();
    if sum.abs() > 1e-12 {
        for coefficient in coeffs.iter_mut() {
            *coefficient /= sum;
        }
    }
}

/// Convert a linear-phase FIR prototype to its minimum-phase equivalent.
///
/// Uses the real-cepstrum method: the minimum-phase spectrum is
/// `exp(FFT(folded IFFT(ln|H|)))`, and its inverse FFT is the causal impulse
/// response. Magnitude response — and therefore passband/stopband behaviour —
/// is preserved; only the phase (and the group delay) changes.
///
/// Runs at decoder construction on the control thread, so the FFT plans and
/// temporary buffers are allowed to allocate.
fn minimum_phase(linear: &[f32]) -> Vec<f32> {
    let taps = linear.len();
    // Zero-padding well past the filter length keeps the cepstral fold from
    // aliasing into the causal half.
    let size = (taps * 8).next_power_of_two().max(256);
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(size);
    let ifft = planner.plan_fft_inverse(size);

    let mut spectrum = vec![Complex::new(0.0f32, 0.0); size];
    for (i, &h) in linear.iter().enumerate() {
        spectrum[i] = Complex::new(h, 0.0);
    }
    fft.process(&mut spectrum);

    let mut cepstrum = vec![Complex::new(0.0f32, 0.0); size];
    for (dst, src) in cepstrum.iter_mut().zip(spectrum.iter()) {
        let magnitude = src.norm().max(1e-9);
        dst.re = magnitude.ln();
    }
    ifft.process(&mut cepstrum);
    // rustfft's inverse is unnormalized.
    for value in &mut cepstrum {
        *value /= size as f32;
    }

    // Fold the real cepstrum: keep the causal half doubled, zero the rest.
    let half = size / 2;
    let mut folded = vec![Complex::new(0.0f32, 0.0); size];
    folded[0] = Complex::new(cepstrum[0].re, 0.0);
    for i in 1..half {
        folded[i] = Complex::new(2.0 * cepstrum[i].re, 0.0);
    }
    folded[half] = Complex::new(cepstrum[half].re, 0.0);

    fft.process(&mut folded);
    for value in &mut folded {
        *value = value.exp();
    }
    ifft.process(&mut folded);

    let mut out: Vec<f32> = folded[..taps]
        .iter()
        .map(|value| value.re / size as f32)
        .collect();
    normalize_dc(&mut out);
    out
}

/// One 2:1 FIR stage in the DSD decimation cascade.
///
/// Keeping the stages explicit makes the passband/stopband contract testable:
/// five 2:1 stages provide the overall 32:1 conversion while each stage only
/// needs to reject the next octave of energy.
pub(crate) struct FirStage {
    pub(crate) coeffs: Vec<f32>,
    history: Vec<f32>,
    flat_buf: Vec<f32>,
}

impl FirStage {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self::with_design(FilterDesign::for_filter(DsdNoiseFilter::Standard))
    }

    fn with_design(design: FilterDesign) -> Self {
        let coeffs = design.coefficients();
        let taps = coeffs.len();
        Self {
            coeffs,
            history: vec![0.0; taps - 1],
            flat_buf: vec![0.0; taps],
        }
    }

    pub(crate) fn reset(&mut self) {
        self.history.fill(0.0);
        self.flat_buf.fill(0.0);
    }

    pub(crate) fn process(&mut self, input: &[f32], output: &mut Vec<f32>) {
        output.clear();
        let taps = self.coeffs.len();
        let history_len = self.history.len();
        let total = history_len + input.len();
        let output_count = if total < taps {
            0
        } else {
            (total - taps) / 2 + 1
        };
        // Decoder-owned stage buffers are preallocated at construction. The
        // fallback below only preserves this private helper's historical API
        // for diagnostic vectors; the realtime decimator path never takes it.
        if output_count > output.capacity() {
            output.reserve(output_count - output.capacity());
        }

        for output_index in 0..output_count {
            let start = output_index * 2;
            let sum = if start >= history_len {
                let in_start = start - history_len;
                crate::dsp::simd::dispatch::dispatch_dot_product(
                    &input[in_start..in_start + taps],
                    &self.coeffs,
                    taps,
                )
            } else {
                let hist_part_len = history_len - start;
                self.flat_buf[..hist_part_len].copy_from_slice(&self.history[start..]);
                let in_part_len = taps - hist_part_len;
                self.flat_buf[hist_part_len..taps].copy_from_slice(&input[..in_part_len]);
                crate::dsp::simd::dispatch::dispatch_dot_product(&self.flat_buf, &self.coeffs, taps)
            };
            output.push(sum);
        }

        // Retain exactly the tail needed to make the next call mathematically
        // equivalent to one-shot processing.
        if input.len() >= history_len {
            self.history
                .copy_from_slice(&input[input.len() - history_len..]);
        } else if !input.is_empty() {
            let keep = history_len - input.len();
            self.history.copy_within(history_len - keep.., 0);
            self.history[keep..].copy_from_slice(input);
        }
    }
}

/// DSD-to-PCM decimation engine with a five-stage 2:1 FIR cascade.
///
/// Converts a 1-bit DSD bitstream down to 32-bit float PCM at 88.2 kHz
/// (DSD64) or 176.4 kHz (DSD128): a 32:1 overall decimation.
///
/// # Quantitative response contract
///
/// The default [`DsdNoiseFilter::Standard`] cascade is five 63-tap
/// Blackman-windowed half-band low-pass stages with:
/// - cutoff at 0.25× the stage input rate (half-band),
/// - DC gain 1.0 (0 dB) — the cascade is DC-transparent,
/// - passband ripple < 0.1 dB out to 20 kHz (DSD64),
/// - stopband rejection > 40 dB at 100 kHz (DSD64),
/// - linear phase (symmetric coefficients), giving a constant group delay of
///   961 input samples = 30.03 samples at the 32× output rate.
///
/// The other [`DsdNoiseFilter`] variants change the prototype length, corner
/// frequency, window, and (for `SlowMp`) the phase. Each variant's realized
/// passband/stopband numbers are measured by `decode::dsd::tests`; nothing in
/// the UI or README is allowed to assert a figure those tests do not check.
///
/// The 1-bit stream is mapped to ±2.0 (the standard +6 dB DSD reference
/// scale) before filtering; only the final output stage is clamped to
/// [-1, 1], while the FIR cascade itself runs unclipped so response
/// measurements are not distorted by saturation.
pub struct DsdToPcmDecimator {
    /// Per-channel FIR cascade: `stages[channel][stage]`.
    pub(crate) stages: Vec<Vec<FirStage>>,
    /// Per-channel, per-stage scratch buffers.
    buffers: Vec<Vec<Vec<f32>>>,
    /// Per-channel scratch for unpacked 1-bit DSD samples.
    scratch: Vec<Vec<f32>>,
    /// The filter design this decimator was built with.
    filter: DsdNoiseFilter,
}

/// Run one channel's five-stage 2:1 FIR cascade.
fn process_channel(
    stages: &mut [FirStage],
    buffers: &mut [Vec<f32>],
    input: &[f32],
    output: &mut Vec<f32>,
) {
    for index in 0..stages.len() {
        if index == 0 {
            stages[index].process(input, &mut buffers[index]);
        } else {
            let (previous, next) = buffers.split_at_mut(index);
            stages[index].process(&previous[index - 1], &mut next[0]);
        }
    }
    output.extend_from_slice(
        buffers
            .last()
            .expect("DSD decimator must have at least one stage"),
    );
}

impl DsdToPcmDecimator {
    const STAGES: usize = 5;

    /// Create a decimator for `channels` channels (32× downsampling) with the
    /// default [`DsdNoiseFilter::Standard`] cascade.
    pub fn new(channels: usize) -> Self {
        Self::new_with_filter(channels, DsdNoiseFilter::Standard)
    }

    /// Create a decimator with an explicit noise-filter design.
    pub fn new_with_filter(channels: usize, filter: DsdNoiseFilter) -> Self {
        let channels = channels.max(1);
        let design = FilterDesign::for_filter(filter);
        let stages = (0..channels)
            .map(|_| {
                (0..Self::STAGES)
                    .map(|_| FirStage::with_design(design))
                    .collect()
            })
            .collect();
        let buffers = (0..channels)
            .map(|_| {
                (0..Self::STAGES)
                    .map(|i| Vec::with_capacity((65536usize >> (i + 1)).max(64)))
                    .collect()
            })
            .collect();
        let scratch = (0..channels).map(|_| Vec::with_capacity(65536)).collect();
        Self {
            stages,
            buffers,
            scratch,
            filter,
        }
    }

    /// Backward-compatible constructor for the stereo 32× decimator.
    pub fn new_32x() -> Self {
        Self::new(2)
    }

    /// The filter design this decimator runs.
    pub fn noise_filter(&self) -> DsdNoiseFilter {
        self.filter
    }

    /// Reset the filter delay lines (used when seeking: pre-seek history
    /// must not bleed into the new position).
    pub fn reset(&mut self) {
        for stage_chain in &mut self.stages {
            for stage in stage_chain {
                stage.reset();
            }
        }
        for channel_buffers in &mut self.buffers {
            for buffer in channel_buffers {
                buffer.clear();
            }
        }
        for scratch in &mut self.scratch {
            scratch.clear();
        }
    }

    /// Decimate one block of raw 1-bit DSD payload (one byte-slice per
    /// channel) to f32 PCM.
    ///
    /// `lsbf` selects the bit order inside each payload byte: `true` for
    /// LSB-first (bit 0 is the earliest DSD sample — the DSF/DFF default),
    /// `false` for MSB-first (DSF `bits_per_sample == 8`). `out` receives one
    /// `Vec<f32>` per channel; each is extended with the freshly decimated
    /// PCM tail. Fewer than 8 bytes per channel produce no output.
    pub fn decimate_channels(
        &mut self,
        channel_bytes: &[&[u8]],
        lsbf: bool,
        out: &mut [&mut Vec<f32>],
    ) {
        let nch = channel_bytes.len().min(out.len()).min(self.stages.len());
        if nch == 0 {
            return;
        }
        let num_bytes = channel_bytes.iter().map(|c| c.len()).min().unwrap_or(0);
        if num_bytes == 0 {
            return;
        }

        // Convert each byte into 8 float samples (+2.0 / -2.0): the standard
        // +6 dB DSD reference mapping (a 1-bit ±1 stream carries half the
        // energy of a full-scale PCM signal, so ±2.0 normalizes it to unity
        // amplitude after the DC-transparent FIR cascade). The final output
        // stage is clamped to [-1, 1] so PCM-range overflow is prevented while
        // the FIR cascade supplies the ultrasonic rejection.
        const DSD_SCALE: f32 = 2.0;
        let required = num_bytes.saturating_mul(8);
        for (ch, scratch) in self.scratch.iter_mut().enumerate().take(nch) {
            if required > scratch.capacity() {
                log::warn!(
                    "DSD block exceeds preallocated decimator capacity ({} samples); dropping block",
                    required
                );
                return;
            }
            scratch.clear();
            for &b in channel_bytes[ch].iter().take(num_bytes) {
                for bit_idx in 0..8u32 {
                    let bit = if lsbf { bit_idx } else { 7 - bit_idx };
                    scratch.push(if b & (1 << bit) != 0 {
                        DSD_SCALE
                    } else {
                        -DSD_SCALE
                    });
                }
            }
        }

        for (ch, scratch) in self.scratch.iter().enumerate().take(nch) {
            process_channel(
                &mut self.stages[ch],
                &mut self.buffers[ch],
                scratch,
                out[ch],
            );
            // The output stage is the only place where the DSD +6 dB
            // intermediate scale is constrained to PCM range. The internal FIR
            // values remain unclipped so response measurements are not
            // distorted by saturation.
            let tail = self.buffers[ch].last().map_or(0, Vec::len);
            let start = out[ch].len().saturating_sub(tail);
            for sample in &mut out[ch][start..] {
                *sample = sample.clamp(-1.0, 1.0);
            }
        }
    }
}
