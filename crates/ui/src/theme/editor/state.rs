use std::collections::HashMap;

use eframe::egui::Color32;

use crate::theme::contrast::{auto_adjust_contrast, validate_contrast};
use crate::theme::fonts::ThemeFonts;
use crate::theme::manager::ThemeManager;
use crate::theme::schema::{for_each_required_color_token, ThemeFile};
use crate::theme::tokens::{format_hex_color, PlayButtonShape, Theme};

/// The editable half of the editor, captured so dirtiness is derived rather
/// than hand-marked by every control. A control can be added without remembering
/// to set a flag, and a failed save leaves the baseline untouched.
#[derive(Clone, Debug, Default, PartialEq)]
struct EditorSnapshot {
    id: String,
    name: String,
    author: String,
    description: String,
    is_light: bool,
    colors: HashMap<String, Color32>,
    radii: HashMap<String, u8>,
    play_button_shape: PlayButtonShape,
    fonts: ThemeFonts,
}

#[derive(Clone, Debug)]
pub(super) struct PendingThemeLoad {
    theme: Theme,
    pub(super) name: String,
    id: String,
    fonts: ThemeFonts,
}

#[derive(Clone, Debug)]
pub(super) enum PendingAction {
    Close,
    Load(Box<PendingThemeLoad>),
}

/// State for the in-app Theme Editor.
#[derive(Debug, Clone)]
pub struct ThemeEditorState {
    pub is_open: bool,
    pub id: String,
    pub name: String,
    pub author: String,
    pub description: String,
    pub is_light: bool,
    pub colors: HashMap<String, Color32>,
    pub radii: HashMap<String, u8>,
    pub play_button_shape: PlayButtonShape,
    pub fonts: ThemeFonts,
    pub status_message: Option<(String, bool)>, // (message, is_error)
    saved: EditorSnapshot,
    pub(super) pending_overwrite: bool,
    pub(super) pending_action: Option<PendingAction>,
}

impl Default for ThemeEditorState {
    fn default() -> Self {
        let mut state = Self {
            is_open: false,
            id: "my_custom_theme".to_string(),
            name: "My Custom Theme".to_string(),
            author: "Me".to_string(),
            description: "Custom ShadowPlayer theme".to_string(),
            is_light: false,
            colors: HashMap::new(),
            radii: HashMap::new(),
            play_button_shape: PlayButtonShape::Circle,
            fonts: ThemeFonts::default(),
            status_message: None,
            saved: EditorSnapshot::default(),
            pending_overwrite: false,
            pending_action: None,
        };
        state.populate_from_theme(&Theme::dark());
        state.mark_saved();
        state
    }
}

impl ThemeEditorState {
    /// Populate editor values from an existing Theme.
    pub fn populate_from_theme(&mut self, theme: &Theme) {
        self.is_light = theme.is_light;

        macro_rules! set_editor_color {
            ($($field:ident),* $(,)?) => {
                $(
                    self.colors.insert(stringify!($field).to_string(), theme.$field);
                )*
            };
        }

        for_each_required_color_token!(set_editor_color);

        self.radii.insert("radius_xs".into(), theme.radius_xs);
        self.radii.insert("radius_sm".into(), theme.radius_sm);
        self.radii.insert("radius_md".into(), theme.radius_md);
        self.radii.insert("radius_lg".into(), theme.radius_lg);
        self.radii.insert("radius_xl".into(), theme.radius_xl);
        self.radii.insert("radius_pill".into(), theme.radius_pill);
        self.play_button_shape = theme.play_button_shape;
    }

    fn snapshot(&self) -> EditorSnapshot {
        EditorSnapshot {
            id: self.id.clone(),
            name: self.name.clone(),
            author: self.author.clone(),
            description: self.description.clone(),
            is_light: self.is_light,
            colors: self.colors.clone(),
            radii: self.radii.clone(),
            play_button_shape: self.play_button_shape,
            fonts: self.fonts.clone(),
        }
    }

    fn apply_snapshot(&mut self, snapshot: EditorSnapshot) {
        self.id = snapshot.id;
        self.name = snapshot.name;
        self.author = snapshot.author;
        self.description = snapshot.description;
        self.is_light = snapshot.is_light;
        self.colors = snapshot.colors;
        self.radii = snapshot.radii;
        self.play_button_shape = snapshot.play_button_shape;
        self.fonts = snapshot.fonts;
    }

    /// True when the draft differs from the last loaded/saved snapshot.
    pub fn is_dirty(&self) -> bool {
        self.snapshot() != self.saved
    }

    fn mark_saved(&mut self) {
        self.saved = self.snapshot();
        self.pending_overwrite = false;
    }

    pub(super) fn revert_to_saved(&mut self) {
        let saved = self.saved.clone();
        self.apply_snapshot(saved);
        self.status_message = None;
        self.pending_overwrite = false;
        self.pending_action = None;
    }

