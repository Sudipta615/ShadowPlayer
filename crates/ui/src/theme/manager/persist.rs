//! The active-theme marker, imports, and the bundled/example theme files.

use super::*;

impl ThemeManager {
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
            .unwrap_or("imported_theme.stm");

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

    /// Write an example theme file to `themes/custom-theme/example_ocean_night.stm`.
    pub fn write_example_theme(&self) -> std::io::Result<PathBuf> {
        let _ = std::fs::create_dir_all(&self.custom_themes_dir);
        let path = self.custom_themes_dir.join("example_ocean_night.stm");
        if path.exists() {
            return Ok(path);
        }
        let example = include_str!("../custom-theme/example_ocean_night.stm");
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
}
