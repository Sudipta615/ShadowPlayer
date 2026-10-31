//! Shared absolute-peak scan kernel (§8.2, Item 29).
//!
//! One job: the maximum `|x|` over a block of samples, `f32` or `f64`. The
//! mix-bus meters (`graph2::prod::arena::nodes::mix`) are its only callers —
//! they need a per-plane peak without carrying their own scalar loop. The
//! module is named for the *operation*, not for a consumer, because the
//! consumer can change and the operation cannot.
//!
//! # This is not limiter DSP
//!
//! `crate::dsp::limiter` does not call these kernels, and must not: the
//! lookahead limiter needs a peak for *every sample of every channel* (the
//! same value feeds the sliding-window maximum, the stereo-link deque, and the
//! true-peak FIR detector), so a block-wide scan would have to be re-run once
//! per sample to produce it. Its peaks are computed inline in
//! `LookaheadLimiter::process_sample_multichannel`. A block scan is the right
//! shape for metering and the wrong shape for a per-sample envelope; the
//! limiter's own vector code is the polyphase true-peak filter, not this.
//!
//! # Qualification
//!
//! `super::scalar::vector_abs_max` and `super::scalar::vector_abs_max_f64` are
//! the portable reference the vector tiers must agree with. The tests in this
//! file call that reference directly instead of re-writing it, so the
//! reference and the check cannot drift apart; they pin the two properties a
//! new tier can silently break: NaN handling and the scalar tail of a short
//! slice.

/// Find maximum absolute sample value in `src` over `n` samples.
///
/// **NaN inputs are ignored**, matching the scalar `if a.abs() > peak { peak =
/// a.abs() }` this replaced and matching Rust's `f32::max`. Every tier agrees
/// on this: SSE2's `maxps` returns its second operand when that operand is
/// NaN and the accumulator is passed first; NEON needs `FMAXNM` explicitly,
/// because `FMAX` propagates NaN and would make the reading architecture-
/// dependent. The f64 twin is a scalar loop and was never affected.
#[inline]
pub fn vector_abs_max(src: &[f32], n: usize) -> f32 {
    let n = n.min(src.len());
    if n == 0 {
        return 0.0;
    }
    let mut i = 0usize;
    let mut max_val = 0.0f32;

    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            use core::arch::x86_64::{
                _mm256_and_ps, _mm256_castps256_ps128, _mm256_extractf128_ps, _mm256_loadu_ps,
                _mm256_max_ps, _mm256_set1_ps, _mm_max_ps, _mm_shuffle_ps, _mm_store_ss,
            };
            let mask = unsafe { _mm256_set1_ps(f32::from_bits(0x7FFF_FFFF)) };
            let mut vmax256 = unsafe { _mm256_set1_ps(0.0) };

            while i + 8 <= n {
                unsafe {
                    let v = _mm256_loadu_ps(src.as_ptr().add(i));
                    let vabs = _mm256_and_ps(v, mask);
                    vmax256 = _mm256_max_ps(vabs, vmax256);
                }
                i += 8;
            }

            unsafe {
                let low128 = _mm256_castps256_ps128(vmax256);
                let high128 = _mm256_extractf128_ps(vmax256, 1);
                let v128 = _mm_max_ps(low128, high128);
                let shuf1 = _mm_shuffle_ps(v128, v128, 0b01_00_11_10);
                let m1 = _mm_max_ps(v128, shuf1);
                let shuf2 = _mm_shuffle_ps(m1, m1, 0b00_00_00_01);
                let m2 = _mm_max_ps(m1, shuf2);
                let mut res = 0.0f32;
                _mm_store_ss(&mut res, m2);
                max_val = max_val.max(res);
            }
        } else if is_x86_feature_detected!("sse2") {
            use core::arch::x86_64::{
                _mm_and_ps, _mm_loadu_ps, _mm_max_ps, _mm_set1_ps, _mm_shuffle_ps, _mm_store_ss,
            };
            let mask = unsafe { _mm_set1_ps(f32::from_bits(0x7FFF_FFFF)) };
            let mut vmax = unsafe { _mm_set1_ps(0.0) };

            while i + 4 <= n {
                unsafe {
                    let v = _mm_loadu_ps(src.as_ptr().add(i));
                    let vabs = _mm_and_ps(v, mask);
                    vmax = _mm_max_ps(vabs, vmax);
                }
                i += 4;
            }

            unsafe {
                let shuf1 = _mm_shuffle_ps(vmax, vmax, 0b01_00_11_10);
                let m1 = _mm_max_ps(vmax, shuf1);
                let shuf2 = _mm_shuffle_ps(m1, m1, 0b00_00_00_01);
                let m2 = _mm_max_ps(m1, shuf2);
                let mut res = 0.0f32;
                _mm_store_ss(&mut res, m2);
                max_val = max_val.max(res);
            }
        }
    }

    #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
    {
        use core::arch::aarch64::{vabsq_f32, vld1q_f32, vmaxnmq_f32, vmaxvq_f32};
        let mut vmax = unsafe { vld1q_f32([0.0, 0.0, 0.0, 0.0].as_ptr()) };

        while i + 4 <= n {
            unsafe {
                let v = vld1q_f32(src.as_ptr().add(i));
                let vabs = vabsq_f32(v);
                vmax = vmaxnmq_f32(vmax, vabs);
            }
            i += 4;
        }

        unsafe {
            let res = vmaxvq_f32(vmax);
            max_val = max_val.max(res);
        }
    }

    while i < n {
        max_val = max_val.max(src[i].abs());
        i += 1;
    }

    max_val
}

