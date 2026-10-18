use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke, Vec2};

/// Color palette faithfully extracted from the reference design.
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

    // Accents & Gradients
    pub accent_purple: Color32,
    pub accent_magenta: Color32,
    pub accent_pink: Color32,
    pub accent_violet: Color32,
    pub accent_orange: Color32,
    pub accent_blue: Color32,

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
    /// Background of the \"scanning…\" information banner.
    pub scan_banner_bg: Color32,
    /// Sun / light-mode accent.
    pub sun_accent: Color32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            is_light: false,
            bg_app: Color32::from_rgb(3, 9, 23),             // #030917 Ultra-dark obsidian navy
            bg_sidebar: Color32::from_rgb(9, 14, 28),         // #090e1c Deep dark sidebar
            bg_card: Color32::from_rgb(11, 18, 34),          // #0b1222 Deep dark card surface
            bg_card_hover: Color32::from_rgb(22, 30, 48),    // #161e30 Card hover
            bg_card_active: Color32::from_rgb(19, 23, 57),   // #131739 Active row indigo glow
            bg_pill: Color32::from_rgb(8, 13, 27),           // #080d1b Pill background
            bg_pill_hover: Color32::from_rgb(19, 27, 50),    // #131b32 Pill hover
            bg_input: Color32::from_rgb(12, 19, 34),         // #0c1322 Search input
            border_subtle: Color32::from_rgb(19, 27, 44),    // #131b2c Subtle border
            border_card: Color32::from_rgb(25, 35, 56),      // #192338 Card border

            accent_purple: Color32::from_rgb(54, 42, 207),   // #362acf Electric royal indigo
            accent_magenta: Color32::from_rgb(129, 140, 248), // #818cf8 Luminous soft indigo
            accent_pink: Color32::from_rgb(236, 72, 153),    // #ec4899 Rose pink
            accent_violet: Color32::from_rgb(54, 42, 207),   // #362acf Primary active indigo
            accent_orange: Color32::from_rgb(249, 115, 22),  // #f97316 Amber orange
            accent_blue: Color32::from_rgb(59, 130, 246),    // #3b82f6 Sky blue

            text_primary: Color32::from_rgb(248, 249, 250),   // #f8f9fa Crisp off-white
            text_secondary: Color32::from_rgb(203, 213, 225), // #cbd5e1 Soft slate
            text_muted: Color32::from_rgb(148, 163, 184),     // #94a3b8 Slate text
            text_dim: Color32::from_rgb(100, 116, 139),       // #64748b Deep slate

            bit_perfect_green: Color32::from_rgb(52, 211, 153),
            warning_yellow: Color32::from_rgb(251, 191, 36),

            bg_active_row: Color32::from_rgb(19, 23, 57),
            border_active_row: Color32::from_rgb(54, 42, 207),
            artwork_placeholder: Color32::from_rgb(30, 24, 48),
            destructive: Color32::from_rgb(244, 63, 94),
            overlay_scrim: Color32::from_rgba_unmultiplied(0, 0, 0, 150),
            scan_banner_bg: Color32::from_rgb(38, 20, 64),
            sun_accent: Color32::from_rgb(245, 158, 11),
        }
    }
}

impl Theme {
    pub fn dark() -> Self {
        Self::default()
    }

    pub fn light() -> Self {
        Self {
            is_light: true,
            bg_app: Color32::from_rgb(243, 244, 248),
            bg_sidebar: Color32::from_rgb(234, 237, 244),
            bg_card: Color32::from_rgb(255, 255, 255),
            bg_card_hover: Color32::from_rgb(238, 242, 252),
            bg_card_active: Color32::from_rgb(233, 226, 250),
            bg_pill: Color32::from_rgb(228, 233, 245),
            bg_pill_hover: Color32::from_rgb(218, 225, 240),
            bg_input: Color32::from_rgb(240, 243, 250),
            border_subtle: Color32::from_rgb(212, 220, 235),
            border_card: Color32::from_rgb(204, 214, 232),

            accent_purple: Color32::from_rgb(147, 51, 234),
            accent_magenta: Color32::from_rgb(225, 29, 72),
            accent_pink: Color32::from_rgb(236, 72, 153),
            accent_violet: Color32::from_rgb(109, 40, 217),
            accent_orange: Color32::from_rgb(249, 115, 22),
            accent_blue: Color32::from_rgb(14, 165, 233),

            text_primary: Color32::from_rgb(15, 23, 42),
            text_secondary: Color32::from_rgb(51, 65, 85),
            text_muted: Color32::from_rgb(100, 116, 139),
            text_dim: Color32::from_rgb(148, 163, 184),

            bit_perfect_green: Color32::from_rgb(22, 163, 74),
            warning_yellow: Color32::from_rgb(202, 138, 4),

            bg_active_row: Color32::from_rgb(233, 226, 250),
            border_active_row: Color32::from_rgb(147, 51, 234),
            artwork_placeholder: Color32::from_rgb(228, 231, 240),
            destructive: Color32::from_rgb(220, 38, 38),
            overlay_scrim: Color32::from_rgba_unmultiplied(255, 255, 255, 170),
            scan_banner_bg: Color32::from_rgb(240, 236, 252),
            sun_accent: Color32::from_rgb(217, 119, 6),
        }
    }

    pub fn toggle(&mut self) {
        if self.is_light {
            *self = Self::dark();
        } else {
            *self = Self::light();
        }
    }

    /// Apply theme styles to egui context
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

        visuals.widgets.noninteractive.bg_fill = self.bg_card;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, self.border_subtle);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, self.text_primary);
        visuals.widgets.noninteractive.corner_radius = CornerRadius::same(10);

        visuals.widgets.inactive.bg_fill = self.bg_pill;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, self.border_subtle);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, self.text_secondary);
        visuals.widgets.inactive.corner_radius = CornerRadius::same(10);

        visuals.widgets.hovered.bg_fill = self.bg_pill_hover;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, self.accent_magenta);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, self.text_primary);
        visuals.widgets.hovered.corner_radius = CornerRadius::same(10);

        visuals.widgets.active.bg_fill = self.accent_violet;
        visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, self.accent_magenta);
        visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, self.text_primary);
        visuals.widgets.active.corner_radius = CornerRadius::same(10);

        visuals.selection.bg_fill = self.accent_violet;
        visuals.selection.stroke = Stroke::new(1.0_f32, self.accent_magenta);

        // Customize window frame
        visuals.window_corner_radius = CornerRadius::same(16);
        visuals.window_stroke = Stroke::new(1.0_f32, self.border_card);

        ctx.set_visuals(visuals);

        // Typography styling
        let mut style = (*ctx.style_of(ctx.theme())).clone();
        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.window_margin = Margin::same(16);
        style.spacing.button_padding = Vec2::new(12.0, 8.0);
        ctx.set_style_of(ctx.theme(), style);
    }
}
