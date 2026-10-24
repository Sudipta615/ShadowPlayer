//! Search-result and command-palette jumps: carry a one-frame request to
//! scroll to a row and flash it.

use crate::theme::Theme;
use eframe::egui::{self, CornerRadius, Rect, Stroke};

/// A pending "jump to this row" request.
struct SettingsJump {
    /// The row to land on, named in the interface's current language.
    target: String,
    /// Whether the row still has to be scrolled into view.
    scroll: bool,
    /// When the highlight started, in egui time. Set when the row is first
    /// drawn rather than when the jump is requested, so switching category
    /// cannot expire the flash before the row exists.
    flash_started: Option<f64>,
}

thread_local! {
    /// The row a search result or palette entry asked for.
    ///
    /// `setting_item` has seventy call sites; threading a "jump target"
    /// parameter through all of them would be a mechanical change that
    /// obscures what each row means. The UI runs on one thread, so a
    /// thread-local is the honest way to carry a one-frame UI instruction.
    static SETTINGS_JUMP: std::cell::RefCell<Option<SettingsJump>> =
        const { std::cell::RefCell::new(None) };
}

/// Ask the next Settings frame to scroll to `label` and flash it.
///
/// `label` is the English text from the settings index. The row it names is
/// rendered translated, so the target is translated too: the jump must land
/// on the row the user is looking at, in whatever language the interface is
/// currently speaking.
pub fn request_settings_jump(label: &'static str) {
    let target = crate::i18n::tr_literal(label);
    SETTINGS_JUMP.with(|jump| {
        *jump.borrow_mut() = Some(SettingsJump {
            target,
            scroll: true,
            flash_started: None,
        });
    });
}

/// Whether a jump is waiting to be drawn and scrolled to.
///
/// A search result or palette entry is a *jump*, not a scroll: it should land
/// on the row at once instead of animating a thousand pixels past the user's
/// eye. The ScrollArea reads this to turn its smoothing off for the frames
/// where a jump still needs to scroll.
pub fn settings_jump_pending() -> bool {
    SETTINGS_JUMP.with(|jump| jump.borrow().as_ref().is_some_and(|state| state.scroll))
}

/// The pending jump, if it is for this row: `(scroll, flash_start, now)`.
fn setting_jump_for(label: &str, ui: &egui::Ui) -> Option<(bool, Option<f64>, f64)> {
    let now = ui.input(|i| i.time);
    SETTINGS_JUMP.with(|jump| {
        let mut slot = jump.borrow_mut();
        let state = slot.as_mut()?;
        if state.target != label {
            return None;
        }
        Some((state.scroll, state.flash_started, now))
    })
}

/// Finish a jump after its row has been drawn: scroll once, then flash.
pub(super) fn apply_setting_jump(
    label: &str,
    response: &egui::Response,
    highlight: Rect,
    ui: &egui::Ui,
    theme: &Theme,
) {
    let Some((scroll, flash_started, now)) = setting_jump_for(label, ui) else {
        return;
    };
    let clip = ui.clip_rect();
    if scroll {
        response.scroll_to_me(Some(egui::Align::Center));
        // The first request can be computed against a layout that is still
        // settling: switching category changes every card's height, and a
        // card whose content arrives a frame later moves the row again. Keep
        // asking until the row is actually in the viewport. A row larger than
        // the viewport counts as landed: it can never be fully contained.
        let landed = clip.contains_rect(highlight)
            || highlight.width() > clip.width()
            || highlight.height() > clip.height();
        if landed {
            SETTINGS_JUMP.with(|jump| {
                if let Some(state) = jump.borrow_mut().as_mut() {
                    state.scroll = false;
                }
            });
        }
    }
    if !clip.intersects(highlight) {
        // Off-screen: wait for the scroll to land before starting the flash,
        // so its 1.5 seconds are spent where the user can see it.
        return;
    }
    let started = flash_started.unwrap_or(now);
    if flash_started.is_none() {
        SETTINGS_JUMP.with(|jump| {
            if let Some(state) = jump.borrow_mut().as_mut() {
                state.flash_started = Some(now);
            }
        });
    }
    if now - started < 1.5 {
        ui.painter().rect_stroke(
            highlight.expand(3.0),
            CornerRadius::same(theme.radius_sm),
            Stroke::new(1.5, theme.accent),
            egui::StrokeKind::Outside,
        );
        ui.ctx().request_repaint();
    } else {
        SETTINGS_JUMP.with(|jump| *jump.borrow_mut() = None);
    }
}
