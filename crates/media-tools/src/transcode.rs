//! Lossless FLAC and Ogg-Opus export.
//!
//! Decoding goes through the engine's own decoder — the same one playback uses,
//! so an export supports every source the player supports. Encoding goes
//! through `libsndfile`, loaded dynamically: FLAC at 16 or 24 bits, Ogg-Opus at
//! 48 kHz with a windowed-sinc resampler when the source runs at another rate.
//!
//! Output is written to a sibling temporary file and renamed only after the
//! encoder has closed it successfully. A cancelled or failed export therefore
//! never leaves a truncated file where the user's album should be.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};

use engine::decode::{Decoder, EditorialTags};
use serde::{Deserialize, Serialize};

use crate::dynlib;
use crate::MediaError;

const SFM_WRITE: c_int = 0x20;
const SF_FORMAT_PCM_16: c_int = 0x0002;
const SF_FORMAT_PCM_24: c_int = 0x0003;
const SF_FORMAT_PCM_32: c_int = 0x0004;
const SF_FORMAT_FLOAT: c_int = 0x0006;
const SF_FORMAT_WAV: c_int = 0x010000;
const SF_FORMAT_AIFF: c_int = 0x020000;
const SF_FORMAT_FLAC: c_int = 0x170000;
const SF_FORMAT_CAF: c_int = 0x180000;
const SF_FORMAT_OGG: c_int = 0x200000;
const SF_FORMAT_MPEG: c_int = 0x230000;
const SF_FORMAT_VORBIS: c_int = 0x0060;
const SF_FORMAT_OPUS: c_int = 0x0064;
const SF_FORMAT_ALAC_16: c_int = 0x0070;
const SF_FORMAT_ALAC_24: c_int = 0x0072;
const SF_FORMAT_MPEG_LAYER_III: c_int = 0x0082;

/// The encoder to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    /// Native FLAC. `bit_depth` must be 16 or 24.
    Flac { bit_depth: u16 },
    /// Uncompressed PCM WAV. `bit_depth` must be 16, 24, or 32.
    Wav { bit_depth: u16 },
    /// 32-bit floating-point WAV.
    WavFloat,
    /// Uncompressed PCM AIFF. `bit_depth` must be 16, 24, or 32.
    Aiff { bit_depth: u16 },
    /// 32-bit floating-point AIFF.
    AiffFloat,
    /// MPEG-1 Audio Layer III (MP3).
    Mp3,
    /// Ogg-Opus. Always 48 kHz; the source is resampled first.
    Opus,
    /// Ogg-Vorbis.
    Vorbis,
    /// Apple Core Audio Format (CAF) with ALAC compression. `bit_depth` must be 16 or 24.
    CafAlac { bit_depth: u16 },
    /// Apple Core Audio Format (CAF) with uncompressed PCM. `bit_depth` must be 16 or 24.
    CafPcm { bit_depth: u16 },
}

