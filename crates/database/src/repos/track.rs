//! Track and track-DSP repositories.

use crate::db::Database;
use domain::ids::*;
use std::path::PathBuf;

/// Compact metadata record used to accelerate library scanning without loading
/// full Track structures or performing individual queries per track.
#[derive(Debug, Clone)]
pub struct TrackScanEntry {
    pub id: TrackId,
    pub path: PathBuf,
    pub mtime_unix_secs: i64,
    pub mtime_nanos: u32,
    pub file_size_bytes: u64,
    pub content_hash: Option<String>,
    pub file_id: Option<String>,
}

const UPSERT_TRACK_SQL: &str = "INSERT INTO tracks (
    id, path, title, artist, album, genre, year, track_number, disc_number,
    duration_secs, sample_rate, channels, format, bitrate_kbps, bit_depth, is_lossless,
    replaygain_track_db, replaygain_track_peak, r128_loudness_lufs,
    replaygain_album_db, replaygain_album_peak,
    file_size_bytes, mtime_unix_secs, mtime_nanos, content_hash, file_id,
    is_favorite, rating, has_lyrics, play_count, skip_count, last_played_at, added_at, bpm
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34)
ON CONFLICT(path) DO UPDATE SET
    title = excluded.title,
    artist = excluded.artist,
    album = excluded.album,
    genre = excluded.genre,
    year = excluded.year,
    track_number = excluded.track_number,
    disc_number = excluded.disc_number,
    duration_secs = excluded.duration_secs,
    sample_rate = excluded.sample_rate,
    channels = excluded.channels,
    format = excluded.format,
    bitrate_kbps = excluded.bitrate_kbps,
    bit_depth = excluded.bit_depth,
    is_lossless = excluded.is_lossless,
    replaygain_track_db = excluded.replaygain_track_db,
    replaygain_track_peak = excluded.replaygain_track_peak,
    r128_loudness_lufs = excluded.r128_loudness_lufs,
    replaygain_album_db = excluded.replaygain_album_db,
    replaygain_album_peak = excluded.replaygain_album_peak,
    has_lyrics = excluded.has_lyrics,
    skip_count = excluded.skip_count,
    bpm = excluded.bpm,
    file_size_bytes = excluded.file_size_bytes,
    mtime_unix_secs = excluded.mtime_unix_secs,
    mtime_nanos = excluded.mtime_nanos,
    content_hash = excluded.content_hash,
    file_id = COALESCE(excluded.file_id, tracks.file_id)";

pub struct TrackRepo {
    db: Database,
}

pub type TrackDspRepo = dsp::TrackDspRepo;

mod dsp;
mod radio;
mod tracks;
