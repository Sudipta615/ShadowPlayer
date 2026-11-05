//! The listening-history report body and its drawing helpers.
use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

use super::{format_span, short_date, ExportFormat, ListeningPeriod, ListeningReport};
use crate::theme::Theme;
use domain::models::ScrobbleCount;

pub fn render_report_body(
    ui: &mut egui::Ui,
    report: &ListeningReport,
    period: &mut ListeningPeriod,
    theme: &Theme,
) -> Option<ExportFormat> {
    // The body's inner width. Charts and header rows lay themselves out
    // against this, not against whatever a nested layout happens to report:
    // that mismatch was why the date and the right edge of both charts were
    // clipped by the modal frame.
    let content_w = ui.available_width();

    ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_lg);

    // ── The headline numbers ───────────────────────────────────────
    let tiles: [(&str, String); 4] = [
        ("Scrobbles", report.total.to_string()),
        ("Listened", format_span(report.listened_secs as u64)),
        ("Days listened", report.days_listened.to_string()),
        (
            "Streak",
            format!(
                "{} day{}",
                report.current_streak,
                if report.current_streak == 1 { "" } else { "s" }
            ),
        ),
    ];
    // Four tiles on one row at the modal's design width. The fixed 168 px
    // tiles plus spacing total 690 px, which is wider than the 640 px window
    // the app supports; shrinking the tile to the row's share keeps the row
    // from widening (and then clipping) the whole body.
    let tile_w = ((content_w - theme.space_sm * 3.0) / 4.0).clamp(120.0, 168.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, theme.space_sm);
        for (label, value) in tiles {
            stat_tile(ui, label, &value, theme, tile_w);
        }
    });

    if report.total == 0 && report.first_played_at.is_none() {
        ui.add_space(theme.space_md);
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new(tr!("Nothing has been scrobbled yet"))
                    .font(crate::theme::font(13.0))
                    .color(theme.text_muted),
            );
            ui.label(
                egui::RichText::new(
                    tr!("A play is scrobbled once it has been heard for thirty \n                     seconds, or half the track, whichever comes first."),
                )
                .font(crate::theme::font(11.5))
                .color(theme.text_dim),
            );
        });
        return None;
    }

    // ── The period and the first-scrobble date ─────────────────────
    //
    // The buttons and the date are siblings with explicit widths. Nested
    // right-to-left layouts inside a horizontal row get the row's remaining
    // width, which is not the frame's inner width; the date then overflowed
    // the frame and was clipped. Reserving its measured width up front keeps
    // both siblings inside `content_w`.
    let date_text = report
        .first_played_at
        .map(|first| format!("First scrobble {}", short_date(first)));
    let date_w = date_text
        .as_ref()
        .map(|text| {
            ui.painter()
                .layout_no_wrap(text.clone(), crate::theme::font(10.5), theme.text_dim)
                .size()
                .x
                + theme.space_sm
        })
        .unwrap_or(0.0);
    let buttons_w = (content_w - date_w - theme.space_sm).max(140.0);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_xs, 0.0);
        ui.allocate_ui_with_layout(
            Vec2::new(buttons_w, 24.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_xs, 0.0);
                for choice in ListeningPeriod::ALL {
                    let selected = choice == *period;
                    let button = egui::Button::new(
                        egui::RichText::new(choice.label())
                            .font(crate::theme::font(11.5))
                            .color(if selected {
                                theme.text_primary
                            } else {
                                theme.text_muted
                            }),
                    )
                    .fill(if selected {
                        theme.bg_card_hover
                    } else {
                        egui::Color32::TRANSPARENT
                    })
                    .stroke(if selected {
                        Stroke::new(1.0, theme.accent)
                    } else {
                        Stroke::NONE
                    })
                    .corner_radius(CornerRadius::same(theme.radius_sm));
                    if ui.add(button).clicked() {
                        *period = choice;
                    }
                }
            },
        );
        if let Some(date) = date_text {
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width().max(date_w), 24.0),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    ui.label(
                        egui::RichText::new(date)
                            .font(crate::theme::font(10.5))
                            .color(theme.text_dim),
                    );
                },
            );
        }
    });

    // ── The three top lists ────────────────────────────────────────
    ui.columns(3, |cols| {
        top_list(&mut cols[0], "Top artists", &report.top_artists, theme);
        top_list(&mut cols[1], "Top albums", &report.top_albums, theme);
        top_list(&mut cols[2], "Top tracks", &report.top_tracks, theme);
    });

    // ── On repeat ──────────────────────────────────────────────────
    if !report.on_repeat.is_empty() {
        section_heading(ui, "On repeat this week", theme);
        let card = egui::Frame::NONE
            .fill(theme.bg_card_hover)
            .corner_radius(CornerRadius::same(theme.radius_md))
            .inner_margin(egui::Margin::symmetric(
                theme.space_xl as i8,
                theme.space_md as i8,
            ));
        card.show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xs);
            for entry in &report.on_repeat {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                    // Reserve the count's width before laying out the
                    // name: a truncating label would otherwise eat the
                    // whole row and the count would sit on top of it.
                    let count_w = 96.0;
                    let text_w = (ui.available_width() - count_w - 8.0).max(80.0);
                    let text = if entry.detail.is_empty() {
                        entry.name.clone()
                    } else {
                        format!("{}  ·  {}", entry.name, entry.detail)
                    };
                    ui.allocate_ui_with_layout(
                        Vec2::new(text_w, 16.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(text)
                                        .font(crate::theme::font(12.0))
                                        .color(theme.text_primary),
                                )
                                .truncate(),
                            );
                        },
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!("{}× this week", entry.count))
                                .font(crate::theme::mono_font(10.5))
                                .color(theme.accent_warm),
                        );
                    });
                });
            }
        });
    }

    // ── Scrobbles per day ─────────────────────────────────────────
    section_heading(ui, "Scrobbles per day", theme);
    let day_values: Vec<usize> = report.per_day.iter().map(|(_, n)| *n).collect();
    bar_chart(ui, &day_values, theme, 54.0, theme.accent, content_w);
    let busiest = day_values.iter().copied().max().unwrap_or(0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(tr!("Last 30 days"))
                .font(crate::theme::font(10.5))
                .color(theme.text_dim),
        );
        let label_w = (ui.available_width() - 4.0).max(60.0);
        ui.allocate_ui_with_layout(
            Vec2::new(label_w, 16.0),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "peak {busiest} · today {}",
                        day_values.last().copied().unwrap_or(0)
                    ))
                    .font(crate::theme::mono_font(10.0))
                    .color(theme.text_muted),
                );
            },
        );
    });

    // ── By hour of day ────────────────────────────────────────────
    section_heading(ui, "By time of day", theme);
    bar_chart(
        ui,
        &report.per_hour,
        theme,
        44.0,
        theme.accent_highlight,
        content_w,
    );
    if let Some((hour, count)) = report.peak_hour() {
        ui.label(
            egui::RichText::new(format!(
                "You listen most around {:02}:00 — {count} scrobbles",
                hour
            ))
            .font(crate::theme::font(10.5))
            .color(theme.text_dim),
        );
    }

    // ── Streaks ────────────────────────────────────────────────────
    section_heading(ui, "Streaks", theme);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);
        for (label, value) in [
            ("Current", report.current_streak),
            ("Longest", report.longest_streak),
            ("Days with a scrobble", report.days_listened),
        ] {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(format!("{value}"))
                        .font(crate::theme::font(15.0))
                        .strong()
                        .color(theme.text_primary),
                );
                ui.label(
                    egui::RichText::new(label)
                        .font(crate::theme::font(10.5))
                        .color(theme.text_muted),
                );
            });
        }
    });

    // ── Export ─────────────────────────────────────────────────────
    section_heading(ui, "Export", theme);
    let mut export = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, theme.space_xs);
        for format in [ExportFormat::Csv, ExportFormat::Json, ExportFormat::LastFm] {
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(format.label())
                            .font(crate::theme::font(12.0))
                            .color(theme.text_primary),
                    )
                    .fill(theme.bg_pill)
                    .corner_radius(CornerRadius::same(theme.radius_md)),
                )
                .on_hover_text(format.description())
                .clicked()
            {
                export = Some(format);
            }
        }
    });
    ui.label(
        egui::RichText::new(
            tr!("Export writes the whole history, not only the selected period. \n             The file is yours; nothing leaves the machine."),
        )
        .font(crate::theme::font(10.5))
        .color(theme.text_dim),
    );

    export
}

