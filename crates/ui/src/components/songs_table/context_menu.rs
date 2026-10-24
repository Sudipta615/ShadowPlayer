//! The track context menu, shared by the row overflow button and right-click.

use super::SongsTableAction;
use crate::theme::Theme;
use domain::models::{Playlist, Track};
use eframe::egui::{self, Ui};

pub(super) fn render_track_context_menu(
    ui: &mut Ui,
    track: &Track,
    is_fav: bool,
    playlists: &[Playlist],
    action: &mut SongsTableAction,
    theme: &Theme,
) {
    if ui.button(tr!(Msg::SCD25432E0F0BFFFB)).clicked() {
        action.play_track = Some(track.id);
        ui.close();
    }
    if ui.button(tr!(Msg::S4B721643601DB784)).clicked() {
        action.queue_next_track = Some(track.id);
        ui.close();
    }
    if ui.button(tr!(Msg::S937116D0ED2A3AC6)).clicked() {
        action.add_to_queue = Some(track.id);
        ui.close();
    }
    if ui.button(tr!("Start Radio")).clicked() {
        action.start_radio = Some(track.id);
        ui.close();
    }

    ui.separator();

    let fav_text = if is_fav {
        "Remove from Favorites"
    } else {
        "Add to Favorites"
    };
    if ui.button(fav_text).clicked() {
        action.toggle_favorite = Some(track.id);
        ui.close();
    }

    ui.menu_button(tr!(Msg::S36E1BFD2D899A7B0), |ui| {
        for stars in 1..=5u8 {
            let label = match stars {
                1 => "1 star".to_string(),
                _ => format!("{stars} stars"),
            };
            if ui.selectable_label(track.rating == stars, label).clicked() {
                action.set_rating = Some((track.id, stars));
                ui.close();
            }
        }
        if track.rating > 0 && ui.button(tr!(Msg::SDA396F7DB3F6E8D7)).clicked() {
            action.set_rating = Some((track.id, 0));
            ui.close();
        }
    });

    if !playlists.is_empty() {
        ui.menu_button(tr!(Msg::SDA44F97ADDA826F9), |ui| {
            for pl in playlists {
                if ui.button(&pl.name).clicked() {
                    action.add_to_playlist = Some((track.id, pl.id));
                    ui.close();
                }
            }
        });
    }

    ui.separator();

    if ui.button(tr!(Msg::S8058D56146093AD1)).clicked() {
        if let Some(parent) = track.path.parent() {
            action.open_folder = Some(parent.to_path_buf());
        }
        ui.close();
    }

    if ui.button(tr!(Msg::SE2868E427E90B359)).clicked() {
        action.show_properties = Some(track.id);
        ui.close();
    }

    if ui.button(tr!(Msg::SDE57BAF260C1358B)).clicked() {
        action.open_dsp = Some(track.id);
        ui.close();
    }

    if ui.button(tr!(Msg::S5851EE93B1C93A0E)).clicked() {
        action.edit_track = Some(track.id);
        ui.close();
    }

    if ui.button("Auto-Tag with MusicBrainz…").clicked() {
        action.autotag_track = Some(track.id);
        ui.close();
    }

    ui.separator();

    if ui
        .button(tr!(Msg::S6DA3E203E02A71DB))
        .on_hover_text(tr!("Restore the default Title / Artist / Album widths"))
        .clicked()
    {
        action.reset_columns = true;
        ui.close();
    }

    ui.separator();

    if ui
        .button(egui::RichText::new(tr!("Remove from Library")).color(theme.destructive))
        .clicked()
    {
        action.remove_track = Some(track.id);
        ui.close();
    }
}
