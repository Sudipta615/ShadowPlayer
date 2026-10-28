//! Acoustic alignment & speaker setup suite.
//!
//! Provides geometric distance compensation, wavefront arrival-time alignment,
//! per-channel level calibration, and Linkwitz-Riley bass management crossover.
//!
//! Mathematical distance delay model:
//! ```text
//! Δt_i = (d_max - d_i) / v_sound * 1000 ms
//! ```
//! where `v_sound = 343.2 m/s` (or `1126.0 ft/s`). Closer speakers are delayed
//! so acoustic wavefronts reach the listener sweet spot at the exact same instant.

mod canvas;
mod controls;
mod types;

use audio_session::AudioSession;
use config::{BassManagementConfig, ChannelTrimConfig, ChannelTrimEntry};
use eframe::egui::{self, Color32, CornerRadius, Vec2};

use crate::theme::Theme;
pub use types::*;

impl SpeakerSetupModalState {
    pub fn open(&mut self) {
        self.is_open = true;
        self.status_banner = None;
    }

    pub fn close(&mut self) {
        self.is_open = false;
    }

    pub fn set_layout(&mut self, layout: SpeakerLayout) {
        self.layout = layout;
        self.speakers = layout
            .roles()
            .iter()
            .map(|&r| SpeakerChannelConfig::new(r, self.unit))
            .collect();
        self.selected_idx = 0;
    }

    pub fn set_unit(&mut self, target: DistanceUnit) {
        if self.unit == target {
            return;
        }
        for s in &mut self.speakers {
            s.distance = self.unit.convert_to(s.distance, target);
        }
        self.unit = target;
    }

    pub fn max_distance(&self) -> f32 {
        self.speakers
            .iter()
            .map(|s| s.distance)
            .fold(0.0_f32, f32::max)
    }

    pub fn delay_ms_for(&self, speaker: &SpeakerChannelConfig) -> f32 {
        let max_d = self.max_distance();
        let speed = self.unit.speed_of_sound();
        if speed <= 0.0 || max_d <= speaker.distance {
            0.0
        } else {
            ((max_d - speaker.distance) / speed * 1000.0).clamp(0.0, 100.0)
        }
    }

    pub fn to_engine_trim_config(&self) -> ChannelTrimConfig {
        let entries = self
            .speakers
            .iter()
            .map(|s| {
                let delay = if s.role.is_subwoofer() {
                    0.0
                } else {
                    self.delay_ms_for(s)
                };
                ChannelTrimEntry {
                    channel: s.channel,
                    gain_db: s.trim_db,
                    delay_ms: delay,
                    invert: s.invert,
                }
            })
            .collect();

        ChannelTrimConfig {
            enabled: true,
            entries,
        }
    }

    pub fn to_engine_bass_config(&self) -> BassManagementConfig {
        let sub_calc_delay = self
            .speakers
            .iter()
            .find(|s| s.role.is_subwoofer())
            .map(|s| self.delay_ms_for(s))
            .unwrap_or(0.0);
        let total_sub_delay = (sub_calc_delay + self.sub_fine_delay_ms).clamp(0.0, 50.0);

        BassManagementConfig {
            enabled: self.bass_management_enabled,
            mains_highpass_enabled: self.mains_highpass,
            crossover_hz: self.crossover_hz,
            q: self.filter_q,
            slope: self.slope,
            filter_type: self.filter_type,
            sub_delay_ms: total_sub_delay,
            sub_phase_degrees: self.sub_phase_deg,
            sub_polarity_invert: self.sub_polarity_invert,
        }
    }

    pub fn apply_to_session(&mut self, session: &AudioSession) {
        session.set_channel_trim(self.to_engine_trim_config());
        session.set_bass_management(self.to_engine_bass_config());
        self.status_banner = Some(tr!("Speaker alignment applied to audio engine"));
    }

    pub fn reset_defaults(&mut self) {
        let unit = self.unit;
        for s in &mut self.speakers {
            *s = SpeakerChannelConfig::new(s.role, unit);
        }
        self.crossover_hz = 80.0;
        self.speaker_size = SpeakerSizePreset::Bookshelf;
        self.sub_phase_deg = 0.0;
        self.sub_polarity_invert = false;
        self.sub_fine_delay_ms = 0.0;
        self.status_banner = None;
    }
}

