use std::sync::{Arc, OnceLock};

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke,
    TextStyle, Vec2,
};

/// The one knob that controls the UI type scale.
///
/// Every explicit text size in the app goes through [`font`], so this is enough
/// to make the interface comfortably larger without touching hundreds of
/// individual call sites. The floor keeps the smallest chart and caption
/// labels legible.
pub const TEXT_SCALE: f32 = 1.08;
pub const TEXT_MIN: f32 = 10.0;

/// Font family used for headings and other display-sized text.
fn display_family() -> FontFamily {
    static DISPLAY: OnceLock<FontFamily> = OnceLock::new();
    DISPLAY
        .get_or_init(|| FontFamily::Name(Arc::from("inter-display")))
        .clone()
}

/// The UI's proportional font at `size`, scaled and floored by the global
/// typography tokens. Sizes at or above display scale use Inter Medium.
pub fn font(size: f32) -> FontId {
    let size = (size.max(TEXT_MIN) * TEXT_SCALE).max(11.0);
    let family = if size >= 20.0 {
        display_family()
    } else {
        FontFamily::Proportional
    };
    FontId::new(size, family)
}

/// Monospace text (track numbers, technical values, diagnostics).
pub fn mono_font(size: f32) -> FontId {
    FontId::new(
        (size.max(9.0) * TEXT_SCALE).max(10.5),
        FontFamily::Monospace,
    )
}

/// Install the bundled Inter family. Called once, before the first frame.
///
/// Inter is deliberately bundled rather than discovered from the system so the
/// app looks the same on every platform. The default egui fonts stay in the
/// fallback chain for glyphs Inter does not carry.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "inter-regular".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Inter-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        "inter-medium".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Inter-Medium.ttf"
        ))),
    );

    let fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();

    let mut proportional = vec!["inter-regular".to_owned()];
    proportional.extend(fallback.iter().cloned());
    fonts
        .families
        .insert(FontFamily::Proportional, proportional);

    let mut display = vec!["inter-medium".to_owned(), "inter-regular".to_owned()];
    display.extend(fallback);
    fonts.families.insert(display_family(), display);

    ctx.set_fonts(fonts);
}

