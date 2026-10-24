use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui::Color32;
use serde::{Deserialize, Serialize};

use super::contrast::{validate_contrast, ContrastError};
use super::tokens::{parse_hex_color, Theme};

/// Macro iterating over every required colour token.
/// This serves as the single source of truth for mandatory theme tokens.
#[macro_export]
macro_rules! for_each_required_color_token {
    ($mac:ident) => {
        $mac! {
            bg_app,
            bg_sidebar,
            bg_card,
            bg_card_hover,
            bg_card_active,
            bg_pill,
            bg_pill_hover,
            bg_input,
            border_subtle,
            border_card,
            border_input,
            slider_track,
            accent,
            accent_highlight,
            accent_warm,
            info,
            text_primary,
            text_secondary,
            text_muted,
            text_dim,
            bit_perfect_green,
            warning_yellow,
            bg_active_row,
            border_active_row,
            artwork_placeholder,
            destructive,
            meter_bg,
            meter_gutter,
            scan_banner_bg,
        }
    };
}
pub(crate) use for_each_required_color_token;

/// Macro iterating over every optional colour token that falls back to base theme.
#[macro_export]
macro_rules! for_each_optional_color_token {
    ($mac:ident) => {
        $mac! {
            meter_grid,
            slider_thumb,
            slider_progress,
            frame_border,
            spectrum_gradient_lo,
            spectrum_gradient_hi,
            overlay_scrim,
            modal_backdrop,
            drawer_scrim,
            shadow,
            sun_accent,
        }
    };
}
#[allow(unused_imports)]
pub(crate) use for_each_optional_color_token;

macro_rules! make_token_str_slice {
    ($($token:ident),* $(,)?) => {
        &[ $( stringify!($token) ),* ]
    };
}

/// The colour tokens that every theme *must* provide. Tokens not in this list
/// fall back to the base theme (dark or light).
pub const REQUIRED_TOKENS: &[&str] = for_each_required_color_token!(make_token_str_slice);

/// Errors that can occur when loading or validating a theme.
#[derive(Debug, Clone)]
pub enum ThemeError {
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

impl std::fmt::Display for ThemeError {
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

impl From<ContrastError> for ThemeError {
    fn from(err: ContrastError) -> Self {
        match err {
            ContrastError::ContrastFailure {
                fg,
                bg,
                ratio,
                required,
            } => Self::ContrastFailure {
                fg,
                bg,
                ratio,
                required,
            },
        }
    }
}

/// Metadata identifying a theme.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeMetadata {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub is_light: bool,
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_version() -> String {
    "2.0.0".to_string()
}

/// The raw JSON structure of a `.shadowtheme` (v2) or `.shadowskin` (legacy v1) file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeFile {
    /// Schema version for forward migration.  Current is 2; v1 had no schema_version.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,

    /// Optional v2 metadata object.
    #[serde(default)]
    pub metadata: Option<ThemeMetadata>,

    // Root-level metadata fields:
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub is_light: Option<bool>,

    /// Flat colour tokens map (v1 and v2 flexible).
    #[serde(default)]
    pub colors: HashMap<String, String>,

