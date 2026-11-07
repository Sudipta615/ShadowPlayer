//! Constructing a manager and reading the theme directories.
//!
//! Discovery is the only place that walks the filesystem; the resolution and
//! persistence halves work on the catalog this fills in.

use super::*;

impl ThemeManager {
    pub fn new(data_dir: &Path) -> Self {
        let themes_dir = data_dir.join("themes");
        let custom_themes_dir = themes_dir.join("custom-theme");
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

    /// Scan `themes/` and `custom-theme/` directories and load all `.stm` (and legacy `.shadowtheme`) files.
    pub fn load_all(&mut self) {
        self.loaded_themes.clear();
        self.errors.clear();
        self.theme_names.clear();

        // Ensure both base themes directory and custom-theme subdirectory exist
        let _ = std::fs::create_dir_all(&self.themes_dir);
        let _ = std::fs::create_dir_all(&self.custom_themes_dir);

        // Scan themes directory (recursively scanning custom-theme/ and subfolders)
        self.scan_directory(&self.themes_dir.clone(), &["stm", "shadowtheme", "json"]);
        // Explicitly scan custom_themes_dir
        self.scan_directory(
            &self.custom_themes_dir.clone(),
            &["stm", "shadowtheme", "json"],
        );

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
        let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
        let mut visited = std::collections::HashSet::new();
        while let Some(dir) = stack.pop() {
            let canonical = match std::fs::canonicalize(&dir) {
                Ok(p) => p,
                Err(e) => {
                    log::warn!("Could not canonicalise directory {:?}: {e}", dir);
                    continue;
                }
            };
            // A symlink cycle would otherwise spin forever (or overflow the
            // stack); canonical paths deduplicate directories we have already
            // walked.
            if !visited.insert(canonical.clone()) {
                log::warn!("Skipping theme directory already visited: {:?}", canonical);
                continue;
            }
            let entries = match std::fs::read_dir(&canonical) {
                Ok(e) => e,
                Err(e) => {
                    log::warn!("Could not read directory {:?}: {e}", canonical);
                    continue;
                }
            };

            for entry in entries.flatten() {
                let path = entry.path();
                let metadata = match path.metadata() {
                    Ok(m) => m,
                    Err(e) => {
                        log::warn!("Could not read metadata for {:?}: {e}", path);
                        continue;
                    }
                };
                if metadata.is_dir() {
                    stack.push(path);
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
    }
}
