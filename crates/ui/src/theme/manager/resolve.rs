//! Resolving ids to themes and fonts, and the staged-theme state.
//!
//! Everything here is a pure read of the loaded catalog (plus the staged
//! selection), so the gallery can ask for a card or a preview per frame
//! without touching the disk.

use super::*;

impl ThemeManager {
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
                accent: t.accent,
                accent_highlight: t.accent_highlight,
                accent_warm: t.accent_warm,
                bg_app: t.bg_app,
                bg_card: t.bg_card,
                bg_pill: t.bg_pill,
                text_primary: t.text_primary,
                text_secondary: t.text_secondary,
                text_muted: t.text_muted,
                text_dim: t.text_dim,
                border_card: t.border_card,
                border_subtle: t.border_subtle,
                play_button_shape: t.play_button_shape,
                fonts: ThemeFonts::default(),
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
                accent: t.accent,
                accent_highlight: t.accent_highlight,
                accent_warm: t.accent_warm,
                bg_app: t.bg_app,
                bg_card: t.bg_card,
                bg_pill: t.bg_pill,
                text_primary: t.text_primary,
                text_secondary: t.text_secondary,
                text_muted: t.text_muted,
                text_dim: t.text_dim,
                border_card: t.border_card,
                border_subtle: t.border_subtle,
                play_button_shape: t.play_button_shape,
                fonts: user.fonts.clone(),
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
}
