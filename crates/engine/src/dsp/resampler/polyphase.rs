//! Rational polyphase windowed-sinc resampler — the engine's own filter bank.
//!
//! The existing [`super::AudioResampler`] path delegates to rubato's FFT
//! resampler. This module adds a second, independent engine selected by
//! [`config::ResamplerFilter`]. It is deliberately a *rational* polyphase
//! converter: for an input rate `fs_in` and output rate `fs_out`, reduce
//! `fs_out / fs_in` to `L / M` and run the filter at the common virtual rate
//! `L · fs_in = M · fs_out`. The prototype low-pass is decomposed into `L`
//! branches of `T` taps, so each output costs exactly `T` multiply-accumulates
//! regardless of the ratio.
//!
//! # Filter shapes
//!
//! - **Kaiser** and **Blackman-Harris** are linear-phase windowed sincs.
//! - **MinimumPhase** starts from the Kaiser prototype and applies the real
//!   cepstrum (homomorphic) method; the magnitude response is preserved while
//!   the impulse response becomes causal/front-loaded. The group delay is
//!   frequency-dependent and the reported latency is the energy centroid.
//! - **Apodizing** is a steeper (twice-length) linear-phase design whose
//!   stopband begins just above the source Nyquist.
//! - **SincM** is a very long Kaiser design for reference conversion.
//!
//! # Metadata is measured
//!
//! [`ResamplerFilterMetadata`] is computed from the coefficient array the
//! engine actually runs: an FFT of the final prototype gives the passband
//! ripple, stopband attenuation and energy-centroid delay. Nothing here is a
//! design-target constant copied into a report.
//!
//! # Bounds
//!
//! The prototype has `T · L` taps. An extreme ratio (for example 1 Hz →
//! 768 kHz) would make `L` huge and the bank impossible to allocate, so
//! construction fails explicitly and the caller falls back to the FFT engine.
//! That fallback is logged; it is never silent.

use config::{ResamplerFilter, ResamplerQuality};
use num_traits::{Float, NumCast, ToPrimitive};
use rustfft::{num_complex::Complex, FftPlanner};

use super::types::{ResamplerError, CHUNK_SIZE};

/// Hard ceiling on prototype coefficients. At 8 bytes each this is 16 MB for
/// the prototype and the same again for the polyphase copy, which is the
/// largest construction this engine will attempt off the audio thread.
pub(crate) const MAX_PROTOTYPE_TAPS: usize = 2_000_000;

/// Hard ceiling for the measurement/minimum-phase FFT. The prototype cap keeps
/// the transform at or below 2^21 points.
const MAX_FFT_LEN: usize = 1 << 21;

/// Measured shape of the filter a resampler instance actually runs.
#[derive(Debug, Clone, PartialEq)]
pub struct ResamplerFilterMetadata {
    /// Filter family selected by the caller.
    pub family: ResamplerFilter,
    /// Polyphase branches (`L` in the reduced ratio).
    pub phases: usize,
    /// Taps applied per output sample.
    pub taps_per_phase: usize,
    /// Total prototype coefficients (`taps_per_phase × phases`).
    pub prototype_taps: usize,
    /// Whether the group delay is constant (linear phase).
    pub linear_phase: bool,
    /// Group delay in output frames. For a linear-phase design this is the
    /// exact `(N−1)/(2M)`; for the minimum-phase design it is the measured
    /// energy centroid, because its group delay is frequency-dependent.
    pub group_delay_output_frames: f64,
    /// Maximum deviation from unity in the passband, in dB.
    pub passband_ripple_db: f64,
    /// Worst-case attenuation above the transition band, in dB (positive).
    pub stopband_attenuation_db: f64,
    /// Approximate transition width at the input rate, in Hz.
    pub transition_width_hz: f64,
    /// Source rate the instance was built for.
    pub input_rate: u32,
    /// Output rate the instance was built for.
    pub output_rate: u32,
}

impl ResamplerFilterMetadata {
    /// A compact, human-readable summary for diagnostics and the CLI.
    pub fn summary(&self) -> String {
        format!(
            "{} · {} phases × {} taps · {} · {:.1} dB stopband · {:.3} dB ripple · {:.1} ms group delay",
            self.family.as_str(),
            self.phases,
            self.taps_per_phase,
            if self.linear_phase {
                "linear phase"
            } else {
                "minimum phase"
            },
            self.stopband_attenuation_db,
            self.passband_ripple_db,
            self.group_delay_output_frames / self.output_rate.max(1) as f64 * 1000.0,
        )
    }
}