impl OutputFormat {
    pub const ALL: &'static [OutputFormat] = &[
        Self::Flac { bit_depth: 16 },
        Self::Flac { bit_depth: 24 },
        Self::Wav { bit_depth: 16 },
        Self::Wav { bit_depth: 24 },
        Self::Wav { bit_depth: 32 },
        Self::WavFloat,
        Self::Mp3,
        Self::Opus,
        Self::Vorbis,
        Self::Aiff { bit_depth: 16 },
        Self::Aiff { bit_depth: 24 },
        Self::Aiff { bit_depth: 32 },
        Self::AiffFloat,
        Self::CafAlac { bit_depth: 16 },
        Self::CafAlac { bit_depth: 24 },
        Self::CafPcm { bit_depth: 16 },
        Self::CafPcm { bit_depth: 24 },
    ];

    pub fn extension(self) -> &'static str {
        match self {
            Self::Flac { .. } => "flac",
            Self::Wav { .. } | Self::WavFloat => "wav",
            Self::Aiff { .. } | Self::AiffFloat => "aiff",
            Self::Mp3 => "mp3",
            Self::Opus => "opus",
            Self::Vorbis => "ogg",
            Self::CafAlac { .. } | Self::CafPcm { .. } => "caf",
        }
    }

    pub fn sample_rate(self, source: u32) -> u32 {
        match self {
            Self::Opus => 48_000,
            _ => source,
        }
    }

    pub fn libsndfile_format(self) -> Result<c_int, MediaError> {
        match self {
            Self::Flac { bit_depth: 16 } => Ok(SF_FORMAT_FLAC | SF_FORMAT_PCM_16),
            Self::Flac { bit_depth: 24 } => Ok(SF_FORMAT_FLAC | SF_FORMAT_PCM_24),
            Self::Flac { bit_depth } => Err(MediaError::InvalidInput(format!(
                "FLAC bit depth must be 16 or 24, not {bit_depth}"
            ))),
            Self::Wav { bit_depth: 16 } => Ok(SF_FORMAT_WAV | SF_FORMAT_PCM_16),
            Self::Wav { bit_depth: 24 } => Ok(SF_FORMAT_WAV | SF_FORMAT_PCM_24),
            Self::Wav { bit_depth: 32 } => Ok(SF_FORMAT_WAV | SF_FORMAT_PCM_32),
            Self::Wav { bit_depth } => Err(MediaError::InvalidInput(format!(
                "WAV bit depth must be 16, 24, or 32, not {bit_depth}"
            ))),
            Self::WavFloat => Ok(SF_FORMAT_WAV | SF_FORMAT_FLOAT),
            Self::Aiff { bit_depth: 16 } => Ok(SF_FORMAT_AIFF | SF_FORMAT_PCM_16),
            Self::Aiff { bit_depth: 24 } => Ok(SF_FORMAT_AIFF | SF_FORMAT_PCM_24),
            Self::Aiff { bit_depth: 32 } => Ok(SF_FORMAT_AIFF | SF_FORMAT_PCM_32),
            Self::Aiff { bit_depth } => Err(MediaError::InvalidInput(format!(
                "AIFF bit depth must be 16, 24, or 32, not {bit_depth}"
            ))),
            Self::AiffFloat => Ok(SF_FORMAT_AIFF | SF_FORMAT_FLOAT),
            Self::Mp3 => Ok(SF_FORMAT_MPEG | SF_FORMAT_MPEG_LAYER_III),
            Self::Opus => Ok(SF_FORMAT_OGG | SF_FORMAT_OPUS),
            Self::Vorbis => Ok(SF_FORMAT_OGG | SF_FORMAT_VORBIS),
            Self::CafAlac { bit_depth: 16 } => Ok(SF_FORMAT_CAF | SF_FORMAT_ALAC_16),
            Self::CafAlac { bit_depth: 24 } => Ok(SF_FORMAT_CAF | SF_FORMAT_ALAC_24),
            Self::CafAlac { bit_depth } => Err(MediaError::InvalidInput(format!(
                "CAF ALAC bit depth must be 16 or 24, not {bit_depth}"
            ))),
            Self::CafPcm { bit_depth: 16 } => Ok(SF_FORMAT_CAF | SF_FORMAT_PCM_16),
            Self::CafPcm { bit_depth: 24 } => Ok(SF_FORMAT_CAF | SF_FORMAT_PCM_24),
            Self::CafPcm { bit_depth } => Err(MediaError::InvalidInput(format!(
                "CAF PCM bit depth must be 16 or 24, not {bit_depth}"
            ))),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Flac { bit_depth: 16 } => "FLAC 16-bit",
            Self::Flac { bit_depth: 24 } => "FLAC 24-bit",
            Self::Flac { .. } => "FLAC",
            Self::Wav { bit_depth: 16 } => "WAV 16-bit",
            Self::Wav { bit_depth: 24 } => "WAV 24-bit",
            Self::Wav { bit_depth: 32 } => "WAV 32-bit",
            Self::Wav { .. } => "WAV",
            Self::WavFloat => "WAV 32-bit Float",
            Self::Aiff { bit_depth: 16 } => "AIFF 16-bit",
            Self::Aiff { bit_depth: 24 } => "AIFF 24-bit",
            Self::Aiff { bit_depth: 32 } => "AIFF 32-bit",
            Self::Aiff { .. } => "AIFF",
            Self::AiffFloat => "AIFF 32-bit Float",
            Self::Mp3 => "MP3 (MPEG Layer III)",
            Self::Opus => "Ogg-Opus 48 kHz",
            Self::Vorbis => "Ogg-Vorbis",
            Self::CafAlac { bit_depth: 16 } => "CAF ALAC 16-bit",
            Self::CafAlac { bit_depth: 24 } => "CAF ALAC 24-bit",
            Self::CafAlac { .. } => "CAF ALAC",
            Self::CafPcm { bit_depth: 16 } => "CAF PCM 16-bit",
            Self::CafPcm { bit_depth: 24 } => "CAF PCM 24-bit",
            Self::CafPcm { .. } => "CAF PCM",
        }
    }
}

