use engine::dsp::AnalyzerSnapshot;

/// Telemetry state snapshot polled from AudioSession in a lock-free manner
#[derive(Debug, Clone)]
pub struct TelemetryState {
    pub position_secs: f64,
    pub duration_secs: f64,
    pub is_playing: bool,
    pub bit_perfect: bool,
    pub bit_perfect_cause: Option<String>,
    pub spectrum: AnalyzerSnapshot,
    pub meters: Option<engine::dsp::ProfessionalMeterSnapshot>,
}

impl Default for TelemetryState {
    fn default() -> Self {
        Self {
            position_secs: 0.0,
            duration_secs: 0.0,
            is_playing: false,
            bit_perfect: false,
            bit_perfect_cause: None,
            spectrum: AnalyzerSnapshot::default(),
            meters: None,
        }
    }
}
