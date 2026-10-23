//! Room-correction measurement wizard.
//!
//! The engine can already play an exponential sweep, capture it, deconvolve it
//! and land the result. This wizard is the human side of that sequence: choose
//! the microphone, choose how long the sweep runs, watch each stage, and cancel
//! without leaving a half-written recording behind.
//!
//! The wizard owns no engine state. It consumes [`RoomMeasurementEvent`]s that
//! the playback coordinator drains each frame, and returns small action flags
//! that the app turns into session calls.

use crate::theme::Theme;
use eframe::egui;
use playback::{MeasurementInputDevice, RoomMeasurementEvent};

/// Which part of the measurement flow the user is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasurementPhase {
    Setup,
    Running,
    Complete,
    Failed,
    Cancelled,
}

/// A completed measurement, ready to display.
#[derive(Debug, Clone, PartialEq)]
pub struct MeasurementResult {
    pub path: std::path::PathBuf,
    pub snr_db: f32,
}

pub struct RoomMeasurementState {
    pub is_open: bool,
    pub phase: MeasurementPhase,
    pub devices: Vec<MeasurementInputDevice>,
    /// Empty means "engine default / system mix".
    pub selected_device: String,
    pub seconds: f32,
    pub pre_emphasis: f32,
    pub stage: String,
    pub result: Option<MeasurementResult>,
    pub error: Option<String>,
    /// Set when the wizard opens and needs the engine to enumerate devices.
    pub needs_devices: bool,
}

impl Default for RoomMeasurementState {
    fn default() -> Self {
        Self {
            is_open: false,
            phase: MeasurementPhase::Setup,
            devices: Vec::new(),
            selected_device: String::new(),
            seconds: 5.0,
            pre_emphasis: 6.0,
            stage: String::new(),
            result: None,
            error: None,
            needs_devices: false,
        }
    }
}

impl RoomMeasurementState {
    /// Open the wizard in its setup step. Device enumeration is requested by
    /// the app, which owns the coordinator.
    pub fn open(&mut self) {
        self.is_open = true;
        self.phase = MeasurementPhase::Setup;
        self.stage.clear();
        self.result = None;
        self.error = None;
        self.needs_devices = true;
    }

    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Apply one event drained from the playback coordinator.
    pub fn apply_event(&mut self, event: RoomMeasurementEvent) {
        match event {
            RoomMeasurementEvent::InputDevices(devices) => {
                self.devices = devices;
                // Keep a still-present choice; otherwise fall back to the
                // default device so the combo never points at nothing.
                if !self.selected_device.is_empty()
                    && !self.devices.iter().any(|d| d.name == self.selected_device)
                {
                    self.selected_device.clear();
                }
            }
            RoomMeasurementEvent::Progress(stage) => {
                if self.phase == MeasurementPhase::Running {
                    self.stage = stage;
                }
            }
            RoomMeasurementEvent::Complete { path, snr_db } => {
                self.phase = MeasurementPhase::Complete;
                self.result = Some(MeasurementResult { path, snr_db });
                self.stage = "Correction derived and enabled".to_string();
                self.error = None;
            }
            RoomMeasurementEvent::Failed(message) => {
                self.phase = MeasurementPhase::Failed;
                self.error = Some(message);
                self.stage.clear();
            }
            RoomMeasurementEvent::Cancelled => {
                self.phase = MeasurementPhase::Cancelled;
                self.stage.clear();
            }
        }
    }

    /// Mark the start locally so the Running view appears before the engine's
    /// first progress event arrives.
    pub fn mark_started(&mut self) {
        self.phase = MeasurementPhase::Running;
        self.stage = "Starting capture…".to_string();
        self.error = None;
        self.result = None;
    }

    pub fn is_running(&self) -> bool {
        self.phase == MeasurementPhase::Running
    }

    /// The device to send with the command: empty means default.
    pub fn requested_device(&self) -> Option<String> {
        let name = self.selected_device.trim();
        (!name.is_empty()).then(|| name.to_string())
    }
}

