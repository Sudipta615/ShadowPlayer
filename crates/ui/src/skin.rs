//! JSON skin format for ShadowPlayer user-defined themes.
//!
//! # File format
//!
//! A skin is a JSON file with a `.shadowskin` extension stored in
//! `<data_dir>/skins/`. The file contains every colour token in `#RRGGBB` or
//! `#RRGGBBAA` hex notation, plus optional radius overrides and metadata.
//!
//! ## Minimal example
//!
//! ```json
//! {
//!   "name": "Ocean Night",
//!   "author": "J. Smith",
//!   "is_light": false,
//!   "colors": {
//!     "bg_app":       "#07111A",
//!     "bg_sidebar":   "#091520",
//!     "bg_card":      "#0D1E2A",
//!     "bg_card_hover":"#152434",
//!     "bg_card_active":"#1A3A4A",
//!     "bg_pill":      "#112030",
//!     "bg_pill_hover":"#1A2E3E",
//!     "bg_input":     "#091825",
//!     "border_subtle": "#1E3040",
//!     "border_card":  "#2A3E50",
//!     "border_input": "#4A6E80",
//!     "slider_track": "#2A3E4A",
//!     "accent":       "#0D7A8A",
//!     "accent_highlight":"#5ECFDF",
//!     "accent_warm":  "#E5960A",
//!     "info":         "#3890E0",
//!     "text_primary": "#EEF5F8",
//!     "text_secondary":"#A0B8C0",
//!     "text_muted":   "#7090A0",
//!     "text_dim":     "#608090",
//!     "bit_perfect_green":"#2EC87A",
//!     "warning_yellow":"#E8B020",
//!     "bg_active_row":"#1A3A4A",
//!     "border_active_row":"#2DC0D0",
//!     "artwork_placeholder":"#102030",
//!     "destructive":  "#E87080",
//!     "meter_bg":     "#080E18",
//!     "meter_gutter": "#0E1420",
//!     "scan_banner_bg":"#102B28"
//!   }
//! }
//! ```
//!
//! # Loading
//!
//! Call [`SkinManager::load_all`] once at startup, then call
//! [`SkinManager::apply`] whenever the user selects a skin by name.
//! An invalid or WCAG-failing skin produces a user-visible error and is not
//! applied.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui::Color32;
use serde::{Deserialize, Serialize};

use crate::theme::Theme;

// ── JSON types ────────────────────────────────────────────────────────────────

/// The raw JSON structure of a `.shadowskin` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkinFile {
    /// Display name shown in the Settings picker.
    pub name: String,
    /// Optional author credit.
    #[serde(default)]
    pub author: String,
    /// Whether this is a light or dark theme base.
    #[serde(default)]
    pub is_light: bool,
    /// Colour tokens in `#RRGGBB` or `#RRGGBBAA` hex.
    pub colors: HashMap<String, String>,
    /// Optional radius overrides (must stay within the allowed token set).
    #[serde(default)]
    pub radii: HashMap<String, u8>,
}

/// A fully validated, loaded skin ready to convert into a [`Theme`].
#[derive(Debug, Clone)]
pub struct LoadedSkin {
    pub path: PathBuf,
    pub name: String,
    pub author: String,
    pub is_light: bool,
    colors: HashMap<String, Color32>,
    radii: HashMap<String, u8>,
}

/// Errors that can occur when loading or validating a skin.
#[derive(Debug, Clone)]
pub enum SkinError {
    /// File could not be read.
    Io(String),
    /// JSON parse failed.
    Json(String),
    /// A required colour token is missing.
    MissingToken(String),
    /// A colour string could not be parsed.
    BadColor { token: String, value: String },
    /// WCAG contrast requirement not met.
    ContrastFailure {
        fg: String,
        bg: String,
        ratio: f32,
        required: f32,
    },
}

impl std::fmt::Display for SkinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {e}"),
            Self::Json(e) => write!(f, "JSON error: {e}"),
            Self::MissingToken(t) => write!(f, "Missing required colour token: '{t}'"),
            Self::BadColor { token, value } => {
                write!(f, "Cannot parse colour '{value}' for token '{token}'")
            }
            Self::ContrastFailure {
                fg,
                bg,
                ratio,
                required,
            } => write!(
                f,
                "Contrast check failed: '{fg}' on '{bg}' is {ratio:.2}:1 (required {required:.1}:1)"
            ),
        }
    }
}

// ── Colour parsing ────────────────────────────────────────────────────────────

/// Parse a `#RRGGBB` or `#RRGGBBAA` hex string into `Color32`.
fn parse_hex_color(hex: &str) -> Option<Color32> {
    crate::theme::parse_hex_color(hex)
}

