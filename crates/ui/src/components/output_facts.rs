//! The negotiated output facts, shared by every diagnostics surface.
//!
//! Settings and the Bit-Perfect inspector used to assemble their own rows from
//! different fields. They could disagree — one said "exclusive", the other
//! inferred it from a boolean — and neither showed the verification reason.
//! Both now render this exact list, so a fact cannot appear in one place and be
//! missing or worded differently in the other.

use config::OutputAccessMode;
use eframe::egui;
use engine::output::OutputInfo;

use crate::theme::Theme;

/// How much confidence the row should communicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactTone {
    /// Plain information.
    Neutral,
    /// A verified, direct, bit-perfect condition.
    Good,
    /// A fallback, an estimate, or an unverified claim.
    Warn,
}

/// One row of negotiated output state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputFact {
    pub id: &'static str,
    pub label: &'static str,
    /// The concise value shown in the control column.
    pub value: String,
    /// A sentence explaining what the value means.
    pub detail: &'static str,
    pub tone: FactTone,
}

/// Build the canonical diagnostics rows.
///
/// `latency_ms` is measured by the session's graph, not by `OutputInfo`; it is
/// passed in so the buffer row can state both the negotiated frame counts and
/// the resulting round-trip estimate.
pub fn output_facts(info: Option<&OutputInfo>, latency_ms: f32) -> Vec<OutputFact> {
    let Some(info) = info else {
        return vec![
            OutputFact {
                id: "requested_backend",
                label: "Requested backend",
                value: "Waiting for output".into(),
                detail: "The engine has not opened an output yet.",
                tone: FactTone::Neutral,
            },
            OutputFact {
                id: "negotiated_backend",
                label: "Negotiated backend",
                value: "No device open".into(),
                detail: "No backend has completed negotiation with the operating system.",
                tone: FactTone::Warn,
            },
            OutputFact {
                id: "verification",
                label: "Verification",
                value: "Not verified".into(),
                detail: "Direct, unmixed hardware access has not been confirmed.",
                tone: FactTone::Warn,
            },
        ];
    };

    let requested = info
        .requested_backend
        .map(crate::components::backend_options::backend_label)
        .unwrap_or_else(|| "Auto / not specified".to_string());
    let negotiated = info
        .actual_backend
        .map(crate::components::backend_options::backend_label)
        .unwrap_or_else(|| "Unknown".to_string());
    let access = access_mode_label(info.access_state.actual);
    let verified = info.access_state.verified && info.access_state.is_bit_perfect();

    let verification_value = if verified {
        "Verified: direct hardware access".to_string()
    } else if let Some(reason) = info
        .fallback_reason
        .as_deref()
        .filter(|reason| !reason.trim().is_empty())
    {
        format!("Not verified — {reason}")
    } else if info.access_state.actual.is_direct() {
        "Reported direct, not independently verified".to_string()
    } else {
        "Not verified — OS mixer/shared access".to_string()
    };

    let device = if info.device_name.trim().is_empty() {
        "Not reported".to_string()
    } else {
        info.device_name.clone()
    };

    let rate_note = if info.requested_rate > 0
        && info.actual_rate > 0
        && info.requested_rate != info.actual_rate
    {
        format!(
            "{} (requested {} Hz)",
            info.format_summary(),
            info.requested_rate
        )
    } else {
        info.format_summary()
    };

    let buffer_value = if info.buffer_size_frames == 0 {
        "Not reported by the backend".to_string()
    } else {
        let estimate = if info.buffer_size_estimated {
            " (estimated)"
        } else {
            ""
        };
        let period = if info.period_frames > 0 {
            format!(" · {}-frame period", info.period_frames)
        } else {
            String::new()
        };
        format!(
            "{}-frame buffer{period}{estimate} · {latency_ms:.1} ms latency",
            info.buffer_size_frames
        )
    };

    vec![
        OutputFact {
            id: "requested_backend",
            label: "Requested backend",
            value: requested,
            detail: "The backend selected in Settings. Auto asks the engine to choose the best available path.",
            tone: FactTone::Neutral,
        },
        OutputFact {
            id: "negotiated_backend",
            label: "Negotiated backend",
            value: negotiated,
            detail: "The backend that actually opened the device. A difference from the request means a fallback.",
            tone: if info.is_fallback {
                FactTone::Warn
            } else {
                FactTone::Neutral
            },
        },
        OutputFact {
            id: "access_mode",
            label: "Access mode",
            value: access.to_string(),
            detail: "Shared streams pass through the OS mixer. Direct hardware and exclusive streams bypass it.",
            tone: if info.access_state.actual.is_direct() {
                FactTone::Good
            } else {
                FactTone::Warn
            },
        },
        OutputFact {
            id: "verification",
            label: "Verification",
            value: verification_value,
            detail: "Verification is a property of the opened stream, not the device name. Only a verified direct stream supports the bit-perfect claim.",
            tone: if verified {
                FactTone::Good
            } else {
                FactTone::Warn
            },
        },
        OutputFact {
            id: "device",
            label: "Output device",
            value: device,
            detail: "The operating system device identity the engine opened, including the ALSA `hw:` or `plughw:` node where applicable.",
            tone: FactTone::Neutral,
        },
        OutputFact {
            id: "format",
            label: "Negotiated rate / container",
            value: rate_note,
            detail: "The sample rate and sample container agreed with the driver. Dither is shown only when it modifies an integer stream.",
            tone: FactTone::Neutral,
        },
        OutputFact {
            id: "buffer",
            label: "Period / buffer",
            value: buffer_value,
            detail: "ALSA's settled period and buffer sizes. Other backends report their callback buffer; estimated means the driver did not expose one.",
            tone: if info.buffer_size_frames == 0 {
                FactTone::Warn
            } else {
                FactTone::Neutral
            },
        },
    ]
}

