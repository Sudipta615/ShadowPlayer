use std::path::{Path, PathBuf};

use super::fonts::ThemeFonts;
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
    pub version: String,
    pub tags: Vec<String>,
    pub is_light: bool,
    pub is_system: bool,
    pub is_active: bool,
    pub is_staged: bool,
    pub preview_colors: [eframe::egui::Color32; 4],
    /// Requested font families that are not bundled; those roles use Inter.
    pub missing_fonts: Vec<String>,
}

/// Custom themes shipped with the player, embedded so packaged builds can
/// write them into the user's `themes/custom-theme/` folder on first launch.
const BUNDLED_THEMES: &[(&str, &str)] = &[
    (
        "midnight-vinyl.shadowtheme",
        include_str!("custom-theme/midnight-vinyl.shadowtheme"),
    ),
    (
        "polar-drift.shadowtheme",
        include_str!("custom-theme/polar-drift.shadowtheme"),
    ),
    (
        "mocha-muse.shadowtheme",
        include_str!("custom-theme/mocha-muse.shadowtheme"),
    ),
    (
        "neon-tokyo.shadowtheme",
        include_str!("custom-theme/neon-tokyo.shadowtheme"),
    ),
    (
        "rosewood-smoke.shadowtheme",
        include_str!("custom-theme/rosewood-smoke.shadowtheme"),
    ),
    (
        "retro-groove.shadowtheme",
        include_str!("custom-theme/retro-groove.shadowtheme"),
    ),
    (
        "ink-and-wave.shadowtheme",
        include_str!("custom-theme/ink-and-wave.shadowtheme"),
    ),
    (
        "forest-hymn.shadowtheme",
        include_str!("custom-theme/forest-hymn.shadowtheme"),
    ),
    (
        "synthwave-sunset.shadowtheme",
        include_str!("custom-theme/synthwave-sunset.shadowtheme"),
    ),
    (
        "neon-grid.shadowtheme",
        include_str!("custom-theme/neon-grid.shadowtheme"),
    ),
    (
        "black-tie.shadowtheme",
        include_str!("custom-theme/black-tie.shadowtheme"),
    ),
    (
        "cafe-nocturne.shadowtheme",
        include_str!("custom-theme/cafe-nocturne.shadowtheme"),
    ),
    (
        "amber-terminal.shadowtheme",
        include_str!("custom-theme/amber-terminal.shadowtheme"),
    ),
    (
        "crimson-velvet.shadowtheme",
        include_str!("custom-theme/crimson-velvet.shadowtheme"),
    ),
    (
        "oat-latte.shadowtheme",
        include_str!("custom-theme/oat-latte.shadowtheme"),
    ),
    (
        "sunlit-studio.shadowtheme",
        include_str!("custom-theme/sunlit-studio.shadowtheme"),
    ),
    (
        "paper-and-ink.shadowtheme",
        include_str!("custom-theme/paper-and-ink.shadowtheme"),
    ),
    (
        "sage-and-clay.shadowtheme",
        include_str!("custom-theme/sage-and-clay.shadowtheme"),
    ),
    (
        "lilac-air.shadowtheme",
        include_str!("custom-theme/lilac-air.shadowtheme"),
    ),
    (
        "arctic-frost.shadowtheme",
        include_str!("custom-theme/arctic-frost.shadowtheme"),
    ),
    (
        "blueprint.shadowtheme",
        include_str!("custom-theme/blueprint.shadowtheme"),
    ),
    (
        "blue-hour.shadowtheme",
        include_str!("custom-theme/blue-hour.shadowtheme"),
    ),
    (
        "cherry-cola.shadowtheme",
        include_str!("custom-theme/cherry-cola.shadowtheme"),
    ),
    (
        "plum-circuit.shadowtheme",
        include_str!("custom-theme/plum-circuit.shadowtheme"),
    ),
    (
        "copper-patina.shadowtheme",
        include_str!("custom-theme/copper-patina.shadowtheme"),
    ),
    (
        "peach-fuzz.shadowtheme",
        include_str!("custom-theme/peach-fuzz.shadowtheme"),
    ),
    (
        "night-swim.shadowtheme",
        include_str!("custom-theme/night-swim.shadowtheme"),
    ),
    (
        "highlighter.shadowtheme",
        include_str!("custom-theme/highlighter.shadowtheme"),
    ),
    (
        "sea-glass.shadowtheme",
        include_str!("custom-theme/sea-glass.shadowtheme"),
    ),
    (
        "citrus-press.shadowtheme",
        include_str!("custom-theme/citrus-press.shadowtheme"),
    ),
    (
        "adobe-rose.shadowtheme",
        include_str!("custom-theme/adobe-rose.shadowtheme"),
    ),
    (
        "silverprint.shadowtheme",
        include_str!("custom-theme/silverprint.shadowtheme"),
    ),
];