/// f64 twin of [`vector_abs_max`].
#[inline]
pub fn vector_abs_max_f64(src: &[f64], n: usize) -> f64 {
    let n = n.min(src.len());
    if n == 0 {
        return 0.0;
    }
    let mut i = 0usize;
    let mut max_val = 0.0f64;

    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            use core::arch::x86_64::{
                _mm256_and_pd, _mm256_castpd256_pd128, _mm256_extractf128_pd, _mm256_loadu_pd,
                _mm256_max_pd, _mm256_set1_pd, _mm_cvtsd_f64, _mm_max_pd, _mm_shuffle_pd,
            };
            let mask = unsafe { _mm256_set1_pd(f64::from_bits(0x7FFF_FFFF_FFFF_FFFF)) };
            let mut vmax256 = unsafe { _mm256_set1_pd(0.0) };

            while i + 4 <= n {
                unsafe {
                    let v = _mm256_loadu_pd(src.as_ptr().add(i));
                    let vabs = _mm256_and_pd(v, mask);
                    vmax256 = _mm256_max_pd(vabs, vmax256);
                }
                i += 4;
            }

            unsafe {
                let low128 = _mm256_castpd256_pd128(vmax256);
                let high128 = _mm256_extractf128_pd(vmax256, 1);
                let v128 = _mm_max_pd(low128, high128);
                let shuf = _mm_shuffle_pd(v128, v128, 0b01);
                let m = _mm_max_pd(v128, shuf);
                max_val = max_val.max(_mm_cvtsd_f64(m));
            }
        } else if is_x86_feature_detected!("sse2") {
            use core::arch::x86_64::{
                _mm_and_pd, _mm_cvtsd_f64, _mm_loadu_pd, _mm_max_pd, _mm_set1_pd, _mm_shuffle_pd,
            };
            let mask = unsafe { _mm_set1_pd(f64::from_bits(0x7FFF_FFFF_FFFF_FFFF)) };
            let mut vmax = unsafe { _mm_set1_pd(0.0) };

            while i + 2 <= n {
                unsafe {
                    let v = _mm_loadu_pd(src.as_ptr().add(i));
                    let vabs = _mm_and_pd(v, mask);
                    vmax = _mm_max_pd(vabs, vmax);
                }
                i += 2;
            }

            unsafe {
                let shuf = _mm_shuffle_pd(vmax, vmax, 0b01);
                let m = _mm_max_pd(vmax, shuf);
                max_val = max_val.max(_mm_cvtsd_f64(m));
            }
        }
    }

    #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
    {
        use core::arch::aarch64::{vabsq_f64, vgetq_lane_f64, vld1q_f64, vmaxnmq_f64};
        let mut vmax = unsafe { vld1q_f64([0.0, 0.0].as_ptr()) };

        while i + 2 <= n {
            unsafe {
                let v = vld1q_f64(src.as_ptr().add(i));
                let vabs = vabsq_f64(v);
                vmax = vmaxnmq_f64(vmax, vabs);
            }
            i += 2;
        }

        unsafe {
            let m0 = vgetq_lane_f64(vmax, 0);
            let m1 = vgetq_lane_f64(vmax, 1);
            max_val = max_val.max(m0.max(m1));
        }
    }

    while i < n {
        max_val = max_val.max(src[i].abs());
        i += 1;
    }

    max_val
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abs_max_finds_peak() {
        let data = vec![0.1f32, -0.9, 0.4, -0.2, 0.85, -0.3, 0.0];
        let peak = vector_abs_max(&data, data.len());
        assert!((peak - 0.9).abs() < 1e-6);
    }
}