/// What to export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscodeOptions {
    pub format: OutputFormat,
    /// Explicit target sample rate in Hz. If None or 0, follows source.
    #[serde(default)]
    pub sample_rate: Option<u32>,
    /// Explicit target channels: None = keep source, Some(1) = mono downmix, Some(2) = stereo downmix.
    #[serde(default)]
    pub channels: Option<usize>,
    /// Copy the source's editorial tags to the new file. On by default.
    #[serde(default = "default_true")]
    pub copy_tags: bool,
}

fn default_true() -> bool {
    true
}

impl Default for TranscodeOptions {
    fn default() -> Self {
        Self {
            format: OutputFormat::Flac { bit_depth: 24 },
            sample_rate: None,
            channels: None,
            copy_tags: true,
        }
    }
}

/// The result of a successful export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscodeReport {
    pub input: PathBuf,
    pub output: PathBuf,
    pub format: String,
    pub source_sample_rate: u32,
    pub output_sample_rate: u32,
    pub channels: usize,
    pub frames: u64,
    pub duration_secs: f64,
    pub resampled: bool,
    pub metadata_written: bool,
    /// Non-fatal problems (for example, a tag container that refused the
    /// copied metadata). The audio file is complete.
    pub warnings: Vec<String>,
}

/// Decode `input`, encode it to `output` in the requested format, and copy its
/// editorial tags when possible.
pub fn transcode(
    input: impl AsRef<Path>,
    output: impl AsRef<Path>,
    options: &TranscodeOptions,
) -> Result<TranscodeReport, MediaError> {
    let input = input.as_ref();
    let output = output.as_ref();
    if input == output {
        return Err(MediaError::InvalidInput(
            "the output path must differ from the input".to_string(),
        ));
    }
    if !input.is_file() {
        return Err(MediaError::Decode {
            path: input.to_path_buf(),
            message: "file does not exist".to_string(),
        });
    }
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let (samples, channels, source_rate, duration_secs) = decode_all(input)?;
    if samples.is_empty() {
        return Err(MediaError::Decode {
            path: input.to_path_buf(),
            message: "the decoder produced no audio".to_string(),
        });
    }
    if channels == 0 {
        return Err(MediaError::Decode {
            path: input.to_path_buf(),
            message: "the decoder reported zero channels".to_string(),
        });
    }
    let target_rate = options
        .sample_rate
        .filter(|&r| r > 0)
        .unwrap_or_else(|| options.format.sample_rate(source_rate));
    let resampled = target_rate != source_rate;
    let (samples, channels) = match options.channels {
        Some(1) if channels != 1 => (downmix_to_mono(&samples, channels), 1),
        Some(2) if channels != 2 => (downmix_to_stereo(&samples, channels), 2),
        _ => (samples, channels),
    };
    if (options.format == OutputFormat::Opus
        || options.format == OutputFormat::Vorbis
        || options.format == OutputFormat::Mp3)
        && channels > 2
    {
        return Err(MediaError::Unsupported(format!(
            "{} export supports mono and stereo sources; choose FLAC or WAV for multichannel layouts",
            options.format.label()
        )));
    }
    let samples = if resampled {
        resample_interleaved(&samples, channels, source_rate, target_rate)
    } else {
        samples
    };

    let temp = temporary_sibling(output);
    let encoded = encode_sndfile(&temp, &samples, channels, target_rate, options.format);
    if let Err(error) = encoded {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }
    if !temp.is_file() {
        return Err(MediaError::Encode {
            path: output.to_path_buf(),
            message: "the encoder did not create an output file".to_string(),
        });
    }
    std::fs::rename(&temp, output).map_err(|error| {
        let _ = std::fs::remove_file(&temp);
        MediaError::Io(error)
    })?;

    let mut warnings = Vec::new();
    let mut metadata_written = false;
    if options.copy_tags {
        match copy_editorial_tags(input, output) {
            Ok(written) => metadata_written = written,
            Err(error) => warnings.push(format!("metadata could not be copied: {error}")),
        }
    }

    let frames = (samples.len() / channels) as u64;
    Ok(TranscodeReport {
        input: input.to_path_buf(),
        output: output.to_path_buf(),
        format: options.format.label().to_string(),
        source_sample_rate: source_rate,
        output_sample_rate: target_rate,
        channels,
        frames,
        duration_secs: duration_secs as f64,
        resampled,
        metadata_written,
        warnings,
    })
}

