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
    pub accent: eframe::egui::Color32,
    pub accent_highlight: eframe::egui::Color32,
    pub accent_warm: eframe::egui::Color32,
    pub bg_app: eframe::egui::Color32,
    pub bg_card: eframe::egui::Color32,
    pub bg_pill: eframe::egui::Color32,
    pub text_primary: eframe::egui::Color32,
    pub text_secondary: eframe::egui::Color32,
    pub text_muted: eframe::egui::Color32,
    pub text_dim: eframe::egui::Color32,
    pub border_card: eframe::egui::Color32,
    pub border_subtle: eframe::egui::Color32,
    pub play_button_shape: super::tokens::PlayButtonShape,
    pub fonts: ThemeFonts,
    /// Requested font families that are not bundled; those roles use Inter.
    pub missing_fonts: Vec<String>,
}

/// Custom themes shipped with the player, embedded so packaged builds can
/// write them into the user's `themes/custom-theme/` folder on first launch.
const BUNDLED_THEMES: &[(&str, &str)] = &[
    (
        "midnight-vinyl.stm",
        include_str!("custom-theme/midnight-vinyl.stm"),
    ),
    (
        "polar-drift.stm",
        include_str!("custom-theme/polar-drift.stm"),
    ),
    (
        "mocha-muse.stm",
        include_str!("custom-theme/mocha-muse.stm"),
    ),
    (
        "neon-tokyo.stm",
        include_str!("custom-theme/neon-tokyo.stm"),
    ),
    (
        "rosewood-smoke.stm",
        include_str!("custom-theme/rosewood-smoke.stm"),
    ),
    (
        "retro-groove.stm",
        include_str!("custom-theme/retro-groove.stm"),
    ),
    (
        "ink-and-wave.stm",
        include_str!("custom-theme/ink-and-wave.stm"),
    ),
    (
        "forest-hymn.stm",
        include_str!("custom-theme/forest-hymn.stm"),
    ),
    (
        "synthwave-sunset.stm",
        include_str!("custom-theme/synthwave-sunset.stm"),
    ),
    ("neon-grid.stm", include_str!("custom-theme/neon-grid.stm")),
    ("black-tie.stm", include_str!("custom-theme/black-tie.stm")),
    (
        "cafe-nocturne.stm",
        include_str!("custom-theme/cafe-nocturne.stm"),
    ),
    (
        "amber-terminal.stm",
        include_str!("custom-theme/amber-terminal.stm"),
    ),
    (
        "crimson-velvet.stm",
        include_str!("custom-theme/crimson-velvet.stm"),
    ),
    ("oat-latte.stm", include_str!("custom-theme/oat-latte.stm")),
    (
        "sunlit-studio.stm",
        include_str!("custom-theme/sunlit-studio.stm"),
    ),
    (
        "paper-and-ink.stm",
        include_str!("custom-theme/paper-and-ink.stm"),
    ),
    (
        "sage-and-clay.stm",
        include_str!("custom-theme/sage-and-clay.stm"),
    ),
    ("lilac-air.stm", include_str!("custom-theme/lilac-air.stm")),
    (
        "arctic-frost.stm",
        include_str!("custom-theme/arctic-frost.stm"),
    ),
    ("blueprint.stm", include_str!("custom-theme/blueprint.stm")),
    ("blue-hour.stm", include_str!("custom-theme/blue-hour.stm")),
    (
        "cherry-cola.stm",
        include_str!("custom-theme/cherry-cola.stm"),
    ),
    (
        "plum-circuit.stm",
        include_str!("custom-theme/plum-circuit.stm"),
    ),
    (
        "copper-patina.stm",
        include_str!("custom-theme/copper-patina.stm"),
    ),
    (
        "peach-fuzz.stm",
        include_str!("custom-theme/peach-fuzz.stm"),
    ),
    (
        "night-swim.stm",
        include_str!("custom-theme/night-swim.stm"),
    ),
    (
        "highlighter.stm",
        include_str!("custom-theme/highlighter.stm"),
    ),
    ("sea-glass.stm", include_str!("custom-theme/sea-glass.stm")),
    (
        "citrus-press.stm",
        include_str!("custom-theme/citrus-press.stm"),
    ),
    (
        "adobe-rose.stm",
        include_str!("custom-theme/adobe-rose.stm"),
    ),
    (
        "silverprint.stm",
        include_str!("custom-theme/silverprint.stm"),
    ),
    (
        "smoked-glass.stm",
        include_str!("custom-theme/smoked-glass.stm"),
    ),
];

/// Manages system and user themes, filesystem discovery, staging, and activation.
pub struct ThemeManager {
    /// Directory scanned for `.stm` files.
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

mod load;
mod persist;
mod resolve;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_initializes_custom_themes_dir() {
        let temp_dir = std::env::temp_dir().join("shadowplayer_test_themes_init");
        let manager = ThemeManager::new(&temp_dir);
        let expected_source = temp_dir.join("themes").join("custom-theme");
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
        let first = custom_dir.join("midnight-vinyl.stm");
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
            serde_json::from_str(include_str!("custom-theme/midnight-vinyl.stm")).unwrap();
        let metadata = theme_file.metadata.as_mut().unwrap();
        metadata.id = "missing-font-demo".into();
        metadata.name = "Missing Font Demo".into();
        theme_file.id = Some(metadata.id.clone());
        theme_file.name = Some(metadata.name.clone());
        theme_file.fonts.display = Some("Unavailable Display".into());
        theme_file.fonts.body = Some("DM Sans".into());
        theme_file.fonts.mono = Some("Unavailable Mono".into());
        std::fs::write(
            custom_dir.join("missing-font-demo.stm"),
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
        let external_file = src_dir.join("imported_sample.stm");
        let example = include_str!("custom-theme/example_ocean_night.stm");
        std::fs::write(&external_file, example).unwrap();

        let mut manager =
            ThemeManager::new_with_custom_dir(&target_dir, target_dir.join("custom-theme"));
        let imported_id = manager
            .import_theme_from_path(&external_file)
            .expect("import theme");
        assert_eq!(imported_id, "ocean-night");
        assert!(manager
            .custom_themes_dir
            .join("imported_sample.stm")
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

        let bad_file = temp_dir.join("invalid.stm");
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
