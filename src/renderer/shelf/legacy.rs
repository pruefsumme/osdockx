use crate::layout::Rect;
use crate::theme::{Color, Theme};
use gtk::cairo::{Context, LinearGradient};

pub(crate) fn draw_legacy_shelf(cr: &Context, shelf: &Rect, theme: &Theme) {
    let slant = shelf.height * theme.shelf_slant_ratio;
    let horizon_y = shelf.y + shelf.height * 0.40;
    let bottom_y = shelf.y + shelf.height;
    cr.save().ok();

    cr.move_to(shelf.x + slant, shelf.y);
    cr.line_to(shelf.x + shelf.width - slant, shelf.y);
    cr.line_to(shelf.x + shelf.width, bottom_y);
    cr.line_to(shelf.x, bottom_y);
    cr.close_path();

    let base_gradient = LinearGradient::new(0.0, shelf.y, 0.0, bottom_y);
    super::super::add_stop(&base_gradient, 0.00, theme.shelf_top.with_alpha(0.96));
    super::super::add_stop(&base_gradient, 0.28, Color::rgba(0.88, 0.94, 0.98, 0.84));
    super::super::add_stop(&base_gradient, 0.52, theme.shelf_bottom.with_alpha(0.76));
    super::super::add_stop(&base_gradient, 1.00, Color::rgba(0.30, 0.38, 0.47, 0.90));
    let _ = cr.set_source(&base_gradient);
    let _ = cr.fill_preserve();
    cr.set_line_width(1.0);
    super::super::set_color(cr, theme.shelf_stroke.with_alpha(0.72));
    let _ = cr.stroke();

    cr.move_to(shelf.x + slant * 0.58, horizon_y);
    cr.line_to(shelf.x + shelf.width - slant * 0.58, horizon_y);
    cr.line_to(shelf.x + shelf.width - slant * 0.15, bottom_y - 1.0);
    cr.line_to(shelf.x + slant * 0.15, bottom_y - 1.0);
    cr.close_path();

    let face_gradient = LinearGradient::new(0.0, horizon_y, 0.0, bottom_y);
    super::super::add_stop(&face_gradient, 0.00, Color::rgba(0.72, 0.82, 0.91, 0.42));
    super::super::add_stop(&face_gradient, 0.55, Color::rgba(0.56, 0.67, 0.78, 0.38));
    super::super::add_stop(&face_gradient, 1.00, Color::rgba(0.18, 0.24, 0.31, 0.42));
    let _ = cr.set_source(&face_gradient);
    let _ = cr.fill();

    cr.move_to(shelf.x + slant, shelf.y);
    cr.line_to(shelf.x + shelf.width - slant, shelf.y);
    cr.set_line_width(1.4);
    super::super::set_color(cr, theme.shelf_highlight.with_alpha(0.90));
    let _ = cr.stroke();

    cr.move_to(shelf.x + slant * 0.65, horizon_y);
    cr.line_to(shelf.x + shelf.width - slant * 0.65, horizon_y);
    cr.set_line_width(1.0);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.34);
    let _ = cr.stroke();

    cr.move_to(shelf.x + 4.0, bottom_y - 1.0);
    cr.line_to(shelf.x + shelf.width - 4.0, bottom_y - 1.0);
    cr.set_line_width(1.0);
    cr.set_source_rgba(0.06, 0.08, 0.11, 0.42);
    let _ = cr.stroke();
    cr.restore().ok();
}