//! Configurable multi-band parametric equalizer.

mod core;
pub mod dynamic;
pub mod headroom;
pub mod svf;
#[cfg(test)]
mod tests;
mod types;

pub use core::ParametricEq;
pub use dynamic::{DynamicEq, DynamicEqBand, DynamicEqBandParams, MAX_DYNAMIC_EQ_BANDS};
pub use headroom::max_curve_gain_db;
pub use svf::{SvfFilter, SvfMode};
pub use types::{EqBandParams, EqFilterType, MAX_EQ_BANDS, NUM_EQ_BANDS};
