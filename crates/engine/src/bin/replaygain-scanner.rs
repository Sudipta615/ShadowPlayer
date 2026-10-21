//! `replaygain-scanner` — offline loudness measurement for the engine.
//!
//! The engine README documents this binary as the way to "execute an
//! automated EBU R128 loudness and true-peak scan". It measures a file (or a
//! whole directory tree) with the engine's *own* loudness meter — the same
//! [`engine::dsp::LoudnessMeter`] the playback chain uses — so the numbers
//! this tool prints are the numbers the engine would normalise against, not a
//! second, drifting estimate.
//!
//! ## What it measures
//!
//! * **EBU R128** (ITU-R BS.1770-5): integrated loudness in LUFS, true peak
//!   in dBTP (the shared 4× FIR estimate, not a sample peak) and the loudness
//!   range in LU.
//! * **ReplayGain 2.0**: track gain in dB and track peak (linear amplitude),
//!   plus the album gain computed with ReplayGain 2.0's power-mean
//!   definition over the scanned set.
//!
//! ## Tag write-back
//!
//! `--write-tags` stores the measured values back into each file with the
//! engine's own writer ([`engine::decode::write_loudness_tags`]), so the tags
//! a player later reads are the tags this tool measured.

use std::path::{Path, PathBuf};

use engine::decode::{scan_track_loudness, LoudnessScanResult};
use engine::dsp::LoudnessMetadata;

const USAGE: &str = "\
replaygain-scanner — measure EBU R128 / ReplayGain 2.0 loudness and write the tags

USAGE:
    replaygain-scanner --file <PATH>
    replaygain-scanner --dir <DIR> [--recursive]

OPTIONS:
    --file <PATH>       Analyse a single file
    --dir <DIR>         Analyse the audio files in a directory
    --recursive         Descend into subdirectories (with --dir)
    --write-tags        Write the measured values back into the files' tags
    --format <json|csv|text>
                        Output format (default: text)
    --r128              Report only the EBU R128 measurements
    --replaygain        Report only the ReplayGain 2.0 measurements
    -h, --help          Print this help and exit

EXIT STATUS:
    0  at least one file was analysed
    1  nothing could be analysed
    2  usage error
";

/// The output format of the report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Text,
    Csv,
    Json,
}

impl Format {
    fn parse(value: &str) -> Result<Self, String> {
        match value.to_ascii_lowercase().as_str() {
            "text" => Ok(Self::Text),
            "csv" => Ok(Self::Csv),
            "json" => Ok(Self::Json),
            other => Err(format!(
                "unknown --format '{other}' (expected json|csv|text)"
            )),
        }
    }
}

/// Which measurement family to report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Selection {
    r128: bool,
    replaygain: bool,
}

impl Selection {
    /// Both families are reported unless one is explicitly selected.
    fn from_flags(r128: bool, replaygain: bool) -> Self {
        if r128 || replaygain {
            Self { r128, replaygain }
        } else {
            Self {
                r128: true,
                replaygain: true,
            }
        }
    }
}

/// The parsed command line.
#[derive(Debug, Clone, PartialEq)]
struct Cli {
    file: Option<PathBuf>,
    dir: Option<PathBuf>,
    recursive: bool,
    write_tags: bool,
    format: Format,
    selection: Selection,
    help: bool,
}

/// Parse the command line. Pure — no I/O, no global state.
fn parse_args(args: &[String]) -> Result<Cli, String> {
    let mut file = None;
    let mut dir = None;
    let mut recursive = false;
    let mut write_tags = false;
    let mut format = Format::Text;
    let mut r128 = false;
    let mut replaygain = false;
    let mut help = false;

    let mut index = 0usize;
    while index < args.len() {
        let arg = args[index].as_str();
        let (name, inline) = match arg.split_once('=') {
            Some((head, tail)) if head.starts_with("--") => (head, Some(tail.to_string())),
            _ => (arg, None),
        };
        let mut take_value = |option: &str| -> Result<String, String> {
            if let Some(value) = inline.clone() {
                return Ok(value);
            }
            index += 1;
            args.get(index)
                .cloned()
                .ok_or_else(|| format!("option '{option}' requires a value"))
        };

        match name {
            "-h" | "--help" => help = true,
            "--recursive" => recursive = true,
            "--write-tags" => write_tags = true,
            "--r128" => r128 = true,
            "--replaygain" => replaygain = true,
            "--file" => {
                let value = take_value(name)?;
                file = Some(PathBuf::from(value));
            }
            "--dir" => {
                let value = take_value(name)?;
                dir = Some(PathBuf::from(value));
            }
            "--format" => {
                let value = take_value(name)?;
                format = Format::parse(&value)?;
            }
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown option '{other}'"));
            }
            other => return Err(format!("unexpected argument '{other}'")),
        }
        index += 1;
    }

    if help {
        return Ok(Cli {
            file,
            dir,
            recursive,
            write_tags,
            format,
            selection: Selection::from_flags(r128, replaygain),
            help,
        });
    }

    if file.is_some() && dir.is_some() {
        return Err("--file and --dir are mutually exclusive".to_string());
    }
    if file.is_none() && dir.is_none() {
        return Err("either --file <PATH> or --dir <DIR> is required".to_string());
    }
    if recursive && dir.is_none() {
        return Err("--recursive only applies to --dir".to_string());
    }

    Ok(Cli {
        file,
        dir,
        recursive,
        write_tags,
        format,
        selection: Selection::from_flags(r128, replaygain),
        help,
    })
}