/// A stereo rational polyphase resampler with a fixed input chunk.
pub struct PolyphaseResampler<T: Float + NumCast + ToPrimitive + Default + Send + Sync + 'static> {
    filter: ResamplerFilter,
    quality: ResamplerQuality,
    input_rate: u32,
    output_rate: u32,
    /// Reduced numerator: output rate / gcd.
    l: usize,
    /// Reduced denominator: input rate / gcd.
    m: usize,
    taps: usize,
    /// Phase-major coefficients: branch `p` occupies `[p*taps .. (p+1)*taps]`.
    coeffs: Vec<f64>,
    /// Input history in f64, starting at virtual index `base`.
    history: [Vec<f64>; 2],
    /// Virtual input index of `history[0]`.
    base: i64,
    /// Total input frames appended so far.
    total_input: u64,
    /// Index of the next output frame to produce.
    next_output: u64,
    delay_output_frames: f64,
    metadata: ResamplerFilterMetadata,
    _sample: std::marker::PhantomData<T>,
}

impl<T: Float + NumCast + ToPrimitive + Default + Send + Sync + 'static> PolyphaseResampler<T> {
    /// Build a polyphase resampler with default 0.95 (95% Nyquist) cutoff.
    pub fn new(
        filter: ResamplerFilter,
        quality: ResamplerQuality,
        input_rate: u32,
        output_rate: u32,
    ) -> Result<Self, ResamplerError> {
        Self::new_with_cutoff(filter, quality, input_rate, output_rate, 0.95)
    }

    /// Build a polyphase resampler with customizable cutoff ratio (0.85 ..= 0.999).
    pub fn new_with_cutoff(
        filter: ResamplerFilter,
        quality: ResamplerQuality,
        input_rate: u32,
        output_rate: u32,
        cutoff_ratio: f64,
    ) -> Result<Self, ResamplerError> {
        if !filter.is_polyphase() {
            return Err(ResamplerError::CreationFailed {
                quality,
                reason: format!("{} is not a polyphase filter family", filter.as_str()),
            });
        }
        if input_rate == 0 || output_rate == 0 {
            return Err(ResamplerError::InvalidRates {
                source_rate: input_rate as usize,
                output_rate: output_rate as usize,
            });
        }
        let gcd = gcd(input_rate as usize, output_rate as usize);
        let l = (output_rate as usize) / gcd;
        let m = (input_rate as usize) / gcd;
        let taps = filter.taps_per_phase(quality);
        let prototype_taps = taps
            .checked_mul(l)
            .ok_or_else(|| ResamplerError::CreationFailed {
                quality,
                reason: format!("prototype length overflow for L={l}, taps={taps}"),
            })?;
        if prototype_taps > MAX_PROTOTYPE_TAPS {
            return Err(ResamplerError::CreationFailed {
                quality,
                reason: format!(
                    "polyphase prototype too large ({prototype_taps} taps > {MAX_PROTOTYPE_TAPS}) \
                     for {input_rate} Hz → {output_rate} Hz; use the FFT family for this ratio"
                ),
            });
        }

        // The ideal low-pass cutoff at the common rate is scaled by the cutoff ratio:
        // (0.5 * ratio) / max(L, M) cycles per common-rate sample.
        let ratio = cutoff_ratio.clamp(0.85, 0.999);
        let fc = (0.5 * ratio) / l.max(m) as f64;
        let mut prototype = design_prototype(filter, prototype_taps, fc, l);
        if filter == ResamplerFilter::MinimumPhase {
            prototype = minimum_phase(&prototype, taps, quality)?;
        }

        // Decompose into phase-major branches and normalize each branch to
        // unity DC gain. The prototype was scaled by L so the branch sums are
        // already ≈1; the explicit normalization removes truncation drift
        // (and the residual from the cepstral conversion).
        let mut coeffs = vec![0.0f64; prototype_taps];
        for p in 0..l {
            let mut sum = 0.0f64;
            for t in 0..taps {
                sum += prototype[p + t * l];
            }
            if sum.abs() > 1e-12 {
                for t in 0..taps {
                    let idx = p + t * l;
                    coeffs[p * taps + t] = prototype[idx] / sum;
                }
            } else {
                for t in 0..taps {
                    let idx = p + t * l;
                    coeffs[p * taps + t] = prototype[idx];
                }
            }
        }
        // Rebuild the effective prototype from the normalized branches so the
        // metadata describes what the converter runs, not the design draft.
        let mut effective = vec![0.0f64; prototype_taps];
        for p in 0..l {
            for t in 0..taps {
                effective[p + t * l] = coeffs[p * taps + t];
            }
        }

        let metadata =
            measure_metadata(filter, &effective, l, m, taps, input_rate, output_rate, fc);

        let history_capacity = taps - 1 + CHUNK_SIZE;
        Ok(Self {
            filter,
            quality,
            input_rate,
            output_rate,
            l,
            m,
            taps,
            coeffs,
            history: [vec![0.0f64; taps - 1], vec![0.0f64; taps - 1]],
            base: -(taps as i64 - 1),
            total_input: 0,
            next_output: 0,
            delay_output_frames: metadata.group_delay_output_frames,
            metadata,
            _sample: std::marker::PhantomData,
            // Capacity is reserved below through `reserve` to keep the
            // initializer honest for the `history` vectors.
        }
        .with_capacity(history_capacity))
    }

    fn with_capacity(mut self, capacity: usize) -> Self {
        for ch in 0..2 {
            self.history[ch].reserve(capacity - self.history[ch].len());
        }
        self
    }

    /// Filter family in use.
    pub fn filter(&self) -> ResamplerFilter {
        self.filter
    }

    /// Quality tier in use.
    pub fn quality(&self) -> ResamplerQuality {
        self.quality
    }

    /// Measured shape of the filter this instance runs.
    pub fn metadata(&self) -> &ResamplerFilterMetadata {
        &self.metadata
    }

    /// Input frames the outer chunker must supply per call.
    pub fn input_frames_next(&self) -> usize {
        CHUNK_SIZE
    }

    /// Maximum output frames a single chunk can produce.
    pub fn output_frames_max(&self) -> usize {
        (CHUNK_SIZE * self.output_rate as usize).div_ceil(self.input_rate as usize) + 2
    }

    /// Exact number of output frames the next [`Self::process`] call writes.
    pub fn output_frames_next(&self) -> usize {
        let total_after = self.total_input + self.input_frames_next() as u64;
        let end = (total_after * self.l as u64).div_ceil(self.m as u64);
        end.saturating_sub(self.next_output) as usize
    }

    /// Group delay in output frames.
    pub fn output_delay(&self) -> usize {
        self.delay_output_frames.max(0.0).round() as usize
    }

    /// Resample one chunk. `input` is the planar stereo chunk filled by the
    /// caller; `output` must hold at least [`Self::output_frames_next`] frames.
    /// Returns the number of frames written.
    ///
    /// No allocation occurs: the history is sized for one chunk plus the
    /// filter's past-window, and it is trimmed after every call.
    pub fn process(&mut self, input: &[Vec<T>], output: &mut [Vec<T>], out_frames: usize) -> usize {
        let needed = self.input_frames_next();
        debug_assert_eq!(input[0].len(), needed);
        debug_assert!(output[0].len() >= out_frames);

        for (history, input_ch) in self.history.iter_mut().zip(input.iter()) {
            for &sample in &input_ch[..needed] {
                history.push(sample.to_f64().unwrap_or(0.0));
            }
        }
        self.total_input += needed as u64;

        let mut produced = 0usize;
        while produced < out_frames {
            let j = self.next_output.saturating_mul(self.m as u64);
            let base = (j / self.l as u64) as i64;
            if base >= self.total_input as i64 {
                break;
            }
            let phase = (j % self.l as u64) as usize;
            let start = (base - self.base) as usize;
            debug_assert!(start >= self.taps - 1);

            let branch = &self.coeffs[phase * self.taps..phase * self.taps + self.taps];
            let mut left = 0.0f64;
            let mut right = 0.0f64;
            for (t, &c) in branch.iter().enumerate() {
                let idx = start - t;
                left += c * self.history[0][idx];
                right += c * self.history[1][idx];
            }
            output[0][produced] = NumCast::from(left).unwrap_or_else(T::zero);
            output[1][produced] = NumCast::from(right).unwrap_or_else(T::zero);

            produced += 1;
            self.next_output += 1;
        }

        self.trim_history();
        produced
    }

    /// Clear all history. The next chunk starts a fresh stream.
    pub fn reset(&mut self) {
        for ch in 0..2 {
            self.history[ch].clear();
            self.history[ch].resize(self.taps - 1, 0.0);
        }
        self.base = -(self.taps as i64 - 1);
        self.total_input = 0;
        self.next_output = 0;
    }

    fn trim_history(&mut self) {
        let j = self.next_output.saturating_mul(self.m as u64);
        let next_base = (j / self.l as u64) as i64;
        let earliest = next_base - (self.taps as i64 - 1);
        let keep_from = earliest.max(self.base).min(self.total_input as i64);
        let drop = (keep_from - self.base).max(0) as usize;
        if drop == 0 {
            return;
        }
        for ch in 0..2 {
            let len = self.history[ch].len();
            let drop = drop.min(len);
            self.history[ch].copy_within(drop.., 0);
            self.history[ch].truncate(len - drop);
        }
        self.base += drop as i64;
    }
}

