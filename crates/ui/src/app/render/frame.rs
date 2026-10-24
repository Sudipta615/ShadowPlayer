use super::super::*;

impl ShadowPlayerApp {
    pub(in crate::app) fn prepare_frame(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Windows/macOS create their SMTC/Now Playing objects from the HWND
        // (or NSWindow) on the first frame; Linux's MPRIS backend is already
        // running from `new()`.
        self.mpris_service.attach_window(frame);
        if self.applied_theme != Some(self.theme) {
            self.theme.apply(ctx);
            self.applied_theme = Some(self.theme);
        }

        // Remember the live window size: this is what the next launch restores.
        let size = ctx.content_rect().size();
        if size.x > 0.0 && size.y > 0.0 {
            self.window_size = (size.x, size.y);
        }

        // Mini-player viewport transition. The normal geometry is kept aside so
        // restoring never lands at the mini player's 320x110 floor.
        if self.app_state.mini_player && !self.mini_applied {
            self.normal_window_size =
                (self.window_size.0.max(640.0), self.window_size.1.max(360.0));
            self.mini_applied = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(Vec2::new(320.0, 110.0)));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(320.0, 110.0)));
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                egui::WindowLevel::AlwaysOnTop,
            ));
        } else if !self.app_state.mini_player && self.mini_applied {
            self.mini_applied = false;
            let (w, h) = self.normal_window_size;
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(Vec2::new(640.0, 360.0)));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
                w.max(640.0),
                h.max(360.0),
            )));
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                egui::WindowLevel::Normal,
            ));
            self.settings_dirty = true;
        }
    }
}
