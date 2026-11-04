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

/// Build a `Color32` from raw channel bytes.
///
/// This is the sanctioned constructor for **image data** — pixels read from
/// artwork, averaged, or written into a texture buffer. Those bytes are not a
/// palette choice and cannot be replaced by a theme token; palette colours
/// still come from [`Theme`], and a view that hard-codes an RGB triple is
/// exactly what `tests/theme_purity.rs` exists to catch.
pub fn rgba_unmultiplied(r: u8, g: u8, b: u8, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// Foreground for text drawn on a bright positive-action fill.
///
/// Both skins render actions like "Apply Alignment" on
/// [`Theme::bit_perfect_green`], which is light in both. The foreground is
/// therefore a contrast constant, not a skin colour: a skin that changes is
/// expected to keep its positive fill light, and this stays black.
pub fn on_positive_fill() -> Color32 {
    Color32::BLACK
}

/// The most legible foreground for text drawn on `fill`.
///
/// Accent colours are not guaranteed to be dark. A custom theme may ship a
/// bright amber or lime accent, where white text fails WCAG while near-black
/// text passes comfortably. Hard-coding `Color32::WHITE` on an accent fill is
/// what produced unreadable selected tabs in light and pastel themes; every
/// selected chip, tab, badge, and primary button asks here instead.
pub fn on_fill(fill: Color32) -> Color32 {
    const DARK_INK: Color32 = Color32::from_rgb(15, 23, 42);
    if super::contrast::contrast_ratio(Color32::WHITE, fill)
        >= super::contrast::contrast_ratio(DARK_INK, fill)
    {
        Color32::WHITE
    } else {
        DARK_INK
    }
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
        let reduce_motion = self.reduce_motion;
        *self = if self.is_light {
            Self::dark()
        } else {
            Self::light()
        };
        self.reduce_motion = reduce_motion;
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
    pub fn content_panel_fill(&self) -> Color32 {
        self.bg_app
    }

    /// Guarantee that an interaction surface is visibly different from the
    /// surface it sits on.
    ///
    /// Custom themes routinely declare a `bg_card_hover` that is only one or
    /// two RGB steps from `bg_card`. On a light theme that is a ~1.08:1
    /// contrast ratio — technically a different colour, visually no hover at
    /// all. Settings rows, theme cards, and tab strips then look broken even
    /// though the code paints a hover. This nudges the hover toward the
    /// theme's text polarity (lighter on dark, darker on light) until the
    /// difference clears a perceptible floor, preserving hue.
    pub fn normalize_interaction_surfaces(mut self) -> Self {
        self.bg_card_hover = ensure_surface_delta(self.bg_card, self.bg_card_hover, self.is_light);
        self.bg_pill_hover = ensure_surface_delta(self.bg_pill, self.bg_pill_hover, self.is_light);
        self.bg_card_active =
            ensure_surface_delta(self.bg_card, self.bg_card_active, self.is_light);
        self.border_subtle = ensure_surface_delta(self.bg_card, self.border_subtle, self.is_light);
        self
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
        visuals.widgets.inactive.weak_bg_fill = self.bg_pill;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, self.border_subtle);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, self.text_secondary);
        visuals.widgets.inactive.corner_radius = rounding;

        visuals.widgets.hovered.bg_fill = self.bg_pill_hover;
        visuals.widgets.hovered.weak_bg_fill = self.bg_pill_hover;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, self.accent_highlight);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, self.text_primary);
        visuals.widgets.hovered.corner_radius = rounding;

        visuals.widgets.active.bg_fill = self.accent;
        visuals.widgets.active.weak_bg_fill = self.accent;
        visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, self.accent_highlight);
        visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, on_fill(self.accent));
        visuals.widgets.active.corner_radius = rounding;

        visuals.widgets.open.bg_fill = self.bg_pill_hover;
        visuals.widgets.open.weak_bg_fill = self.bg_pill_hover;
        visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, self.accent_highlight);
        visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, self.text_primary);
        visuals.widgets.open.corner_radius = rounding;

        visuals.selection.bg_fill = self.accent;
        visuals.selection.stroke = Stroke::new(1.0_f32, self.accent_highlight);

        visuals.window_corner_radius = CornerRadius::same(self.radius_xl);
        visuals.window_stroke = Stroke::new(1.0_f32, self.border_card);
        visuals.menu_corner_radius = CornerRadius::same(self.radius_lg);
        visuals.popup_shadow = egui::epaint::Shadow {
            offset: [0, 6],
            blur: 20,
            spread: 0,
            color: self.shadow,
        };
        visuals.window_shadow = egui::epaint::Shadow {
            offset: [0, 10],
            blur: 32,
            spread: 0,
            color: self.shadow,
        };

        ctx.set_visuals(visuals);

        let mut style = (*ctx.style_of(ctx.theme())).clone();
        style.spacing.item_spacing = Vec2::new(10.0, 10.0);
        style.spacing.window_margin = Margin::same(20);
        style.spacing.button_padding = Vec2::new(12.0, 7.0);
        style.spacing.interact_size.y = 34.0;
        style.spacing.menu_margin = Margin::same(6);
        style.spacing.combo_width = 160.0;
        style.spacing.icon_width = 16.0;
        style.spacing.icon_spacing = 6.0;
        style.spacing.tooltip_width = 360.0;
        // Reserve a gutter between scrollable content and the scrollbar.
        // egui's floating scrollbars are painted over the content area by
        // default, so cards ended up underneath the handle. The margin moves
        // the content inwards (right for vertical bars, bottom for horizontal
        // ones) and keeps the bar legible without changing every call site.
        style.spacing.scroll.content_margin = Margin {
            left: 0,
            right: 18,
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
}

