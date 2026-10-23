use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui::Color32;
use serde::{Deserialize, Serialize};

use super::contrast::{validate_contrast, ContrastError};
use super::tokens::{parse_hex_color, Theme};

/// The colour tokens that every theme *must* provide. Tokens not in this list
/// fall back to the base theme (dark or light).
pub const REQUIRED_TOKENS: &[&str] = &[
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
    /// Optional v2 metadata object.
    #[serde(default)]
    pub metadata: Option<ThemeMetadata>,

    // Legacy v1 root fields:
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub is_light: Option<bool>,

    /// Flat colour tokens map (v1 and v2 flexible).
    #[serde(default)]
    pub colors: HashMap<String, String>,

    /// Optional radius overrides.
    #[serde(default)]
    pub radii: HashMap<String, u8>,
}

impl ThemeFile {
    /// Return the resolved metadata from either v2 metadata or legacy v1 fields.
    pub fn resolved_metadata(&self, fallback_id: &str) -> ThemeMetadata {
        if let Some(ref m) = self.metadata {
            let mut meta = m.clone();
            if meta.id.is_empty() {
                meta.id = fallback_id.to_string();
            }
            return meta;
        }

        let name = self.name.clone().unwrap_or_else(|| fallback_id.to_string());
        let author = self.author.clone().unwrap_or_default();
        let is_light = self.is_light.unwrap_or(false);

        ThemeMetadata {
            id: fallback_id.to_string(),
            name,
            author,
            description: String::new(),
            version: "1.0.0".to_string(),
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
}

impl LoadedTheme {
    /// Load and validate a theme from a JSON file.
    pub fn from_file(path: &Path) -> Result<Self, ThemeError> {
        let text = std::fs::read_to_string(path).map_err(|e| ThemeError::Io(e.to_string()))?;
        let theme_file: ThemeFile =
            serde_json::from_str(&text).map_err(|e| ThemeError::Json(e.to_string()))?;
        Self::from_theme_file(path.to_path_buf(), theme_file)
    }

    pub fn from_theme_file(path: PathBuf, theme_file: ThemeFile) -> Result<Self, ThemeError> {
        let fallback_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("custom_theme");
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
            ($field:ident) => {
                if let Some(&c) = self.colors.get(stringify!($field)) {
                    theme.$field = c;
                }
            };
        }

        apply_color!(bg_app);
        apply_color!(bg_sidebar);
        apply_color!(bg_card);
        apply_color!(bg_card_hover);
        apply_color!(bg_card_active);
        apply_color!(bg_pill);
        apply_color!(bg_pill_hover);
        apply_color!(bg_input);
        apply_color!(border_subtle);
        apply_color!(border_card);
        apply_color!(border_input);
        apply_color!(slider_track);
        apply_color!(accent);
        apply_color!(accent_highlight);
        apply_color!(accent_warm);
        apply_color!(info);
        apply_color!(text_primary);
        apply_color!(text_secondary);
        apply_color!(text_muted);
        apply_color!(text_dim);
        apply_color!(bit_perfect_green);
        apply_color!(warning_yellow);
        apply_color!(bg_active_row);
        apply_color!(border_active_row);
        apply_color!(artwork_placeholder);
        apply_color!(destructive);
        apply_color!(meter_bg);
        apply_color!(meter_gutter);
        apply_color!(meter_grid);
        apply_color!(scan_banner_bg);
        apply_color!(slider_thumb);
        apply_color!(slider_progress);
        apply_color!(frame_border);
        apply_color!(spectrum_gradient_lo);
        apply_color!(spectrum_gradient_hi);

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