/// Manages system and user themes, filesystem discovery, staging, and activation.
pub struct ThemeManager {
    /// Directory scanned for `.shadowtheme` files.
    pub themes_dir: PathBuf,
    /// Dedicated directory for custom user themes (`themes/custom-theme`).
    pub custom_themes_dir: PathBuf,
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
    /// Whether loaded themes are potentially dirty and need a filesystem rescan.
    pub is_dirty: bool,
    /// Timestamp of last successful directory scan.
    pub last_loaded: Option<std::time::Instant>,
}

impl ThemeManager {
    /// Create a new manager for the given application data directory.
    pub fn new(data_dir: &Path) -> Self {
        let themes_dir = data_dir.join("themes");
        let source_custom_dir =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/theme/custom-theme");
        let custom_themes_dir = if source_custom_dir.is_dir() {
            source_custom_dir
        } else {
            themes_dir.join("custom-theme")
        };
        Self::new_with_custom_dir(data_dir, custom_themes_dir)
    }

    /// Create a new manager with an explicit custom themes directory.
    pub fn new_with_custom_dir(data_dir: &Path, custom_themes_dir: PathBuf) -> Self {
        let mut manager = Self {
            themes_dir: data_dir.join("themes"),
            custom_themes_dir,
            loaded_themes: Vec::new(),
            theme_names: Vec::new(),
            errors: Vec::new(),
            active_id: "system:dark".to_string(),
            staged_id: None,
            favorites: Vec::new(),
            is_dirty: false,
            last_loaded: None,
        };
        manager.load_saved_active_id();
        manager
    }