fn downmix_to_stereo(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels == 2 {
        return samples.to_vec();
    }
    let frames = samples.len() / channels;
    let mut out = Vec::with_capacity(frames * 2);
    if channels == 1 {
        for &s in samples {
            out.push(s);
            out.push(s);
        }
    } else if channels >= 6 {
        let inv_sqrt2 = std::f32::consts::FRAC_1_SQRT_2;
        for i in 0..frames {
            let offset = i * channels;
            let l = samples[offset];
            let r = samples[offset + 1];
            let c = samples[offset + 2];
            let ls = samples[offset + 4];
            let rs = samples[offset + 5];
            let left = l + c * inv_sqrt2 + ls * inv_sqrt2;
            let right = r + c * inv_sqrt2 + rs * inv_sqrt2;
            out.push(left * 0.7);
            out.push(right * 0.7);
        }
    } else {
        for i in 0..frames {
            let offset = i * channels;
            out.push(samples[offset]);
            out.push(samples[offset + 1.min(channels - 1)]);
        }
    }
    out
}

fn downmix_to_mono(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels == 1 {
        return samples.to_vec();
    }
    let frames = samples.len() / channels;
    let mut out = Vec::with_capacity(frames);
    let scale = 1.0 / channels as f32;
    for i in 0..frames {
        let offset = i * channels;
        let mut sum = 0.0f32;
        for ch in 0..channels {
            sum += samples[offset + ch];
        }
        out.push(sum * scale);
    }
    out
}

/// Decode every PCM frame into one interleaved buffer.
///
/// Export is a control-path operation. Keeping the samples together makes the
/// resampler's window and the encoder's frame accounting exact, at the cost of
/// holding the track in memory; this is the same trade-off offline rendering
/// tools make, and it is never on the playback thread.
fn decode_all(input: &Path) -> Result<(Vec<f32>, usize, u32, f32), MediaError> {
    let mut decoder = Decoder::open(input).map_err(|error| MediaError::Decode {
        path: input.to_path_buf(),
        message: error.to_string(),
    })?;
    let info = decoder.info();
    let channels = info.channels;
    let sample_rate = info.sample_rate;
    let duration_secs = info.duration_secs;
    if channels == 0 || sample_rate == 0 {
        return Err(MediaError::Decode {
            path: input.to_path_buf(),
            message: "the file has no usable audio format".to_string(),
        });
    }

    let mut samples = Vec::new();
    loop {
        match decoder.decode_next(16_384) {
            Ok(chunk) => {
                if chunk.raw_dsd.is_some() {
                    return Err(MediaError::Unsupported(
                        "raw DSD transport cannot be exported; decode it to PCM first".to_string(),
                    ));
                }
                samples.extend_from_slice(&chunk.samples);
            }
            Err(engine::decode::DecodeError::EndOfStream) => break,
            Err(error) => {
                return Err(MediaError::Decode {
                    path: input.to_path_buf(),
                    message: error.to_string(),
                })
            }
        }
    }
    Ok((samples, channels, sample_rate, duration_secs))
}

fn copy_editorial_tags(input: &Path, output: &Path) -> Result<bool, String> {
    let tags = engine::decode::extract_track_metadata(input);
    let year = tags.date.trim().get(0..4).and_then(|s| s.parse().ok());
    let editorial = EditorialTags {
        title: non_empty(tags.title),
        artist: non_empty(tags.artist),
        album: non_empty(tags.album),
        genre: non_empty(tags.genre),
        year,
        track_number: (tags.track_number > 0).then_some(tags.track_number),
        disc_number: (tags.disc_number > 0).then_some(tags.disc_number),
    };
    let has_any = editorial.title.is_some()
        || editorial.artist.is_some()
        || editorial.album.is_some()
        || editorial.genre.is_some()
        || editorial.year.is_some()
        || editorial.track_number.is_some()
        || editorial.disc_number.is_some();
    if !has_any {
        return Ok(false);
    }
    engine::decode::write_editorial_tags(output, &editorial)
        .map(|()| true)
        .map_err(|error| error.to_string())
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty() && !trimmed.starts_with("Unknown ")).then(|| trimmed.to_string())
}

