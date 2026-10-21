//! `engine-tui` — the engine's terminal player.
//!
//! The engine README documents this binary as "an interactive, zero-overhead
//! TUI player built with Ratatui and Crossterm", and pins an exact keybinding
//! table. The table is the specification of this file: [`keys::handle_key`]
//! is a pure function from a key event to an [`keys::Action`], and the unit
//! tests at the bottom of the module assert that **every** row of the README
//! table is implemented.
//!
//! ## What is real, and what is not
//!
//! The TUI is a *client* of the engine, and it drives the real engine:
//!
//! * Transport (`Space`) calls the engine's own `Play` / `Pause` commands.
//! * The equalizer panel reads the engine's live band settings and writes
//!   them back through `EngineCommand::SetEqBandParams`, so the curve drawn
//!   on screen is the curve the engine is actually running.
//! * The curve itself is computed with the engine's own biquad code
//!   ([`engine::dsp::biquad::BiquadCoeffs`]), not with a re-implementation.
//! * The meters and the spectrum come from the engine's own
//!   [`engine::dsp::meters::ProfessionalMeters`] and
//!   [`engine::dsp::analyzer::AudioAnalyzer`] snapshots.
//! * The output panel lists the devices the engine's own enumeration
//!   returns, and shows the negotiated output state the engine published.
//!
//! Where the engine exposes nothing, the panel says so instead of drawing
//! invented numbers.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Axis, Block, Borders, Chart, Dataset, Gauge, GraphType, LineGauge, List, ListItem, ListState,
    Paragraph, Sparkline,
};
use ratatui::{Frame, Terminal};

use engine::dsp::biquad::BiquadCoeffs;
use engine::dsp::equalizer::EqFilterType;
use engine::{AudioEngine, EngineConfig, EngineHandle, EqBandSetting};

// ─────────────────────────────────────────────────────────────────────────────
// Key handling
// ─────────────────────────────────────────────────────────────────────────────

