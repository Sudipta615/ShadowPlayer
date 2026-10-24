pub mod contrast;
pub mod editor;
pub mod fonts;
pub mod gallery;
pub mod manager;
pub mod preview;
pub mod schema;
pub mod system;
pub mod tokens;

pub use contrast::*;
pub use editor::*;
pub use fonts::*;
pub use gallery::*;
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
}
