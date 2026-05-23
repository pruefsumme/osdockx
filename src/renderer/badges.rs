use super::{add_stop, rounded_rect};
use crate::layout::Rect;
use crate::theme::Color;
use gtk::cairo::{Context, FontSlant, FontWeight, LinearGradient};

pub(super) fn draw_badge(cr: &Context, rect: Rect, count: u32, color: Color, alpha: f64) {
    let alpha = alpha.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    let text = count.min(99).to_string();
    let width = 19.0_f64.max(12.0 + text.len() as f64 * 7.2);
    let height = 17.0;
    let x = rect.x + rect.width - width * 0.76;
    let y = rect.y + 3.0;

    cr.save().ok();
    rounded_rect(cr, x, y, width, height, height * 0.48);
    let fill = LinearGradient::new(0.0, y, 0.0, y + height);
    add_stop(
        &fill,
        0.00,
        color
            .mix(Color::rgba(1.0, 1.0, 1.0, 1.0), 0.22)
            .with_alpha(0.98 * alpha),
    );
    add_stop(&fill, 0.56, color.with_alpha(0.98 * alpha));
    add_stop(
        &fill,
        1.00,
        color
            .mix(Color::rgba(0.0, 0.0, 0.0, 1.0), 0.12)
            .with_alpha(0.98 * alpha),
    );
    let _ = cr.set_source(&fill);
    let _ = cr.fill_preserve();
    cr.set_line_width(1.0);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.64 * alpha);
    let _ = cr.stroke();

    cr.save().ok();
    rounded_rect(
        cr,
        x + 1.2,
        y + 1.0,
        width - 2.4,
        height * 0.38,
        height * 0.20,
    );
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.20 * alpha);
    let _ = cr.fill();
    cr.restore().ok();

    cr.select_font_face("Sans", FontSlant::Normal, FontWeight::Bold);
    cr.set_font_size(11.0);
    let extents = cr.text_extents(&text).ok();
    cr.set_source_rgba(1.0, 1.0, 1.0, alpha);
    let text_x = extents
        .as_ref()
        .map(|e| x + width / 2.0 - (e.width() / 2.0 + e.x_bearing()))
        .unwrap_or(x + 6.0);
    let text_y = extents
        .as_ref()
        .map(|e| y + height / 2.0 - (e.height() / 2.0 + e.y_bearing()))
        .unwrap_or(y + 14.0);
    cr.move_to(text_x, text_y);
    let _ = cr.show_text(&text);
    cr.restore().ok();
}
