/// What the sleep timer should do when it fires.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SleepTimerMode {
    /// Pause playback after a duration (stored in `remaining`).
    Duration,
    /// Pause playback when the current track finishes.
    EndOfTrack,
}

/// Active sleep timer.
#[derive(Debug, Clone)]
pub struct SleepTimer {
    pub mode: SleepTimerMode,
    /// Seconds left for [`SleepTimerMode::Duration`].
    pub remaining: f64,
    /// Total duration originally requested (for progress display).
    pub total: f64,
}

impl SleepTimer {
    pub fn after_seconds(secs: f64) -> Self {
        Self {
            mode: SleepTimerMode::Duration,
            remaining: secs,
            total: secs,
        }
    }

    pub fn end_of_track() -> Self {
        Self {
            mode: SleepTimerMode::EndOfTrack,
            remaining: 0.0,
            total: 0.0,
        }
    }

    /// Human-readable label shown on the timer button.
    pub fn label(&self) -> String {
        match self.mode {
            SleepTimerMode::EndOfTrack => "End of track".to_string(),
            SleepTimerMode::Duration => {
                let secs = self.remaining.max(0.0) as u64;
                if secs >= 3600 {
                    format!("{}h {:02}m", secs / 3600, (secs % 3600) / 60)
                } else {
                    format!("{}:{:02}", secs / 60, secs % 60)
                }
            }
        }
    }
}
