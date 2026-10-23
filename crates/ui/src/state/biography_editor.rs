/// Inline biography editing for the Artist and Album detail panes.
///
/// Like the lyrics editor, the draft belongs to one entity at a time and is
/// discarded when the user navigates to a different artist or album. The key
/// is `(kind, entity_name)` because two albums can share a title.
#[derive(Debug, Default, Clone)]
pub struct BiographyEditorState {
    pub key: Option<(String, String)>,
    pub draft: String,
    pub error: Option<String>,
}

impl BiographyEditorState {
    pub fn is_editing(&self, kind: &str, name: &str) -> bool {
        self.key
            .as_ref()
            .is_some_and(|(k, n)| k == kind && n == name)
    }

    pub fn begin(&mut self, kind: &str, name: &str, text: &str) {
        self.key = Some((kind.to_string(), name.to_string()));
        self.draft = text.to_string();
        self.error = None;
    }

    /// Store a save failure so the pane can show it next to the buttons.
    pub fn fail(&mut self, message: String) {
        self.error = Some(message);
    }

    pub fn close(&mut self) {
        self.key = None;
        self.draft.clear();
        self.error = None;
    }
}