/// File extensions the scanner considers to be audio.
///
/// Anything else in a scanned directory is *ignored* (counted separately from
/// the skipped files) so a music folder full of cover art does not report
/// hundreds of "failed" tracks.
const AUDIO_EXTENSIONS: &[&str] = &[
    "flac", "wav", "wave", "mp3", "ogg", "oga", "opus", "m4a", "mp4", "m4b", "aac", "alac", "ape",
    "apl", "mac", "wv", "aiff", "aif", "aifc", "tta", "mka", "mkv", "dsf", "dff",
];

/// True when the path looks like an audio file the scanner should analyse.
fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let e = e.to_ascii_lowercase();
            AUDIO_EXTENSIONS.contains(&e.as_str())
        })
        .unwrap_or(false)
}

/// One analysed file.
struct FileReport {
    path: PathBuf,
    scan: LoudnessScanResult,
    /// Whether the tags were written successfully.
    tags_written: bool,
    /// Why the tags were not written (if they were not).
    tag_error: Option<String>,
}

/// Counts collected while scanning.
#[derive(Debug, Default, Clone, Copy)]
struct Counts {
    analysed: usize,
    /// Audio files that could not be decoded or measured.
    skipped: usize,
    /// Files that are not audio and were therefore not considered at all.
    ignored: usize,
    /// Files whose tags could not be written.
    tag_failures: usize,
}

/// Collect the files to analyse.
fn collect_files(cli: &Cli) -> Result<Vec<PathBuf>, String> {
    if let Some(file) = &cli.file {
        if !file.exists() {
            return Err(format!("no such file: {}", file.display()));
        }
        return Ok(vec![file.clone()]);
    }

    let dir = cli.dir.as_ref().expect("checked by parse_args");
    if !dir.is_dir() {
        return Err(format!("not a directory: {}", dir.display()));
    }

    let mut files = Vec::new();
    let mut stack = vec![dir.clone()];
    while let Some(current) = stack.pop() {
        let entries = std::fs::read_dir(&current)
            .map_err(|e| format!("cannot read directory {}: {e}", current.display()))?;
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(_) => continue,
            };
            if file_type.is_dir() {
                if cli.recursive {
                    stack.push(path);
                }
            } else if file_type.is_file() {
                files.push(path);
            }
        }
    }
    // A stable order makes the report diffable between runs.
    files.sort();
    Ok(files)
}

/// Analyse one file and (optionally) write its tags back.
fn analyse(path: &Path, write_tags: bool) -> Option<FileReport> {
    let scan = scan_track_loudness(path)?;
    let mut tags_written = false;
    let mut tag_error = None;
    if write_tags {
        let metadata = LoudnessMetadata {
            replaygain_track_db: scan.replaygain_track_db,
            replaygain_album_db: None,
            replaygain_track_peak: scan.replaygain_track_peak,
            replaygain_album_peak: None,
            ebu_r128_loudness: scan.ebu_r128_loudness,
            ebu_r128_peak: scan.ebu_r128_peak_dbtp,
        };
        match engine::decode::write_loudness_tags(path, &metadata) {
            Ok(()) => tags_written = true,
            Err(error) => tag_error = Some(error.to_string()),
        }
    }
    Some(FileReport {
        path: path.to_path_buf(),
        scan,
        tags_written,
        tag_error,
    })
}

