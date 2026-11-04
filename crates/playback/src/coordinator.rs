use audio_session::AudioSession;
use database::{Database, HistoryRepo, QueueRepo, ScrobbleRepo, TrackRepo};
use domain::models::{RepeatModeChoice, Track};
use std::path::PathBuf;
use std::sync::Arc;

/// A track counts as "played" once this many seconds have been listened to (or
/// the whole track, whichever is shorter). Skips before this do not register.
const PLAY_THRESHOLD_SECS: f32 = 20.0;

/// A play counts as a scrobble at thirty seconds, or half the track,
/// whichever comes first.
///
/// This is the rule the scrobbling services use, and it is deliberately
/// stricter than [`PLAY_THRESHOLD_SECS`]: a play that was abandoned after
/// twenty seconds is a play, but it is not a scrobble.
const SCROBBLE_MIN_SECS: f32 = 30.0;

/// The fraction of a track that has to be heard for the play to scrobble.
const SCROBBLE_FRACTION: f32 = 0.5;

/// The largest position advance between two polls that still counts as
/// continuous playback. Anything bigger is a seek and must not inflate the
/// listening total; a normal frame is two orders of magnitude smaller.
const MAX_CREDITED_STEP_SECS: f32 = 2.0;

/// Pressing *previous* this far into a track restarts it instead of moving on.
const RESTART_SEEK_THRESHOLD_SECS: f32 = 3.0;

/// The `.cue` names to look for beside an album image, after one named for the
/// image itself. These are the two conventions the rippers actually use.
const SIBLING_CUE_NAMES: [&str; 2] = ["album.cue", "CDImage.cue"];

/// An input endpoint as the measurement wizard sees it, decoupled from the
/// engine's type so callers of the coordinator do not need the engine crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeasurementInputDevice {
    pub name: String,
    pub is_default: bool,
    pub channels: Vec<u16>,
    pub sample_rates: Vec<u32>,
    pub measurement_capable: bool,
}

/// Engine events relevant to the room-correction measurement wizard.
#[derive(Debug, Clone, PartialEq)]
pub enum RoomMeasurementEvent {
    /// Reply to [`PlaybackCoordinator::enumerate_measurement_inputs`].
    InputDevices(Vec<MeasurementInputDevice>),
    /// A named stage in the sweep/capture/deconvolution sequence.
    Progress(String),
    /// The derived correction is now live.
    Complete {
        path: PathBuf,
        snr_db: f32,
    },
    Failed(String),
    Cancelled,
}

pub struct PlaybackCoordinator {
    session: Arc<AudioSession>,
    _db: Database,
    track_repo: TrackRepo,
    history_repo: HistoryRepo,
    scrobble_repo: ScrobbleRepo,
    queue_repo: QueueRepo,
    queue: Vec<Track>,
    /// Original insertion order, kept while shuffle is active so we can restore
    /// it when shuffle is turned off.
    unshuffled_queue: Option<Vec<Track>>,
    current_index: Option<usize>,
    current_track: Option<Track>,
    shuffle: bool,
    repeat_mode: RepeatModeChoice,

    // Listening-accounting for the recently/most played rules.
    listen_secs: f32,
    last_position: f32,
    play_registered: bool,

    /// Engine and audio-device problems worth showing the user, collected by
    /// [`PlaybackCoordinator::poll_events`] and drained by the shell. These
    /// used to be dropped on the floor (`_ => {}`), so a decode failure or a
    /// disconnected DAC was invisible.
    engine_notices: Vec<String>,

    /// Measurement events, kept separate from notices so the wizard can show a
    /// progress stage without also raising a toast for it.
    room_measurement_events: Vec<RoomMeasurementEvent>,

    /// The last output-device arrival or negotiated change. The coordinator
    /// owns the only drain of the bounded output-event channel (it must, or
    /// the channel fills), so it republishes the arrival here for the shell's
    /// device-bound EQ presets.
    pending_output_device: Option<String>,
}

mod policy;
mod queue;

fn credited_listen_secs(last_position: f32, pos: f32) -> f32 {
    let delta = pos - last_position;
    if delta > 0.0 && delta <= MAX_CREDITED_STEP_SECS {
        delta
    } else {
        0.0
    }
}

pub(crate) fn sibling_cue_candidates(real_path: &std::path::Path) -> [std::path::PathBuf; 3] {
    let stem = real_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let parent = real_path.parent().unwrap_or(std::path::Path::new("."));
    [
        parent.join(format!("{}.cue", stem)),
        parent.join(SIBLING_CUE_NAMES[0]),
        parent.join(SIBLING_CUE_NAMES[1]),
    ]
}
#[cfg(test)]
mod coordinator_unit_tests {
    use super::credited_listen_secs;

    /// A forward jump in the playhead is a seek, not listening.
    #[test]
    fn only_contiguous_playback_counts_as_listening() {
        // A normal UI frame.
        assert!((credited_listen_secs(10.0, 10.016) - 0.016).abs() < 1e-6);
        // A stalled frame is still playback.
        assert!((credited_listen_secs(10.0, 11.5) - 1.5).abs() < 1e-6);
        // Dragging the progress bar near the end is not.
        assert_eq!(credited_listen_secs(10.0, 175.0), 0.0);
        // Neither is seeking backwards or restarting a track.
        assert_eq!(credited_listen_secs(100.0, 5.0), 0.0);
        assert_eq!(credited_listen_secs(0.0, 0.0), 0.0);
    }
}