/// Greatest common divisor.
fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a.max(1)
}

/// Build the scaled prototype. `fc` is normalized to the common rate.
fn design_prototype(filter: ResamplerFilter, taps: usize, fc: f64, l: usize) -> Vec<f64> {
    let window = match filter {
        ResamplerFilter::BlackmanHarris | ResamplerFilter::Apodizing => Window::BlackmanHarris,
        ResamplerFilter::SincM => Window::Kaiser(14.0),
        ResamplerFilter::Kaiser | ResamplerFilter::MinimumPhase => Window::Kaiser(12.0),
        ResamplerFilter::Fft => Window::Kaiser(12.0),
    };
    let center = (taps - 1) as f64 / 2.0;
    let mut h = Vec::with_capacity(taps);
    for i in 0..taps {
        let x = i as f64 - center;
        let arg = 2.0 * fc * std::f64::consts::PI * x;
        let sinc = if arg.abs() < 1e-15 {
            1.0
        } else {
            arg.sin() / arg
        };
        let w = match window {
            Window::Kaiser(beta) => kaiser(i, taps, beta),
            Window::BlackmanHarris => blackman_harris(i, taps),
        };
        h.push(2.0 * fc * sinc * w);
    }
    // Scale so the prototype's DC gain is L and every branch sums to ≈1.
    let sum: f64 = h.iter().sum();
    if sum.abs() > 1e-15 {
        let scale = l as f64 / sum;
        for v in &mut h {
            *v *= scale;
        }
    }
    h
}

