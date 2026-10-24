use eframe::egui::Color32;

use super::tokens::Theme;

/// Information about a built-in system theme preset.
#[derive(Clone, Debug)]
pub struct SystemThemeInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub author: &'static str,
    pub description: &'static str,
    pub tags: &'static [&'static str],
    pub is_light: bool,
    pub theme_fn: fn() -> Theme,
}

impl PartialEq for SystemThemeInfo {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Theme {
    /// Obsidian & Jade dark mode. Signature visual identity.
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
            modal_backdrop: Color32::from_black_alpha(128),
            drawer_scrim: Color32::from_black_alpha(80),
            shadow: Color32::from_black_alpha(160),
            meter_bg: Color32::from_rgb(12, 14, 22), // #0C0E16
            meter_gutter: Color32::from_rgb(18, 20, 32), // #121420
            meter_grid: Color32::from_rgba_unmultiplied(100, 116, 139, 40),

            slider_thumb: Color32::from_rgb(94, 234, 212),
            slider_progress: Color32::from_rgb(15, 118, 110),
            frame_border: Color32::from_rgb(42, 54, 57),
            frame_radius: 12,
            spectrum_gradient_lo: Color32::from_rgb(15, 118, 110),
            spectrum_gradient_hi: Color32::from_rgb(94, 234, 212),

            radius_xs: 2,
            radius_pill: 19,
            radius_sm: 6,
            radius_md: 8,
            radius_lg: 12,
            radius_xl: 14,
            space_xxxs: 1.0,
            space_xxs: 2.0,
            space_xs: 4.0,
            space_sm: 6.0,
            space_md: 8.0,
            space_lg: 10.0,
            space_xl: 12.0,
            space_2xl: 14.0,
            space_3xl: 16.0,
            space_4xl: 18.0,
            space_5xl: 20.0,
            card_w: 148.0,
            card_gutter: 14.0,
            scan_banner_bg: Color32::from_rgb(16, 43, 40), // #102B28
            sun_accent: Color32::from_rgb(245, 158, 11),
        }
    }

    /// Studio Mint light mode.
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
            modal_backdrop: Color32::from_black_alpha(56),
            drawer_scrim: Color32::from_black_alpha(24),
            shadow: Color32::from_black_alpha(160),
            meter_bg: Color32::from_rgb(238, 243, 241), // #EEF3F1
            meter_gutter: Color32::from_rgb(220, 229, 226), // #DCE5E2
            meter_grid: Color32::from_rgba_unmultiplied(16, 32, 29, 30),

            slider_thumb: Color32::from_rgb(15, 118, 110),
            slider_progress: Color32::from_rgb(15, 118, 110),
            frame_border: Color32::from_rgb(180, 195, 190),
            frame_radius: 12,
            spectrum_gradient_lo: Color32::from_rgb(15, 118, 110),
            spectrum_gradient_hi: Color32::from_rgb(11, 111, 102),

            radius_xs: 2,
            radius_pill: 19,
            radius_sm: 6,
            radius_md: 8,
            radius_lg: 12,
            radius_xl: 14,
            space_xxxs: 1.0,
            space_xxs: 2.0,
            space_xs: 4.0,
            space_sm: 6.0,
            space_md: 8.0,
            space_lg: 10.0,
            space_xl: 12.0,
            space_2xl: 14.0,
            space_3xl: 16.0,
            space_4xl: 18.0,
            space_5xl: 20.0,
            card_w: 148.0,
            card_gutter: 14.0,
            scan_banner_bg: Color32::from_rgb(226, 244, 239), // #E2F4EF
            sun_accent: Color32::from_rgb(180, 83, 9),
        }
    }

    /// OLED Midnight pure black theme for OLED displays and maximum contrast.
    pub fn oled() -> Self {
        let mut theme = Self::dark();
        theme.bg_app = Color32::from_rgb(0, 0, 0); // Pure black
        theme.bg_sidebar = Color32::from_rgb(0, 0, 0); // Pure black
        theme.bg_card = Color32::from_rgb(8, 10, 11);
        theme.bg_card_hover = Color32::from_rgb(16, 20, 22);
        theme.bg_pill = Color32::from_rgb(12, 16, 18);
        theme.bg_pill_hover = Color32::from_rgb(22, 28, 32);
        theme.bg_input = Color32::from_rgb(8, 10, 12);
        theme.border_subtle = Color32::from_rgb(24, 32, 36);
        theme.border_card = Color32::from_rgb(36, 48, 52);
        theme.meter_bg = Color32::from_rgb(0, 0, 0);
        theme.meter_gutter = Color32::from_rgb(10, 12, 16);
        theme
    }
}

/// Catalog of all built-in system themes.
pub fn system_theme_catalog() -> &'static [SystemThemeInfo] {
    static CATALOG: &[SystemThemeInfo] = &[
        SystemThemeInfo {
            id: "system:dark",
            name: "Obsidian Jade",
            author: "ShadowPlayer",
            description: "Signature dark theme with obsidian surfaces and jade/mint accents",
            tags: &["dark", "default", "system"],
            is_light: false,
            theme_fn: Theme::dark,
        },
        SystemThemeInfo {
            id: "system:light",
            name: "Studio Mint",
            author: "ShadowPlayer",
            description: "Crisp studio light mode with high-contrast jade accents",
            tags: &["light", "clean", "system"],
            is_light: true,
            theme_fn: Theme::light,
        },
        SystemThemeInfo {
            id: "system:oled",
            name: "OLED Midnight",
            author: "ShadowPlayer",
            description: "True-black theme optimized for OLED panels and battery savings",
            tags: &["oled", "dark", "high-contrast", "system"],
            is_light: false,
            theme_fn: Theme::oled,
        },
    ];
    CATALOG
}
