pub mod contrast;
pub mod editor;
pub mod fonts;
pub mod gallery;
pub mod glass;
pub mod manager;
pub mod preview;
pub mod schema;
pub mod system;
pub mod tokens;

pub use contrast::*;
pub use editor::*;
pub use fonts::*;
pub use gallery::*;
pub use glass::*;
pub use manager::*;
pub use preview::*;
pub use schema::*;
pub use system::*;
pub use tokens::*;

#[cfg(test)]
mod tests {
    use super::contrast::contrast_ratio;
    use super::tokens::Theme;
    use eframe::egui::Color32;

    #[test]
    fn text_tokens_clear_wcag_aa_in_all_system_themes() {
        for theme in [Theme::dark(), Theme::light(), Theme::oled()] {
            let text_tokens: [(&str, Color32); 6] = [
                ("text_primary", theme.text_primary),
                ("text_secondary", theme.text_secondary),
                ("text_muted", theme.text_muted),
                ("text_dim", theme.text_dim),
                ("accent_highlight", theme.accent_highlight),
                ("accent_warm", theme.accent_warm),
            ];
            let surfaces: [(&str, Color32); 3] = [
                ("bg_app", theme.bg_app),
                ("bg_card", theme.bg_card),
                ("bg_sidebar", theme.bg_sidebar),
            ];
            for (fg_name, fg) in text_tokens {
                for (bg_name, bg) in surfaces {
                    let ratio = contrast_ratio(fg, bg);
                    assert!(
                        ratio >= 4.5,
                        "{:?} theme: {fg_name} on {bg_name} is {ratio:.2}:1, below the 4.5:1 AA bar",
                        if theme.is_light { "light" } else { "dark/oled" }
                    );
                }
            }
        }
    }

    #[test]
    fn input_borders_clear_the_non_text_contrast_bar() {
        for theme in [Theme::dark(), Theme::light(), Theme::oled()] {
            let ratio = contrast_ratio(theme.border_input, theme.bg_input);
            assert!(
                ratio >= 3.0,
                "{:?} theme: border_input on bg_input is {ratio:.2}:1, below the 3:1 bar",
                if theme.is_light { "light" } else { "dark/oled" }
            );
        }
    }

    #[test]
    fn reduced_motion_snaps_timed_animations() {
        let mut theme = Theme::dark();
        assert_eq!(theme.motion_time(0.12), 0.12);
        theme.reduce_motion = true;
        assert_eq!(theme.motion_time(0.12), 0.0);

        // Toggling the palette is a theme switch, not a preference reset.
        theme.frosted = true;
        theme.toggle();
        assert!(theme.is_light);
        assert!(theme.reduce_motion);
        assert!(theme.frosted);
    }

    #[test]
    fn frosted_content_uses_a_legible_scrim() {
        let mut theme = Theme::dark();
        theme.frosted = true;
        let dark_alpha = theme.content_panel_fill().a();
        assert!(
            (120..255).contains(&dark_alpha),
            "the frosted scrim should keep the backdrop visible but legible, got alpha {dark_alpha}"
        );

        let mut light = Theme::light();
        light.frosted = true;
        let light_alpha = light.content_panel_fill().a();
        assert!(
            light_alpha > dark_alpha,
            "light content needs a stronger white scrim ({light_alpha}) than dark ({dark_alpha})"
        );
    }
}
