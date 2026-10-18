use eframe::egui::{self, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Ui, Vec2};
use domain::ids::PlaylistId;
use domain::models::Playlist;
use domain::smart::SmartPlaylist;
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavItem {
    Home,
    Albums,
    Artists,
    Genres,
    Folders,
    Favorites,
    RecentlyPlayed,
    MostPlayed,
    HiRes,
    Lossless,
    Playlist(PlaylistId),
    SmartPlaylist(PlaylistId),
    Settings,
    Help,
}

pub struct SideBar<'a> {
    pub active_item: NavItem,
    pub playlists: &'a [Playlist],
    pub smart_playlists: &'a [SmartPlaylist],
    pub theme: &'a Theme,
    pub on_select: Box<dyn FnMut(NavItem) + 'a>,
    pub on_create_playlist: Box<dyn FnMut() + 'a>,
    pub on_create_smart_playlist: Box<dyn FnMut() + 'a>,
}

impl<'a> SideBar<'a> {
    pub fn new(
        active_item: NavItem,
        playlists: &'a [Playlist],
        smart_playlists: &'a [SmartPlaylist],
        theme: &'a Theme,
        on_select: impl FnMut(NavItem) + 'a,
        on_create_playlist: impl FnMut() + 'a,
        on_create_smart_playlist: impl FnMut() + 'a,
    ) -> Self {
        Self {
            active_item,
            playlists,
            smart_playlists,
            theme,
            on_select: Box::new(on_select),
            on_create_playlist: Box::new(on_create_playlist),
            on_create_smart_playlist: Box::new(on_create_smart_playlist),
        }
    }

    pub fn show(mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.set_width(168.0);
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 4.0);

            // 1. Branding: Glowing Logo + Title + Subtitle
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(10.0, 0.0);