/// The key→action mapping.
///
/// This is deliberately a separate, pure module: the README's keybinding
/// table is the contract, and a table that can only be verified by running
/// the binary is a table that silently rots.
mod keys {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    /// What a key press means to the application.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Action {
        /// Tab — move to the next panel.
        NextPanel,
        /// Shift+Tab — move to the previous panel.
        PrevPanel,
        /// Up — move the selection up.
        SelectUp,
        /// Down — move the selection down.
        SelectDown,
        /// Left — decrease the selected value.
        AdjustLeft,
        /// Right — increase the selected value.
        AdjustRight,
        /// Enter — activate the selected item.
        Activate,
        /// Space — toggle play / pause.
        TogglePlay,
        /// `/` — open the file and directory browser.
        OpenBrowser,
        /// `f` / `F` — lower / raise the selected EQ band's frequency.
        EqFrequency(i32),
        /// `w` / `W` — lower / raise the selected EQ band's Q factor.
        EqQ(i32),
        /// `t` — cycle the selected EQ band's filter type.
        EqCycleFilter,
        /// `x` — bypass or enable the selected EQ band.
        EqToggleBand,
        /// `r` — rescan the hardware output devices.
        RescanDevices,
        /// `q` — quit (the application requires a second `q`).
        Quit,
    }

    /// Map a key event to an action.
    ///
    /// Pure: the same event always yields the same action, and no state is
    /// consulted. Key *release* events are ignored (Windows and some
    /// terminals deliver them); presses and auto-repeats both map.
    pub fn handle_key(key: KeyEvent) -> Option<Action> {
        if key.kind == KeyEventKind::Release {
            return None;
        }
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        match key.code {
            KeyCode::Tab => Some(if shift {
                Action::PrevPanel
            } else {
                Action::NextPanel
            }),
            // Some terminals report Shift+Tab as its own code.
            KeyCode::BackTab => Some(Action::PrevPanel),
            KeyCode::Up => Some(Action::SelectUp),
            KeyCode::Down => Some(Action::SelectDown),
            KeyCode::Left => Some(Action::AdjustLeft),
            KeyCode::Right => Some(Action::AdjustRight),
            KeyCode::Enter => Some(Action::Activate),
            KeyCode::Char(c) => {
                if ctrl && c.eq_ignore_ascii_case(&'c') {
                    return Some(Action::Quit);
                }
                // A terminal may deliver `F` either as an uppercase character
                // or as `f` with the SHIFT modifier; both mean "raise".
                let upper = c.is_uppercase() || (shift && c.is_ascii_lowercase());
                match c.to_ascii_lowercase() {
                    ' ' => Some(Action::TogglePlay),
                    '/' => Some(Action::OpenBrowser),
                    'f' => Some(if upper {
                        Action::EqFrequency(1)
                    } else {
                        Action::EqFrequency(-1)
                    }),
                    'w' => Some(if upper {
                        Action::EqQ(1)
                    } else {
                        Action::EqQ(-1)
                    }),
                    't' => Some(Action::EqCycleFilter),
                    'x' => Some(Action::EqToggleBand),
                    'r' => Some(Action::RescanDevices),
                    'q' => Some(Action::Quit),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Panels
// ─────────────────────────────────────────────────────────────────────────────

/// The panels the README names, cycled with Tab / Shift+Tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    /// File and directory browser.
    Browser,
    /// Transport, level meters and spectrum.
    Transport,
    /// Parametric equalizer.
    Equalizer,
    /// Output devices and the negotiated output state.
    Output,
}

impl Panel {
    /// Every panel, in cycle order.
    const ALL: [Panel; 4] = [
        Panel::Browser,
        Panel::Transport,
        Panel::Equalizer,
        Panel::Output,
    ];

    fn next(self) -> Self {
        let index = Panel::ALL.iter().position(|p| *p == self).unwrap_or(0);
        Panel::ALL[(index + 1) % Panel::ALL.len()]
    }

    fn prev(self) -> Self {
        let index = Panel::ALL.iter().position(|p| *p == self).unwrap_or(0);
        Panel::ALL[(index + Panel::ALL.len() - 1) % Panel::ALL.len()]
    }

    fn title(self) -> &'static str {
        match self {
            Panel::Browser => "Browser",
            Panel::Transport => "Transport",
            Panel::Equalizer => "Equalizer",
            Panel::Output => "Output",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Application state
// ─────────────────────────────────────────────────────────────────────────────

/// One entry of the file browser.
#[derive(Debug, Clone)]
struct Entry {
    name: String,
    path: PathBuf,
    is_dir: bool,
    /// The synthetic "go to the parent directory" entry.
    is_parent: bool,
}

/// The file / directory browser.
struct Browser {
    cwd: PathBuf,
    entries: Vec<Entry>,
    selected: usize,
}

impl Browser {
    fn new(cwd: PathBuf) -> Self {
        let mut browser = Self {
            cwd,
            entries: Vec::new(),
            selected: 0,
        };
        browser.refresh();
        browser
    }

    /// Re-read the current directory.
    fn refresh(&mut self) {
        self.entries.clear();
        self.selected = 0;
        if let Some(parent) = self
            .cwd
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            self.entries.push(Entry {
                name: ".. (parent directory)".to_string(),
                path: parent.to_path_buf(),
                is_dir: true,
                is_parent: true,
            });
        }
        let Ok(read_dir) = std::fs::read_dir(&self.cwd) else {
            return;
        };
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        for entry in read_dir.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if is_dir {
                dirs.push(Entry {
                    name,
                    path,
                    is_dir,
                    is_parent: false,
                });
            } else {
                files.push(Entry {
                    name,
                    path,
                    is_dir,
                    is_parent: false,
                });
            }
        }
        dirs.sort_by(|a, b| a.name.cmp(&b.name));
        files.sort_by(|a, b| a.name.cmp(&b.name));
        self.entries.extend(dirs);
        self.entries.extend(files);
    }

    /// The entry under the cursor, if any.
    fn selected_entry(&self) -> Option<&Entry> {
        self.entries.get(self.selected)
    }

    /// Move the cursor, clamped to the list.
    fn move_selection(&mut self, delta: isize) {
        if self.entries.is_empty() {
            self.selected = 0;
            return;
        }
        let last = self.entries.len() as isize - 1;
        let next = (self.selected as isize + delta).clamp(0, last);
        self.selected = next as usize;
    }
}

/// The output-device list shown in the output panel.
#[derive(Default)]
struct OutputState {
    devices: Vec<String>,
    selected: usize,
    /// Set when the device list was refreshed, so the panel can say so.
    refreshed_at: Option<Instant>,
}

/// Key-repeat acceleration.
///
/// Holding a key down repeats it; the step grows with the number of
/// consecutive repeats so a long list can be traversed quickly.
#[derive(Default)]
struct Repeat {
    last: Option<keys::Action>,
    count: u32,
    last_at: Option<Instant>,
}

impl Repeat {
    /// Register an action and return the step size it should use.
    fn step(&mut self, action: keys::Action, now: Instant) -> usize {
        const WINDOW: Duration = Duration::from_millis(400);
        let continuing = self.last == Some(action)
            && self
                .last_at
                .is_some_and(|at| now.duration_since(at) < WINDOW);
        if continuing {
            self.count = self.count.saturating_add(1);
        } else {
            self.count = 0;
        }
        self.last = Some(action);
        self.last_at = Some(now);
        1 + (self.count / 4) as usize
    }
}

/// The local model of the engine's parametric-EQ bands.
///
/// The engine's settings read-back is published on the tick that applied a
/// command, so a value sent a millisecond ago is not in the snapshot yet.
/// The TUI therefore keeps its own model: an edit is applied to the model
/// immediately *and* pushed to the engine, and the engine's snapshot is
/// merged back in once it catches up (see [`App::sync_eq`]).
#[derive(Default)]
struct EqModel {
    /// What the TUI shows.
    bands: Vec<EqBandSetting>,
    /// The value the TUI last asked the engine to use, per band.
    sent: Vec<EqBandSetting>,
    /// Whether an edit is still in flight, per band.
    dirty: Vec<bool>,
}

/// The whole application state.
struct App {
    handle: EngineHandle,
    panel: Panel,
    browser: Browser,
    output: OutputState,
    /// The local model of the engine's EQ bands.
    eq: EqModel,
    /// The selected parametric-EQ band.
    eq_band: usize,
    /// The last status line message.
    message: Option<String>,
    /// `q` was pressed once; a second `q` confirms the quit.
    quit_armed: bool,
    /// `q` was pressed twice: the application exits.
    quit_confirmed: bool,
    repeat: Repeat,
}

impl App {
    fn new(handle: EngineHandle, initial: Option<PathBuf>) -> Self {
        let cwd = initial
            .as_ref()
            .filter(|path| path.is_dir())
            .cloned()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let browser = Browser::new(cwd);
        let mut app = Self {
            handle,
            panel: Panel::Transport,
            browser,
            output: OutputState::default(),
            eq: EqModel::default(),
            eq_band: 0,
            message: None,
            quit_armed: false,
            quit_confirmed: false,
            repeat: Repeat::default(),
        };
        app.rescan_devices();
        app.sync_eq();
        if let Some(path) = initial {
            if path.is_file() {
                app.handle.open_file(path);
                app.handle.play();
            }
        }
        app
    }

    /// True when the application should exit.
    fn should_quit(&self) -> bool {
        self.quit_confirmed
    }

    /// Apply an action.
    fn apply(&mut self, action: keys::Action) {
        let now = Instant::now();
        let step = self.repeat.step(action, now);

        match action {
            keys::Action::NextPanel => {
                self.panel = self.panel.next();
                self.message = Some(format!("panel: {}", self.panel.title()));
            }
            keys::Action::PrevPanel => {
                self.panel = self.panel.prev();
                self.message = Some(format!("panel: {}", self.panel.title()));
            }
            keys::Action::SelectUp => self.select(-(step as isize)),
            keys::Action::SelectDown => self.select(step as isize),
            keys::Action::AdjustLeft => self.adjust(-(step as isize)),
            keys::Action::AdjustRight => self.adjust(step as isize),
            keys::Action::Activate => self.activate(),
            keys::Action::TogglePlay => {
                if self.handle.is_playing() {
                    self.handle.pause();
                    self.message = Some("paused".to_string());
                } else {
                    self.handle.play();
                    self.message = Some("playing".to_string());
                }
            }
            keys::Action::OpenBrowser => {
                self.browser.refresh();
                self.panel = Panel::Browser;
                self.message = Some(format!("browser: {}", self.browser.cwd.display()));
            }
            keys::Action::EqFrequency(direction) => self.adjust_eq_frequency(direction, step),
            keys::Action::EqQ(direction) => self.adjust_eq_q(direction, step),
            keys::Action::EqCycleFilter => self.cycle_eq_filter(),
            keys::Action::EqToggleBand => self.toggle_eq_band(),
            keys::Action::RescanDevices => {
                self.rescan_devices();
                self.message = Some(format!(
                    "{} output device(s) found",
                    self.output.devices.len()
                ));
            }
            keys::Action::Quit => {
                if self.quit_armed {
                    self.quit_confirmed = true;
                    self.message = Some("quitting".to_string());
                } else {
                    self.quit_armed = true;
                    self.message = Some("press q again to quit".to_string());
                }
            }
        }

        // Any key other than `q` disarms the quit confirmation.
        if action != keys::Action::Quit {
            self.quit_armed = false;
        }
    }

    /// Move the selection of the active panel.
    fn select(&mut self, delta: isize) {
        match self.panel {
            Panel::Browser => self.browser.move_selection(delta),
            Panel::Equalizer => {
                let count = self.eq_bands().len();
                if count == 0 {
                    return;
                }
                let last = count as isize - 1;
                self.eq_band = (self.eq_band as isize + delta).clamp(0, last) as usize;
            }
            Panel::Output => {
                if self.output.devices.is_empty() {
                    return;
                }
                let last = self.output.devices.len() as isize - 1;
                self.output.selected =
                    (self.output.selected as isize + delta).clamp(0, last) as usize;
            }
            Panel::Transport => {}
        }
    }

    /// Adjust the value the active panel is showing.
    fn adjust(&mut self, delta: isize) {
        match self.panel {
            Panel::Equalizer => self.adjust_eq_gain(delta),
            Panel::Transport => {
                // The transport panel's adjustable value is the master volume.
                let current = self.handle.volume();
                let next = (current + delta as f32 * 0.02).clamp(0.0, 1.0);
                self.handle.set_volume(next);
                self.message = Some(format!("volume: {:.0} %", next * 100.0));
            }
            Panel::Browser | Panel::Output => {}
        }
    }

    /// Activate the selected item of the active panel.
    fn activate(&mut self) {
        match self.panel {
            Panel::Browser => {
                let Some(entry) = self.browser.selected_entry().cloned() else {
                    return;
                };
                if entry.is_dir {
                    self.browser.cwd = entry.path.clone();
                    self.browser.refresh();
                    self.message = Some(format!("entered {}", entry.path.display()));
                } else {
                    self.handle.open_file(entry.path.clone());
                    self.handle.play();
                    self.message = Some(format!("playing {}", entry.path.display()));
                }
            }
            Panel::Output => {
                if let Some(device) = self.output.devices.get(self.output.selected) {
                    let device = device.clone();
                    self.handle.set_output_device(Some(device.clone()));
                    self.message = Some(format!("output device: {device}"));
                }
            }
            Panel::Equalizer => self.toggle_eq_band(),
            Panel::Transport => {}
        }
    }

    /// The engine's live parametric-EQ bands.
    ///
    /// The engine publishes its settings read-back on the tick that applied
    /// a command, so a value sent a millisecond ago is not in the snapshot
    /// yet. The TUI therefore keeps its own model of the bands: an edit is
    /// applied to the model immediately and pushed to the engine, and the
    /// engine's snapshot is merged back in once it catches up. Without the
    /// model, holding `f` down would re-send the *same* frequency on every
    /// repeat and the band would barely move.
    fn eq_bands(&self) -> Vec<EqBandSetting> {
        self.eq.bands.clone()
    }

    /// Reconcile the local band model with the engine's published settings.
    fn sync_eq(&mut self) {
        let engine = self.handle.settings().eq_bands;
        if engine.is_empty() {
            self.eq.bands.clear();
            self.eq.sent.clear();
            self.eq.dirty.clear();
            return;
        }
        if self.eq.bands.len() != engine.len() {
            self.eq.bands = engine.clone();
            self.eq.sent = engine.clone();
            self.eq.dirty = vec![false; engine.len()];
            return;
        }
        for (index, band) in engine.iter().enumerate() {
            if self.eq.dirty[index] {
                // An edit of ours is still in flight: it lands when the
                // engine publishes exactly what we asked for.
                if self.eq.sent[index] == *band {
                    self.eq.dirty[index] = false;
                    self.eq.bands[index] = *band;
                }
                continue;
            }
            // No edit of ours is in flight: the engine is authoritative
            // (it may have been changed from outside the TUI).
            self.eq.bands[index] = *band;
            self.eq.sent[index] = *band;
        }
    }

    /// The currently selected band, if the engine has any.
    fn selected_band(&self) -> Option<EqBandSetting> {
        self.eq.bands.get(self.eq_band).copied()
    }

    /// Adjust the selected band's gain (left / right).
    fn adjust_eq_gain(&mut self, delta: isize) {
        let Some(band) = self.selected_band() else {
            self.message = Some("no parametric EQ bands in this build".to_string());
            return;
        };
        let gain = (band.gain_db + delta as f32 * 0.5).clamp(-24.0, 24.0);
        self.commit_eq_band(EqBandSetting {
            gain_db: gain,
            ..band
        });
        self.message = Some(format!("band {} gain: {gain:+.1} dB", self.eq_band));
    }

    /// Adjust the selected band's frequency.
    ///
    /// One step is a semitone (`2^(1/12)`), which is the musically
    /// meaningful increment for a frequency control.
    fn adjust_eq_frequency(&mut self, direction: i32, step: usize) {
        let Some(band) = self.selected_band() else {
            self.message = Some("no parametric EQ bands in this build".to_string());
            return;
        };
        let ratio = 2f32.powf(1.0 / 12.0).powf(direction as f32 * step as f32);
        let frequency = (band.frequency * ratio).clamp(20.0, 20_000.0);
        self.commit_eq_band(EqBandSetting { frequency, ..band });
        self.message = Some(format!(
            "band {} frequency: {frequency:.0} Hz",
            self.eq_band
        ));
    }

    /// Adjust the selected band's Q factor.
    fn adjust_eq_q(&mut self, direction: i32, step: usize) {
        let Some(band) = self.selected_band() else {
            self.message = Some("no parametric EQ bands in this build".to_string());
            return;
        };
        let ratio = 1.1f32.powf(direction as f32 * step as f32);
        let q = (band.q * ratio).clamp(0.1, 30.0);
        self.commit_eq_band(EqBandSetting { q, ..band });
        self.message = Some(format!("band {} Q: {q:.2}", self.eq_band));
    }

    /// Cycle the selected band's filter type.
    fn cycle_eq_filter(&mut self) {
        let Some(band) = self.selected_band() else {
            self.message = Some("no parametric EQ bands in this build".to_string());
            return;
        };
        let next = next_filter_type(band.filter_type);
        self.commit_eq_band(EqBandSetting {
            filter_type: next,
            ..band
        });
        self.message = Some(format!("band {} filter: {next:?}", self.eq_band));
    }

    /// Bypass or enable the selected band.
    fn toggle_eq_band(&mut self) {
        let Some(band) = self.selected_band() else {
            self.message = Some("no parametric EQ bands in this build".to_string());
            return;
        };
        self.commit_eq_band(EqBandSetting {
            enabled: !band.enabled,
            ..band
        });
        self.message = Some(format!(
            "band {} {}",
            self.eq_band,
            if band.enabled { "bypassed" } else { "enabled" }
        ));
    }

    /// Write a changed band to the engine and to the local model.
    fn commit_eq_band(&mut self, band: EqBandSetting) {
        let index = self.eq_band;
        self.handle.set_eq_band_params(
            index,
            band.frequency,
            band.gain_db,
            band.q,
            band.filter_type,
            band.enabled,
        );
        if let Some(slot) = self.eq.bands.get_mut(index) {
            *slot = band;
        }
        if let Some(slot) = self.eq.sent.get_mut(index) {
            *slot = band;
        }
        if let Some(slot) = self.eq.dirty.get_mut(index) {
            *slot = true;
        }
    }

    /// Re-enumerate the output devices through the engine's own enumeration.
    fn rescan_devices(&mut self) {
        let backend = self.handle.settings().output_backend;
        self.output.devices = engine::output::cpal_devices::enumerate_devices(backend);
        if self.output.selected >= self.output.devices.len() {
            self.output.selected = 0;
        }
        self.output.refreshed_at = Some(Instant::now());
    }
}

/// The filter types the `t` key cycles through.
///
/// The README names "Peaking, Shelf, Notch, HPF, LPF". The engine has two
/// shelf types (low and high), so both are in the cycle and the README's
/// "Shelf" row is covered by the pair.
const FILTER_TYPE_CYCLE: [EqFilterType; 6] = [
    EqFilterType::Peaking,
    EqFilterType::LowShelf,
    EqFilterType::HighShelf,
    EqFilterType::Notch,
    EqFilterType::HighPass,
    EqFilterType::LowPass,
];

/// The next filter type in the cycle.
fn next_filter_type(current: EqFilterType) -> EqFilterType {
    let index = FILTER_TYPE_CYCLE
        .iter()
        .position(|candidate| *candidate == current)
        .unwrap_or(0);
    FILTER_TYPE_CYCLE[(index + 1) % FILTER_TYPE_CYCLE.len()]
}

// ─────────────────────────────────────────────────────────────────────────────
// EQ curve
// ─────────────────────────────────────────────────────────────────────────────

/// The biquad coefficients of one EQ band, from the engine's own code.
fn band_coeffs(band: &EqBandSetting, sample_rate: f32) -> BiquadCoeffs<f32> {
    match band.filter_type {
        EqFilterType::Peaking => {
            BiquadCoeffs::peaking(sample_rate, band.frequency, band.gain_db, band.q)
        }
        EqFilterType::LowShelf => {
            BiquadCoeffs::lowshelf(sample_rate, band.frequency, band.gain_db, band.q)
        }
        EqFilterType::HighShelf => {
            BiquadCoeffs::highshelf(sample_rate, band.frequency, band.gain_db, band.q)
        }
        EqFilterType::Notch => BiquadCoeffs::notch(sample_rate, band.frequency, band.q),
        EqFilterType::HighPass => BiquadCoeffs::highpass(sample_rate, band.frequency, band.q),
        EqFilterType::LowPass => BiquadCoeffs::lowpass(sample_rate, band.frequency, band.q),
        EqFilterType::Bandpass => BiquadCoeffs::bandpass(sample_rate, band.frequency, band.q),
        EqFilterType::AllPass => BiquadCoeffs::allpass(sample_rate, band.frequency, band.q),
    }
}

/// The frequency (Hz) of position `t` on the log-scaled curve axis.
fn curve_frequency(t: f64) -> f64 {
    20.0 * 1000f64.powf(t)
}

/// The EQ curve in dB, computed with the engine's own biquad magnitude
/// response.
///
/// `x` is the position on a logarithmic frequency axis (0 = 20 Hz,
/// 1 = 20 kHz), `y` the magnitude in dB.
fn eq_curve(bands: &[EqBandSetting], sample_rate: f32) -> Vec<(f64, f64)> {
    const POINTS: usize = 160;
    let mut points = Vec::with_capacity(POINTS + 1);
    for index in 0..=POINTS {
        let t = index as f64 / POINTS as f64;
        let frequency = curve_frequency(t);
        let mut magnitude = 1.0f64;
        for band in bands {
            if !band.enabled {
                continue;
            }
            let coeffs = band_coeffs(band, sample_rate);
            magnitude *= coeffs.evaluate_magnitude(frequency as f32, sample_rate);
        }
        let db = 20.0 * magnitude.max(1e-12).log10();
        points.push((t, db));
    }
    points
}

// ─────────────────────────────────────────────────────────────────────────────
// Rendering
// ─────────────────────────────────────────────────────────────────────────────

/// The application's colours.
fn accent() -> Style {
    Style::default().fg(Color::Cyan)
}

fn dim() -> Style {
    Style::default().fg(Color::DarkGray)
}

impl App {
    /// Draw one frame.
    fn render(&mut self, frame: &mut Frame) {
        // Pick up whatever the engine published since the last frame.
        self.sync_eq();

        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(4),
            ])
            .split(frame.area());

        self.render_header(frame, areas[0]);
        match self.panel {
            Panel::Browser => self.render_browser(frame, areas[1]),
            Panel::Transport => self.render_transport(frame, areas[1]),
            Panel::Equalizer => self.render_equalizer(frame, areas[1]),
            Panel::Output => self.render_output(frame, areas[1]),
        }
        self.render_footer(frame, areas[2]);
    }

    /// The status bar: what is playing, in which state, and at what quality.
    fn render_header(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let info = self.handle.playback_info();
        let state = match info.state {
            engine::playback_info::PlaybackState::Playing => "playing",
            engine::playback_info::PlaybackState::Paused => "paused",
            engine::playback_info::PlaybackState::Buffering => "buffering",
            engine::playback_info::PlaybackState::Stopped => "stopped",
        };
        let source = match &info.current_source {
            Some(engine::AudioSource::File(path)) => path.display().to_string(),
            Some(engine::AudioSource::Uri(uri)) => uri.clone(),
            Some(_) => "<memory>".to_string(),
            None => "<nothing loaded>".to_string(),
        };
        let bit_perfect = match info.engine_stats.as_ref() {
            Some(stats) if stats.bit_perfect => "bit-perfect".to_string(),
            Some(stats) => match &stats.bit_perfect_reason {
                Some(reason) => format!("not bit-perfect ({reason})"),
                None => "bit-perfect status unknown".to_string(),
            },
            None => "bit-perfect status unknown".to_string(),
        };
        let line = Line::from(vec![
            Span::styled(
                format!(" {} ", engine::NAME),
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("v{}  ", engine::VERSION)),
            Span::styled(format!("{state}  "), accent()),
            Span::raw(source),
        ]);
        let second = Line::from(vec![
            Span::styled("panel: ", dim()),
            Span::styled(self.panel.title(), accent()),
            Span::styled("   ", Style::default()),
            Span::styled("bit-perfect: ", dim()),
            Span::raw(bit_perfect),
        ]);
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Shadow Engine TUI");
        frame.render_widget(Paragraph::new(vec![line, second]).block(block), area);
    }

    /// The browser panel.
    fn render_browser(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let items: Vec<ListItem> = self
            .browser
            .entries
            .iter()
            .map(|entry| {
                // Directories are marked with a trailing slash: an ASCII
                // marker, so the column stays aligned in every terminal (an
                // emoji is two cells wide and shifts the whole layout).
                let name = if entry.is_dir && !entry.is_parent {
                    format!("{}/", entry.name)
                } else {
                    entry.name.clone()
                };
                ListItem::new(Line::from(name))
            })
            .collect();
        let title = format!(" Browser — {} ", self.browser.cwd.display());
        let block = Block::default().borders(Borders::ALL).title(title);
        let list = List::new(items).block(block).highlight_style(
            Style::default()
                .bg(Color::Rgb(40, 60, 80))
                .add_modifier(Modifier::BOLD),
        );
        let mut state = ListState::default();
        state.select(Some(
            self.browser
                .selected
                .min(self.browser.entries.len().saturating_sub(1)),
        ));
        frame.render_stateful_widget(list, area, &mut state);
    }

    /// The transport panel: position, meters, spectrum, quality.
    fn render_transport(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let info = self.handle.playback_info();
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(4),
            ])
            .split(area);

        // 1. Transport summary.
        let duration = info.duration_secs;
        let position = info.position_secs;
        let ratio = if duration > 0.0 {
            f64::from((position / duration).clamp(0.0, 1.0))
        } else {
            0.0
        };
        let gain_reduction = info
            .engine_stats
            .as_ref()
            .map(|stats| stats.limiter_gain_reduction_db)
            .unwrap_or(0.0);
        let summary = Paragraph::new(vec![
            Line::from(format!(
                "position: {position:.1} s / {duration:.1} s   latency: {:.1} ms",
                info.latency_ms
            )),
            Line::from(format!(
                "rate: {} Hz   channels: {}   volume: {:.0} %   cpu: {:.1} %",
                info.sample_rate,
                info.meters.as_ref().map(|m| m.channels).unwrap_or(0),
                info.volume * 100.0,
                info.cpu_usage_pct
            )),
            Line::from(format!(
                "output: {}   limiter gain reduction: {gain_reduction:.1} dB",
                info.output_info
                    .as_ref()
                    .map(|o| o.summary())
                    .unwrap_or_else(|| "not opened".to_string())
            )),
        ])
        .block(Block::default().borders(Borders::ALL).title(" Transport "));
        frame.render_widget(summary, rows[0]);

        // 2. Level meters — the engine's own professional meters.
        match info.meters.as_ref() {
            Some(meters) => {
                let peaks: Vec<f32> = meters.peak_db.clone();
                let rms: Vec<f32> = meters.rms_db.clone();
                let lines: Vec<Line> = peaks
                    .iter()
                    .enumerate()
                    .map(|(index, peak)| {
                        let channel_rms = rms.get(index).copied().unwrap_or(-120.0);
                        Line::from(format!(
                            "ch{}  peak {:>7.1} dBFS   rms {:>7.1} dBFS   true peak {:>7.1} dBTP",
                            index + 1,
                            peak,
                            channel_rms,
                            meters
                                .true_peak_dbtp
                                .get(index)
                                .copied()
                                .unwrap_or(f32::NEG_INFINITY)
                        ))
                    })
                    .collect();
                let meter_text = vec![
                    Line::from(format!(
                        "LUFS  momentary {:>6.1}   short-term {:>6.1}   integrated {:>6.1}",
                        meters.lufs_momentary, meters.lufs_short_term, meters.lufs_integrated
                    )),
                    Line::from(format!(
                        "LRA   {:>6.1} LU   clips: {}",
                        meters.loudness_range_lu, meters.clip_count
                    )),
                ];
                let mut all = meter_text;
                all.extend(lines);
                frame.render_widget(
                    Paragraph::new(all)
                        .block(Block::default().borders(Borders::ALL).title(" Meters ")),
                    rows[1],
                );
            }
            None => {
                frame.render_widget(
                    Paragraph::new(Line::from(
                        "no meter data yet — the engine has not published a snapshot",
                    ))
                    .block(Block::default().borders(Borders::ALL).title(" Meters ")),
                    rows[1],
                );
            }
        }

        // 3. The playhead, as a progress line.
        let position_gauge = LineGauge::default()
            .filled_style(Style::default().fg(Color::Cyan))
            .ratio(ratio)
            .label(format!("{position:.1} s / {duration:.1} s"))
            .block(Block::default().borders(Borders::ALL).title(" Position "));
        frame.render_widget(position_gauge, rows[2]);

        // 4. A level gauge for the master peak.
        let peak = info
            .meters
            .as_ref()
            .and_then(|m| m.peak_db.first().copied())
            .unwrap_or(-120.0);
        let gauge_ratio = f64::from(((peak + 60.0) / 60.0).clamp(0.0, 1.0));
        let gauge = Gauge::default()
            .gauge_style(Style::default().fg(Color::Green))
            .ratio(gauge_ratio)
            .label(format!("peak {peak:.1} dBFS"))
            .block(Block::default().borders(Borders::ALL).title(" Level "));
        frame.render_widget(gauge, rows[3]);

        // 5. The spectrum, from the engine's own analyzer.
        let snapshot = self.handle.analyzer().snapshot();
        if snapshot.spectrum_db.is_empty() {
            frame.render_widget(
                Paragraph::new(Line::from(
                    "no spectrum data yet — nothing has been analysed",
                ))
                .block(Block::default().borders(Borders::ALL).title(" Spectrum ")),
                rows[4],
            );
        } else {
            let width = rows[4].width.saturating_sub(2).max(1) as usize;
            let data = resample_spectrum(&snapshot.spectrum_db, width);
            let sparkline = Sparkline::default()
                .data(&data)
                .block(Block::default().borders(Borders::ALL).title(" Spectrum "));
            frame.render_widget(sparkline, rows[4]);
        }
    }

    /// The equalizer panel: the live bands and the curve they produce.
    fn render_equalizer(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let bands = self.eq_bands();
        let info = self.handle.playback_info();
        let sample_rate = info.sample_rate.max(1) as f32;

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(5), Constraint::Length(12)])
            .split(area);

        if bands.is_empty() {
            frame.render_widget(
                Paragraph::new(Line::from(
                    "the engine exposes no parametric EQ bands in this build",
                ))
                .block(Block::default().borders(Borders::ALL).title(" Equalizer ")),
                rows[0],
            );
        } else {
            let items: Vec<ListItem> = bands
                .iter()
                .enumerate()
                .map(|(index, band)| {
                    ListItem::new(Line::from(format!(
                        "{:>2}  {:>8.1} Hz  {:>+6.1} dB  Q {:>5.2}  {:<10}  {}",
                        index,
                        band.frequency,
                        band.gain_db,
                        band.q,
                        format!("{:?}", band.filter_type),
                        if band.enabled { "on" } else { "bypassed" }
                    )))
                })
                .collect();
            let list = List::new(items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Equalizer — bands (f/F freq, w/W Q, t type, x bypass) "),
                )
                .highlight_style(
                    Style::default()
                        .bg(Color::Rgb(40, 60, 80))
                        .add_modifier(Modifier::BOLD),
                );
            let mut state = ListState::default();
            state.select(Some(self.eq_band.min(bands.len() - 1)));
            frame.render_stateful_widget(list, rows[0], &mut state);
        }

        // The curve, computed with the engine's own biquad magnitude response.
        let points = eq_curve(&bands, sample_rate);
        let dataset = Dataset::default()
            .name("EQ")
            .marker(ratatui::symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Magenta))
            .data(&points);
        let x_labels: Vec<String> = [0.0, 0.25, 0.5, 0.75, 1.0]
            .iter()
            .map(|t| format_frequency(curve_frequency(*t)))
            .collect();
        let chart = Chart::new(vec![dataset])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Equalizer — curve (engine biquad response) "),
            )
            .x_axis(
                Axis::default()
                    .title("Hz")
                    .bounds([0.0, 1.0])
                    .labels(x_labels),
            )
            .y_axis(Axis::default().title("dB").bounds([-24.0, 24.0]).labels([
                "-24".to_string(),
                "0".to_string(),
                "+24".to_string(),
            ]));
        frame.render_widget(chart, rows[1]);
    }

    /// The output panel: the devices and the negotiated output state.
    fn render_output(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let info = self.handle.playback_info();
        let settings = self.handle.settings_summary();
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(6), Constraint::Min(4)])
            .split(area);

        let mut details: Vec<Line> = vec![Line::from(format!(
            "requested backend: {:?}   requested device: {}",
            settings.output_backend,
            settings
                .output_device
                .as_deref()
                .unwrap_or("<system default>")
        ))];
        match info.output_info.as_ref() {
            Some(output) => {
                details.push(Line::from(format!(
                    "negotiated: {:?} at {} Hz, {} ch, {}",
                    output.actual_backend,
                    output.actual_rate,
                    output.channels,
                    output.sample_format.label()
                )));
                details.push(Line::from(format!(
                    "access: {:?} (verified: {}, fallback: {})",
                    output.access_mode, output.access_state.verified, output.is_fallback
                )));
                details.push(Line::from(format!(
                    "exclusive: {}   device: {}",
                    output.is_exclusive, output.device_name
                )));
            }
            None => {
                details.push(Line::from("no output stream is open yet"));
                details.push(Line::from(""));
                details.push(Line::from("the engine has not negotiated an output device"));
            }
        }
        frame.render_widget(
            Paragraph::new(details).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Output — negotiated state "),
            ),
            rows[0],
        );

        let items: Vec<ListItem> = if self.output.devices.is_empty() {
            vec![ListItem::new(Line::from("no output devices found"))]
        } else {
            self.output
                .devices
                .iter()
                .map(|device| ListItem::new(Line::from(device.clone())))
                .collect()
        };
        let title = match self.output.refreshed_at {
            Some(_) => " Output — devices (r rescan, Enter select) ".to_string(),
            None => " Output — devices ".to_string(),
        };
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(title))
            .highlight_style(
                Style::default()
                    .bg(Color::Rgb(40, 60, 80))
                    .add_modifier(Modifier::BOLD),
            );
        let mut state = ListState::default();
        state.select(Some(
            self.output
                .selected
                .min(self.output.devices.len().saturating_sub(1)),
        ));
        frame.render_stateful_widget(list, rows[1], &mut state);
    }

    /// The key hint / message line.
    fn render_footer(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let message = self.message.clone().unwrap_or_default();
        let hints = Line::from(vec![
            Span::styled("Tab", accent()),
            Span::raw(" panel  "),
            Span::styled("↑↓", accent()),
            Span::raw(" select  "),
            Span::styled("←→", accent()),
            Span::raw(" adjust  "),
            Span::styled("Enter", accent()),
            Span::raw(" activate  "),
            Span::styled("Space", accent()),
            Span::raw(" play  "),
            Span::styled("/", accent()),
            Span::raw(" browse  "),
            Span::styled("f/w/t/x", accent()),
            Span::raw(" EQ  "),
            Span::styled("r", accent()),
            Span::raw(" rescan  "),
            Span::styled("q", accent()),
            Span::raw(" quit"),
        ]);
        let message_line = if message.is_empty() {
            Line::from("")
        } else {
            Line::from(Span::styled(message, Style::default().fg(Color::Yellow)))
        };
        frame.render_widget(
            Paragraph::new(vec![hints, message_line])
                .block(Block::default().borders(Borders::ALL).title(" Keys ")),
            area,
        );
    }
}