#[cfg(test)]
mod nan_tests {
    use super::*;
    // The portable scalar reference is the kernel module's own, not a copy
    // kept beside the test: if the reference changes, this test changes with
    // it, and the two can never be asserting different contracts. The aliases
    // keep the assertions reading as "SIMD vs scalar".
    use crate::dsp::simd::scalar::{
        vector_abs_max as scalar_abs_max_f32, vector_abs_max_f64 as scalar_abs_max_f64,
    };

    /// SIMD and scalar must agree — including on NaN.
    ///
    /// This is the test that was missing when the mix-bus meters were moved
    /// onto `vector_abs_max`. The tiers disagreed: SSE2's `maxps` returns its
    /// second operand when that operand is NaN, so an accumulator passed first
    /// ignores NaN, while NEON's `FMAX` propagates it. The same audio therefore
    /// reported a number on x86_64 and `NaN` on aarch64.
    /// `tests/fidelity/simd_qualification.rs` covers gain and levels but had no
    /// NaN case, so nothing pinned this.
    ///
    /// It runs on whatever host builds it, which is the point: the NEON path is
    /// only observable on aarch64 and the SSE2 path only on x86_64, so the test
    /// asserts the shared contract rather than one tier's behaviour.
    #[test]
    fn simd_peak_scan_matches_the_scalar_scan_including_nan() {
        let plain = [0.1f32, -0.9, 0.4, -0.2, 0.75, -0.05];
        assert_eq!(
            vector_abs_max(&plain, plain.len()),
            scalar_abs_max_f32(&plain, plain.len()),
            "ordinary input must agree"
        );

        // NaN first, last, and between two real peaks — the three positions
        // that could plausibly behave differently.
        for (name, input) in [
            ("leading", [f32::NAN, 0.5, -0.8, 0.2]),
            ("trailing", [0.5, -0.8, 0.2, f32::NAN]),
            ("interior", [0.5, f32::NAN, -0.8, 0.2]),
        ] {
            assert_eq!(
                vector_abs_max(&input, input.len()),
                scalar_abs_max_f32(&input, input.len()),
                "NaN {name}: the scan must ignore it, not propagate it"
            );
        }

        // Infinities are values, not noise: the largest magnitude must win.
        let with_inf = [0.5f32, f32::INFINITY, 0.2];
        assert_eq!(
            vector_abs_max(&with_inf, with_inf.len()),
            f32::INFINITY,
            "+Inf is a real peak"
        );
    }

    /// A short slice never enters the vector loop; the tail must not disagree
    /// with the body about NaN either.
    #[test]
    fn the_scalar_tail_ignores_nan() {
        let short = [f32::NAN, 0.25, -0.5];
        assert_eq!(
            vector_abs_max(&short, short.len()),
            scalar_abs_max_f32(&short, short.len()),
            "a short slice must agree too"
        );
    }

    /// The f64 twin is scalar today, but pin the contract anyway so switching
    /// it to a vector path cannot silently change NaN behaviour.
    #[test]
    fn the_f64_twin_also_ignores_nan() {
        let input = [0.5f64, f64::NAN, -0.8, 0.2];
        assert_eq!(
            vector_abs_max_f64(&input, input.len()),
            scalar_abs_max_f64(&input, input.len()),
            "f64 must agree with the scalar reference"
        );
    }
}
