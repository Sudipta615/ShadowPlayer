//! Dithering for bit-depth reduction
//!
//! When reducing bit depth (e.g. floating-point → 16-bit integer), quantization
//! error introduces harmonic distortion.  Dither decorrelates this error from the
//! signal, replacing distortion with spectrally-flat noise.
//!
//! ## Mode selection
//!
//! | Mode                 | Recommended for                                      |
//! |----------------------|------------------------------------------------------|
//! | `None`               | 32-bit float output (no quantization)                |
//! | `Triangular`         | 16-bit / 24-bit integer output (default)             |
//! | `HighPassTriangular` | 16-bit; pushes dither noise to high frequencies      |
//! | `Shibata`            | 16-bit; minimum perceptual noise (psychoacoustic)    |
//! | `Ns5` / `Ns9`        | High-order high-pass shaping for 16/24-bit           |
//! | `Lns15`              | 15th-order, gently weighted shaping at high rates    |
//! | `Rectangular`        | Debug / measurement only                              |
//! | `NoiseShaped`        | **DEPRECATED** — use `Triangular` instead            |
//!
//! ## Float-output guard
//!
//! When the hardware output format is `f32` or `f64`, no quantization occurs
//! and dither MUST NOT be applied (it would add audible noise for no benefit).
//! Call [`Dither::set_output_is_float`] with `true` in this case to engage
//! the automatic bypass.

// The coefficient tables are copied at full published precision; truncating
// them to the shortest f32 spelling would alter the fitted response and make
// the measured NTF disagree with the design. The extra digits are meaningful
// for the design source even though the runtime type is f32.
#![allow(clippy::excessive_precision)]

/// Dither mode
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DitherType {
    /// No dithering — fastest, but introduces quantization distortion at low levels.
    None,
    /// Rectangular PDF dither: one uniform random sample per channel.
    /// Suitable only for debug/measurement.  Prefer `Triangular` for production.
    Rectangular,
    /// Triangular PDF dither (TPDF): sum of two independent rectangular
    /// sources.  Eliminates all harmonic distortion from quantization.
    /// **Recommended for all production use.**
    Triangular,
    /// High-pass TPDF: difference of two consecutive rectangular samples.
    /// Spectrally-white dither energy is shaped towards Nyquist, keeping
    /// low-frequency noise floor lower than plain TPDF.
    HighPassTriangular,
    /// Shibata F-weighted IIR noise shaping.
    ///
    /// Uses a 9-tap error-feedback FIR with psychoacoustically optimised
    /// coefficients. Shibata published designs for exactly 44.1 kHz and
    /// 48 kHz; every other rate reuses the nearer table (see
    /// `shibata_design_rate`) and [`Dither::with_sample_rate`] warns once
    /// per process when that happens. At the two design rates this achieves
    /// the lowest perceptible noise floor of all modes.
    Shibata,
    /// First-order error-feedback noise shaping.
    ///
    /// **DEPRECATED.**  Retained for backward compatibility only.
    /// New code should use `Triangular`.
    #[deprecated(
        since = "0.22.0",
        note = "Undefined transfer function; use DitherType::Triangular instead."
    )]
    NoiseShaped,
    /// 16-bit psychoacoustically optimized noise shaping (Wannamaker 4-tap F-weighting).
    NoiseShaped16,
    /// 20-bit noise shaping with balanced high-frequency boost.
    NoiseShaped20,
    /// 24-bit subtle high-order noise shaping for reference dynamic range.
    NoiseShaped24,
    /// 5th-order high-pass noise shaping (engine-designed, 44.1–192 kHz).
    ///
    /// The noise transfer function is fitted against a high-pass target that
    /// starts above the audible band. Measured from the final coefficient
    /// set at 44.1 kHz: 1 kHz −19.1 dB, 10 kHz +2.9 dB, maximum +7.5 dB.
    /// Coefficients are selected per design rate; an unlisted rate reuses the
    /// nearest table and logs once (see [`Dither::with_sample_rate`]).
    Ns5,
    /// 9th-order high-pass noise shaping (engine-designed, 44.1–192 kHz).
    ///
    /// Longer shaping tail than [`Self::Ns5`]; measured at 44.1 kHz:
    /// 1 kHz −17.0 dB, 10 kHz +2.6 dB, maximum +8.5 dB. The 1 kHz figure is
    /// bounded by the one-sample error-feedback delay, not by the order: a
    /// deeper notch would require a larger high-frequency boost.
    Ns9,
    /// 15th-order "low-noise shaping" (engine-designed, 44.1–192 kHz).
    ///
    /// The gentlest high-frequency rise of the three (measured +7.0 dB
    /// maximum and +2.5 dB at 10 kHz at 44.1 kHz) with the longest shaping
    /// tail. At 44.1 kHz it measures −16.3 dB at 1 kHz; at 192 kHz the same
    /// family measures −29.7 dB at 1 kHz with a +6.6 dB maximum. Intended for
    /// high-rate output where the shaped band sits above the audible range.
    Lns15,
}

/// Design rates for the engine's high-order shapers. Each family has a table
/// for every rate here; the nearest table is reused elsewhere, exactly like
/// the Shibata designs. The designs are the engine's own, not HQPlayer's
/// proprietary coefficient sets.
const SHAPER_DESIGN_RATES: [u32; 6] = [44_100, 48_000, 88_200, 96_000, 176_400, 192_000];

impl DitherType {
    /// Map the user-facing configuration enum onto the implementation type.
    pub fn from_noise_shaper(shaper: config::NoiseShaper) -> Self {
        match shaper {
            config::NoiseShaper::Tpdf => Self::Triangular,
            config::NoiseShaper::Shibata => Self::Shibata,
            config::NoiseShaper::Ns5 => Self::Ns5,
            config::NoiseShaper::Ns9 => Self::Ns9,
            config::NoiseShaper::Lns15 => Self::Lns15,
        }
    }

    /// Stable implementation name for diagnostics.
    #[allow(deprecated)]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Rectangular => "rectangular",
            Self::Triangular => "tpdf",
            Self::HighPassTriangular => "high_pass_tpdf",
            Self::Shibata => "shibata",
            Self::NoiseShaped => "noise_shaped",
            Self::NoiseShaped16 => "noise_shaped_16",
            Self::NoiseShaped20 => "noise_shaped_20",
            Self::NoiseShaped24 => "noise_shaped_24",
            Self::Ns5 => "ns5",
            Self::Ns9 => "ns9",
            Self::Lns15 => "lns15",
        }
    }
}

/// Construction-time default for output backends.
///
/// The engine has one global dither policy (`EngineConfig::dither_enabled`)
/// and one global shaper (`EngineConfig::noise_shaper`); per-endpoint
/// overrides are not part of the config surface yet. Output converters are
/// constructed on their own render/control threads, so this is an atomic
/// process configuration value read at construction — never on the audio
/// thread and never per sample. A change takes effect on the next stream
/// open, which is exactly when a converter is rebuilt.
static CONFIGURED_NOISE_SHAPER: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Select the shaper new output converters use.
pub fn set_configured_noise_shaper(shaper: config::NoiseShaper) {
    use std::sync::atomic::Ordering;
    let code = match shaper {
        config::NoiseShaper::Tpdf => 0,
        config::NoiseShaper::Shibata => 1,
        config::NoiseShaper::Ns5 => 2,
        config::NoiseShaper::Ns9 => 3,
        config::NoiseShaper::Lns15 => 4,
    };
    CONFIGURED_NOISE_SHAPER.store(code, Ordering::Relaxed);
}

/// The shaper new output converters will use.
pub fn configured_noise_shaper() -> config::NoiseShaper {
    use std::sync::atomic::Ordering;
    match CONFIGURED_NOISE_SHAPER.load(Ordering::Relaxed) {
        1 => config::NoiseShaper::Shibata,
        2 => config::NoiseShaper::Ns5,
        3 => config::NoiseShaper::Ns9,
        4 => config::NoiseShaper::Lns15,
        _ => config::NoiseShaper::Tpdf,
    }
}

/// [`DitherType`] for [`configured_noise_shaper`].
pub fn configured_dither_type() -> DitherType {
    DitherType::from_noise_shaper(configured_noise_shaper())
}

/// Shibata F-weighted noise shaping coefficients for 44.1 kHz (9-tap IIR error feedback).
/// Derived from Shibata's psychoacoustically optimised design.
const SHIBATA_COEFFS_44100: [f32; 9] = [
    2.0860, -2.5061, 2.1855, -1.7032, 1.0982, -0.5671, 0.2376, -0.0669, 0.0101,
];

/// Shibata F-weighted noise shaping coefficients for 48 kHz.
const SHIBATA_COEFFS_48000: [f32; 9] = [
    2.2374, -2.7120, 2.3784, -1.8640, 1.2035, -0.6267, 0.2625, -0.0740, 0.0119,
];

/// The design rate of the Shibata table selected for `sample_rate`.
///
/// Shibata published exactly two designs — 44.1 and 48 kHz — and there is no
/// table for any other rate: this returns the nearer of the two (the 46 kHz
/// threshold sits between them). It is a separate function from the
/// coefficient lookup so callers — and tests — can tell "landed on a
/// published design" from "reused the nearer table".
fn shibata_design_rate(sample_rate: u32) -> u32 {
    if sample_rate <= 46_000 {
        44_100
    } else {
        48_000
    }
}

/// The coefficient table for a rate returned by [`shibata_design_rate`].
fn shibata_coefficients_for(design_rate: u32) -> [f32; 9] {
    if design_rate == 44_100 {
        SHIBATA_COEFFS_44100
    } else {
        SHIBATA_COEFFS_48000
    }
}

