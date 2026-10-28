//! Noise-transfer-function coefficient tables for the PCM→SDM modulators.
//!
//! Each row is a fifth- or seventh-order low-pass NTF synthesized with
//! Schreier's method (`synthesizeNTF`, optimized zeros, `H_inf = 1.5`) and
//! realized in the CRFB (cascade-of-resonators, feedback) structure with a
//! delaying-integrator topology. The coefficients are generated once per
//! rate tier and compiled in: there is no allocation or trigonometry on the
//! audio path.
//!
//! The tiers are the four DSD multiples (64/128/256/512 × the 44.1/48 kHz
//! base). The NTF zeros are placed for an audio band of 20 kHz at the
//! corresponding rate, so the in-band noise floor stays low as the rate
//! rises instead of being designed only for DSD64. This is the "rate
//! adaptive" part of the modulator selection; the topology itself is fixed.

/// One NTF realization: filter order plus the CRFB coefficients.
///
/// `a[i]` is the i-th feedback coefficient (the `B2` column of the
/// state-space form is `-a`), and `g[i]` is the resonator coefficient of the
/// i-th complex-zero pair. The coefficients come from `realizeNTF(ntf,
/// 'CRFB')`; `c` is all ones and `b = a` with a direct input term of one, as
/// the toolbox's unity-STF realization prescribes.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NtfCoefficients {
    pub order: usize,
    pub multiple: u32,
    pub a: &'static [f64],
    pub g: &'static [f64],
}

/// Fifth-order tables, one per DSD multiple.
const FIFTH_ORDER: &[(u32, &[f64], &[f64])] = &[
    (
        64,
        &[
            0.000326564100464,
            0.011920373275240,
            0.033627760733025,
            0.303480309021484,
            0.467944591755677,
        ],
        &[0.000698612615580, 0.001978322017536],
    ),
    (
        128,
        &[
            0.000561622378810,
            0.009833692352520,
            0.048791050654326,
            0.268699018254682,
            0.527021903742028,
        ],
        &[0.000174660780492, 0.000494641671980],
    ),
    (
        256,
        &[
            0.000631185498196,
            0.009123964901018,
            0.053763176177508,
            0.256550297928933,
            0.547924770166522,
        ],
        &[0.000043665671796, 0.000123664241206],
    ),
    (
        512,
        &[
            0.000649353976900,
            0.008932539128329,
            0.055094174734137,
            0.253253375308035,
            0.553615254340600,
        ],
        &[0.000010916447741, 0.000030916299256],
    ),
];

/// Seventh-order tables, one per DSD multiple.
const SEVENTH_ORDER: &[(u32, &[f64], &[f64])] = &[
    (
        64,
        &[
            0.000001425518036,
            0.000132516233772,
            0.000860542045702,
            0.012139932635833,
            0.049491300886746,
            0.262253919463572,
            0.541181899389949,
        ],
        &[0.000396865249507, 0.001324799669202, 0.002170163085373],
    ),
    (
        128,
        &[
            0.000003358485410,
            0.000114933132088,
            0.001162164363068,
            0.011482847910645,
            0.055271839606198,
            0.256896065839292,
            0.550542575237505,
        ],
        &[0.000099218773468, 0.000331227345189, 0.000542614378934],
    ),
    (
        256,
        &[
            0.000003936739766,
            0.000108143321295,
            0.001259876255426,
            0.011185810738984,
            0.057247127084118,
            0.254651034129473,
            0.554192978637513,
        ],
        &[0.000024804847187, 0.000082808550611, 0.000135658195520],
    ),
    (
        512,
        &[
            0.000004088106466,
            0.000106265666694,
            0.001285968607974,
            0.011101451972889,
            0.057781386812780,
            0.254020356317219,
            0.555207697680449,
        ],
        &[0.000006201221411, 0.000020702244798, 0.000033914836434],
    ),
];

/// Select the coefficient table for `order` (5 or 7) at `multiple`
/// (64/128/256/512). An unsupported pair returns the DSD64 table because it
/// is the most conservative design; callers validate the order first.
pub(crate) fn coefficients(order: usize, multiple: u32) -> Option<NtfCoefficients> {
    let table = match order {
        5 => FIFTH_ORDER,
        7 => SEVENTH_ORDER,
        _ => return None,
    };
    let (multiple, a, g) = table
        .iter()
        .find(|(m, _, _)| *m == multiple)
        .copied()
        .unwrap_or(table[0]);
    Some(NtfCoefficients {
        order,
        multiple,
        a,
        g,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_order_and_multiple_has_a_table() {
        for order in [5, 7] {
            for multiple in [64, 128, 256, 512] {
                let c = coefficients(order, multiple).expect("coefficients");
                assert_eq!(c.order, order);
                assert_eq!(c.multiple, multiple);
                assert_eq!(c.a.len(), order);
                assert_eq!(c.g.len(), order / 2);
                assert!(c.a.iter().all(|v| v.is_finite() && *v > 0.0));
                assert!(c.g.iter().all(|v| v.is_finite() && *v > 0.0));
            }
        }
    }

    #[test]
    fn unsupported_multiple_falls_back_to_dsd64() {
        let c = coefficients(5, 99).expect("coefficients");
        assert_eq!(c.multiple, 64);
    }
}