/// Run the scan over the selected files and print the report.
fn run(cli: &Cli) -> i32 {
    let files = match collect_files(cli) {
        Ok(files) => files,
        Err(message) => {
            eprintln!("replaygain-scanner: {message}");
            return 2;
        }
    };

    let mut counts = Counts::default();
    let mut reports: Vec<FileReport> = Vec::new();

    for path in &files {
        // An explicitly named file is always attempted — the user asked for
        // that file. The extension filter only applies to a directory walk,
        // where most entries are not audio at all.
        if cli.file.is_none() && !is_audio_file(path) {
            counts.ignored += 1;
            continue;
        }
        match analyse(path, cli.write_tags) {
            Some(report) => {
                if report.tag_error.is_some() {
                    counts.tag_failures += 1;
                }
                counts.analysed += 1;
                reports.push(report);
            }
            None => {
                counts.skipped += 1;
                eprintln!(
                    "replaygain-scanner: skipped {} (unreadable or no measurable audio)",
                    path.display()
                );
            }
        }
    }

    if reports.is_empty() {
        eprintln!(
            "replaygain-scanner: nothing could be analysed ({} skipped, {} ignored)",
            counts.skipped, counts.ignored
        );
        return 1;
    }

    // The album gain is defined over the *set* of tracks (ReplayGain 2.0's
    // power mean over the tracks' mean squares), so it can only be computed
    // once every track has been measured.
    let album =
        engine::decode::accumulate_album_replaygain(reports.iter().map(|report| &report.scan));

    // When tags are requested, the album values are written as well — a
    // player in album mode needs `REPLAYGAIN_ALBUM_GAIN` to preserve the
    // relative loudness between tracks.
    if cli.write_tags {
        if let Some(album) = &album {
            for report in &mut reports {
                let metadata = LoudnessMetadata {
                    replaygain_track_db: report.scan.replaygain_track_db,
                    replaygain_album_db: Some(album.album_gain_db),
                    replaygain_track_peak: report.scan.replaygain_track_peak,
                    replaygain_album_peak: Some(album.album_peak),
                    ebu_r128_loudness: report.scan.ebu_r128_loudness,
                    ebu_r128_peak: report.scan.ebu_r128_peak_dbtp,
                };
                match engine::decode::write_loudness_tags(&report.path, &metadata) {
                    Ok(()) => report.tags_written = true,
                    Err(error) => {
                        report.tag_error = Some(error.to_string());
                        counts.tag_failures += 1;
                    }
                }
            }
        }
    }

    print_report(cli, &reports, album.as_ref(), &counts);

    if counts.analysed == 0 {
        1
    } else {
        0
    }
}

/// Print the report in the requested format.
fn print_report(
    cli: &Cli,
    reports: &[FileReport],
    album: Option<&engine::decode::AlbumReplayGain>,
    counts: &Counts,
) {
    match cli.format {
        Format::Text => print_text(cli, reports, album, counts),
        Format::Csv => print_csv(cli, reports, album),
        Format::Json => print_json(cli, reports, album, counts),
    }
}

/// Human-readable report.
fn print_text(
    cli: &Cli,
    reports: &[FileReport],
    album: Option<&engine::decode::AlbumReplayGain>,
    counts: &Counts,
) {
    for report in reports {
        println!("{}", report.path.display());
        if cli.selection.r128 {
            println!(
                "  EBU R128: {} LUFS, true peak {} dBTP, LRA {} LU",
                format_optional(report.scan.ebu_r128_loudness, 2),
                format_optional(report.scan.ebu_r128_peak_dbtp, 2),
                format_optional(report.scan.lra_lu, 2),
            );
        }
        if cli.selection.replaygain {
            println!(
                "  ReplayGain 2.0: {} dB, peak {}",
                format_optional(report.scan.replaygain_track_db, 2),
                format_optional(report.scan.replaygain_track_peak, 6),
            );
        }
        if cli.write_tags {
            match &report.tag_error {
                None => println!("  tags: written"),
                Some(error) => println!("  tags: not written ({error})"),
            }
        }
    }
    if let Some(album) = album {
        if cli.selection.replaygain {
            println!();
            println!(
                "album gain: {:.2} dB, album peak: {:.6} ({} tracks)",
                album.album_gain_db, album.album_peak, album.track_count
            );
        }
    }
    println!();
    println!(
        "{} analysed, {} skipped, {} ignored, {} tag write failures",
        counts.analysed, counts.skipped, counts.ignored, counts.tag_failures
    );
}

