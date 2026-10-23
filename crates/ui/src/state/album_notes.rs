//! Editor state for Markdown album liner notes.
//!
//! Notes are stored per album identity, not per track path, so a rescan or a
//! retag does not orphan them. The editor keeps the last saved text alongside
//! the draft so Cancel can restore it and the view can tell whether there is
//! anything to save.

use database::AlbumNotesRepo;
use domain::ids::AlbumId;

use crate::components::sidebar::NavItem;

/// The liner-notes editor for the album currently open in the detail view.
#[derive(Default)]
pub struct AlbumNotesState {
    /// Which album the loaded/draft text belongs to.
    pub album_id: Option<AlbumId>,
    pub title: String,
    pub artist: String,
    /// The text in the editor.
    pub draft: String,
    /// The text as last loaded from or written to the database.
    pub saved: String,
    pub editing: bool,
    pub error: Option<String>,
    pub just_saved: bool,
    /// Set after a failed load, so the view can offer a retry rather than
    /// silently showing an empty editor over a note that does exist.
    pub load_failed: bool,
}

impl AlbumNotesState {
    /// Load the note for an album the first time it is shown.
    ///
    /// Called every frame by the album detail view. A different album always
    /// reloads; the same album never overwrites a draft the user is typing in.
    pub fn sync(&mut self, album_id: AlbumId, title: &str, artist: &str, repo: &AlbumNotesRepo) {
        if self.album_id == Some(album_id) {
            return;
        }
        self.album_id = Some(album_id);
        self.title = title.to_string();
        self.artist = artist.to_string();
        self.editing = false;
        self.error = None;
        self.just_saved = false;
        self.load_failed = false;
        match repo.get(&album_id) {
            Ok(Some(note)) => {
                self.draft = note.markdown.clone();
                self.saved = note.markdown;
            }
            Ok(None) => {
                self.draft.clear();
                self.saved.clear();
            }
            Err(e) => {
                self.draft.clear();
                self.saved.clear();
                self.load_failed = true;
                self.error = Some(format!("Could not load liner notes: {e}"));
            }
        }
    }

    pub fn begin_edit(&mut self) {
        self.draft = self.saved.clone();
        self.editing = true;
        self.error = None;
        self.just_saved = false;
    }

    pub fn cancel(&mut self) {
        self.draft = self.saved.clone();
        self.editing = false;
        self.error = None;
    }

    pub fn is_dirty(&self) -> bool {
        self.draft != self.saved
    }

    /// Validate and persist the draft.
    ///
    /// Empty Markdown deletes the note through the repository, which is how the
    /// album view distinguishes "never written" from "cleared".
    pub fn save(&mut self, repo: &AlbumNotesRepo) -> bool {
        let Some(album_id) = self.album_id else {
            return false;
        };
        if let Err(e) = media_tools::markdown::validate_notes(&self.draft) {
            self.error = Some(e.to_string());
            return false;
        }
        match repo.save(&album_id, &self.title, &self.artist, &self.draft) {
            Ok(()) => {
                self.saved = self.draft.clone();
                self.editing = false;
                self.error = None;
                self.just_saved = true;
                true
            }
            Err(e) => {
                self.error = Some(format!("Could not save liner notes: {e}"));
                false
            }
        }
    }

    /// Forget the loaded album, e.g. when leaving the Albums view.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// `true` when the current album's notes exist and are not being edited.
    pub fn has_saved_notes(&self) -> bool {
        !self.saved.trim().is_empty()
    }

    /// Navigate away only after dropping a draft that would otherwise follow
    /// the user to a different album.
    pub fn leaving(&mut self, nav: NavItem) -> bool {
        if nav == NavItem::Albums {
            false
        } else {
            if self.editing || self.is_dirty() {
                self.cancel();
            }
            self.clear();
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_restores_the_saved_text() {
        let mut state = AlbumNotesState {
            saved: "saved".into(),
            ..AlbumNotesState::default()
        };
        state.begin_edit();
        state.draft = "changed".into();
        assert!(state.is_dirty());
        state.cancel();
        assert_eq!(state.draft, "saved");
        assert!(!state.editing);
    }

    #[test]
    fn empty_notes_are_not_reported_as_present() {
        let state = AlbumNotesState {
            saved: "  \n".into(),
            ..AlbumNotesState::default()
        };
        assert!(!state.has_saved_notes());
    }
}
