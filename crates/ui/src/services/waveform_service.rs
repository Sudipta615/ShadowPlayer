//! Waveform peaks for the seek bar.
//!
//! The first time a track is played the audio is decoded once on a worker
//! thread and reduced to a few hundred peak values. Those bytes are cached in
//! the library database, so every later play paints the waveform immediately
//! and never touches the audio file again.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;

use database::{TrackRepo, WaveformRepo};
use domain::ids::TrackId;
use domain::models::Track;

/// How many columns the cached waveform carries. The seek bar is at most
/// about 900 px wide on a large window, so 1200 peaks leaves headroom without
/// making the cache bigger than a kilobyte per track.
pub const PEAK_COUNT: usize = 1200;

/// Bound the in-memory peak cache. The database is the durable cache; this
/// only saves a query when the user skips back and forth.
const CACHE_LIMIT: usize = 32;

pub struct WaveformService {
    waveforms: WaveformRepo,
    /// A cheap existence check before decoding. `TrackRepo` is kept so a
    /// waveform is not generated for a row that has been deleted.
    tracks: TrackRepo,
    cache: HashMap<TrackId, Arc<Vec<f32>>>,
    order: VecDeque<TrackId>,
    /// Tracks whose waveform is known to be unavailable, so a broken file is
    /// not decoded again on every frame.
    unavailable: HashSet<TrackId>,
    inflight: Option<(TrackId, Receiver<Result<(Vec<u8>, f32), String>>)>,
    pub generated: u32,
}

impl WaveformService {
    pub fn new(db: database::Database) -> Self {
        Self {
            waveforms: WaveformRepo::new(db.clone()),
            tracks: TrackRepo::new(db),
            cache: HashMap::new(),
            order: VecDeque::new(),
            unavailable: HashSet::new(),
            inflight: None,
            generated: 0,
        }
    }

    /// Make sure a waveform is available for `track`, loading it from the
    /// database or starting a decode.
    pub fn ensure_loaded(&mut self, track: &Track) {
        if self.cache.contains_key(&track.id) || self.unavailable.contains(&track.id) {
            return;
        }
        if self
            .inflight
            .as_ref()
            .is_some_and(|(id, _)| *id == track.id)
        {
            return;
        }

        if let Ok(Some(cached)) = self.waveforms.get(&track.id) {
            self.insert(track.id, cached.amplitudes());
            return;
        }

        // One decode at a time. A skip supersedes the previous request; its
        // answer is simply not inserted when it arrives.
        let id = track.id;
        let path = track.path.clone();
        let duration = track.duration_secs;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let outcome = generate_peaks(&path, duration, PEAK_COUNT)
                .map_err(|e| format!("waveform for {}: {e}", path.display()));
            let _ = tx.send(outcome);
        });
        self.inflight = Some((id, rx));
    }

    /// Collect a finished decode. Returns true when a waveform became
    /// available and the UI should repaint.
    pub fn poll(&mut self) -> bool {
        let Some((id, rx)) = self.inflight.take() else {
            return false;
        };
        match rx.try_recv() {
            Ok(Ok((peaks, duration))) => {
                if self.tracks.get_by_id(&id).ok().flatten().is_none() {
                    return false;
                }
                let _ = self.waveforms.put(&id, &peaks, duration);
                self.generated = self.generated.saturating_add(1);
                self.insert(id, peaks.iter().map(|p| *p as f32 / 255.0).collect());
                true
            }
            Ok(Err(e)) => {
                log::debug!("{e}");
                self.unavailable.insert(id);
                false
            }
            Err(TryRecvError::Empty) => {
                self.inflight = Some((id, rx));
                false
            }
            Err(TryRecvError::Disconnected) => {
                self.unavailable.insert(id);
                false
            }
        }
    }

    /// The current peaks for a track, if they have been cached.
    pub fn peaks(&self, id: &TrackId) -> Option<&[f32]> {
        self.cache.get(id).map(|p| p.as_slice())
    }

    pub fn is_loading(&self, id: &TrackId) -> bool {
        self.inflight
            .as_ref()
            .is_some_and(|(pending, _)| pending == id)
    }

    /// Whether a decode is in flight for any track.
    pub fn has_pending(&self) -> bool {
        self.inflight.is_some()
    }

    fn insert(&mut self, id: TrackId, peaks: Vec<f32>) {
        if self.cache.insert(id, Arc::new(peaks)).is_none() {
            self.order.push_back(id);
        }
        while self.order.len() > CACHE_LIMIT {
            if let Some(oldest) = self.order.pop_front() {
                self.cache.remove(&oldest);
            }
        }
    }
}

