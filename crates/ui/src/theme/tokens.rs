use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke, TextStyle, Vec2};
use serde::{Deserialize, Serialize};

use super::fonts::{font, mono_font};

/// The silhouette of the primary transport play/pause button.
///
/// Themes can use this to express a little personality without changing the
/// control's size or hit target: a sharp square for a blueprint, a pill for a
/// retro or synthwave palette, and the default circle everywhere else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlayButtonShape {
    #[default]
    Circle,
    Rounded,
    Pill,
    Square,
}

impl PlayButtonShape {
    pub const ALL: [PlayButtonShape; 4] = [
        PlayButtonShape::Circle,
        PlayButtonShape::Rounded,
        PlayButtonShape::Pill,
        PlayButtonShape::Square,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Circle => "Circle",
            Self::Rounded => "Rounded",
            Self::Pill => "Pill",
            Self::Square => "Square",
        }
    }

    /// Corner radius for a painted silhouette of `size` in this theme.
    ///
    /// Circle and Pill follow the shorter dimension, so a pill stays a
    /// capsule rather than rounding into a circle.
    pub fn corner_radius(self, size: Vec2, theme: &Theme) -> CornerRadius {
        let radius = match self {
            Self::Circle => size.x.min(size.y) * 0.5,
            Self::Rounded => theme.radius_lg.max(theme.radius_md) as f32,
            Self::Pill => size.y * 0.5,
            Self::Square => theme.radius_xs as f32,
        };
        CornerRadius::same(radius.clamp(0.0, 255.0) as u8)
    }
}

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

    // Theme personality
    /// Silhouette used by the primary play/pause transport control.
    pub play_button_shape: PlayButtonShape,
    /// True when the translucent frosted-glass surface treatment is active.
    /// Components use this to add specular sheens and a live backdrop.
    pub frosted: bool,
    /// True when the user has asked for reduced motion. Components use
    /// [`Theme::motion_time`] so hover and transition fades snap instead of
    /// animating; egui's own animations follow the style set in
    /// [`Theme::apply`].
    pub reduce_motion: bool,

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
        let frosted = self.frosted;
        let reduce_motion = self.reduce_motion;
        *self = if self.is_light {
            Self::dark()
        } else {
            Self::light()
        };
        self.reduce_motion = reduce_motion;
        if frosted {
            *self = self.with_frosted_glass(true);
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

    /// A motion duration that honours the reduced-motion preference.
    ///
    /// egui has no global "disable animations" switch for explicitly timed
    /// fades, so every hover/transition animation asks here first.
    pub fn motion_time(&self, seconds: f32) -> f32 {
        if self.reduce_motion {
            0.0
        } else {
            seconds
        }
    }

    /// Corner radius for compact controls: search fields, navigation tabs,
    /// filter chips, and other pills that are not the transport button.
    ///
    /// `radius_pill` is a deliberate capsule request, not a universal
    /// default. The theme's declared transport silhouette is the strongest
    /// signal: a pill transport gets pill controls, a square transport gets
    /// square controls, and a rounded transport gets the theme's normal
    /// rounded scale. Themes that never declared a silhouette fall back to
    /// their radius scale, so a high-radius custom theme can still ask for a
    /// capsule while a boxy one keeps its tight corners. This is what stops
    /// every theme from being forced into the same pill shape.
    pub fn control_radius(&self) -> CornerRadius {
        let radius = match self.play_button_shape {
            PlayButtonShape::Pill => self.radius_pill,
            PlayButtonShape::Square => self.radius_sm,
            PlayButtonShape::Rounded => self.radius_md,
            PlayButtonShape::Circle => {
                if self.radius_md <= 4 {
                    // A tight scale is a boxy theme even when its pill token
                    // is proportionally large; rounding it into a capsule
                    // would fight the rest of the surface.
                    self.radius_sm
                } else if u32::from(self.radius_pill) >= 2 * u32::from(self.radius_md) {
                    self.radius_pill
                } else {
                    self.radius_md
                }
            }
        };
        CornerRadius::same(radius)
    }

    /// The fill behind dense content (tables, settings cards) in the central
    /// content area.
    ///
    /// Frosted mode keeps the backdrop visible at the window edges, but a
    /// fully transparent content panel puts a moving/blurred image directly
    /// behind row text. This scrim sits between the backdrop and the cards:
    /// the glass still reads as glass, while the table stays legible.
    pub fn content_panel_fill(&self) -> Color32 {
        if self.frosted {
            with_alpha(self.bg_app, if self.is_light { 196 } else { 172 })
        } else {
            self.bg_app
        }
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
        // Reserve a gutter between scrollable content and the scrollbar.
        // egui's floating scrollbars are painted over the content area by
        // default, so cards ended up underneath the handle. The margin moves
        // the content inwards (right for vertical bars, bottom for horizontal
        // ones) and keeps the bar legible without changing every call site.
        style.spacing.scroll.content_margin = Margin {
            left: 0,
            right: 10,
            top: 0,
            bottom: 10,
        };
        style.animation_time = self.motion_time(0.2);
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

    /// Return a copy of this theme with translucent frosted glass surfaces
    /// and optical specular edge highlights applied across cards and panels.
    ///
    /// This only supplies the *surface* half of the effect. The app paints a
    /// blurred artwork backdrop behind the shell whenever [`Theme::frosted`]
    /// is true, so these alpha values reveal a soft, defocused image instead
    /// of the desktop or a flat panel colour.
    pub fn with_frosted_glass(&self, enabled: bool) -> Self {
        let mut theme = *self;
        theme.frosted = enabled;
        if !enabled {
            return theme;
        }

        // Translucent surface opacities. Sidebar and inputs stay a little more
        // opaque than cards so navigation and text remain comfortably legible;
        // cards are the most transparent because they sit directly over the
        // blurred artwork.
        let (sidebar_a, card_a, hover_a, active_a, pill_a, pill_hover_a, input_a) = if self.is_light
        {
            (212, 168, 194, 214, 176, 202, 204)
        } else {
            (204, 148, 178, 202, 160, 190, 188)
        };

        theme.bg_sidebar = with_alpha(self.bg_sidebar, sidebar_a);
        theme.bg_card = with_alpha(self.bg_card, card_a);
        theme.bg_card_hover = with_alpha(self.bg_card_hover, hover_a);
        theme.bg_card_active = with_alpha(self.bg_card_active, active_a);
        theme.bg_pill = with_alpha(self.bg_pill, pill_a);
        theme.bg_pill_hover = with_alpha(self.bg_pill_hover, pill_hover_a);
        theme.bg_input = with_alpha(self.bg_input, input_a);
        theme.bg_active_row = with_alpha(self.bg_active_row, hover_a);

        // Specular rim / micro-bevel highlights: glass edges catch light and
        // create a delicate 1 px contour. A stronger shadow lifts the pane off
        // the blurred backdrop instead of letting it dissolve into it.
        if self.is_light {
            theme.border_card = Color32::from_rgba_unmultiplied(255, 255, 255, 176);
            theme.border_subtle = Color32::from_rgba_unmultiplied(255, 255, 255, 132);
            theme.border_input = Color32::from_rgba_unmultiplied(55, 72, 68, 150);
            theme.shadow = Color32::from_rgba_unmultiplied(31, 46, 43, 76);
        } else {
            theme.border_card = Color32::from_rgba_unmultiplied(255, 255, 255, 46);
            theme.border_subtle = Color32::from_rgba_unmultiplied(255, 255, 255, 28);
            theme.border_input = Color32::from_rgba_unmultiplied(190, 214, 209, 96);
            theme.shadow = Color32::from_rgba_unmultiplied(0, 0, 0, 150);
        }

        theme
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_radius_follows_the_declared_theme_style() {
        let mut theme = Theme::dark();
        theme.play_button_shape = PlayButtonShape::Rounded;
        assert_eq!(theme.control_radius(), CornerRadius::same(theme.radius_md));

        theme.play_button_shape = PlayButtonShape::Square;
        assert_eq!(theme.control_radius(), CornerRadius::same(theme.radius_sm));

        theme.play_button_shape = PlayButtonShape::Pill;
        assert_eq!(
            theme.control_radius(),
            CornerRadius::same(theme.radius_pill)
        );

        // A custom theme that never declared a silhouette still keeps its
        // high-radius capsule when the pill token is deliberately large.
        theme.play_button_shape = PlayButtonShape::Circle;
        theme.radius_md = 10;
        theme.radius_pill = 30;
        theme.radius_sm = 6;
        assert_eq!(
            theme.control_radius(),
            CornerRadius::same(theme.radius_pill)
        );

        // ...and a boxy scale stays square.
        theme.radius_md = 3;
        theme.radius_pill = 6;
        theme.radius_sm = 1;
        assert_eq!(theme.control_radius(), CornerRadius::same(theme.radius_sm));
    }

    #[test]
    fn default_themes_keep_transport_and_controls_out_of_a_pill() {
        for theme in [Theme::dark(), Theme::light(), Theme::oled()] {
            assert_eq!(theme.play_button_shape, PlayButtonShape::Rounded);
            assert!(
                theme.control_radius() == CornerRadius::same(theme.radius_md),
                "default controls should use the rounded control scale"
            );
        }
    }
}
