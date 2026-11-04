/// The singular noun for one, the plural otherwise.
///
/// Appending an "s" is not universal — "1 Genre" is right but "1 Series"
/// would not be — so the two forms are chosen explicitly.
pub fn noun<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 {
        singular
    } else {
        plural
    }
}

/// A count with its noun: "1 song", "2 songs".
pub fn count_label(count: usize, singular: &str, plural: &str) -> String {
    format!("{count} {}", noun(count, singular, plural))
}

#[cfg(test)]
mod tests {
    use super::{count_label, noun};

    #[test]
    fn counts_are_singular_only_at_one() {
        assert_eq!(noun(0, "song", "songs"), "songs");
        assert_eq!(noun(1, "song", "songs"), "song");
        assert_eq!(noun(2, "song", "songs"), "songs");
        assert_eq!(count_label(1, "genre", "genres"), "1 genre");
        assert_eq!(count_label(24, "genre", "genres"), "24 genres");
        assert_eq!(count_label(1, "day", "days"), "1 day");
    }
}

pub mod albums_panel;
pub mod artists_panel;
pub mod backend_options;
pub mod biography_pane;
pub mod clap_rack_panel;
pub mod command_palette;
pub mod eq_panel;
pub mod folders_panel;
pub mod fx_panel;
pub mod genres_panel;
pub mod icons;
pub mod info_box;
pub mod mini_player;
pub mod now_playing;
pub mod now_playing_full;
pub mod output_facts;
pub mod page;
pub mod play_button;
pub mod podcasts_view;
pub mod right_panel;
pub mod search_field;
pub mod settings_tab;
pub mod sidebar;
pub mod songs_table;
pub mod text;
pub mod toasts;
pub mod topbar;
pub mod track_properties_modal;
pub mod visualizer;
pub mod waveform_seek;