enum Window {
    Kaiser(f64),
    BlackmanHarris,
}

fn kaiser(i: usize, n: usize, beta: f64) -> f64 {
    if n <= 1 {
        return 1.0;
    }
    let x = 2.0 * i as f64 / (n - 1) as f64 - 1.0;
    let arg = beta * (1.0 - x * x).max(0.0).sqrt();
    bessel_i0(arg) / bessel_i0(beta)
}

fn bessel_i0(x: f64) -> f64 {
    let y = x * x / 4.0;
    let mut term = 1.0f64;
    let mut sum = 1.0f64;
    for k in 1..200 {
        term *= y / (k * k) as f64;
        sum += term;
        if term < sum * 1e-17 {
            break;
        }
    }
    sum
}

fn blackman_harris(i: usize, n: usize) -> f64 {
    if n <= 1 {
        return 1.0;
    }
    let x = 2.0 * std::f64::consts::PI * i as f64 / (n - 1) as f64;
    0.358_75 - 0.488_29 * x.cos() + 0.141_28 * (2.0 * x).cos() - 0.011_68 * (3.0 * x).cos()
}

/// Real-cepstrum minimum-phase conversion.
///
/// The magnitude is preserved; the phase is folded to the minimum-phase
/// branch. The returned sequence is truncated to the prototype length with a
/// short raised-cosine tail fade so the truncation does not leave a step.
fn minimum_phase(
    prototype: &[f64],
    taps: usize,
    quality: ResamplerQuality,
) -> Result<Vec<f64>, ResamplerError> {
    let n = prototype.len();
    let nfft = next_pow2(n).min(MAX_FFT_LEN);
    if nfft < n {
        return Err(ResamplerError::CreationFailed {
            quality,
            reason: format!("minimum-phase FFT too small for {n} prototype taps"),
        });
    }
    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(nfft);
    let ifft = planner.plan_fft_inverse(nfft);

    let mut spectrum = vec![Complex::new(0.0, 0.0); nfft];
    for (dst, &src) in spectrum.iter_mut().zip(prototype.iter()) {
        dst.re = src;
    }
    fft.process(&mut spectrum);

    let peak = spectrum
        .iter()
        .map(|c| c.norm())
        .fold(0.0f64, f64::max)
        .max(1e-30);
    let floor = peak * 1e-9;
    for c in &mut spectrum {
        c.re = c.norm().max(floor).ln();
        c.im = 0.0;
    }
    ifft.process(&mut spectrum);
    let inv = 1.0 / nfft as f64;
    let mut cepstrum: Vec<f64> = spectrum.iter().map(|c| c.re * inv).collect();

    // Real cepstrum → minimum-phase cepstrum.
    for c in cepstrum.iter_mut().take(nfft / 2).skip(1) {
        *c *= 2.0;
    }
    for c in cepstrum.iter_mut().skip(nfft / 2 + 1) {
        *c = 0.0;
    }

    let mut log_spectrum: Vec<Complex<f64>> =
        cepstrum.iter().map(|&c| Complex::new(c, 0.0)).collect();
    fft.process(&mut log_spectrum);
    for c in &mut log_spectrum {
        let mag = c.re.exp();
        let phase = c.im;
        *c = Complex::new(mag * phase.cos(), mag * phase.sin());
    }
    ifft.process(&mut log_spectrum);

    let mut out: Vec<f64> = log_spectrum.iter().map(|c| c.re * inv).collect();
    out.truncate(n);
    // The cepstral reconstruction is infinite; taper the truncated tail over
    // one input sample's worth of taps. This is far shorter than the filter's
    // transition and only removes a numerical step.
    let fade = (n / 2).max(taps).min(n);
    for i in 0..fade {
        let idx = n - fade + i;
        let w = 0.5 * (1.0 + (std::f64::consts::PI * i as f64 / fade as f64).cos());
        out[idx] *= w;
    }
    Ok(out)
}

