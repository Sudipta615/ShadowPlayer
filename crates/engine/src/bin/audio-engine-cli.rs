//! `audio-engine-cli` — the engine's scriptable, non-interactive player and
//! inspector.
//!
//! The engine README documents this binary as "a fast REPL for scripted
//! playback, benchmark runs, and endpoint configuration". This is the
//! non-interactive half of that promise: every capability is reachable from a
//! shell script, and every run is reproducible (no hidden REPL state, no
//! terminal-dependent behaviour).
//!
//! ## What it does
//!
//! * `--list-devices` enumerates the output devices of the requested backend
//!   through the engine's own device enumeration
//!   ([`engine::output::cpal_devices::enumerate_devices`]).
//! * `probe <PATH>` opens a file with the engine's decoder
//!   ([`engine::decode::Decoder`]) and prints the decoded format plus the
//!   engine's real bit-perfect report
//!   ([`engine::dsp::pipeline::DspPipeline::bit_perfect_report_with_format`]).
//! * `selftest` runs the engine's conformance suites
//!   ([`engine::eval::run_quality`]) headlessly and exits non-zero on failure.
//! * Anything else plays the given paths through a real
//!   [`engine::AudioEngine`], reporting transport state changes as they
//!   happen.
//!
//! ## JSON mode
//!
//! `--json` switches stdout to *newline-delimited JSON*: exactly one JSON
//! object per line, one line per event. This is the shape `jq` and every
//! line-oriented log pipeline expects — a single pretty-printed document
//! would be useless for a program that runs for hours.
//!
//! ## Honesty rules
//!
//! This binary never claims a capability the engine does not expose. Where a
//! value cannot be known without a live output device (the transport half of
//! the bit-perfect verdict, for instance) it says so instead of guessing.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use engine::dsp::pipeline::{BitPerfectReport, DspPipeline, OutputSampleFormat};
use engine::events::EngineEvent;
use engine::playback_info::PlaybackState;
use engine::{AudioBackend, AudioEngine, EngineConfig, EngineHandle, SampleRatePolicy};

const USAGE: &str = "\
audio-engine-cli — scriptable player and inspector for the Shadow audio engine

USAGE:
    audio-engine-cli [OPTIONS] <PATH>...
    audio-engine-cli [OPTIONS] probe <PATH>
    audio-engine-cli [OPTIONS] selftest

OPTIONS:
    -b, --backend <auto|alsa|wasapi|asio|coreaudio|null>
                          Output backend to request from the engine
    -d, --device <ID>     Output device name (as reported by --list-devices)
        --bit-perfect     Engage the engine's bit-perfect (pure-direct) bypass
        --rate <HZ>       Fixed output sample rate
        --gain <DB>       Master gain in dB (-60 .. 0)
        --loop            Repeat the given paths forever
        --json            Emit one JSON object per line on stdout
        --list-devices    List the output devices of the chosen backend
        --version         Print the engine version and exit
    -h, --help            Print this help and exit

SUBCOMMANDS:
    probe <PATH>          Print the decoded format and the bit-perfect report
    selftest              Run the engine's conformance suites headlessly

EXIT STATUS:
    0  success
    1  runtime failure (no output device, undecodable file, failed suite)
    2  usage error
";

/// Output backend requested on the command line.
///
/// The engine's own backend vocabulary is larger (`PipeWire`, `Jack`,
/// `Custom`); this list is the subset that maps to a backend the engine can
/// actually open on the host it is running on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Backend {
    Auto,
    Alsa,
    Wasapi,
    Asio,
    CoreAudio,
    Null,
}

