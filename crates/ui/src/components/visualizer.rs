//! The Now Playing card's waveform visualiser.
//!
//! The shape follows PlayTune's `WaveformVisualizer`: a strip of mirrored,
//! capsule-capped bars whose heights trace the track's energy envelope and
//! whose colour follows the playhead — bars the listener has already heard
//! carry the theme's accent, the rest stay quiet. Clicking or dragging the
//! strip seeks, the same gesture the seek bar below it accepts.
//!
//! Heights come from the cached per-track peaks the seek bar already uses, so
//! painting is one bucket scan per bar and never decodes audio on the UI
//! thread. Until the first decode lands the strip falls back to the
//! deterministic envelope below — the same resting shape PlayTune shows before
//! its first analysis window arrives.
//!
//! Colour is deliberately single-tone. PlayTune paints a two-colour gradient
//! because it ships one theme; ShadowPlayer has dozens, so the strip takes the
//! theme's own `accent_highlight` for the played span and a muted
//! `text_dim` for the rest. Nothing here is hard-coded, so every theme —
//! built-in, imported `.stm`, or user-authored — renders correctly.

use crate::theme::Theme;
use eframe::egui::{self, CornerRadius, Pos2, Rect, Response, Sense, Ui, Vec2};

/// Gap between bars. Kept narrow so a compact card still reads as a waveform.
const BAR_GAP: f32 = 2.0;
const MIN_BAR_WIDTH: f32 = 2.0;
const MAX_BAR_WIDTH: f32 = 5.0;
/// Bar-count bounds: below this the strip loses its waveform character, above
/// it the bars turn into noise at card widths.
const MIN_BARS: usize = 24;
const MAX_BARS: usize = 96;
/// Shortest bar, so a silent passage is a visible dash rather than a hole.
const MIN_BAR_HEIGHT: f32 = 3.0;
/// How much the live shimmer may scale a bar, on each side of 1.0.
const SHIMMER_DEPTH: f32 = 0.14;

pub struct VisualizerWidget<'a> {
    is_playing: bool,
    /// Cached normalised 0.0–1.0 peaks for the current track. `None` (or
    /// empty) until the first decode finishes.
    peaks: Option<&'a [f32]>,
    /// Playback position as a 0.0–1.0 fraction of the track.
    progress: f32,
    theme: &'a Theme,
}

impl<'a> VisualizerWidget<'a> {
    pub fn new(is_playing: bool, theme: &'a Theme) -> Self {
        Self {
            is_playing,
            peaks: None,
            progress: 0.0,
            theme,
        }
    }

    /// Attach the track's cached waveform peaks.
    pub fn peaks(mut self, peaks: Option<&'a [f32]>) -> Self {
        self.peaks = peaks;
        self
    }

    /// Attach the playback position as a 0.0–1.0 fraction.
    pub fn progress(mut self, progress: f32) -> Self {
        self.progress = progress;
        self
    }

    /// Show the strip. The returned fraction is `Some` only on the frame the
    /// listener clicked or dragged it, mirroring
    /// [`crate::components::waveform_seek::WaveformSeek::show`].
    pub fn show(self, ui: &mut Ui, size: Vec2) -> (Response, Option<f32>) {
        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());

        // PlayTune insets the strip by 2 px so the tallest bar never touches
        // the card edge.
        let inner = rect.shrink2(Vec2::new(2.0, 2.0));
        if !ui.is_rect_visible(rect) || inner.width() <= 0.0 || inner.height() <= 0.0 {
            return (response, None);
        }

        let progress = self.progress.clamp(0.0, 1.0);
        let painter = ui.painter_at(rect);

        let bars = ((inner.width() / (MIN_BAR_WIDTH + BAR_GAP)) as usize).clamp(MIN_BARS, MAX_BARS);
        let step = inner.width() / bars as f32;
        let bar_width = (step - BAR_GAP).clamp(MIN_BAR_WIDTH, MAX_BAR_WIDTH);
        let corner = CornerRadius::same((bar_width * 0.5).round().max(1.0) as u8);