/// One-time guard for the off-design-rate warning: stream setup can run per
/// output configuration, and the same approximation does not need repeating.
static SHIBATA_APPROX_WARNED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Report, once per process, that `sample_rate` has no Shibata design and the
/// `design_rate` table is being reused.
fn warn_shibata_approximation_once(sample_rate: u32, design_rate: u32) {
    use std::sync::atomic::Ordering;
    if !SHIBATA_APPROX_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "dither: no Shibata noise-shaping design exists for {sample_rate} Hz; \
             reusing the {design_rate} Hz coefficients (published designs: 44100, 48000)"
        );
    }
}

/// Shibata published two designs, for 44.1 and 48 kHz, and there is no table
/// for any other rate: [`shibata_design_rate`] picks the nearer of the two, so
/// an 88.2, 96 or 192 kHz stream is shaped with the 48 kHz F-weighting and a
/// 32 kHz stream with the 44.1 kHz design — neither derived for its own band.
///
/// That is a real limitation, stated rather than implied, and
/// [`Dither::with_sample_rate`] logs a warning (once per process) whenever a
/// stream lands off the two published rates. Inaudibility at other rates is
/// *not* asserted: the designs were never characterised there. The
/// alternative was worse: the conversion path used to select its
/// coefficients from a hardcoded `44100`, so *every* rate above 44.1 kHz was
/// shaped against the wrong noise floor. See
/// [`AudioFormatConverter::new_at_rate`] and
/// `shibata_coefficients_depend_on_the_sample_rate`.
///
/// The 16-bit shaping filter is a separate, fourth-order design:
///
/// Wannamaker F-weighted 4-tap noise shaping coefficients for 16-bit word length.
/// H(z) = 1 - (2.033 z^-1 - 2.165 z^-2 + 1.959 z^-3 - 0.835 z^-4)
const NOISE_SHAPING_16: [f32; 4] = [2.033, -2.165, 1.959, -0.835];

/// Psychoacoustically tuned 3-tap noise shaping coefficients for 20-bit word length.
const NOISE_SHAPING_20: [f32; 3] = [1.5, -0.8, 0.1];

/// High-order subtle 2-tap noise shaping coefficients for 24-bit word length.
const NOISE_SHAPING_24: [f32; 2] = [1.0, -0.25];

// ── High-order shapers (NS5 / NS9 / LNS15) ───────────────────────────────
//
// Each table is a causal FIR error-feedback filter C(z). The realised noise
// transfer function is NTF(z) = 1 − C(z); positive coefficients therefore
// suppress the band where C is close to 1. The designs were fitted by
// weighted least squares against a high-pass target with a transition above
// the audible band, then characterised from the final quantised coefficient
// set. They are *not* copies of HQPlayer's proprietary tables; the measured
// numbers in the README and in the module tests are this engine's.
//
// `f32` is deliberate: this runs once per output sample on the audio thread,
// and the error history is already f32 in the integer-quantization path.

const NS5_44100: [f32; 11] = [
    0.877138417,
    0.243680333,
    -0.237547683,
    0.227532042,
    -0.213863141,
    0.197030485,
    -0.177589740,
    0.156368383,
    -0.134220614,
    0.112173294,
    -0.091128331,
];
const NS5_48000: [f32; 11] = [
    0.841361250,
    0.313634637,
    -0.302864579,
    0.285617343,
    -0.262791958,
    0.235658619,
    -0.205586570,
    0.174153800,
    -0.142866075,
    0.113249575,
    -0.086544265,
];
const NS5_88200: [f32; 11] = [
    0.607078474,
    0.738934950,
    -0.617832170,
    0.468169084,
    -0.333035569,
    0.232911481,
    -0.166338687,
    0.122834546,
    -0.092147527,
    0.066906659,
    -0.044015098,
];
const NS5_96000: [f32; 11] = [
    0.584812705,
    0.775410204,
    -0.635873982,
    0.469031629,
    -0.325286257,
    0.224157317,
    -0.159386129,
    0.117537449,
    -0.088170542,
    0.064121079,
    -0.041970526,
];
const NS5_176400: [f32; 11] = [
    0.470281188,
    0.948817007,
    -0.695237623,
    0.445892548,
    -0.279196047,
    0.185341085,
    -0.130829477,
    0.095652683,
    -0.071593956,
    0.052944585,
    -0.034120339,
];
const NS5_192000: [f32; 11] = [
    0.459191076,
    0.964279727,
    -0.698228764,
    0.441970470,
    -0.274864283,
    0.182228119,
    -0.128520383,
    0.093932594,
    -0.070298791,
    0.052056791,
    -0.033534755,
];

const NS9_44100: [f32; 19] = [
    0.885312738,
    0.228588229,
    -0.226219957,
    0.222394167,
    -0.217201068,
    0.210835772,
    -0.203420783,
    0.195141729,
    -0.186073937,
    0.176330200,
    -0.165906209,
    0.154851027,
    -0.143134480,
    0.130833467,
    -0.118004215,
    0.104859539,
    -0.091618291,
    0.078649700,
    -0.066286462,
];
const NS9_48000: [f32; 19] = [
    0.853905760,
    0.290804755,
    -0.286652448,
    0.279877238,
    -0.270642009,
    0.259281335,
    -0.246154729,
    0.231762332,
    -0.216548964,
    0.200978891,
    -0.185327106,
    0.169782612,
    -0.154309013,
    0.138841229,
    -0.123234194,
    0.107509623,
    -0.091793047,
    0.076483259,
    -0.062066351,
];
const NS9_88200: [f32; 19] = [
    0.645621525,
    0.690507466,
    -0.639676742,
    0.566543814,
    -0.483895068,
    0.403019844,
    -0.331131910,
    0.271191847,
    -0.223055506,
    0.184991596,
    -0.154814038,
    0.130737959,
    -0.111325262,
    0.095270168,
    -0.081628931,
    0.069989169,
    -0.059280023,
    0.047484422,
    -0.033950174,
];
const NS9_96000: [f32; 19] = [
    0.625635229,
    0.727262131,
    -0.668029838,
    0.584233335,
    -0.491641727,
    0.403448581,
    -0.327301443,
    0.265446463,
    -0.216769005,
    0.178899259,
    -0.149229137,
    0.125680668,
    -0.106810482,
    0.091357327,
    -0.078223926,
    0.066997576,
    -0.056854339,
    0.045683557,
    -0.032471327,
];
const NS9_176400: [f32; 19] = [
    0.522827658,
    0.910627973,
    -0.795847239,
    0.648236438,
    -0.505019202,
    0.386943139,
    -0.297947484,
    0.233101088,
    -0.185808280,
    0.150805124,
    -0.124303871,
    0.103891521,
    -0.087775410,
    0.074761930,
    -0.064092290,
    0.054802653,
    -0.046611615,
    0.038214168,
    -0.026719829,
];
const NS9_192000: [f32; 19] = [
    0.512859269,
    0.927831998,
    -0.806572798,
    0.652206982,
    -0.504419230,
    0.384268714,
    -0.294738718,
    0.230033409,
    -0.183064607,
    0.148412160,
    -0.122247493,
    0.102106619,
    -0.086250850,
    0.073441313,
    -0.062956572,
    0.053853388,
    -0.045792543,
    0.037613819,
    -0.026288154,
];

const LNS15_44100: [f32; 31] = [
    0.963434946,
    0.073033032,
    -0.072731628,
    0.072239323,
    -0.071550010,
    0.070682343,
    -0.069636176,
    0.068435767,
    -0.067084871,
    0.065608943,
    -0.064010107,
    0.062310315,
    -0.060508126,
    0.058623887,
    -0.056657871,
    0.054635412,
    -0.052563239,
    0.050471356,
    -0.048365596,
    0.046266510,
    -0.044161065,
    0.042043673,
    -0.039873089,
    0.037620766,
    -0.035235475,
    0.032697645,
    -0.029986268,
    0.027130661,
    -0.024169942,
    0.021193807,
    -0.018289801,
];
const LNS15_48000: [f32; 31] = [
    0.953760248,
    0.092286853,
    -0.091704721,
    0.090759366,
    -0.089463870,
    0.087854372,
    -0.085947325,
    0.083779440,
    -0.081369444,
    0.078760792,
    -0.075982727,
    0.073088853,
    -0.070112562,
    0.067101514,
    -0.064073089,
    0.061053805,
    -0.058042813,
    0.055058639,
    -0.052105145,
    0.049213466,
    -0.046396869,
    0.043679773,
    -0.041044368,
    0.038461741,
    -0.035852924,
    0.033143415,
    -0.030248447,
    0.027142492,
    -0.023842284,
    0.020454669,
    -0.017123762,
];
const LNS15_88200: [f32; 31] = [
    0.889131723,
    0.219252465,
    -0.212058836,
    0.200896215,
    -0.186814821,
    0.170996630,
    -0.154549886,
    0.138392003,
    -0.123160995,
    0.109243332,
    -0.096800783,
    0.085843627,
    -0.076274016,
    0.067964912,
    -0.060760259,
    0.054507139,
    -0.049065051,
    0.044326468,
    -0.040171864,
    0.036509611,
    -0.033274643,
    0.030403713,
    -0.027810344,
    0.025467630,
    -0.023346135,
    0.021351872,
    -0.019417250,
    0.017577540,
    -0.015631201,
    0.013054598,
    -0.009693523,
];
const LNS15_96000: [f32; 31] = [
    0.882900921,
    0.231275665,
    -0.222848479,
    0.209873972,
    -0.193692964,
    0.175774427,
    -0.157442830,
    0.139737197,
    -0.123335414,
    0.108593667,
    -0.095606572,
    0.084320931,
    -0.074580376,
    0.066198770,
    -0.058987599,
    0.052778316,
    -0.047402668,
    0.042737843,
    -0.038673653,
    0.035106471,
    -0.031953928,
    0.029170699,
    -0.026674117,
    0.024409096,
    -0.022362116,
    0.020465866,
    -0.018615413,
    0.016841065,
    -0.015013321,
    0.012584980,
    -0.009290316,
];
const LNS15_176400: [f32; 31] = [
    0.850812473,
    0.292377090,
    -0.275502785,
    0.250718346,
    -0.221792466,
    0.192218470,
    -0.164499058,
    0.139982408,
    -0.119080754,
    0.101632491,
    -0.087203755,
    0.075306147,
    -0.065466617,
    0.057297649,
    -0.050468131,
    0.044721386,
    -0.039853052,
    0.035694617,
    -0.032122250,
    0.029029867,
    -0.026328654,
    0.023965655,
    -0.021860203,
    0.019991138,
    -0.018295916,
    0.016731656,
    -0.015289845,
    0.013833460,
    -0.012379850,
    0.010613737,
    -0.007705328,
];
const LNS15_192000: [f32; 31] = [
    0.847700837,
    0.298221957,
    -0.280331245,
    0.254186795,
    -0.223887310,
    0.193162085,
    -0.164610831,
    0.139565584,
    -0.118367159,
    0.100779068,
    -0.086304611,
    0.074415529,
    -0.064613812,
    0.056494607,
    -0.049722036,
    0.044030814,
    -0.039216547,
    0.035109938,
    -0.031583035,
    0.028535879,
    -0.025873296,
    0.023546884,
    -0.021476971,
    0.019636299,
    -0.017974339,
    0.016435871,
    -0.015022199,
    0.013598640,
    -0.012167949,
    0.010453405,
    -0.007585369,
];

