use std::path::{Path, PathBuf};

use super::schema::{LoadedTheme, ThemeError};
use super::system::system_theme_catalog;
use super::tokens::Theme;

/// Information needed to render a theme card in the gallery.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeCardInfo {
    pub id: String,
    pub name: String,
    pub author: String,
    pub description: String,
    pub tags: Vec<String>,
    pub is_light: bool,
    pub is_system: bool,
    pub is_active: bool,
    pub is_staged: bool,
    pub preview_colors: [eframe::egui::Color32; 4],
}

/// Manages system and user themes, filesystem discovery, staging, and activation.
pub struct ThemeManager {
    /// Directory scanned for `.shadowtheme` files.
    pub themes_dir: PathBuf,
    /// Loaded user themes.
    pub loaded_themes: Vec<LoadedTheme>,
    /// Sorted names of available custom themes.
    pub theme_names: Vec<String>,
    /// Load errors reported per file.
    pub errors: Vec<(PathBuf, ThemeError)>,
    /// ID or name of the currently active committed theme.
    pub active_id: String,
    /// ID or name of a temporarily staged theme for live preview testing.
    pub staged_id: Option<String>,
    /// Favorite theme IDs.
    pub favorites: Vec<String>,
}

impl ThemeManager {
    /// Create a new manager for the given application data directory.
    pub fn new(data_dir: &Path) -> Self {
        Self {
            themes_dir: data_dir.join("themes"),
            loaded_themes: Vec::new(),
            theme_names: Vec::new(),
            errors: Vec::new(),
            active_id: "system:dark".to_string(),
            staged_id: None,
            favorites: Vec::new(),
        }
    }

    /// Scan `themes/` directory and load all `.shadowtheme` files.
    pub fn load_all(&mut self) {
        self.loaded_themes.clear();
        self.errors.clear();
        self.theme_names.clear();

        // Ensure themes directory exists
        let _ = std::fs::create_dir_all(&self.themes_dir);

        // Scan themes directory
        self.scan_directory(&self.themes_dir.clone(), &["shadowtheme", "json"]);

        // Deduplicate loaded themes by ID/name
        let mut seen = std::collections::HashSet::new();
        self.loaded_themes.retain(|t| seen.insert(t.metadata.id.clone()));

        // Sort by name for stable gallery presentation
        self.loaded_themes.sort_by(|a, b| a.metadata.name.cmp(&b.metadata.name));
        self.theme_names = self
            .loaded_themes
            .iter()
            .map(|t| t.metadata.name.clone())
            .collect();
    }

    fn scan_directory(&mut self, dir: &Path, extensions: &[&str]) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("Could not read directory {:?}: {e}", dir);
                return;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let matches_ext = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|ext| extensions.contains(&ext));

            if !matches_ext {
                continue;
            }

            match LoadedTheme::from_file(&path) {
                Ok(theme) => {
                    log::info!("Loaded theme '{}' from {:?}", theme.metadata.name, path);
                    self.loaded_themes.push(theme);
                }
                Err(e) => {
                    log::warn!("Failed to load theme {:?}: {e}", path);
                    self.errors.push((path, e));
                }
            }
        }
    }

    /// Return card metadata for all available themes (both system and user).
    pub fn all_theme_cards(&self) -> Vec<ThemeCardInfo> {
        let mut cards = Vec::new();

        // 1. System themes
        for sys in system_theme_catalog() {
            let t = (sys.theme_fn)();
            let is_active = self.active_id == sys.id;
            let is_staged = self.staged_id.as_deref() == Some(sys.id);

            cards.push(ThemeCardInfo {
                id: sys.id.to_string(),
                name: sys.name.to_string(),
                author: sys.author.to_string(),
                description: sys.description.to_string(),
                tags: sys.tags.iter().map(|&s| s.to_string()).collect(),
                is_light: sys.is_light,
                is_system: true,
                is_active,
                is_staged,
                preview_colors: [t.bg_app, t.accent, t.accent_highlight, t.accent_warm],
            });
        }

        // 2. User loaded themes
        for user in &self.loaded_themes {
            let t = user.into_theme();
            let is_active = self.active_id == user.metadata.id
                || self.active_id == user.metadata.name;
            let is_staged = self.staged_id.as_deref() == Some(&user.metadata.id)
                || self.staged_id.as_deref() == Some(&user.metadata.name);

            cards.push(ThemeCardInfo {
                id: user.metadata.id.clone(),
                name: user.metadata.name.clone(),
                author: user.metadata.author.clone(),
                description: user.metadata.description.clone(),
                tags: user.metadata.tags.clone(),
                is_light: user.metadata.is_light,
                is_system: false,
                is_active,
                is_staged,
                preview_colors: [t.bg_app, t.accent, t.accent_highlight, t.accent_warm],
            });
        }

        cards
    }

    /// Resolve an identifier or name into a concrete [`Theme`].
    pub fn resolve_theme(&self, id_or_name: &str, base_is_light: bool) -> Theme {
        // System themes
        match id_or_name {
            "system:dark" | "Obsidian Jade" => return Theme::dark(),
            "system:light" | "Studio Mint" => return Theme::light(),
            "system:oled" | "OLED Midnight" => return Theme::oled(),
            "" => {
                return if base_is_light {
                    Theme::light()
                } else {
                    Theme::dark()
                };
            }
            _ => {}
        }

        // Custom loaded themes
        if let Some(user_theme) = self
            .loaded_themes
            .iter()
            .find(|t| t.metadata.id == id_or_name || t.metadata.name == id_or_name)
        {
            return user_theme.into_theme();
        }

        // Fallback
        if base_is_light {
            Theme::light()
        } else {
            Theme::dark()
        }
    }

    /// Return the concrete Theme for the active committed ID.
    pub fn active_theme(&self) -> Theme {
        self.resolve_theme(&self.active_id, false)
    }

    /// Return the concrete Theme currently effective (staged if in preview, otherwise active).
    pub fn effective_theme(&self) -> Theme {
        if let Some(ref staged) = self.staged_id {
            self.resolve_theme(staged, false)
        } else {
            self.active_theme()
        }
    }

    /// Set a theme as temporarily staged for live previewing.
    pub fn stage_theme(&mut self, id_or_name: &str) {
        self.staged_id = Some(id_or_name.to_string());
    }

    /// Cancel the staged theme preview.
    pub fn unstage_theme(&mut self) {
        self.staged_id = None;
    }

    /// Commit the currently staged theme as the active theme.
    pub fn commit_staged(&mut self) -> Option<String> {
        if let Some(staged) = self.staged_id.take() {
            self.active_id = staged.clone();
            Some(staged)
        } else {
            None
        }
    }

    /// Check if a preview is currently staged.
    pub fn is_staged(&self) -> bool {
        self.staged_id.is_some()
    }

    /// Write an example theme file to `themes/example_ocean_night.shadowtheme`.
    pub fn write_example_theme(&self) -> std::io::Result<PathBuf> {
        let path = self.themes_dir.join("example_ocean_night.shadowtheme");
        if path.exists() {
            return Ok(path);
        }
        let example = include_str!("../../../../docs/themes/example_ocean_night.shadowtheme");
        std::fs::write(&path, example)?;
        Ok(path)
    }

    pub fn errors(&self) -> &[(PathBuf, ThemeError)] {
        &self.errors
    }

    pub fn theme_for(&self, name: &str) -> Option<Theme> {
        self.loaded_themes
            .iter()
            .find(|t| t.metadata.name == name || t.metadata.id == name)
            .map(|t| t.into_theme())
    }

    pub fn active_id(&self) -> &str {
        &self.active_id
    }
}
