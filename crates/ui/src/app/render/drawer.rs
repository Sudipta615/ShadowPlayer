use super::super::*;
use super::PlaybackView;

/// Geometry shared by the wide drawer column and the narrow overlay.
pub(super) struct DrawerLayout {
    pub full_rect: Rect,
    pub avail_w: f32,
    pub avail_h: f32,
    pub show_right_panel: bool,
    pub right_w: f32,
    pub right_rect: Rect,
}

impl ShadowPlayerApp {
    pub(super) fn render_drawer(
        &mut self,
        root_ui: &mut egui::Ui,
        ctx: &egui::Context,
        window_w: f32,
        layout: DrawerLayout,
        playback: &PlaybackView<'_>,
    ) {
        let DrawerLayout {
            full_rect,
            avail_w,
            avail_h,
            show_right_panel,
            right_w,
            right_rect,
        } = layout;

        let is_help_view = self.app_state.active_nav == NavItem::Help;
        if show_right_panel {
            let mut right_ui = root_ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(right_rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            right_ui.set_clip_rect(right_rect);
            right_ui.allocate_ui_with_layout(
                Vec2::new(right_w, 38.0),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    self.render_topbar_actions(ui, window_w);
                },
            );
            right_ui.add_space(self.theme.space_sm);
            let prev_light = self.theme.is_light;
            let prev_vol = self.app_state.volume;
            let prev_muted = self.app_state.is_muted;
            render_right_drawer(
                &mut right_ui,
                ctx,
                right_w,
                avail_h,
                &mut self.app_state,
                &mut self.coordinator,
                &self.session,
                &mut self.artwork_service,
                &mut self.lyrics_service,
                playback.current_track,
                playback.active_lyrics,
                playback.plain_lyrics,
                playback.lyrics_source,
                playback.active_lyric_idx,
                &mut self.theme,
                true,
            );
            if self.theme.is_light != prev_light
                || (self.app_state.volume - prev_vol).abs() > 0.001
                || self.app_state.is_muted != prev_muted
            {
                self.settings_dirty = true;
            }
        }

        // Narrow layout: the same drawer becomes a temporary
        // overlay anchored below the top bar, leaving the queue button
        // reachable. Escape closes it as well.
        if !is_help_view
            && self.app_state.active_nav != NavItem::Settings
            && window_w < 1100.0
            && self.app_state.right_overlay_open
        {
            let overlay_w = (avail_w - 16.0).clamp(240.0, 320.0);
            let top = full_rect.min.y + 50.0;
            let overlay_rect = Rect::from_min_max(
                Pos2::new(full_rect.max.x - overlay_w, top),
                Pos2::new(full_rect.max.x, full_rect.max.y),
            );
            // The overlay is a floating card with transparent margins
            // around it. Paint the column background first so table
            // content cannot bleed through below the card.
            root_ui
                .painter()
                .rect_filled(overlay_rect, 0.0, self.theme.bg_app);
            // A soft scrim makes the drawer read as foreground without
            // hiding the top-bar controls.
            root_ui.painter().rect_filled(
                Rect::from_min_max(
                    Pos2::new(full_rect.min.x, top),
                    Pos2::new(overlay_rect.min.x, full_rect.max.y),
                ),
                0.0,
                self.theme.drawer_scrim,
            );
            let mut overlay_ui = root_ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(overlay_rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            overlay_ui.set_clip_rect(overlay_rect);
            let prev_light = self.theme.is_light;
            let prev_vol = self.app_state.volume;
            let prev_muted = self.app_state.is_muted;
            render_right_drawer(
                &mut overlay_ui,
                ctx,
                overlay_w,
                overlay_rect.height(),
                &mut self.app_state,
                &mut self.coordinator,
                &self.session,
                &mut self.artwork_service,
                &mut self.lyrics_service,
                playback.current_track,
                playback.active_lyrics,
                playback.plain_lyrics,
                playback.lyrics_source,
                playback.active_lyric_idx,
                &mut self.theme,
                false,
            );
            if self.theme.is_light != prev_light
                || (self.app_state.volume - prev_vol).abs() > 0.001
                || self.app_state.is_muted != prev_muted
            {
                self.settings_dirty = true;
            }
        }
    }
}
