use eframe::egui::{self, Margin};
use domain::models::Playlist;

use crate::components::sidebar::{NavItem, SideBar};
use crate::theme::Theme;

/// Render application left sidebar panel
pub fn render_sidebar(
    ui: &mut egui::Ui,
    active_nav: NavItem,
    playlists: &[Playlist],
    theme: &Theme,
    on_select_nav: impl FnOnce(NavItem),
    on_create_playlist: impl FnOnce(),
) {
    egui::Panel::left("left_sidebar")
        .exact_size(192.0)
        .resizable(false)
        .frame(
            egui::Frame::NONE
                .fill(theme.bg_sidebar)
                .inner_margin(Margin {
                    left: 16,
                    right: 10,
                    top: 14,
                    bottom: 14,
                }),
        )
        .show(ui, |ui| {
            let mut nav_to_select = None;
            let mut create_playlist_req = false;

            SideBar::new(
                active_nav,
                playlists,
                theme,
                |nav| nav_to_select = Some(nav),
                || create_playlist_req = true,
            )
            .show(ui);

            if let Some(nav) = nav_to_select {
                on_select_nav(nav);
            }
            if create_playlist_req {
                on_create_playlist();
            }
        });
}
