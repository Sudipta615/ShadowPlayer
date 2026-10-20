use domain::models::Track;
use eframe::egui::{self, Color32, Vec2};

use crate::theme::Theme;

/// State for the \"Edit Song Info\" dialog. Holds a copy of the track being
/// edited plus string buffers for the free-form / numeric fields.
#[derive(Default)]
pub struct EditTrackState {
    pub is_open: bool,
    /// Whether the dialog was open last frame. Used to move the keyboard
    /// focus into the dialog exactly once, when it opens.
    was_open: bool,
    editing: Option<Track>,
    title: String,
    artist: String,
    album: String,
    genre: String,
    year: String,
    track_number: String,
    disc_number: String,
}

impl EditTrackState {
    /// Open the dialog seeded with the given track's current tags.
    pub fn open_with(&mut self, track: &Track) {
        self.is_open = true;
        self.editing = Some(track.clone());
        self.title = track.title.clone();
        self.artist = track.artist.clone();
        self.album = track.album.clone();
        self.genre = track.genre.clone().unwrap_or_default();
        self.year = track.year.map(|y| y.to_string()).unwrap_or_default();
        self.track_number = track
            .track_number
            .map(|n| n.to_string())
            .unwrap_or_default();
        self.disc_number = track.disc_number.map(|n| n.to_string()).unwrap_or_default();
    }
}

#[derive(Default)]
pub struct EditTrackActions {
    /// The edited track to persist.
    pub save: Option<Track>,
}

fn parse_opt_u32(s: &str) -> Option<u32> {
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        s.parse::<u32>().ok()
    }
}

/// Render the \"Edit Song Info\" dialog. Returns the edited track when the user
/// confirms.
pub fn show_edit_track_modal(
    ctx: &egui::Context,
    state: &mut EditTrackState,
    theme: &Theme,
) -> EditTrackActions {
    let mut actions = EditTrackActions::default();
    if !state.is_open {
        // Reset the "was open" flag, so the next time the dialog opens it
        // takes the keyboard focus again.
        state.was_open = false;
        return actions;
    }

    let mut is_open = state.is_open;
    let mut cancel = false;
    let mut save = false;

    // The fields need stable ids so the keyboard focus can be parked in the
    // first one when the dialog opens.
    let title_field_id = egui::Id::unique("edit_track_title");
    let artist_field_id = egui::Id::unique("edit_track_artist");
    let album_field_id = egui::Id::unique("edit_track_album");
    let genre_field_id = egui::Id::unique("edit_track_genre");
    let year_field_id = egui::Id::unique("edit_track_year");
    let track_number_field_id = egui::Id::unique("edit_track_track_number");
    let disc_number_field_id = egui::Id::unique("edit_track_disc_number");

    let closed =
        crate::dialog::show_modal(ctx, "edit_track", "Edit Song Info", 380.0, theme, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_md);

            let field = |ui: &mut egui::Ui, label: &str, buf: &mut String, id: egui::Id| {
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new(90.0, 22.0),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            ui.label(
                                egui::RichText::new(label)
                                    .font(crate::theme::font(12.0))
                                    .color(theme.text_secondary),
                            );
                        },
                    );
                    ui.add(
                        egui::TextEdit::singleline(buf)
                            .id(id)
                            .desired_width(ui.available_width()),
                    );
                });
            };

            field(ui, "Title", &mut state.title, title_field_id);
            field(ui, "Artist", &mut state.artist, artist_field_id);
            field(ui, "Album", &mut state.album, album_field_id);
            field(ui, "Genre", &mut state.genre, genre_field_id);
            field(ui, "Year", &mut state.year, year_field_id);
            field(
                ui,
                "Track #",
                &mut state.track_number,
                track_number_field_id,
            );
            field(ui, "Disc #", &mut state.disc_number, disc_number_field_id);

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let save_btn = egui::Button::new(egui::RichText::new("Save").color(Color32::WHITE))
                    .fill(theme.accent);
                if ui.add(save_btn).clicked() {
                    save = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
    if closed {
        cancel = true;
    }

    // Move the focus into the dialog when it opens, so the user can start
    // typing straight away instead of hunting for the field.
    if is_open && !state.was_open {
        ctx.memory_mut(|m| m.request_focus(title_field_id));
    }
    state.was_open = is_open;

    if save {
        if let Some(mut track) = state.editing.take() {
            track.title = std::mem::take(&mut state.title);
            track.artist = std::mem::take(&mut state.artist);
            track.album = std::mem::take(&mut state.album);
            track.genre = {
                let g = std::mem::take(&mut state.genre);
                if g.trim().is_empty() {
                    None
                } else {
                    Some(g)
                }
            };
            track.year = parse_opt_u32(&state.year);
            track.track_number = parse_opt_u32(&state.track_number);
            track.disc_number = parse_opt_u32(&state.disc_number);
            actions.save = Some(track);
        }
        is_open = false;
    } else if cancel {
        state.editing = None;
        is_open = false;
    }

    state.is_open = is_open;
    actions
}
