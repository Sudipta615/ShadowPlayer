use std::sync::{Arc, OnceLock};

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontId};
use serde::{Deserialize, Serialize};

/// The one knob that controls the UI type scale.
pub const TEXT_SCALE: f32 = 1.08;
pub const TEXT_MIN: f32 = 10.0;

/// A font family that ships inside ShadowPlayer.
///
/// `display` is an optional heavier face for headings. Families without a
/// second weight reuse `regular` for both roles.
#[derive(Clone, Copy, Debug)]
pub struct BundledFontFamily {
    pub name: &'static str,
    pub regular: &'static [u8],
    pub display: &'static [u8],
}

/// Inter is the universal fallback and is always installed.
pub const INTER_FONT: BundledFontFamily = BundledFontFamily {
    name: "Inter",
    regular: include_bytes!("../../assets/fonts/Inter-Regular.ttf"),
    display: include_bytes!("../../assets/fonts/Inter-Medium.ttf"),
};

/// The default face for the `mono` role when a theme does not declare one.
pub const DEFAULT_MONO_FONT: BundledFontFamily = BundledFontFamily {
    name: "JetBrains Mono",
    regular: include_bytes!("../../assets/fonts/families/JetBrainsMono-Regular.ttf"),
    display: include_bytes!("../../assets/fonts/families/JetBrainsMono-Medium.ttf"),
};

/// Curated OFL families available to themes. Keep this list small enough that
/// the binary stays lean, and broad enough to cover editorial, geometric,
/// warm, retro, and technical typography.
pub const BUNDLED_FONT_FAMILIES: &[BundledFontFamily] = &[
    INTER_FONT,
    BundledFontFamily {
        name: "Fraunces",
        regular: include_bytes!("../../assets/fonts/families/Fraunces-Regular.ttf"),
        display: include_bytes!("../../assets/fonts/families/Fraunces-Medium.ttf"),
    },
    BundledFontFamily {
        name: "Instrument Serif",
        regular: include_bytes!("../../assets/fonts/families/InstrumentSerif-Regular.ttf"),
        display: include_bytes!("../../assets/fonts/families/InstrumentSerif-Regular.ttf"),
    },
    BundledFontFamily {
        name: "Space Grotesk",
        regular: include_bytes!("../../assets/fonts/families/SpaceGrotesk-Regular.ttf"),
        display: include_bytes!("../../assets/fonts/families/SpaceGrotesk-Medium.ttf"),
    },
    BundledFontFamily {
        name: "DM Sans",
        regular: include_bytes!("../../assets/fonts/families/DMSans-Regular.ttf"),
        display: include_bytes!("../../assets/fonts/families/DMSans-Medium.ttf"),
    },
    BundledFontFamily {
        name: "Bricolage Grotesque",
        regular: include_bytes!("../../assets/fonts/families/BricolageGrotesque-Regular.ttf"),
        display: include_bytes!("../../assets/fonts/families/BricolageGrotesque-Medium.ttf"),
    },
    DEFAULT_MONO_FONT,
];

/// The type roles a theme can set independently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontRole {
    Display,
    Body,
    Mono,
}

impl FontRole {
    pub const ALL: [FontRole; 3] = [FontRole::Display, FontRole::Body, FontRole::Mono];

    pub fn label(self) -> &'static str {
        match self {
            Self::Display => "Display",
            Self::Body => "Body",
            Self::Mono => "Mono",
        }
    }
}

/// Font-family declarations on a theme.
///
/// Omitted text roles use Inter; an omitted `mono` role uses JetBrains Mono.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeFonts {
    /// Headings and large display text. Alias: `heading`.
    #[serde(default, alias = "heading", skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    /// Normal UI and body copy. Alias: `text`.
    #[serde(default, alias = "text", skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Track numbers, technical values, and diagnostics. Alias: `monospace`.
    #[serde(default, alias = "monospace", skip_serializing_if = "Option::is_none")]
    pub mono: Option<String>,
}

impl ThemeFonts {
    pub fn is_empty(&self) -> bool {
        self.display.is_none() && self.body.is_none() && self.mono.is_none()
    }

