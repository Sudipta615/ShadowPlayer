use crate::theme::Theme;
use domain::ids::PlaylistId;
use domain::models::Playlist;
use domain::smart::SmartPlaylist;
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Ui, Vec2};

/// The room the wordmark needs before it has to step its type down.
///
/// The sidebar is 208 px wide, and 200 px in the compact layout; after the
/// panel's own margins that leaves 184 px and 176 px.
const BRAND_FULL_WIDTH: f32 = 180.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavItem {
    Home,
    Albums,
    Artists,
    Genres,
    /// Local smart-shuffle station seeded from a track.
    Radio,
    /// Podcast subscriptions, separate from the music library.
    Podcasts,
    Folders,
    Favorites,
    RecentlyPlayed,
    MostPlayed,
    /// Newest additions to the library, by import time.
    RecentlyAdded,
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
    /// Icon-only navigation for narrow windows. Every item keeps its tooltip
    /// and the playlist shortcuts remain reachable.
    pub rail: bool,
    pub on_select: Box<dyn FnMut(NavItem) + 'a>,
    pub on_create_playlist: Box<dyn FnMut() + 'a>,
    pub on_create_smart_playlist: Box<dyn FnMut() + 'a>,
    pub on_delete_playlist: Box<dyn FnMut(PlaylistId) + 'a>,
    pub on_delete_smart_playlist: Box<dyn FnMut(PlaylistId) + 'a>,
    pub on_edit_smart_playlist: Box<dyn FnMut(PlaylistId) + 'a>,
}

