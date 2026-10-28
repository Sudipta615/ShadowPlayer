use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Stroke, Ui, Vec2};

#[inline]
fn fit_square(rect: Rect) -> Rect {
    let side = rect.width().min(rect.height());
    Rect::from_center_size(rect.center(), Vec2::splat(side))
}

pub fn draw_search(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/search.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_home(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/home.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_disc(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/disc.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_artist(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/user.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_genres(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/music.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

/// A music note, used for artwork placeholders.
pub fn draw_music_note(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/music.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_folder(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/folder.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_gear(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/settings.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_help(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/help-circle.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_heart(ui: &Ui, rect: Rect, color: Color32, filled: bool) {
    let r = fit_square(rect);
    if filled {
        egui::Image::new(egui::include_image!("../../assets/icons/heart-filled.svg"))
            .tint(color)
            .paint_at(ui, r);
    } else {
        egui::Image::new(egui::include_image!("../../assets/icons/heart.svg"))
            .tint(color)
            .paint_at(ui, r);
    }
}

pub fn draw_more_menu(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/more-vertical.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_plus_circle(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/plus-circle.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_equalizer_sliders(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/sliders.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_puzzle(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/puzzle.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_shuffle(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/shuffle.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_repeat(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/repeat.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_skip_prev(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/skip-back.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_skip_next(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/skip-forward.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_list_icon(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/list.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_grid_icon(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/grid.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_clock(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/clock.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_trending_up(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/trending-up.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_bar_chart(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/bar-chart-2.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

/// Funnel glyph marking rule-based ("smart") playlists.
pub fn draw_filter(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/filter.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_speaker(ui: &Ui, rect: Rect, color: Color32, is_muted: bool) {
    let r = fit_square(rect);
    if is_muted {
        egui::Image::new(egui::include_image!("../../assets/icons/volume-x.svg"))
            .tint(color)
            .paint_at(ui, r);
    } else {
        egui::Image::new(egui::include_image!("../../assets/icons/volume-2.svg"))
            .tint(color)
            .paint_at(ui, r);
    }
}

pub fn draw_trash(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/trash-2.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_chevron_down(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/chevron-down.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_play(ui: &Ui, rect: Rect, color: Color32, filled: bool) {
    let r = fit_square(rect);
    if filled {
        egui::Image::new(egui::include_image!("../../assets/icons/play-filled.svg"))
            .tint(color)
            .paint_at(ui, r);
    } else {
        egui::Image::new(egui::include_image!("../../assets/icons/play.svg"))
            .tint(color)
            .paint_at(ui, r);
    }
}

pub fn draw_pause(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/pause.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_sun(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/sun.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_moon(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/moon.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_logo(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/logo.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_window_min(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/minus.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_window_max(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/square.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_window_close(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/x.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_reset(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/reset.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_palette(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/palette.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

pub fn draw_headphones(ui: &Ui, rect: Rect, color: Color32) {
    egui::Image::new(egui::include_image!("../../assets/icons/headphones.svg"))
        .tint(color)
        .paint_at(ui, fit_square(rect));
}

/// A broadcast mast for the local Radio view.
pub fn draw_radio(ui: &Ui, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.5;
    let p = ui.painter();
    p.circle_filled(c, r * 0.16, color);
    // Two arcs on each side: the classic broadcast mark.
    for (index, radius) in [0.34f32, 0.52].into_iter().enumerate() {
        let width = 1.4 + index as f32 * 0.2;
        for side in [-1.0f32, 1.0] {
            let start = if side < 0.0 {
                std::f32::consts::FRAC_PI_2 + 0.55
            } else {
                -std::f32::consts::FRAC_PI_2 - 0.55
            };
            p.add(egui::Shape::line(
                (0..=10)
                    .map(|step| {
                        let angle = start + step as f32 * 0.11;
                        Pos2::new(
                            c.x + side * radius * r * angle.cos().abs(),
                            c.y - radius * r * angle.sin(),
                        )
                    })
                    .collect(),
                Stroke::new(width, color),
            ));
        }
    }
    p.line_segment(
        [
            Pos2::new(c.x, c.y + r * 0.12),
            Pos2::new(c.x, c.y + r * 0.62),
        ],
        Stroke::new(1.5, color),
    );
}

/// A microphone for Podcasts.
pub fn draw_mic(ui: &Ui, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.5;
    let p = ui.painter();
    let capsule = Rect::from_center_size(
        Pos2::new(c.x, c.y - r * 0.18),
        Vec2::new(r * 0.58, r * 0.86),
    );
    p.rect_filled(capsule, CornerRadius::same((r * 0.29) as u8), color);
    p.add(egui::Shape::line(
        (0..=12)
            .map(|step| {
                let angle = std::f32::consts::PI * (0.08 + 0.84 * step as f32 / 12.0);
                Pos2::new(
                    c.x + (r * 0.52) * angle.cos(),
                    c.y + r * 0.05 + (r * 0.52) * angle.sin(),
                )
            })
            .collect(),
        Stroke::new(1.5, color),
    ));
    p.line_segment(
        [
            Pos2::new(c.x, c.y + r * 0.55),
            Pos2::new(c.x, c.y + r * 0.78),
        ],
        Stroke::new(1.5, color),
    );
    p.line_segment(
        [
            Pos2::new(c.x - r * 0.22, c.y + r * 0.78),
            Pos2::new(c.x + r * 0.22, c.y + r * 0.78),
        ],
        Stroke::new(1.5, color),
    );
}
