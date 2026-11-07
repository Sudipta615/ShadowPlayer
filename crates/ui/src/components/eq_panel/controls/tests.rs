//! Unit tests for the shared controls.

use super::preamp_display_limit;

#[test]
fn preamp_slider_widens_to_show_a_preset_that_needs_more_headroom() {
    assert_eq!(preamp_display_limit(0.0, false), 12.0);
    assert_eq!(preamp_display_limit(-12.0, false), 12.0);
    assert_eq!(preamp_display_limit(-12.5, false), 24.0);
    assert_eq!(preamp_display_limit(-24.0, false), 24.0);
    assert_eq!(preamp_display_limit(-25.0, false), 30.0);
    assert_eq!(preamp_display_limit(0.0, true), 24.0);
    assert_eq!(preamp_display_limit(26.0, true), 30.0);
}