// ── libsndfile ──────────────────────────────────────────────────────────────

#[repr(C)]
struct SfInfo {
    frames: i64,
    samplerate: i32,
    channels: i32,
    format: i32,
    sections: i32,
    seekable: i32,
}

struct SndFile {
    _library: libloading::Library,
    open: unsafe extern "C" fn(*const c_char, c_int, *mut SfInfo) -> *mut c_void,
    writef_float: unsafe extern "C" fn(*mut c_void, *const f32, i64) -> i64,
    close: unsafe extern "C" fn(*mut c_void) -> c_int,
    strerror: unsafe extern "C" fn(*mut c_void) -> *const c_char,
    format_check: unsafe extern "C" fn(*mut SfInfo) -> c_int,
}

impl SndFile {
    fn load() -> Result<Self, MediaError> {
        let names = if cfg!(target_os = "windows") {
            vec!["libsndfile-1.dll".to_string(), "sndfile.dll".to_string()]
        } else if cfg!(target_os = "macos") {
            vec![
                "libsndfile.1.dylib".to_string(),
                "libsndfile.dylib".to_string(),
            ]
        } else {
            dynlib::linux_macos_candidates("sndfile", "1")
        };
        let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();
        let library = dynlib::open("Transcoding", "libsndfile", &borrowed)?;
        unsafe {
            let open = *library
                .get::<unsafe extern "C" fn(*const c_char, c_int, *mut SfInfo) -> *mut c_void>(
                    b"sf_open\0",
                )
                .map_err(|e| symbol_error("sf_open", e))?;
            let writef_float = *library
                .get::<unsafe extern "C" fn(*mut c_void, *const f32, i64) -> i64>(
                    b"sf_writef_float\0",
                )
                .map_err(|e| symbol_error("sf_writef_float", e))?;
            let close = *library
                .get::<unsafe extern "C" fn(*mut c_void) -> c_int>(b"sf_close\0")
                .map_err(|e| symbol_error("sf_close", e))?;
            let strerror = *library
                .get::<unsafe extern "C" fn(*mut c_void) -> *const c_char>(b"sf_strerror\0")
                .map_err(|e| symbol_error("sf_strerror", e))?;
            let format_check = *library
                .get::<unsafe extern "C" fn(*mut SfInfo) -> c_int>(b"sf_format_check\0")
                .map_err(|e| symbol_error("sf_format_check", e))?;
            Ok(Self {
                _library: library,
                open,
                writef_float,
                close,
                strerror,
                format_check,
            })
        }
    }

    fn error_message(&self, handle: *mut c_void) -> String {
        unsafe {
            let pointer = (self.strerror)(handle);
            if pointer.is_null() {
                "unknown libsndfile error".to_string()
            } else {
                CStr::from_ptr(pointer).to_string_lossy().into_owned()
            }
        }
    }
}

fn symbol_error(name: &str, error: libloading::Error) -> MediaError {
    MediaError::LibraryMissing {
        what: "Transcoding",
        library: "libsndfile",
        detail: format!("symbol {name}: {error}"),
    }
}