// ── Required tokens ───────────────────────────────────────────────────────────

/// The colour tokens that every skin *must* provide. Tokens not in this list
/// fall back to the base theme (dark or light).
const REQUIRED_TOKENS: &[&str] = &[
    "bg_app",
    "bg_sidebar",
    "bg_card",
    "bg_card_hover",
    "bg_card_active",
    "bg_pill",
    "bg_pill_hover",
    "bg_input",
    "border_subtle",
    "border_card",
    "border_input",
    "slider_track",
    "accent",
    "accent_highlight",
    "accent_warm",
    "info",
    "text_primary",
    "text_secondary",
    "text_muted",
    "text_dim",
    "bit_perfect_green",
    "warning_yellow",
    "bg_active_row",
    "border_active_row",
    "artwork_placeholder",
    "destructive",
    "meter_bg",
    "meter_gutter",
    "scan_banner_bg",
];

// ── WCAG contrast helpers ─────────────────────────────────────────────────────

fn luminance(c: Color32) -> f32 {
    fn ch(v: u8) -> f32 {
        let v = v as f32 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * ch(c.r()) + 0.7152 * ch(c.g()) + 0.0722 * ch(c.b())
}

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

// ── Skin loading and validation ───────────────────────────────────────────────

impl LoadedSkin {
    /// Load and validate a skin from a JSON file.
    pub fn from_file(path: &Path) -> Result<Self, SkinError> {
        let text = std::fs::read_to_string(path).map_err(|e| SkinError::Io(e.to_string()))?;
        let skin_file: SkinFile =
            serde_json::from_str(&text).map_err(|e| SkinError::Json(e.to_string()))?;
        Self::from_skin_file(path.to_path_buf(), skin_file)
    }

    fn from_skin_file(path: PathBuf, skin_file: SkinFile) -> Result<Self, SkinError> {
        // Parse all colour tokens.
        let mut colors: HashMap<String, Color32> = HashMap::new();
        for (token, value) in &skin_file.colors {
            let color = parse_hex_color(value).ok_or_else(|| SkinError::BadColor {
                token: token.clone(),
                value: value.clone(),
            })?;
            colors.insert(token.clone(), color);
        }

        // Check required tokens.
        for token in REQUIRED_TOKENS {
            if !colors.contains_key(*token) {
                return Err(SkinError::MissingToken(token.to_string()));
            }
        }

        // Validate WCAG contrast.
        validate_contrast(&colors)?;

        Ok(Self {
            path,
            name: skin_file.name,
            author: skin_file.author,
            is_light: skin_file.is_light,
            colors,
            radii: skin_file.radii,
        })
    }

    /// Convert this skin into a `Theme` by overlaying its tokens on top of the
    /// base dark or light theme.
    pub fn into_theme(&self) -> Theme {
        let mut theme = if self.is_light {
            Theme::light()
        } else {
            Theme::dark()
        };

        // Override every colour token present in the skin.
        macro_rules! apply {
            ($field:ident) => {
                if let Some(&c) = self.colors.get(stringify!($field)) {
                    theme.$field = c;
                }
            };
        }

        apply!(bg_app);
        apply!(bg_sidebar);
        apply!(bg_card);
        apply!(bg_card_hover);
        apply!(bg_card_active);
        apply!(bg_pill);
        apply!(bg_pill_hover);
        apply!(bg_input);
        apply!(border_subtle);
        apply!(border_card);
        apply!(border_input);
        apply!(slider_track);
        apply!(accent);
        apply!(accent_highlight);
        apply!(accent_warm);
        apply!(info);
        apply!(text_primary);
        apply!(text_secondary);
        apply!(text_muted);
        apply!(text_dim);
        apply!(bit_perfect_green);
        apply!(warning_yellow);
        apply!(bg_active_row);
        apply!(border_active_row);
        apply!(artwork_placeholder);
        apply!(destructive);
        apply!(meter_bg);
        apply!(meter_gutter);
        apply!(scan_banner_bg);

        // Override radius tokens if the skin provides them.
        macro_rules! apply_radius {
            ($field:ident) => {
                if let Some(&v) = self.radii.get(stringify!($field)) {
                    theme.$field = v;
                }
            };
        }

        apply_radius!(radius_xs);
        apply_radius!(radius_sm);
        apply_radius!(radius_md);
        apply_radius!(radius_lg);
        apply_radius!(radius_xl);
        apply_radius!(radius_pill);

        theme.is_light = self.is_light;
        theme
    }
}

/// Validate WCAG contrast for the text tokens from the skin's colour map.
fn validate_contrast(colors: &HashMap<String, Color32>) -> Result<(), SkinError> {
    let text_tokens = [
        "text_primary",
        "text_secondary",
        "text_muted",
        "text_dim",
        "accent_highlight",
        "accent_warm",
    ];
    let bg_tokens = ["bg_app", "bg_card", "bg_sidebar"];

    for fg_name in text_tokens {
        let Some(&fg) = colors.get(fg_name) else {
            continue;
        };
        for bg_name in bg_tokens {
            let Some(&bg) = colors.get(bg_name) else {
                continue;
            };
            let ratio = contrast_ratio(fg, bg);
            if ratio < 4.5 {
                return Err(SkinError::ContrastFailure {
                    fg: fg_name.to_string(),
                    bg: bg_name.to_string(),
                    ratio,
                    required: 4.5,
                });
            }
        }
    }

    // Input border must be 3:1 on bg_input.
    if let (Some(&bi), Some(&bgi)) = (colors.get("border_input"), colors.get("bg_input")) {
        let ratio = contrast_ratio(bi, bgi);
        if ratio < 3.0 {
            return Err(SkinError::ContrastFailure {
                fg: "border_input".to_string(),
                bg: "bg_input".to_string(),
                ratio,
                required: 3.0,
            });
        }
    }

    Ok(())
}

// ── SkinManager ───────────────────────────────────────────────────────────────

/// Manages the set of available user skins and the currently applied one.
pub struct SkinManager {
    /// The directory that is scanned for `.shadowskin` files.
    pub skins_dir: PathBuf,
    /// Successfully loaded skins, indexed by name.
    pub skins: Vec<LoadedSkin>,
    /// Sorted names of available skins.
    pub skin_names: Vec<String>,
    /// Load errors reported per file, shown in the Settings UI.
    pub errors: Vec<(PathBuf, SkinError)>,
    /// The name of the currently active skin, or `None` for the built-in theme.
    pub active: Option<String>,
}

impl SkinManager {
    /// Create a new manager pointing at `<data_dir>/skins/`.
    pub fn new(data_dir: &Path) -> Self {
        Self {
            skins_dir: data_dir.join("skins"),
            skins: Vec::new(),
            skin_names: Vec::new(),
            errors: Vec::new(),
            active: None,
        }
    }

    /// Scan the skins directory and load every `.shadowskin` file.
    pub fn load_all(&mut self) {
        self.skins.clear();
        self.errors.clear();
        self.skin_names.clear();

        if !self.skins_dir.exists() {
            if let Err(e) = std::fs::create_dir_all(&self.skins_dir) {
                log::warn!("Could not create skins directory: {e}");
            }
            return;
        }

        let entries = match std::fs::read_dir(&self.skins_dir) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("Could not read skins directory: {e}");
                return;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("shadowskin") {
                continue;
            }
            match LoadedSkin::from_file(&path) {
                Ok(skin) => {
                    log::info!("Loaded skin '{}' from {:?}", skin.name, path);
                    self.skins.push(skin);
                }
                Err(e) => {
                    log::warn!("Failed to load skin {:?}: {e}", path);
                    self.errors.push((path, e));
                }
            }
        }

        // Sort by name for a stable ordering in the picker.
        self.skins.sort_by(|a, b| a.name.cmp(&b.name));
        self.skin_names = self.skins.iter().map(|s| s.name.clone()).collect();
    }

    pub fn available_skins(&self) -> &[String] {
        &self.skin_names
    }

    pub fn active_skin_name(&self) -> Option<&str> {
        self.active.as_deref()
    }

    pub fn errors(&self) -> &[(PathBuf, SkinError)] {
        &self.errors
    }

    /// Return the `Theme` for the named skin, or `None` if not found.
    pub fn theme_for(&self, name: &str) -> Option<Theme> {
        self.skins
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.into_theme())
    }

    /// Apply the named skin, or the built-in base theme if `name` is empty.
    pub fn resolve_theme(&self, name: &str, base_is_light: bool) -> Theme {
        if name.is_empty() {
            if base_is_light {
                Theme::light()
            } else {
                Theme::dark()
            }
        } else {
            self.theme_for(name).unwrap_or_else(|| {
                if base_is_light {
                    Theme::light()
                } else {
                    Theme::dark()
                }
            })
        }
    }

    /// Write a minimal example skin file to the skins directory so the user
    /// can use it as a starting point.
    pub fn write_example_skin(&self) -> std::io::Result<PathBuf> {
        let path = self.skins_dir.join("example_ocean_night.shadowskin");
        if path.exists() {
            return Ok(path);
        }
        let example = include_str!("../../../docs/skins/example_ocean_night.shadowskin");
        std::fs::write(&path, example)?;
        Ok(path)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_skin_file(is_light: bool, overrides: &[(&str, &str)]) -> SkinFile {
        let base = if is_light {
            Theme::light()
        } else {
            Theme::dark()
        };

        fn hex(c: Color32) -> String {
            format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
        }

        let mut colors: HashMap<String, String> = HashMap::new();
        // Populate all required tokens from the base theme.
        colors.insert("bg_app".into(), hex(base.bg_app));
        colors.insert("bg_sidebar".into(), hex(base.bg_sidebar));
        colors.insert("bg_card".into(), hex(base.bg_card));
        colors.insert("bg_card_hover".into(), hex(base.bg_card_hover));
        colors.insert("bg_card_active".into(), hex(base.bg_card_active));
        colors.insert("bg_pill".into(), hex(base.bg_pill));
        colors.insert("bg_pill_hover".into(), hex(base.bg_pill_hover));
        colors.insert("bg_input".into(), hex(base.bg_input));
        colors.insert("border_subtle".into(), hex(base.border_subtle));
        colors.insert("border_card".into(), hex(base.border_card));
        colors.insert("border_input".into(), hex(base.border_input));
        colors.insert("slider_track".into(), hex(base.slider_track));
        colors.insert("accent".into(), hex(base.accent));
        colors.insert("accent_highlight".into(), hex(base.accent_highlight));
        colors.insert("accent_warm".into(), hex(base.accent_warm));
        colors.insert("info".into(), hex(base.info));
        colors.insert("text_primary".into(), hex(base.text_primary));
        colors.insert("text_secondary".into(), hex(base.text_secondary));
        colors.insert("text_muted".into(), hex(base.text_muted));
        colors.insert("text_dim".into(), hex(base.text_dim));
        colors.insert("bit_perfect_green".into(), hex(base.bit_perfect_green));
        colors.insert("warning_yellow".into(), hex(base.warning_yellow));
        colors.insert("bg_active_row".into(), hex(base.bg_active_row));
        colors.insert("border_active_row".into(), hex(base.border_active_row));
        colors.insert("artwork_placeholder".into(), hex(base.artwork_placeholder));
        colors.insert("destructive".into(), hex(base.destructive));
        colors.insert("meter_bg".into(), hex(base.meter_bg));
        colors.insert("meter_gutter".into(), hex(base.meter_gutter));
        colors.insert("scan_banner_bg".into(), hex(base.scan_banner_bg));

        // Apply overrides.
        for (k, v) in overrides {
            colors.insert(k.to_string(), v.to_string());
        }

        SkinFile {
            name: "Test".into(),
            author: "Tester".into(),
            is_light,
            colors,
            radii: HashMap::new(),
        }
    }

    #[test]
    fn base_theme_tokens_pass_validation() {
        // A skin built from the unmodified dark-theme colours must be valid.
        let skin_file = make_skin_file(false, &[]);
        let result = LoadedSkin::from_skin_file(PathBuf::from("test.shadowskin"), skin_file);
        assert!(result.is_ok(), "{:?}", result.err());
    }

    #[test]
    fn missing_token_is_an_error() {
        let mut skin_file = make_skin_file(false, &[]);
        skin_file.colors.remove("bg_app");
        let result = LoadedSkin::from_skin_file(PathBuf::from("test.shadowskin"), skin_file);
        assert!(matches!(result, Err(SkinError::MissingToken(t)) if t == "bg_app"));
    }

    #[test]
    fn bad_hex_color_is_an_error() {
        let skin_file = make_skin_file(false, &[("accent", "not-a-color")]);
        let result = LoadedSkin::from_skin_file(PathBuf::from("test.shadowskin"), skin_file);
        assert!(matches!(result, Err(SkinError::BadColor { .. })));
    }

    #[test]
    fn into_theme_overrides_accent() {
        // Override only the accent token; the rest should stay at dark defaults.
        let skin_file = make_skin_file(false, &[("accent", "#FF0000")]);
        let skin = LoadedSkin::from_skin_file(PathBuf::from("test.shadowskin"), skin_file).unwrap();
        let theme = skin.into_theme();
        assert_eq!(theme.accent, Color32::from_rgb(0xFF, 0x00, 0x00));
        // bg_app should still be the dark default.
        assert_eq!(theme.bg_app, Theme::dark().bg_app);
    }

    #[test]
    fn parse_hex_color_basic() {
        assert_eq!(
            parse_hex_color("#0F766E"),
            Some(Color32::from_rgb(0x0F, 0x76, 0x6E))
        );
        assert_eq!(
            parse_hex_color("#0F766E80"),
            Some(Color32::from_rgba_unmultiplied(0x0F, 0x76, 0x6E, 0x80))
        );
        assert_eq!(parse_hex_color("bad"), None);
    }
}
