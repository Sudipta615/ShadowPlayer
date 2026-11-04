//! The shared implementation behind the desktop MPRIS/SMTC service and the
//! headless loopback control socket.
//!
//! The two front ends differ in *where* a request arrives from and in how
//! they react to focus/quit/volume changes, but the meaning of a transport
//! command must not depend on which front end received it. Both call
//! [`apply_mpris_command`] and then apply [`TransportOutcome`] to whatever
//! UI or process machinery they own.

use domain::models::RepeatModeChoice;
use playback::PlaybackCoordinator;

use audio_session::AudioSession;

use super::mpris_service::MprisCommand;

/// The narrowest accepted MPRIS rate.
pub const MIN_MPRIS_RATE: f64 = 0.5;
/// The widest accepted MPRIS rate.
pub const MAX_MPRIS_RATE: f64 = 2.0;

/// What the caller must still do after a command has been applied.
///
/// Volume is reported rather than persisted here because the desktop owns a
/// settings copy (and must clear mute / mark itself dirty), whereas the
/// headless front end has no settings to write.
#[derive(Debug, Default)]
pub struct TransportOutcome {
    /// Clamped linear volume the caller should mirror into its own state.
    pub volume: Option<f32>,
    /// The front end should bring its window to the front.
    pub raise: bool,
    /// The front end should quit.
    pub quit: bool,
}

/// The player state a transport command may act on.
///
/// `coordinator` is present on the desktop and absent in headless mode; the
/// session is always available and carries the same semantics, so a missing
/// coordinator just means "no in-memory queue to permute".
pub struct TransportHost<'a> {
    pub session: &'a AudioSession,
    pub coordinator: Option<&'a mut PlaybackCoordinator>,
}

/// Map the persisted repeat mode to the MPRIS `LoopStatus` spelling.
pub fn loop_status_mpris(mode: RepeatModeChoice) -> &'static str {
    match mode {
        RepeatModeChoice::Off => "None",
        RepeatModeChoice::One => "Track",
        RepeatModeChoice::All => "Playlist",
    }
}

/// Parse an MPRIS `LoopStatus` value; `None` if it is not one of the three.
pub fn loop_status_choice(value: &str) -> Option<RepeatModeChoice> {
    match value.trim().to_ascii_lowercase().as_str() {
        "none" => Some(RepeatModeChoice::Off),
        "track" => Some(RepeatModeChoice::One),
        "playlist" => Some(RepeatModeChoice::All),
        _ => None,
    }
}

/// Clamp an MPRIS rate to the advertised range; a non-finite value resets to 1.0.
pub fn clamp_mpris_rate(rate: f64) -> f64 {
    if !rate.is_finite() {
        return 1.0;
    }
    rate.clamp(MIN_MPRIS_RATE, MAX_MPRIS_RATE)
}

/// Apply one transport command to the session (and the queue, when present).
pub fn apply_mpris_command(cmd: MprisCommand, host: &mut TransportHost<'_>) -> TransportOutcome {
    match cmd {
        MprisCommand::Play => host.session.play(),
        MprisCommand::Pause => host.session.pause(),
        MprisCommand::PlayPause => {
            if let Some(coordinator) = host.coordinator.as_deref_mut() {
                coordinator.toggle_play_pause();
            } else if host.session.is_playing() {
                host.session.pause();
            } else {
                host.session.play();
            }
        }
        MprisCommand::Stop => host.session.stop(),
        MprisCommand::Next => {
            if let Some(coordinator) = host.coordinator.as_deref_mut() {
                coordinator.next();
            } else {
                host.session.next();
            }
        }
        MprisCommand::Previous => {
            if let Some(coordinator) = host.coordinator.as_deref_mut() {
                coordinator.previous();
            } else {
                host.session.previous();
            }
        }
        MprisCommand::SeekRelative(delta) => {
            if !delta.is_finite() {
                return TransportOutcome::default();
            }
            let duration = seek_upper_bound(host);
            let current = host.session.position_secs_compensated() as f64;
            let target = (current + delta).clamp(0.0, duration);
            host.session.seek(target as f32);
        }
        MprisCommand::SetPosition(position) => {
            if !position.is_finite() {
                return TransportOutcome::default();
            }
            let duration = seek_upper_bound(host);
            host.session.seek(position.clamp(0.0, duration) as f32);
        }
        MprisCommand::SetVolume(volume) => {
            if !volume.is_finite() {
                return TransportOutcome::default();
            }
            let volume = volume.clamp(0.0, 1.0);
            host.session.set_volume(volume);
            return TransportOutcome {
                volume: Some(volume),
                ..TransportOutcome::default()
            };
        }
        MprisCommand::SetLoopStatus(value) => match loop_status_choice(&value) {
            Some(mode) => {
                if let Some(coordinator) = host.coordinator.as_deref_mut() {
                    coordinator.set_repeat_mode(mode);
                } else {
                    host.session.set_repeat_mode(match mode {
                        RepeatModeChoice::Off => audio_session::RepeatMode::Off,
                        RepeatModeChoice::One => audio_session::RepeatMode::One,
                        RepeatModeChoice::All => audio_session::RepeatMode::All,
                    });
                }
            }
            None => log::warn!("Ignoring unsupported MPRIS LoopStatus value: {value:?}"),
        },
        MprisCommand::SetShuffle(enabled) => {
            if let Some(coordinator) = host.coordinator.as_deref_mut() {
                coordinator.set_shuffle(enabled);
            } else {
                host.session.set_shuffle(enabled);
            }
        }
        MprisCommand::SetRate(rate) => {
            let rate = clamp_mpris_rate(rate);
            host.session.set_speed(rate as f32);
        }
        MprisCommand::Raise => {
            return TransportOutcome {
                raise: true,
                ..TransportOutcome::default()
            }
        }
        MprisCommand::Quit => {
            return TransportOutcome {
                quit: true,
                ..TransportOutcome::default()
            }
        }
    }
    TransportOutcome::default()
}

/// The largest seek position MPRIS clients may request.
///
/// Falls back to the track's declared length, then to an unbounded upper
/// bound when neither the session nor the queue knows the duration.
fn seek_upper_bound(host: &TransportHost<'_>) -> f64 {
    let mut duration = host.session.duration_secs() as f64;
    if duration <= 0.0 || !duration.is_finite() {
        if let Some(coordinator) = host.coordinator.as_deref() {
            if let Some(track) = coordinator.current_track() {
                duration = track.duration_secs as f64;
            }
        }
    }
    if duration <= 0.0 || !duration.is_finite() {
        f64::MAX
    } else {
        duration
    }
}