/// Decode a file into a fixed number of normalised peak bytes.
///
/// The peak of a bucket is its largest absolute sample across every channel,
/// which is what makes a quiet passage still look quiet after the whole file
/// is scaled into one bar. The decoder is the same one playback uses, so a
/// format that plays also has a waveform.
pub fn generate_peaks(
    path: &std::path::Path,
    duration_secs: f32,
    buckets: usize,
) -> Result<(Vec<u8>, f32), String> {
    use engine::decode::Decoder;

    let buckets = buckets.max(1);
    let mut decoder = Decoder::open(path).map_err(|e| e.to_string())?;
    let duration = if duration_secs > 0.0 {
        duration_secs
    } else {
        decoder.duration_secs()
    };
    let sample_rate = decoder.info().sample_rate.max(1) as f64;
    let total_frames = (duration as f64 * sample_rate).max(1.0);

    let mut peaks = vec![0.0f32; buckets];
    let mut frame_position = 0f64;
    loop {
        let chunk = match decoder.decode_next(4096) {
            Ok(chunk) => chunk,
            // End of stream is the normal way this loop ends; a codec may
            // also report a truncated final packet, which must not throw away
            // the peaks collected so far.
            Err(_) => break,
        };
        if chunk.frame_count == 0 {
            break;
        }
        let channels = chunk.channels.max(1);
        for frame in 0..chunk.frame_count {
            let mut peak = 0.0f32;
            let base = frame * channels;
            for channel in 0..channels {
                if let Some(sample) = chunk.samples.get(base + channel) {
                    peak = peak.max(sample.abs());
                }
            }
            let bucket = ((frame_position / total_frames) * buckets as f64) as usize;
            if let Some(slot) = peaks.get_mut(bucket.min(buckets - 1)) {
                *slot = slot.max(peak);
            }
            frame_position += 1.0;
        }
    }

    // A file that decoded no frames still gets a flat waveform rather than an
    // error: the seek bar can then distinguish "no peaks" from "still loading".
    let max = peaks.iter().copied().fold(0.0f32, f32::max);
    let scale = if max > 0.0001 { 255.0 / max } else { 0.0 };
    let bytes = peaks
        .iter()
        .map(|p| (p * scale).round().clamp(0.0, 255.0) as u8)
        .collect();
    Ok((bytes, duration))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_buckets_cover_the_whole_file() {
        // A hand-built 1-second 16-bit mono WAV: silence, then a loud second
        // half. 16-bit samples have no signed/unsigned ambiguity, so the
        // decoder cannot turn digital silence into a DC step.
        let dir = std::env::temp_dir().join(format!("sp_waveform_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ramp.wav");
        let sample_rate = 8000u32;
        let frames = sample_rate as usize;
        let mut data = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let amplitude: i16 = if i < frames / 2 { 0 } else { 20_000 };
            data.extend_from_slice(&amplitude.to_le_bytes());
        }
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // mono
        wav.extend_from_slice(&sample_rate.to_le_bytes());
        wav.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
        wav.extend_from_slice(&2u16.to_le_bytes()); // block align
        wav.extend_from_slice(&16u16.to_le_bytes()); // bits
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
        wav.extend_from_slice(&data);
        std::fs::write(&path, wav).unwrap();

        let (peaks, duration) = generate_peaks(&path, 1.0, 20).unwrap();
        assert_eq!(peaks.len(), 20);
        assert!((duration - 1.0).abs() < 0.05, "duration={duration}");
        assert!(peaks[..8].iter().all(|p| *p < 8), "first half is quiet");
        assert!(peaks[12..].iter().any(|p| *p > 200), "second half is loud");
    }
}
