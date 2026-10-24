//! Transport, playlist navigation, and playback status.
//!
//! These are the controls the UI uses on every frame; they forward to the
//! engine handle and never block.

use super::{AudioSession, BitPerfectCause, PlaybackInfo, PlaybackState};
use engine::engine::cue_split::PregapPolicy;
use engine::playlist::RepeatMode;
use engine::AudioSource;
use std::path::Path;

impl AudioSession {
    pub fn play(&self) {
        self.handle.play();
    }

    pub fn pause(&self) {
        self.handle.pause();
    }

    pub fn stop(&self) {
        self.handle.stop();
    }

    pub fn seek(&self, position_secs: f32) {
        self.handle.seek(position_secs);
    }

    pub fn set_volume(&self, linear: f32) {
        self.handle.set_volume(linear.clamp(0.0, 1.0));
    }

    pub fn set_volume_db(&self, db: f32) {
        self.handle.set_volume_db(db.clamp(-60.0, 0.0));
    }

    pub fn open_file(&self, path: impl AsRef<Path>) {
        self.handle.open_file(path.as_ref().to_path_buf());
    }

    pub fn open_source(&self, source: AudioSource) {
        self.handle.open(source);
    }

    pub fn prepare_next_file(&self, path: impl AsRef<Path>) {
        self.handle.prepare_next_file(path.as_ref().to_path_buf());
    }

    pub fn prepare_next_source(&self, source: AudioSource) {
        self.handle.prepare_next(source);
    }

    pub fn enqueue_file(&self, path: impl AsRef<Path>) {
        self.handle.enqueue_file(path.as_ref().to_path_buf());
    }

    pub fn enqueue_source(&self, source: AudioSource) {
        self.handle.enqueue(source);
    }

    pub fn enqueue_cue_sheet(&self, path: impl AsRef<Path>, pregap: PregapPolicy) {
        self.handle
            .enqueue_cue_sheet(path.as_ref().to_path_buf(), pregap);
    }

    /// Open an audio file and seek directly to the CUE track's start offset
    pub fn open_cue_track(
        &self,
        audio_path: impl AsRef<Path>,
        cue_file: impl AsRef<Path>,
        track_number: u32,
    ) -> bool {
        if let Ok(sheet) = engine::decode::cue::CueSheet::parse_file(cue_file.as_ref()) {
            if let Some(ct) = sheet.tracks.iter().find(|t| t.number == track_number) {
                self.open_file(audio_path.as_ref());
                self.seek(ct.start_time_seconds() as f32);
                return true;
            }
        }
        false
    }

    pub fn load_playlist_file(&self, path: impl AsRef<Path>) {
        self.handle.load_playlist_file(path.as_ref().to_path_buf());
    }

    pub fn save_playlist_file(&self, path: impl AsRef<Path>) {
        self.handle.save_playlist_file(path.as_ref().to_path_buf());
    }

    pub fn dequeue(&self) {
        self.handle.dequeue();
    }

    pub fn clear_playlist(&self) {
        self.handle.clear_playlist();
    }

    pub fn play_index(&self, index: usize) {
        self.handle.play_index(index);
    }

    pub fn next(&self) {
        self.handle.next();
    }

    pub fn previous(&self) {
        self.handle.previous();
    }

    pub fn set_repeat_mode(&self, mode: RepeatMode) {
        self.handle.set_repeat_mode(mode);
    }

    pub fn set_shuffle(&self, enabled: bool) {
        self.handle.set_shuffle(enabled);
    }

    // ── Lock-Free Telemetry ──────────────────────────────────────────────

    #[inline]
    pub fn playback_info(&self) -> PlaybackInfo {
        self.handle.playback_info()
    }

    #[inline]
    pub fn position_secs(&self) -> f32 {
        self.handle.position_secs()
    }

    /// Audible-position for UI progress bar and synced lyrics.
    #[inline]
    pub fn position_secs_compensated(&self) -> f32 {
        self.handle.position_secs_compensated()
    }

    #[inline]
    pub fn duration_secs(&self) -> f32 {
        self.handle.duration_secs()
    }

    #[inline]
    pub fn is_playing(&self) -> bool {
        self.handle.is_playing()
    }

    #[inline]
    pub fn state(&self) -> PlaybackState {
        self.handle.state()
    }

    #[inline]
    pub fn volume(&self) -> f32 {
        self.handle.volume()
    }

    #[inline]
    pub fn current_source(&self) -> Option<AudioSource> {
        self.handle.current_source()
    }

    #[inline]
    pub fn analyzer_snapshot(&self) -> engine::dsp::AnalyzerSnapshot {
        self.handle.analyzer().snapshot()
    }

    #[inline]
    pub fn meters_snapshot(&self) -> engine::dsp::ProfessionalMeterSnapshot {
        self.handle.meters_snapshot()
    }

    #[inline]
    pub fn bit_perfect(&self) -> bool {
        self.handle.playback_info().bit_perfect
    }

    pub fn bit_perfect_cause(&self) -> Option<BitPerfectCause> {
        let info = self.handle.playback_info();
        if info.bit_perfect {
            None
        } else {
            info.engine_stats.as_ref().and_then(|s| s.bit_perfect_cause)
        }
    }
}