        let played_x = inner.left() + inner.width() * progress;
        let played_colour = self.theme.accent_highlight;
        // The unplayed span is the same token the seek bar mutes, so the two
        // strips read as one family rather than two competing palettes.
        let rest_colour = self.theme.text_dim.gamma_multiply(0.5);

        // Motion runs only while audio is playing, and never when the listener
        // asked for reduced motion. It modulates the envelope by a few percent
        // so the strip feels live between track changes; a frozen strip is
        // still the honest rendering, which is why this is the only frame-rate
        // cost and it is capped at 60 fps.
        let animate = self.is_playing && !self.theme.reduce_motion;
        let time = if animate {
            ui.ctx().input(|i| i.time)
        } else {
            0.0
        };
        let mid_y = inner.center().y;

        for bar in 0..bars {
            let amplitude = self.amplitude(bar, bars);
            let amplitude = if animate {
                let phase = time * 3.1 + bar as f64 * 0.42;
                amplitude * (1.0 - SHIMMER_DEPTH + SHIMMER_DEPTH * phase.sin().abs() as f32)
            } else {
                amplitude
            };

            let half = (amplitude * inner.height() * 0.5).max(MIN_BAR_HEIGHT * 0.5);
            let x = inner.left() + bar as f32 * step;
            let bar_rect = Rect::from_min_max(
                Pos2::new(x, mid_y - half),
                Pos2::new(x + bar_width, mid_y + half),
            );
            let colour = if bar_rect.center().x <= played_x {
                played_colour
            } else {
                rest_colour
            };
            painter.rect_filled(bar_rect, corner, colour);
        }

        let seek = if response.dragged() || response.clicked() {
            response
                .interact_pointer_pos()
                .map(|pointer| ((pointer.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0))
        } else {
            None
        };
        response.widget_info(|| {
            egui::WidgetInfo::slider(true, f64::from(progress), "Playback position")
        });

        if animate {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }

        (
            response.on_hover_cursor(egui::CursorIcon::PointingHand),
            seek,
        )
    }

    /// Amplitude for one bar, as a 0.0–1.0 fraction of the strip's half-height.
    fn amplitude(&self, bar: usize, bars: usize) -> f32 {
        match self.peaks.filter(|peaks| !peaks.is_empty()) {
            Some(peaks) => {
                // Bucket max, like the seek bar: downsampling by peak keeps
                // transients visible instead of averaging them away.
                let from = bar * peaks.len() / bars;
                let to = ((bar + 1) * peaks.len() / bars)
                    .max(from + 1)
                    .min(peaks.len());
                peaks[from..to]
                    .iter()
                    .copied()
                    .fold(0.0f32, f32::max)
                    .clamp(0.0, 1.0)
            }
            None => resting_envelope(bar, bars),
        }
    }
}

/// The resting shape shown before the track's own peaks are decoded: a
/// natural-looking wave that swells through the vocal mids and settles at both
/// ends. It is deterministic, so it costs nothing and never flickers.
fn resting_envelope(bar: usize, bars: usize) -> f32 {
    use std::f32::consts::PI;

    let x = bar as f32 / bars.max(1) as f32;
    let value = 0.18
        + 0.45 * (x * PI).sin()
        + 0.22 * (x * 4.0 * PI).sin() * (x * 2.5 * PI).cos()
        + 0.15 * (x * 12.0 * PI + 1.2).sin();
    value.abs().clamp(0.12, 0.95)
}

#[cfg(test)]
mod tests {
    use super::resting_envelope;

    #[test]
    fn resting_envelope_stays_in_range_and_varies() {
        let bars = 64;
        let mut lowest = f32::MAX;
        let mut highest = f32::MIN;
        for bar in 0..bars {
            let value = resting_envelope(bar, bars);
            assert!((0.12..=0.95).contains(&value), "bar {bar} produced {value}");
            lowest = lowest.min(value);
            highest = highest.max(value);
        }
        // A flat line would mean the fallback stopped being a waveform.
        assert!(
            highest - lowest > 0.4,
            "envelope is too flat: {lowest}..{highest}"
        );
    }
}
