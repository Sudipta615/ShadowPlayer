//! The "Edit Song Info" dialog.
//!
//! Fields are grouped the way a person thinks about a track — identity first,
//! then numbering — instead of one seven-row column that put Year next to
//! Album. The file-tag decision is its own section because it is the one
//! control here that changes something outside the database.

use domain::models::Track;
use eframe::egui::{self, CornerRadius, Margin, Stroke, Vec2};

use crate::theme::Theme;

/// State for the "Edit Song Info" dialog. Holds a copy of the track being
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
    /// Whether saving should also write the tags into the file.
    ///
    /// The file is the source of truth for a music library: an edit that only
    /// lives in the database is lost the moment the file moves or the library
    /// is rebuilt from disk.
    write_to_file: bool,
}

impl EditTrackState {
    /// Open the dialog seeded with the given track's current tags.
    pub fn open_with(&mut self, track: &Track) {
        self.is_open = true;
        // Writing the tags back is the default: the file is the source of
        // truth for a music library.
        self.write_to_file = true;
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
    /// The user asked for the changes to be written into the file's own tags
    /// as well as the library.
    pub write_to_file: bool,
}

fn parse_opt_u32(s: &str) -> Option<u32> {
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        s.parse::<u32>().ok()
    }
}

/// Render the "Edit Song Info" dialog. Returns the edited track when the user
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

    let screen = crate::dialog::viewport_rect(ctx);
    let width = 460.0_f32.min((screen.width() - 48.0).max(300.0));
    let dialog_title = tr!(Msg::S5851EE93B1C93A0E);

    let closed = crate::dialog::show_modal_scrollable(
        ctx,
        "edit_track",
        &dialog_title,
        width,
        theme,
        |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_md);
            ui.visuals_mut().widgets.inactive.weak_bg_fill = theme.bg_input;
            ui.visuals_mut().widgets.inactive.bg_fill = theme.bg_input;
            ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1.0, theme.border_input);
            ui.visuals_mut().widgets.inactive.corner_radius = CornerRadius::same(theme.radius_md);

            ui.label(
                egui::RichText::new(tr!(
                    "Changes are saved to the library. Writing the file's tags keeps the file and the library in step."
                ))
                .font(crate::theme::font(11.8))
                .color(theme.text_muted),
            );

            // ── Identity ──────────────────────────────────────────────
            section(ui, tr!("Song info"), theme);
            field_block(ui, tr!("Title"), &mut state.title, title_field_id, theme);
            field_block(ui, tr!("Artist"), &mut state.artist, artist_field_id, theme);
            field_block(ui, tr!("Album"), &mut state.album, album_field_id, theme);
            field_block(ui, tr!("Genre"), &mut state.genre, genre_field_id, theme);

            // ── Numbering ─────────────────────────────────────────────
            ui.add_space(theme.space_xs);
            section(ui, tr!("Numbering"), theme);
            let gap = theme.space_md;
            let col_w = ((ui.available_width() - gap * 2.0) / 3.0).max(72.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(gap, 0.0);
                number_field(
                    ui,
                    tr!("Year"),
                    &mut state.year,
                    year_field_id,
                    col_w,
                    theme,
                    4,
                );
                number_field(
                    ui,
                    tr!("Track #"),
                    &mut state.track_number,
                    track_number_field_id,
                    col_w,
                    theme,
                    3,
                );
                number_field(
                    ui,
                    tr!("Disc #"),
                    &mut state.disc_number,
                    disc_number_field_id,
                    col_w,
                    theme,
                    3,
                );
            });

            // ── File tags ─────────────────────────────────────────────
            ui.add_space(theme.space_xs);
            egui::Frame::NONE
                .fill(theme.bg_app)
                .stroke(Stroke::new(1.0, theme.border_card))
                .corner_radius(CornerRadius::same(theme.radius_lg))
                .inner_margin(Margin::symmetric(
                    theme.space_xl as i8,
                    theme.space_md as i8,
                ))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
                        ui.checkbox(&mut state.write_to_file, "");
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(0.0, 1.0);
                            ui.label(
                                egui::RichText::new(tr!("Write the tags into the file too"))
                                    .font(crate::theme::font(12.0))
                                    .color(theme.text_primary),
                            );
                            ui.label(
                                egui::RichText::new(tr!("— the file is the source of truth"))
                                    .font(crate::theme::font(11.0))
                                    .color(theme.text_muted),
                            );
                        });
                    });
                });

            ui.add_space(theme.space_xs);
            ui.separator();
            ui.add_space(theme.space_xs);

            // ── Footer ────────────────────────────────────────────────
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(tr!(Msg::S2CEB11BE2290BB1B))
                                .font(crate::theme::font(12.0))
                                .color(theme.text_secondary),
                        )
                        .fill(theme.bg_pill)
                        .stroke(Stroke::new(1.0, theme.border_subtle))
                        .corner_radius(CornerRadius::same(theme.radius_md))
                        .min_size(Vec2::new(84.0, 30.0)),
                    )
                    .clicked()
                {
                    cancel = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(tr!("Save"))
                                    .font(crate::theme::font(12.0))
                                    .strong()
                                    .color(crate::theme::on_fill(theme.accent)),
                            )
                            .fill(theme.accent)
                            .corner_radius(CornerRadius::same(theme.radius_md))
                            .min_size(Vec2::new(96.0, 30.0)),
                        )
                        .clicked()
                    {
                        save = true;
                    }
                });
            });
        },
    );
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
            actions.write_to_file = state.write_to_file;
        }
        is_open = false;
    } else if cancel {
        state.editing = None;
        is_open = false;
    }

    state.is_open = is_open;
    actions
}

/// A field label above its input, which keeps a narrow dialog from wasting a
/// third of its width on a label column.
fn field_block(ui: &mut egui::Ui, label: String, buf: &mut String, id: egui::Id, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xxs);
            ui.label(
                egui::RichText::new(label)
                    .font(crate::theme::font(11.5))
                    .color(theme.text_muted),
            );
            ui.add(
                egui::TextEdit::singleline(buf)
                    .id(id)
                    .desired_width(ui.available_width()),
            );
        });
    });
}

/// A compact numeric field with its label above it.
fn number_field(
    ui: &mut egui::Ui,
    label: String,
    buf: &mut String,
    id: egui::Id,
    width: f32,
    theme: &Theme,
    digits: usize,
) {
    ui.allocate_ui_with_layout(
        Vec2::new(width, 44.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xxs);
            ui.label(
                egui::RichText::new(label)
                    .font(crate::theme::font(11.5))
                    .color(theme.text_muted),
            );
            ui.add(
                egui::TextEdit::singleline(buf)
                    .id(id)
                    .char_limit(digits)
                    .desired_width(width),
            );
        },
    );
}

fn section(ui: &mut egui::Ui, text: String, theme: &Theme) {
    ui.label(
        egui::RichText::new(text)
            .font(crate::theme::font(12.5))
            .strong()
            .color(theme.text_primary),
    );
}