#[derive(Default)]
pub struct RoomMeasurementActions {
    /// Start a sweep with the wizard's current choices.
    pub start: bool,
    /// Stop an in-flight sweep and discard its recording.
    pub cancel: bool,
    /// Return to the setup step from a terminal state.
    pub restart: bool,
    /// Close the wizard.
    pub close: bool,
    /// Ask the app for a save location and copy the measured capture there.
    pub save_ir: bool,
    /// Ask the app for an impulse-response file and load it instead.
    pub load_ir: bool,
}

/// Draw the wizard. Returns actions; the app performs them because only it
/// owns the session and the native file pickers.
pub fn show_room_measurement_modal(
    ctx: &egui::Context,
    state: &mut RoomMeasurementState,
    theme: &Theme,
) -> RoomMeasurementActions {
    let mut actions = RoomMeasurementActions::default();
    if !state.is_open {
        return actions;
    }

    let closed = crate::dialog::show_modal_scrollable(
        ctx,
        "room_measurement_wizard",
        "Room Correction Measurement",
        560.0,
        theme,
        |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::new(0.0, theme.space_lg);

            ui.label(
                egui::RichText::new(
                    "Plays a short exponential sine sweep through your speakers and records it \
                     with the selected microphone. ShadowPlayer deconvolves the recording into \
                     an impulse response, derives the correction and enables it.",
                )
                .font(crate::theme::font(12.0))
                .color(theme.text_muted),
            );
            ui.add_space(theme.space_lg);

            match state.phase {
                MeasurementPhase::Setup => {
                    static_hint(
                        ui,
                        theme,
                        "Turn off crossfeed, EQ and other correction while measuring. Keep the \
                         room quiet and place the microphone at the listening position.",
                    );

                    setting_row(
                        ui,
                        theme,
                        "Input device",
                        "The microphone or interface input used for the recording.",
                        |ui| {
                            let selected = if state.selected_device.is_empty() {
                                "System default / loopback".to_string()
                            } else {
                                state.selected_device.clone()
                            };
                            egui::ComboBox::from_id_salt("room_measurement_device")
                            .selected_text(selected)
                            .width(250.0)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(state.selected_device.is_empty(), "System default / loopback")
                                    .clicked()
                                {
                                    state.selected_device.clear();
                                }
                                for device in &state.devices {
                                    let label = if device.is_default {
                                        format!("{} (default)", device.name)
                                    } else {
                                        device.name.clone()
                                    };
                                    let response = ui.selectable_label(
                                        state.selected_device == device.name,
                                        label,
                                    );
                                    let response = if device.measurement_capable {
                                        response
                                    } else {
                                        response.on_disabled_hover_text(
                                            "This device does not advertise a standard capture rate",
                                        )
                                    };
                                    if device.measurement_capable && response.clicked() {
                                        state.selected_device = device.name.clone();
                                    }
                                }
                            });
                            if state.devices.is_empty() {
                                ui.label(
                                    egui::RichText::new("No input devices reported yet")
                                        .font(crate::theme::font(10.5))
                                        .color(theme.text_dim),
                                );
                            }
                        },
                    );

                    setting_row(
                        ui,
                        theme,
                        "Sweep length",
                        "Longer sweeps average more of the room; five seconds is a good default.",
                        |ui| {
                            ui.add(
                                egui::Slider::new(&mut state.seconds, 1.0..=30.0)
                                    .suffix(" s")
                                    .step_by(0.5),
                            );
                        },
                    );

                    setting_row(ui, theme, "Pre-emphasis", "Boosts high frequencies in the sweep to improve their signal-to-noise ratio.", |ui| {
                        ui.add(
                            egui::Slider::new(&mut state.pre_emphasis, 0.0..=12.0)
                                .suffix(" dB")
                                .step_by(1.0),
                        );
                    });

                    ui.add_space(theme.space_md);
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Start Measurement")
                                        .font(crate::theme::font(12.5))
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                )
                                .fill(theme.accent)
                                .corner_radius(egui::CornerRadius::same(theme.radius_md)),
                            )
                            .clicked()
                        {
                            actions.start = true;
                        }
                        if ui.button("Load Impulse Response…").clicked() {
                            actions.load_ir = true;
                        }
                    });
                }
                MeasurementPhase::Running => {
                    ui.label(
                        egui::RichText::new("Measuring")
                            .font(crate::theme::font(15.0))
                            .strong()
                            .color(theme.text_primary),
                    );
                    ui.label(
                        egui::RichText::new(if state.stage.is_empty() {
                            "Waiting for the engine…"
                        } else {
                            state.stage.as_str()
                        })
                        .font(crate::theme::font(12.0))
                        .color(theme.accent_highlight),
                    );
                    ui.add(
                        egui::ProgressBar::new((ctx.input(|i| i.time) as f32 % 1.0).max(0.02))
                            .animate(true)
                            .desired_width(f32::INFINITY)
                            .fill(theme.accent),
                    );
                    ui.label(
                        egui::RichText::new(
                            "The sweep is playing now. Stay quiet until the measurement finishes.",
                        )
                        .font(crate::theme::font(11.0))
                        .color(theme.text_muted),
                    );
                    ui.add_space(theme.space_sm);
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Cancel Measurement")
                                    .font(crate::theme::font(12.0))
                                    .color(theme.destructive),
                            )
                            .corner_radius(egui::CornerRadius::same(theme.radius_md)),
                        )
                        .clicked()
                    {
                        actions.cancel = true;
                    }
                    // Animation while a stage is in flight.
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
                MeasurementPhase::Complete => {
                    if let Some(result) = &state.result {
                        static_hint_ok(
                            ui,
                            theme,
                            "The correction is now live. Audition familiar music before deciding on the depth.",
                        );
                        setting_row(
                            ui,
                            theme,
                            "Measurement SNR",
                            "Higher is better; low values mean the room was too noisy.",
                            |ui| {
                                ui.label(
                                    egui::RichText::new(format!("{:.1} dB", result.snr_db))
                                        .font(crate::theme::mono_font(13.0))
                                        .strong()
                                        .color(if result.snr_db >= 20.0 {
                                            theme.bit_perfect_green
                                        } else {
                                            theme.warning_yellow
                                        }),
                                );
                            },
                        );
                        setting_row(
                            ui,
                            theme,
                            "Recording",
                            "The captured sweep backing the correction.",
                            |ui| {
                                ui.label(
                                    egui::RichText::new(result.path.display().to_string())
                                        .font(crate::theme::mono_font(10.5))
                                        .color(theme.text_muted),
                                )
                                .on_hover_text(result.path.display().to_string());
                            },
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Save Measurement WAV…").clicked() {
                                actions.save_ir = true;
                            }
                            if ui.button("Measure Again").clicked() {
                                actions.restart = true;
                            }
                        });
                    }
                }
                MeasurementPhase::Failed => {
                    static_hint_error(
                        ui,
                        theme,
                        state.error.as_deref().unwrap_or("The measurement failed."),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Try Again").clicked() {
                            actions.restart = true;
                        }
                        if ui.button("Load Impulse Response…").clicked() {
                            actions.load_ir = true;
                        }
                    });
                }
                MeasurementPhase::Cancelled => {
                    static_hint(
                        ui,
                        theme,
                        "Measurement cancelled. No partial recording was kept and your previous \
                         correction is unchanged.",
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Start Again").clicked() {
                            actions.restart = true;
                        }
                        if ui.button("Load Impulse Response…").clicked() {
                            actions.load_ir = true;
                        }
                    });
                }
            }

            ui.add_space(theme.space_md);
            ui.label(
                egui::RichText::new(
                    "Microphone access is used only while this wizard is measuring.",
                )
                .font(crate::theme::font(10.5))
                .color(theme.text_dim),
            );
        },
    );

    if closed {
        if state.is_running() {
            actions.cancel = true;
        }
        actions.close = true;
    }
    actions
}