    /// Optional radius overrides.
    #[serde(default)]
    pub radii: HashMap<String, u8>,
}

fn default_schema_version() -> u32 {
    // Files without a schema_version field are treated as v1 (legacy).
    1
}

/// In-place migration helper: upgrades legacy v1 files to schema_version 2.
pub fn migrate_theme_file(theme_file: &mut ThemeFile, fallback_id: &str) {
    if theme_file.schema_version < 2 {
        // 1. Synthesize metadata if absent
        if theme_file.metadata.is_none() {
            theme_file.metadata = Some(theme_file.resolved_metadata(fallback_id));
        }

        // 2. Synthesize missing required tokens with sensible defaults
        let is_light = theme_file
            .metadata
            .as_ref()
            .map(|m| m.is_light)
            .unwrap_or(false);

        let bg_card = theme_file
            .colors
            .get("bg_card")
            .cloned()
            .unwrap_or_else(|| {
                if is_light {
                    "#f0f0f0".into()
                } else {
                    "#181818".into()
                }
            });
        let bg_card_hover = theme_file
            .colors
            .get("bg_card_hover")
            .cloned()
            .unwrap_or_else(|| bg_card.clone());
        let accent = theme_file
            .colors
            .get("accent")
            .cloned()
            .unwrap_or_else(|| "#4ade80".into());
        let accent_highlight = theme_file
            .colors
            .get("accent_highlight")
            .cloned()
            .unwrap_or_else(|| accent.clone());

        let meter_bg = theme_file
            .colors
            .get("bg_input")
            .cloned()
            .unwrap_or_else(|| {
                if is_light {
                    "#e0e0e0".into()
                } else {
                    "#111111".into()
                }
            });
        let scan_banner_bg = theme_file
            .colors
            .get("bg_pill")
            .cloned()
            .unwrap_or_else(|| bg_card.clone());

        theme_file
            .colors
            .entry("bg_active_row".into())
            .or_insert_with(|| bg_card_hover.clone());
        theme_file
            .colors
            .entry("border_active_row".into())
            .or_insert_with(|| accent.clone());
        theme_file
            .colors
            .entry("artwork_placeholder".into())
            .or_insert_with(|| bg_card.clone());
        theme_file
            .colors
            .entry("destructive".into())
            .or_insert_with(|| "#e05252".into());
        theme_file
            .colors
            .entry("meter_bg".into())
            .or_insert(meter_bg);
        theme_file
            .colors
            .entry("meter_gutter".into())
            .or_insert_with(|| bg_card.clone());
        theme_file
            .colors
            .entry("scan_banner_bg".into())
            .or_insert(scan_banner_bg);
        theme_file
            .colors
            .entry("accent_warm".into())
            .or_insert_with(|| accent_highlight.clone());
        theme_file
            .colors
            .entry("info".into())
            .or_insert_with(|| accent.clone());
        theme_file
            .colors
            .entry("bit_perfect_green".into())
            .or_insert_with(|| "#4ade80".into());
        theme_file
            .colors
            .entry("warning_yellow".into())
            .or_insert_with(|| "#facc15".into());

        // 3. Bump version
        theme_file.schema_version = 2;
    }
}

impl ThemeFile {
    /// Upgrade this theme file to schema_version 2 if it was an older format.
    pub fn migrate_to_latest(&mut self, fallback_id: &str) {
        migrate_theme_file(self, fallback_id);
    }

    /// Return the resolved metadata from either v2 metadata or root fields.
    pub fn resolved_metadata(&self, fallback_id: &str) -> ThemeMetadata {
        if let Some(ref m) = self.metadata {
            let mut meta = m.clone();
            if meta.id.is_empty() {
                meta.id = fallback_id.to_string();
            }
            return meta;
        }

        let id = self
            .id
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| fallback_id.to_string());
        let name = self.name.clone().unwrap_or_else(|| fallback_id.to_string());
        let author = self.author.clone().unwrap_or_default();
        let description = self.description.clone().unwrap_or_default();
        let is_light = self.is_light.unwrap_or(false);

        ThemeMetadata {
            id,
            name,
            author,
            description,
            version: "2.0.0".to_string(),
            is_light,
            tags: if is_light {
                vec!["light".into()]
            } else {
                vec!["dark".into()]
            },
        }
    }
}

/// A fully validated, loaded theme ready to convert into a [`Theme`].
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedTheme {
    pub path: PathBuf,
    pub metadata: ThemeMetadata,
    pub colors: HashMap<String, Color32>,
    pub radii: HashMap<String, u8>,
    /// The schema version declared in the source file.  1 = legacy, 2 = current.
    pub schema_version: u32,
}

impl LoadedTheme {
    /// Load and validate a theme from a JSON file.
    pub fn from_file(path: &Path) -> Result<Self, ThemeError> {
        let text = std::fs::read_to_string(path).map_err(|e| ThemeError::Io(e.to_string()))?;
        let theme_file: ThemeFile =
            serde_json::from_str(&text).map_err(|e| ThemeError::Json(e.to_string()))?;
        Self::from_theme_file(path.to_path_buf(), theme_file)
    }

