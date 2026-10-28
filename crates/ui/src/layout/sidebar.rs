use domain::models::Playlist;
use domain::smart::SmartPlaylist;
use eframe::egui::{self, Margin};

use crate::components::sidebar::{NavItem, SideBar};
use crate::theme::Theme;

/// Render application left sidebar panel.
///
/// `rail` switches to icon-only navigation for narrow windows. `width` is the
/// outer panel width chosen by the responsive shell (208 full, 200 compact,
/// 60 rail).
#[allow(clippy::too_many_arguments)]
pub fn render_sidebar(
    ui: &mut egui::Ui,
    active_nav: NavItem,
    playlists: &[Playlist],
    smart_playlists: &[SmartPlaylist],
    theme: &Theme,
    rail: bool,
    width: f32,
    on_select_nav: impl FnOnce(NavItem),
    on_create_playlist: impl FnOnce(),
    on_create_smart_playlist: impl FnOnce(),
    on_delete_playlist: impl FnOnce(domain::ids::PlaylistId),
    on_delete_smart_playlist: impl FnOnce(domain::ids::PlaylistId),
    on_edit_smart_playlist: impl FnOnce(domain::ids::PlaylistId),
) {
    let margin_x = if rail { 10 } else { 12 };
    egui::Panel::left("left_sidebar")
        .exact_size(width)
        .resizable(false)
        .frame(
            egui::Frame::NONE
                .fill(theme.bg_sidebar)
                .inner_margin(Margin {
                    left: margin_x,
                    right: margin_x,
                    top: 14,
                    bottom: 12,
                }),
        )
        .show(ui, |ui| {
            let mut nav_to_select = None;
            let mut create_playlist_req = false;
            let mut create_smart_req = false;
            let mut delete_playlist_id = None;
            let mut delete_smart_id = None;
            let mut edit_smart_id = None;

            {
                let mut bar = SideBar::new(
                    active_nav,
                    playlists,
                    smart_playlists,
                    theme,
                    rail,
                    |nav| nav_to_select = Some(nav),
                    || create_playlist_req = true,
                    || create_smart_req = true,
                    |id| delete_playlist_id = Some(id),
                    |id| delete_smart_id = Some(id),
                    |id| edit_smart_id = Some(id),
                );

                // Pin Settings and Help to the bottom of the sidebar. They live
                // outside the scroll area, so a long playlist cannot push them
                // off-screen.
                egui::Panel::bottom("sidebar_nav_footer")
                    .resizable(false)
                    .frame(
                        egui::Frame::NONE
                            .fill(theme.bg_sidebar)
                            .inner_margin(Margin {
                                left: 0,
                                right: 0,
                                top: 8,
                                bottom: 0,
                            }),
                    )
                    .show(ui, |ui| {
                        ui.separator();
                        ui.add_space(4.0);
                        bar.show_footer(ui);
                    });

                egui::ScrollArea::vertical()
                    .id_salt("sidebar_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        bar.show(ui);
                    });
            }

            if let Some(nav) = nav_to_select {
                on_select_nav(nav);
            }
            if create_playlist_req {
                on_create_playlist();
            }
            if create_smart_req {
                on_create_smart_playlist();
            }
            if let Some(id) = delete_playlist_id {
                on_delete_playlist(id);
            }
            if let Some(id) = delete_smart_id {
                on_delete_smart_playlist(id);
            }
            if let Some(id) = edit_smart_id {
                on_edit_smart_playlist(id);
            }
        });
}