    pub fn requested(&self, role: FontRole) -> Option<&str> {
        let family = match role {
            FontRole::Display => self.display.as_deref(),
            FontRole::Body => self.body.as_deref(),
            FontRole::Mono => self.mono.as_deref(),
        }?;
        let family = family.trim();
        (!family.is_empty()).then_some(family)
    }

    /// Unique font families this theme asks for that are not bundled.
    ///
    /// The gallery uses this to explain that those roles fall back to Inter.
    pub fn missing_fonts(&self) -> Vec<String> {
        let mut missing: Vec<String> = Vec::new();
        for role in FontRole::ALL {
            let Some(family) = self.requested(role) else {
                continue;
            };
            if resolve_font_family(family).is_none()
                && !missing
                    .iter()
                    .any(|known| known.eq_ignore_ascii_case(family))
            {
                missing.push(family.to_string());
            }
        }
        missing
    }

    /// Resolved family for a role.
    ///
    /// Text roles default to Inter. An omitted mono role defaults to JetBrains
    /// Mono so numeric columns keep their fixed advance; an explicitly
    /// requested but unavailable mono family still falls back to Inter.
    pub fn resolved(&self, role: FontRole) -> &'static BundledFontFamily {
        if let Some(requested) = self.requested(role) {
            return resolve_font_family(requested).unwrap_or(&INTER_FONT);
        }
        match role {
            FontRole::Mono => &DEFAULT_MONO_FONT,
            FontRole::Display | FontRole::Body => &INTER_FONT,
        }
    }
}

/// Look up a bundled family by display name. Matching is trimmed and
/// case-insensitive so hand-written theme files are forgiving.
pub fn resolve_font_family(name: &str) -> Option<&'static BundledFontFamily> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    BUNDLED_FONT_FAMILIES
        .iter()
        .find(|family| family.name.eq_ignore_ascii_case(name))
}

/// Names of every family a theme can request without a fallback warning.
pub fn bundled_font_family_names() -> impl Iterator<Item = &'static str> {
    BUNDLED_FONT_FAMILIES.iter().map(|family| family.name)
}

/// Font family used for headings and other display-sized text.
fn display_family() -> FontFamily {
    static DISPLAY: OnceLock<FontFamily> = OnceLock::new();
    DISPLAY
        .get_or_init(|| FontFamily::Name(Arc::from("shadow-display")))
        .clone()
}

/// The UI's proportional font at `size`, scaled and floored by the global
/// typography tokens. Sizes at or above display scale use the display face.
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

/// Install the default typefaces (Inter for text, JetBrains Mono for numbers).
/// Called once, before the first frame.
pub fn install_fonts(ctx: &egui::Context) {
    apply_theme_fonts(ctx, &ThemeFonts::default());
}

/// Install the typefaces requested by the effective theme.
///
/// Unknown families are ignored and their role falls back to Inter, so a
/// third-party theme can never leave the UI without a usable font. Every
/// selected family is followed by Inter and egui's default fonts, which keeps
/// emoji and uncommon glyphs working.
pub fn apply_theme_fonts(ctx: &egui::Context, selection: &ThemeFonts) {
    let mut fonts = FontDefinitions::default();

    add_font(&mut fonts, "inter-regular", INTER_FONT.regular);
    add_font(&mut fonts, "inter-medium", INTER_FONT.display);

    let default_proportional = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let default_monospace = fonts
        .families
        .get(&FontFamily::Monospace)
        .cloned()
        .unwrap_or_default();

    let body = selection.resolved(FontRole::Body);
    let display = selection.resolved(FontRole::Display);
    let mono = selection.resolved(FontRole::Mono);

    if body.name != INTER_FONT.name {
        add_font(&mut fonts, "theme-body", body.regular);
    }
    if display.name != INTER_FONT.name {
        add_font(&mut fonts, "theme-display", display.display);
    }
    if mono.name != INTER_FONT.name {
        add_font(&mut fonts, "theme-mono", mono.regular);
    }

    let mut proportional = Vec::new();
    if body.name != INTER_FONT.name {
        proportional.push("theme-body".to_owned());
    }
    proportional.push("inter-regular".to_owned());
    proportional.extend(default_proportional);
    fonts
        .families
        .insert(FontFamily::Proportional, proportional);

    let mut display_chain = Vec::new();
    if display.name != INTER_FONT.name {
        display_chain.push("theme-display".to_owned());
    }
    display_chain.push("inter-medium".to_owned());
    display_chain.push("inter-regular".to_owned());
    display_chain.extend(
        fonts
            .families
            .get(&FontFamily::Proportional)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|name| name != "theme-body" && name != "inter-regular"),
    );
    fonts.families.insert(display_family(), display_chain);

    let mut monospace = Vec::new();
    if mono.name != INTER_FONT.name {
        monospace.push("theme-mono".to_owned());
    }
    monospace.push("inter-regular".to_owned());
    monospace.extend(default_monospace);
    fonts.families.insert(FontFamily::Monospace, monospace);

    ctx.set_fonts(fonts);
}