// ─────────────────────────────────────────────────────────────────────────────
// Pure helpers
// ─────────────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────────────
// Drawing helpers
// ─────────────────────────────────────────────────────────────────────────────

fn stat_tile(ui: &mut egui::Ui, label: &str, value: &str, theme: &Theme, width: f32) {
    egui::Frame::NONE
        .fill(theme.bg_card_hover)
        .corner_radius(CornerRadius::same(theme.radius_md))
        .inner_margin(egui::Margin::symmetric(
            theme.space_xl as i8,
            theme.space_md as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(width - 24.0);
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(value)
                        .font(crate::theme::font(16.0))
                        .strong()
                        .color(theme.text_primary),
                );
                ui.label(
                    egui::RichText::new(label)
                        .font(crate::theme::font(10.5))
                        .color(theme.text_muted),
                );
            });
        });
}

fn section_heading(ui: &mut egui::Ui, title: &str, theme: &Theme) {
    ui.add_space(theme.space_xs);
    ui.label(
        egui::RichText::new(title)
            .font(crate::theme::font(12.5))
            .strong()
            .color(theme.text_secondary),
    );
    ui.add_space(theme.space_xxs);
}

fn top_list(ui: &mut egui::Ui, title: &str, entries: &[ScrobbleCount], theme: &Theme) {
    section_heading(ui, title, theme);
    if entries.is_empty() {
        ui.label(
            egui::RichText::new(tr!("Nothing yet"))
                .font(crate::theme::font(11.0))
                .color(theme.text_dim),
        );
        return;
    }
    for entry in entries {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xxs);
            // Counts ride with the detail line rather than being right-aligned:
            // a column's reported width is wider than its actual clip rect, so
            // a right-to-left child would land in the next column. One
            // truncating label per line keeps three narrow columns honest.
            ui.add(
                egui::Label::new(
                    egui::RichText::new(entry.name.clone())
                        .font(crate::theme::font(12.0))
                        .color(theme.text_primary),
                )
                .truncate(),
            );
            let detail = if entry.detail.is_empty() {
                format!("{}×", entry.count)
            } else {
                format!("{}× · {}", entry.count, entry.detail)
            };
            ui.add(
                egui::Label::new(
                    egui::RichText::new(detail)
                        .font(crate::theme::font(10.5))
                        .color(theme.text_dim),
                )
                .truncate(),
            );
        });
    }
}