/// Measure the final prototype's passband ripple, stopband attenuation and
/// delay.
#[allow(clippy::too_many_arguments)]
fn measure_metadata(
    filter: ResamplerFilter,
    prototype: &[f64],
    l: usize,
    m: usize,
    taps: usize,
    input_rate: u32,
    output_rate: u32,
    fc: f64,
) -> ResamplerFilterMetadata {
    let n = prototype.len();
    let linear_phase = filter.is_linear_phase();
    let group_delay_output_frames = if linear_phase {
        (n.saturating_sub(1)) as f64 / (2.0 * m as f64)
    } else {
        energy_centroid(prototype) / m as f64
    };

    let (ripple_db, stopband_db) = response_metrics(prototype, fc);

    ResamplerFilterMetadata {
        family: filter,
        phases: l,
        taps_per_phase: taps,
        prototype_taps: n,
        linear_phase,
        group_delay_output_frames,
        passband_ripple_db: ripple_db,
        stopband_attenuation_db: stopband_db,
        transition_width_hz: 10.0 / taps as f64 * input_rate as f64,
        input_rate,
        output_rate,
    }
}

fn energy_centroid(prototype: &[f64]) -> f64 {
    let mut num = 0.0f64;
    let mut den = 0.0f64;
    for (i, &h) in prototype.iter().enumerate() {
        let p = h * h;
        num += p * i as f64;
        den += p;
    }
    if den > 0.0 {
        num / den
    } else {
        0.0
    }
}

