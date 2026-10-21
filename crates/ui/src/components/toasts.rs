//! Transient, self-dismissing notifications.
//!
//! Historically the app only reported the outcome of background work (scans,
//! imports, exports, destructive bulk edits) through the log, which the user
//! never sees. This is the visible feedback channel: a small stack of pills in
//! the bottom-centre that fade out on their own.
//!
//! Toasts carry no interaction — they never steal focus or eat clicks.

use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, CornerRadius, Vec2};

use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
}

impl ToastKind {
    fn accent(self, theme: &Theme) -> Color32 {
        match self {
            ToastKind::Info => theme.info,
            ToastKind::Success => theme.bit_perfect_green,
            ToastKind::Warning => theme.accent_warm,
        }
    }
}

struct Toast {
    message: String,
    kind: ToastKind,
    born: Instant,
    ttl: Duration,
}

/// How long a toast stays fully visible before it starts fading.
const DEFAULT_TTL: Duration = Duration::from_millis(3200);
/// Length of the fade-out at the end of a toast's life.
const FADE: Duration = Duration::from_millis(450);

#[derive(Default)]
pub struct ToastState {
    items: Vec<Toast>,
}

impl ToastState {
    pub fn push(&mut self, kind: ToastKind, message: impl Into<String>) {
        let message = message.into();
        // Identical back-to-back messages (e.g. a flurry of import results)
        // collapse instead of stacking up.
        if let Some(last) = self.items.last_mut() {
            if last.message == message {
                last.born = Instant::now();
                return;
            }
        }
        // Never let a runaway producer grow the stack without bound.
        if self.items.len() >= 4 {
            self.items.remove(0);
        }
        self.items.push(Toast {
            message,
            kind,
            born: Instant::now(),
            ttl: DEFAULT_TTL,
        });
    }

    pub fn info(&mut self, message: impl Into<String>) {
        self.push(ToastKind::Info, message);
    }

    pub fn success(&mut self, message: impl Into<String>) {
        self.push(ToastKind::Success, message);
    }

    pub fn warning(&mut self, message: impl Into<String>) {
        self.push(ToastKind::Warning, message);
    }

    /// Drop expired toasts and report whether any remain (so the caller can
    /// keep the frame loop ticking while one is on screen).
    pub fn prune(&mut self) -> bool {
        let now = Instant::now();
        self.items.retain(|t| now.duration_since(t.born) < t.ttl);
        !self.items.is_empty()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Draw the stack. No-op when there is nothing to show.
    pub fn show(&self, ctx: &egui::Context, theme: &Theme) {
        if self.items.is_empty() {
            return;
        }
        let now = Instant::now();

        egui::Area::new(egui::Id::unique("shadowplayer_toasts"))
            .anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -78.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_sm);
                ui.vertical_centered(|ui| {
                    for t in &self.items {
                        let age = now.duration_since(t.born);
                        let remaining = t.ttl.saturating_sub(age);
                        // Fade only over the final stretch of the lifetime.
                        let alpha = if remaining < FADE {
                            remaining.as_secs_f32() / FADE.as_secs_f32()
                        } else {
                            1.0
                        };
                        let a = (alpha.clamp(0.0, 1.0) * 255.0) as u8;

                        let accent = t.kind.accent(theme);
                        let fill = crate::theme::with_alpha(theme.bg_card, a.saturating_sub(30));
                        let border = crate::theme::with_alpha(accent, a);
                        let text = crate::theme::with_alpha(theme.text_primary, a);

                        egui::Frame::NONE
                            .fill(fill)
                            .stroke(egui::Stroke::new(1.0, border))
                            .corner_radius(CornerRadius::same(theme.radius_md))
                            .inner_margin(egui::Margin::symmetric(
                                theme.space_2xl as i8,
                                theme.space_lg as i8,
                            ))
                            .show(ui, |ui| {
                                let resp = ui.add(egui::Label::new(
                                    egui::RichText::new(&t.message)
                                        .font(crate::theme::font(12.5))
                                        .color(text),
                                ));
                                // An alert role, so a screen reader announces
                                // the message instead of leaving it as a
                                // silent label on the page.
                                resp.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::Role::Alert,
                                        true,
                                        t.message.clone(),
                                    )
                                });
                            });
                    }
                });
            });
    }
}