    /// Load a theme into the editor under a new, editable identity.
    pub fn load_for_customizing(&mut self, theme: &Theme, name: &str, id: &str) {
        self.load_with_fonts(theme, name, id, ThemeFonts::default());
    }

    /// Load a theme unless the current draft has unsaved changes.
    ///
    /// Returns `false` when the request was parked for confirmation; the
    /// editor shows the discard bar and calls back into `apply_pending_load`.
    pub fn request_load_for_customizing(
        &mut self,
        theme: &Theme,
        name: &str,
        id: &str,
        fonts: ThemeFonts,
    ) -> bool {
        if self.is_open && self.is_dirty() {
            self.pending_action = Some(PendingAction::Load(Box::new(PendingThemeLoad {
                theme: *theme,
                name: name.to_string(),
                id: id.to_string(),
                fonts,
            })));
            false
        } else {
            self.load_with_fonts(theme, name, id, fonts);
            true
        }
    }

    fn load_with_fonts(&mut self, theme: &Theme, name: &str, id: &str, fonts: ThemeFonts) {
        self.populate_from_theme(theme);
        let id = format!("{}_custom", id.replace([':', ' ', '/'], "_"));
        self.name = format!("{name} Custom");
        self.id = id;
        self.author = "Me".to_string();
        self.description = format!("A custom variation of {name}.");
        self.fonts = fonts;
        self.status_message = None;
        self.pending_overwrite = false;
        self.pending_action = None;
        self.is_open = true;
        self.mark_saved();
    }

    pub(super) fn apply_pending_load(&mut self, load: PendingThemeLoad) {
        self.load_with_fonts(&load.theme, &load.name, &load.id, load.fonts);
    }

    /// A live copy of the theme built from the editor's current values.
    ///
    /// Only the tokens the editor exposes are replaced; optional tokens keep
    /// the values of `base`, so the preview is complete without duplicating
    /// every field in the editor state.
    pub fn preview_theme(&self, base: &Theme) -> Theme {
        let mut theme = *base;
        theme.is_light = self.is_light;

        macro_rules! apply_editor_color {
            ($($field:ident),* $(,)?) => {
                $(
                    if let Some(color) = self.colors.get(stringify!($field)) {
                        theme.$field = *color;
                    }
                )*
            };
        }
        for_each_required_color_token!(apply_editor_color);

        let radius = |key: &str, fallback: u8| self.radii.get(key).copied().unwrap_or(fallback);
        theme.radius_xs = radius("radius_xs", theme.radius_xs);
        theme.radius_sm = radius("radius_sm", theme.radius_sm);
        theme.radius_md = radius("radius_md", theme.radius_md);
        theme.radius_lg = radius("radius_lg", theme.radius_lg);
        theme.radius_xl = radius("radius_xl", theme.radius_xl);
        theme.radius_pill = radius("radius_pill", theme.radius_pill);
        theme.play_button_shape = self.play_button_shape;
        theme
    }

    /// Automatically adjust all text tokens so they satisfy WCAG 2.1 AA against host surfaces.
    pub fn auto_fix_contrast(&mut self) {
        let bg_app = self.colors.get("bg_app").copied().unwrap_or(Color32::BLACK);
        let bg_card = self
            .colors
            .get("bg_card")
            .copied()
            .unwrap_or(Color32::BLACK);
        let bg_input = self
            .colors
            .get("bg_input")
            .copied()
            .unwrap_or(Color32::BLACK);

        let text_tokens = [
            "text_primary",
            "text_secondary",
            "text_muted",
            "text_dim",
            "accent_highlight",
            "accent_warm",
        ];

        for token in text_tokens {
            if let Some(mut fg) = self.colors.get(token).copied() {
                fg = auto_adjust_contrast(fg, bg_app, 4.5);
                fg = auto_adjust_contrast(fg, bg_card, 4.5);
                self.colors.insert(token.to_string(), fg);
            }
        }

        if let Some(border_input) = self.colors.get("border_input").copied() {
            let adjusted = auto_adjust_contrast(border_input, bg_input, 3.0);
            self.colors.insert("border_input".to_string(), adjusted);
        }

        self.status_message = Some((
            "All contrast values successfully auto-adjusted!".into(),
            false,
        ));
    }
}

pub(super) enum SaveOutcome {
    Saved(String),
    NeedsOverwrite,
    Error(String),
}

