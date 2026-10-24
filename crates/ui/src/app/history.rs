use super::*;

impl ShadowPlayerApp {
    pub(super) fn render_history_modals(&mut self, ctx: &egui::Context) {
        // The scrobble log.
        if self.app_state.scrobble_log_open {
            let log_actions = crate::modals::scrobble_log_modal::show_scrobble_log_modal(
                ctx,
                &self.scrobble_repo,
                &self.theme,
            );
            if log_actions.closed {
                self.app_state.scrobble_log_open = false;
            }
            if log_actions.open_listening {
                self.app_state.scrobble_log_open = false;
                self.app_state.listening_modal.open();
            }
        }

        // The listening history: the local answer to a scrobbling service.
        {
            let listening_actions = crate::modals::listening_modal::show_listening_modal(
                ctx,
                &mut self.app_state.listening_modal,
                &self.scrobble_repo,
                &self.theme,
            );
            if listening_actions.close {
                self.app_state.listening_modal.is_open = false;
            }
            if let Some(format) = listening_actions.export {
                match crate::modals::listening_modal::export_with_dialog(
                    &self.scrobble_repo,
                    format,
                ) {
                    Ok(Some(path)) => self.toasts.success(format!(
                        "Exported the listening history to {}",
                        path.display()
                    )),
                    Ok(None) => {}
                    Err(e) => self.toasts.warning(e),
                }
            }
        }

        // Importing the playlists other players left behind.
        {
            let import_actions = crate::modals::import_playlists_modal::show_import_playlists_modal(
                ctx,
                &mut self.app_state.import_playlists_modal,
                &self.library_state.all_tracks,
                &self.theme,
            );
            if !import_actions.selected.is_empty() {
                let selected = import_actions.selected.clone();
                let found = self.app_state.import_playlists_modal.found.clone();
                let tracks = self.library_state.all_tracks.clone();
                let mut imported = 0;
                for index in selected {
                    let Some(playlist) = found.get(index) else {
                        continue;
                    };
                    let matched =
                        crate::modals::import_playlists_modal::matched_tracks(playlist, &tracks);
                    if matched.is_empty() {
                        continue;
                    }
                    let ids: Vec<domain::ids::TrackId> = matched.iter().map(|t| t.id).collect();
                    match self.playlist_repo.create(&playlist.name, Some("Imported")) {
                        Ok(created) => {
                            let mut added = true;
                            for id in &ids {
                                if self.playlist_repo.add_track(&created.id, id).is_err() {
                                    added = false;
                                    break;
                                }
                            }
                            if added {
                                imported += 1;
                            }
                        }
                        Err(e) => {
                            self.toasts
                                .warning(format!("Could not import a playlist: {e}"));
                        }
                    }
                }
                if imported > 0 {
                    self.toasts.success(format!(
                        "Imported {imported} playlist{}",
                        if imported == 1 { "" } else { "s" }
                    ));
                }
                self.app_state.import_playlists_modal.is_open = false;
            }
        }
    }
}