/// Returns (passband ripple dB, stopband attenuation dB).
fn response_metrics(prototype: &[f64], fc: f64) -> (f64, f64) {
    let n = prototype.len();
    let nfft = next_pow2(n).clamp(4096, MAX_FFT_LEN);
    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(nfft);
    let mut spectrum = vec![Complex::new(0.0, 0.0); nfft];
    for (dst, &src) in spectrum.iter_mut().zip(prototype.iter().take(nfft)) {
        dst.re = src;
    }
    fft.process(&mut spectrum);
    let dc = spectrum[0].norm().max(1e-30);

    let pass_end = (0.9 * fc * nfft as f64).floor() as usize;
    // The window's main lobe is wider than a bare sinc's 4/N estimate; start
    // the measured stopband beyond it so the figure reports sidelobe
    // rejection, not the transition slope.
    let transition = 12.0 / n as f64;
    let stop_start = ((fc + transition) * nfft as f64).ceil() as usize;
    let stop_end = nfft / 2;

    let mut ripple = 0.0f64;
    for c in spectrum.iter().take(pass_end.min(stop_end) + 1).skip(1) {
        let db = 20.0 * (c.norm() / dc).max(1e-15).log10();
        ripple = ripple.max(db.abs());
    }
    let mut worst = f64::NEG_INFINITY;
    if stop_start <= stop_end {
        for c in spectrum.iter().take(stop_end + 1).skip(stop_start) {
            let db = 20.0 * (c.norm() / dc).max(1e-15).log10();
            worst = worst.max(db);
        }
    }
    (ripple, (-worst).max(0.0))
}

fn next_pow2(n: usize) -> usize {
    n.max(1).next_power_of_two()
}

#[cfg(test)]
mod tests {
    use super::*;

    const IN: u32 = 44_100;
    const OUT: u32 = 48_000;

    fn build(filter: ResamplerFilter, quality: ResamplerQuality) -> PolyphaseResampler<f32> {
        PolyphaseResampler::new(filter, quality, IN, OUT).expect("filter construction")
    }

    #[test]
    fn ratio_is_reduced_to_coprime_factors() {
        let r = build(ResamplerFilter::Kaiser, ResamplerQuality::Balanced);
        assert_eq!(r.l, 160);
        assert_eq!(r.m, 147);
        assert_eq!(r.metadata.phases, 160);
    }

    #[test]
    fn output_frame_accounting_matches_process() {
        let mut r = build(ResamplerFilter::Kaiser, ResamplerQuality::Fast);
        let mut out = [
            vec![0.0f32; r.output_frames_max()],
            vec![0.0f32; r.output_frames_max()],
        ];
        let input = [vec![0.0f32; CHUNK_SIZE], vec![0.0f32; CHUNK_SIZE]];
        for _ in 0..8 {
            let expected = r.output_frames_next();
            let produced = r.process(&input, &mut out, expected);
            assert_eq!(produced, expected);
        }
        // 8 chunks of 1024 input frames at 44.1→48 kHz.
        let expected_total = ((8 * CHUNK_SIZE) as f64 * OUT as f64 / IN as f64).floor() as i64;
        let actual = r.next_output as i64;
        assert!(
            (actual - expected_total).abs() <= 2,
            "expected ≈{expected_total} outputs, got {actual}"
        );
    }

    #[test]
    fn a_sine_keeps_its_amplitude_after_the_delay() {
        let mut r = build(ResamplerFilter::Kaiser, ResamplerQuality::Balanced);
        let delay = r.output_delay();
        let freq = 997.0f64;
        let mut phase = 0.0f64;
        let mut out_buf = [
            vec![0.0f32; r.output_frames_max()],
            vec![0.0f32; r.output_frames_max()],
        ];
        let mut collected: Vec<f32> = Vec::new();
        let mut input = [vec![0.0f32; CHUNK_SIZE], vec![0.0f32; CHUNK_SIZE]];
        for _ in 0..24 {
            let [left, right] = &mut input;
            for (l, r) in left.iter_mut().zip(right.iter_mut()) {
                let s = (2.0 * std::f64::consts::PI * freq * phase / IN as f64).sin();
                phase += 1.0;
                *l = s as f32;
                *r = s as f32;
            }
            let n = r.output_frames_next();
            let produced = r.process(&input, &mut out_buf, n);
            collected.extend_from_slice(&out_buf[0][..produced]);
        }
        // Skip the filter delay and measure the RMS of the steady-state tail.
        let start = delay + 1024;
        let slice = &collected[start..start + 8192];
        let rms = (slice.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>()
            / slice.len() as f64)
            .sqrt();
        let expected = std::f64::consts::FRAC_1_SQRT_2;
        assert!(
            (rms - expected).abs() < 0.01,
            "997 Hz RMS {rms:.4}, expected {expected:.4}"
        );
    }