fn bar_chart(
    ui: &mut egui::Ui,
    values: &[usize],
    theme: &Theme,
    height: f32,
    color: egui::Color32,
    width: f32,
) {
    if values.is_empty() {
        return;
    }
    // The width is the body's inner width, measured before any nested layout
    // narrowed the available rect. Sizing against `ui.available_width()` here
    // made the right-hand end of every chart run under the modal's edge.
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let painter = ui.painter_at(rect);
    let max = values.iter().copied().max().unwrap_or(0).max(1) as f32;
    let gap = 2.0f32;
    let n = values.len() as f32;
    let bar_w = ((rect.width() - gap * (n - 1.0)) / n).max(1.0);
    for (i, value) in values.iter().enumerate() {
        let h = (*value as f32 / max) * (height - 2.0);
        let x = rect.min.x + i as f32 * (bar_w + gap);
        let bar = Rect::from_min_max(
            Pos2::new(x, rect.max.y - h),
            Pos2::new(x + bar_w, rect.max.y),
        );
        // An empty day still gets a hairline, so the chart reads as a
        // timeline rather than a few floating bars.
        if *value == 0 {
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(x, rect.max.y - 1.0),
                    Pos2::new(x + bar_w, rect.max.y),
                ),
                CornerRadius::same(theme.radius_xs),
                theme.border_subtle,
            );
        } else {
            painter.rect_filled(bar, CornerRadius::same(theme.radius_xs), color);
        }
    }
}
