use super::super::*;

impl ShadowPlayerApp {
    pub(super) fn render_sidebar_panel(&mut self, ui: &mut egui::Ui) {
        // 8. Render Left Sidebar
        let window_w = ui.ctx().content_rect().width();
        let sidebar_rail = window_w < 720.0;
        let sidebar_w = if sidebar_rail {
            60.0
        } else if window_w < 1100.0 {
            // Wide enough for "Help & Feedback" without clipping, narrow
            // enough to leave the table usable on a 900px tiled window.
            200.0
        } else {
            208.0
        };
        let mut nav_to_select = None;
        let mut create_playlist_req = false;
        let mut create_smart_playlist_req = false;
        let mut delete_playlist_id = None;
        let mut delete_smart_playlist_id = None;
        let mut edit_smart_playlist_id = None;
        render_sidebar(
            ui,
            self.app_state.active_nav,
            &self.library_state.playlists,
            &self.library_state.smart_playlists,
            &self.theme,
            sidebar_rail,
            sidebar_w,
            |nav| nav_to_select = Some(nav),
            || create_playlist_req = true,
            || create_smart_playlist_req = true,
            |id| delete_playlist_id = Some(id),
            |id| delete_smart_playlist_id = Some(id),
            |id| edit_smart_playlist_id = Some(id),
        );

        if let Some(nav) = nav_to_select {
            if nav != NavItem::Settings {
                self.crossfade_preview.stop();
            }
            self.app_state.active_nav = nav;
            self.library_state.selected_album = None;
            self.library_state.selected_artist = None;
            self.library_state.selected_genre = None;
            self.library_state.selected_folder = None;
            if nav != NavItem::Settings && nav != NavItem::Help {
                self.refresh_view();
            }
        }
        if create_playlist_req {
            self.app_state.open_playlist_editor = true;
        }
        if create_smart_playlist_req {
            self.smart_playlist_editor.open_new();
        }
        if let Some(id) = delete_playlist_id {
            if let Err(e) = self.playlist_repo.delete(&id) {
                log::warn!("Could not delete playlist: {e}");
            }
            if self.app_state.active_nav == NavItem::Playlist(id) {
                self.app_state.active_nav = NavItem::Home;
            }
            self.reload_library();
        }
        if let Some(id) = delete_smart_playlist_id {
            if let Err(e) = self.smart_playlist_repo.delete(&id) {
                log::warn!("Could not delete smart playlist: {e}");
            }
            if self.app_state.active_nav == NavItem::SmartPlaylist(id) {
                self.app_state.active_nav = NavItem::Home;
            }
            self.reload_library();
        }
        if let Some(id) = edit_smart_playlist_id {
            if let Some(sp) = self
                .library_state
                .smart_playlists
                .iter()
                .find(|s| s.id == id)
                .cloned()
            {
                self.smart_playlist_editor.open_existing(&sp);
            }
        }
    }
}
