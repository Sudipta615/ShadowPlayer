//! The parametric-band model used by AutoEQ targets and chain presets.

use engine::dsp::equalizer::EqFilterType;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParametricBand {
    pub freq: f32,
    pub gain_db: f32,
    pub q: f32,
    pub filter_type: EqFilterType,
    pub enabled: bool,
}

impl Default for ParametricBand {
    fn default() -> Self {
        Self {
            freq: 1000.0,
            gain_db: 0.0,
            q: 0.707,
            filter_type: EqFilterType::Peaking,
            enabled: true,
        }
    }
}