/// Format a frequency for an axis label.
fn format_frequency(frequency: f64) -> String {
    if frequency >= 1000.0 {
        format!("{:.1}k", frequency / 1000.0)
    } else {
        format!("{frequency:.0}")
    }
}

/// Resample the engine's spectrum bins into `width` bars.
///
/// The analyzer's bins are in dBFS; they are mapped onto 0..=100 so the
/// sparkline has something to draw. No value is invented: a bin the engine
/// did not produce stays at the floor.
fn resample_spectrum(bins: &[f32], width: usize) -> Vec<u64> {
    if bins.is_empty() || width == 0 {
        return Vec::new();
    }
    let mut data = Vec::with_capacity(width);
    for column in 0..width {
        let start = column * bins.len() / width;
        let end = ((column + 1) * bins.len() / width)
            .max(start + 1)
            .min(bins.len());
        let slice = &bins[start..end];
        let peak = slice.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let level = if peak.is_finite() { peak } else { -120.0 };
        let scaled = ((level + 120.0) / 120.0).clamp(0.0, 1.0) * 100.0;
        data.push(scaled.round() as u64);
    }
    data
}

// ─────────────────────────────────────────────────────────────────────────────
// Terminal setup
// ─────────────────────────────────────────────────────────────────────────────

/// Restores the terminal when dropped.
///
/// The alternate screen and raw mode are restored on *every* exit path —
/// including a panic unwinding through `main` — so a crash can never leave
/// the user's shell in raw mode.
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> std::io::Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(std::io::stdout(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Engine bootstrap
// ─────────────────────────────────────────────────────────────────────────────

/// Start the engine on a background tick thread and return its handle.
///
/// The engine owns the output device and the decode/DSP pump; a host drives
/// it with [`engine::AudioEngine::tick_blocking`] (the same contract the
/// engine's C FFI uses).
fn start_engine() -> Result<(EngineHandle, Arc<AtomicBool>, std::thread::JoinHandle<()>), String> {
    let config = EngineConfig::default();
    let (sender, receiver) = mpsc::channel::<Result<EngineHandle, String>>();
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);

    let thread = std::thread::Builder::new()
        .name("tui-engine-tick".into())
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

// ─────────────────────────────────────────────────────────────────────────────
// Entry point
// ─────────────────────────────────────────────────────────────────────────────

/// The usage text.
const USAGE: &str = "\
engine-tui — terminal player for the Shadow audio engine

USAGE:
    engine-tui [PATH]

    PATH may be a file to play or a directory to browse.

KEYS:
    Tab / Shift+Tab   cycle between UI panels
    Up / Down         select item / row (hold to accelerate)
    Left / Right      adjust selected value / parameter
    Enter             activate selected item / drill down
    Space             play / pause toggle
    /                 open file and directory browser
    f / F             equalizer: adjust frequency
    w / W             equalizer: adjust Q factor
    t                 equalizer: cycle filter type
    x                 equalizer: bypass or enable selected band
    r                 output panel: rescan hardware output devices
    q                 quit application (press twice)

NOTE:
    This build does not link a TOML parser, so the engine's `--config` file
    is not read here; the engine's own defaults are used.
";

/// Parse the command line. Pure.
fn parse_args(args: &[String]) -> Result<Option<PathBuf>, String> {
    let mut path = None;
    let mut index = 0usize;
    while index < args.len() {
        let arg = args[index].as_str();
        match arg {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            other if other.starts_with("--") => {
                return Err(format!(
                    "unknown option '{other}' (this build reads no configuration file)"
                ));
            }
            other => {
                if path.is_some() {
                    return Err("only one path may be given".to_string());
                }
                path = Some(PathBuf::from(other));
            }
        }
        index += 1;
    }
    Ok(path)
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let initial = match parse_args(&args) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("engine-tui: {message}");
            eprint!("{USAGE}");
            std::process::exit(2);
        }
    };

    let (handle, stop, thread) = match start_engine() {
        Ok(parts) => parts,
        Err(message) => {
            eprintln!("engine-tui: {message}");
            std::process::exit(1);
        }
    };
    // The professional meters are opt-in; the TUI wants them.
    handle.set_meters_enabled(true);

    let mut app = App::new(handle, initial);

    // The guard restores the terminal even if the loop below returns an
    // error or a panic unwinds.
    let _guard = match TerminalGuard::enter() {
        Ok(guard) => guard,
        Err(error) => {
            eprintln!("engine-tui: could not initialise the terminal: {error}");
            eprintln!("engine-tui: a real terminal is required — the TUI cannot run on a pipe.");
            std::process::exit(1);
        }
    };
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = match Terminal::new(backend) {
        Ok(terminal) => terminal,
        Err(error) => {
            eprintln!("engine-tui: could not create the terminal: {error}");
            std::process::exit(1);
        }
    };

    let result = run_loop(&mut terminal, &mut app);

    app.handle.stop();
    stop.store(true, Ordering::Relaxed);
    let _ = thread.join();

    result
}