    pub fn from_theme_file(path: PathBuf, mut theme_file: ThemeFile) -> Result<Self, ThemeError> {
        let fallback_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("custom_theme");

        // Automatically migrate legacy v1 themes to schema_version 2
        theme_file.migrate_to_latest(fallback_id);

        let metadata = theme_file.resolved_metadata(fallback_id);

        // Parse all colour tokens.
        let mut colors: HashMap<String, Color32> = HashMap::new();
        for (token, value) in &theme_file.colors {
            let color = parse_hex_color(value).ok_or_else(|| ThemeError::BadColor {
                token: token.clone(),
                value: value.clone(),
            })?;
            colors.insert(token.clone(), color);
        }

        // Check required tokens.
        for token in REQUIRED_TOKENS {
            if !colors.contains_key(*token) {
                return Err(ThemeError::MissingToken(token.to_string()));
            }
        }

        // Validate WCAG contrast.
        validate_contrast(&colors)?;

        Ok(Self {
            path,
            metadata,
            colors,
            radii: theme_file.radii,
            schema_version: theme_file.schema_version,
        })
    }

    /// Convert this theme into a concrete [`Theme`] struct.
    pub fn into_theme(&self) -> Theme {
        let mut theme = if self.metadata.is_light {
            Theme::light()
        } else {
            Theme::dark()
        };

        macro_rules! apply_color {
            ($($field:ident),* $(,)?) => {
                $(
                    if let Some(&c) = self.colors.get(stringify!($field)) {
                        theme.$field = c;
                    }
                )*
            };
        }

        for_each_required_color_token!(apply_color);
        for_each_optional_color_token!(apply_color);

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
        apply_radius!(frame_radius);

        theme.is_light = self.metadata.is_light;
        theme
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_tokens_list_covers_expected_tokens() {
        assert!(REQUIRED_TOKENS.contains(&"bg_app"));
        assert!(REQUIRED_TOKENS.contains(&"accent"));
        assert!(REQUIRED_TOKENS.contains(&"text_primary"));
        assert!(REQUIRED_TOKENS.contains(&"scan_banner_bg"));
        assert_eq!(REQUIRED_TOKENS.len(), 29);
    }

    #[test]
    fn legacy_v1_theme_migrates_to_v2_and_loads() {
        // Minimal legacy v1 file without schema_version, metadata block, or newer tokens
        let legacy_json = r##"{
            "name": "Legacy Dark Theme",
            "author": "Alice",
            "is_light": false,
            "colors": {
                "bg_app": "#121212",
                "bg_sidebar": "#161616",
                "bg_card": "#1e1e1e",
                "bg_card_hover": "#282828",
                "bg_card_active": "#323232",
                "bg_pill": "#242424",
                "bg_pill_hover": "#303030",
                "bg_input": "#1a1a1a",
                "border_subtle": "#2c2c2c",
                "border_card": "#333333",
                "border_input": "#777777",
                "slider_track": "#444444",
                "accent": "#4ade80",
                "accent_highlight": "#86efac",
                "text_primary": "#f8f8f8",
                "text_secondary": "#cccccc",
                "text_muted": "#aaaaaa",
                "text_dim": "#888888"
            }
        }"##;

        let theme_file: ThemeFile = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(theme_file.schema_version, 1);
        assert!(theme_file.metadata.is_none());

        let loaded =
            LoadedTheme::from_theme_file(PathBuf::from("legacy_dark.shadowskin"), theme_file)
                .expect("legacy theme should migrate and load successfully");

        assert_eq!(loaded.schema_version, 2);
        assert_eq!(loaded.metadata.name, "Legacy Dark Theme");
        assert_eq!(loaded.metadata.author, "Alice");
        assert!(!loaded.metadata.is_light);

        // Verify missing tokens were synthesized
        assert!(loaded.colors.contains_key("scan_banner_bg"));
        assert!(loaded.colors.contains_key("meter_bg"));
        assert!(loaded.colors.contains_key("meter_gutter"));
        assert!(loaded.colors.contains_key("bg_active_row"));
        assert!(loaded.colors.contains_key("border_active_row"));
        assert!(loaded.colors.contains_key("artwork_placeholder"));
        assert!(loaded.colors.contains_key("destructive"));

        // Verify into_theme converts successfully
        let concrete = loaded.into_theme();
        assert_eq!(concrete.bg_app, parse_hex_color("#121212").unwrap());
        assert_eq!(concrete.accent, parse_hex_color("#4ade80").unwrap());
    }
}
