//! Sample-rate selection policy and rate fallback helpers.

use serde::{Deserialize, Serialize};

use super::enums::RateFallbackPolicy;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SampleRatePolicy {
    DeviceDefault,
    #[default]
    FollowTrack,
    Fixed(u32),
    BestSupported,
    BaseRateSyncExactFirst,
    BaseRateSyncHighest,
    /// Alias for `BaseRateSyncExactFirst` (canonical default behavior).
    BaseRateSync,
}

const FAMILY_44: &[u32] = &[44_100, 88_200, 176_400, 352_800, 705_600];
const FAMILY_48: &[u32] = &[48_000, 96_000, 192_000, 384_000, 768_000];

impl SampleRatePolicy {
    pub fn select_rate(&self, source_rate: u32, supported: &[u32]) -> u32 {
        self.select_rate_with_fallback(source_rate, supported, RateFallbackPolicy::Nearest)
    }

    pub fn select_rate_with_fallback(
        &self,
        source_rate: u32,
        supported: &[u32],
        fallback: RateFallbackPolicy,
    ) -> u32 {
        self.select_rate_with_ranges(source_rate, supported, &[], fallback)
    }

    /// Select a rate filtering through an optional allowed rates whitelist.
    pub fn select_rate_with_allowed(
        &self,
        source_rate: u32,
        supported: &[u32],
        ranges: &[(u32, u32)],
        allowed: &[u32],
        fallback: RateFallbackPolicy,
    ) -> u32 {
        if allowed.is_empty() {
            return self.select_rate_with_ranges(source_rate, supported, ranges, fallback);
        }

        // A whitelist names concrete rates. Keep only entries the hardware
        // actually supports, either discretely or inside a reported range.
        // Passing the ranges through would let an in-range rate that is *not*
        // whitelisted win, which is exactly what the whitelist forbids.
        let mut candidates: Vec<u32> = allowed
            .iter()
            .copied()
            .filter(|&rate| {
                supported.contains(&rate)
                    || ranges.iter().any(|&(min, max)| rate >= min && rate <= max)
            })
            .collect();
        candidates.sort_unstable();
        candidates.dedup();

        if candidates.is_empty() {
            // No allowed rate is supported by this device. Return the closest
            // allowed rate rather than silently selecting an unlisted one; the
            // output layer's negotiation error is the visible failure.
            let mut sorted = allowed.to_vec();
            sorted.sort_unstable();
            sorted.dedup();
            return apply_fallback(source_rate, &sorted, fallback);
        }

        self.select_rate_with_ranges(source_rate, &candidates, &[], fallback)
    }

    /// Select a rate using both discrete rates and continuous driver ranges.
    ///
    /// CPAL commonly reports a range such as 44.1–192 kHz rather than one
    /// entry per rate. A source or fixed target inside such a range is valid
    /// even when it is absent from `supported`; range endpoints are also
    /// considered for fallback policies and maximum-rate selection.
    pub fn select_rate_with_ranges(
        &self,
        source_rate: u32,
        supported: &[u32],
        ranges: &[(u32, u32)],
        fallback: RateFallbackPolicy,
    ) -> u32 {
        if ranges.is_empty() {
            return self.select_rate_discrete(source_rate, supported, fallback);
        }

        let candidates = range_candidates(supported, ranges);
        let supports = |rate: u32| {
            supported.contains(&rate) || ranges.iter().any(|&(min, max)| rate >= min && rate <= max)
        };

        match self {
            Self::DeviceDefault => supported
                .first()
                .copied()
                .or_else(|| candidates.first().copied())
                .unwrap_or(source_rate),
            Self::FollowTrack => {
                if supports(source_rate) {
                    source_rate
                } else {
                    apply_fallback(source_rate, &candidates, fallback)
                }
            }
            Self::Fixed(rate) => {
                if supports(*rate) {
                    *rate
                } else {
                    apply_fallback(*rate, &candidates, fallback)
                }
            }
            Self::BestSupported => candidates.last().copied().unwrap_or(source_rate),
            Self::BaseRateSyncExactFirst | Self::BaseRateSync => {
                if supports(source_rate) {
                    return source_rate;
                }
                let family = clock_family(source_rate);
                let same_family: Vec<u32> = candidates
                    .iter()
                    .copied()
                    .filter(|rate| family.contains(rate))
                    .collect();
                if let Some(&lowest_higher) = same_family.iter().find(|&&r| r >= source_rate) {
                    lowest_higher
                } else if let Some(&highest_lower) = same_family.last() {
                    highest_lower
                } else {
                    apply_fallback(source_rate, &candidates, fallback)
                }
            }
            Self::BaseRateSyncHighest => {
                let family = clock_family(source_rate);
                if let Some(&best) = candidates.iter().rev().find(|rate| family.contains(rate)) {
                    best
                } else {
                    Self::FollowTrack.select_rate_with_ranges(
                        source_rate,
                        supported,
                        ranges,
                        fallback,
                    )
                }
            }
        }
    }

