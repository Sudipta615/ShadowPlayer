use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Ui, Vec2};

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

/// A 3-node signal path diagram representing the Audio Path (Source -> Processing -> DAC).
pub fn draw_audio_path(ui: &Ui, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.44;
    let p = ui.painter();
    let y = c.y;
    let left_x = c.x - r * 0.85;
    let mid_x = c.x;
    let right_x = c.x + r * 0.85;
    let stroke = Stroke::new(1.6, color);

    // Horizontal connecting bus line
    p.line_segment([Pos2::new(left_x, y), Pos2::new(right_x, y)], stroke);

    // 3 Nodes: Input circle, Processing node, Output circle
    p.circle_filled(Pos2::new(left_x, y), 2.4, color);
    p.circle_filled(Pos2::new(mid_x, y), 3.2, color);
    p.circle_filled(Pos2::new(right_x, y), 2.4, color);
    p.line_segment(
        [
            Pos2::new(mid_x, y - r * 0.45),
            Pos2::new(mid_x, y + r * 0.45),
        ],
        Stroke::new(1.4, color),
    );
}

/// Multi-zone audio icon representing multi-room output distribution.
pub fn draw_zones(ui: &Ui, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.42;
    let p = ui.painter();
    let stroke = Stroke::new(1.4, color);

    // Center hub speaker/source
    p.circle_filled(c, 2.8, color);

    // Left and right concentric distribution lines
    p.line_segment(
        [
            Pos2::new(c.x - r * 0.7, c.y - r * 0.5),
            Pos2::new(c.x - r * 0.7, c.y + r * 0.5),
        ],
        stroke,
    );
    p.line_segment(
        [
            Pos2::new(c.x + r * 0.7, c.y - r * 0.5),
            Pos2::new(c.x + r * 0.7, c.y + r * 0.5),
        ],
        stroke,
    );
    p.line_segment(
        [
            Pos2::new(c.x - r * 0.35, c.y),
            Pos2::new(c.x + r * 0.35, c.y),
        ],
        stroke,
    );
}
