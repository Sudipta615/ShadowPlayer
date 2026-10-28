use crate::dsp::{
    graph2::prod::arena::node::DspNode,
    pipeline::{DspStageCapability, StageChannelSupport, StagePrecision},
    ultrasonic::UltrasonicStage,
};
use config::UltrasonicFilter;

/// Output ultrasonic low-pass node (output domain, before the limiter).
///
/// Disabled or inapplicable at the current rate = bit-exact passthrough; the
/// underlying [`UltrasonicStage`] owns the design and the realtime budget.
pub struct UltrasonicNode {
    pub stage: UltrasonicStage,
    selection: UltrasonicFilter,
    sample_rate: f32,
}

impl UltrasonicNode {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            stage: UltrasonicStage::new(UltrasonicFilter::Off, sample_rate),
            selection: UltrasonicFilter::Off,
            sample_rate,
        }
    }

    /// The requested selection (which may be inactive at this rate).
    pub fn filter(&self) -> UltrasonicFilter {
        self.selection
    }

    /// The realized corner frequency, or 0 when inactive.
    pub fn corner_hz(&self) -> f32 {
        self.stage.corner_hz()
    }

    pub fn set_filter(&mut self, filter: UltrasonicFilter) {
        self.selection = filter;
        self.stage.configure(filter, self.sample_rate);
    }
}

impl DspNode for UltrasonicNode {
    fn capability(&self) -> DspStageCapability {
        DspStageCapability {
            name: "ultrasonic",
            channel_support: StageChannelSupport::AllChannels,
            position: "output domain",
            stateful: true,
            realtime_safe: true,
            bit_perfect_compatible: false,
            sample_rate_sensitive: true,
            precision: StagePrecision::F32,
        }
    }

    fn is_active(&self) -> bool {
        self.stage.is_active()
    }

    fn reset(&mut self) {
        self.stage.reset();
    }

    fn prepare(&mut self, sample_rate: f32, _max_channels: usize) {
        self.sample_rate = sample_rate;
        self.stage.configure(self.selection, sample_rate);
    }

    fn process_block_f32(&mut self, planes: &mut [&mut [f32]]) {
        self.stage.process_block(planes);
    }

    fn process_block_f64(&mut self, planes: &mut [&mut [f64]]) {
        self.stage.process_block_f64(planes);
    }
}