    /// Scan `themes/` and `custom-theme/` directories and load all `.shadowtheme` files.
    pub fn load_all(&mut self) {
        self.loaded_themes.clear();
        self.errors.clear();
        self.theme_names.clear();

        // Ensure both base themes directory and custom-theme subdirectory exist
        let _ = std::fs::create_dir_all(&self.themes_dir);
        let _ = std::fs::create_dir_all(&self.custom_themes_dir);

        // Also scan src/theme/custom-theme if it is not already our custom_themes_dir
        let source_custom_dir =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/theme/custom-theme");
        if source_custom_dir.is_dir() && source_custom_dir != self.custom_themes_dir {
            self.scan_directory(&source_custom_dir, &["shadowtheme", "json"]);
        }

        // Scan themes directory (recursively scanning custom-theme/ and subfolders)
        self.scan_directory(&self.themes_dir.clone(), &["shadowtheme", "json"]);
        // Explicitly scan custom_themes_dir
        self.scan_directory(&self.custom_themes_dir.clone(), &["shadowtheme", "json"]);

        // Deduplicate loaded themes by ID/name
        let mut seen = std::collections::HashSet::new();
        self.loaded_themes
            .retain(|t| seen.insert(t.metadata.id.clone()));

        // Sort by name for stable gallery presentation
        self.loaded_themes
            .sort_by(|a, b| a.metadata.name.cmp(&b.metadata.name));
        self.theme_names = self
            .loaded_themes
            .iter()
            .map(|t| t.metadata.name.clone())
            .collect();
        self.is_dirty = false;
        self.last_loaded = Some(std::time::Instant::now());
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
            if path.is_dir() {
                self.scan_directory(&path, extensions);
                continue;
            }

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
                version: "built-in".to_string(),
                tags: sys.tags.iter().map(|&s| s.to_string()).collect(),
                is_light: sys.is_light,
                is_system: true,
                is_active,
                is_staged,
                preview_colors: [t.bg_app, t.accent, t.accent_highlight, t.accent_warm],
                missing_fonts: Vec::new(),
            });
        }

        // 2. User loaded themes
        for user in &self.loaded_themes {
            let t = user.into_theme();
            let is_active =
                self.active_id == user.metadata.id || self.active_id == user.metadata.name;
            let is_staged = self.staged_id.as_deref() == Some(&user.metadata.id)
                || self.staged_id.as_deref() == Some(&user.metadata.name);

            cards.push(ThemeCardInfo {
                id: user.metadata.id.clone(),
                name: user.metadata.name.clone(),
                author: user.metadata.author.clone(),
                description: user.metadata.description.clone(),
                version: user.metadata.version.clone(),
                tags: user.metadata.tags.clone(),
                is_light: user.metadata.is_light,
                is_system: false,
                is_active,
                is_staged,
                preview_colors: [t.bg_app, t.accent, t.accent_highlight, t.accent_warm],
                missing_fonts: user.fonts.missing_fonts(),
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

    /// Resolve the typeface declarations for an identifier or name.
    ///
    /// System themes and unknown IDs get the default role mapping: Inter for
    /// display/body text and JetBrains Mono for technical text.
    pub fn resolve_fonts(&self, id_or_name: &str) -> ThemeFonts {
        self.loaded_themes
            .iter()
            .find(|t| t.metadata.id == id_or_name || t.metadata.name == id_or_name)
            .map(|t| t.fonts.clone())
            .unwrap_or_default()
    }

    /// Typefaces requested by the active committed theme.
    pub fn active_fonts(&self) -> ThemeFonts {
        self.resolve_fonts(&self.active_id)
    }

    /// Typefaces currently effective (staged preview wins over the active theme).
    pub fn effective_fonts(&self) -> ThemeFonts {
        self.staged_id
            .as_deref()
            .map(|staged| self.resolve_fonts(staged))
            .unwrap_or_else(|| self.active_fonts())
    }

    /// Return the concrete Theme for the active committed ID.
    pub fn active_theme(&self) -> Theme {
        // Peek at the active theme's light/dark flag to guide fallback resolution.
        // This prevents a custom light theme that fails to resolve from silently
        // falling back to dark mode.
        let is_light = self
            .loaded_themes
            .iter()
            .find(|t| t.metadata.id == self.active_id || t.metadata.name == self.active_id)
            .map(|t| t.metadata.is_light)
            .unwrap_or_else(|| self.active_id == "system:light");
        self.resolve_theme(&self.active_id, is_light)
    }

    /// Return the concrete Theme currently effective (staged if in preview, otherwise active).
    pub fn effective_theme(&self) -> Theme {
        if let Some(ref staged) = self.staged_id {
            // Infer the light/dark fallback from the staged theme's own metadata.
            let is_light = self
                .loaded_themes
                .iter()
                .find(|t| t.metadata.id == *staged || t.metadata.name == *staged)
                .map(|t| t.metadata.is_light)
                .unwrap_or_else(|| staged == "system:light");
            self.resolve_theme(staged, is_light)
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

    /// Commit the currently staged theme as the active theme and persist it.
    pub fn commit_staged(&mut self) -> Option<String> {
        if let Some(staged) = self.staged_id.take() {
            self.active_id = staged.clone();
            self.save_active_id();
            Some(staged)
        } else {
            None
        }
    }

    /// Check if a preview is currently staged.
    pub fn is_staged(&self) -> bool {
        self.staged_id.is_some()
    }

    /// Path to the file where active theme ID is persisted.
    pub fn active_theme_path(&self) -> PathBuf {
        self.themes_dir.join("active_theme.txt")
    }

    /// Persist the currently active theme ID to disk.
    pub fn save_active_id(&self) {
        let _ = std::fs::create_dir_all(&self.themes_dir);
        let _ = std::fs::write(self.active_theme_path(), &self.active_id);
    }

    /// Load the saved active theme ID from disk if it exists.
    pub fn load_saved_active_id(&mut self) -> Option<String> {
        let path = self.active_theme_path();
        if let Ok(content) = std::fs::read_to_string(&path) {
            let id = content.trim().to_string();
            if !id.is_empty() {
                self.active_id = id.clone();
                return Some(id);
            }
        }
        None
    }

    /// Explicitly set the active theme ID and save it to disk.
    pub fn set_active_id(&mut self, id: impl Into<String>) {
        self.active_id = id.into();
        self.save_active_id();
    }

    /// Mark the manager as dirty, indicating the filesystem cache may be stale.
    pub fn mark_dirty(&mut self) {
        self.is_dirty = true;
    }

    /// Reload themes from disk only if marked dirty.
    pub fn reload_if_dirty(&mut self) {
        if self.is_dirty {
            self.load_all();
        }
    }

    /// Import a theme file from any storage path into the `custom-theme/` directory.
    pub fn import_theme_from_path(&mut self, src_path: &Path) -> Result<String, String> {
        if !src_path.is_file() {
            return Err("Selected path is not a file".to_string());
        }

        // Validate theme format first
        let loaded = LoadedTheme::from_file(src_path).map_err(|e| format!("{e}"))?;

        // Ensure custom-theme directory exists
        let _ = std::fs::create_dir_all(&self.custom_themes_dir);

        let file_name = src_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("imported_theme.shadowtheme");

        let dest_path = self.custom_themes_dir.join(file_name);

        if let Err(e) = std::fs::copy(src_path, &dest_path) {
            return Err(format!("Could not copy file: {e}"));
        }

        let theme_id = if !loaded.metadata.id.is_empty() {
            loaded.metadata.id.clone()
        } else {
            loaded.metadata.name.clone()
        };

        // Directly insert or update in memory to avoid redundant full-disk traversal
        self.loaded_themes.retain(|t| {
            t.metadata.id != loaded.metadata.id && t.metadata.name != loaded.metadata.name
        });
        self.loaded_themes.push(loaded);
        self.loaded_themes
            .sort_by(|a, b| a.metadata.name.cmp(&b.metadata.name));
        self.theme_names = self
            .loaded_themes
            .iter()
            .map(|t| t.metadata.name.clone())
            .collect();
        self.is_dirty = false;

        Ok(theme_id)
    }

    /// Open the `custom-theme/` folder in the system file manager.
    pub fn open_custom_themes_folder(&self) {
        let _ = std::fs::create_dir_all(&self.custom_themes_dir);
        #[cfg(target_os = "linux")]
        let _ = std::process::Command::new("xdg-open")
            .arg(&self.custom_themes_dir)
            .spawn();
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open")
            .arg(&self.custom_themes_dir)
            .spawn();
        #[cfg(target_os = "windows")]
        let _ = std::process::Command::new("explorer")
            .arg(&self.custom_themes_dir)
            .spawn();
    }

    /// Write an example theme file to `themes/custom-theme/example_ocean_night.shadowtheme`.
    pub fn write_example_theme(&self) -> std::io::Result<PathBuf> {
        let _ = std::fs::create_dir_all(&self.custom_themes_dir);
        let path = self
            .custom_themes_dir
            .join("example_ocean_night.shadowtheme");
        if path.exists() {
            return Ok(path);
        }
        let example = include_str!("../../../../docs/themes/example_ocean_night.shadowtheme");
        std::fs::write(&path, example)?;
        Ok(path)
    }

    /// Copy the themes bundled with the player into the custom-theme folder.
    ///
    /// A source checkout already has these files next to the theme module, but
    /// a packaged build does not: its `CARGO_MANIFEST_DIR` is a build-machine
    /// path. Writing them once into the user's data directory keeps the gallery
    /// identical in both cases. Existing files are never overwritten, so a user
    /// edit survives an upgrade.
    pub fn write_bundled_themes(&self) {
        if let Err(e) = std::fs::create_dir_all(&self.custom_themes_dir) {
            log::warn!(
                "Could not create custom theme directory {:?}: {e}",
                self.custom_themes_dir
            );
            return;
        }

        for (file_name, contents) in BUNDLED_THEMES {
            let path = self.custom_themes_dir.join(file_name);
            if path.exists() {
                continue;
            }
            if let Err(e) = std::fs::write(&path, contents) {
                log::warn!("Could not write bundled theme {:?}: {e}", path);
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_initializes_custom_themes_dir() {
        let temp_dir = std::env::temp_dir().join("shadowplayer_test_themes_init");
        let manager = ThemeManager::new(&temp_dir);
        let expected_source =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/theme/custom-theme");
        assert_eq!(manager.custom_themes_dir, expected_source);

        let isolated =
            ThemeManager::new_with_custom_dir(&temp_dir, temp_dir.join("themes/custom-theme"));
        assert_eq!(
            isolated.custom_themes_dir,
            temp_dir.join("themes/custom-theme")
        );
    }

    #[test]
    fn manager_writes_example_theme_and_loads_it() {
        let temp_dir = std::env::temp_dir().join("shadowplayer_test_example_theme");
        let _ = std::fs::remove_dir_all(&temp_dir);

        let mut manager =
            ThemeManager::new_with_custom_dir(&temp_dir, temp_dir.join("custom-theme"));
        let path = manager.write_example_theme().expect("write example theme");
        assert!(path.exists());
        assert_eq!(path.parent().unwrap(), manager.custom_themes_dir);

        manager.load_all();
        assert!(!manager.loaded_themes.is_empty());
        assert!(manager.theme_for("Ocean Night").is_some());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn manager_writes_every_bundled_theme() {
        let temp_dir = std::env::temp_dir().join("shadowplayer_test_bundled_themes");
        let custom_dir = temp_dir.join("custom-theme");
        let _ = std::fs::remove_dir_all(&temp_dir);

        let mut manager = ThemeManager::new_with_custom_dir(&temp_dir, custom_dir.clone());
        manager.write_bundled_themes();

        assert!(BUNDLED_THEMES.len() >= 32);
        for (file_name, _) in BUNDLED_THEMES {
            let path = custom_dir.join(file_name);
            assert!(path.is_file(), "missing bundled theme {}", path.display());
        }

        manager.load_all();
        assert!(manager.loaded_themes.len() >= BUNDLED_THEMES.len());
        assert!(manager.theme_for("Midnight Vinyl").is_some());
        assert!(manager.theme_for("Oat Latte").is_some());

        // A second write must not overwrite a user's edit.
        let first = custom_dir.join("midnight-vinyl.shadowtheme");
        std::fs::write(&first, "{}\n").unwrap();
        manager.write_bundled_themes();
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "{}\n");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn manager_reports_unbundled_fonts_and_resolves_fallbacks() {
        let temp_dir = std::env::temp_dir().join("shadowplayer_test_font_cards");
        let custom_dir = temp_dir.join("custom-theme");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&custom_dir).unwrap();

        let mut theme_file: super::super::schema::ThemeFile =
            serde_json::from_str(include_str!("custom-theme/midnight-vinyl.shadowtheme")).unwrap();
        let metadata = theme_file.metadata.as_mut().unwrap();
        metadata.id = "missing-font-demo".into();
        metadata.name = "Missing Font Demo".into();
        theme_file.id = Some(metadata.id.clone());
        theme_file.name = Some(metadata.name.clone());
        theme_file.fonts.display = Some("Unavailable Display".into());
        theme_file.fonts.body = Some("DM Sans".into());
        theme_file.fonts.mono = Some("Unavailable Mono".into());
        std::fs::write(
            custom_dir.join("missing-font-demo.shadowtheme"),
            serde_json::to_string(&theme_file).unwrap(),
        )
        .unwrap();

        let mut manager = ThemeManager::new_with_custom_dir(&temp_dir, custom_dir);
        manager.load_all();

        let card = manager
            .all_theme_cards()
            .into_iter()
            .find(|card| card.id == "missing-font-demo")
            .expect("fixture theme should load");
        assert_eq!(
            card.missing_fonts,
            vec!["Unavailable Display", "Unavailable Mono"]
        );

        let fonts = manager.resolve_fonts("missing-font-demo");
        assert_eq!(fonts.body.as_deref(), Some("DM Sans"));
        assert_eq!(manager.active_fonts(), ThemeFonts::default());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn manager_imports_theme_from_external_path() {
        let src_dir = std::env::temp_dir().join("shadowplayer_test_src");
        let target_dir = std::env::temp_dir().join("shadowplayer_test_target");
        let _ = std::fs::remove_dir_all(&src_dir);
        let _ = std::fs::remove_dir_all(&target_dir);

        std::fs::create_dir_all(&src_dir).unwrap();
        let external_file = src_dir.join("imported_sample.shadowtheme");
        let example = include_str!("../../../../docs/themes/example_ocean_night.shadowtheme");
        std::fs::write(&external_file, example).unwrap();

        let mut manager =
            ThemeManager::new_with_custom_dir(&target_dir, target_dir.join("custom-theme"));
        let imported_id = manager
            .import_theme_from_path(&external_file)
            .expect("import theme");
        assert_eq!(imported_id, "ocean-night");
        assert!(manager
            .custom_themes_dir
            .join("imported_sample.shadowtheme")
            .exists());
        assert!(manager.theme_for("Ocean Night").is_some());

        let _ = std::fs::remove_dir_all(&src_dir);
        let _ = std::fs::remove_dir_all(&target_dir);
    }

    #[test]
    fn manager_rejects_invalid_theme_import() {
        let temp_dir = std::env::temp_dir().join("shadowplayer_test_invalid_import");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let bad_file = temp_dir.join("invalid.shadowtheme");
        std::fs::write(&bad_file, "this is not valid json").unwrap();

        let mut manager =
            ThemeManager::new_with_custom_dir(&temp_dir, temp_dir.join("custom-theme"));
        let res = manager.import_theme_from_path(&bad_file);
        assert!(res.is_err());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn manager_persists_and_restores_active_id() {
        let temp_dir = std::env::temp_dir().join("shadowplayer_test_active_id_persist");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let mut manager =
            ThemeManager::new_with_custom_dir(&temp_dir, temp_dir.join("custom-theme"));
        assert_eq!(manager.active_id, "system:dark");

        // Staging and committing should persist
        manager.stage_theme("system:oled");
        let committed = manager.commit_staged();
        assert_eq!(committed.as_deref(), Some("system:oled"));
        assert_eq!(manager.active_id, "system:oled");
        assert!(manager.active_theme_path().exists());

        // A freshly created manager should restore the persisted active_id
        let restored = ThemeManager::new_with_custom_dir(&temp_dir, temp_dir.join("custom-theme"));
        assert_eq!(restored.active_id, "system:oled");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn manager_dirty_flag_invalidation() {
        let temp_dir = std::env::temp_dir().join("shadowplayer_test_dirty_flag");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let mut manager =
            ThemeManager::new_with_custom_dir(&temp_dir, temp_dir.join("custom-theme"));
        assert!(!manager.is_dirty);

        manager.mark_dirty();
        assert!(manager.is_dirty);

        manager.reload_if_dirty();
        assert!(!manager.is_dirty);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