fn setting_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    detail: &str,
    mut content: impl FnMut(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(210.0);
            ui.label(
                egui::RichText::new(label)
                    .font(crate::theme::font(12.0))
                    .strong()
                    .color(theme.text_primary),
            );
            ui.label(
                egui::RichText::new(detail)
                    .font(crate::theme::font(10.5))
                    .color(theme.text_muted),
            );
        });
        ui.add_space(theme.space_lg);
        ui.vertical(|ui| content(ui));
    });
    ui.add_space(theme.space_sm);
}

fn static_hint(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    egui::Frame::new()
        .fill(theme.bg_pill)
        .corner_radius(egui::CornerRadius::same(theme.radius_md))
        .inner_margin(egui::Margin::same(theme.space_lg as i8))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text)
                    .font(crate::theme::font(11.5))
                    .color(theme.text_secondary),
            );
        });
}

fn static_hint_ok(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    egui::Frame::new()
        .fill(theme.bit_perfect_green.gamma_multiply(0.14))
        .corner_radius(egui::CornerRadius::same(theme.radius_md))
        .inner_margin(egui::Margin::same(theme.space_lg as i8))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text)
                    .font(crate::theme::font(11.5))
                    .color(theme.bit_perfect_green),
            );
        });
}

fn static_hint_error(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    egui::Frame::new()
        .fill(theme.destructive.gamma_multiply(0.12))
        .corner_radius(egui::CornerRadius::same(theme.radius_md))
        .inner_margin(egui::Margin::same(theme.space_lg as i8))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text)
                    .font(crate::theme::font(11.5))
                    .color(theme.destructive),
            );
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str) -> MeasurementInputDevice {
        MeasurementInputDevice {
            name: name.to_string(),
            is_default: false,
            channels: vec![1, 2],
            sample_rates: vec![48_000],
            measurement_capable: true,
        }
    }

    #[test]
    fn progress_only_counts_while_running() {
        let mut state = RoomMeasurementState::default();
        state.apply_event(RoomMeasurementEvent::Progress("stray".into()));
        assert!(state.stage.is_empty(), "setup does not show a stray stage");

        state.mark_started();
        state.apply_event(RoomMeasurementEvent::Progress("capture started".into()));
        assert_eq!(state.stage, "capture started");
        assert!(state.is_running());
    }

    #[test]
    fn completion_carries_the_recording_and_snr() {
        let mut state = RoomMeasurementState::default();
        state.mark_started();
        state.apply_event(RoomMeasurementEvent::Complete {
            path: std::path::PathBuf::from("/tmp/sweep.wav"),
            snr_db: 31.5,
        });
        assert_eq!(state.phase, MeasurementPhase::Complete);
        assert_eq!(state.result.as_ref().unwrap().snr_db, 31.5);
        assert!(!state.is_running());
    }

    #[test]
    fn failure_and_cancellation_are_distinct_terminal_states() {
        let mut state = RoomMeasurementState::default();
        state.mark_started();
        state.apply_event(RoomMeasurementEvent::Failed("no mic".into()));
        assert_eq!(state.phase, MeasurementPhase::Failed);
        assert_eq!(state.error.as_deref(), Some("no mic"));

        state.mark_started();
        state.apply_event(RoomMeasurementEvent::Cancelled);
        assert_eq!(state.phase, MeasurementPhase::Cancelled);
        assert!(state.error.is_none());
    }

    #[test]
    fn a_removed_device_is_cleared_from_the_selection() {
        let mut state = RoomMeasurementState::default();
        state.apply_event(RoomMeasurementEvent::InputDevices(vec![device("USB Mic")]));
        state.selected_device = "USB Mic".to_string();
        state.apply_event(RoomMeasurementEvent::InputDevices(vec![device("Built-in")]));
        assert!(state.selected_device.is_empty());

        state.selected_device = "Built-in".to_string();
        state.apply_event(RoomMeasurementEvent::InputDevices(vec![device("Built-in")]));
        assert_eq!(state.requested_device().as_deref(), Some("Built-in"));
    }
}