/// How far an interaction surface must move away from its base before the
/// change reads as feedback rather than a rendering artefact.
const MIN_SURFACE_DELTA: f32 = 1.16;

/// Nudge `surface` away from `base` until the two are perceptibly different.
///
/// The walk is capped at 18% toward the theme's text polarity, so a theme with
/// an intentionally subtle hover keeps its character while gaining a floor the
/// eye can actually see.
fn ensure_surface_delta(base: Color32, surface: Color32, is_light: bool) -> Color32 {
    if base.a() == 0 || super::contrast::contrast_ratio(surface, base) >= MIN_SURFACE_DELTA {
        return surface;
    }
    let target = if is_light {
        Color32::from_rgb(15, 23, 42)
    } else {
        Color32::WHITE
    };
    let mut candidate = surface;
    for step in 1..=18 {
        candidate = lerp_color(surface, target, step as f32 * 0.01);
        if super::contrast::contrast_ratio(candidate, base) >= MIN_SURFACE_DELTA {
            return candidate;
        }
    }
    candidate
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

    #[test]
    fn interaction_surfaces_are_never_invisible() {
        let mut theme = Theme::light();
        // A custom theme whose hover is one RGB step from the card.
        theme.bg_card = Color32::from_rgb(255, 255, 255);
        theme.bg_card_hover = Color32::from_rgb(253, 254, 254);
        theme.bg_pill = Color32::from_rgb(240, 240, 240);
        theme.bg_pill_hover = Color32::from_rgb(239, 240, 240);

        let normalized = theme.normalize_interaction_surfaces();
        assert!(
            crate::theme::contrast::contrast_ratio(normalized.bg_card_hover, normalized.bg_card)
                >= 1.16,
            "card hover must be perceptible"
        );
        assert!(
            crate::theme::contrast::contrast_ratio(normalized.bg_pill_hover, normalized.bg_pill)
                >= 1.16,
            "pill hover must be perceptible"
        );
    }

    #[test]
    fn on_fill_picks_the_readable_ink() {
        assert_eq!(on_fill(Color32::from_rgb(15, 118, 110)), Color32::WHITE);
        assert_eq!(
            on_fill(Color32::from_rgb(250, 204, 21)),
            Color32::from_rgb(15, 23, 42)
        );
    }
}
