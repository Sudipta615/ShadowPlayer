use super::*;

impl ShadowPlayerApp {
    /// Handle the app-wide keyboard shortcuts.
    ///
    /// Two subtleties shape this function, and both were wrong before:
    ///
    /// * `InputState::focused` is the *native window* focus — whether the OS is
    ///   delivering key presses to this window — not "a widget has focus".
    ///   Guarding on `!i.focused` therefore ran these bindings only while the
    ///   window was in the background, where the OS delivers no key events at
    ///   all. Space/K, J, L, M, S, R and the four arrows could never fire.
    /// * A focused text field owns the keyboard, and egui routes Space/Enter
    ///   to whatever widget holds focus. Both have to be left alone, or typing
    ///   "moon" into the search box would mute the player.
    pub(super) fn handle_global_shortcuts(&mut self, ctx: &egui::Context) {
        let text_focused = ctx.text_edit_focused();
        let widget_focused = ctx.memory(|m| m.focused()).is_some();

        ctx.input(|i| {
            let ctrl = i.modifiers.command || i.modifiers.ctrl;

            // Ctrl/Cmd chords are explicit app commands, and none of them is a
            // key a text field wants, so they stay live even while a field has
            // focus.
            if ctrl && i.modifiers.alt {
                // Ctrl/Cmd+Alt+Up/Down steps the rating. It is checked first so
                // Alt does not fall through to the volume arrows below.
                if i.key_pressed(egui::Key::ArrowUp) {
                    self.step_target_rating(1);
                }
                if i.key_pressed(egui::Key::ArrowDown) {
                    self.step_target_rating(-1);
                }
            } else if ctrl {
                let rating_key = if i.key_pressed(egui::Key::Num0) {
                    Some(0)
                } else if i.key_pressed(egui::Key::Num1) {
                    Some(1)
                } else if i.key_pressed(egui::Key::Num2) {
                    Some(2)
                } else if i.key_pressed(egui::Key::Num3) {
                    Some(3)
                } else if i.key_pressed(egui::Key::Num4) {
                    Some(4)
                } else if i.key_pressed(egui::Key::Num5) {
                    Some(5)
                } else {
                    None
                };
                if let Some(rating) = rating_key {
                    self.rate_targets(rating);
                }

                if i.key_pressed(egui::Key::N) {
                    self.app_state.open_playlist_editor = true;
                }
                if i.key_pressed(egui::Key::B) {
                    self.toggle_bit_perfect();
                }
                if i.key_pressed(egui::Key::P) {
                    self.app_state.active_nav = NavItem::Settings;
                }
                if i.key_pressed(egui::Key::O) {
                    self.app_state.active_nav = NavItem::Folders;
                }
                if i.key_pressed(egui::Key::E) {
                    self.dsp_state.eq_panel.is_open = !self.dsp_state.eq_panel.is_open;
                }
                if i.key_pressed(egui::Key::F) {
                    self.request_search_focus();
                }
                if i.key_pressed(egui::Key::Space) {
                    self.app_state.command_palette_open = true;
                }
            }

            if i.key_pressed(egui::Key::Escape) {
                // The full-screen Now Playing mode is the outermost surface:
                // Escape leaves it, and only then does the chain below run.
                if self.app_state.now_playing_full {
                    self.set_now_playing_full(ctx, false);
                    return;
                }
                // The info popup is the topmost transient surface, so it takes
                // the press first: one Escape must not close it *and* the
                // layer behind it.
                if crate::components::info_box::dismiss_open_info_box(ctx) {
                    return;
                }
                self.dismiss_foreground_overlay();
            }

            // The plain transport keys. Ctrl/Cmd is excluded so `Ctrl+M` and
            // friends stay available to the OS, and Shift/Alt are excluded so
            // they keep their usual text-selection and window-management jobs.
            if ctrl || i.modifiers.shift || i.modifiers.alt || text_focused {
                return;
            }

            // Space also activates a focused widget (egui synthesises a click
            // for it), so leave it alone when something else holds focus.
            if !widget_focused && (i.key_pressed(egui::Key::Space) || i.key_pressed(egui::Key::K)) {
                self.coordinator.toggle_play_pause();
            }
            if i.key_pressed(egui::Key::J) {
                self.coordinator.previous();
            }
            if i.key_pressed(egui::Key::Slash) {
                self.request_search_focus();
            }
            if i.key_pressed(egui::Key::F) {
                let next = !self.app_state.now_playing_full;
                self.set_now_playing_full(ctx, next);
            }
            if i.key_pressed(egui::Key::L) {
                self.coordinator.next();
            }
            if i.key_pressed(egui::Key::M) {
                self.app_state.is_muted = !self.app_state.is_muted;
                let target_vol = if self.app_state.is_muted {
                    0.0
                } else {
                    self.app_state.volume
                };
                self.session.set_volume(target_vol);
                self.settings_dirty = true;
            }
            if i.key_pressed(egui::Key::S) {
                let new_shuffle = !self.coordinator.is_shuffle();
                self.coordinator.set_shuffle(new_shuffle);
                self.settings_dirty = true;
            }
            if i.key_pressed(egui::Key::R) {
                let next_mode = match self.coordinator.repeat_mode() {
                    domain::models::RepeatModeChoice::Off => domain::models::RepeatModeChoice::All,
                    domain::models::RepeatModeChoice::All => domain::models::RepeatModeChoice::One,
                    domain::models::RepeatModeChoice::One => domain::models::RepeatModeChoice::Off,
                };
                self.coordinator.set_repeat_mode(next_mode);
                self.settings_dirty = true;
            }
            if i.key_pressed(egui::Key::ArrowLeft) {
                let cur = self.session.position_secs_compensated();
                self.session.seek((cur - 5.0).max(0.0));
            }
            if i.key_pressed(egui::Key::ArrowRight) {
                let cur = self.session.position_secs_compensated();
                let dur = self.session.duration_secs().max(1.0);
                self.session.seek((cur + 5.0).min(dur));
            }
            if i.key_pressed(egui::Key::ArrowUp) {
                self.app_state.volume = (self.app_state.volume + 0.05).min(1.0);
                self.app_state.is_muted = false;
                self.session.set_volume(self.app_state.volume);
                self.settings_dirty = true;
            }
            if i.key_pressed(egui::Key::ArrowDown) {
                self.app_state.volume = (self.app_state.volume - 0.05).max(0.0);
                self.app_state.is_muted = false;
                self.session.set_volume(self.app_state.volume);
                self.settings_dirty = true;
            }
        });
    }

    /// Flip the bit-perfect "Pure Direct" mode, bypassing the whole DSP chain.
    ///
    /// Enabling it also neutralises every stage that would alter the signal, so
    /// the toggle is the same in both directions.
    pub(super) fn toggle_bit_perfect(&mut self) {
        let cur_bp = self.session.bit_perfect();
        if !cur_bp {
            // Activate Pure Direct
            self.session.set_volume(1.0);
            self.session.set_preamp(0.0);
            self.session.set_speed(1.0);
            self.session.set_eq_enabled(false);
            self.session.set_graphic_eq_enabled(false);
            self.session.set_limiter_enabled(false);
            self.session.set_compressor_enabled(false);
            self.session.set_spatial_enabled(false);
            self.session.set_stereo_enhancer_enabled(false);
            self.session.set_bit_perfect(true);
            self.dsp_state.settings_tab.bit_perfect = true;
        } else {
            self.session.set_bit_perfect(false);
            self.dsp_state.settings_tab.bit_perfect = false;
        }
    }
}
