//! Output-domain final safety limiter for [`DspGraph`].

use super::*;

impl DspGraph {
    pub fn process_final_limiter(&mut self, left: f32, right: f32) -> (f32, f32) {
        if self.dop_bypass || self.bit_perfect {
            return (left, right);
        }
        let (left, right) = self.ultrasonic_mut().stage.process(left, right);
        self.limiter_mut().limiter.process(left, right)
    }

    pub fn process_final_limiter_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        if self.dop_bypass || self.bit_perfect {
            return;
        }
        self.ultrasonic_mut()
            .stage
            .process_block(&mut [&mut left[..], &mut right[..]]);
        self.limiter_mut().limiter.process_block(left, right);
    }

    pub fn flush_final_limiter(&mut self) -> Vec<(f32, f32)> {
        if self.dop_bypass || self.bit_perfect {
            Vec::new()
        } else {
            self.limiter_mut().limiter.flush()
        }
    }

    pub fn process_final_limiter_multichannel(&mut self, interleaved: &mut [f32], channels: usize) {
        if self.dop_bypass || self.bit_perfect {
            return;
        }
        self.ultrasonic_mut()
            .stage
            .process_interleaved(interleaved, channels);
        self.limiter_mut()
            .limiter
            .process_block_multichannel(interleaved, channels);
    }

    /// Change the output-domain ultrasonic filter in place.
    ///
    /// The stage is output-domain, so this is the only path that reaches it;
    /// an off selection is a hard bypass and resets the filter state.
    pub fn set_ultrasonic_filter(&mut self, filter: config::UltrasonicFilter) {
        self.ultrasonic_mut().stage.set_filter(filter);
    }

    pub fn flush_final_limiter_multichannel(&mut self, channels: usize) -> Vec<f32> {
        if self.dop_bypass || self.bit_perfect {
            Vec::new()
        } else {
            self.limiter_mut().limiter.flush_multichannel(channels)
        }
    }
}