impl<'a> SideBar<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        active_item: NavItem,
        playlists: &'a [Playlist],
        smart_playlists: &'a [SmartPlaylist],
        theme: &'a Theme,
        rail: bool,
        on_select: impl FnMut(NavItem) + 'a,
        on_create_playlist: impl FnMut() + 'a,
        on_create_smart_playlist: impl FnMut() + 'a,
        on_delete_playlist: impl FnMut(PlaylistId) + 'a,
        on_delete_smart_playlist: impl FnMut(PlaylistId) + 'a,
        on_edit_smart_playlist: impl FnMut(PlaylistId) + 'a,
    ) -> Self {
        Self {
            active_item,
            playlists,
            smart_playlists,
            theme,
            rail,
            on_select: Box::new(on_select),
            on_create_playlist: Box::new(on_create_playlist),
            on_create_smart_playlist: Box::new(on_create_smart_playlist),
            on_delete_playlist: Box::new(on_delete_playlist),
            on_delete_smart_playlist: Box::new(on_delete_smart_playlist),
            on_edit_smart_playlist: Box::new(on_edit_smart_playlist),
        }
    }

    pub fn show(mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.set_width(if self.rail {
                40.0
            } else {
                ui.available_width().max(120.0)
            });
            ui.spacing_mut().item_spacing = Vec2::new(0.0, if self.rail { 6.0 } else { 4.0 });

            // 1. Branding: logo + title + subtitle in the full sidebar, just
            // the mark in the rail.
            if self.rail {
                let (logo_rect, _) = ui.allocate_exact_size(Vec2::new(34.0, 34.0), Sense::hover());
                crate::components::icons::draw_logo(
                    ui,
                    Rect::from_center_size(logo_rect.center(), Vec2::splat(28.0)),
                    self.theme.accent_warm,
                );
            } else {
                // The sidebar is 208 px wide, and 200 px in the compact
                // layout; after its own margins that leaves 184 px and
                // 176 px of room. At the full display size the wordmark is a
                // few pixels too wide in the compact layout, so step the
                // brand type down there instead of truncating "ShadowPla...".
                let compact_brand = ui.available_width() < BRAND_FULL_WIDTH;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(self.theme.space_lg, 0.0);

                    let (logo_rect, _) =
                        ui.allocate_exact_size(Vec2::new(30.0, 30.0), Sense::hover());
                    crate::components::icons::draw_logo(ui, logo_rect, self.theme.accent_warm);

                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(0.0, self.theme.space_xxs);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(tr!("ShadowPlayer"))
                                    .font(crate::theme::font(if compact_brand {
                                        15.0
                                    } else {
                                        17.0
                                    }))
                                    .strong()
                                    .color(self.theme.text_primary),
                            )
                            .truncate(),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(tr!("Music for a better you"))
                                    .font(crate::theme::font(10.5))
                                    .color(self.theme.text_muted),
                            )
                            .truncate(),
                        );
                    });
                });
            }

            ui.add_space(if self.rail { 8.0 } else { 16.0 });

            // 2. Primary Navigation
            self.render_nav_item(ui, NavItem::Home, "Home");
            self.render_nav_item(ui, NavItem::Albums, "Albums");
            self.render_nav_item(ui, NavItem::Artists, "Artists");
            self.render_nav_item(ui, NavItem::Genres, "Genres");
            self.render_nav_item(ui, NavItem::Folders, "Folders");

            // Collections section removed per user request

            // 3. Playlists Header with Circular "+" Button
            ui.add_space(if self.rail { 8.0 } else { 16.0 });
            if self.rail {
                let (plus_rect, plus_resp) =
                    ui.allocate_exact_size(Vec2::new(40.0, 30.0), Sense::click());
                plus_resp.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Button, true, "New playlist")
                });
                let plus_color = if plus_resp.hovered() {
                    self.theme.text_primary
                } else {
                    self.theme.text_muted
                };
                crate::components::icons::draw_plus_circle(
                    ui,
                    Rect::from_center_size(plus_rect.center(), Vec2::splat(18.0)),
                    plus_color,
                );
                if plus_resp
                    .on_hover_text(tr!(Msg::SD4F48AACA7B6E405))
                    .clicked()
                {
                    (self.on_create_playlist)();
                }
            } else {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(tr!("Playlists"))
                            .font(crate::theme::font(13.0))
                            .color(self.theme.text_muted),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let plus_id = ui.make_persistent_id("add_playlist_popup_btn");
                        let mut menu_open: bool =
                            ui.ctx().data(|d| d.get_temp(plus_id).unwrap_or(false));
                        let (plus_rect, plus_resp) =
                            ui.allocate_exact_size(Vec2::new(22.0, 22.0), Sense::click());
                        let plus_color = if plus_resp.hovered() || menu_open {
                            self.theme.text_primary
                        } else {
                            self.theme.text_muted
                        };
                        crate::components::icons::draw_plus_circle(ui, plus_rect, plus_color);
                        plus_resp.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::Button, true, "New playlist")
                        });
                        let _ = plus_resp.clone().on_hover_text(tr!(Msg::SD4F48AACA7B6E405));
                        if plus_resp.clicked() {
                            menu_open = !menu_open;
                            ui.ctx().data_mut(|d| d.insert_temp(plus_id, menu_open));
                        }
                        if menu_open {
                            let popup_pos = plus_rect.left_bottom() + Vec2::new(0.0, 4.0);
                            let mut close = false;
                            let mut popup_rect: Option<Rect> = None;
                            egui::Area::new(plus_id.with("popup"))
                                .order(egui::Order::Foreground)
                                .fixed_pos(popup_pos)
                                .show(ui.ctx(), |ui| {
                                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                                        ui.set_min_width(140.0);
                                        if ui.button(tr!(Msg::SD50E805A69F8C016)).clicked() {
                                            (self.on_create_playlist)();
                                            close = true;
                                        }
                                        if ui.button(tr!(Msg::SDA82BB90A6E11970)).clicked() {
                                            (self.on_create_smart_playlist)();
                                            close = true;
                                        }
                                    });
                                    popup_rect = Some(ui.min_rect());
                                });

                            let clicked_elsewhere = ui.input(|i| i.pointer.any_click())
                                && !plus_resp.hovered()
                                && !popup_rect
                                    .map(|r| ui.rect_contains_pointer(r))
                                    .unwrap_or(false);
                            if close || clicked_elsewhere {
                                ui.ctx().data_mut(|d| d.insert_temp(plus_id, false));
                            }
                        }
                    });
                });
            }

            ui.add_space(4.0);

            // 4. Playlist Special Items with Colored Badges
            self.render_playlist_badge_item(
                ui,
                NavItem::Favorites,
                "Favorites",
                self.theme.accent_highlight,
            );
            self.render_playlist_badge_item(
                ui,
                NavItem::RecentlyPlayed,
                "Recently Played",
                self.theme.accent,
            );
            self.render_playlist_badge_item(
                ui,
                NavItem::MostPlayed,
                "Most Played",
                self.theme.accent_warm,
            );
            self.render_playlist_badge_item(
                ui,
                NavItem::RecentlyAdded,
                "Recently Added",
                self.theme.info,
            );

            // Merged User Custom Playlists and Smart Playlists
            if !self.playlists.is_empty() || !self.smart_playlists.is_empty() {
                ui.add_space(4.0);
                for pl in self.playlists {
                    self.render_user_playlist_item(ui, pl);
                }
                for sp in self.smart_playlists {
                    self.render_smart_playlist_item(ui, sp);
                }
            }

            // 5. Settings and Help follow the content rather than being
            // absolutely anchored. In short windows the sidebar scrolls, so an
            // anchor would simply overlap the playlists.
            ui.add_space(if self.rail { 8.0 } else { 16.0 });
            self.render_nav_item(ui, NavItem::Settings, "Settings");
            self.render_nav_item(ui, NavItem::Help, "Help & Feedback");
            ui.add_space(4.0);
        });
    }

    fn render_smart_playlist_item(&mut self, ui: &mut Ui, playlist: &SmartPlaylist) {
        let item = NavItem::SmartPlaylist(playlist.id);
        if self.rail {
            let is_active = self.active_item == item;
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), Sense::click());
            if response.clicked() {
                (self.on_select)(item);
            }
            let painter = ui.painter_at(rect);
            if is_active {
                painter.rect_filled(
                    rect,
                    CornerRadius::same(self.theme.radius_md),
                    self.theme.accent,
                );
            } else if response.hovered() {
                painter.rect_filled(
                    rect,
                    CornerRadius::same(self.theme.radius_md),
                    self.theme.bg_card_hover,
                );
            }
            crate::components::icons::draw_filter(
                ui,
                Rect::from_center_size(rect.center(), Vec2::splat(16.0)),
                if is_active {
                    Color32::WHITE
                } else {
                    self.theme.info
                },
            );
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::Role::Button,
                    true,
                    is_active,
                    format!("Smart playlist: {}", playlist.name),
                )
            });
            let _ = response.on_hover_text(format!("Smart playlist: {}", playlist.name));
            return;
        }
        let is_active = self.active_item == item;
        let height = 34.0;
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::Role::Button,
                true,
                is_active,
                format!("Smart playlist: {}", playlist.name),
            )
        });

        let trash_rect = Rect::from_center_size(
            Pos2::new(rect.max.x - 12.0, rect.center().y),
            Vec2::new(16.0, 16.0),
        );
        let trash_id = ui.make_persistent_id(format!("del_sp_{}", playlist.id));
        let trash_resp = ui
            .interact(trash_rect, trash_id, Sense::click())
            .on_hover_text(tr!(Msg::S4161CC1EC08A7B2F));
        trash_resp.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, true, "Delete smart playlist")
        });

        if trash_resp.clicked() {
            (self.on_delete_smart_playlist)(playlist.id);
        } else if response.clicked() {
            (self.on_select)(item);
        }

        let painter = ui.painter_at(rect);
        let is_hovered = response.hovered();

        if is_active {
            painter.rect_filled(
                rect,
                CornerRadius::same(self.theme.radius_sm),
                self.theme.info,
            );
        } else if is_hovered {
            painter.rect_filled(
                rect,
                CornerRadius::same(self.theme.radius_sm),
                self.theme.bg_card_hover,
            );
        }

        let text_color = if is_active || is_hovered {
            self.theme.text_primary
        } else {
            self.theme.text_secondary
        };

        // A filter glyph marks rule-based playlists.
        crate::components::icons::draw_filter(
            ui,
            Rect::from_center_size(
                Pos2::new(rect.min.x + 14.0, rect.center().y),
                Vec2::new(13.0, 13.0),
            ),
            if is_active {
                Color32::WHITE
            } else {
                self.theme.info
            },
        );

        // Delete trash icon
        let trash_color = if trash_resp.hovered() {
            self.theme.accent_warm
        } else if is_hovered || is_active {
            self.theme.text_secondary
        } else {
            self.theme.text_muted.gamma_multiply(0.6)
        };
        crate::components::icons::draw_trash(ui, trash_rect, trash_color);

        // Smart playlist name
        let max_label_w = (rect.width() - 28.0 - 24.0).max(20.0);
        let mut job = egui::text::LayoutJob::simple_singleline(
            playlist.name.clone(),
            crate::theme::font(12.5),
            text_color,
        );
        job.wrap.max_width = max_label_w;
        job.wrap.break_anywhere = false;
        job.wrap.overflow_character = Some('…');
        let galley = painter.layout_job(job);
        painter.galley(
            Pos2::new(rect.min.x + 26.0, rect.center().y - galley.size().y * 0.5),
            galley,
            text_color,
        );
    }

    fn render_user_playlist_item(&mut self, ui: &mut Ui, playlist: &Playlist) {
        let item = NavItem::Playlist(playlist.id);
        if self.rail {
            let is_active = self.active_item == item;
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), Sense::click());
            if response.clicked() {
                (self.on_select)(item);
            }
            let painter = ui.painter_at(rect);
            if is_active {
                painter.rect_filled(
                    rect,
                    CornerRadius::same(self.theme.radius_md),
                    self.theme.accent,
                );
            } else if response.hovered() {
                painter.rect_filled(
                    rect,
                    CornerRadius::same(self.theme.radius_md),
                    self.theme.bg_card_hover,
                );
            }
            crate::components::icons::draw_list_icon(
                ui,
                Rect::from_center_size(rect.center(), Vec2::splat(16.0)),
                if is_active {
                    Color32::WHITE
                } else {
                    self.theme.accent_warm
                },
            );
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::Role::Button,
                    true,
                    is_active,
                    format!("Playlist: {}", playlist.name),
                )
            });
            let _ = response.on_hover_text(format!("Playlist: {}", playlist.name));
            return;
        }
        let is_active = self.active_item == item;
        let height = 34.0;
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::Role::Button,
                true,
                is_active,
                format!("Playlist: {}", playlist.name),
            )
        });

        let trash_rect = Rect::from_center_size(
            Pos2::new(rect.max.x - 12.0, rect.center().y),
            Vec2::new(16.0, 16.0),
        );
        let trash_id = ui.make_persistent_id(format!("del_pl_{}", playlist.id));
        let trash_resp = ui
            .interact(trash_rect, trash_id, Sense::click())
            .on_hover_text(tr!(Msg::SAD9F16B467D53F18));
        trash_resp
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Delete playlist"));

        if trash_resp.clicked() {
            (self.on_delete_playlist)(playlist.id);
        } else if response.clicked() {
            (self.on_select)(NavItem::Playlist(playlist.id));
        }

        let painter = ui.painter_at(rect);
        let is_hovered = response.hovered();

        if is_active {
            painter.rect_filled(
                rect,
                CornerRadius::same(self.theme.radius_sm),
                self.theme.accent,
            );
        } else if is_hovered {
            painter.rect_filled(
                rect,
                CornerRadius::same(self.theme.radius_sm),
                self.theme.bg_card_hover,
            );
        }

        let text_color = if is_active || is_hovered {
            self.theme.text_primary
        } else {
            self.theme.text_secondary
        };

        // Icon
        let pl_icon_rect = Rect::from_center_size(
            Pos2::new(rect.min.x + 14.0, rect.center().y),
            Vec2::new(13.0, 13.0),
        );
        crate::components::icons::draw_list_icon(
            ui,
            pl_icon_rect,
            if is_active {
                Color32::WHITE
            } else {
                self.theme.accent_warm
            },
        );

        // Delete trash icon
        let trash_color = if trash_resp.hovered() {
            self.theme.accent_warm
        } else if is_hovered || is_active {
            self.theme.text_secondary
        } else {
            self.theme.text_muted.gamma_multiply(0.6)
        };
        crate::components::icons::draw_trash(ui, trash_rect, trash_color);

        // Count badge
        let count_str = format!("{}", playlist.track_count);
        let count_pos = Pos2::new(rect.max.x - 24.0, rect.center().y);
        painter.text(
            count_pos,
            egui::Align2::RIGHT_CENTER,
            count_str,
            crate::theme::font(10.5),
            if is_active {
                Color32::WHITE
            } else {
                self.theme.text_muted
            },
        );

        // Playlist name
        let max_label_w = (rect.width() - 28.0 - 46.0).max(20.0);
        let mut job = egui::text::LayoutJob::simple_singleline(
            playlist.name.clone(),
            crate::theme::font(12.5),
            text_color,
        );
        job.wrap.max_width = max_label_w;
        job.wrap.break_anywhere = false;
        job.wrap.overflow_character = Some('…');
        let galley = painter.layout_job(job);
        painter.galley(
            Pos2::new(rect.min.x + 26.0, rect.center().y - galley.size().y * 0.5),
            galley,
            text_color,
        );
    }

    fn render_nav_item(&mut self, ui: &mut Ui, item: NavItem, label: &str) {
        let is_active = self.active_item == item;
        let height = if self.rail { 40.0 } else { 38.0 };
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());

        if response.clicked() {
            (self.on_select)(item);
        }

        let painter = ui.painter_at(rect);
        let is_hovered = response.hovered();

        // The active fill fades in rather than snapping. The id is the item's
        // label, so there is one animation state per navigation item.
        let active_t =
            ui.ctx()
                .animate_bool_with_time(ui.make_persistent_id(("nav", label)), is_active, 0.16);
        let fill = if is_active {
            crate::theme::lerp_color(self.theme.bg_card_hover, self.theme.accent, active_t)
        } else if is_hovered {
            self.theme.bg_card_hover
        } else {
            Color32::TRANSPARENT
        };
        if fill != Color32::TRANSPARENT {
            painter.rect_filled(rect, CornerRadius::same(self.theme.radius_md), fill);
        }

        let text_color = if is_active || is_hovered {
            self.theme.text_primary
        } else {
            self.theme.text_secondary
        };

        // Render Icon
        let icon_center = if self.rail {
            rect.center()
        } else {
            Pos2::new(rect.min.x + 22.0, rect.center().y)
        };
        let icon_rect = Rect::from_center_size(icon_center, Vec2::new(20.0, 20.0));
        match item {
            NavItem::Home => crate::components::icons::draw_home(ui, icon_rect, text_color),
            NavItem::Albums => crate::components::icons::draw_disc(ui, icon_rect, text_color),
            NavItem::Artists => crate::components::icons::draw_artist(ui, icon_rect, text_color),
            NavItem::Genres => crate::components::icons::draw_genres(ui, icon_rect, text_color),
            NavItem::Radio => crate::components::icons::draw_radio(ui, icon_rect, text_color),
            NavItem::Podcasts => crate::components::icons::draw_mic(ui, icon_rect, text_color),
            NavItem::Folders => crate::components::icons::draw_folder(ui, icon_rect, text_color),
            NavItem::Settings => crate::components::icons::draw_gear(ui, icon_rect, text_color),
            NavItem::Help => crate::components::icons::draw_help(ui, icon_rect, text_color),
            _ => {}
        }

        response
            .widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, is_active, label));
        if self.rail {
            let _ = response.on_hover_text(label);
        } else {
            // Render Label
            let label_pos = Pos2::new(rect.min.x + 44.0, rect.center().y);
            painter.text(
                label_pos,
                egui::Align2::LEFT_CENTER,
                label,
                crate::theme::font(13.8),
                text_color,
            );
        }
    }

    fn render_playlist_badge_item(
        &mut self,
        ui: &mut Ui,
        item: NavItem,
        label: &str,
        badge_color: Color32,
    ) {
        let is_active = self.active_item == item;
        let height = if self.rail { 40.0 } else { 38.0 };
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());

        if response.clicked() {
            (self.on_select)(item);
        }

        let painter = ui.painter_at(rect);
        let is_hovered = response.hovered();

        // The active fill fades in rather than snapping. One animation state
        // per badge item.
        let active_t = ui.ctx().animate_bool_with_time(
            ui.make_persistent_id(("badge", label)),
            is_active,
            0.16,
        );
        let fill = if is_active {
            crate::theme::lerp_color(
                self.theme.bg_card_hover,
                self.theme.bg_card_active,
                active_t,
            )
        } else if is_hovered {
            self.theme.bg_card_hover
        } else {
            Color32::TRANSPARENT
        };
        if fill != Color32::TRANSPARENT {
            painter.rect_filled(rect, CornerRadius::same(self.theme.radius_md), fill);
        }

        // Draw colored rounded square badge
        let badge_center = if self.rail {
            rect.center()
        } else {
            Pos2::new(rect.min.x + 22.0, rect.center().y)
        };
        let badge_rect = Rect::from_center_size(badge_center, Vec2::new(24.0, 24.0));
        painter.rect_filled(
            badge_rect,
            CornerRadius::same(self.theme.radius_sm),
            badge_color,
        );
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
            NavItem::RecentlyAdded => {
                crate::components::icons::draw_trending_up(ui, icon_sub_rect, Color32::WHITE);
            }
            _ => {
                painter.circle_filled(badge_rect.center(), 3.5, Color32::WHITE);
            }
        }

        response
            .widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, is_active, label));
        if self.rail {
            let _ = response.on_hover_text(label);
            return;
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
            crate::theme::font(13.8),
            text_color,
        );
    }
}