fn encode_sndfile(
    path: &Path,
    samples: &[f32],
    channels: usize,
    sample_rate: u32,
    format: OutputFormat,
) -> Result<(), MediaError> {
    let api = SndFile::load()?;
    let format_code = format.libsndfile_format()?;
    let path_text = path.to_string_lossy().into_owned();
    let c_path = CString::new(path_text.as_bytes()).map_err(|_| {
        MediaError::InvalidInput("the output path contains an interior NUL byte".to_string())
    })?;
    let mut info = SfInfo {
        frames: 0,
        samplerate: sample_rate as i32,
        channels: channels as i32,
        format: format_code,
        sections: 0,
        seekable: 0,
    };
    let supported = unsafe { (api.format_check)(&mut info) };
    if supported == 0 {
        return Err(MediaError::Encode {
            path: path.to_path_buf(),
            message: format!(
                "the installed libsndfile cannot write {} at {} Hz with {} channel(s)",
                format.label(),
                sample_rate,
                channels
            ),
        });
    }
    let handle = unsafe { (api.open)(c_path.as_ptr(), SFM_WRITE, &mut info) };
    if handle.is_null() {
        return Err(MediaError::Encode {
            path: path.to_path_buf(),
            message: api.error_message(handle),
        });
    }

    let mut written = 0usize;
    let mut error = None;
    while written < samples.len() {
        let frames = ((samples.len() - written) / channels).min(16_384);
        if frames == 0 {
            break;
        }
        let wrote =
            unsafe { (api.writef_float)(handle, samples[written..].as_ptr(), frames as i64) };
        if wrote < 0 {
            error = Some(api.error_message(handle));
            break;
        }
        if wrote == 0 {
            error = Some("the encoder accepted no samples".to_string());
            break;
        }
        written += wrote as usize * channels;
    }
    let close_result = unsafe { (api.close)(handle) };
    if let Some(message) = error {
        return Err(MediaError::Encode {
            path: path.to_path_buf(),
            message,
        });
    }
    if close_result != 0 {
        return Err(MediaError::Encode {
            path: path.to_path_buf(),
            message: format!("libsndfile could not finalize the file (code {close_result})"),
        });
    }
    Ok(())
}

/// A unique temporary file beside `output`, so the final rename is atomic on
/// the same filesystem.
fn temporary_sibling(output: &Path) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let name = output
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "export".to_string());
    output.with_file_name(format!(".{name}.{}-{stamp}.part", std::process::id()))
}

// ── Resampling ──────────────────────────────────────────────────────────────

/// Half the sinc kernel width, in source samples. 24 zero crossings is a
/// deliberate quality/latency trade-off for offline export; the kernel is
/// evaluated ahead of time and windowed with Blackman-Harris.
const RESAMPLE_HALF_TAPS: isize = 24;

