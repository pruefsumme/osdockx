use crate::layout::Rect;
use crate::theme::{Color, Theme};
use gtk::cairo::Context;

pub(crate) fn draw_shadow(cr: &Context, shelf: &Rect, theme: &Theme) {
    cr.save().ok();
    let shadow_y = shelf.y + shelf.height * 0.82;
    let base_alpha = (0.07 * theme.shadow_strength).clamp(0.012, 0.040);
    let shelf_right = shelf.x + shelf.width;
    for pass in 0..6 {
        let grow = pass as f64 * shelf.height * 0.14;
        let x = (shelf.x + shelf.height * 0.10 - grow * 0.88).max(shelf.x);
        let right = (shelf.x + shelf.width - shelf.height * 0.10 + grow * 0.88).min(shelf_right);
        super::super::rounded_rect(
            cr,
            x,
            shadow_y - grow * 0.10,
            (right - x).max(0.0),
            shelf.height * 0.20 + grow * 0.32,
            shelf.height * 0.10 + grow * 0.24,
        );
        cr.set_source_rgba(0.0, 0.0, 0.0, base_alpha / (pass as f64 + 1.0));
        let _ = cr.fill();
    }
    cr.restore().ok();
}

pub(crate) fn fill_crystal_material<F>(
    cr: &Context,
    bounds: &Rect,
    base: Color,
    texture_strength: f64,
    path: F,
) where
    F: Fn(&Context),
{
    path(cr);
    super::super::set_color(cr, base);
    let _ = cr.fill();

    cr.save().ok();
    path(cr);
    cr.clip();
    draw_plank_texture(cr, bounds, base, texture_strength);
    cr.restore().ok();
}

fn draw_plank_texture(cr: &Context, bounds: &Rect, base: Color, strength: f64) {
    if strength <= 0.0 {
        return;
    }

    cr.save().ok();
    cr.set_line_width(1.0);
    let min_y = bounds.y.floor() as i32;
    let max_y = (bounds.y + bounds.height).ceil() as i32;
    for y in min_y..=max_y {
        let noise = (((y * 37 + 17).rem_euclid(23)) as f64 / 22.0) - 0.5;
        let mix = (noise.abs() * 0.032 + 0.010).min(0.035);
        let color = if noise >= 0.0 {
            base.mix(Color::rgba(1.0, 1.0, 1.0, 1.0), mix)
        } else {
            base.mix(Color::rgba(0.0, 0.0, 0.0, 1.0), mix)
        };
        super::super::set_color(cr, color.with_alpha(strength * (0.10 + noise.abs() * 0.08)));
        let yy = y as f64 + 0.5;
        cr.move_to(bounds.x, yy);
        cr.line_to(bounds.x + bounds.width, yy);
        let _ = cr.stroke();
    }
    cr.restore().ok();
}