impl Backend {
    /// Parse the `--backend` value. Case-insensitive.
    fn parse(value: &str) -> Result<Self, String> {
        match value.to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "alsa" => Ok(Self::Alsa),
            "wasapi" => Ok(Self::Wasapi),
            "asio" => Ok(Self::Asio),
            "coreaudio" => Ok(Self::CoreAudio),
            "null" => Ok(Self::Null),
            other => Err(format!(
                "unknown backend '{other}' (expected auto|alsa|wasapi|asio|coreaudio|null)"
            )),
        }
    }

    /// The engine backend this maps to.
    ///
    /// `ExclusiveAlsa` / `ExclusiveWasapi` / `ExclusiveCoreAudioHog` /
    /// `ExclusiveAsio` are the engine's *direct hardware* backends — the ones
    /// that can satisfy a bit-perfect request. `Null` is the engine's
    /// hardware-free sink.
    fn as_engine(self) -> AudioBackend {
        match self {
            Self::Auto => AudioBackend::Auto,
            Self::Alsa => AudioBackend::ExclusiveAlsa,
            Self::Wasapi => AudioBackend::ExclusiveWasapi,
            Self::Asio => AudioBackend::ExclusiveAsio,
            Self::CoreAudio => AudioBackend::ExclusiveCoreAudioHog,
            Self::Null => AudioBackend::Null,
        }
    }
}

/// What the parsed command line asks the binary to do.
#[derive(Debug, Clone, PartialEq)]
enum Command {
    /// Play the given paths in order.
    Play(Vec<PathBuf>),
    /// Inspect a single file and exit.
    Probe(PathBuf),
    /// Run the engine's conformance suites and exit.
    Selftest,
}

/// The fully parsed command line.
#[derive(Debug, Clone, PartialEq)]
struct Cli {
    backend: Backend,
    device: Option<String>,
    bit_perfect: bool,
    rate: Option<u32>,
    gain: Option<f32>,
    loop_playback: bool,
    json: bool,
    list_devices: bool,
    version: bool,
    help: bool,
    command: Command,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            backend: Backend::Auto,
            device: None,
            bit_perfect: false,
            rate: None,
            gain: None,
            loop_playback: false,
            json: false,
            list_devices: false,
            version: false,
            help: false,
            command: Command::Play(Vec::new()),
        }
    }
}

/// Parse the command line.
///
/// Pure: it takes the argument vector (without the program name) and returns
/// the parsed command, or a human-readable error. It touches no global state,
/// opens no file and starts no thread, so it is directly unit-testable.
///
/// Both `--option value` and `--option=value` spellings are accepted.
fn parse_args(args: &[String]) -> Result<Cli, String> {
    let mut cli = Cli::default();
    let mut positionals: Vec<String> = Vec::new();
    let mut index = 0usize;

    while index < args.len() {
        let arg = args[index].as_str();
        // `--name=value` is split here; a bare `--name` keeps `None`.
        let (name, inline) = match arg.split_once('=') {
            Some((head, tail)) if head.starts_with("--") => (head, Some(tail.to_string())),
            _ => (arg, None),
        };

        // Pull the value of an option, either from `--name=value` or from the
        // next argument.
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
            "-h" | "--help" => cli.help = true,
            "--version" => cli.version = true,
            "--json" => cli.json = true,
            "--bit-perfect" => cli.bit_perfect = true,
            "--loop" => cli.loop_playback = true,
            "--list-devices" => cli.list_devices = true,
            "-b" | "--backend" => {
                let value = take_value(name)?;
                cli.backend = Backend::parse(&value)?;
            }
            "-d" | "--device" => {
                let value = take_value(name)?;
                cli.device = Some(value);
            }
            "--rate" => {
                let value = take_value(name)?;
                cli.rate = Some(parse_rate(&value)?);
            }
            "--gain" => {
                let value = take_value(name)?;
                cli.gain = Some(parse_gain(&value)?);
            }
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown option '{other}'"));
            }
            _ => positionals.push(arg.to_string()),
        }
        index += 1;
    }

    cli.command = match positionals.first().map(String::as_str) {
        Some("probe") => {
            let rest = &positionals[1..];
            if rest.len() != 1 {
                return Err("'probe' takes exactly one <PATH>".to_string());
            }
            Command::Probe(PathBuf::from(&rest[0]))
        }
        Some("selftest") => Command::Selftest,
        _ => Command::Play(positionals.iter().map(PathBuf::from).collect()),
    };

    // `--help`, `--version` and `--list-devices` are complete on their own.
    if !cli.help && !cli.version && !cli.list_devices {
        if let Command::Play(paths) = &cli.command {
            if paths.is_empty() {
                return Err("no input path given".to_string());
            }
        }
    }

    Ok(cli)
}