                let (logo_rect, _) = ui.allocate_exact_size(Vec2::new(30.0, 30.0), Sense::hover());
                crate::components::icons::draw_logo(ui, logo_rect, self.theme.accent_pink);

                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 1.0);
                    ui.label(
                        egui::RichText::new("ShadowPlayer")
                            .font(FontId::proportional(19.0))
                            .strong()
                            .color(self.theme.text_primary),
                    );
                    ui.label(
                        egui::RichText::new("Music for a better you")
                            .font(FontId::proportional(11.5))
                            .color(self.theme.text_muted),
                    );
                });
            });

            ui.add_space(16.0);

            // 2. Primary Navigation
            self.render_nav_item(ui, NavItem::Home, "Home");
            self.render_nav_item(ui, NavItem::Albums, "Albums");
            self.render_nav_item(ui, NavItem::Artists, "Artists");
            self.render_nav_item(ui, NavItem::Genres, "Genres");
            self.render_nav_item(ui, NavItem::Folders, "Folders");

            ui.add_space(16.0);

            // 3. Playlists Header with Circular "+" Button
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Playlists")
                        .font(FontId::proportional(13.0))
                        .color(self.theme.text_muted),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (plus_rect, plus_resp) = ui.allocate_exact_size(Vec2::new(22.0, 22.0), Sense::click());
                    let plus_color = if plus_resp.hovered() { self.theme.text_primary } else { self.theme.text_muted };
                    crate::components::icons::draw_plus_circle(ui, plus_rect, plus_color);
                    if plus_resp.clicked() {
                        (self.on_create_playlist)();
                    }
                });
            });

            ui.add_space(4.0);

            // 4. Playlist Special Items with Colored Badges (matching mockup)
            self.render_playlist_badge_item(
                ui,
                NavItem::Favorites,
                "Favorites",
                self.theme.accent_magenta,
            );
            self.render_playlist_badge_item(
                ui,
                NavItem::RecentlyPlayed,
                "Recently Played",
                self.theme.accent_violet,
            );
            self.render_playlist_badge_item(
                ui,
                NavItem::MostPlayed,
                "Most Played",
                self.theme.accent_orange,
            );

            // User Custom Playlists
            if !self.playlists.is_empty() {
                ui.add_space(6.0);
                for pl in self.playlists {
                    self.render_user_playlist_item(ui, pl);
                }
            }

            // Smart (rule-based) Playlists
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Smart Playlists")
                        .font(FontId::proportional(13.0))
                        .color(self.theme.text_muted),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (plus_rect, plus_resp) =
                        ui.allocate_exact_size(Vec2::new(22.0, 22.0), Sense::click());
                    let plus_color = if plus_resp.hovered() {
                        self.theme.text_primary
                    } else {
                        self.theme.text_muted
                    };
                    crate::components::icons::draw_plus_circle(ui, plus_rect, plus_color);
                    if plus_resp.clicked() {
                        (self.on_create_smart_playlist)();
                    }
                });
            });
            ui.add_space(4.0);

            if self.smart_playlists.is_empty() {
                ui.label(
                    egui::RichText::new("  e.g. FLAC · played > 5")
                        .font(FontId::proportional(11.0))
                        .color(self.theme.text_muted),
                );
            } else {
                for sp in self.smart_playlists {
                    self.render_smart_playlist_item(ui, sp);
                }
            }

            // 5. Anchor Settings & Help to the bottom
            let remaining_h = ui.available_height() - 94.0;
            if remaining_h > 0.0 {
                ui.add_space(remaining_h);
            }

            // Bottom items
            self.render_nav_item(ui, NavItem::Settings, "Settings");
            self.render_nav_item(ui, NavItem::Help, "Help & Feedback");
            ui.add_space(4.0);
        });
    }

    fn render_smart_playlist_item(&mut self, ui: &mut Ui, playlist: &SmartPlaylist) {
        let item = NavItem::SmartPlaylist(playlist.id);
        let is_active = self.active_item == item;
        let height = 34.0;
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());

        if response.clicked() {
            (self.on_select)(item);
        }

        let painter = ui.painter_at(rect);
        let is_hovered = response.hovered();

        if is_active {
            painter.rect_filled(rect, CornerRadius::same(7), self.theme.accent_blue);
        } else if is_hovered {
            painter.rect_filled(rect, CornerRadius::same(7), self.theme.bg_card_hover);
        }

        let text_color = if is_active || is_hovered {
            self.theme.text_primary
        } else {
            self.theme.text_secondary
        };

        // A filter glyph marks rule-based playlists.
        crate::components::icons::draw_filter(
            ui,
            Rect::from_center_size(Pos2::new(rect.min.x + 16.0, rect.center().y), Vec2::new(13.0, 13.0)),
            if is_active { Color32::WHITE } else { self.theme.accent_blue },
        );

        let label_pos = Pos2::new(rect.min.x + 30.0, rect.center().y);
        let galley = painter.layout_no_wrap(
            playlist.name.clone(),
            FontId::proportional(12.8),
            text_color,
        );
        painter.galley(
            Pos2::new(label_pos.x, rect.center().y - galley.size().y * 0.5),
            galley,
            text_color,
        );
    }

    fn render_user_playlist_item(&mut self, ui: &mut Ui, playlist: &Playlist) {
        let is_active = self.active_item == NavItem::Playlist(playlist.id);
        let height = 34.0;
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());

        if response.clicked() {
            (self.on_select)(NavItem::Playlist(playlist.id));
        }

        let painter = ui.painter_at(rect);
        let is_hovered = response.hovered();

        if is_active {
            painter.rect_filled(rect, CornerRadius::same(7), self.theme.accent_violet);
        } else if is_hovered {
            painter.rect_filled(rect, CornerRadius::same(7), self.theme.bg_card_hover);
        }

        let text_color = if is_active || is_hovered {
            self.theme.text_primary
        } else {
            self.theme.text_secondary
        };

        // Icon
        let pl_icon_rect = Rect::from_center_size(Pos2::new(rect.min.x + 16.0, rect.center().y), Vec2::new(14.0, 14.0));
        crate::components::icons::draw_list_icon(ui, pl_icon_rect, if is_active { Color32::WHITE } else { self.theme.accent_pink });

        // Playlist name
        let label_pos = Pos2::new(rect.min.x + 30.0, rect.center().y);
        let galley = painter.layout_no_wrap(
            playlist.name.clone(),
            FontId::proportional(12.8),
            text_color,
        );
        painter.galley(Pos2::new(label_pos.x, rect.center().y - galley.size().y * 0.5), galley, text_color);

        // Count badge
        painter.text(
            Pos2::new(rect.max.x - 8.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            &format!("{}", playlist.track_count),
            FontId::proportional(11.0),
            if is_active { Color32::WHITE } else { self.theme.text_muted },
        );
    }

    fn render_nav_item(&mut self, ui: &mut Ui, item: NavItem, label: &str) {
        let is_active = self.active_item == item;
        let height = 38.0;
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());

        if response.clicked() {
            (self.on_select)(item);
        }

        let painter = ui.painter_at(rect);
        let is_hovered = response.hovered();

        if is_active {
            painter.rect_filled(rect, CornerRadius::same(9), self.theme.accent_violet);
        } else if is_hovered {
            painter.rect_filled(rect, CornerRadius::same(9), self.theme.bg_card_hover);
        }

        let text_color = if is_active {
            self.theme.text_primary
        } else if is_hovered {
            self.theme.text_primary
        } else {
            self.theme.text_secondary
        };

        // Render Icon
        let icon_rect = Rect::from_center_size(Pos2::new(rect.min.x + 22.0, rect.center().y), Vec2::new(20.0, 20.0));
        match item {
            NavItem::Home => crate::components::icons::draw_home(ui, icon_rect, text_color),
            NavItem::Albums => crate::components::icons::draw_disc(ui, icon_rect, text_color),
            NavItem::Artists => crate::components::icons::draw_artist(ui, icon_rect, text_color),
            NavItem::Genres => crate::components::icons::draw_genres(ui, icon_rect, text_color),
            NavItem::Folders => crate::components::icons::draw_folder(ui, icon_rect, text_color),
            NavItem::Settings => crate::components::icons::draw_gear(ui, icon_rect, text_color),
            NavItem::Help => crate::components::icons::draw_help(ui, icon_rect, text_color),
            _ => {}
        }

        // Render Label
        let label_pos = Pos2::new(rect.min.x + 44.0, rect.center().y);
        painter.text(
            label_pos,
            egui::Align2::LEFT_CENTER,
            label,
            FontId::proportional(13.8),
            text_color,
        );
    }

    fn render_playlist_badge_item(
        &mut self,
        ui: &mut Ui,
        item: NavItem,
        label: &str,
        badge_color: Color32,
    ) {
        let is_active = self.active_item == item;
        let height = 38.0;
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());

        if response.clicked() {
            (self.on_select)(item);
        }

        let painter = ui.painter_at(rect);
        let is_hovered = response.hovered();

        if is_active {
            painter.rect_filled(rect, CornerRadius::same(9), self.theme.bg_card_active);
        } else if is_hovered {
            painter.rect_filled(rect, CornerRadius::same(9), self.theme.bg_card_hover);
        }

        // Draw colored rounded square badge
        let badge_rect = Rect::from_center_size(
            Pos2::new(rect.min.x + 22.0, rect.center().y),
            Vec2::new(24.0, 24.0),
        );
        painter.rect_filled(badge_rect, CornerRadius::same(7), badge_color);
        let icon_sub_rect = badge_rect.shrink2(Vec2::new(4.5, 4.5));
        match item {
            NavItem::Favorites => {
                crate::components::icons::draw_heart(ui, icon_sub_rect, Color32::WHITE, true);
            }
            NavItem::RecentlyPlayed => {
                crate::components::icons::draw_clock(ui, icon_sub_rect, Color32::WHITE);
            }
            NavItem::MostPlayed => {
                crate::components::icons::draw_bar_chart(ui, icon_sub_rect, Color32::WHITE);
            }
            _ => {
                painter.circle_filled(badge_rect.center(), 3.5, Color32::WHITE);
            }
        }

        // Label
        let text_color = if is_active || is_hovered {
            self.theme.text_primary
        } else {
            self.theme.text_secondary
        };

        painter.text(
            Pos2::new(rect.min.x + 44.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            FontId::proportional(13.8),
            text_color,
        );
    }
}