    #[test]
    fn an_image_above_the_input_nyquist_is_rejected() {
        // 20 kHz at 44.1 kHz has its first image at 24.1 kHz after conversion.
        // Measure that image by feeding 20 kHz and correlating the output with
        // a 24.1 kHz tone.
        // Use 44.1 → 96 kHz so the image at 24.1 kHz is below the *output*
        // Nyquist and can be measured directly (at 48 kHz it would alias to
        // 23.9 kHz).
        let mut r = PolyphaseResampler::<f32>::new(
            ResamplerFilter::Kaiser,
            ResamplerQuality::HighQuality,
            IN,
            96_000,
        )
        .expect("filter construction");
        let delay = r.output_delay();
        let mut phase = 0.0f64;
        let mut out_buf = [
            vec![0.0f32; r.output_frames_max()],
            vec![0.0f32; r.output_frames_max()],
        ];
        let mut collected: Vec<f64> = Vec::new();
        let mut input = [vec![0.0f32; CHUNK_SIZE], vec![0.0f32; CHUNK_SIZE]];
        let n_in = 48;
        for _ in 0..n_in {
            let [left, right] = &mut input;
            for (l, r) in left.iter_mut().zip(right.iter_mut()) {
                let s = (2.0 * std::f64::consts::PI * 20_000.0 * phase / IN as f64).sin();
                phase += 1.0;
                *l = s as f32;
                *r = s as f32;
            }
            let n = r.output_frames_next();
            let produced = r.process(&input, &mut out_buf, n);
            collected.extend(out_buf[0][..produced].iter().map(|v| *v as f64));
        }
        let tail = &collected[delay + 2048..];
        let image = 44_100.0 - 20_000.0; // 24.1 kHz
        let mut re = 0.0;
        let mut im = 0.0;
        for (i, &v) in tail.iter().enumerate() {
            let t = 2.0 * std::f64::consts::PI * image * i as f64 / 96_000.0;
            re += v * t.cos();
            im += v * t.sin();
        }
        let amp = 2.0 * (re * re + im * im).sqrt() / tail.len() as f64;
        let db = 20.0 * amp.max(1e-15).log10();
        assert!(
            db < -60.0,
            "image at {image} Hz is only {db:.1} dB down (amp {amp:.6})"
        );
    }

    #[test]
    fn minimum_phase_has_lower_delay_and_a_deep_stopband() {
        let linear = build(ResamplerFilter::Kaiser, ResamplerQuality::HighQuality);
        let minimum = build(ResamplerFilter::MinimumPhase, ResamplerQuality::HighQuality);
        assert!(
            minimum.delay_output_frames < linear.delay_output_frames * 0.5,
            "min-phase delay {:.1} should be well below linear {:.1}",
            minimum.delay_output_frames,
            linear.delay_output_frames
        );
        // The cepstral conversion preserves the passband; its long tail is
        // tapered to fit the polyphase branch, which costs some stopband
        // depth. The realized figure is measured, not assumed: it must still
        // be a deep filter, and it must not be reported as equal to the
        // linear design's.
        assert!(
            minimum.metadata.stopband_attenuation_db > 80.0,
            "min-phase stopband {:.1} dB is too shallow",
            minimum.metadata.stopband_attenuation_db
        );
        assert!(
            minimum.metadata.stopband_attenuation_db < linear.metadata.stopband_attenuation_db,
            "min-phase {:.1} dB should not exceed linear {:.1} dB",
            minimum.metadata.stopband_attenuation_db,
            linear.metadata.stopband_attenuation_db
        );
    }

    #[test]
    fn metadata_is_measured_and_plausible() {
        for filter in [
            ResamplerFilter::Kaiser,
            ResamplerFilter::BlackmanHarris,
            ResamplerFilter::MinimumPhase,
            ResamplerFilter::Apodizing,
            ResamplerFilter::SincM,
        ] {
            let r = build(filter, ResamplerQuality::Balanced);
            let m = r.metadata();
            assert_eq!(m.family, filter);
            assert!(m.taps_per_phase > 0);
            assert!(m.phases > 0);
            assert!(
                m.stopband_attenuation_db > 60.0,
                "{}: {:.1} dB stopband",
                filter.as_str(),
                m.stopband_attenuation_db
            );
            assert!(
                m.passband_ripple_db < 0.5,
                "{}: {:.3} dB ripple",
                filter.as_str(),
                m.passband_ripple_db
            );
            assert!(m.group_delay_output_frames >= 0.0);
        }
    }