/// CSV report: one header row, one row per analysed file.
fn print_csv(cli: &Cli, reports: &[FileReport], album: Option<&engine::decode::AlbumReplayGain>) {
    let mut header = vec!["path".to_string()];
    if cli.selection.r128 {
        header.push("r128_lufs".to_string());
        header.push("r128_true_peak_dbtp".to_string());
        header.push("lra_lu".to_string());
    }
    if cli.selection.replaygain {
        header.push("replaygain_track_gain_db".to_string());
        header.push("replaygain_track_peak".to_string());
    }
    if album.is_some() && cli.selection.replaygain {
        header.push("album_gain_db".to_string());
        header.push("album_peak".to_string());
    }
    header.push("tags_written".to_string());
    println!("{}", header.join(","));

    for report in reports {
        let mut row = vec![csv_field(&report.path.display().to_string())];
        if cli.selection.r128 {
            row.push(csv_number(report.scan.ebu_r128_loudness));
            row.push(csv_number(report.scan.ebu_r128_peak_dbtp));
            row.push(csv_number(report.scan.lra_lu));
        }
        if cli.selection.replaygain {
            row.push(csv_number(report.scan.replaygain_track_db));
            row.push(csv_number(report.scan.replaygain_track_peak));
        }
        if let Some(album) = album {
            if cli.selection.replaygain {
                row.push(csv_number(Some(album.album_gain_db)));
                row.push(csv_number(Some(album.album_peak)));
            }
        }
        row.push(if report.tag_error.is_some() {
            "error".to_string()
        } else if report.tags_written {
            "true".to_string()
        } else {
            "false".to_string()
        });
        println!("{}", row.join(","));
    }
}

/// JSON report: one object per line (newline-delimited JSON).
fn print_json(
    cli: &Cli,
    reports: &[FileReport],
    album: Option<&engine::decode::AlbumReplayGain>,
    counts: &Counts,
) {
    for report in reports {
        let mut map = serde_json::Map::new();
        map.insert("event".to_string(), serde_json::Value::from("track"));
        map.insert(
            "path".to_string(),
            serde_json::Value::from(report.path.display().to_string()),
        );
        if cli.selection.r128 {
            let mut r128 = serde_json::Map::new();
            r128.insert(
                "integrated_lufs".to_string(),
                json_number(report.scan.ebu_r128_loudness),
            );
            r128.insert(
                "true_peak_dbtp".to_string(),
                json_number(report.scan.ebu_r128_peak_dbtp),
            );
            r128.insert("lra_lu".to_string(), json_number(report.scan.lra_lu));
            map.insert("r128".to_string(), serde_json::Value::Object(r128));
        }
        if cli.selection.replaygain {
            let mut replaygain = serde_json::Map::new();
            replaygain.insert(
                "track_gain_db".to_string(),
                json_number(report.scan.replaygain_track_db),
            );
            replaygain.insert(
                "track_peak".to_string(),
                json_number(report.scan.replaygain_track_peak),
            );
            map.insert(
                "replaygain".to_string(),
                serde_json::Value::Object(replaygain),
            );
        }
        if let Some(album) = album {
            if cli.selection.replaygain {
                let mut album_map = serde_json::Map::new();
                album_map.insert(
                    "gain_db".to_string(),
                    json_number(Some(album.album_gain_db)),
                );
                album_map.insert("peak".to_string(), json_number(Some(album.album_peak)));
                album_map.insert(
                    "track_count".to_string(),
                    serde_json::Value::from(album.track_count),
                );
                map.insert("album".to_string(), serde_json::Value::Object(album_map));
            }
        }
        map.insert(
            "tags_written".to_string(),
            serde_json::Value::from(report.tags_written),
        );
        if let Some(error) = &report.tag_error {
            map.insert(
                "tag_error".to_string(),
                serde_json::Value::from(error.clone()),
            );
        }
        println!("{}", serde_json::Value::Object(map));
    }

    let mut summary = serde_json::Map::new();
    summary.insert("event".to_string(), serde_json::Value::from("summary"));
    summary.insert(
        "analysed".to_string(),
        serde_json::Value::from(counts.analysed),
    );
    summary.insert(
        "skipped".to_string(),
        serde_json::Value::from(counts.skipped),
    );
    summary.insert(
        "ignored".to_string(),
        serde_json::Value::from(counts.ignored),
    );
    summary.insert(
        "tag_failures".to_string(),
        serde_json::Value::from(counts.tag_failures),
    );
    println!("{}", serde_json::Value::Object(summary));
}

/// Format an optional measurement for the text report.
fn format_optional(value: Option<f32>, decimals: usize) -> String {
    match value {
        Some(value) if value.is_finite() => format!("{value:.decimals$}"),
        _ => "n/a".to_string(),
    }
}

/// Format an optional measurement for the CSV report.
fn csv_number(value: Option<f32>) -> String {
    match value {
        Some(value) if value.is_finite() => format!("{value:.4}"),
        _ => String::new(),
    }
}