/// Render the interactive speaker setup modal dialog.
pub fn show_speaker_setup_modal(
    ctx: &egui::Context,
    state: &mut SpeakerSetupModalState,
    session: &AudioSession,
    theme: &Theme,
) {
    if !state.is_open {
        return;
    }

    let mut changed = false;

    let closed = crate::dialog::show_modal_scrollable(
        ctx,
        "speaker_setup_acoustic_alignment",
        &tr!("Speaker Setup & Acoustic Alignment"),
        680.0,
        theme,
        |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_md);

            // Subtitle & introduction
            ui.label(
                egui::RichText::new(tr!(
                    "Calibrate speaker distances, arrival delay compensation, level trims, and subwoofer crossover"
                ))
                .font(crate::theme::font(11.5))
                .color(theme.text_secondary),
            );

            // ── Top Controls: Layout & Unit Selectors ─────────────────────────
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(tr!("Speaker Layout"))
                        .font(crate::theme::font(12.0))
                        .strong()
                        .color(theme.text_primary),
                );
                for (layout, label) in [
                    (SpeakerLayout::Stereo20, tr!("Stereo (2.0)")),
                    (SpeakerLayout::Stereo21, tr!("Stereo + Sub (2.1)")),
                    (SpeakerLayout::Lcr31, tr!("LCR + Sub (3.1)")),
                    (SpeakerLayout::Surround51, tr!("Surround (5.1)")),
                    (SpeakerLayout::Surround71, tr!("Surround (7.1)")),
                ] {
                    let active = state.layout == layout;
                    if ui
                        .add(crate::components::settings_tab::pill_btn(
                            label, active, theme,
                        ))
                        .clicked()
                        && !active
                    {
                        state.set_layout(layout);
                        changed = true;
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for (u, label) in [
                        (DistanceUnit::Feet, tr!("Feet")),
                        (DistanceUnit::Meters, tr!("Meters")),
                    ] {
                        let active = state.unit == u;
                        if ui
                            .add(crate::components::settings_tab::pill_btn(
                                label, active, theme,
                            ))
                            .clicked()
                            && !active
                        {
                            state.set_unit(u);
                        }
                    }
                    ui.label(
                        egui::RichText::new(tr!("Distance Unit"))
                            .font(crate::theme::font(11.0))
                            .color(theme.text_secondary),
                    );
                });
            });

            // ── 2D Soundstage Geometry Canvas ─────────────────────────────────
            canvas::render_soundstage_canvas(ui, state, theme, &mut changed);

            // ── Channel Inspector & Trim Sliders ──────────────────────────────
            if let Some(selected) = state.speakers.get_mut(state.selected_idx) {
                controls::render_selected_speaker_card(
                    ui,
                    selected,
                    state.unit,
                    theme,
                    &mut changed,
                );
            }

            // ── Crossover & Bass Management Suite ─────────────────────────────
            controls::render_bass_management_card(ui, state, theme, &mut changed);

            // ── Bottom Action & Engine Sync Bar ───────────────────────────────
            ui.add_space(theme.space_sm);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(tr!("Apply Alignment to Engine"))
                                .font(crate::theme::font(13.0))
                                .strong()
                                .color(Color32::BLACK),
                        )
                        .fill(theme.bit_perfect_green)
                        .corner_radius(CornerRadius::same(theme.radius_md)),
                    )
                    .clicked()
                {
                    state.apply_to_session(session);
                }

                ui.add_space(theme.space_sm);
                let mut live = state.live_preview;
                if ui
                    .checkbox(&mut live, tr!("Live Preview"))
                    .on_hover_text("Instantly apply distance delays and trims as sliders move")
                    .changed()
                {
                    state.live_preview = live;
                    if live {
                        state.apply_to_session(session);
                    }
                }

                if ui.button(tr!("Reset to Defaults")).clicked() {
                    state.reset_defaults();
                    changed = true;
                }

                if let Some(ref banner) = state.status_banner {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(banner)
                                .font(crate::theme::font(11.0))
                                .color(theme.bit_perfect_green),
                        );
                    });
                }
            });
        },
    );

    if closed {
        state.is_open = false;
    }

    if changed && state.live_preview {
        state.apply_to_session(session);
    }
}

#[cfg(test)]
mod tests;