/// ShadowPlayer's visual identity: obsidian surfaces, jade actions, mint
/// highlights, and a warm amber secondary. No purple, no multi-hue ramps.
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub is_light: bool,

    // Backgrounds
    pub bg_app: Color32,
    pub bg_sidebar: Color32,
    pub bg_card: Color32,
    pub bg_card_hover: Color32,
    pub bg_card_active: Color32,
    pub bg_pill: Color32,
    pub bg_pill_hover: Color32,
    pub bg_input: Color32,
    pub border_subtle: Color32,
    pub border_card: Color32,
    /// The border of a text input.
    ///
    /// WCAG 1.4.11 asks for 3:1 on anything a user has to be able to
    /// identify. A text field's fill is very close to the page's, so its
    /// border is the only thing that says "this is an input" — it has to
    /// clear that bar in both themes.
    pub border_input: Color32,
    pub slider_track: Color32,

    // Accents
    /// Primary jade fill: active navigation, primary buttons, progress.
    pub accent: Color32,
    /// Mint/teal highlight for icon strokes, selected labels and focus.
    pub accent_highlight: Color32,
    /// Amber secondary: favourites, ratings, warm status accents.
    pub accent_warm: Color32,
    /// Informational blue.
    pub info: Color32,

    // Text colors
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
    pub text_dim: Color32,

    // Status colors
    pub bit_perfect_green: Color32,
    pub warning_yellow: Color32,

    // Derived / semantic tokens so views don't hardcode colours.
    /// Fill for the currently playing row/card in track lists.
    pub bg_active_row: Color32,
    /// Border of the currently playing row/card.
    pub border_active_row: Color32,
    /// Background behind a missing artwork placeholder.
    pub artwork_placeholder: Color32,
    /// Destructive-action colour (remove from library, etc.).
    pub destructive: Color32,
    /// Translucent scrim drawn over artwork on hover.
    pub overlay_scrim: Color32,
    /// The background of the instrument panels — the VU meters, the LUFS
    /// meter and the spatial radar.
    ///
    /// They used to hardcode a dark navy, which stayed dark in light mode and
    /// looked like a hole in the page.
    pub meter_bg: Color32,
    /// The unfilled part of a meter bar.
    pub meter_gutter: Color32,
    /// The grid lines inside the instrument panels.
    pub meter_grid: Color32,
    /// Background of the "scanning…" information banner.
    pub scan_banner_bg: Color32,
    /// Sun / light-mode accent.
    pub sun_accent: Color32,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            is_light: false,
            bg_app: Color32::from_rgb(11, 15, 16), // #0B0F10
            bg_sidebar: Color32::from_rgb(14, 20, 22), // #0E1416
            bg_card: Color32::from_rgb(18, 24, 26), // #12181A
            bg_card_hover: Color32::from_rgb(25, 33, 36), // #192124
            bg_card_active: Color32::from_rgb(16, 48, 43), // #10302B
            bg_pill: Color32::from_rgb(21, 28, 30), // #151C1E
            bg_pill_hover: Color32::from_rgb(32, 43, 46), // #202B2E
            bg_input: Color32::from_rgb(16, 22, 25), // #101619
            border_subtle: Color32::from_rgb(32, 43, 46), // #202B2E
            border_card: Color32::from_rgb(42, 54, 57), // #2A3639
            border_input: Color32::from_rgb(80, 106, 112), // #506A70 (3.16:1 on bg_input)
            slider_track: Color32::from_rgb(43, 56, 59), // #2B383B

            accent: Color32::from_rgb(15, 118, 110), // #0F766E jade
            accent_highlight: Color32::from_rgb(94, 234, 212), // #5EEAD4 mint
            accent_warm: Color32::from_rgb(245, 158, 11), // #F59E0B amber
            info: Color32::from_rgb(56, 189, 248),   // #38BDF8 sky

            text_primary: Color32::from_rgb(242, 246, 245), // #F2F6F5
            text_secondary: Color32::from_rgb(185, 199, 196), // #B9C7C4
            text_muted: Color32::from_rgb(131, 145, 142),   // #83918E
            text_dim: Color32::from_rgb(122, 138, 134),     // #7A8A86 (4.96:1 on bg_card)

            bit_perfect_green: Color32::from_rgb(52, 211, 153), // #34D399
            warning_yellow: Color32::from_rgb(251, 191, 36),    // #FBBF24

            bg_active_row: Color32::from_rgb(16, 48, 43), // #10302B
            border_active_row: Color32::from_rgb(45, 212, 191), // #2DD4BF
            artwork_placeholder: Color32::from_rgb(26, 36, 38), // #1A2426
            destructive: Color32::from_rgb(251, 113, 133), // #FB7185
            overlay_scrim: Color32::from_rgba_unmultiplied(0, 0, 0, 160),
            meter_bg: Color32::from_rgb(12, 14, 22), // #0C0E16
            meter_gutter: Color32::from_rgb(18, 20, 32), // #121420
            meter_grid: Color32::from_rgba_unmultiplied(100, 116, 139, 40),
            scan_banner_bg: Color32::from_rgb(16, 43, 40), // #102B28
            sun_accent: Color32::from_rgb(245, 158, 11),
        }
    }

    pub fn light() -> Self {
        Self {
            is_light: true,
            bg_app: Color32::from_rgb(244, 247, 246), // #F4F7F6
            bg_sidebar: Color32::from_rgb(236, 242, 240), // #ECF2F0
            bg_card: Color32::from_rgb(255, 255, 255),
            bg_card_hover: Color32::from_rgb(239, 245, 243), // #EFF5F3
            bg_card_active: Color32::from_rgb(223, 243, 238), // #DFF3EE
            bg_pill: Color32::from_rgb(232, 239, 237),       // #E8EFED
            bg_pill_hover: Color32::from_rgb(220, 231, 227), // #DCE7E3
            bg_input: Color32::from_rgb(241, 245, 244),      // #F1F5F4
            border_subtle: Color32::from_rgb(198, 210, 206), // #C6D2CE
            border_card: Color32::from_rgb(180, 195, 190),   // #B4C3BE
            border_input: Color32::from_rgb(118, 134, 130),  // #768682 (3.47:1 on bg_input)
            slider_track: Color32::from_rgb(201, 214, 210),  // #C9D6D2

            accent: Color32::from_rgb(15, 118, 110), // #0F766E jade
            accent_highlight: Color32::from_rgb(11, 111, 102), // #0B6F66 (5.60:1 on bg_app)
            accent_warm: Color32::from_rgb(172, 78, 8), // #AC4E08 (4.82:1 on bg_sidebar)
            info: Color32::from_rgb(3, 105, 161),    // #0369A1

            text_primary: Color32::from_rgb(16, 32, 29), // #10201D
            text_secondary: Color32::from_rgb(56, 75, 71), // #384B47
            text_muted: Color32::from_rgb(95, 113, 108), // #5F716C
            text_dim: Color32::from_rgb(94, 109, 105),   // #5E6D69 (5.04:1 on bg_app)

            bit_perfect_green: Color32::from_rgb(21, 128, 61), // #15803D
            warning_yellow: Color32::from_rgb(180, 83, 9),     // #B45309

            bg_active_row: Color32::from_rgb(223, 243, 238), // #DFF3EE
            border_active_row: Color32::from_rgb(15, 118, 110), // #0F766E
            artwork_placeholder: Color32::from_rgb(227, 235, 232), // #E3EBE8
            destructive: Color32::from_rgb(190, 18, 60),     // #BE123C
            overlay_scrim: Color32::from_rgba_unmultiplied(255, 255, 255, 170),
            meter_bg: Color32::from_rgb(238, 243, 241), // #EEF3F1
            meter_gutter: Color32::from_rgb(220, 229, 226), // #DCE5E2
            meter_grid: Color32::from_rgba_unmultiplied(16, 32, 29, 30),
            scan_banner_bg: Color32::from_rgb(226, 244, 239), // #E2F4EF
            sun_accent: Color32::from_rgb(180, 83, 9),
        }
    }

    pub fn toggle(&mut self) {
        if self.is_light {
            *self = Self::dark();
        } else {
            *self = Self::light();
        }
    }

    /// Apply theme styles to egui context.
    ///
    /// The app calls this only when the theme actually changes; calling it every
    /// frame made egui rebuild style-dependent layout and tessellation work for
    /// no visual benefit.
    pub fn apply(&self, ctx: &egui::Context) {
        let mut visuals = if self.is_light {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        };

        visuals.panel_fill = self.bg_app;
        visuals.window_fill = self.bg_card;
        visuals.faint_bg_color = self.bg_card;
        visuals.extreme_bg_color = self.bg_app;
        visuals.hyperlink_color = self.accent_highlight;
        visuals.warn_fg_color = self.warning_yellow;
        visuals.error_fg_color = self.destructive;

        let rounding = CornerRadius::same(10);
        visuals.widgets.noninteractive.bg_fill = self.bg_card;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, self.border_subtle);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, self.text_primary);
        visuals.widgets.noninteractive.corner_radius = rounding;

        visuals.widgets.inactive.bg_fill = self.bg_pill;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, self.border_subtle);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, self.text_secondary);
        visuals.widgets.inactive.corner_radius = rounding;

        visuals.widgets.hovered.bg_fill = self.bg_pill_hover;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, self.accent_highlight);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, self.text_primary);
        visuals.widgets.hovered.corner_radius = rounding;

        visuals.widgets.active.bg_fill = self.accent;
        visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, self.accent_highlight);
        visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
        visuals.widgets.active.corner_radius = rounding;

        visuals.selection.bg_fill = self.accent;
        visuals.selection.stroke = Stroke::new(1.0_f32, self.accent_highlight);

        visuals.window_corner_radius = CornerRadius::same(14);
        visuals.window_stroke = Stroke::new(1.0_f32, self.border_card);
        visuals.menu_corner_radius = CornerRadius::same(12);

        ctx.set_visuals(visuals);

        let mut style = (*ctx.style_of(ctx.theme())).clone();
        style.spacing.item_spacing = Vec2::new(10.0, 10.0);
        style.spacing.window_margin = Margin::same(20);
        style.spacing.button_padding = Vec2::new(14.0, 8.0);
        style.spacing.interact_size.y = 36.0;
        style.text_styles = [
            (TextStyle::Small, font(10.5)),
            (TextStyle::Body, font(13.0)),
            (TextStyle::Button, font(13.0)),
            (TextStyle::Heading, font(21.0)),
            (TextStyle::Monospace, mono_font(12.0)),
        ]
        .into();
        ctx.set_style_of(ctx.theme(), style);
    }
}

