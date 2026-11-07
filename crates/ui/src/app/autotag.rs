use super::*;

impl ShadowPlayerApp {
    pub(super) fn render_autotag_modals(&mut self, ctx: &egui::Context) {
        // The MusicBrainz / AcoustID Auto-Tag modal.
        let autotag_actions = show_autotag_modal(ctx, &mut self.autotag_modal, &self.theme);
        if autotag_actions.cancelled {
            self.autotag_service.cancel();
            self.autotag_modal.close();
        }
        if !autotag_actions.apply.is_empty() {
            // Tag files are written on the worker, not while the frame is
            // being drawn. The summary toast is raised when the batch lands.
            let mut writes = Vec::new();
            for (review, m) in autotag_actions.apply {
                let track_id = review.result.track_id;
                if let Ok(Some(mut track)) = self.track_repo.get_by_id(&track_id) {
                    if review.fields.title {
                        track.title = m.title.clone();
                    }
                    if review.fields.artist {
                        track.artist = m.artist.clone();
                    }
                    if review.fields.album {
                        track.album = m.album.clone();
                    }
                    if review.fields.year {
                        track.year = m.year;
                    }
                    if review.fields.track_number {
                        track.track_number = m.track_number;
                    }
                    if review.fields.disc_number {
                        track.disc_number = m.disc_number;
                    }
                    if review.fields.genre && m.genre.is_some() {
                        track.genre = m.genre.clone();
                    }
                    let _ = self.track_repo.upsert(&track);

                    let editorial = audio_session::engine::decode::EditorialTags {
                        title: if review.fields.title {
                            Some(m.title)
                        } else {
                            None
                        },
                        artist: if review.fields.artist {
                            Some(m.artist)
                        } else {
                            None
                        },
                        album: if review.fields.album {
                            Some(m.album)
                        } else {
                            None
                        },
                        genre: if review.fields.genre { m.genre } else { None },
                        year: if review.fields.year { m.year } else { None },
                        track_number: if review.fields.track_number {
                            m.track_number
                        } else {
                            None
                        },
                        disc_number: if review.fields.disc_number {
                            m.disc_number
                        } else {
                            None
                        },
                    };
                    writes.push((track.path.clone(), editorial));
                }
            }
            self.reload_library();
            self.autotag_modal.close();
            let count = writes.len();
            if count == 0
                || self.file_tasks.submit(FileTask::WriteTagsBatch {
                    items: writes,
                    success_message: tr_args!(
                        "Tags applied to {count} track(s)",
                        "count" => count
                    ),
                })
            {
                if count > 0 {
                    self.toasts.info(tr!("Applying tags…"));
                }
            } else {
                self.toasts
                    .warning(tr!("Could not start the background task"));
            }
        }

        // The Bulk Cover Art Fetch modal.
        let artwork_actions =
            show_artwork_fetch_modal(ctx, &mut self.artwork_fetch_modal, &self.theme);
        if artwork_actions.start {
            let missing_tracks: Vec<domain::models::Track> = self
                .library_state
                .all_tracks
                .iter()
                .filter(|t| !self.artwork_service.has_artwork(&t.id, &t.path))
                .cloned()
                .collect();
            self.artwork_fetch_service
                .start(missing_tracks, self.artwork_service.manager());
        }
        if artwork_actions.cancel {
            self.artwork_fetch_service.cancel();
        }
    }
}