/// Parse a `--rate` value: a positive integer number of Hz.
///
/// The upper bound is the highest rate the engine's own resampler families
/// define (768 kHz); anything above it is a typo, not a request.
fn parse_rate(value: &str) -> Result<u32, String> {
    let rate: u32 = value
        .parse()
        .map_err(|_| format!("invalid --rate '{value}' (expected an integer number of Hz)"))?;
    if !(8_000..=768_000).contains(&rate) {
        return Err(format!("--rate {rate} is out of range (8000 .. 768000 Hz)"));
    }
    Ok(rate)
}

/// Parse a `--gain` value: a decibel figure inside the engine's volume range.
///
/// The engine's `SetVolumeDb` command documents `[-60.0, 0.0]` dB; values
/// outside it are refused rather than silently clamped, so a typo cannot
/// quietly play 20 dB too loud.
fn parse_gain(value: &str) -> Result<f32, String> {
    let gain: f32 = value
        .parse()
        .map_err(|_| format!("invalid --gain '{value}' (expected a number of dB)"))?;
    if !gain.is_finite() {
        return Err(format!("invalid --gain '{value}' (must be finite)"));
    }
    if !(-60.0..=0.0).contains(&gain) {
        return Err(format!(
            "--gain {gain} dB is out of range (-60.0 .. 0.0 dB)"
        ));
    }
    Ok(gain)
}

/// Writes events either as newline-delimited JSON or as human-readable lines.
struct Reporter {
    json: bool,
}

impl Reporter {
    fn new(json: bool) -> Self {
        Self { json }
    }

    /// Emit one event.
    ///
    /// In JSON mode this is exactly one object on one line. In human mode the
    /// caller supplies the pre-formatted text.
    fn emit(&self, fields: Vec<(&str, serde_json::Value)>, human: &str) {
        if self.json {
            let mut map = serde_json::Map::new();
            for (key, value) in fields {
                map.insert(key.to_string(), value);
            }
            println!("{}", serde_json::Value::Object(map));
        } else {
            println!("{human}");
        }
    }

    fn opened(
        &self,
        path: &Path,
        format: &str,
        sample_rate: u32,
        channels: usize,
        duration_secs: f32,
    ) {
        self.emit(
            vec![
                ("event", serde_json::Value::from("opened")),
                ("format", serde_json::Value::from(format)),
                ("sample_rate", serde_json::Value::from(sample_rate)),
                ("channels", serde_json::Value::from(channels)),
                ("duration_secs", serde_json::Value::from(duration_secs)),
                ("path", serde_json::Value::from(path.display().to_string())),
            ],
            &format!(
                "opened: {format} {sample_rate} Hz, {channels} ch, {:.1} s — {}",
                duration_secs,
                path.display()
            ),
        );
    }

    fn state(&self, state: PlaybackState, position_secs: f32) {
        let playing = matches!(state, PlaybackState::Playing);
        self.emit(
            vec![
                ("event", serde_json::Value::from("state")),
                ("playing", serde_json::Value::from(playing)),
                ("state", serde_json::Value::from(state_label(state))),
                ("position_secs", serde_json::Value::from(position_secs)),
            ],
            &format!("state: {} ({position_secs:.1} s)", state_label(state)),
        );
    }

    fn position(&self, position_secs: f32, duration_secs: f32) {
        self.emit(
            vec![
                ("event", serde_json::Value::from("position")),
                ("position_secs", serde_json::Value::from(position_secs)),
                ("duration_secs", serde_json::Value::from(duration_secs)),
            ],
            &format!("position: {position_secs:.1} / {duration_secs:.1} s"),
        );
    }

    fn error(&self, message: &str) {
        self.emit(
            vec![
                ("event", serde_json::Value::from("error")),
                ("message", serde_json::Value::from(message)),
            ],
            &format!("error: {message}"),
        );
    }

    fn note(&self, message: &str) {
        self.emit(
            vec![
                ("event", serde_json::Value::from("note")),
                ("message", serde_json::Value::from(message)),
            ],
            &format!("note: {message}"),
        );
    }
}

/// Stable, machine-friendly name for a transport state.
fn state_label(state: PlaybackState) -> &'static str {
    match state {
        PlaybackState::Stopped => "stopped",
        PlaybackState::Playing => "playing",
        PlaybackState::Paused => "paused",
        PlaybackState::Buffering => "buffering",
    }
}