fn add_font(fonts: &mut FontDefinitions, key: &str, bytes: &'static [u8]) {
    fonts
        .font_data
        .insert(key.to_owned(), Arc::new(FontData::from_static(bytes)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_families_are_reported_once() {
        let fonts = ThemeFonts {
            display: Some("Fraunces".into()),
            body: Some("Imaginary Sans".into()),
            mono: Some("imaginary sans".into()),
        };

        assert_eq!(fonts.missing_fonts(), vec!["Imaginary Sans"]);
        assert_eq!(
            fonts.resolved(FontRole::Display).name,
            "Fraunces",
            "bundled families should resolve"
        );
        assert_eq!(
            fonts.resolved(FontRole::Body).name,
            "Inter",
            "unknown families must fall back to Inter"
        );
        assert_eq!(
            ThemeFonts::default().resolved(FontRole::Mono).name,
            "JetBrains Mono",
            "an omitted mono role needs a monospaced default"
        );
        assert_eq!(
            ThemeFonts {
                mono: Some("Imaginary Mono".into()),
                ..ThemeFonts::default()
            }
            .resolved(FontRole::Mono)
            .name,
            "Inter",
            "an explicitly missing mono family falls back to Inter"
        );
    }

    #[test]
    fn family_lookup_is_trimmed_and_case_insensitive() {
        for name in ["fraunces", "  FRAUNCES  ", "Fraunces"] {
            let resolved =
                resolve_font_family(name).unwrap_or_else(|| panic!("{name} should resolve"));
            if name.trim().eq_ignore_ascii_case("fraunces") {
                assert_eq!(resolved.name, "Fraunces");
            }
        }
        assert!(resolve_font_family("   ").is_none());
        assert!(ThemeFonts::default().missing_fonts().is_empty());
    }

    #[test]
    fn every_bundled_family_resolves_to_itself() {
        for family in BUNDLED_FONT_FAMILIES {
            assert_eq!(
                resolve_font_family(family.name).map(|f| f.name),
                Some(family.name)
            );
            assert!(!family.regular.is_empty());
            assert!(!family.display.is_empty());
        }
    }

    #[test]
    fn every_bundled_family_and_the_fallback_install_into_egui() {
        let ctx = egui::Context::default();
        let mut selections: Vec<ThemeFonts> = BUNDLED_FONT_FAMILIES
            .iter()
            .map(|family| ThemeFonts {
                display: Some(family.name.to_owned()),
                body: Some(family.name.to_owned()),
                mono: Some(family.name.to_owned()),
            })
            .collect();
        selections.push(ThemeFonts {
            display: Some("Imaginary Display".into()),
            body: Some("Imaginary Body".into()),
            mono: Some("Imaginary Mono".into()),
        });

        for selection in selections {
            apply_theme_fonts(&ctx, &selection);
            let mut output = ctx.run_ui(Default::default(), |ui| {
                ui.label(egui::RichText::new("Aa 123").font(font(24.0)));
                ui.label(egui::RichText::new("Body copy").font(font(13.0)));
                ui.label(egui::RichText::new("01:42").font(mono_font(11.0)));
            });
            output.textures_delta.clear();
        }
    }
}
