use domain::ids::TrackId;

/// Inline lyrics editing state for the right-hand panel.
///
/// The draft is kept here rather than in the panel because the panel is
/// rebuilt every frame, and because a track change must discard a draft that
/// belongs to the previous song.
#[derive(Debug, Default, Clone)]
pub struct LyricsEditorState {
    pub editing: bool,
    pub draft: String,
    /// The track the draft belongs to. `None` while the editor is closed.
    pub track_id: Option<TrackId>,
    /// A save failed; shown under the editor until the next attempt.
    pub error: Option<String>,
}

impl LyricsEditorState {
    /// Open the editor for `track_id`, seeding it with the current lyrics.
    pub fn begin(&mut self, track_id: TrackId, text: String) {
        self.editing = true;
        self.track_id = Some(track_id);
        self.draft = text;
        self.error = None;
    }

    /// Close the editor. Called when the current track changes, so a draft
    /// cannot be saved onto the wrong file.
    pub fn close(&mut self) {
        self.editing = false;
        self.track_id = None;
        self.draft.clear();
        self.error = None;
    }
}
