//! When the frame loop should wake again.
//!
//! `AGENTS.md` §2.1 makes 0% idle CPU a product guarantee, and §2.2 caps
//! playback at 60 fps. Both come down to one decision per frame: request a
//! repaint for an active reason, or request none and sleep until an event.
//!
//! The decision used to be an if-ladder inside the poll function, where the
//! idle case was correct only because nobody had added a branch. It is a pure
//! function now, so the idle case is covered by tests instead of by review.

use std::time::Duration;

/// The frame rate playback and scanning need (30 fps, with headroom under the
/// 60 fps cap; the transport position does not change faster than this).
pub const ACTIVE_FRAME: Duration = Duration::from_millis(33);
/// While the window is minimized nothing is visible: keep the timers that
/// matter correct and stop drawing.
pub const MINIMIZED_FRAME: Duration = Duration::from_millis(500);
/// A progress bar moves slowly; a quarter second is plenty.
pub const SLOW_PROGRESS: Duration = Duration::from_millis(250);
/// Waiting on a picker or an animation that is settling.
pub const POLL_FRAME: Duration = Duration::from_millis(120);
/// A countdown or a debounce that only needs a visible tick.
pub const IDLE_TICK: Duration = Duration::from_millis(500);

/// Everything the frame loop knows about work in progress.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RepaintSignals {
    /// The transport is running.
    pub playing: bool,
    /// The window is minimized.
    pub minimized: bool,
    /// A library scan is running.
    pub scanning: bool,
    /// A loudness analysis batch is running.
    pub loudness_running: bool,
    /// A native file picker or its worker is open.
    pub import_active: bool,
    /// The sleep timer is counting down.
    pub sleep_timer_armed: bool,
    /// A watched folder changed and its rescan is still debouncing.
    pub watch_pending: bool,
    /// Scan roots are queued behind the running one.
    pub scan_queued: bool,
    /// A notification is fading out.
    pub toasts_visible: bool,
    /// Lyrics or a waveform are being read on a worker.
    pub background_read_pending: bool,
}

impl RepaintSignals {
    /// True when nothing is animating or progressing.
    #[cfg(test)]
    pub fn is_idle(&self) -> bool {
        !(self.playing
            || self.scanning
            || self.loudness_running
            || self.import_active
            || self.sleep_timer_armed
            || self.watch_pending
            || self.scan_queued
            || self.toasts_visible
            || self.background_read_pending)
    }
}

/// How long to wait before the next frame, or `None` for "do not wake me".
///
/// `None` is the idle contract: with playback stopped, no scan, no timer and
/// nothing on screen, the app must be woken by the next event (an input event,
/// a finished worker) and not by a heartbeat.
pub fn wake_after(signals: RepaintSignals) -> Option<Duration> {
    if signals.minimized {
        return (signals.playing
            || signals.scanning
            || signals.loudness_running
            || signals.import_active)
            .then_some(MINIMIZED_FRAME);
    }
    if signals.playing || signals.scanning {
        return Some(ACTIVE_FRAME);
    }
    if signals.loudness_running {
        return Some(SLOW_PROGRESS);
    }
    if signals.import_active {
        return Some(POLL_FRAME);
    }
    if signals.sleep_timer_armed {
        return Some(IDLE_TICK);
    }
    if signals.watch_pending || signals.scan_queued {
        return Some(IDLE_TICK);
    }
    if signals.toasts_visible || signals.background_read_pending {
        return Some(POLL_FRAME);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signals() -> RepaintSignals {
        RepaintSignals::default()
    }

    #[test]
    fn idle_requests_no_frames() {
        // The 0% CPU contract: paused, nothing scanning, nothing animating.
        assert_eq!(wake_after(signals()), None);
        assert!(signals().is_idle());
    }

    #[test]
    fn a_minimized_idle_window_requests_no_frames() {
        assert_eq!(
            wake_after(RepaintSignals {
                minimized: true,
                ..signals()
            }),
            None
        );
    }

    #[test]
    fn every_active_signal_makes_the_loop_wake() {
        let active = [
            RepaintSignals {
                playing: true,
                ..signals()
            },
            RepaintSignals {
                scanning: true,
                ..signals()
            },
            RepaintSignals {
                loudness_running: true,
                ..signals()
            },
            RepaintSignals {
                import_active: true,
                ..signals()
            },
            RepaintSignals {
                sleep_timer_armed: true,
                ..signals()
            },
            RepaintSignals {
                watch_pending: true,
                ..signals()
            },
            RepaintSignals {
                scan_queued: true,
                ..signals()
            },
            RepaintSignals {
                toasts_visible: true,
                ..signals()
            },
            RepaintSignals {
                background_read_pending: true,
                ..signals()
            },
        ];
        for signal in active {
            assert!(
                wake_after(signal).is_some(),
                "an active signal must wake the loop: {signal:?}"
            );
            assert!(!signal.is_idle());
        }
    }

    #[test]
    fn playback_is_capped_below_sixty_fps() {
        let delay = wake_after(RepaintSignals {
            playing: true,
            ..signals()
        })
        .expect("playback must repaint");
        assert_eq!(delay, ACTIVE_FRAME);
        assert!(delay >= Duration::from_millis(16), "must not exceed 60 fps");
    }

    #[test]
    fn a_minimized_playback_window_is_throttled() {
        let delay = wake_after(RepaintSignals {
            minimized: true,
            playing: true,
            ..signals()
        });
        assert_eq!(delay, Some(MINIMIZED_FRAME));
    }

    #[test]
    fn sleeping_through_a_toast_only_waits_while_the_toast_is_visible() {
        assert_eq!(
            wake_after(RepaintSignals {
                toasts_visible: true,
                ..signals()
            }),
            Some(POLL_FRAME)
        );
        assert_eq!(wake_after(signals()), None);
    }
}