/// The container name of a file, uppercased (`FLAC`, `WAV`, …).
///
/// The engine's decoder reports the container it actually opened; when the
/// file cannot be decoded the extension is used instead.
fn probe_format(path: &Path) -> String {
    match engine::decode::Decoder::open(path) {
        Ok(decoder) => decoder.format_info().container.clone(),
        Err(_) => path
            .extension()
            .map(|e| e.to_string_lossy().to_uppercase())
            .unwrap_or_else(|| "unknown".to_string()),
    }
}

/// Start the engine on a background tick thread and return its handle.
///
/// The engine owns the output device and the decode/DSP pump; a host drives
/// it by calling [`engine::AudioEngine::tick_blocking`] (this is the same
/// contract the engine's C FFI uses). The engine is moved onto that thread
/// so the caller only holds the lock-free [`EngineHandle`].
fn start_engine(
    cli: &Cli,
) -> Result<(EngineHandle, Arc<AtomicBool>, std::thread::JoinHandle<()>), String> {
    let mut config = EngineConfig {
        output_backend: cli.backend.as_engine(),
        output_device: cli.device.clone(),
        ..Default::default()
    };
    if let Some(rate) = cli.rate {
        config.sample_rate_policy = SampleRatePolicy::Fixed(rate);
    }

    let (sender, receiver) = mpsc::channel::<Result<EngineHandle, String>>();
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);

    let thread = std::thread::Builder::new()
        .name("cli-engine-tick".into())
        .spawn(move || {
            let mut engine = match AudioEngine::new(config) {
                Ok(engine) => engine,
                Err(error) => {
                    let _ = sender.send(Err(format!("engine construction failed: {error}")));
                    return;
                }
            };
            if let Err(error) = engine.start() {
                let _ = sender.send(Err(format!("output could not be opened: {error}")));
                return;
            }
            let handle = engine.handle();
            if sender.send(Ok(handle)).is_err() {
                engine.stop();
                return;
            }
            while !thread_stop.load(Ordering::Relaxed) {
                // Wake instantly on a command, otherwise tick every 5 ms so
                // playback stays smooth while the idle CPU cost stays near
                // zero.
                engine.tick_blocking(Duration::from_millis(5));
            }
            engine.stop();
        })
        .map_err(|e| format!("could not spawn the engine thread: {e}"))?;

    match receiver.recv() {
        Ok(Ok(handle)) => Ok((handle, stop, thread)),
        Ok(Err(message)) => Err(message),
        Err(_) => Err("the engine thread stopped before the engine was ready".to_string()),
    }
}

/// Run the requested command. Returns the process exit code.
fn run(cli: &Cli) -> i32 {
    if cli.help {
        print!("{USAGE}");
        return 0;
    }
    if cli.version {
        println!("{}", engine::VERSION);
        return 0;
    }
    if cli.list_devices {
        return list_devices(cli);
    }
    match &cli.command {
        Command::Probe(path) => probe(cli, path),
        Command::Selftest => selftest(cli),
        Command::Play(paths) => play(cli, paths),
    }
}

/// `--list-devices`: enumerate the output devices of the requested backend.
///
/// The list comes from the backend itself, not from cpal: cpal's WASAPI
/// backend opens shared-mode streams, so a cpal list is not the list the
/// exclusive backend will open, and the ASIO drivers are not in it at all.
fn list_devices(cli: &Cli) -> i32 {
    let backend = cli.backend.as_engine();
    let devices = engine::output::devices::available_devices(backend);
    let reporter = Reporter::new(cli.json);
    if devices.is_empty() {
        reporter.note(&format!(
            "no output devices found for backend '{:?}'",
            backend
        ));
        return 0;
    }
    for device in devices {
        let mut fields = vec![
            ("event", serde_json::Value::from("device")),
            ("backend", serde_json::Value::from(format!("{backend:?}"))),
            ("id", serde_json::Value::from(device.id.clone())),
            ("name", serde_json::Value::from(device.name.clone())),
            ("default", serde_json::Value::from(device.is_default)),
        ];
        // The stable id is what must be handed back with `--device`.
        fields.push(("select", serde_json::Value::from(device.id.clone())));
        reporter.emit(fields, &device.name);
    }
    0
}