#[cfg(test)]
mod tests {
    use super::Theme;
    use eframe::egui::Color32;

    /// The relative luminance of a colour, per WCAG 2.1.
    fn luminance(c: Color32) -> f32 {
        fn channel(v: u8) -> f32 {
            let v = v as f32 / 255.0;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
    }

    /// The WCAG contrast ratio between two colours: 1.0 to 21.0.
    fn contrast_ratio(a: Color32, b: Color32) -> f32 {
        let (hi, lo) = {
            let (la, lb) = (luminance(a), luminance(b));
            if la >= lb {
                (la, lb)
            } else {
                (lb, la)
            }
        };
        (hi + 0.05) / (lo + 0.05)
    }

    /// Every token that is used for body text has to clear WCAG AA against
    /// every surface it can land on. The light theme used to fail on
    /// `accent_highlight` and `text_dim`, and the dark theme on `text_dim`.
    #[test]
    fn text_tokens_clear_wcag_aa_in_both_themes() {
        for theme in [Theme::dark(), Theme::light()] {
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
                        "{} theme: {fg_name} on {bg_name} is {ratio:.2}:1, below the 4.5:1 AA bar",
                        if theme.is_light { "light" } else { "dark" }
                    );
                }
            }
        }
    }

    /// A text input's fill is very close to the page's, so its border is the
    /// only thing that identifies it. WCAG 1.4.11 asks for 3:1.
    #[test]
    fn input_borders_clear_the_non_text_contrast_bar() {
        for theme in [Theme::dark(), Theme::light()] {
            let ratio = contrast_ratio(theme.border_input, theme.bg_input);
            assert!(
                ratio >= 3.0,
                "{} theme: border_input on bg_input is {ratio:.2}:1, below the 3:1 bar",
                if theme.is_light { "light" } else { "dark" }
            );
        }
    }
}
