//! One truncation implementation for every painted label.
//!
//! Several views used to paint text with [`egui::Painter::layout_no_wrap`],
//! which clips at the edge of its rect with no indication that there is more
//! text. The song table had its own private copy of the ellipsis logic, so the
//! two disagreed: a long album name was cut mid-word in the grids while the
//! same name was elided with `…` in the table.
//!
//! [`truncated_galley`] is now the only code that lays out one line of text,
//! and [`ellipsize`] exposes the resulting string for callers that need the
//! text rather than the galley.

use eframe::egui::{self, text::LayoutJob, Color32, FontId, Painter};
use std::sync::Arc;

/// Lay out `text` on one line, eliding it with `…` when it does not fit
/// `max_width`.
///
/// The returned galley is already truncated; painting it can never overflow
/// the width it was given. A non-positive width still produces a galley (an
/// ellipsis), so callers do not have to special-case a collapsed column.
pub fn truncated_galley(
    painter: &Painter,
    text: impl Into<String>,
    font: FontId,
    color: Color32,
    max_width: f32,
) -> Arc<egui::Galley> {
    let mut job = LayoutJob::simple_singleline(text.into(), font, color);
    // `truncate_at_width` keeps the text on one row and elides it, where
    // setting `max_width` alone would wrap it onto a second row.
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_width.max(0.0));
    painter.layout_job(job)
}

/// The elided text itself.
///
/// Useful where the string is fed to a widget rather than painted directly.
/// For painting, prefer [`truncated_galley`]: it lays the text out once.
pub fn ellipsize(
    painter: &Painter,
    text: impl Into<String>,
    font: FontId,
    max_width: f32,
) -> String {
    let galley = truncated_galley(painter, text, font, Color32::WHITE, max_width);
    galley
        .rows
        .first()
        .map(|row| row.text())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{self, Pos2, Vec2};

    /// Lay text out inside a real egui frame so the painter has fonts.
    fn with_painter<R>(f: impl FnOnce(&Painter) -> R) -> R {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut result = None;
        let mut f = Some(f);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(900.0, 600.0),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            if let Some(f) = f.take() {
                result = Some(f(ui.painter()));
            }
        });
        // We are not driving a renderer, so there is nothing to upload the
        // font atlas to; acknowledging the delta keeps epaint's drop check
        // from firing.
        output.textures_delta.clear();
        result.expect("the frame ran")
    }

    #[test]
    fn text_that_fits_is_returned_unchanged() {
        with_painter(|painter| {
            let font = crate::theme::font(13.0);
            let text = "Kind of Blue";
            let width = painter
                .layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE)
                .size()
                .x;
            assert_eq!(ellipsize(painter, text, font, width), text);
        });
    }

    #[test]
    fn a_narrower_width_is_elided() {
        with_painter(|painter| {
            let font = crate::theme::font(13.0);
            let text = "Ambient Transmission";
            let width = painter
                .layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE)
                .size()
                .x;
            // Glyph advances are pixel-snapped, so a single point is not
            // always enough to cross the boundary; 10 points always is.
            let result = ellipsize(painter, text, font, width - 10.0);
            assert_ne!(result, text);
            assert!(result.ends_with('…'), "got {result:?}");
            assert!(result.chars().count() < text.chars().count());
        });
    }

    #[test]
    fn the_empty_string_stays_empty() {
        with_painter(|painter| {
            let result = ellipsize(painter, "", crate::theme::font(13.0), 40.0);
            assert_eq!(result, "");
        });
    }

    #[test]
    fn wide_unicode_is_elided_without_splitting_a_grapheme() {
        with_painter(|painter| {
            let font = crate::theme::font(13.0);
            let text = "東京事変の音楽";
            let result = ellipsize(painter, text, font, 24.0);
            assert!(result.ends_with('…'), "got {result:?}");
            // Every character that survived is still a valid character of the
            // input; truncation never emits a replacement glyph.
            assert!(!result.contains('\u{FFFD}'), "got {result:?}");
        });
    }

    #[test]
    fn an_emoji_is_not_cut_in_half() {
        with_painter(|painter| {
            let font = crate::theme::font(13.0);
            let text = "Night 🎵 Drive";
            let result = ellipsize(painter, text, font, 30.0);
            assert!(result.ends_with('…'), "got {result:?}");
            assert!(!result.contains('\u{FFFD}'), "got {result:?}");
        });
    }
}