/// `probe`: print the decoded format and the engine's bit-perfect report.
fn probe(cli: &Cli, path: &Path) -> i32 {
    let decoder = match engine::decode::Decoder::open(path) {
        Ok(decoder) => decoder,
        Err(error) => {
            eprintln!("probe: cannot open {}: {error}", path.display());
            return 1;
        }
    };
    let info = decoder.info().clone();
    let format = decoder.format_info().clone();

    println!("path: {}", path.display());
    println!("container: {}", format.container);
    println!("codec: {}", format.codec);
    println!("sample rate: {} Hz", info.sample_rate);
    println!("channels: {}", info.channels);
    println!(
        "bit depth: {}",
        format
            .bit_depth
            .map(|d| d.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    );
    println!("sample format: {}", format.sample_format);
    println!("duration: {:.3} s", info.duration_secs);
    println!("lossless: {}", format.is_lossless);
    println!("dsd: {}", format.is_dsd);
    if let Some(rate) = format.input_sample_rate {
        println!("input sample rate: {rate} Hz");
    }

    // The bit-perfect report is the engine's own verdict, produced by the
    // engine's own pipeline. `probe` deliberately opens no output device, so
    // the transport half of the verdict cannot be verified: the report is
    // evaluated against the source format at the source rate with a
    // lossless (f32) output container and an *unverified* transport.
    let source_depth = format.bit_depth.unwrap_or(0);
    let mut pipeline = DspPipeline::from_config(&EngineConfig::default(), info.sample_rate as f32);
    if cli.bit_perfect {
        pipeline.set_bit_perfect(true);
    }
    let mut report = pipeline.bit_perfect_report_with_format(
        info.sample_rate,
        info.sample_rate,
        source_depth,
        32,
        OutputSampleFormat::F32,
        false,
        false,
    );
    // These four fields are owned by the engine tick (it knows the live
    // device, the crossfade mixer and the dither the output applied). In
    // `probe` there is no output and no transition, so they are filled from
    // what the decoder reported.
    report.source_channels = info.channels as u32;
    report.output_channels = info.channels as u32;
    report.decoder_lossless = format.is_lossless;
    report.crossfade_active = false;
    report.dither_active = false;
    report.finalize_with_engine_state();

    println!();
    println!("bit-perfect report:");
    print_report(&report);
    println!();
    println!(
        "note: no output device was opened by 'probe', so the transport half of the \
         verdict is unverified (access_verified=false)."
    );
    0
}

/// Print a bit-perfect report in a stable, greppable form.
fn print_report(report: &BitPerfectReport) {
    println!("  verdict: {}", verdict_label(report));
    if let Some(cause) = report.cause {
        println!("  cause: {cause}");
    }
    if let Some(reason) = &report.reason {
        println!("  reason: {reason}");
    }
    println!("  bit_perfect_samples: {}", report.bit_perfect_samples);
    println!("  bit_perfect_transport: {}", report.bit_perfect_transport);
    println!("  volume_unity: {}", report.volume_unity);
    println!("  eq_bypassed: {}", report.eq_bypassed);
    println!("  dynamics_bypassed: {}", report.dynamics_bypassed);
    println!("  compressor_bypassed: {}", report.compressor_bypassed);
    println!("  limiter_bypassed: {}", report.limiter_bypassed);
    println!("  convolution_bypassed: {}", report.convolution_bypassed);
    println!("  crossfeed_bypassed: {}", report.crossfeed_bypassed);
    println!("  stereo_bypassed: {}", report.stereo_bypassed);
    println!("  loudness_bypassed: {}", report.loudness_bypassed);
    println!("  resampler_bypassed: {}", report.resampler_bypassed);
    println!("  sample_rate_matched: {}", report.sample_rate_matched);
    println!(
        "  bit_depth_not_truncated: {}",
        report.bit_depth_not_truncated
    );
    println!(
        "  format_conversion_lossless: {}",
        report.format_conversion_lossless
    );
    println!("  output_format: {}", report.output_format.label());
    println!("  output_exclusive: {}", report.output_exclusive);
    println!("  access_requested: {:?}", report.access_requested);
    println!("  access_actual: {:?}", report.access_actual);
    println!("  access_verified: {}", report.access_verified);
    println!("  fallback_occurred: {}", report.fallback_occurred);
    println!("  decoder_lossless: {}", report.decoder_lossless);
    println!("  source_channels: {}", report.source_channels);
    println!("  output_channels: {}", report.output_channels);
    println!("  dither_active: {}", report.dither_active);
    println!("  crossfade_active: {}", report.crossfade_active);
    println!("  volume_path: {:?}", report.volume_path);
}

/// One-line verdict for a bit-perfect report.
fn verdict_label(report: &BitPerfectReport) -> String {
    match report.result {
        engine::dsp::pipeline::BitPerfectResult::BitPerfect => "bit-perfect".to_string(),
        engine::dsp::pipeline::BitPerfectResult::Dsp => {
            "not bit-perfect (the signal path modifies the samples)".to_string()
        }
        engine::dsp::pipeline::BitPerfectResult::Unknown => {
            "not proven (the samples are untouched, but the transport is unverified)".to_string()
        }
    }
}

/// `selftest`: run the engine's conformance suites headlessly.
fn selftest(cli: &Cli) -> i32 {
    let reporter = Reporter::new(cli.json);
    let report = engine::eval::run_quality();
    if cli.json {
        match report.to_json() {
            Ok(json) => println!("{json}"),
            Err(error) => {
                eprintln!("selftest: could not render the report: {error}");
                return 1;
            }
        }
    } else {
        print!("{}", report.render_text());
    }
    reporter.emit(
        vec![
            ("event", serde_json::Value::from("selftest")),
            (
                "components",
                serde_json::Value::from(report.components.len()),
            ),
            ("checks", serde_json::Value::from(report.total_checks())),
            ("passes", serde_json::Value::from(report.total_passes())),
            ("failures", serde_json::Value::from(report.total_failures())),
            ("all_pass", serde_json::Value::from(report.is_all_pass())),
        ],
        &format!(
            "selftest: {} components, {} checks, {} pass, {} fail",
            report.components.len(),
            report.total_checks(),
            report.total_passes(),
            report.total_failures()
        ),
    );
    if report.is_all_pass() {
        0
    } else {
        1
    }
}

/// Play the given paths through the engine, reporting state changes.
fn play(cli: &Cli, paths: &[PathBuf]) -> i32 {
    let reporter = Reporter::new(cli.json);

    let (handle, stop, thread) = match start_engine(cli) {
        Ok(parts) => parts,
        Err(message) => {
            eprintln!("audio-engine-cli: {message}");
            return 1;
        }
    };

    if let Some(gain) = cli.gain {
        handle.set_volume_db(gain);
    }
    if cli.bit_perfect {
        // The engine's own bit-perfect bypass: every DSP stage is bypassed and
        // only unity volume and the seek fades survive.
        handle.set_bit_perfect(true);
    }

    let events = handle.clone_event_receiver();
    let mut index = 0usize;
    open_path(&handle, &reporter, &paths[index]);

    // The transport state is reported once per *transition*: the engine's
    // events and the telemetry poll see the same transitions, so the last
    // reported state is tracked to avoid reporting it twice.
    let mut last_state = PlaybackState::Stopped;
    let mut last_position_report = Instant::now();
    let mut finished = false;

    while !finished {
        while let Ok(event) = events.try_recv() {
            match event {
                EngineEvent::SourceOpened {
                    sample_rate,
                    channels,
                    duration_secs,
                    ..
                } => {
                    // The format name comes from the engine's own decoder.
                    let current = &paths[index];
                    let format = probe_format(current);
                    reporter.opened(current, &format, sample_rate, channels, duration_secs);
                }
                EngineEvent::PlaybackStarted => {
                    if last_state != PlaybackState::Playing {
                        last_state = PlaybackState::Playing;
                        reporter.state(PlaybackState::Playing, handle.position_secs());
                    }
                }
                EngineEvent::PlaybackPaused => {
                    if last_state != PlaybackState::Paused {
                        last_state = PlaybackState::Paused;
                        reporter.state(PlaybackState::Paused, handle.position_secs());
                    }
                }
                EngineEvent::PlaybackStopped => {
                    if last_state != PlaybackState::Stopped {
                        last_state = PlaybackState::Stopped;
                        reporter.state(PlaybackState::Stopped, handle.position_secs());
                    }
                }
                EngineEvent::SourceFinished { .. } => {
                    index += 1;
                    if index >= paths.len() {
                        if cli.loop_playback {
                            index = 0;
                            reporter.note("looping: restarting the first path");
                            open_path(&handle, &reporter, &paths[index]);
                        } else {
                            finished = true;
                        }
                    } else {
                        open_path(&handle, &reporter, &paths[index]);
                    }
                }
                EngineEvent::FormatChanged {
                    sample_rate,
                    channels,
                } => {
                    reporter.note(&format!("format changed: {sample_rate} Hz, {channels} ch"));
                }
                EngineEvent::Error(message) => {
                    reporter.error(&message);
                }
                _ => {}
            }
        }

        let info = handle.playback_info();
        if info.state != last_state {
            last_state = info.state;
            reporter.state(info.state, info.position_secs);
        }
        if last_position_report.elapsed() >= Duration::from_millis(500) {
            last_position_report = Instant::now();
            reporter.position(info.position_secs, info.duration_secs);
        }

        std::thread::sleep(Duration::from_millis(50));
    }

    handle.stop();
    stop.store(true, Ordering::Relaxed);
    let _ = thread.join();
    0
}

/// Open a path and start playing it.
fn open_path(handle: &EngineHandle, reporter: &Reporter, path: &Path) {
    reporter.note(&format!("opening {}", path.display()));
    handle.open_file(path.to_path_buf());
    handle.play();
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cli = match parse_args(&args) {
        Ok(cli) => cli,
        Err(message) => {
            eprintln!("audio-engine-cli: {message}");
            eprintln!();
            eprint!("{USAGE}");
            std::process::exit(2);
        }
    };
    std::process::exit(run(&cli));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn defaults_are_empty_and_auto() {
        let cli = parse_args(&args(&["song.flac"])).expect("parse");
        assert_eq!(cli.backend, Backend::Auto);
        assert_eq!(cli.device, None);
        assert!(!cli.bit_perfect);
        assert!(!cli.json);
        assert!(!cli.loop_playback);
        assert_eq!(cli.rate, None);
        assert_eq!(cli.gain, None);
        assert_eq!(cli.command, Command::Play(vec![PathBuf::from("song.flac")]));
    }

    #[test]
    fn long_options_are_parsed() {
        let cli = parse_args(&args(&[
            "--backend",
            "alsa",
            "--device",
            "hw:0,0",
            "--bit-perfect",
            "--rate",
            "96000",
            "--gain",
            "-6.5",
            "--loop",
            "--json",
            "a.flac",
            "b.flac",
        ]))
        .expect("parse");
        assert_eq!(cli.backend, Backend::Alsa);
        assert_eq!(cli.device.as_deref(), Some("hw:0,0"));
        assert!(cli.bit_perfect);
        assert_eq!(cli.rate, Some(96_000));
        assert_eq!(cli.gain, Some(-6.5));
        assert!(cli.loop_playback);
        assert!(cli.json);
        assert_eq!(
            cli.command,
            Command::Play(vec![PathBuf::from("a.flac"), PathBuf::from("b.flac")])
        );
    }

    #[test]
    fn short_options_are_parsed() {
        let cli = parse_args(&args(&["-b", "null", "-d", "default", "x.wav"])).expect("parse");
        assert_eq!(cli.backend, Backend::Null);
        assert_eq!(cli.device.as_deref(), Some("default"));
    }

    #[test]
    fn inline_values_are_accepted() {
        let cli = parse_args(&args(&["--backend=wasapi", "--rate=44100", "x.wav"])).expect("parse");
        assert_eq!(cli.backend, Backend::Wasapi);
        assert_eq!(cli.rate, Some(44_100));
    }

    #[test]
    fn every_documented_backend_is_accepted() {
        for (name, expected) in [
            ("auto", Backend::Auto),
            ("alsa", Backend::Alsa),
            ("wasapi", Backend::Wasapi),
            ("asio", Backend::Asio),
            ("coreaudio", Backend::CoreAudio),
            ("null", Backend::Null),
            ("ALSA", Backend::Alsa),
        ] {
            let cli = parse_args(&args(&["--backend", name, "x.wav"])).expect("parse");
            assert_eq!(cli.backend, expected, "backend {name}");
        }
    }

    #[test]
    fn unknown_backend_is_rejected() {
        let error = parse_args(&args(&["--backend", "pipewire", "x.wav"])).unwrap_err();
        assert!(error.contains("unknown backend"), "{error}");
    }

    #[test]
    fn unknown_option_is_rejected() {
        let error = parse_args(&args(&["--nope", "x.wav"])).unwrap_err();
        assert!(error.contains("unknown option"), "{error}");
    }

    #[test]
    fn missing_value_is_rejected() {
        let error = parse_args(&args(&["--backend"])).unwrap_err();
        assert!(error.contains("requires a value"), "{error}");
    }

    #[test]
    fn missing_path_is_rejected() {
        let error = parse_args(&args(&[])).unwrap_err();
        assert!(error.contains("no input path"), "{error}");
    }

    #[test]
    fn help_and_version_need_no_path() {
        assert!(parse_args(&args(&["--help"])).expect("parse").help);
        assert!(parse_args(&args(&["--version"])).expect("parse").version);
        assert!(parse_args(&args(&["-h"])).expect("parse").help);
        assert!(
            parse_args(&args(&["--list-devices"]))
                .expect("parse")
                .list_devices
        );
    }

    #[test]
    fn probe_subcommand_takes_one_path() {
        let cli = parse_args(&args(&["probe", "track.flac"])).expect("parse");
        assert_eq!(cli.command, Command::Probe(PathBuf::from("track.flac")));

        let error = parse_args(&args(&["probe"])).unwrap_err();
        assert!(error.contains("exactly one"), "{error}");

        let error = parse_args(&args(&["probe", "a.flac", "b.flac"])).unwrap_err();
        assert!(error.contains("exactly one"), "{error}");
    }

    #[test]
    fn selftest_subcommand_is_recognised() {
        let cli = parse_args(&args(&["selftest"])).expect("parse");
        assert_eq!(cli.command, Command::Selftest);
    }

    #[test]
    fn rate_is_range_checked() {
        assert_eq!(parse_rate("48000").expect("rate"), 48_000);
        assert!(parse_rate("0").is_err());
        assert!(parse_rate("-1").is_err());
        assert!(parse_rate("1000000").is_err());
        assert!(parse_rate("abc").is_err());
    }

    #[test]
    fn gain_is_range_checked() {
        assert_eq!(parse_gain("-6.5").expect("gain"), -6.5);
        assert_eq!(parse_gain("0").expect("gain"), 0.0);
        assert!(parse_gain("6").is_err());
        assert!(parse_gain("-61").is_err());
        assert!(parse_gain("loud").is_err());
    }

    #[test]
    fn backend_maps_onto_engine_backends() {
        assert_eq!(Backend::Auto.as_engine(), AudioBackend::Auto);
        assert_eq!(Backend::Alsa.as_engine(), AudioBackend::ExclusiveAlsa);
        assert_eq!(Backend::Wasapi.as_engine(), AudioBackend::ExclusiveWasapi);
        assert_eq!(Backend::Asio.as_engine(), AudioBackend::ExclusiveAsio);
        assert_eq!(
            Backend::CoreAudio.as_engine(),
            AudioBackend::ExclusiveCoreAudioHog
        );
        assert_eq!(Backend::Null.as_engine(), AudioBackend::Null);
    }

    #[test]
    fn version_is_the_engine_version() {
        // The binary must report the crate's own version, not a stale copy.
        assert!(!engine::VERSION.is_empty());
        assert_eq!(engine::VERSION, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn state_labels_are_stable() {
        assert_eq!(state_label(PlaybackState::Stopped), "stopped");
        assert_eq!(state_label(PlaybackState::Playing), "playing");
        assert_eq!(state_label(PlaybackState::Paused), "paused");
        assert_eq!(state_label(PlaybackState::Buffering), "buffering");
    }
}