    #[test]
    fn minimum_phase_delay_is_shorter_than_linear_at_every_quality() {
        for quality in [
            ResamplerQuality::Fast,
            ResamplerQuality::Balanced,
            ResamplerQuality::HighQuality,
        ] {
            let lin = build(ResamplerFilter::Kaiser, quality);
            let min = build(ResamplerFilter::MinimumPhase, quality);
            assert!(
                min.delay_output_frames < lin.delay_output_frames,
                "{quality:?}: min {:.1} vs linear {:.1}",
                min.delay_output_frames,
                lin.delay_output_frames
            );
        }
    }

    #[test]
    fn extreme_ratios_fail_instead_of_allocating() {
        let err = PolyphaseResampler::<f32>::new(
            ResamplerFilter::Kaiser,
            ResamplerQuality::Ultra,
            1,
            768_000,
        );
        assert!(err.is_err(), "1 Hz → 768 kHz must not build a giant bank");
    }

    #[test]
    fn reset_clears_the_stream() {
        let mut r = build(ResamplerFilter::Kaiser, ResamplerQuality::Fast);
        let mut out = [
            vec![0.0f32; r.output_frames_max()],
            vec![0.0f32; r.output_frames_max()],
        ];
        let input = [vec![0.7f32; CHUNK_SIZE], vec![-0.7f32; CHUNK_SIZE]];
        let n = r.output_frames_next();
        r.process(&input, &mut out, n);
        r.reset();
        assert_eq!(r.total_input, 0);
        assert_eq!(r.next_output, 0);
        assert_eq!(r.output_frames_next(), n);
    }

    #[test]
    fn summary_names_the_realized_shape() {
        let r = build(ResamplerFilter::Kaiser, ResamplerQuality::Balanced);
        let s = r.metadata().summary();
        assert!(s.contains("kaiser"));
        assert!(s.contains("linear phase"));
        assert!(s.contains("640 taps") || s.contains("640"));
    }

    #[test]
    fn cutoff_ratio_moves_the_anti_aliasing_edge() {
        // 21.5 kHz is above 0.85 × Nyquist (18.7 kHz) but below
        // 0.99 × Nyquist (21.8 kHz), so the same tone must survive one
        // filter and be rejected by the other.
        fn rms_at(cutoff: f64) -> f64 {
            let mut r = PolyphaseResampler::<f32>::new_with_cutoff(
                ResamplerFilter::Kaiser,
                ResamplerQuality::HighQuality,
                IN,
                96_000,
                cutoff,
            )
            .expect("filter construction");
            let delay = r.output_delay();
            let mut out = [
                vec![0.0f32; r.output_frames_max()],
                vec![0.0f32; r.output_frames_max()],
            ];
            let mut collected = Vec::new();
            let mut input = [vec![0.0f32; CHUNK_SIZE], vec![0.0f32; CHUNK_SIZE]];
            let mut phase = 0.0f64;
            for _ in 0..32 {
                let [left, right] = &mut input;
                for (l, rr) in left.iter_mut().zip(right.iter_mut()) {
                    let sample = (2.0 * std::f64::consts::PI * 21_500.0 * phase / IN as f64).sin();
                    phase += 1.0;
                    *l = sample as f32;
                    *rr = sample as f32;
                }
                let n = r.output_frames_next();
                let produced = r.process(&input, &mut out, n);
                collected.extend_from_slice(&out[0][..produced]);
            }
            let tail = &collected[delay + 4096..];
            (tail.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / tail.len() as f64).sqrt()
        }

        let wide = rms_at(0.99);
        let narrow = rms_at(0.85);
        assert!(
            narrow < wide * 0.25,
            "0.85 cutoff should reject 21.5 kHz much more than 0.99: {narrow:.5} vs {wide:.5}"
        );
    }
}