pub fn access_mode_label(mode: OutputAccessMode) -> &'static str {
    match mode {
        OutputAccessMode::Shared => "Shared mixer",
        OutputAccessMode::Exclusive => "Exclusive",
        OutputAccessMode::DirectHw => "Direct hardware",
        OutputAccessMode::BitstreamPassthrough => "Bitstream passthrough",
    }
}

/// Fact rows as a two-column diagnostics table. Used by the Bit-Perfect modal;
/// Settings renders the same [`OutputFact`] list through its own row widget.
pub fn render_fact_table(ui: &mut egui::Ui, facts: &[OutputFact], theme: &Theme) {
    egui::Frame::NONE
        .fill(theme.bg_card)
        .stroke(egui::Stroke::new(1.0_f32, theme.border_subtle))
        .corner_radius(egui::CornerRadius::same(theme.radius_md))
        .inner_margin(egui::Margin::symmetric(
            theme.space_2xl as i8,
            theme.space_lg as i8,
        ))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::new(0.0, theme.space_sm);
            for fact in facts {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(fact.label)
                            .font(crate::theme::font(12.0))
                            .color(theme.text_secondary),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let color = match fact.tone {
                            FactTone::Neutral => theme.text_primary,
                            FactTone::Good => theme.bit_perfect_green,
                            FactTone::Warn => theme.warning_yellow,
                        };
                        ui.label(
                            egui::RichText::new(&fact.value)
                                .font(crate::theme::mono_font(11.5))
                                .color(color),
                        )
                        .on_hover_text(fact.detail);
                    });
                });
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::dsp::pipeline::OutputSampleFormat;

    fn verified_info() -> OutputInfo {
        let mut info = OutputInfo::exclusive(
            "hw:1,0".into(),
            96_000,
            2,
            Some(config::AudioBackend::ExclusiveAlsa),
        );
        info.sample_format = OutputSampleFormat::I24Le;
        info.buffer_size_frames = 4096;
        info.buffer_size_estimated = false;
        info.period_frames = 512;
        info
    }

    #[test]
    fn verified_direct_output_reports_every_required_fact() {
        let facts = output_facts(Some(&verified_info()), 21.3);
        let ids: Vec<_> = facts.iter().map(|f| f.id).collect();
        for required in [
            "requested_backend",
            "negotiated_backend",
            "access_mode",
            "verification",
            "device",
            "format",
            "buffer",
        ] {
            assert!(ids.contains(&required), "missing {required}");
        }
        let verification = facts.iter().find(|f| f.id == "verification").unwrap();
        assert_eq!(verification.tone, FactTone::Good);
        let buffer = facts.iter().find(|f| f.id == "buffer").unwrap();
        assert!(buffer.value.contains("512-frame period"));
        assert!(buffer.value.contains("4096-frame buffer"));
        let format = facts.iter().find(|f| f.id == "format").unwrap();
        assert!(format.value.contains("i24le"));
        assert!(format.value.contains("96 kHz"));
    }

    #[test]
    fn fallback_reason_is_visible_in_verification() {
        let info = OutputInfo::shared(
            "default".into(),
            96_000,
            48_000,
            2,
            Some(config::AudioBackend::ExclusiveAlsa),
            Some(config::AudioBackend::PipeWire),
            "hw:1,0 was busy",
        );
        let facts = output_facts(Some(&info), 9.0);
        let verification = facts.iter().find(|f| f.id == "verification").unwrap();
        assert!(verification.value.contains("hw:1,0 was busy"));
        assert_eq!(verification.tone, FactTone::Warn);
        let format = facts.iter().find(|f| f.id == "format").unwrap();
        assert!(format.value.contains("requested 96000 Hz"));
    }

    #[test]
    fn no_info_is_honest_rather_than_blank() {
        let facts = output_facts(None, 0.0);
        assert!(facts.iter().any(|f| f.value.contains("Waiting")));
        assert!(facts
            .iter()
            .any(|f| f.id == "verification" && f.tone == FactTone::Warn));
    }
}
