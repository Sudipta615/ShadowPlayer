//! Audio-CD extraction through `libcdio`, `libcdio-paranoia` and (optionally)
//! `libcddb`.
//!
//! Drives are opened by path, so the same code works for `/dev/sr0`, a macOS
//! device node, and a Windows drive letter. Every native library is loaded at
//! runtime: a system without CD support still builds and still plays files.
//!
//! Extraction is track-at-a-time and checks the sector count the drive reports.
//! Output is a 16-bit 44.1 kHz WAV, optionally encoded through
//! [`crate::transcode`]. CDDB is best-effort; a failed lookup never loses audio.

use std::ffi::{c_char, c_int, c_uint, c_void, CStr, CString};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::dynlib;
use crate::transcode::{self, OutputFormat, TranscodeOptions};
use crate::MediaError;

const CD_FRAMES_PER_SEC: u32 = 75;
const CD_SAMPLES_PER_SECTOR: usize = 2352 / 2;
const SEEK_SET: c_int = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RipFormat {
    Wav,
    Flac16,
    Flac24,
    Opus,
}

impl RipFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Flac16 | Self::Flac24 => "flac",
            Self::Opus => "opus",
        }
    }

    fn export_format(self) -> Option<OutputFormat> {
        match self {
            Self::Wav => None,
            Self::Flac16 => Some(OutputFormat::Flac { bit_depth: 16 }),
            Self::Flac24 => Some(OutputFormat::Flac { bit_depth: 24 }),
            Self::Opus => Some(OutputFormat::Opus),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CdTrack {
    pub number: u32,
    pub title: String,
    pub artist: String,
    pub first_sector: u32,
    pub last_sector: u32,
    pub duration_secs: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DiscMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub year: Option<u32>,
    pub genre: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CdDisc {
    pub device: String,
    pub tracks: Vec<CdTrack>,
    pub metadata: DiscMetadata,
    pub warnings: Vec<String>,
}

impl CdDisc {
    pub fn total_duration_secs(&self) -> f64 {
        self.tracks.iter().map(|track| track.duration_secs).sum()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RipProgress {
    pub track_number: u32,
    pub track_count: u32,
    pub sector: u32,
    pub total_sectors: u32,
    pub track_title: String,
    pub completed: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RipOptions {
    pub output_dir: PathBuf,
    pub format: RipFormat,
    pub fetch_metadata: bool,
    pub overwrite: bool,
}

impl RipOptions {
    pub fn new(output_dir: impl Into<PathBuf>) -> Self {
        Self {
            output_dir: output_dir.into(),
            format: RipFormat::Flac24,
            fetch_metadata: true,
            overwrite: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RipReport {
    pub disc: CdDisc,
    pub files: Vec<PathBuf>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CdRipper {
    device: PathBuf,
}

impl CdRipper {
    pub fn new(device: impl Into<PathBuf>) -> Self {
        Self {
            device: device.into(),
        }
    }

    /// Linux/BSD conventional drives. macOS and Windows expose no reliable
    /// conventional path, so those callers name the drive themselves.
    pub fn default_device() -> Result<Self, MediaError> {
        for path in ["/dev/cdrom", "/dev/sr0", "/dev/sr1"] {
            if Path::new(path).exists() {
                return Ok(Self::new(path));
            }
        }
        Err(MediaError::Cd(
            "no optical drive was found at /dev/cdrom or /dev/sr0".to_string(),
        ))
    }

    pub fn device(&self) -> &Path {
        &self.device
    }

    pub fn probe(&self, fetch_metadata: bool) -> Result<CdDisc, MediaError> {
        let api = Cdda::load()?;
        let mut warnings = Vec::new();
        let raw = api.raw_open(&self.device)?;
        let count = unsafe { (api.tracks)(raw) };
        if count <= 0 {
            unsafe { (api.close)(raw) };
            return Err(MediaError::Cd(
                "the drive reports no audio tracks; is an audio CD inserted?".to_string(),
            ));
        }
        let count = count as u32;
        let mut tracks = Vec::with_capacity(count as usize);
        for number in 1..=count {
            let first = unsafe { (api.track_first)(raw, number) };
            let last = unsafe { (api.track_last)(raw, number) };
            if last < first {
                unsafe { (api.close)(raw) };
                return Err(MediaError::Cd(format!(
                    "track {number} reports an impossible sector range {first}..{last}"
                )));
            }
            tracks.push(CdTrack {
                number,
                title: format!("Track {number:02}"),
                artist: String::new(),
                first_sector: first,
                last_sector: last,
                duration_secs: (last - first + 1) as f64 / CD_FRAMES_PER_SEC as f64,
            });
        }
        unsafe { (api.close)(raw) };

        let metadata = if fetch_metadata {
            match lookup_metadata(&tracks) {
                Ok((metadata, named)) => {
                    for track in &mut tracks {
                        if let Some(found) = named.iter().find(|t| t.number == track.number) {
                            if !found.title.trim().is_empty() {
                                track.title = found.title.clone();
                            }
                            if !found.artist.trim().is_empty() {
                                track.artist = found.artist.clone();
                            }
                        }
                    }
                    metadata
                }
                Err(error) => {
                    warnings.push(format!("CDDB lookup failed: {error}"));
                    DiscMetadata::default()
                }
            }
        } else {
            DiscMetadata::default()
        };

        if let Some(artist) = metadata.artist.as_deref().filter(|a| !a.trim().is_empty()) {
            for track in &mut tracks {
                if track.artist.trim().is_empty() {
                    track.artist = artist.to_string();
                }
            }
        }

        Ok(CdDisc {
            device: self.device.to_string_lossy().into_owned(),
            tracks,
            metadata,
            warnings,
        })
    }

    pub fn rip(
        &self,
        options: &RipOptions,
        mut progress: impl FnMut(RipProgress) -> bool,
    ) -> Result<RipReport, MediaError> {
        let disc = self.probe(options.fetch_metadata)?;
        if disc.tracks.is_empty() {
            return Err(MediaError::Cd("the disc has no audio tracks".to_string()));
        }
        std::fs::create_dir_all(&options.output_dir)?;
        let mut files = Vec::new();
        let mut warnings = disc.warnings.clone();
        for track in &disc.tracks {
            match self.rip_one(&disc, track, options, &mut progress, &files) {
                Ok(path) => files.push(path),
                Err(MediaError::Cd(skip)) if skip.starts_with("exists:") => {
                    let path = PathBuf::from(&skip["exists:".len()..]);
                    warnings.push(format!("{} already exists; skipped", path.display()));
                    files.push(path);
                }
                Err(error) => return Err(error),
            }
        }
        Ok(RipReport {
            disc,
            files,
            warnings,
        })
    }

    pub fn rip_track(
        &self,
        track_number: u32,
        options: &RipOptions,
        mut progress: impl FnMut(RipProgress) -> bool,
    ) -> Result<RipReport, MediaError> {
        let disc = self.probe(options.fetch_metadata)?;
        let track = disc
            .tracks
            .iter()
            .find(|track| track.number == track_number)
            .cloned()
            .ok_or_else(|| {
                MediaError::Cd(format!(
                    "the disc has no track {track_number} (1..={})",
                    disc.tracks.len()
                ))
            })?;
        std::fs::create_dir_all(&options.output_dir)?;
        let files = Vec::new();
        let path = self.rip_one(&disc, &track, options, &mut progress, &files)?;
        Ok(RipReport {
            disc,
            files: vec![path],
            warnings: Vec::new(),
        })
    }

    fn rip_one(
        &self,
        disc: &CdDisc,
        track: &CdTrack,
        options: &RipOptions,
        progress: &mut impl FnMut(RipProgress) -> bool,
        completed: &[PathBuf],
    ) -> Result<PathBuf, MediaError> {
        let base = track_file_stem(disc, track);
        let final_path = options
            .output_dir
            .join(format!("{base}.{}", options.format.extension()));
        if final_path.exists() && !options.overwrite {
            return Err(MediaError::Cd(format!("exists:{}", final_path.display())));
        }
        if let Some(parent) = final_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let wav_path = if options.format == RipFormat::Wav {
            final_path.clone()
        } else {
            options.output_dir.join(format!(".{base}.rip.wav"))
        };
        if let Some(parent) = wav_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let api = Cdda::load()?;
        let paranoia = Paranoia::load()?;
        let raw = api.raw_open(&self.device)?;
        let handle = unsafe { (paranoia.init)(raw) };
        if handle.is_null() {
            unsafe { (api.close)(raw) };
            return Err(MediaError::Cd(
                "libcdio-paranoia could not initialise this drive".to_string(),
            ));
        }
        let outcome = (|| {
            let seek = unsafe { (paranoia.seek)(handle, track.first_sector, SEEK_SET) };
            if seek < 0 {
                return Err(MediaError::Cd(format!(
                    "could not seek the drive to sector {}",
                    track.first_sector
                )));
            }
            let total = track.last_sector.saturating_sub(track.first_sector) + 1;
            let mut pcm = Vec::with_capacity(total as usize * CD_SAMPLES_PER_SECTOR);
            for sector in 0..total {
                let pointer = unsafe { (paranoia.read)(handle, std::ptr::null_mut()) };
                if pointer.is_null() {
                    return Err(MediaError::Cd(format!(
                        "track {}: read failed at sector {}",
                        track.number,
                        track.first_sector + sector
                    )));
                }
                let samples = unsafe { std::slice::from_raw_parts(pointer, CD_SAMPLES_PER_SECTOR) };
                pcm.extend_from_slice(samples);
                if !progress(RipProgress {
                    track_number: track.number,
                    track_count: disc.tracks.len() as u32,
                    sector,
                    total_sectors: total,
                    track_title: track.title.clone(),
                    completed: completed.to_vec(),
                }) {
                    return Err(MediaError::Cancelled);
                }
            }
            write_wav_i16(&wav_path, 44_100, 2, &pcm)
        })();
        unsafe {
            (paranoia.free)(handle);
            (api.close)(raw);
        }
        if let Err(error) = outcome {
            let _ = std::fs::remove_file(&wav_path);
            return Err(error);
        }

        if let Some(format) = options.format.export_format() {
            let encode_options = TranscodeOptions {
                format,
                sample_rate: None,
                channels: None,
                copy_tags: false,
            };
            if let Err(error) = transcode::transcode(&wav_path, &final_path, &encode_options) {
                let _ = std::fs::remove_file(&wav_path);
                return Err(error);
            }
            let _ = std::fs::remove_file(&wav_path);
            write_cd_tags(&final_path, disc, track)?;
        }
        Ok(final_path)
    }
}

fn track_file_stem(disc: &CdDisc, track: &CdTrack) -> String {
    let album = disc.metadata.title.as_deref().unwrap_or("CD");
    let title = if track.title.trim().is_empty() {
        format!("Track {:02}", track.number)
    } else {
        track.title.clone()
    };
    format!(
        "{}/{:02} - {}",
        sanitise_filename(album),
        track.number,
        sanitise_filename(&title)
    )
}

/// Remove characters illegal in a filename on at least one platform. A CDDB
/// title is untrusted text and must never be interpreted as a path.
pub fn sanitise_filename(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_control() || matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
            out.push('_');
        } else if ch.is_whitespace() {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    let trimmed = out.trim().trim_end_matches('.').trim();
    let mut result = if trimmed.is_empty() {
        "Untitled".to_string()
    } else {
        trimmed.to_string()
    };
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVED
        .iter()
        .any(|reserved| result.eq_ignore_ascii_case(reserved))
    {
        result.push('_');
    }
    result
}

fn write_cd_tags(path: &Path, disc: &CdDisc, track: &CdTrack) -> Result<(), MediaError> {
    let artist = if track.artist.trim().is_empty() {
        disc.metadata.artist.clone()
    } else {
        Some(track.artist.clone())
    };
    let tags = engine::decode::EditorialTags {
        title: Some(track.title.clone()).filter(|title| !title.trim().is_empty()),
        artist: artist.filter(|artist| !artist.trim().is_empty()),
        album: disc.metadata.title.clone(),
        genre: disc.metadata.genre.clone(),
        year: disc.metadata.year,
        track_number: Some(track.number),
        disc_number: None,
    };
    engine::decode::write_editorial_tags(path, &tags).map_err(|error| MediaError::Encode {
        path: path.to_path_buf(),
        message: format!("the audio was written but tagging failed: {error}"),
    })
}

fn write_wav_i16(
    path: &Path,
    sample_rate: u32,
    channels: u16,
    samples: &[i16],
) -> Result<(), MediaError> {
    let data_len = samples.len() * 2;
    let mut bytes = Vec::with_capacity(44 + data_len);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * channels as u32 * 2).to_le_bytes());
    bytes.extend_from_slice(&(channels * 2).to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(data_len as u32).to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

struct Cdda {
    _cdda: libloading::Library,
    _cdio: libloading::Library,
    identify: unsafe extern "C" fn(*const c_char, c_int, *mut *mut c_char) -> *mut c_void,
    open: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    close: unsafe extern "C" fn(*mut c_void),
    tracks: unsafe extern "C" fn(*mut c_void) -> c_int,
    track_first: unsafe extern "C" fn(*mut c_void, c_uint) -> c_uint,
    track_last: unsafe extern "C" fn(*mut c_void, c_uint) -> c_uint,
}

impl Cdda {
    fn load() -> Result<Self, MediaError> {
        let (cdda_names, cdio_names) = if cfg!(target_os = "macos") {
            (
                vec!["libcdio_cdda.2.dylib", "libcdio_cdda.dylib"],
                vec!["libcdio.19.dylib", "libcdio.dylib"],
            )
        } else if cfg!(target_os = "windows") {
            (
                vec!["libcdio_cdda-2.dll", "cdio_cdda.dll"],
                vec!["libcdio-19.dll", "cdio.dll"],
            )
        } else {
            (
                vec!["libcdio_cdda.so.2", "libcdio_cdda.so"],
                vec!["libcdio.so.19", "libcdio.so"],
            )
        };
        let cdda = dynlib::open("CD extraction", "libcdio_cdda", &cdda_names)?;
        let cdio = dynlib::open("CD extraction", "libcdio", &cdio_names)?;
        unsafe {
            macro_rules! sym {
                ($lib:expr, $name:literal, $ty:ty) => {
                    *$lib
                        .get::<$ty>(concat!($name, "\0").as_bytes())
                        .map_err(|e| MediaError::LibraryMissing {
                            what: "CD extraction",
                            library: "libcdio_cdda",
                            detail: format!("symbol {}: {e}", $name),
                        })?
                };
            }
            Ok(Self {
                identify: sym!(
                    cdda,
                    "cdio_cddap_identify",
                    unsafe extern "C" fn(*const c_char, c_int, *mut *mut c_char) -> *mut c_void
                ),
                open: sym!(
                    cdda,
                    "cdio_cddap_open",
                    unsafe extern "C" fn(*mut c_void) -> *mut c_void
                ),
                close: sym!(cdda, "cdio_cddap_close", unsafe extern "C" fn(*mut c_void)),
                tracks: sym!(
                    cdda,
                    "cdio_cddap_tracks",
                    unsafe extern "C" fn(*mut c_void) -> c_int
                ),
                track_first: sym!(
                    cdda,
                    "cdio_cddap_track_firstsector",
                    unsafe extern "C" fn(*mut c_void, c_uint) -> c_uint
                ),
                track_last: sym!(
                    cdda,
                    "cdio_cddap_track_lastsector",
                    unsafe extern "C" fn(*mut c_void, c_uint) -> c_uint
                ),
                _cdda: cdda,
                _cdio: cdio,
            })
        }
    }

    fn raw_open(&self, device: &Path) -> Result<*mut c_void, MediaError> {
        let text = device.to_string_lossy();
        let c_device = CString::new(text.as_bytes()).map_err(|_| {
            MediaError::InvalidInput("the CD device path contains a NUL byte".to_string())
        })?;
        let mut message: *mut c_char = std::ptr::null_mut();
        let raw = unsafe { (self.identify)(c_device.as_ptr(), 0, &mut message) };
        if !message.is_null() {
            unsafe {
                unsafe extern "C" {
                    fn free(pointer: *mut c_void);
                }
                free(message.cast());
            }
        }
        if raw.is_null() {
            return Err(MediaError::Cd(format!(
                "cannot identify {} as a CD drive",
                device.display()
            )));
        }
        let opened = unsafe { (self.open)(raw) };
        if opened.is_null() {
            unsafe { (self.close)(raw) };
            return Err(MediaError::Cd(format!(
                "cannot open {} for audio extraction",
                device.display()
            )));
        }
        Ok(opened)
    }
}

struct Paranoia {
    _library: libloading::Library,
    init: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    read: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut i16,
    seek: unsafe extern "C" fn(*mut c_void, c_uint, c_int) -> c_int,
    free: unsafe extern "C" fn(*mut c_void),
}

impl Paranoia {
    fn load() -> Result<Self, MediaError> {
        // Some distributions ship the paranoia symbols in libcdio_paranoia,
        // others in libcdio_cdda. Try both.
        let mut library = None;
        for stem in ["cdio_paranoia", "cdio_cdda"] {
            let names = if cfg!(target_os = "macos") {
                vec![format!("lib{stem}.2.dylib"), format!("lib{stem}.dylib")]
            } else if cfg!(target_os = "windows") {
                vec![format!("lib{stem}-2.dll"), format!("{stem}.dll")]
            } else {
                dynlib::linux_macos_candidates(stem, "2")
            };
            let refs: Vec<&str> = names.iter().map(String::as_str).collect();
            if let Ok(candidate) = dynlib::open("CD extraction", "libcdio_paranoia", &refs) {
                if unsafe { candidate.get::<*mut c_void>(b"cdio_paranoia_read\0") }.is_ok() {
                    library = Some(candidate);
                    break;
                }
            }
        }
        let library = library.ok_or_else(|| MediaError::LibraryMissing {
            what: "CD extraction",
            library: "libcdio_paranoia",
            detail: "the paranoia API was not exported by any installed CD library".to_string(),
        })?;
        unsafe {
            macro_rules! sym {
                ($name:literal, $ty:ty) => {
                    *library
                        .get::<$ty>(concat!($name, "\0").as_bytes())
                        .map_err(|e| MediaError::LibraryMissing {
                            what: "CD extraction",
                            library: "libcdio_paranoia",
                            detail: format!("symbol {}: {e}", $name),
                        })?
                };
            }
            Ok(Self {
                init: sym!(
                    "cdio_paranoia_init",
                    unsafe extern "C" fn(*mut c_void) -> *mut c_void
                ),
                read: sym!(
                    "cdio_paranoia_read",
                    unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut i16
                ),
                seek: sym!(
                    "cdio_paranoia_seek",
                    unsafe extern "C" fn(*mut c_void, c_uint, c_int) -> c_int
                ),
                free: sym!("cdio_paranoia_free", unsafe extern "C" fn(*mut c_void)),
                _library: library,
            })
        }
    }
}

fn lookup_metadata(tracks: &[CdTrack]) -> Result<(DiscMetadata, Vec<CdTrack>), MediaError> {
    let cddb = Cddb::load()?;
    cddb.lookup(tracks)
}

struct Cddb {
    _library: libloading::Library,
    conn_new: unsafe extern "C" fn() -> *mut c_void,
    connect: unsafe extern "C" fn(*mut c_void) -> c_int,
    destroy: unsafe extern "C" fn(*mut c_void),
    disc_new: unsafe extern "C" fn() -> *mut c_void,
    disc_destroy: unsafe extern "C" fn(*mut c_void),
    track_new: unsafe extern "C" fn() -> *mut c_void,
    disc_add_track: unsafe extern "C" fn(*mut c_void, *mut c_void),
    track_set_offset: unsafe extern "C" fn(*mut c_void, c_uint),
    disc_set_length: unsafe extern "C" fn(*mut c_void, c_uint),
    query: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void,
    read: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void,
    disc_title: unsafe extern "C" fn(*mut c_void) -> *const c_char,
    disc_artist: unsafe extern "C" fn(*mut c_void) -> *const c_char,
    disc_year: unsafe extern "C" fn(*mut c_void) -> c_int,
    disc_genre: unsafe extern "C" fn(*mut c_void) -> *const c_char,
    disc_track_count: unsafe extern "C" fn(*mut c_void) -> c_int,
    disc_track: unsafe extern "C" fn(*mut c_void, c_int) -> *mut c_void,
    track_title: unsafe extern "C" fn(*mut c_void) -> *const c_char,
    track_artist: unsafe extern "C" fn(*mut c_void) -> *const c_char,
}

struct CddbDisc {
    handle: *mut c_void,
    destroy: unsafe extern "C" fn(*mut c_void),
}

impl Drop for CddbDisc {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { (self.destroy)(self.handle) };
        }
    }
}

impl Cddb {
    fn load() -> Result<Self, MediaError> {
        let names = if cfg!(target_os = "macos") {
            vec!["libcddb.2.dylib", "libcddb.dylib"]
        } else if cfg!(target_os = "windows") {
            vec!["libcddb-2.dll", "cddb.dll"]
        } else {
            vec!["libcddb.so.2", "libcddb.so"]
        };
        let library = dynlib::open("CDDB lookup", "libcddb", &names)?;
        unsafe {
            macro_rules! sym {
                ($name:literal, $ty:ty) => {
                    *library
                        .get::<$ty>(concat!($name, "\0").as_bytes())
                        .map_err(|e| MediaError::LibraryMissing {
                            what: "CDDB lookup",
                            library: "libcddb",
                            detail: format!("symbol {}: {e}", $name),
                        })?
                };
            }
            Ok(Self {
                conn_new: sym!("cddb_new", unsafe extern "C" fn() -> *mut c_void),
                connect: sym!("cddb_connect", unsafe extern "C" fn(*mut c_void) -> c_int),
                destroy: sym!("cddb_destroy", unsafe extern "C" fn(*mut c_void)),
                disc_new: sym!("cddb_disc_new", unsafe extern "C" fn() -> *mut c_void),
                disc_destroy: sym!("cddb_disc_destroy", unsafe extern "C" fn(*mut c_void)),
                track_new: sym!("cddb_track_new", unsafe extern "C" fn() -> *mut c_void),
                disc_add_track: sym!(
                    "cddb_disc_add_track",
                    unsafe extern "C" fn(*mut c_void, *mut c_void)
                ),
                track_set_offset: sym!(
                    "cddb_track_set_frame_offset",
                    unsafe extern "C" fn(*mut c_void, c_uint)
                ),
                disc_set_length: sym!(
                    "cddb_disc_set_length",
                    unsafe extern "C" fn(*mut c_void, c_uint)
                ),
                query: sym!(
                    "cddb_query",
                    unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void
                ),
                read: sym!(
                    "cddb_read",
                    unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void
                ),
                disc_title: sym!(
                    "cddb_disc_get_title",
                    unsafe extern "C" fn(*mut c_void) -> *const c_char
                ),
                disc_artist: sym!(
                    "cddb_disc_get_artist",
                    unsafe extern "C" fn(*mut c_void) -> *const c_char
                ),
                disc_year: sym!(
                    "cddb_disc_get_year",
                    unsafe extern "C" fn(*mut c_void) -> c_int
                ),
                disc_genre: sym!(
                    "cddb_disc_get_genre",
                    unsafe extern "C" fn(*mut c_void) -> *const c_char
                ),
                disc_track_count: sym!(
                    "cddb_disc_get_track_count",
                    unsafe extern "C" fn(*mut c_void) -> c_int
                ),
                disc_track: sym!(
                    "cddb_disc_get_track",
                    unsafe extern "C" fn(*mut c_void, c_int) -> *mut c_void
                ),
                track_title: sym!(
                    "cddb_track_get_title",
                    unsafe extern "C" fn(*mut c_void) -> *const c_char
                ),
                track_artist: sym!(
                    "cddb_track_get_artist",
                    unsafe extern "C" fn(*mut c_void) -> *const c_char
                ),
                _library: library,
            })
        }
    }

    fn lookup(&self, tracks: &[CdTrack]) -> Result<(DiscMetadata, Vec<CdTrack>), MediaError> {
        unsafe {
            let conn = (self.conn_new)();
            if conn.is_null() {
                return Err(MediaError::Cd(
                    "CDDB connection could not be created".to_string(),
                ));
            }
            let result = (|| {
                if (self.connect)(conn) < 0 {
                    return Err(MediaError::Cd(
                        "CDDB server could not be reached".to_string(),
                    ));
                }
                let query = (self.disc_new)();
                if query.is_null() {
                    return Err(MediaError::Cd("CDDB disc could not be created".to_string()));
                }
                let query = CddbDisc {
                    handle: query,
                    destroy: self.disc_destroy,
                };
                for track in tracks {
                    let item = (self.track_new)();
                    if item.is_null() {
                        continue;
                    }
                    (self.track_set_offset)(item, track.first_sector);
                    (self.disc_add_track)(query.handle, item);
                }
                let length = tracks
                    .last()
                    .map(|track| track.last_sector.saturating_sub(track.first_sector) + 1)
                    .unwrap_or(0);
                (self.disc_set_length)(query.handle, length);

                let found = (self.query)(conn, query.handle);
                if found.is_null() {
                    return Ok((DiscMetadata::default(), Vec::new()));
                }
                let read = (self.read)(conn, found);
                let owned = CddbDisc {
                    handle: if read.is_null() { found } else { read },
                    destroy: self.disc_destroy,
                };
                if owned.handle.is_null() {
                    return Ok((DiscMetadata::default(), Vec::new()));
                }
                let metadata = DiscMetadata {
                    title: cstr_opt((self.disc_title)(owned.handle)),
                    artist: cstr_opt((self.disc_artist)(owned.handle)),
                    year: {
                        let year = (self.disc_year)(owned.handle);
                        (year > 0).then_some(year as u32)
                    },
                    genre: cstr_opt((self.disc_genre)(owned.handle)),
                };
                let count = (self.disc_track_count)(owned.handle).max(0);
                let mut named = Vec::new();
                for index in 0..count {
                    let item = (self.disc_track)(owned.handle, index);
                    if item.is_null() {
                        continue;
                    }
                    named.push(CdTrack {
                        number: (index + 1) as u32,
                        title: cstr_opt((self.track_title)(item)).unwrap_or_default(),
                        artist: cstr_opt((self.track_artist)(item)).unwrap_or_default(),
                        first_sector: 0,
                        last_sector: 0,
                        duration_secs: 0.0,
                    });
                }
                Ok((metadata, named))
            })();
            (self.destroy)(conn);
            result
        }
    }
}

fn cstr_opt(pointer: *const c_char) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    let value = unsafe { CStr::from_ptr(pointer) }
        .to_string_lossy()
        .trim()
        .to_string();
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filenames_are_safe_on_every_platform() {
        assert_eq!(sanitise_filename("AC/DC: Live?"), "AC_DC_ Live_");
        assert_eq!(sanitise_filename("  ..  "), "Untitled");
        assert_eq!(sanitise_filename("CON"), "CON_");
        assert_eq!(sanitise_filename("nul"), "nul_");
        assert_eq!(sanitise_filename("line\nbreak"), "line_break");
    }

    #[test]
    fn track_names_are_numbered_under_the_album() {
        let disc = CdDisc {
            device: "/dev/sr0".to_string(),
            tracks: Vec::new(),
            metadata: DiscMetadata {
                title: Some("An Album".to_string()),
                ..Default::default()
            },
            warnings: Vec::new(),
        };
        let track = CdTrack {
            number: 3,
            title: "A Song".to_string(),
            artist: String::new(),
            first_sector: 0,
            last_sector: 100,
            duration_secs: 1.0,
        };
        assert_eq!(track_file_stem(&disc, &track), "An Album/03 - A Song");
    }

    #[test]
    fn the_wav_header_describes_the_audio_it_carries() {
        let dir = std::env::temp_dir().join(format!(
            "sp_cd_wav_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("track.wav");
        write_wav_i16(&path, 44_100, 2, &[0, i16::MAX, i16::MIN, 0]).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 2);
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            44_100
        );
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 8);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_discs_duration_is_the_sum_of_its_tracks() {
        let disc = CdDisc {
            device: "x".into(),
            tracks: vec![
                CdTrack {
                    number: 1,
                    title: "a".into(),
                    artist: "".into(),
                    first_sector: 0,
                    last_sector: 74,
                    duration_secs: 1.0,
                },
                CdTrack {
                    number: 2,
                    title: "b".into(),
                    artist: "".into(),
                    first_sector: 75,
                    last_sector: 149,
                    duration_secs: 1.0,
                },
            ],
            metadata: DiscMetadata::default(),
            warnings: Vec::new(),
        };
        assert!((disc.total_duration_secs() - 2.0).abs() < f64::EPSILON);
    }
}
