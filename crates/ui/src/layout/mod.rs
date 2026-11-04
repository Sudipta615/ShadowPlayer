pub mod right_drawer;
pub mod sidebar;

pub use right_drawer::render_right_drawer;
pub use sidebar::render_sidebar;

/// The smallest window the shell is designed to lay out.
///
/// This must stay below [`SIDEBAR_RAIL_BREAKPOINT`], otherwise the icon-only
/// rail is unreachable in a normal window and the compact layout only exists
/// in tests. The old floor was 1080 while the rail started at 720, so the rail
/// was dead code for every user.
pub const MIN_WINDOW_WIDTH: f32 = 720.0;

/// The smallest height that keeps the transport, table header and one full
/// song row visible at the same time.
pub const MIN_WINDOW_HEIGHT: f32 = 640.0;

/// Below this width the sidebar collapses to icon-only navigation.
pub const SIDEBAR_RAIL_BREAKPOINT: f32 = 820.0;

/// Between the rail and this width the sidebar uses its 200 px compact form;
/// above it the full 208 px form fits without clipping "Help & Feedback".
pub const SIDEBAR_COMPACT_BREAKPOINT: f32 = 1100.0;

/// The right queue/lyrics drawer only earns its width above this breakpoint.
pub const RIGHT_PANEL_BREAKPOINT: f32 = 1100.0;

/// Width below which the Settings page swaps its rail for a row of chips.
///
/// One value for the whole shell. The page previously kept its own 720 px
/// breakpoint, so two constants answered the same question.
pub const SETTINGS_COMPACT_BREAKPOINT: f32 = 720.0;

/// Interface scale bounds. Below 0.8 the 10.5 px Small text stops being
/// readable; above 1.5 the three-column shell stops fitting on a 1080p
/// display. The default is the size the theme was designed at.
pub const UI_SCALE_MIN: f32 = 0.8;
pub const UI_SCALE_MAX: f32 = 1.5;
pub const UI_SCALE_DEFAULT: f32 = 1.0;

/// Clamp a scale from disk or from egui's keyboard zoom to the supported
/// range. A corrupt or out-of-range value must never leave the UI unusable.
pub fn clamp_ui_scale(scale: f32) -> f32 {
    if scale.is_finite() {
        scale.clamp(UI_SCALE_MIN, UI_SCALE_MAX)
    } else {
        UI_SCALE_DEFAULT
    }
}

/// A breakpoint nobody reaches is a layout that cannot be seen. These pin
/// the constants in their required order; clippy flags constant assertions,
/// but the constants *are* the specification being pinned, so allow it here
/// and nowhere else.
#[cfg(test)]
#[allow(clippy::assertions_on_constants)]
mod breakpoints {
    use super::*;

    #[test]
    fn the_minimum_window_can_reach_the_rail() {
        assert!(
            MIN_WINDOW_WIDTH < SIDEBAR_RAIL_BREAKPOINT,
            "the rail starts at {SIDEBAR_RAIL_BREAKPOINT} but the window floor is {MIN_WINDOW_WIDTH}; \
             the compact layout would be unreachable"
        );
    }

    #[test]
    fn the_sidebar_breakpoints_are_ordered() {
        assert!(
            SIDEBAR_RAIL_BREAKPOINT < SIDEBAR_COMPACT_BREAKPOINT,
            "the rail breakpoint {SIDEBAR_RAIL_BREAKPOINT} is not below the \
             compact breakpoint {SIDEBAR_COMPACT_BREAKPOINT}"
        );
    }

    #[test]
    fn the_right_drawer_does_not_appear_before_the_compact_sidebar() {
        assert!(
            RIGHT_PANEL_BREAKPOINT >= SIDEBAR_COMPACT_BREAKPOINT,
            "the right drawer breakpoint {RIGHT_PANEL_BREAKPOINT} is below the \
             compact sidebar breakpoint {SIDEBAR_COMPACT_BREAKPOINT}"
        );
    }

    #[test]
    fn the_window_floor_still_fits_a_song_row() {
        assert!(
            MIN_WINDOW_HEIGHT >= 600.0,
            "the window floor height {MIN_WINDOW_HEIGHT} is below the 600 required \
             for a song row"
        );
    }

    #[test]
    fn the_settings_page_uses_the_shell_breakpoint() {
        assert_eq!(
            SETTINGS_COMPACT_BREAKPOINT,
            crate::components::settings_tab::SETTINGS_COMPACT_BREAKPOINT
        );
    }

    #[test]
    fn the_interface_scale_is_clamped() {
        assert_eq!(clamp_ui_scale(f32::NAN), UI_SCALE_DEFAULT);
        assert_eq!(clamp_ui_scale(0.5), UI_SCALE_MIN);
        assert_eq!(clamp_ui_scale(2.0), UI_SCALE_MAX);
        assert_eq!(clamp_ui_scale(1.25), 1.25);
    }
}
