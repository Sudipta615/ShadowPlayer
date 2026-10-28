//! Multi-zone playback and ZoneSwitch routing modal.

mod actions;
mod rules_tab;
mod types;
mod zones_tab;

pub use actions::ZoneManagerAction;
use eframe::egui::{self, Context, Vec2};
use playback::ZoneCoordinator;
pub use types::{ZoneManagerModalState, ZoneModalTab};

use crate::theme::Theme;

/// Render the Zone Manager modal if open. Returns actions emitted during this frame.
pub fn show_zone_manager_modal(
    ctx: &Context,
    state: &mut ZoneManagerModalState,
    coordinator: &ZoneCoordinator,
    available_devices: &[String],
    theme: &Theme,
) -> Vec<ZoneManagerAction> {
    let mut actions = Vec::new();
    if !state.is_open {
        return actions;
    }

    let mut is_open = state.is_open;

    egui::Window::new("Audio Zones & Multi-Room Hub")
        .open(&mut is_open)
        .resizable(true)
        .collapsible(false)
        .default_size(Vec2::new(620.0, 520.0))
        .min_width(520.0)
        .min_height(400.0)
        .show(ctx, |ui| {
            // Tab navigation
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(state.active_tab == ZoneModalTab::Zones, "Audio Zones")
                    .clicked()
                {
                    state.active_tab = ZoneModalTab::Zones;
                }
                if ui
                    .selectable_label(
                        state.active_tab == ZoneModalTab::RoutingRules,
                        "ZoneSwitch Rules",
                    )
                    .clicked()
                {
                    state.active_tab = ZoneModalTab::RoutingRules;
                }
            });

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);

            match state.active_tab {
                ZoneModalTab::Zones => {
                    zones_tab::render_zones_tab(
                        ui,
                        state,
                        coordinator,
                        available_devices,
                        theme,
                        &mut actions,
                    );
                }
                ZoneModalTab::RoutingRules => {
                    rules_tab::render_rules_tab(ui, state, coordinator, theme, &mut actions);
                }
            }
        });

    state.is_open = is_open;
    actions
}
