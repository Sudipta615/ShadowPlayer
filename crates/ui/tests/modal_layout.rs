//! Layout guards for modal bodies.
//!
//! The Listening History header used to nest a right-to-left layout inside a
//! horizontal row, so the "First scrobble …" date and the right-hand edge of
//! both charts were clipped by the modal frame. The body is now rendered in
//! isolation at the three widths the app is expected to survive, and every
//! painted shape must stay inside the width it was given.

use domain::models::ScrobbleCount;
use eframe::egui::{self, Pos2, Rect, Vec2};
use ui::modals::listening_modal::{render_report_body, ListeningPeriod, ListeningReport};
use ui::theme::Theme;

/// The narrowest laptop window, the modal's design width, and a wide desktop.
const MODAL_WIDTHS: [f32; 3] = [640.0, 900.0, 1280.0];

/// A report with long names and every section populated, so the test exercises
/// truncation and the widest rows rather than an empty state.
fn worst_case_report() -> ListeningReport {
    let count = |name: &str, detail: &str, n: usize| ScrobbleCount {
        name: name.to_string(),
        detail: detail.to_string(),
        count: n,
    };

    ListeningReport {
        total: 12_345,
        listened_secs: 987_654.0,
        first_played_at: Some(1_600_000_000),
        top_artists: (0..8)
            .map(|i| {
                count(
                    &format!("The Extremely Long Artist Name Number {i}"),
                    "",
                    900 - i,
                )
            })
            .collect(),
        top_albums: (0..8)
            .map(|i| {
                count(
                    &format!("An Album With A Very Long Title, Volume {i}"),
                    "The Extremely Long Artist Name",
                    800 - i,
                )
            })
            .collect(),
        top_tracks: (0..8)
            .map(|i| {
                count(
                    &format!("A Track Title That Would Run Past The Column Edge {i}"),
                    "An Artist With A Long Name",
                    700 - i,
                )
            })
            .collect(),
        on_repeat: (0..5)
            .map(|i| {
                count(
                    &format!("A Repeated Track With A Wide Title {i}"),
                    "A Repeated Artist",
                    5 + i,
                )
            })
            .collect(),
        per_day: (0..30)
            .map(|day| (1_600_000_000i64 + day * 86_400, ((day * 7) % 40) as usize))
            .collect(),
        per_hour: std::array::from_fn(|hour| (hour * 13) % 55),
        current_streak: 12,
        longest_streak: 40,
        days_listened: 300,
        today: 1_602_592_000,
    }
}

#[test]
fn the_listening_report_stays_inside_its_frame_at_every_width() {
    let report = worst_case_report();
    for theme in [Theme::dark(), Theme::light()] {
        for width in MODAL_WIDTHS {
            let body = render_at_width(width, &report, &theme);
            let overflow: Vec<String> = body
                .painted
                .iter()
                .filter(|rect| {
                    rect.min.x < body.inner.min.x - 1.0 || rect.max.x > body.inner.max.x + 1.0
                })
                .map(|rect| format!("{rect:?}"))
                .collect();
            assert!(
                overflow.is_empty(),
                "content painted outside the {width} px body (inner {:?}):\n{}",
                body.inner,
                overflow.join("\n")
            );
        }
    }
}

#[test]
fn the_first_scrobble_date_is_rendered_at_every_width() {
    let report = worst_case_report();
    let theme = Theme::dark();
    for width in MODAL_WIDTHS {
        let body = render_at_width(width, &report, &theme);
        let dates: Vec<&String> = body
            .texts
            .iter()
            .filter(|text| text.contains("First scrobble"))
            .collect();
        assert_eq!(
            dates.len(),
            1,
            "expected exactly one first-scrobble label at {width} px, got {dates:?}"
        );
        assert!(
            !dates[0].ends_with('…'),
            "the first-scrobble date was truncated at {width} px: {:?}",
            dates[0]
        );
    }
}

struct RenderedBody {
    inner: Rect,
    painted: Vec<Rect>,
    texts: Vec<String>,
}

/// Render the report body at `width` and collect the bounds of everything it
/// painted.
fn render_at_width(width: f32, report: &ListeningReport, theme: &Theme) -> RenderedBody {
    let ctx = egui::Context::default();
    ui::theme::install_fonts(&ctx);
    let mut inner = None;
    let mut period = ListeningPeriod::Month;

    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(
            Pos2::ZERO,
            Vec2::new(width.max(1280.0), 4000.0),
        )),
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        ui.allocate_ui_with_layout(
            Vec2::new(width, 3900.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(width);
                inner = Some(ui.max_rect());
                let _ = render_report_body(ui, report, &mut period, theme);
            },
        );
    });

    let inner = inner.expect("the body ran");
    let mut painted = Vec::new();
    let mut texts = Vec::new();
    for clipped in &output.shapes {
        let rect = clipped.shape.visual_bounding_rect();
        if rect.is_finite() && rect.width() >= 0.0 && rect.height() >= 0.0 {
            painted.push(rect);
        }
        if let egui::Shape::Text(text) = &clipped.shape {
            texts.push(text.galley.text().to_string());
        }
    }
    // Acknowledge the font atlas so epaint's drop check does not fire; there
    // is no renderer in this test.
    output.textures_delta.clear();

    RenderedBody {
        inner,
        painted,
        texts,
    }
}