fn unique_theme_id(manager: &ThemeManager, base: &str) -> String {
    let base = base.trim();
    let base = if base.is_empty() {
        "my_custom_theme"
    } else {
        base
    };
    let stem = match base.rsplit_once("_copy") {
        Some((stem, suffix))
            if !stem.is_empty()
                && (suffix.is_empty() || suffix.chars().all(|c| c.is_ascii_digit())) =>
        {
            stem
        }
        _ => base,
    };
    let mut attempt = 1u32;
    loop {
        let candidate = if attempt == 1 {
            format!("{stem}_copy")
        } else {
            format!("{stem}_copy{attempt}")
        };
        let file_exists = manager
            .custom_themes_dir
            .join(format!("{candidate}.shadowtheme"))
            .exists();
        let id_exists = manager
            .all_theme_cards()
            .iter()
            .any(|card| card.id == candidate);
        if !file_exists && !id_exists {
            return candidate;
        }
        attempt += 1;
    }
}

pub(super) fn save_current_theme(
    state: &mut ThemeEditorState,
    manager: &ThemeManager,
    force_overwrite: bool,
    save_as_new: bool,
) -> SaveOutcome {
    let id = state.id.trim().to_string();
    let name = state.name.trim().to_string();
    if id.is_empty() || name.is_empty() {
        return SaveOutcome::Error("Name and file ID are required".to_string());
    }
    if let Err(e) = validate_contrast(&state.colors) {
        return SaveOutcome::Error(format!("Cannot save: {e}"));
    }

    if save_as_new {
        state.id = unique_theme_id(manager, &id);
        state.name = format!("{name} Copy");
    }

    let filename = format!("{}.shadowtheme", state.id.trim());
    let _ = std::fs::create_dir_all(&manager.custom_themes_dir);
    let target_path = manager.custom_themes_dir.join(&filename);
    if target_path.exists() && !force_overwrite {
        return SaveOutcome::NeedsOverwrite;
    }

    let hex_colors: HashMap<String, String> = state
        .colors
        .iter()
        .map(|(k, v)| (k.clone(), format_hex_color(*v)))
        .collect();

    let theme_file = ThemeFile {
        schema_version: 2,
        metadata: Some(crate::theme::schema::ThemeMetadata {
            id: state.id.trim().to_string(),
            name: state.name.trim().to_string(),
            author: state.author.trim().to_string(),
            description: state.description.trim().to_string(),
            version: "2.0.0".to_string(),
            is_light: state.is_light,
            tags: vec!["custom".to_string()],
        }),
        id: Some(state.id.trim().to_string()),
        name: Some(state.name.trim().to_string()),
        author: Some(state.author.trim().to_string()),
        description: Some(state.description.trim().to_string()),
        is_light: Some(state.is_light),
        colors: hex_colors,
        radii: state.radii.clone(),
        play_button: Some(state.play_button_shape),
        fonts: state.fonts.clone(),
    };

    match serde_json::to_string_pretty(&theme_file) {
        Ok(json) => match std::fs::write(&target_path, json) {
            Ok(_) => {
                state.status_message = Some((
                    format!("Saved successfully to {}", target_path.display()),
                    false,
                ));
                state.mark_saved();
                SaveOutcome::Saved(state.id.trim().to_string())
            }
            Err(e) => SaveOutcome::Error(format!("Write error: {e}")),
        },
        Err(e) => SaveOutcome::Error(format!("JSON error: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_tracks_edits_against_the_saved_snapshot() {
        let mut state = ThemeEditorState::default();
        assert!(!state.is_dirty(), "a freshly populated editor is clean");

        state.name = "A Different Name".into();
        assert!(state.is_dirty());
        state.revert_to_saved();
        assert_eq!(state.name, "My Custom Theme");
        assert!(!state.is_dirty());

        state.load_for_customizing(&Theme::light(), "Studio", "system:light");
        assert!(!state.is_dirty());
        state.is_light = false;
        assert!(state.is_dirty());
    }

    #[test]
    fn save_as_new_never_overwrites_an_existing_theme() {
        let temp_dir = std::env::temp_dir().join("shadowplayer_editor_save_as_new");
        let custom_dir = temp_dir.join("custom-theme");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let manager = ThemeManager::new_with_custom_dir(&temp_dir, custom_dir.clone());
        let mut state = ThemeEditorState::default();
        state.load_for_customizing(&Theme::dark(), "Demo", "system:dark");

        let first = match save_current_theme(&mut state, &manager, true, true) {
            SaveOutcome::Saved(id) => id,
            _ => panic!("first save-as-new should succeed"),
        };
        assert!(custom_dir.join(format!("{first}.shadowtheme")).exists());
        assert!(!state.is_dirty());

        let second = match save_current_theme(&mut state, &manager, true, true) {
            SaveOutcome::Saved(id) => id,
            _ => panic!("second save-as-new should succeed"),
        };
        assert_ne!(first, second);
        assert!(custom_dir.join(format!("{second}.shadowtheme")).exists());
        assert_eq!(
            std::fs::read_dir(&custom_dir)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry
                    .path()
                    .extension()
                    .is_some_and(|ext| ext == "shadowtheme"))
                .count(),
            2,
            "save-as-new must create a new file instead of replacing the first"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