/// The high-order family a [`DitherType`] selects, if any.
fn high_order_coefficients(dither_type: DitherType, sample_rate: u32) -> Option<&'static [f32]> {
    let table = |rates: [u32; 6], tables: [&'static [f32]; 6]| {
        let rate = nearest_design_rate(sample_rate, rates);
        tables[rates.iter().position(|r| *r == rate).unwrap()]
    };
    match dither_type {
        DitherType::NoiseShaped16 => Some(&NOISE_SHAPING_16),
        DitherType::NoiseShaped20 => Some(&NOISE_SHAPING_20),
        DitherType::NoiseShaped24 => Some(&NOISE_SHAPING_24),
        DitherType::Ns5 => Some(table(
            SHAPER_DESIGN_RATES,
            [
                &NS5_44100,
                &NS5_48000,
                &NS5_88200,
                &NS5_96000,
                &NS5_176400,
                &NS5_192000,
            ],
        )),
        DitherType::Ns9 => Some(table(
            SHAPER_DESIGN_RATES,
            [
                &NS9_44100,
                &NS9_48000,
                &NS9_88200,
                &NS9_96000,
                &NS9_176400,
                &NS9_192000,
            ],
        )),
        DitherType::Lns15 => Some(table(
            SHAPER_DESIGN_RATES,
            [
                &LNS15_44100,
                &LNS15_48000,
                &LNS15_88200,
                &LNS15_96000,
                &LNS15_176400,
                &LNS15_192000,
            ],
        )),
        _ => None,
    }
}

/// Nearest design rate by absolute distance; ties pick the lower rate. Rates
/// outside the published set are reported by
/// [`warn_high_order_approximation_once`].
fn nearest_design_rate(sample_rate: u32, rates: [u32; 6]) -> u32 {
    let mut best = rates[0];
    let mut best_distance = u64::MAX;
    for rate in rates {
        let distance = sample_rate.abs_diff(rate) as u64;
        if distance < best_distance {
            best_distance = distance;
            best = rate;
        }
    }
    best
}

static HIGH_ORDER_APPROX_WARNED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

fn warn_high_order_approximation_once(sample_rate: u32, design_rate: u32) {
    use std::sync::atomic::Ordering;
    if !HIGH_ORDER_APPROX_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "dither: no high-order noise-shaping design exists for {sample_rate} Hz; \
             reusing the {design_rate} Hz coefficients (designs: 44100, 48000, 88200, \
             96000, 176400, 192000)"
        );
    }
}

/// Dither processor.
///
/// Owned by [`crate::output::format_converter::AudioFormatConverter`] which
/// applies it exactly once, immediately before the integer-quantization step.
///
/// This is a standalone type, not a `DspPipeline` stage. `DspPipeline` once
/// carried a `dither` field; it was removed because the pipeline operates
/// entirely in `f32`/`f64` and does not quantize, and dithering belongs at the
/// integer-quantization boundary (`output::format_converter`).
#[derive(Debug, Clone)]
pub struct Dither {
    dither_type: DitherType,
    bit_depth: u32,
    /// Previous quantization error for noise shaping (left channel)
    shape_left: f32,
    /// Previous quantization error for noise shaping (right channel)
    shape_right: f32,
    /// PRNG state for left channel (xorshift64)
    rng_state_left: u64,
    /// PRNG state for right channel (xorshift64)
    rng_state_right: u64,
    enabled: bool,
    /// When true, the output format is f32 or f64 (no quantization).
    /// Dither is unconditionally disabled regardless of all other settings.
    output_is_float: bool,
    // HP-TPDF state: previous rectangular sample for each channel
    hp_prev_left: f32,
    hp_prev_right: f32,
    // Shibata 9-tap IIR error history for left and right
    shibata_err_left: [f32; 9],
    shibata_err_right: [f32; 9],
    shibata_err_pos: usize,
    // Active Shibata coefficients (set from sample_rate at construction)
    shibata_coeffs: [f32; 9],
    /// The rate `shibata_coeffs` was selected for.
    sample_rate: u32,
    /// Active high-order error-feedback coefficients, if the selected
    /// [`DitherType`] is a noise shaper. Empty for every other mode.
    shaper_coeffs: &'static [f32],
    /// High-order error history. Sized for the longest design (LNS15, 31
    /// taps), so every shaper fits without a per-sample bounds check.
    shaper_err_left: [f32; 32],
    shaper_err_right: [f32; 32],
    shaper_err_pos: usize,
}

impl Dither {
    /// Create a new dither processor for a 44.1 kHz stream.
    ///
    /// Prefer [`Self::with_sample_rate`] at any other rate. Shibata's
    /// coefficients are rate-dependent, and this constructor selects the
    /// 44.1 kHz set unconditionally.
    ///
    /// # Arguments
    /// * `dither_type` — The dither algorithm to use
    /// * `bit_depth`   — Target bit depth (1–32). Dither is a no-op at ≥ 32.
    pub fn new(dither_type: DitherType, bit_depth: u32) -> Self {
        Self::with_sample_rate(dither_type, bit_depth, 44100)
    }

    /// Create a dither processor with sample-rate-aware Shibata coefficients.
    ///
    /// Shibata published designs for exactly 44.1 and 48 kHz. Any other rate
    /// reuses the nearer table (see `shibata_design_rate`) and this
    /// constructor logs a warning — once per process — when the selected
    /// type is [`DitherType::Shibata`], so the approximation is visible in
    /// diagnostics rather than silent.
    pub fn with_sample_rate(dither_type: DitherType, bit_depth: u32, sample_rate: u32) -> Self {
        let design_rate = shibata_design_rate(sample_rate);
        let shibata_coeffs = shibata_coefficients_for(design_rate);
        let shaper_coeffs = high_order_coefficients(dither_type, sample_rate).unwrap_or(&[]);
        if matches!(dither_type, DitherType::Shibata)
            && sample_rate != 44_100
            && sample_rate != 48_000
        {
            warn_shibata_approximation_once(sample_rate, design_rate);
        }
        if !shaper_coeffs.is_empty() && !SHAPER_DESIGN_RATES.contains(&sample_rate) {
            let nearest = nearest_design_rate(sample_rate, SHAPER_DESIGN_RATES);
            warn_high_order_approximation_once(sample_rate, nearest);
        }
        Self {
            dither_type,
            bit_depth: bit_depth.clamp(1, 32),
            shape_left: 0.0,
            shape_right: 0.0,
            rng_state_left: Self::random_seed(),
            rng_state_right: Self::random_seed().wrapping_add(0xDEADBEEF_12345678),
            enabled: dither_type != DitherType::None,
            output_is_float: false,
            hp_prev_left: 0.0,
            hp_prev_right: 0.0,
            shibata_err_left: [0.0; 9],
            shibata_err_right: [0.0; 9],
            shibata_err_pos: 0,
            shibata_coeffs,
            sample_rate,
            shaper_coeffs,
            shaper_err_left: [0.0; 32],
            shaper_err_right: [0.0; 32],
            shaper_err_pos: 0,
        }
    }

    /// The sample rate this processor's noise-shaping coefficients were
    /// selected for.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Seed the dither generator, so the noise sequence is reproducible.
    ///
    /// Both channels are seeded deterministically from `seed` and a fixed
    /// offset, so a given seed always produces the same noise. A zero seed is
    /// replaced rather than accepted, because it would leave the generator in
    /// its degenerate all-zero state.
    ///
    /// This exists for measurement: a noise floor cannot be asserted from the
    /// engine's default seeding, because the estimate moves from run to run
    /// with the PRNG and a tolerance tight enough to catch a wrong constant
    /// would then fail at random. It is also useful offline -- two renders of
    /// the same material differ only in their dither, and that is sometimes
    /// exactly what a comparison wants.
    pub fn set_random_seed(&mut self, seed: u64) {
        let seed = if seed == 0 {
            0x1234_5678_9ABC_DEF0
        } else {
            seed
        };
        self.rng_state_left = seed;
        self.rng_state_right = seed.wrapping_add(0xDEAD_BEEF_1234_5678);
    }

    /// Mark the output format as floating-point (f32 or f64).
    ///
    /// When `true`, all dither is unconditionally disabled — no quantization
    /// occurs in a float output path, so adding noise would be harmful.
    pub fn set_output_is_float(&mut self, is_float: bool) {
        self.output_is_float = is_float;
    }

    #[inline]
    fn is_active(&self) -> bool {
        self.enabled
            && !self.output_is_float
            && self.dither_type != DitherType::None
            && self.bit_depth < 32
    }

    /// Compute Shibata noise-shaping error-feedback value for one channel.
    #[inline]
    fn shibata_feedback(&self, err: &[f32; 9], pos: usize) -> f32 {
        let mut sum = 0.0f32;
        for k in 0..9 {
            let idx = (pos + 9 - 1 - k) % 9;
            sum += self.shibata_coeffs[k] * err[idx];
        }
        sum
    }

