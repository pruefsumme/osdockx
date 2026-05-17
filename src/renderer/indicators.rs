use super::{add_stop, rounded_rect};
use crate::layout::{DockLayout, Rect};
use crate::theme::Theme;
use gtk::cairo::{Context, LinearGradient};

pub(super) fn leopard_running_indicator_size(active: bool) -> (f64, f64) {
    if active { (15.6, 3.2) } else { (11.2, 2.6) }
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
    let geom = super::compute_perspective_shelf_geometry(&layout.shelf, theme);
    let face_height = (geom.bottom_y - geom.lip_y).max(1.0);
    let height = (face_height * if active { 0.68 } else { 0.54 }).clamp(2.2, 5.0);
    let width = (rect.width * if active { 0.17 } else { 0.12 }).clamp(
        if active { 15.6 } else { 11.2 },
        if active { 24.0 } else { 18.0 },
    );
    let x = rect.center_x() - width / 2.0;
    let color = theme
        .shelf_highlight
        .mix(theme.indicator, if active { 0.42 } else { 0.26 });

    draw_lit_led_bar(
        cr,
        &layout.shelf,
        x,
        y - height / 2.0,
        width,
        height,
        color,
        theme,
        active,
        alpha,
    );
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
    let geom = super::compute_perspective_shelf_geometry(&layout.shelf, theme);
    let face_height = (geom.bottom_y - geom.lip_y).max(1.0);
    let height = (face_height * 0.76).clamp(2.4, 5.4);
    let width = (rect.width * 0.21).clamp(17.0, 28.0);
    let y = leopard_active_indicator_center_y(rect, &layout.shelf, theme);
    let x = rect.center_x() - width / 2.0;
    let color = theme.shelf_highlight.mix(theme.indicator, 0.62);

    draw_lit_led_bar(
        cr,
        &layout.shelf,
        x,
        y - height / 2.0,
        width,
        height,
        color,
        theme,
        true,
        alpha,
    );
}

fn draw_lit_led_bar(
    cr: &Context,
    shelf: &Rect,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    color: crate::theme::Color,
    theme: &Theme,
    active: bool,
    alpha: f64,
) {
    let radius = (height * 0.42).clamp(1.0, 2.4);
    let geom = super::compute_perspective_shelf_geometry(shelf, theme);
    let body = super::leopard_wedge_body_geometry(shelf, theme);

    cr.save().ok();
    super::leopard_front_face_path(cr, &geom, &body);
    cr.clip();

    rounded_rect(
        cr,
        x - width * 0.56,
        y - height * 1.42,
        width * 2.12,
        height * 3.34,
        radius * 3.8,
    );
    cr.set_source_rgba(
        theme.indicator.red,
        theme.indicator.green,
        theme.indicator.blue,
        (if active { 0.26 } else { 0.14 }) * alpha,
    );
    let _ = cr.fill();

    rounded_rect(
        cr,
        x - width * 0.22,
        y - height * 0.68,
        width * 1.44,
        height * 2.18,
        radius * 2.4,
    );
    cr.set_source_rgba(
        theme.indicator.red,
        theme.indicator.green,
        theme.indicator.blue,
        (if active { 0.48 } else { 0.24 }) * alpha,
    );
    let _ = cr.fill();
    cr.restore().ok();

    cr.save().ok();
    super::leopard_front_face_path(cr, &geom, &body);
    cr.clip();

    rounded_rect(
        cr,
        x - width * 0.03,
        y - height * 0.04,
        width * 1.06,
        height * 1.12,
        radius * 1.1,
    );
    cr.set_source_rgba(0.0, 0.0, 0.0, (if active { 0.24 } else { 0.16 }) * alpha);
    let _ = cr.fill();

    rounded_rect(cr, x, y, width, height, radius);
    let fill = LinearGradient::new(0.0, y, 0.0, y + height);
    add_stop(
        &fill,
        0.00,
        theme
            .shelf_highlight
            .mix(theme.indicator, 0.28)
            .with_alpha((if active { 0.98 } else { 0.72 }) * alpha),
    );
    add_stop(
        &fill,
        0.42,
        theme
            .indicator
            .mix(color, 0.18)
            .with_alpha((if active { 1.0 } else { 0.86 }) * alpha),
    );
    add_stop(
        &fill,
        1.00,
        theme
            .indicator
            .mix(crate::theme::Color::rgba(0.0, 0.08, 0.12, 1.0), 0.18)
            .with_alpha((if active { 0.98 } else { 0.70 }) * alpha),
    );
    let _ = cr.set_source(&fill);
    let _ = cr.fill_preserve();
    cr.set_line_width(0.65);
    cr.set_source_rgba(1.0, 1.0, 1.0, (if active { 0.76 } else { 0.42 }) * alpha);
    let _ = cr.stroke();

    rounded_rect(
        cr,
        x + width * 0.13,
        y + height * 0.16,
        width * 0.74,
        height * 0.20,
        radius * 0.55,
    );
    cr.set_source_rgba(1.0, 1.0, 1.0, (if active { 0.92 } else { 0.52 }) * alpha);
    let _ = cr.fill();

    rounded_rect(
        cr,
        x + width * 0.10,
        y + height * 0.48,
        width * 0.80,
        height * 0.24,
        radius * 0.65,
    );
    cr.set_source_rgba(
        theme.indicator.red,
        theme.indicator.green,
        theme.indicator.blue,
        (if active { 0.96 } else { 0.50 }) * alpha,
    );
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
    let _ = rect;
    let face_height = geom.bottom_y - geom.lip_y;
    let inner_margin = (face_height * 0.28).clamp(0.9, 1.8);
    (geom.lip_y + face_height * 0.50).clamp(geom.lip_y + inner_margin, geom.bottom_y - inner_margin)
}
