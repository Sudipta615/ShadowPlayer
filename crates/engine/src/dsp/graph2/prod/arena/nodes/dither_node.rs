use crate::dsp::{
    dither::{Dither, DitherType},
    graph2::prod::arena::node::DspNode,
    pipeline::{DspStageCapability, StageChannelSupport, StagePrecision},
};

/// Dither and output quantization node.
pub struct DitherNode {
    pub dither: Dither,
    dither_type: DitherType,
}

impl DitherNode {
    pub fn new(sample_rate: f32) -> Self {
        Self::with_type(crate::dsp::dither::configured_dither_type(), sample_rate)
    }

    /// Construct with an explicit dither family. The graph's dither node is
    /// the 16-bit reference node; the production integer boundary is the
    /// output converter, which reads the same configured default.
    pub fn with_type(dither_type: DitherType, sample_rate: f32) -> Self {
        Self {
            dither: Dither::with_sample_rate(dither_type, 16, sample_rate as u32),
            dither_type,
        }
    }
}

impl DspNode for DitherNode {
    fn capability(&self) -> DspStageCapability {
        DspStageCapability {
            name: "dither",
            channel_support: StageChannelSupport::AllChannels,
            position: "output conversion",
            stateful: true,
            realtime_safe: true,
            bit_perfect_compatible: false,
            sample_rate_sensitive: false,
            precision: StagePrecision::Any,
        }
    }

    fn is_active(&self) -> bool {
        self.dither.is_enabled()
    }

    fn reset(&mut self) {
        self.dither.reset();
    }

    fn prepare(&mut self, sample_rate: f32, _max_channels: usize) {
        self.dither = Dither::with_sample_rate(self.dither_type, 16, sample_rate as u32);
    }

    fn process_block_f32(&mut self, planes: &mut [&mut [f32]]) {
        if planes.len() < 2 {
            return;
        }
        let (front, rest) = planes.split_at_mut(1);
        let left = &mut front[0];
        let right = &mut rest[0];
        self.dither.process_block(left, right);
    }

    fn process_block_f64(&mut self, planes: &mut [&mut [f64]]) {
        if planes.len() < 2 {
            return;
        }
        let (front, rest) = planes.split_at_mut(1);
        let left = &mut front[0];
        let right = &mut rest[0];
        self.dither.process_block_f64(left, right);
    }
}
