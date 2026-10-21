//! Headphone crossfeed: every profile must change the signal, the profiles
//! must differ from each other, and a mono programme must stay mono.
//!
//! The engine ships four crossfeed profiles — Bauer, Chu Moy, Jan Meier and
//! a user-defined one — each of which is a different low-pass / delay pair
//! for the signal crossing from one ear to the other. This suite checks the
//! three properties a user can hear:
//!
//! * the crossfeed does something (the opposite channel gets content it did
//!   not have);
//! * the profiles are genuinely different designs, not four names for one
//!   filter; and
//! * a mono programme stays mono, so a mono source is not turned into a
//!   phantom image by the crossfeed.

use config::CrossfeedProfile;
use engine::dsp::crossfeed::Crossfeed;

/// Sample rate for the measurements.
const SR: f32 = 48_000.0;

/// The profiles the engine offers, with the delay each one documents.
///
/// The custom profile's delay is the one this suite configures it with
/// (0.5 ms), not a fixed value: it is the user's parameter.
const PROFILES: &[(CrossfeedProfile, f32)] = &[
    (CrossfeedProfile::Bauer, 0.3),
    (CrossfeedProfile::ChuMoy, 0.25),
    (CrossfeedProfile::Jmeier, 0.35),
    (CrossfeedProfile::Custom, 0.5),
];

/// A crossfeed stage set to `profile`, with the custom profile given
/// non-default parameters so it is not a copy of one of the named ones.
fn crossfeed_for(profile: CrossfeedProfile) -> Crossfeed {
    let mut crossfeed = Crossfeed::new(SR);
    // The custom profile's parameters only matter when it is the active
    // profile, so they are set before the profile is selected.
    crossfeed.set_custom_params(500.0, 0.9, 0.5);
    crossfeed.set_profile(profile);
    crossfeed.set_enabled(true);
    crossfeed
}

/// A hard-left programme: silence in the right channel.
fn hard_left(freq_hz: f32, frames: usize) -> Vec<(f32, f32)> {
    (0..frames)
        .map(|i| {
            let s = (2.0 * std::f32::consts::PI * freq_hz * i as f32 / SR).sin() * 0.5;
            (s, 0.0)
        })
        .collect()
}