/// Format an optional measurement for the JSON report.
///
/// A non-finite measurement is `null`, never a fabricated number.
fn json_number(value: Option<f32>) -> serde_json::Value {
    match value {
        Some(value) if value.is_finite() => serde_json::Value::from(f64::from(value)),
        _ => serde_json::Value::Null,
    }
}

/// Escape one CSV field (RFC 4180).
fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cli = match parse_args(&args) {
        Ok(cli) => cli,
        Err(message) => {
            eprintln!("replaygain-scanner: {message}");
            eprintln!();
            eprint!("{USAGE}");
            std::process::exit(2);
        }
    };
    if cli.help {
        print!("{USAGE}");
        std::process::exit(0);
    }
    std::process::exit(run(&cli));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn a_single_file_is_parsed() {
        let cli = parse_args(&args(&["--file", "/tmp/a.flac"])).expect("parse");
        assert_eq!(cli.file, Some(PathBuf::from("/tmp/a.flac")));
        assert_eq!(cli.dir, None);
        assert!(!cli.recursive);
        assert!(!cli.write_tags);
        assert_eq!(cli.format, Format::Text);
        assert_eq!(
            cli.selection,
            Selection {
                r128: true,
                replaygain: true
            }
        );
    }

    #[test]
    fn a_directory_with_recursion_is_parsed() {
        let cli = parse_args(&args(&["--dir", "/music", "--recursive"])).expect("parse");
        assert_eq!(cli.dir, Some(PathBuf::from("/music")));
        assert!(cli.recursive);
        assert_eq!(cli.file, None);
    }

    #[test]
    fn the_output_format_is_validated() {
        assert_eq!(
            parse_args(&args(&["--file", "a.flac", "--format", "csv"]))
                .expect("parse")
                .format,
            Format::Csv
        );
        assert_eq!(
            parse_args(&args(&["--file", "a.flac", "--format", "JSON"]))
                .expect("parse")
                .format,
            Format::Json
        );
        let error = parse_args(&args(&["--file", "a.flac", "--format", "xml"])).unwrap_err();
        assert!(error.contains("unknown --format"), "{error}");
    }

    #[test]
    fn the_measurement_selection_is_honoured() {
        let cli = parse_args(&args(&["--file", "a.flac", "--r128"])).expect("parse");
        assert_eq!(
            cli.selection,
            Selection {
                r128: true,
                replaygain: false
            }
        );
        let cli = parse_args(&args(&["--file", "a.flac", "--replaygain"])).expect("parse");
        assert_eq!(
            cli.selection,
            Selection {
                r128: false,
                replaygain: true
            }
        );
    }

    #[test]
    fn write_tags_is_parsed() {
        let cli = parse_args(&args(&["--file", "a.flac", "--write-tags"])).expect("parse");
        assert!(cli.write_tags);
    }

    #[test]
    fn a_target_is_required() {
        let error = parse_args(&args(&[])).unwrap_err();
        assert!(error.contains("--file"), "{error}");
    }

    #[test]
    fn file_and_dir_are_mutually_exclusive() {
        let error = parse_args(&args(&["--file", "a.flac", "--dir", "/m"])).unwrap_err();
        assert!(error.contains("mutually exclusive"), "{error}");
    }

    #[test]
    fn recursive_requires_a_directory() {
        let error = parse_args(&args(&["--file", "a.flac", "--recursive"])).unwrap_err();
        assert!(error.contains("--recursive"), "{error}");
    }

    #[test]
    fn unknown_options_are_rejected() {
        let error = parse_args(&args(&["--file", "a.flac", "--nope"])).unwrap_err();
        assert!(error.contains("unknown option"), "{error}");
    }

    #[test]
    fn audio_extensions_are_recognised() {
        assert!(is_audio_file(Path::new("track.flac")));
        assert!(is_audio_file(Path::new("track.FLAC")));
        assert!(is_audio_file(Path::new("track.opus")));
        assert!(!is_audio_file(Path::new("cover.jpg")));
        assert!(!is_audio_file(Path::new("track")));
    }

    #[test]
    fn csv_fields_are_escaped() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn optional_numbers_are_formatted_honestly() {
        assert_eq!(format_optional(Some(-14.25), 2), "-14.25");
        assert_eq!(format_optional(None, 2), "n/a");
        assert_eq!(format_optional(Some(f32::NAN), 2), "n/a");
        assert_eq!(csv_number(None), "");
        assert!(json_number(None).is_null());
        assert!(json_number(Some(f32::INFINITY)).is_null());
    }
}
