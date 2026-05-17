use super::{add_stop, rounded_rect};
use crate::layout::{DockLayout, Rect};
use crate::theme::Theme;
use gtk::cairo::{Context, LinearGradient};

pub(super) fn leopard_running_indicator_size(active: bool) -> (f64, f64) {
    if active { (14.4, 4.15) } else { (10.4, 3.15) }
}

pub(super) fn draw_leopard_running_indicator(
    cr: &Context,
    rect: Rect,
    layout: &DockLayout,
    theme: &Theme,
    active: bool,
    alpha: f64,
) {
    let alpha = alpha.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    let y = leopard_running_indicator_center_y(&layout.shelf, theme);
    let (width, height) = leopard_running_indicator_size(active);
    let x = rect.center_x() - width / 2.0;
    let color = theme
        .shelf_highlight
        .mix(theme.indicator, if active { 0.42 } else { 0.26 });

    cr.save().ok();
    rounded_rect(
        cr,
        x - 2.1,
        y - height * 0.80,
        width + 4.2,
        height * 1.60,
        height * 0.92,
    );
    cr.set_source_rgba(
        theme.indicator.red,
        theme.indicator.green,
        theme.indicator.blue,
        (if active { 0.30 } else { 0.18 }) * alpha,
    );
    let _ = cr.fill();
    cr.restore().ok();

    cr.save().ok();
    rounded_rect(
        cr,
        x - 0.9,
        y - height * 0.56,
        width + 1.8,
        height * 1.12,
        height * 0.64,
    );
    cr.set_source_rgba(1.0, 1.0, 1.0, (if active { 0.24 } else { 0.14 }) * alpha);
    let _ = cr.fill();
    cr.restore().ok();

    cr.save().ok();
    rounded_rect(cr, x, y - height / 2.0, width, height, height / 2.0);
    let fill = LinearGradient::new(0.0, y - height / 2.0, 0.0, y + height / 2.0);
    add_stop(
        &fill,
        0.00,
        theme
            .shelf_highlight
            .with_alpha((if active { 0.96 } else { 0.74 }) * alpha),
    );
    add_stop(
        &fill,
        0.24,
        color.with_alpha((if active { 1.0 } else { 0.88 }) * alpha),
    );
    add_stop(
        &fill,
        1.00,
        theme
            .shelf_highlight
            .mix(theme.indicator, if active { 0.54 } else { 0.34 })
            .with_alpha((if active { 0.98 } else { 0.78 }) * alpha),
    );
    let _ = cr.set_source(&fill);
    let _ = cr.fill_preserve();
    cr.set_line_width(0.7);
    cr.set_source_rgba(1.0, 1.0, 1.0, (if active { 0.62 } else { 0.36 }) * alpha);
    let _ = cr.stroke();
    cr.restore().ok();

    cr.save().ok();
    rounded_rect(
        cr,
        x + width * 0.18,
        y - height * 0.28,
        width * 0.64,
        height * 0.22,
        height * 0.20,
    );
    cr.set_source_rgba(1.0, 1.0, 1.0, (if active { 0.82 } else { 0.46 }) * alpha);
    let _ = cr.fill();
    cr.restore().ok();
}

pub(super) fn draw_leopard_active_indicator(
    cr: &Context,
    rect: Rect,
    layout: &DockLayout,
    theme: &Theme,
    alpha: f64,
) {
    let alpha = alpha.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    let y = leopard_active_indicator_center_y(rect, &layout.shelf, theme);
    let x = rect.center_x();
    let color = theme.indicator;

    cr.save().ok();
    cr.arc(x, y, 8.4, 0.0, std::f64::consts::TAU);
    cr.set_source_rgba(color.red, color.green, color.blue, 0.24 * alpha);
    let _ = cr.fill();
    cr.restore().ok();

    cr.save().ok();
    cr.arc(x, y, 4.7, 0.0, std::f64::consts::TAU);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.98 * alpha);
    let _ = cr.fill();
    cr.restore().ok();

    cr.save().ok();
    cr.arc(x, y, 2.6, 0.0, std::f64::consts::TAU);
    cr.set_source_rgba(color.red, color.green, color.blue, 0.92 * alpha);
    let _ = cr.fill();
    cr.restore().ok();
}

pub(super) fn leopard_running_indicator_center_y(shelf: &Rect, theme: &Theme) -> f64 {
    let geom = super::compute_perspective_shelf_geometry(shelf, theme);
    let face_height = geom.bottom_y - geom.lip_y;
    let inner_margin = (face_height * 0.28).clamp(0.9, 1.8);
    (geom.lip_y + face_height * 0.50).clamp(geom.lip_y + inner_margin, geom.bottom_y - inner_margin)
}

pub(super) fn leopard_active_indicator_center_y(rect: Rect, shelf: &Rect, theme: &Theme) -> f64 {
    let geom = super::compute_perspective_shelf_geometry(shelf, theme);
    let shelf_drop = geom.bottom_y + (shelf.height * (0.15 + theme.depth * 0.01)).clamp(6.8, 8.8);
    let icon_drop = rect.y + rect.height + (rect.height * 0.14).clamp(7.0, 10.0);
    shelf_drop.max(icon_drop)
}