/// The amplitude of `samples` at `freq_hz`, measured with a Goertzel filter.
fn amplitude_at(samples: &[f32], freq_hz: f32) -> f32 {
    let n = samples.len() as f32;
    let k = 2.0 * std::f32::consts::PI * freq_hz / SR;
    let coeff = 2.0 * k.cos();
    let mut s1 = 0.0f32;
    let mut s2 = 0.0f32;
    for &x in samples {
        let s0 = x + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    let real = s1 - s2 * k.cos();
    let imag = s2 * k.sin();
    2.0 * (real * real + imag * imag).sqrt() / n
}

#[test]
fn every_profile_the_engine_offers_changes_the_signal() {
    let frames = 8192;
    let input = hard_left(1_000.0, frames);

    for (profile, _delay_ms) in PROFILES {
        let mut crossfeed = crossfeed_for(*profile);
        let mut output = Vec::with_capacity(frames);
        for &(l, r) in &input {
            let (ol, or_) = crossfeed.process(l, r);
            output.push((ol, or_));
        }

        // The right channel was silent and must now carry the crossfed
        // signal; the left channel must still carry the direct signal.
        let right_energy: f32 = output.iter().map(|&(_, r)| r * r).sum();
        assert!(
            right_energy > 1e-6,
            "the {profile:?} profile must put signal into the silent channel, \
             it produced {right_energy:e}"
        );
        let left_energy: f32 = output.iter().map(|&(l, _)| l * l).sum();
        assert!(
            left_energy > right_energy,
            "the {profile:?} profile must keep the direct channel dominant \
             (left {left_energy:e}, right {right_energy:e})"
        );
    }
}

#[test]
fn the_profiles_are_different_designs() {
    // Four names for one filter would be a lie in the settings UI.
    let frames = 8192;
    let input = hard_left(1_000.0, frames);

    let mut outputs: Vec<(CrossfeedProfile, Vec<(f32, f32)>)> = Vec::new();
    for (profile, _delay_ms) in PROFILES {
        let mut crossfeed = crossfeed_for(*profile);
        let mut output = Vec::with_capacity(frames);
        for &(l, r) in &input {
            let (ol, or_) = crossfeed.process(l, r);
            output.push((ol, or_));
        }
        outputs.push((*profile, output));
    }

    for i in 0..outputs.len() {
        for j in (i + 1)..outputs.len() {
            let (pa, a) = &outputs[i];
            let (pb, b) = &outputs[j];
            let identical = a.iter().zip(b.iter()).all(|(&(al, ar), &(bl, br))| {
                al.to_bits() == bl.to_bits() && ar.to_bits() == br.to_bits()
            });
            assert!(
                !identical,
                "the {pa:?} and {pb:?} profiles produce identical output"
            );
        }
    }
}

#[test]
fn a_mono_programme_stays_mono() {
    // The crossfeed's two delay lines are the same length and are fed the
    // same signal for a mono input, so the two ears must stay identical: a
    // mono source must not be turned into a phantom image.
    let frames = 8192;
    for (profile, _delay_ms) in PROFILES {
        let mut crossfeed = crossfeed_for(*profile);
        let mut mismatches = 0usize;
        let mut max_delta = 0.0f32;
        for i in 0..frames {
            let s = (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / SR).sin() * 0.5;
            let (l, r) = crossfeed.process(s, s);
            let delta = (l - r).abs();
            if delta != 0.0 {
                mismatches += 1;
            }
            max_delta = max_delta.max(delta);
        }
        assert_eq!(
            mismatches, 0,
            "the {profile:?} profile broke the mono symmetry of its input \
             (worst delta {max_delta:e})"
        );
    }
}

#[test]
fn a_disabled_crossfeed_is_a_bit_exact_passthrough() {
    let frames = 1024;
    let mut crossfeed = Crossfeed::new(SR);
    assert!(!crossfeed.is_enabled());
    for i in 0..frames {
        let s = (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / SR).sin() * 0.5;
        let (l, r) = crossfeed.process(s, -s);
        assert_eq!(l.to_bits(), s.to_bits(), "sample {i} changed");
        assert_eq!(r.to_bits(), (-s).to_bits(), "sample {i} changed");
    }
}

#[test]
fn the_crossfeed_reports_the_delay_its_profile_documents() {
    for (profile, delay_ms) in PROFILES {
        let crossfeed = crossfeed_for(*profile);
        // The delay line is sized in whole samples, so the reported latency is
        // the documented delay truncated to a sample — one sample period
        // (0.021 ms at 48 kHz) is the whole of the discrepancy.
        let one_sample_ms = 1000.0 / SR;
        let reported = crossfeed.latency_ms();
        assert!(
            (reported - delay_ms).abs() <= one_sample_ms,
            "the {profile:?} profile reports {reported:.4} ms of latency, \
             its documented delay is {delay_ms} ms"
        );
        let frames = (reported / 1000.0 * SR).round();
        assert!(
            (frames * 1000.0 / SR - delay_ms).abs() <= one_sample_ms,
            "the {profile:?} profile's delay line must be a whole number of \
             samples"
        );
    }
}

#[test]
fn the_crossfed_signal_is_low_passed() {
    // The point of the crossfeed is a *low-passed* copy of the opposite
    // channel: the high frequencies are the ones that cause the headphone
    // "hole", so they must not be copied across.
    let frames = 16_384;
    for (profile, _delay_ms) in PROFILES {
        let mut low = crossfeed_for(*profile);
        let mut high = crossfeed_for(*profile);
        let mut low_right = Vec::with_capacity(frames);
        let mut high_right = Vec::with_capacity(frames);
        for i in 0..frames {
            let l = (2.0 * std::f32::consts::PI * 100.0 * i as f32 / SR).sin() * 0.5;
            let lh = (2.0 * std::f32::consts::PI * 10_000.0 * i as f32 / SR).sin() * 0.5;
            low_right.push(low.process(l, 0.0).1);
            high_right.push(high.process(lh, 0.0).1);
        }
        let low_amp = amplitude_at(&low_right, 100.0);
        let high_amp = amplitude_at(&high_right, 10_000.0);
        assert!(
            low_amp > 0.0,
            "the {profile:?} profile must copy a low tone across"
        );
        let drop_db = 20.0 * (low_amp / high_amp).log10();
        assert!(
            drop_db > 20.0,
            "the {profile:?} profile must low-pass the crossfed signal, a 10 kHz \
             tone came through only {drop_db:.1} dB below a 100 Hz tone"
        );
    }
}

#[test]
fn the_custom_profile_uses_its_own_parameters() {
    // The custom profile is not a fifth name for Bauer: its three
    // parameters must reach the filter.
    let mut custom = Crossfeed::new(SR);
    custom.set_custom_params(500.0, 0.9, 0.5);
    custom.set_profile(CrossfeedProfile::Custom);
    custom.set_enabled(true);
    let (freq, q, delay_ms) = custom.custom_params();
    assert_eq!(freq, 500.0);
    assert_eq!(q, 0.9);
    assert_eq!(delay_ms, 0.5);
    assert!(
        (custom.latency_ms() - 0.5).abs() < 0.01,
        "the custom profile's delay must reach the delay line, it reports \
         {:.4} ms",
        custom.latency_ms()
    );
    assert_eq!(custom.profile(), CrossfeedProfile::Custom);
}
