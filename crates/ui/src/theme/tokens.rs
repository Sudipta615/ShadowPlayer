use std::sync::{Arc, OnceLock};

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke,
    TextStyle, Vec2,
};

/// Blend two colours, for animated state changes.
pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(
        mix(a.r(), b.r()),
        mix(a.g(), b.g()),
        mix(a.b(), b.b()),
        mix(a.a(), b.a()),
    )
}

/// The same colour with a different alpha.
pub fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

/// The one knob that controls the UI type scale.
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
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "inter-regular".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/Inter-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        "inter-medium".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/Inter-Medium.ttf"
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

/// Parse a `#RRGGBB` or `#RRGGBBAA` hex colour string.
pub fn parse_hex_color(hex: &str) -> Option<Color32> {
    let hex = hex.trim().strip_prefix('#').unwrap_or(hex.trim());
    match hex.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(Color32::from_rgb(r, g, b))
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            Some(Color32::from_rgba_unmultiplied(r, g, b, a))
        }
        _ => None,
    }
}

/// Format a `Color32` into a `#RRGGBB` or `#RRGGBBAA` hex string.
pub fn format_hex_color(color: Color32) -> String {
    if color.a() == 255 {
        format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b())
    } else {
        format!(
            "#{:02X}{:02X}{:02X}{:02X}",
            color.r(),
            color.g(),
            color.b(),
            color.a()
        )
    }
}

/// ShadowPlayer's visual identity tokens: surfaces, borders, accents, text,
/// layout metrics, and visualization tokens.
#[derive(Clone, Copy, Debug, PartialEq)]
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
    pub border_input: Color32,
    pub slider_track: Color32,

    // Accents
    pub accent: Color32,
    pub accent_highlight: Color32,
    pub accent_warm: Color32,
    pub info: Color32,

    // Text colors
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
    pub text_dim: Color32,

    // Status colors
    pub bit_perfect_green: Color32,
    pub warning_yellow: Color32,

    // Derived / semantic tokens
    pub bg_active_row: Color32,
    pub border_active_row: Color32,
    pub artwork_placeholder: Color32,
    pub destructive: Color32,
    pub overlay_scrim: Color32,
    pub modal_backdrop: Color32,
    pub drawer_scrim: Color32,
    pub shadow: Color32,
    pub meter_bg: Color32,
    pub meter_gutter: Color32,
    pub meter_grid: Color32,

    // Player controls & Visualizations
    pub slider_thumb: Color32,
    pub slider_progress: Color32,
    pub frame_border: Color32,
    pub frame_radius: u8,
    pub spectrum_gradient_lo: Color32,
    pub spectrum_gradient_hi: Color32,

    // Layout tokens
    pub radius_xs: u8,
    pub radius_pill: u8,
    pub radius_sm: u8,
    pub radius_md: u8,
    pub radius_lg: u8,
    pub radius_xl: u8,
    pub space_xxxs: f32,
    pub space_xxs: f32,
    pub space_xs: f32,
    pub space_sm: f32,
    pub space_md: f32,
    pub space_lg: f32,
    pub space_xl: f32,
    pub space_2xl: f32,
    pub space_3xl: f32,
    pub space_4xl: f32,
    pub space_5xl: f32,
    pub card_w: f32,
    pub card_gutter: f32,
    pub scan_banner_bg: Color32,
    pub sun_accent: Color32,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// Toggle between dark and light themes.
    pub fn toggle(&mut self) {
        if self.is_light {
            *self = Self::dark();
        } else {
            *self = Self::light();
        }
    }

    /// Returns true if this theme is OLED Midnight (pure black background).
    pub fn is_oled(&self) -> bool {
        !self.is_light && self.bg_app.r() == 0 && self.bg_app.g() == 0 && self.bg_app.b() == 0
    }

    /// Returns true if this theme is Obsidian Jade (standard dark mode).
    pub fn is_obsidian(&self) -> bool {
        !self.is_light && !self.is_oled()
    }

    /// Apply theme styles to egui context.
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

        let rounding = CornerRadius::same(self.radius_md);
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

        visuals.window_corner_radius = CornerRadius::same(self.radius_xl);
        visuals.window_stroke = Stroke::new(1.0_f32, self.border_card);
        visuals.menu_corner_radius = CornerRadius::same(self.radius_lg);

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