    /// Compute high-order error-feedback for one channel.
    ///
    /// `err` is a ring of 32 samples and `pos` is the next write position, so
    /// the newest error is at `pos - 1`. Every published design is at most 31
    /// taps, which is why the ring is a fixed array rather than a `Vec`.
    #[inline]
    fn shaper_feedback(coeffs: &[f32], err: &[f32; 32], pos: usize) -> f32 {
        let mut sum = 0.0f32;
        for (k, &c) in coeffs.iter().enumerate() {
            let idx = (pos + 32 - 1 - k) % 32;
            sum += c * err[idx];
        }
        sum
    }

    /// Whether the selected mode is handled by the generic high-order path.
    #[inline]
    fn is_high_order(&self) -> bool {
        !self.shaper_coeffs.is_empty()
    }

    fn random_seed() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let instance_id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x12345678_9ABCDEF0);
        let seed = ns
            .wrapping_add(instance_id.wrapping_mul(0x9E3779B97F4A7C15))
            .wrapping_mul(0x5851F42D4C957F2D);
        if seed == 0 {
            0x12345678_9ABCDEF0
        } else {
            seed
        }
    }

    #[inline]
    fn next_random(state: &mut u64) -> u64 {
        if *state == 0 {
            *state = 0x12345678_9ABCDEF0;
        }
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    #[inline]
    fn next_random_f32(state: &mut u64) -> f32 {
        let bits = Self::next_random(state);
        let top24 = (bits >> (64 - 24)) as f32 + 0.5;
        top24 * (1.0 / 8388608.0) - 1.0
    }

    #[inline]
    fn next_random_f64(state: &mut u64) -> f64 {
        let bits = Self::next_random(state);
        let top53 = (bits >> (64 - 53)) as f64 + 0.5;
        top53 * (1.0 / 9007199254740992.0) - 1.0
    }

    /// Process a stereo sample pair with dithering and quantization.
    ///
    /// Returns the dithered and quantized sample pair, clamped to [-1.0, 1.0].
    #[inline]
    pub fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        if !self.is_active() {
            return (left, right);
        }

        let quant_steps = 1u64 << (self.bit_depth - 1);
        let quant_steps_f = quant_steps as f32;
        let half_lsb = 0.5 / quant_steps_f;

        #[allow(deprecated)]
        let (dithered_l, dithered_r) = match self.dither_type {
            DitherType::None => (left, right),

            DitherType::Rectangular => {
                let nl = Self::next_random_f32(&mut self.rng_state_left) * half_lsb;
                let nr = Self::next_random_f32(&mut self.rng_state_right) * half_lsb;
                (left + nl, right + nr)
            }

            DitherType::Triangular => {
                let nl = (Self::next_random_f32(&mut self.rng_state_left)
                    + Self::next_random_f32(&mut self.rng_state_left))
                    * half_lsb;
                let nr = (Self::next_random_f32(&mut self.rng_state_right)
                    + Self::next_random_f32(&mut self.rng_state_right))
                    * half_lsb;
                (left + nl, right + nr)
            }

            DitherType::HighPassTriangular => {
                // HP-TPDF: noise = current_rect - prev_rect (difference of successive rectangulars).
                // Spectral null at DC, bump at Nyquist — lowers audible noise floor.
                let cur_l = Self::next_random_f32(&mut self.rng_state_left);
                let cur_r = Self::next_random_f32(&mut self.rng_state_right);
                let nl = (cur_l - self.hp_prev_left) * half_lsb;
                let nr = (cur_r - self.hp_prev_right) * half_lsb;
                self.hp_prev_left = cur_l;
                self.hp_prev_right = cur_r;
                (left + nl, right + nr)
            }

            DitherType::Shibata => {
                // Shibata F-weighted noise shaping: TPDF + 9-tap IIR error feedback.
                let noise_l = (Self::next_random_f32(&mut self.rng_state_left)
                    + Self::next_random_f32(&mut self.rng_state_left))
                    * half_lsb;
                let noise_r = (Self::next_random_f32(&mut self.rng_state_right)
                    + Self::next_random_f32(&mut self.rng_state_right))
                    * half_lsb;

                let pos = self.shibata_err_pos;
                let feedback_l = self.shibata_feedback(&self.shibata_err_left, pos);
                let feedback_r = self.shibata_feedback(&self.shibata_err_right, pos);

                let shaped_l = left + noise_l - feedback_l;
                let shaped_r = right + noise_r - feedback_r;

                let q_l = (shaped_l * quant_steps_f).round() / quant_steps_f;
                let q_r = (shaped_r * quant_steps_f).round() / quant_steps_f;

                // Store quantization error for next feedback iteration
                self.shibata_err_left[pos] = q_l - shaped_l;
                self.shibata_err_right[pos] = q_r - shaped_r;
                self.shibata_err_pos = (pos + 1) % 9;

                return (q_l.clamp(-1.0, 1.0), q_r.clamp(-1.0, 1.0));
            }

            DitherType::NoiseShaped => {
                // Retained for backward compatibility only — see deprecation note.
                let nl = (Self::next_random_f32(&mut self.rng_state_left)
                    + Self::next_random_f32(&mut self.rng_state_left))
                    * half_lsb;
                let nr = (Self::next_random_f32(&mut self.rng_state_right)
                    + Self::next_random_f32(&mut self.rng_state_right))
                    * half_lsb;
                let shaped_l = left + nl - self.shape_left * 0.5;
                let shaped_r = right + nr - self.shape_right * 0.5;
                let q_l = (shaped_l * quant_steps_f).round() / quant_steps_f;
                let q_r = (shaped_r * quant_steps_f).round() / quant_steps_f;
                self.shape_left = q_l - shaped_l + self.shape_left * 0.5;
                self.shape_right = q_r - shaped_r + self.shape_right * 0.5;
                return (q_l.clamp(-1.0, 1.0), q_r.clamp(-1.0, 1.0));
            }

            DitherType::NoiseShaped16
            | DitherType::NoiseShaped20
            | DitherType::NoiseShaped24
            | DitherType::Ns5
            | DitherType::Ns9
            | DitherType::Lns15 => {
                debug_assert!(self.is_high_order());
                let noise_l = (Self::next_random_f32(&mut self.rng_state_left)
                    + Self::next_random_f32(&mut self.rng_state_left))
                    * half_lsb;
                let noise_r = (Self::next_random_f32(&mut self.rng_state_right)
                    + Self::next_random_f32(&mut self.rng_state_right))
                    * half_lsb;

                let pos = self.shaper_err_pos;
                let feedback_l =
                    Self::shaper_feedback(self.shaper_coeffs, &self.shaper_err_left, pos);
                let feedback_r =
                    Self::shaper_feedback(self.shaper_coeffs, &self.shaper_err_right, pos);

                let shaped_l = left + noise_l - feedback_l;
                let shaped_r = right + noise_r - feedback_r;

                let q_l = (shaped_l * quant_steps_f).round() / quant_steps_f;
                let q_r = (shaped_r * quant_steps_f).round() / quant_steps_f;

                self.shaper_err_left[pos] = q_l - shaped_l;
                self.shaper_err_right[pos] = q_r - shaped_r;
                self.shaper_err_pos = (pos + 1) % 32;

                return (q_l.clamp(-1.0, 1.0), q_r.clamp(-1.0, 1.0));
            }
        };

        let ql = (dithered_l * quant_steps_f).round() / quant_steps_f;
        let qr = (dithered_r * quant_steps_f).round() / quant_steps_f;
        (ql.clamp(-1.0, 1.0), qr.clamp(-1.0, 1.0))
    }

    /// Process a stereo f64 sample pair with full 64-bit precision dithering and quantization.
    #[inline]
    pub fn process_f64(&mut self, left: f64, right: f64) -> (f64, f64) {
        if !self.is_active() {
            return (left, right);
        }

        let quant_steps = 1u64 << (self.bit_depth - 1);
        let quant_steps_f = quant_steps as f64;
        let half_lsb = 0.5 / quant_steps_f;

        #[allow(deprecated)]
        let (dithered_l, dithered_r) = match self.dither_type {
            DitherType::None => (left, right),

            DitherType::Rectangular => {
                let nl = Self::next_random_f64(&mut self.rng_state_left) * half_lsb;
                let nr = Self::next_random_f64(&mut self.rng_state_right) * half_lsb;
                (left + nl, right + nr)
            }

            DitherType::Triangular => {
                let nl = (Self::next_random_f64(&mut self.rng_state_left)
                    + Self::next_random_f64(&mut self.rng_state_left))
                    * half_lsb;
                let nr = (Self::next_random_f64(&mut self.rng_state_right)
                    + Self::next_random_f64(&mut self.rng_state_right))
                    * half_lsb;
                (left + nl, right + nr)
            }

            DitherType::HighPassTriangular => {
                let cur_l = Self::next_random_f64(&mut self.rng_state_left) as f32;
                let cur_r = Self::next_random_f64(&mut self.rng_state_right) as f32;
                let nl = (cur_l - self.hp_prev_left) as f64 * half_lsb;
                let nr = (cur_r - self.hp_prev_right) as f64 * half_lsb;
                self.hp_prev_left = cur_l;
                self.hp_prev_right = cur_r;
                (left + nl, right + nr)
            }

            DitherType::Shibata => {
                let noise_l = (Self::next_random_f64(&mut self.rng_state_left)
                    + Self::next_random_f64(&mut self.rng_state_left))
                    * half_lsb;
                let noise_r = (Self::next_random_f64(&mut self.rng_state_right)
                    + Self::next_random_f64(&mut self.rng_state_right))
                    * half_lsb;

                let pos = self.shibata_err_pos;
                let feedback_l = self.shibata_feedback(&self.shibata_err_left, pos) as f64;
                let feedback_r = self.shibata_feedback(&self.shibata_err_right, pos) as f64;

                let shaped_l = left + noise_l - feedback_l;
                let shaped_r = right + noise_r - feedback_r;

                let q_l = (shaped_l * quant_steps_f).round() / quant_steps_f;
                let q_r = (shaped_r * quant_steps_f).round() / quant_steps_f;

                self.shibata_err_left[pos] = (q_l - shaped_l) as f32;
                self.shibata_err_right[pos] = (q_r - shaped_r) as f32;
                self.shibata_err_pos = (pos + 1) % 9;

                return (q_l.clamp(-1.0, 1.0), q_r.clamp(-1.0, 1.0));
            }

            DitherType::NoiseShaped => {
                let nl = (Self::next_random_f64(&mut self.rng_state_left)
                    + Self::next_random_f64(&mut self.rng_state_left))
                    * half_lsb;
                let nr = (Self::next_random_f64(&mut self.rng_state_right)
                    + Self::next_random_f64(&mut self.rng_state_right))
                    * half_lsb;
                let shaped_l = left + nl - (self.shape_left as f64) * 0.5;
                let shaped_r = right + nr - (self.shape_right as f64) * 0.5;
                let q_l = (shaped_l * quant_steps_f).round() / quant_steps_f;
                let q_r = (shaped_r * quant_steps_f).round() / quant_steps_f;
                self.shape_left = (q_l - shaped_l + (self.shape_left as f64) * 0.5) as f32;
                self.shape_right = (q_r - shaped_r + (self.shape_right as f64) * 0.5) as f32;
                return (q_l.clamp(-1.0, 1.0), q_r.clamp(-1.0, 1.0));
            }

            DitherType::NoiseShaped16
            | DitherType::NoiseShaped20
            | DitherType::NoiseShaped24
            | DitherType::Ns5
            | DitherType::Ns9
            | DitherType::Lns15 => {
                debug_assert!(self.is_high_order());
                let noise_l = (Self::next_random_f64(&mut self.rng_state_left)
                    + Self::next_random_f64(&mut self.rng_state_left))
                    * half_lsb;
                let noise_r = (Self::next_random_f64(&mut self.rng_state_right)
                    + Self::next_random_f64(&mut self.rng_state_right))
                    * half_lsb;

                let pos = self.shaper_err_pos;
                let feedback_l =
                    Self::shaper_feedback(self.shaper_coeffs, &self.shaper_err_left, pos) as f64;
                let feedback_r =
                    Self::shaper_feedback(self.shaper_coeffs, &self.shaper_err_right, pos) as f64;

                let shaped_l = left + noise_l - feedback_l;
                let shaped_r = right + noise_r - feedback_r;

                let q_l = (shaped_l * quant_steps_f).round() / quant_steps_f;
                let q_r = (shaped_r * quant_steps_f).round() / quant_steps_f;

                self.shaper_err_left[pos] = (q_l - shaped_l) as f32;
                self.shaper_err_right[pos] = (q_r - shaped_r) as f32;
                self.shaper_err_pos = (pos + 1) % 32;

                return (q_l.clamp(-1.0, 1.0), q_r.clamp(-1.0, 1.0));
            }
        };

        let ql = (dithered_l * quant_steps_f).round() / quant_steps_f;
        let qr = (dithered_r * quant_steps_f).round() / quant_steps_f;
        (ql.clamp(-1.0, 1.0), qr.clamp(-1.0, 1.0))
    }

    /// Process a block of stereo frames in place with dithering and quantization.
    #[inline]
    pub fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        if !self.is_active() {
            return;
        }
        let n = left.len().min(right.len());
        if n == 0 {
            return;
        }

        let quant_steps = 1u64 << (self.bit_depth - 1);
        let quant_steps_f = quant_steps as f32;
        let recip_quant_steps_f = 1.0 / quant_steps_f;
        let half_lsb = 0.5 * recip_quant_steps_f;

        #[allow(deprecated)]
        match self.dither_type {
            DitherType::None => {}

            DitherType::Triangular => {
                for i in 0..n {
                    let nl = (Self::next_random_f32(&mut self.rng_state_left)
                        + Self::next_random_f32(&mut self.rng_state_left))
                        * half_lsb;
                    let nr = (Self::next_random_f32(&mut self.rng_state_right)
                        + Self::next_random_f32(&mut self.rng_state_right))
                        * half_lsb;
                    let dithered_l = left[i] + nl;
                    let dithered_r = right[i] + nr;
                    let ql = (dithered_l * quant_steps_f).round() * recip_quant_steps_f;
                    let qr = (dithered_r * quant_steps_f).round() * recip_quant_steps_f;
                    left[i] = ql.clamp(-1.0, 1.0);
                    right[i] = qr.clamp(-1.0, 1.0);
                }
            }

            DitherType::Rectangular => {
                for i in 0..n {
                    let nl = Self::next_random_f32(&mut self.rng_state_left) * half_lsb;
                    let nr = Self::next_random_f32(&mut self.rng_state_right) * half_lsb;
                    let dithered_l = left[i] + nl;
                    let dithered_r = right[i] + nr;
                    let ql = (dithered_l * quant_steps_f).round() * recip_quant_steps_f;
                    let qr = (dithered_r * quant_steps_f).round() * recip_quant_steps_f;
                    left[i] = ql.clamp(-1.0, 1.0);
                    right[i] = qr.clamp(-1.0, 1.0);
                }
            }

            DitherType::HighPassTriangular => {
                for i in 0..n {
                    let cur_l = Self::next_random_f32(&mut self.rng_state_left);
                    let cur_r = Self::next_random_f32(&mut self.rng_state_right);
                    let nl = (cur_l - self.hp_prev_left) * half_lsb;
                    let nr = (cur_r - self.hp_prev_right) * half_lsb;
                    self.hp_prev_left = cur_l;
                    self.hp_prev_right = cur_r;
                    let dithered_l = left[i] + nl;
                    let dithered_r = right[i] + nr;
                    let ql = (dithered_l * quant_steps_f).round() * recip_quant_steps_f;
                    let qr = (dithered_r * quant_steps_f).round() * recip_quant_steps_f;
                    left[i] = ql.clamp(-1.0, 1.0);
                    right[i] = qr.clamp(-1.0, 1.0);
                }
            }

            DitherType::Shibata => {
                for i in 0..n {
                    let noise_l = (Self::next_random_f32(&mut self.rng_state_left)
                        + Self::next_random_f32(&mut self.rng_state_left))
                        * half_lsb;
                    let noise_r = (Self::next_random_f32(&mut self.rng_state_right)
                        + Self::next_random_f32(&mut self.rng_state_right))
                        * half_lsb;

                    let pos = self.shibata_err_pos;
                    let feedback_l = self.shibata_feedback(&self.shibata_err_left, pos);
                    let feedback_r = self.shibata_feedback(&self.shibata_err_right, pos);

                    let shaped_l = left[i] + noise_l - feedback_l;
                    let shaped_r = right[i] + noise_r - feedback_r;

                    let q_l = (shaped_l * quant_steps_f).round() * recip_quant_steps_f;
                    let q_r = (shaped_r * quant_steps_f).round() * recip_quant_steps_f;

                    self.shibata_err_left[pos] = q_l - shaped_l;
                    self.shibata_err_right[pos] = q_r - shaped_r;
                    self.shibata_err_pos = (pos + 1) % 9;

                    left[i] = q_l.clamp(-1.0, 1.0);
                    right[i] = q_r.clamp(-1.0, 1.0);
                }
            }

            DitherType::NoiseShaped => {
                for i in 0..n {
                    let nl = (Self::next_random_f32(&mut self.rng_state_left)
                        + Self::next_random_f32(&mut self.rng_state_left))
                        * half_lsb;
                    let nr = (Self::next_random_f32(&mut self.rng_state_right)
                        + Self::next_random_f32(&mut self.rng_state_right))
                        * half_lsb;
                    let shaped_l = left[i] + nl - self.shape_left * 0.5;
                    let shaped_r = right[i] + nr - self.shape_right * 0.5;
                    let q_l = (shaped_l * quant_steps_f).round() * recip_quant_steps_f;
                    let q_r = (shaped_r * quant_steps_f).round() * recip_quant_steps_f;
                    self.shape_left = q_l - shaped_l + self.shape_left * 0.5;
                    self.shape_right = q_r - shaped_r + self.shape_right * 0.5;
                    left[i] = q_l.clamp(-1.0, 1.0);
                    right[i] = q_r.clamp(-1.0, 1.0);
                }
            }

            DitherType::NoiseShaped16
            | DitherType::NoiseShaped20
            | DitherType::NoiseShaped24
            | DitherType::Ns5
            | DitherType::Ns9
            | DitherType::Lns15 => {
                debug_assert!(self.is_high_order());
                for i in 0..n {
                    let noise_l = (Self::next_random_f32(&mut self.rng_state_left)
                        + Self::next_random_f32(&mut self.rng_state_left))
                        * half_lsb;
                    let noise_r = (Self::next_random_f32(&mut self.rng_state_right)
                        + Self::next_random_f32(&mut self.rng_state_right))
                        * half_lsb;

                    let pos = self.shaper_err_pos;
                    let feedback_l =
                        Self::shaper_feedback(self.shaper_coeffs, &self.shaper_err_left, pos);
                    let feedback_r =
                        Self::shaper_feedback(self.shaper_coeffs, &self.shaper_err_right, pos);

                    let shaped_l = left[i] + noise_l - feedback_l;
                    let shaped_r = right[i] + noise_r - feedback_r;

                    let q_l = (shaped_l * quant_steps_f).round() * recip_quant_steps_f;
                    let q_r = (shaped_r * quant_steps_f).round() * recip_quant_steps_f;

                    self.shaper_err_left[pos] = q_l - shaped_l;
                    self.shaper_err_right[pos] = q_r - shaped_r;
                    self.shaper_err_pos = (pos + 1) % 32;

                    left[i] = q_l.clamp(-1.0, 1.0);
                    right[i] = q_r.clamp(-1.0, 1.0);
                }
            }
        }
    }

    /// Process a block of stereo frames in place with f64 precision dithering and quantization.
    #[inline]
    pub fn process_block_f64(&mut self, left: &mut [f64], right: &mut [f64]) {
        if !self.is_active() {
            return;
        }
        let n = left.len().min(right.len());
        if n == 0 {
            return;
        }

        let quant_steps = 1u64 << (self.bit_depth - 1);
        let quant_steps_f = quant_steps as f64;
        let recip_quant_steps_f = 1.0 / quant_steps_f;
        let half_lsb = 0.5 * recip_quant_steps_f;

        #[allow(deprecated)]
        match self.dither_type {
            DitherType::None => {}

            DitherType::Triangular => {
                for i in 0..n {
                    let nl = (Self::next_random_f64(&mut self.rng_state_left)
                        + Self::next_random_f64(&mut self.rng_state_left))
                        * half_lsb;
                    let nr = (Self::next_random_f64(&mut self.rng_state_right)
                        + Self::next_random_f64(&mut self.rng_state_right))
                        * half_lsb;
                    let dithered_l = left[i] + nl;
                    let dithered_r = right[i] + nr;
                    let ql = (dithered_l * quant_steps_f).round() * recip_quant_steps_f;
                    let qr = (dithered_r * quant_steps_f).round() * recip_quant_steps_f;
                    left[i] = ql.clamp(-1.0, 1.0);
                    right[i] = qr.clamp(-1.0, 1.0);
                }
            }

            DitherType::Rectangular => {
                for i in 0..n {
                    let nl = Self::next_random_f64(&mut self.rng_state_left) * half_lsb;
                    let nr = Self::next_random_f64(&mut self.rng_state_right) * half_lsb;
                    let dithered_l = left[i] + nl;
                    let dithered_r = right[i] + nr;
                    let ql = (dithered_l * quant_steps_f).round() * recip_quant_steps_f;
                    let qr = (dithered_r * quant_steps_f).round() * recip_quant_steps_f;
                    left[i] = ql.clamp(-1.0, 1.0);
                    right[i] = qr.clamp(-1.0, 1.0);
                }
            }

            DitherType::HighPassTriangular => {
                for i in 0..n {
                    let cur_l = Self::next_random_f64(&mut self.rng_state_left) as f32;
                    let cur_r = Self::next_random_f64(&mut self.rng_state_right) as f32;
                    let nl = (cur_l - self.hp_prev_left) as f64 * half_lsb;
                    let nr = (cur_r - self.hp_prev_right) as f64 * half_lsb;
                    self.hp_prev_left = cur_l;
                    self.hp_prev_right = cur_r;
                    let dithered_l = left[i] + nl;
                    let dithered_r = right[i] + nr;
                    let ql = (dithered_l * quant_steps_f).round() * recip_quant_steps_f;
                    let qr = (dithered_r * quant_steps_f).round() * recip_quant_steps_f;
                    left[i] = ql.clamp(-1.0, 1.0);
                    right[i] = qr.clamp(-1.0, 1.0);
                }
            }

            DitherType::Shibata => {
                for i in 0..n {
                    let noise_l = (Self::next_random_f64(&mut self.rng_state_left)
                        + Self::next_random_f64(&mut self.rng_state_left))
                        * half_lsb;
                    let noise_r = (Self::next_random_f64(&mut self.rng_state_right)
                        + Self::next_random_f64(&mut self.rng_state_right))
                        * half_lsb;

                    let pos = self.shibata_err_pos;
                    let feedback_l = self.shibata_feedback(&self.shibata_err_left, pos) as f64;
                    let feedback_r = self.shibata_feedback(&self.shibata_err_right, pos) as f64;

                    let shaped_l = left[i] + noise_l - feedback_l;
                    let shaped_r = right[i] + noise_r - feedback_r;

                    let q_l = (shaped_l * quant_steps_f).round() * recip_quant_steps_f;
                    let q_r = (shaped_r * quant_steps_f).round() * recip_quant_steps_f;

                    self.shibata_err_left[pos] = (q_l - shaped_l) as f32;
                    self.shibata_err_right[pos] = (q_r - shaped_r) as f32;
                    self.shibata_err_pos = (pos + 1) % 9;

                    left[i] = q_l.clamp(-1.0, 1.0);
                    right[i] = q_r.clamp(-1.0, 1.0);
                }
            }

            DitherType::NoiseShaped => {
                for i in 0..n {
                    let nl = (Self::next_random_f64(&mut self.rng_state_left)
                        + Self::next_random_f64(&mut self.rng_state_left))
                        * half_lsb;
                    let nr = (Self::next_random_f64(&mut self.rng_state_right)
                        + Self::next_random_f64(&mut self.rng_state_right))
                        * half_lsb;
                    let shaped_l = left[i] + nl - (self.shape_left as f64) * 0.5;
                    let shaped_r = right[i] + nr - (self.shape_right as f64) * 0.5;
                    let q_l = (shaped_l * quant_steps_f).round() * recip_quant_steps_f;
                    let q_r = (shaped_r * quant_steps_f).round() * recip_quant_steps_f;
                    self.shape_left = (q_l - shaped_l + (self.shape_left as f64) * 0.5) as f32;
                    self.shape_right = (q_r - shaped_r + (self.shape_right as f64) * 0.5) as f32;
                    left[i] = q_l.clamp(-1.0, 1.0);
                    right[i] = q_r.clamp(-1.0, 1.0);
                }
            }

            DitherType::NoiseShaped16
            | DitherType::NoiseShaped20
            | DitherType::NoiseShaped24
            | DitherType::Ns5
            | DitherType::Ns9
            | DitherType::Lns15 => {
                debug_assert!(self.is_high_order());
                for i in 0..n {
                    let noise_l = (Self::next_random_f64(&mut self.rng_state_left)
                        + Self::next_random_f64(&mut self.rng_state_left))
                        * half_lsb;
                    let noise_r = (Self::next_random_f64(&mut self.rng_state_right)
                        + Self::next_random_f64(&mut self.rng_state_right))
                        * half_lsb;

                    let pos = self.shaper_err_pos;
                    let feedback_l =
                        Self::shaper_feedback(self.shaper_coeffs, &self.shaper_err_left, pos)
                            as f64;
                    let feedback_r =
                        Self::shaper_feedback(self.shaper_coeffs, &self.shaper_err_right, pos)
                            as f64;

                    let shaped_l = left[i] + noise_l - feedback_l;
                    let shaped_r = right[i] + noise_r - feedback_r;

                    let q_l = (shaped_l * quant_steps_f).round() * recip_quant_steps_f;
                    let q_r = (shaped_r * quant_steps_f).round() * recip_quant_steps_f;

                    self.shaper_err_left[pos] = (q_l - shaped_l) as f32;
                    self.shaper_err_right[pos] = (q_r - shaped_r) as f32;
                    self.shaper_err_pos = (pos + 1) % 32;

                    left[i] = q_l.clamp(-1.0, 1.0);
                    right[i] = q_r.clamp(-1.0, 1.0);
                }
            }
        }
    }

    /// Process a single (mono) sample with dithering and quantization.
    #[inline]
    pub fn process_mono(&mut self, sample: f32) -> f32 {
        if !self.is_active() {
            return sample;
        }

        let quant_steps = 1u64 << (self.bit_depth - 1);
        let quant_steps_f = quant_steps as f32;
        let half_lsb = 0.5 / quant_steps_f;

        #[allow(deprecated)]
        match self.dither_type {
            DitherType::None => sample,
            DitherType::Rectangular => {
                let noise = Self::next_random_f32(&mut self.rng_state_left) * half_lsb;
                ((sample + noise) * quant_steps_f).round() / quant_steps_f
            }
            DitherType::Triangular => {
                let noise = (Self::next_random_f32(&mut self.rng_state_left)
                    + Self::next_random_f32(&mut self.rng_state_left))
                    * half_lsb;
                ((sample + noise) * quant_steps_f).round() / quant_steps_f
            }
            DitherType::HighPassTriangular => {
                let cur = Self::next_random_f32(&mut self.rng_state_left);
                let noise = (cur - self.hp_prev_left) * half_lsb;
                self.hp_prev_left = cur;
                ((sample + noise) * quant_steps_f).round() / quant_steps_f
            }
            DitherType::Shibata => {
                let noise = (Self::next_random_f32(&mut self.rng_state_left)
                    + Self::next_random_f32(&mut self.rng_state_left))
                    * half_lsb;
                let pos = self.shibata_err_pos;
                let feedback = self.shibata_feedback(&self.shibata_err_left, pos);
                let shaped = sample + noise - feedback;
                let q = (shaped * quant_steps_f).round() / quant_steps_f;
                self.shibata_err_left[pos] = q - shaped;
                self.shibata_err_pos = (pos + 1) % 9;
                q.clamp(-1.0, 1.0)
            }
            DitherType::NoiseShaped => {
                let noise = (Self::next_random_f32(&mut self.rng_state_left)
                    + Self::next_random_f32(&mut self.rng_state_left))
                    * half_lsb;
                let shaped = sample + noise - self.shape_left * 0.5;
                let q = (shaped * quant_steps_f).round() / quant_steps_f;
                self.shape_left = q - shaped + self.shape_left * 0.5;
                q.clamp(-1.0, 1.0)
            }
            DitherType::NoiseShaped16
            | DitherType::NoiseShaped20
            | DitherType::NoiseShaped24
            | DitherType::Ns5
            | DitherType::Ns9
            | DitherType::Lns15 => {
                debug_assert!(self.is_high_order());
                let noise = (Self::next_random_f32(&mut self.rng_state_left)
                    + Self::next_random_f32(&mut self.rng_state_left))
                    * half_lsb;
                let pos = self.shaper_err_pos;
                let feedback =
                    Self::shaper_feedback(self.shaper_coeffs, &self.shaper_err_left, pos);
                let shaped = sample + noise - feedback;
                let q = (shaped * quant_steps_f).round() / quant_steps_f;
                self.shaper_err_left[pos] = q - shaped;
                self.shaper_err_pos = (pos + 1) % 32;
                q.clamp(-1.0, 1.0)
            }
        }
    }

    /// Process a single (mono) f64 sample with dithering and quantization.
    #[inline]
    pub fn process_mono_f64(&mut self, sample: f64) -> f64 {
        if !self.is_active() {
            return sample;
        }

        let quant_steps = 1u64 << (self.bit_depth - 1);
        let quant_steps_f = quant_steps as f64;
        let half_lsb = 0.5 / quant_steps_f;

        #[allow(deprecated)]
        match self.dither_type {
            DitherType::None => sample,
            DitherType::Rectangular => {
                let noise = Self::next_random_f64(&mut self.rng_state_left) * half_lsb;
                ((sample + noise) * quant_steps_f).round() / quant_steps_f
            }
            DitherType::Triangular => {
                let noise = (Self::next_random_f64(&mut self.rng_state_left)
                    + Self::next_random_f64(&mut self.rng_state_left))
                    * half_lsb;
                ((sample + noise) * quant_steps_f).round() / quant_steps_f
            }
            DitherType::HighPassTriangular => {
                let cur = Self::next_random_f64(&mut self.rng_state_left) as f32;
                let noise = (cur - self.hp_prev_left) as f64 * half_lsb;
                self.hp_prev_left = cur;
                ((sample + noise) * quant_steps_f).round() / quant_steps_f
            }
            DitherType::Shibata => {
                let noise = (Self::next_random_f64(&mut self.rng_state_left)
                    + Self::next_random_f64(&mut self.rng_state_left))
                    * half_lsb;
                let pos = self.shibata_err_pos;
                let feedback = self.shibata_feedback(&self.shibata_err_left, pos) as f64;
                let shaped = sample + noise - feedback;
                let q = (shaped * quant_steps_f).round() / quant_steps_f;
                self.shibata_err_left[pos] = (q - shaped) as f32;
                self.shibata_err_pos = (pos + 1) % 9;
                q.clamp(-1.0, 1.0)
            }
            DitherType::NoiseShaped => {
                let noise = (Self::next_random_f64(&mut self.rng_state_left)
                    + Self::next_random_f64(&mut self.rng_state_left))
                    * half_lsb;
                let shaped = sample + noise - (self.shape_left as f64) * 0.5;
                let q = (shaped * quant_steps_f).round() / quant_steps_f;
                self.shape_left = (q - shaped + (self.shape_left as f64) * 0.5) as f32;
                q.clamp(-1.0, 1.0)
            }
            DitherType::NoiseShaped16
            | DitherType::NoiseShaped20
            | DitherType::NoiseShaped24
            | DitherType::Ns5
            | DitherType::Ns9
            | DitherType::Lns15 => {
                debug_assert!(self.is_high_order());
                let noise = (Self::next_random_f64(&mut self.rng_state_left)
                    + Self::next_random_f64(&mut self.rng_state_left))
                    * half_lsb;
                let pos = self.shaper_err_pos;
                let feedback =
                    Self::shaper_feedback(self.shaper_coeffs, &self.shaper_err_left, pos) as f64;
                let shaped = sample + noise - feedback;
                let q = (shaped * quant_steps_f).round() / quant_steps_f;
                self.shaper_err_left[pos] = (q - shaped) as f32;
                self.shaper_err_pos = (pos + 1) % 32;
                q.clamp(-1.0, 1.0)
            }
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Whether dithering is currently active (enabled, non-float output, bit depth < 32).
    pub fn is_enabled(&self) -> bool {
        self.is_active()
    }

    pub fn bit_depth(&self) -> u32 {
        self.bit_depth
    }
    pub fn dither_type(&self) -> DitherType {
        self.dither_type
    }

    pub fn reset(&mut self) {
        self.shape_left = 0.0;
        self.shape_right = 0.0;
        self.hp_prev_left = 0.0;
        self.hp_prev_right = 0.0;
        self.shibata_err_left = [0.0; 9];
        self.shibata_err_right = [0.0; 9];
        self.shibata_err_pos = 0;
        self.shaper_err_left = [0.0; 32];
        self.shaper_err_right = [0.0; 32];
        self.shaper_err_pos = 0;
        self.rng_state_left = Self::random_seed();
        self.rng_state_right = Self::random_seed().wrapping_add(0xDEADBEEF_12345678);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dither_output_bounded() {
        let mut dither = Dither::new(DitherType::Triangular, 16);
        for _ in 0..10000 {
            let (l, r) = dither.process(0.5, -0.5);
            assert!(l.abs() <= 1.0);
            assert!(r.abs() <= 1.0);
        }
    }

    #[test]
    fn test_no_dither_at_high_bit_depth() {
        let mut dither = Dither::new(DitherType::Triangular, 32);
        let (l, r) = dither.process(0.5, 0.5);
        assert!((l - 0.5).abs() < 1e-5);
        assert!((r - 0.5).abs() < 1e-5);
    }

    #[test]
    fn test_tpdf_statistics() {
        let mut dither = Dither::new(DitherType::Triangular, 16);
        let n = 100000;
        let mut sum = 0.0;
        for _ in 0..n {
            let (l, _) = dither.process(0.0, 0.0);
            sum += l;
        }
        let mean = sum / n as f32;
        assert!(
            mean.abs() < 0.001,
            "TPDF mean should be near zero, got {}",
            mean
        );
    }

    #[test]
    fn test_noise_shaped_modes_bounded_and_stable() {
        for &(mode, depth) in &[
            (DitherType::NoiseShaped16, 16),
            (DitherType::NoiseShaped20, 20),
            (DitherType::NoiseShaped24, 24),
        ] {
            let mut dither = Dither::new(mode, depth);
            for _ in 0..10_000 {
                let (l, r) = dither.process(0.2, -0.2);
                assert!(l.abs() <= 1.0, "{:?} output must be bounded", mode);
                assert!(r.abs() <= 1.0, "{:?} output must be bounded", mode);
                assert!(!l.is_nan(), "{:?} must not produce NaN", mode);
                assert!(!r.is_nan(), "{:?} must not produce NaN", mode);
            }
        }
    }

    /// Shibata's coefficients are rate-dependent, so a processor built for
    /// 44.1 kHz must not be handed a 48 kHz stream.
    ///
    /// This compares the coefficient tables directly rather than the output,
    /// because every `Dither` seeds its own PRNG from entropy: two instances
    /// fed the same input produce different samples for that reason alone, so
    /// an output comparison cannot distinguish "different coefficients" from
    /// "different noise" — and would pass on the latter. (An earlier version
    /// of this test did exactly that.)
    #[test]
    fn shibata_coefficients_depend_on_the_sample_rate() {
        let at_44k = Dither::with_sample_rate(DitherType::Shibata, 16, 44_100);
        let at_48k = Dither::with_sample_rate(DitherType::Shibata, 16, 48_000);

        assert_ne!(
            at_44k.shibata_coeffs, at_48k.shibata_coeffs,
            "44.1 kHz and 48 kHz must select different Shibata coefficient sets"
        );
        assert_eq!(at_44k.shibata_coeffs, SHIBATA_COEFFS_44100);
        assert_eq!(at_48k.shibata_coeffs, SHIBATA_COEFFS_48000);

        // The selection has exactly two outcomes keyed on one threshold, so
        // every rate must land on one of the two tables and the threshold must
        // sit where the table names imply.
        for (rate, expected) in [
            (8_000u32, SHIBATA_COEFFS_44100),
            (32_000, SHIBATA_COEFFS_44100),
            (44_100, SHIBATA_COEFFS_44100),
            (46_000, SHIBATA_COEFFS_44100),
            (46_001, SHIBATA_COEFFS_48000),
            (48_000, SHIBATA_COEFFS_48000),
            (88_200, SHIBATA_COEFFS_48000),
            (96_000, SHIBATA_COEFFS_48000),
            (192_000, SHIBATA_COEFFS_48000),
        ] {
            assert_eq!(
                Dither::with_sample_rate(DitherType::Shibata, 16, rate).shibata_coeffs,
                expected,
                "{rate} Hz selected the wrong Shibata coefficient set"
            );
        }

        // A 96 kHz stream shares the 48 kHz table. That is a real limitation
        // and it is now stated rather than implied: Shibata published designs
        // for two rates, and every rate above the threshold gets the 48 kHz
        // F-weighting rather than one derived for its own band.
        assert_eq!(
            Dither::with_sample_rate(DitherType::Shibata, 16, 192_000).shibata_coeffs,
            SHIBATA_COEFFS_48000,
            "the high-rate table is the 48 kHz design; see the module note"
        );

        // The design selection reports *which* published design a rate
        // lands on, and that is exactly the predicate the off-design warning
        // uses: a rate is either one of the two published rates or it is
        // reusing the nearer table.
        for (rate, expected_design) in [
            (8_000u32, 44_100u32),
            (32_000, 44_100),
            (44_100, 44_100),
            (46_000, 44_100),
            (46_001, 48_000),
            (48_000, 48_000),
            (88_200, 48_000),
            (96_000, 48_000),
            (192_000, 48_000),
        ] {
            assert_eq!(
                shibata_design_rate(rate),
                expected_design,
                "{rate} Hz must reuse the {expected_design} Hz Shibata design"
            );
            assert_eq!(
                Dither::with_sample_rate(DitherType::Shibata, 16, rate).shibata_coeffs,
                shibata_coefficients_for(expected_design),
                "{rate} Hz coefficients must come from the {expected_design} Hz design"
            );
        }
    }

    /// The converter is the quantisation boundary, so it is the thing that has
    /// to carry the rate. Assert the rate reaches the dither through the
    /// converter rather than only through a direct `Dither` construction — this
    /// is the path every backend uses.
    #[test]
    fn format_converter_carries_the_rate_into_its_dither() {
        use crate::output::format_converter::{AudioFormatConverter, TargetFormat};

        let at_44k =
            AudioFormatConverter::new_at_rate(TargetFormat::I16, DitherType::Shibata, 44_100);
        let at_48k =
            AudioFormatConverter::new_at_rate(TargetFormat::I16, DitherType::Shibata, 48_000);
        let at_96k =
            AudioFormatConverter::new_at_rate(TargetFormat::I16, DitherType::Shibata, 96_000);

        assert_eq!(at_44k.sample_rate(), 44_100);
        assert_eq!(at_48k.sample_rate(), 48_000);
        assert_eq!(at_96k.sample_rate(), 96_000);

        // And the legacy constructor still resolves to 44.1 kHz, which is why
        // every backend now passes the negotiated rate explicitly.
        let legacy = AudioFormatConverter::new(TargetFormat::I16, DitherType::Shibata);
        assert_eq!(legacy.sample_rate(), 44_100);
    }

    #[test]
    fn test_dither_block_equivalence() {
        let mut dither_sample = Dither::new(DitherType::Triangular, 16);
        dither_sample.set_random_seed(12345);
        let mut dither_block = Dither::new(DitherType::Triangular, 16);
        dither_block.set_random_seed(12345);

        let n = 256;
        let mut sample_l = vec![0.12345f32; n];
        let mut sample_r = vec![-0.6789f32; n];
        for i in 0..n {
            let (l, r) = dither_sample.process(sample_l[i], sample_r[i]);
            sample_l[i] = l;
            sample_r[i] = r;
        }

        let mut block_l = vec![0.12345f32; n];
        let mut block_r = vec![-0.6789f32; n];
        dither_block.process_block(&mut block_l, &mut block_r);

        for i in 0..n {
            assert_eq!(sample_l[i], block_l[i], "mismatch at sample {i} for left");
            assert_eq!(sample_r[i], block_r[i], "mismatch at sample {i} for right");
        }
    }

    /// Magnitude of the realised noise transfer function at normalised
    /// frequency `x` (0 = DC, 1 = Nyquist).
    fn ntf_magnitude(coeffs: &[f32], x: f64) -> f64 {
        use std::f64::consts::PI;
        let mut re = 1.0f64;
        let mut im = 0.0f64;
        for (n, &c) in coeffs.iter().enumerate() {
            let phase = PI * x * (n + 1) as f64;
            re -= c as f64 * phase.cos();
            im += c as f64 * phase.sin();
        }
        re.hypot(im)
    }

    fn ntf_stats(coeffs: &[f32], rate: u32) -> (f64, f64, f64) {
        let dc = ntf_magnitude(coeffs, 0.0);
        let one_k = ntf_magnitude(coeffs, 1_000.0 / (rate as f64 / 2.0));
        let mut max = 0.0f64;
        for k in 0..=4_000 {
            max = max.max(ntf_magnitude(coeffs, k as f64 / 4_000.0));
        }
        (dc, one_k, max)
    }

    #[test]
    fn high_order_shapers_have_the_measured_noise_transfer_shape() {
        // The DC and 1 kHz figures are the design goal; the maximum is the
        // stability-relevant high-frequency boost. These bounds are wide
        // enough for the f32 tables but tight enough to catch a swapped or
        // truncated coefficient set. The 1 kHz bound includes the unavoidable
        // one-sample error-feedback delay: −16.3 dB is the floor with this
        // structure, so the test does not demand a physically impossible
        // deeper notch.
        for (family, max_gain_db) in [
            (DitherType::Ns5, 10.0),
            (DitherType::Ns9, 11.0),
            (DitherType::Lns15, 10.0),
        ] {
            for rate in SHAPER_DESIGN_RATES {
                let coeffs = high_order_coefficients(family, rate)
                    .unwrap_or_else(|| panic!("{} at {rate} has no table", family.as_str()));
                let (dc, one_k, max) = ntf_stats(coeffs, rate);
                assert!(
                    dc < 0.08,
                    "{} at {rate}: DC NTF {dc:.4} is not suppressed",
                    family.as_str()
                );
                assert!(
                    one_k < 0.17,
                    "{} at {rate}: 1 kHz NTF {one_k:.4} is not suppressed",
                    family.as_str()
                );
                let max_db = 20.0 * max.log10();
                assert!(
                    max_db <= max_gain_db,
                    "{} at {rate}: {max_db:.1} dB boost exceeds {max_gain_db} dB",
                    family.as_str()
                );
            }
        }
    }

    #[test]
    fn every_design_rate_selects_its_own_table_and_nearest_fallback_is_explicit() {
        for (family, tables) in [
            (
                DitherType::Ns5,
                [
                    &NS5_44100[..],
                    &NS5_48000[..],
                    &NS5_88200[..],
                    &NS5_96000[..],
                    &NS5_176400[..],
                    &NS5_192000[..],
                ],
            ),
            (
                DitherType::Ns9,
                [
                    &NS9_44100[..],
                    &NS9_48000[..],
                    &NS9_88200[..],
                    &NS9_96000[..],
                    &NS9_176400[..],
                    &NS9_192000[..],
                ],
            ),
            (
                DitherType::Lns15,
                [
                    &LNS15_44100[..],
                    &LNS15_48000[..],
                    &LNS15_88200[..],
                    &LNS15_96000[..],
                    &LNS15_176400[..],
                    &LNS15_192000[..],
                ],
            ),
        ] {
            for (rate, table) in SHAPER_DESIGN_RATES.iter().zip(tables) {
                assert_eq!(
                    high_order_coefficients(family, *rate),
                    Some(table),
                    "{} at {rate} selected the wrong table",
                    family.as_str()
                );
            }
        }
        assert_eq!(nearest_design_rate(44_050, SHAPER_DESIGN_RATES), 44_100);
        assert_eq!(nearest_design_rate(50_000, SHAPER_DESIGN_RATES), 48_000);
        assert_eq!(nearest_design_rate(100_000, SHAPER_DESIGN_RATES), 96_000);
        assert_eq!(nearest_design_rate(200_000, SHAPER_DESIGN_RATES), 192_000);
    }

    #[test]
    fn high_order_shapers_are_bounded_and_stable() {
        for dither_type in [DitherType::Ns5, DitherType::Ns9, DitherType::Lns15] {
            for depth in [16u32, 24] {
                for rate in [44_100u32, 96_000] {
                    let mut dither = Dither::with_sample_rate(dither_type, depth, rate);
                    dither.set_random_seed(0x1234_5678_9ABC_DEF0);
                    for i in 0..100_000 {
                        let phase = i as f64 / rate as f64;
                        let sample = (2.0 * std::f64::consts::PI * 997.0 * phase).sin() * 0.2;
                        let (l, r) = dither.process(sample as f32, -sample as f32);
                        assert!(
                            l.is_finite() && r.is_finite(),
                            "{} produced a non-finite sample",
                            dither_type.as_str()
                        );
                        assert!(
                            l.abs() <= 1.0 && r.abs() <= 1.0,
                            "{} produced an out-of-range sample",
                            dither_type.as_str()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn high_order_block_and_sample_paths_are_identical() {
        for dither_type in [
            DitherType::Ns5,
            DitherType::Ns9,
            DitherType::Lns15,
            DitherType::NoiseShaped16,
            DitherType::NoiseShaped20,
            DitherType::NoiseShaped24,
        ] {
            let mut sample = Dither::with_sample_rate(dither_type, 16, 48_000);
            sample.set_random_seed(0xDEAD_BEEF_1234_5678);
            let mut block = Dither::with_sample_rate(dither_type, 16, 48_000);
            block.set_random_seed(0xDEAD_BEEF_1234_5678);

            let n = 257;
            let mut sl = vec![0.12345f32; n];
            let mut sr = vec![-0.6789f32; n];
            for i in 0..n {
                let (l, r) = sample.process(sl[i], sr[i]);
                sl[i] = l;
                sr[i] = r;
            }
            let mut bl = vec![0.12345f32; n];
            let mut br = vec![-0.6789f32; n];
            block.process_block(&mut bl, &mut br);
            assert_eq!(sl, bl, "{} left block mismatch", dither_type.as_str());
            assert_eq!(sr, br, "{} right block mismatch", dither_type.as_str());
        }
    }

    #[test]
    fn float_output_bypass_survives_every_noise_shaper() {
        for shaper in config::NoiseShaper::ALL {
            let mut dither =
                Dither::with_sample_rate(DitherType::from_noise_shaper(shaper), 24, 96_000);
            dither.set_output_is_float(true);
            assert!(!dither.is_enabled());
            let (l, r) = dither.process(0.123_456, -0.654_321);
            assert_eq!((l, r), (0.123_456, -0.654_321));
            let mut block_l = [0.25f32; 8];
            let mut block_r = [-0.25f32; 8];
            dither.process_block(&mut block_l, &mut block_r);
            assert_eq!(block_l, [0.25f32; 8]);
            assert_eq!(block_r, [-0.25f32; 8]);
        }
    }

    #[test]
    fn configuration_enum_maps_to_the_implementation_type() {
        assert_eq!(
            DitherType::from_noise_shaper(config::NoiseShaper::Tpdf),
            DitherType::Triangular
        );
        assert_eq!(
            DitherType::from_noise_shaper(config::NoiseShaper::Shibata),
            DitherType::Shibata
        );
        assert_eq!(
            DitherType::from_noise_shaper(config::NoiseShaper::Ns5),
            DitherType::Ns5
        );
        assert_eq!(
            DitherType::from_noise_shaper(config::NoiseShaper::Ns9),
            DitherType::Ns9
        );
        assert_eq!(
            DitherType::from_noise_shaper(config::NoiseShaper::Lns15),
            DitherType::Lns15
        );
        for shaper in config::NoiseShaper::ALL {
            assert!(!shaper.as_str().is_empty());
        }
    }
}