/// The event loop.
fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> std::io::Result<()> {
    loop {
        terminal.draw(|frame| app.render(frame))?;
        if app.should_quit() {
            return Ok(());
        }
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    if let Some(action) = keys::handle_key(key) {
                        app.apply(action);
                    }
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn key_with(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    /// The README's keybinding table, row by row.
    ///
    /// Every row is asserted below; if a binding is dropped from the
    /// implementation, this table stops matching and the test fails.
    const README_BINDINGS: &[(KeyCode, KeyModifiers, keys::Action)] = &[
        (KeyCode::Tab, KeyModifiers::NONE, keys::Action::NextPanel),
        (KeyCode::Tab, KeyModifiers::SHIFT, keys::Action::PrevPanel),
        (KeyCode::Up, KeyModifiers::NONE, keys::Action::SelectUp),
        (KeyCode::Down, KeyModifiers::NONE, keys::Action::SelectDown),
        (KeyCode::Left, KeyModifiers::NONE, keys::Action::AdjustLeft),
        (
            KeyCode::Right,
            KeyModifiers::NONE,
            keys::Action::AdjustRight,
        ),
        (KeyCode::Enter, KeyModifiers::NONE, keys::Action::Activate),
        (
            KeyCode::Char(' '),
            KeyModifiers::NONE,
            keys::Action::TogglePlay,
        ),
        (
            KeyCode::Char('/'),
            KeyModifiers::NONE,
            keys::Action::OpenBrowser,
        ),
        (
            KeyCode::Char('f'),
            KeyModifiers::NONE,
            keys::Action::EqFrequency(-1),
        ),
        (
            KeyCode::Char('F'),
            KeyModifiers::NONE,
            keys::Action::EqFrequency(1),
        ),
        (
            KeyCode::Char('w'),
            KeyModifiers::NONE,
            keys::Action::EqQ(-1),
        ),
        (KeyCode::Char('W'), KeyModifiers::NONE, keys::Action::EqQ(1)),
        (
            KeyCode::Char('t'),
            KeyModifiers::NONE,
            keys::Action::EqCycleFilter,
        ),
        (
            KeyCode::Char('x'),
            KeyModifiers::NONE,
            keys::Action::EqToggleBand,
        ),
        (
            KeyCode::Char('r'),
            KeyModifiers::NONE,
            keys::Action::RescanDevices,
        ),
        (KeyCode::Char('q'), KeyModifiers::NONE, keys::Action::Quit),
    ];

    #[test]
    fn every_readme_keybinding_is_implemented() {
        assert_eq!(README_BINDINGS.len(), 17, "the README table has 17 keys");
        for (code, modifiers, expected) in README_BINDINGS {
            let event = key_with(*code, *modifiers);
            assert_eq!(
                keys::handle_key(event),
                Some(*expected),
                "README binding {code:?} with {modifiers:?} must map to {expected:?}"
            );
        }
    }

    #[test]
    fn the_readme_table_rows_are_all_covered() {
        // One assertion per README row, so a row cannot be dropped from the
        // table above without this test noticing.
        let mut covered: Vec<&'static str> = Vec::new();
        for (code, modifiers, _) in README_BINDINGS {
            covered.push(match (*code, *modifiers) {
                (KeyCode::Tab, KeyModifiers::NONE) => "Tab",
                (KeyCode::Tab, KeyModifiers::SHIFT) => "Shift+Tab",
                (KeyCode::Up, _) => "Up",
                (KeyCode::Down, _) => "Down",
                (KeyCode::Left, _) => "Left",
                (KeyCode::Right, _) => "Right",
                (KeyCode::Enter, _) => "Enter",
                (KeyCode::Char(' '), _) => "Space",
                (KeyCode::Char('/'), _) => "/",
                (KeyCode::Char('f'), _) => "f",
                (KeyCode::Char('F'), _) => "F",
                (KeyCode::Char('w'), _) => "w",
                (KeyCode::Char('W'), _) => "W",
                (KeyCode::Char('t'), _) => "t",
                (KeyCode::Char('x'), _) => "x",
                (KeyCode::Char('r'), _) => "r",
                (KeyCode::Char('q'), _) => "q",
                _ => panic!("unclassified binding {code:?}"),
            });
        }
        for expected in [
            "Tab",
            "Shift+Tab",
            "Up",
            "Down",
            "Left",
            "Right",
            "Enter",
            "Space",
            "/",
            "f",
            "F",
            "w",
            "W",
            "t",
            "x",
            "r",
            "q",
        ] {
            assert!(
                covered.contains(&expected),
                "the README binding '{expected}' is not implemented"
            );
        }
    }

    #[test]
    fn shift_plus_f_is_the_uppercase_frequency_binding() {
        // Some terminals deliver Shift+f as 'f' with the SHIFT modifier.
        assert_eq!(
            keys::handle_key(key_with(KeyCode::Char('f'), KeyModifiers::SHIFT)),
            Some(keys::Action::EqFrequency(1))
        );
        assert_eq!(
            keys::handle_key(key_with(KeyCode::Char('w'), KeyModifiers::SHIFT)),
            Some(keys::Action::EqQ(1))
        );
    }

    #[test]
    fn key_release_events_are_ignored() {
        let mut event = key(KeyCode::Char('q'));
        event.kind = KeyEventKind::Release;
        assert_eq!(keys::handle_key(event), None);
    }

    #[test]
    fn auto_repeat_still_maps_to_an_action() {
        let mut event = key(KeyCode::Char(' '));
        event.kind = KeyEventKind::Repeat;
        assert_eq!(keys::handle_key(event), Some(keys::Action::TogglePlay));
    }

    #[test]
    fn unbound_keys_produce_no_action() {
        assert_eq!(keys::handle_key(key(KeyCode::Char('z'))), None);
        assert_eq!(keys::handle_key(key(KeyCode::F(5))), None);
        assert_eq!(keys::handle_key(key(KeyCode::Esc)), None);
    }

    #[test]
    fn panels_cycle_in_both_directions() {
        assert_eq!(Panel::Browser.next(), Panel::Transport);
        assert_eq!(Panel::Transport.next(), Panel::Equalizer);
        assert_eq!(Panel::Equalizer.next(), Panel::Output);
        assert_eq!(Panel::Output.next(), Panel::Browser);
        assert_eq!(Panel::Browser.prev(), Panel::Output);
        assert_eq!(Panel::Output.prev(), Panel::Equalizer);
        assert_eq!(Panel::ALL.len(), 4);
    }

    #[test]
    fn the_filter_cycle_covers_the_readme_filter_types() {
        assert!(FILTER_TYPE_CYCLE.contains(&EqFilterType::Peaking));
        assert!(FILTER_TYPE_CYCLE.contains(&EqFilterType::LowShelf));
        assert!(FILTER_TYPE_CYCLE.contains(&EqFilterType::HighShelf));
        assert!(FILTER_TYPE_CYCLE.contains(&EqFilterType::Notch));
        assert!(FILTER_TYPE_CYCLE.contains(&EqFilterType::HighPass));
        assert!(FILTER_TYPE_CYCLE.contains(&EqFilterType::LowPass));
        // The cycle must return to the start.
        let mut current = EqFilterType::Peaking;
        for _ in 0..FILTER_TYPE_CYCLE.len() {
            current = next_filter_type(current);
        }
        assert_eq!(current, EqFilterType::Peaking);
    }

    #[test]
    fn the_eq_curve_is_computed_from_the_bands() {
        let bands = vec![EqBandSetting {
            frequency: 1000.0,
            gain_db: 6.0,
            q: 1.0,
            filter_type: EqFilterType::Peaking,
            enabled: true,
        }];
        let curve = eq_curve(&bands, 48_000.0);
        assert!(!curve.is_empty());
        // A +6 dB peaking band must peak at its centre frequency.
        let centre = curve
            .iter()
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .expect("a curve point");
        assert!(
            centre.1 > 5.0,
            "the peak must show the boost, got {centre:?}"
        );
        // ... and be flat far away from it.
        let low = curve.first().expect("first point");
        assert!(low.1.abs() < 0.5, "the low end must be flat, got {low:?}");
    }

    #[test]
    fn a_disabled_band_does_not_change_the_curve() {
        let band = EqBandSetting {
            frequency: 1000.0,
            gain_db: 12.0,
            q: 1.0,
            filter_type: EqFilterType::Peaking,
            enabled: false,
        };
        let curve = eq_curve(&[band], 48_000.0);
        assert!(curve.iter().all(|(_, db)| db.abs() < 0.01));
    }

    #[test]
    fn the_spectrum_resampler_keeps_the_peak() {
        let bins = vec![-120.0, -60.0, 0.0, -30.0];
        let data = resample_spectrum(&bins, 4);
        assert_eq!(data.len(), 4);
        assert_eq!(data.iter().copied().max(), Some(100));
        assert!(resample_spectrum(&[], 4).is_empty());
    }

    #[test]
    fn the_browser_selection_is_clamped() {
        let mut browser = Browser {
            cwd: PathBuf::from("."),
            entries: vec![
                Entry {
                    name: "a".to_string(),
                    path: PathBuf::from("a"),
                    is_dir: false,
                    is_parent: false,
                },
                Entry {
                    name: "b".to_string(),
                    path: PathBuf::from("b"),
                    is_dir: false,
                    is_parent: false,
                },
            ],
            selected: 0,
        };
        browser.move_selection(-1);
        assert_eq!(browser.selected, 0);
        browser.move_selection(5);
        assert_eq!(browser.selected, 1);
    }

    #[test]
    fn the_repeat_acceleration_grows_the_step() {
        let mut repeat = Repeat::default();
        let now = Instant::now();
        assert_eq!(repeat.step(keys::Action::SelectDown, now), 1);
        assert_eq!(repeat.step(keys::Action::SelectDown, now), 1);
        assert_eq!(repeat.step(keys::Action::SelectDown, now), 1);
        assert_eq!(repeat.step(keys::Action::SelectDown, now), 1);
        assert_eq!(repeat.step(keys::Action::SelectDown, now), 2);
    }

    #[test]
    fn the_command_line_accepts_one_path() {
        assert_eq!(
            parse_args(&["/music".to_string()]).expect("parse"),
            Some(PathBuf::from("/music"))
        );
        assert_eq!(parse_args(&[]).expect("parse"), None);
        assert!(parse_args(&["a".to_string(), "b".to_string()]).is_err());
        assert!(parse_args(&["--config".to_string()]).is_err());
    }
}
