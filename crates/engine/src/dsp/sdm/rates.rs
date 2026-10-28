//! Rate planning for PCM→SDM output.
//!
//! The engine does not run its PCM DSP chain at the DSD bit rate. It runs at
//! a **working rate** of eight times the source clock family
//! (352.8 kHz for 44.1 kHz material, 384 kHz for 48 kHz material), exactly
//! like a PCM-to-SDM converter that oversamples in stages. The existing
//! resampler brings the source to the working rate; the SDM stage then
//! interpolates by the small integer `stage_factor` (8/16/32/64) and
//! modulates. This keeps the DSP cost bounded and lets the well-tested
//! resampler handle the hard part of the band-limited conversion.
//!
//! A plan is only produced when the output can actually accept the rate:
//! the caller passes the device's advertised rates (empty means "unknown")
//! and every candidate is still verified by `set_native_dsd` before use.

use config::SdmRate;

/// The working rate is `family × WORKING_MULTIPLE`.
pub(crate) const WORKING_MULTIPLE: u32 = 8;

/// A concrete PCM→SDM rate plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SdmRatePlan {
    /// Source/PCM rate (before the resampler).
    pub pcm_rate: u32,
    /// Rate the PCM DSP chain and the SDM stage's input run at.
    pub working_rate: u32,
    /// Native DSD bit rate.
    pub dsd_rate: u32,
    /// DSD multiple of the clock family (64/128/256/512).
    pub multiple: u32,
    /// Interpolation factor from the working rate to the DSD rate.
    pub stage_factor: u32,
    /// Clock family (44_100 or 48_000).
    pub family: u32,
}

impl SdmRatePlan {
    /// Group delay of the half-band cascade, in milliseconds.
    ///
    /// `odd_taps` is the number of non-zero taps in one half-band stage's odd
    /// phase (8 for the 17-tap design). Each stage delays by `odd_taps / 2`
    /// input samples and the stages run at successively doubled rates, so the
    /// working-rate delay converges to `odd_taps` samples.
    pub fn interpolator_delay_ms(&self, odd_taps: usize) -> f64 {
        let stages = self.stage_factor.trailing_zeros() as i32;
        let samples = odd_taps as f64 * (1.0 - 0.5f64.powi(stages));
        samples / self.working_rate as f64 * 1000.0
    }

    /// `DSD64`-style label for the negotiated multiple.
    pub fn label(&self) -> String {
        format!("DSD{}", self.multiple)
    }
}

/// The clock family a source rate belongs to.
///
/// This decides whether the DSD rate is a multiple of 44.1 kHz or 48 kHz.
/// The two families never share a bit rate, and choosing the matching family
/// keeps the resampler ratio simple and the SDM stage's factor exact.
pub fn clock_family(rate: u32) -> u32 {
    const FORTY_EIGHT_K: [u32; 6] = [6_000, 8_000, 12_000, 16_000, 24_000, 32_000];
    if rate.is_multiple_of(48_000) || FORTY_EIGHT_K.contains(&rate) {
        48_000
    } else {
        44_100
    }
}

/// All plans to try, best first.
///
/// For a fixed [`SdmRate`] there is at most one entry. For `Auto`, the
/// candidates run from the highest multiple down so the caller can settle on
/// the best rate the device actually accepts. When the device advertises no
/// rates, all four are candidates and `set_native_dsd` decides.
pub fn candidates(pcm_rate: u32, requested: SdmRate, supported_rates: &[u32]) -> Vec<SdmRatePlan> {
    if pcm_rate == 0 {
        return Vec::new();
    }
    let family = clock_family(pcm_rate);
    let working_rate = family * WORKING_MULTIPLE;
    let multiples: Vec<u32> = match requested.multiple() {
        Some(m) => vec![m],
        None => vec![512, 256, 128, 64],
    };
    let mut out = Vec::with_capacity(multiples.len());
    for multiple in multiples {
        if multiple % WORKING_MULTIPLE != 0 {
            continue;
        }
        let Some(dsd_rate) = family.checked_mul(multiple) else {
            continue;
        };
        if !supported_rates.is_empty() && !supported_rates.contains(&dsd_rate) {
            continue;
        }
        out.push(SdmRatePlan {
            pcm_rate,
            working_rate,
            dsd_rate,
            multiple,
            stage_factor: multiple / WORKING_MULTIPLE,
            family,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_are_selected_from_the_source_clock() {
        assert_eq!(clock_family(44_100), 44_100);
        assert_eq!(clock_family(88_200), 44_100);
        assert_eq!(clock_family(176_400), 44_100);
        assert_eq!(clock_family(48_000), 48_000);
        assert_eq!(clock_family(96_000), 48_000);
        assert_eq!(clock_family(192_000), 48_000);
        assert_eq!(clock_family(32_000), 48_000);
        assert_eq!(clock_family(22_050), 44_100);
    }

    #[test]
    fn auto_prefers_the_highest_supported_rate() {
        let plans = candidates(44_100, SdmRate::Auto, &[2_822_400, 5_644_800, 11_289_600]);
        assert_eq!(plans.len(), 3);
        assert_eq!(plans[0].multiple, 256);
        assert_eq!(plans[0].dsd_rate, 11_289_600);
        assert_eq!(plans[0].working_rate, 352_800);
        assert_eq!(plans[0].stage_factor, 32);
    }

    #[test]
    fn fixed_rate_is_the_only_candidate() {
        let plans = candidates(48_000, SdmRate::Dsd64, &[]);
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].dsd_rate, 3_072_000);
        assert_eq!(plans[0].working_rate, 384_000);
        assert_eq!(plans[0].stage_factor, 8);
        assert_eq!(plans[0].family, 48_000);
    }

    #[test]
    fn unsupported_fixed_rate_yields_no_plan() {
        let plans = candidates(44_100, SdmRate::Dsd512, &[2_822_400]);
        assert!(plans.is_empty());
    }

    #[test]
    fn stage_factor_is_always_a_power_of_two_multiple_of_eight() {
        for pcm in [44_100, 48_000, 88_200, 96_000, 192_000] {
            for plan in candidates(pcm, SdmRate::Auto, &[]) {
                assert!(plan.stage_factor.is_power_of_two());
                assert!(plan.stage_factor >= 8);
                assert_eq!(
                    plan.working_rate * plan.stage_factor,
                    plan.dsd_rate,
                    "plan {plan:?}"
                );
            }
        }
    }
}