/// Resample interleaved audio with a windowed-sinc low-pass filter.
///
/// This is not a linear interpolator dressed up as a resampler: every output
/// sample sums the source samples around its fractional position through a
/// sinc kernel, scaled to the lower Nyquist frequency when downsampling. A
/// Blackman-Harris window suppresses the truncation sidelobes. The function is
/// deterministic and allocation-bounded, and is covered by tests for DC level,
/// frequency preservation and frame count.
pub fn resample_interleaved(
    samples: &[f32],
    channels: usize,
    source_rate: u32,
    target_rate: u32,
) -> Vec<f32> {
    if channels == 0 || source_rate == 0 || target_rate == 0 || source_rate == target_rate {
        return samples.to_vec();
    }
    let source_frames = samples.len() / channels;
    if source_frames == 0 {
        return Vec::new();
    }
    let ratio = target_rate as f64 / source_rate as f64;
    let target_frames = (source_frames as f64 * ratio).round().max(1.0) as usize;
    let cutoff = ratio.min(1.0) * 0.96;
    let mut output = vec![0.0f32; target_frames * channels];

    for out_frame in 0..target_frames {
        let position = out_frame as f64 / ratio;
        let centre = position.floor() as isize;
        let first = centre - RESAMPLE_HALF_TAPS + 1;
        let last = centre + RESAMPLE_HALF_TAPS;
        let mut weights = Vec::with_capacity((last - first + 1) as usize);
        let mut sum = 0.0f64;
        for tap in first..=last {
            let distance = position - tap as f64;
            if distance.abs() > RESAMPLE_HALF_TAPS as f64 {
                weights.push(0.0);
                continue;
            }
            let sinc = if distance.abs() < 1e-12 {
                1.0
            } else {
                let x = std::f64::consts::PI * distance * cutoff;
                x.sin() / x
            };
            // Blackman-Harris over the kernel support.
            let normalised = (distance / RESAMPLE_HALF_TAPS as f64 + 1.0) * 0.5;
            let window = 0.35875 - 0.48829 * (std::f64::consts::TAU * normalised).cos()
                + 0.14128 * (2.0 * std::f64::consts::TAU * normalised).cos()
                - 0.01168 * (3.0 * std::f64::consts::TAU * normalised).cos();
            let weight = sinc * cutoff * window;
            weights.push(weight);
            sum += weight;
        }
        if sum.abs() > 1e-12 {
            for weight in &mut weights {
                *weight /= sum;
            }
        }

        for channel in 0..channels {
            let mut value = 0.0f64;
            for (index, tap) in (first..=last).enumerate() {
                // Replicate the edge sample instead of dropping the tap. The
                // weights are normalised to sum to one, so dropping edge taps
                // would make a constant signal fade in and out at the ends of
                // the file; clamping keeps DC exact.
                let tap = tap.clamp(0, source_frames as isize - 1);
                let sample = samples[tap as usize * channels + channel] as f64;
                value += sample * weights[index];
            }
            output[out_frame * channels + channel] = value.clamp(-1.0, 1.0) as f32;
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: u32, frames: usize, channels: usize) -> Vec<f32> {
        let mut out = Vec::with_capacity(frames * channels);
        for frame in 0..frames {
            let value = (std::f32::consts::TAU * freq * frame as f32 / rate as f32).sin() * 0.5;
            for _ in 0..channels {
                out.push(value);
            }
        }
        out
    }

    #[test]
    fn resampling_preserves_dc_and_channels() {
        let input = vec![0.25f32; 1_000 * 2];
        let output = resample_interleaved(&input, 2, 44_100, 48_000);
        assert_eq!(output.len() % 2, 0);
        assert!(((output.len() / 2) as isize - 1_088).abs() <= 1);
        for (index, sample) in output.iter().enumerate() {
            assert!(
                (*sample - 0.25).abs() < 0.01,
                "sample {index} drifted to {sample}"
            );
        }
    }

    #[test]
    fn resampling_keeps_a_midband_sine_at_its_frequency() {
        let rate = 48_000;
        let input = sine(1_000.0, rate, rate as usize / 2, 1);
        let output = resample_interleaved(&input, 1, rate, 96_000);
        assert_eq!(output.len(), rate as usize);

        // Count zero crossings in the steady middle: 1 kHz over 0.25 s is 250
        // cycles, or 500 crossings.
        let middle = &output[12_000..36_000];
        let crossings = middle
            .windows(2)
            .filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
            .count();
        assert!(
            (crossings as i32 - 250).abs() <= 3,
            "expected 250 cycles, counted {crossings}"
        );
    }

    #[test]
    fn same_rate_resampling_is_exact() {
        let input = sine(440.0, 44_100, 441, 2);
        assert_eq!(
            resample_interleaved(&input, 2, 44_100, 44_100),
            input,
            "a same-rate request must not touch a sample"
        );
    }

    #[test]
    fn opus_is_always_48k_and_flac_keeps_the_rate() {
        assert_eq!(OutputFormat::Opus.sample_rate(44_100), 48_000);
        assert_eq!(
            OutputFormat::Flac { bit_depth: 24 }.sample_rate(96_000),
            96_000
        );
    }

    #[test]
    fn invalid_flac_depth_is_refused_before_touching_the_encoder() {
        let error = OutputFormat::Flac { bit_depth: 20 }
            .libsndfile_format()
            .unwrap_err();
        assert!(error.to_string().contains("16 or 24"));
    }

    #[test]
    fn the_temporary_file_is_a_sibling_with_a_part_suffix() {
        let output = Path::new("/music/Album/track.flac");
        let temp = temporary_sibling(output);
        assert_eq!(temp.parent(), output.parent());
        assert!(temp
            .file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with(".part"));
        assert_ne!(temp, output);
    }

    #[test]
    fn format_definitions_and_extensions_are_consistent() {
        for fmt in OutputFormat::ALL {
            assert!(!fmt.extension().is_empty());
            assert!(!fmt.label().is_empty());
            assert!(fmt.libsndfile_format().is_ok());
        }
    }

    #[test]
    fn channel_downmix_transforms_buffers_as_expected() {
        let stereo = vec![1.0, 0.5, 0.8, 0.2];
        let mono = downmix_to_mono(&stereo, 2);
        assert_eq!(mono.len(), 2);
        assert!((mono[0] - 0.75).abs() < 1e-4);
        assert!((mono[1] - 0.50).abs() < 1e-4);

        let reconstructed = downmix_to_stereo(&mono, 1);
        assert_eq!(reconstructed.len(), 4);
        assert_eq!(reconstructed, vec![0.75, 0.75, 0.5, 0.5]);
    }
}