    fn select_rate_discrete(
        &self,
        source_rate: u32,
        supported: &[u32],
        fallback: RateFallbackPolicy,
    ) -> u32 {
        if supported.is_empty() {
            return source_rate;
        }

        match self {
            Self::DeviceDefault => supported.first().copied().unwrap_or(source_rate),
            Self::FollowTrack => {
                if supported.contains(&source_rate) {
                    source_rate
                } else {
                    apply_fallback(source_rate, supported, fallback)
                }
            }
            Self::Fixed(rate) => {
                if supported.contains(rate) {
                    *rate
                } else {
                    apply_fallback(*rate, supported, fallback)
                }
            }
            Self::BestSupported => supported.last().copied().unwrap_or(source_rate),
            Self::BaseRateSyncExactFirst | Self::BaseRateSync => {
                if supported.contains(&source_rate) {
                    return source_rate;
                }
                let family = clock_family(source_rate);
                let same_family: Vec<u32> = family
                    .iter()
                    .filter(|&&r| supported.contains(&r))
                    .copied()
                    .collect();
                if let Some(&lowest_higher) = same_family.iter().find(|&&r| r >= source_rate) {
                    lowest_higher
                } else if let Some(&highest_lower) = same_family.last() {
                    highest_lower
                } else {
                    apply_fallback(source_rate, supported, fallback)
                }
            }
            Self::BaseRateSyncHighest => {
                let family = clock_family(source_rate);
                let family_rates: Vec<u32> = family
                    .iter()
                    .filter(|&&r| supported.contains(&r))
                    .copied()
                    .collect();
                if let Some(&best) = family_rates.last() {
                    best
                } else {
                    Self::FollowTrack.select_rate_discrete(source_rate, supported, fallback)
                }
            }
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            Self::DeviceDefault => "Device Default".to_string(),
            Self::FollowTrack => "Follow Track".to_string(),
            Self::Fixed(r) => format!("Fixed {}Hz", r),
            Self::BestSupported => "Best Supported".to_string(),
            Self::BaseRateSyncExactFirst | Self::BaseRateSync => {
                "Base-Rate Sync (Exact First)".to_string()
            }
            Self::BaseRateSyncHighest => "Base-Rate Sync (Highest)".to_string(),
        }
    }
}

fn range_candidates(supported: &[u32], ranges: &[(u32, u32)]) -> Vec<u32> {
    let mut candidates = supported.to_vec();
    for &(min, max) in ranges {
        if min <= max {
            candidates.push(min);
            candidates.push(max);
        }
    }
    candidates.sort_unstable();
    candidates.dedup();
    candidates
}

pub fn clock_family(rate: u32) -> &'static [u32] {
    let base = base_rate(rate);
    if base == 44_100 {
        FAMILY_44
    } else {
        FAMILY_48
    }
}

pub fn base_rate(rate: u32) -> u32 {
    let mut r = rate;
    loop {
        if r <= 48_000 {
            break;
        }
        let half = r / 2;
        if half * 2 != r {
            break;
        }
        r = half;
    }
    if r == 44_100 || r == 22_050 || r == 11_025 {
        44_100
    } else {
        48_000
    }
}

pub fn apply_fallback(target: u32, supported: &[u32], fallback: RateFallbackPolicy) -> u32 {
    match fallback {
        RateFallbackPolicy::Nearest => nearest_rate(target, supported),
        RateFallbackPolicy::PreferHigher => {
            if let Some(&higher) = supported.iter().find(|&&r| r >= target) {
                higher
            } else {
                supported.last().copied().unwrap_or(target)
            }
        }
        RateFallbackPolicy::PreferLower => {
            if let Some(&lower) = supported.iter().rev().find(|&&r| r <= target) {
                lower
            } else {
                supported.first().copied().unwrap_or(target)
            }
        }
        RateFallbackPolicy::SameFamilyFirst => {
            let family = clock_family(target);
            let same_family: Vec<u32> = family
                .iter()
                .filter(|&&r| supported.contains(&r))
                .copied()
                .collect();
            if !same_family.is_empty() {
                nearest_rate(target, &same_family)
            } else {
                nearest_rate(target, supported)
            }
        }
    }
}

pub fn nearest_rate(target: u32, supported: &[u32]) -> u32 {
    supported
        .iter()
        .copied()
        .min_by_key(|&r| (r as i64 - target as i64).unsigned_abs())
        .unwrap_or(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whitelist_only_returns_rates_on_the_whitelist() {
        let supported = [44_100, 48_000, 88_200, 96_000, 192_000];
        let allowed = [48_000, 96_000];

        // FollowTrack asks for 192 kHz, which the device supports but the
        // user excluded: the nearest allowed rate wins.
        assert_eq!(
            SampleRatePolicy::FollowTrack.select_rate_with_allowed(
                192_000,
                &supported,
                &[],
                &allowed,
                RateFallbackPolicy::Nearest,
            ),
            96_000
        );
        assert_eq!(
            SampleRatePolicy::BestSupported.select_rate_with_allowed(
                44_100,
                &supported,
                &[],
                &allowed,
                RateFallbackPolicy::Nearest,
            ),
            96_000
        );
        assert_eq!(
            SampleRatePolicy::Fixed(44_100).select_rate_with_allowed(
                44_100,
                &supported,
                &[],
                &allowed,
                RateFallbackPolicy::Nearest,
            ),
            48_000,
            "a fixed rate outside the whitelist must not leak through"
        );
    }

    #[test]
    fn whitelist_intersects_discrete_rates_and_hardware_ranges() {
        let supported = [44_100, 48_000];
        let ranges = [(48_000, 192_000)];
        let allowed = [50_000, 96_000, 176_400];

        assert_eq!(
            SampleRatePolicy::FollowTrack.select_rate_with_allowed(
                50_000,
                &supported,
                &ranges,
                &allowed,
                RateFallbackPolicy::Nearest,
            ),
            50_000,
            "an in-range whitelisted rate must be accepted"
        );
    }

    #[test]
    fn an_entirely_unsupported_whitelist_returns_the_closest_allowed_rate() {
        let allowed = [88_200, 96_000];
        let selected = SampleRatePolicy::FollowTrack.select_rate_with_allowed(
            44_100,
            &[44_100, 48_000],
            &[],
            &allowed,
            RateFallbackPolicy::Nearest,
        );
        assert!(
            allowed.contains(&selected),
            "the fallback must stay inside the whitelist, got {selected}"
        );
        assert_eq!(selected, 88_200);
    }
}
