use super::{add_stop, rounded_rect};
use crate::layout::{DockLayout, Rect};
use crate::theme::{Color, Theme};
use gtk::cairo::{Context, LinearGradient};

pub(super) fn leopard_running_indicator_size(active: bool) -> (f64, f64) {
    if active { (24.0, 5.0) } else { (19.0, 4.2) }
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
    let metrics = leopard_indicator_metrics(rect, &layout.shelf, theme, active);
    draw_glowing_lip_indicator(cr, &layout.shelf, theme, metrics, active, alpha);
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
    let metrics = leopard_indicator_metrics(rect, &layout.shelf, theme, true);
    draw_glowing_lip_indicator(cr, &layout.shelf, theme, metrics, true, alpha);
}

#[derive(Debug, Clone, Copy)]
struct LeopardIndicatorMetrics {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    radius: f64,
}

fn leopard_indicator_metrics(
    rect: Rect,
    shelf: &Rect,
    theme: &Theme,
    active: bool,
) -> LeopardIndicatorMetrics {
    let geom = super::compute_perspective_shelf_geometry(shelf, theme);
    let face_height = (geom.bottom_y - geom.lip_y).max(1.0);
    let height = (face_height * if active { 0.72 } else { 0.58 }).clamp(
        if active { 3.6 } else { 3.0 },
        if active { 4.6 } else { 4.0 },
    );
    let width = (rect.width * if active { 0.22 } else { 0.16 }).clamp(
        if active { 23.0 } else { 16.0 },
        if active { 29.0 } else { 23.0 },
    );
    LeopardIndicatorMetrics {
        x: rect.center_x() - width / 2.0,
        y: geom.bottom_y - height - (face_height * 0.08).clamp(0.4, 1.1),
        width,
        height,
        radius: (height * 0.42).clamp(1.2, 2.2),
    }
}

fn draw_glowing_lip_indicator(
    cr: &Context,
    shelf: &Rect,
    theme: &Theme,
    metrics: LeopardIndicatorMetrics,
    active: bool,
    alpha: f64,
) {
    let geom = super::compute_perspective_shelf_geometry(shelf, theme);
    let body = super::leopard_wedge_body_geometry(shelf, theme);
    let electric = Color::rgba(0.42, 0.86, 1.0, 1.0);
    let hot = Color::rgba(0.96, 1.0, 1.0, 1.0);
    let core = theme.indicator.mix(hot, 0.78);
    let lower_blue = theme.indicator.mix(electric, 0.34).mix(hot, 0.48);
    let strength = if active { 1.0 } else { 0.76 };

    cr.save().ok();
    super::leopard_front_face_path(cr, &geom, &body);
    cr.clip();

    rounded_rect(
        cr,
        metrics.x - metrics.width * 0.26,
        metrics.y - metrics.height * 0.86,
        metrics.width * 1.52,
        metrics.height * 2.48,
        metrics.height * 1.10,
    );
    cr.set_source_rgba(
        electric.red,
        electric.green,
        electric.blue,
        0.17 * strength * alpha,
    );
    let _ = cr.fill();

    rounded_rect(
        cr,
        metrics.x - metrics.width * 0.08,
        metrics.y - metrics.height * 0.34,
        metrics.width * 1.16,
        metrics.height * 1.58,
        metrics.height * 0.70,
    );
    cr.set_source_rgba(hot.red, hot.green, hot.blue, 0.38 * strength * alpha);
    let _ = cr.fill();

    rounded_rect(
        cr,
        metrics.x - 0.45,
        metrics.y - 0.15,
        metrics.width + 0.90,
        metrics.height + 0.35,
        metrics.radius + 0.4,
    );
    cr.set_source_rgba(0.0, 0.04, 0.08, 0.08 * strength * alpha);
    let _ = cr.fill();

    rounded_rect(
        cr,
        metrics.x,
        metrics.y,
        metrics.width,
        metrics.height,
        metrics.radius,
    );
    let fill = LinearGradient::new(0.0, metrics.y, 0.0, metrics.y + metrics.height);
    add_stop(&fill, 0.00, hot.with_alpha(1.00 * strength * alpha));
    add_stop(
        &fill,
        0.58,
        hot.mix(core, 0.16).with_alpha(1.00 * strength * alpha),
    );
    add_stop(&fill, 1.00, lower_blue.with_alpha(0.72 * strength * alpha));
    let _ = cr.set_source(&fill);
    let _ = cr.fill_preserve();
    cr.set_line_width(0.55);
    cr.set_source_rgba(0.98, 1.0, 1.0, 0.86 * strength * alpha);
    let _ = cr.stroke();

    rounded_rect(
        cr,
        metrics.x + metrics.width * 0.14,
        metrics.y + metrics.height * 0.18,
        metrics.width * 0.72,
        metrics.height * 0.24,
        metrics.radius * 0.55,
    );
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.92 * strength * alpha);
    let _ = cr.fill();

    rounded_rect(
        cr,
        metrics.x + metrics.width * 0.12,
        metrics.y + metrics.height * 0.58,
        metrics.width * 0.76,
        metrics.height * 0.24,
        metrics.radius * 0.70,
    );
    cr.set_source_rgba(
        electric.red,
        electric.green,
        electric.blue,
        0.18 * strength * alpha,
    );
    let _ = cr.fill();
    cr.restore().ok();
}

pub(super) fn leopard_running_indicator_center_y(shelf: &Rect, theme: &Theme) -> f64 {
    let geom = super::compute_perspective_shelf_geometry(shelf, theme);
    let face_height = geom.bottom_y - geom.lip_y;
    geom.lip_y + face_height * 0.50
}

pub(super) fn leopard_active_indicator_center_y(rect: Rect, shelf: &Rect, theme: &Theme) -> f64 {
    let geom = super::compute_perspective_shelf_geometry(shelf, theme);
    let _ = rect;
    let face_height = geom.bottom_y - geom.lip_y;
    geom.lip_y + face_height * 0.50
}
