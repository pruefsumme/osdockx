use super::super::add_stop;
use super::{compute_perspective_shelf_geometry, leopard_wedge_body_geometry};
use crate::config::ShelfStyle;
use crate::layout::{DockSeparatorLayout, Rect};
use crate::theme::{Color, Theme};
use gtk::cairo::{Context, LinearGradient};

pub(crate) fn draw_shelf_section_separator(
    cr: &Context,
    shelf: &Rect,
    separator: &DockSeparatorLayout,
    theme: &Theme,
) {
    if theme.shelf_style == ShelfStyle::LeopardPlank {
        draw_leopard_separator(cr, shelf, separator, theme);
        return;
    }

    let top = separator.rect.y.max(shelf.y + shelf.height * 0.08);
    let bottom = (separator.rect.y + separator.rect.height).min(shelf.y + shelf.height * 0.96);
    if bottom <= top {
        return;
    }

    let center_x = separator.rect.x + separator.rect.width / 2.0;
    let inner_left = center_x - 0.8;
    let inner_right = center_x + 0.8;

    cr.save().ok();
    cr.rectangle(
        shelf.x,
        shelf.y - shelf.height * 0.18,
        shelf.width,
        shelf.height * 1.24,
    );
    cr.clip();

    let cut_fill = LinearGradient::new(0.0, top, 0.0, bottom);
    add_stop(
        &cut_fill,
        0.00,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.0, 0.0, 0.0, 1.0), 0.18)
            .with_alpha(0.04),
    );
    add_stop(
        &cut_fill,
        0.44,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.0, 0.0, 0.0, 1.0), 0.42)
            .with_alpha(0.12),
    );
    add_stop(
        &cut_fill,
        1.00,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.0, 0.0, 0.0, 1.0), 0.26)
            .with_alpha(0.08),
    );
    cr.rectangle(
        separator.rect.x - 1.2,
        top,
        separator.rect.width + 2.4,
        bottom - top,
    );
    let _ = cr.set_source(&cut_fill);
    let _ = cr.fill();

    let shadow = LinearGradient::new(0.0, top, 0.0, bottom);
    add_stop(
        &shadow,
        0.00,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.02, 0.03, 0.05, 1.0), 0.42)
            .with_alpha(0.08),
    );
    add_stop(
        &shadow,
        0.50,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.02, 0.03, 0.05, 1.0), 0.58)
            .with_alpha(0.22),
    );
    add_stop(
        &shadow,
        1.00,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.02, 0.03, 0.05, 1.0), 0.46)
            .with_alpha(0.10),
    );
    cr.move_to(inner_left, top + 1.0);
    cr.line_to(inner_left, bottom - 1.0);
    cr.set_line_width(1.1);
    let _ = cr.set_source(&shadow);
    let _ = cr.stroke();

    let highlight = LinearGradient::new(0.0, top, 0.0, bottom);
    add_stop(
        &highlight,
        0.00,
        theme.shelf_highlight.with_alpha(0.06 * theme.highlight_strength),
    );
    add_stop(
        &highlight,
        0.50,
        theme.shelf_highlight.with_alpha(0.24 * theme.highlight_strength),
    );
    add_stop(
        &highlight,
        1.00,
        theme.shelf_highlight.with_alpha(0.10 * theme.highlight_strength),
    );
    cr.move_to(inner_right, top + 1.0);
    cr.line_to(inner_right, bottom - 1.0);
    cr.set_line_width(1.0);
    let _ = cr.set_source(&highlight);
    let _ = cr.stroke();

    cr.restore().ok();
}

fn draw_leopard_separator(
    cr: &Context,
    shelf: &Rect,
    separator: &DockSeparatorLayout,
    theme: &Theme,
) {
    let geom = compute_perspective_shelf_geometry(shelf, theme);
    let body = leopard_wedge_body_geometry(shelf, theme);
    let center_x = separator.rect.x + separator.rect.width / 2.0;
    let front_width = (geom.lip_right.x - geom.lip_left.x).max(1.0);
    let section_t = ((center_x - geom.lip_left.x) / front_width).clamp(0.0, 1.0);
    let top_center_x = lerp(geom.back_left.x, geom.back_right.x, section_t);
    let lip_center_x = lerp(geom.lip_left.x, geom.lip_right.x, section_t);
    let bottom_center_x = lerp(body.face_left_bottom.x, body.face_right_bottom.x, section_t);
    let top = geom.back_left.y + (geom.lip_y - geom.back_left.y) * 0.10;
    let lip_y = geom.lip_y;
    let bottom = body.face_left_bottom.y - shelf.height * 0.03;
    if bottom <= top {
        return;
    }

    cr.save().ok();

    let trench = LinearGradient::new(0.0, top, 0.0, bottom);
    add_stop(
        &trench,
        0.00,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.0, 0.0, 0.0, 1.0), 0.22)
            .with_alpha(0.03),
    );
    add_stop(
        &trench,
        0.36,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.0, 0.0, 0.0, 1.0), 0.38)
            .with_alpha(0.07),
    );
    add_stop(
        &trench,
        0.70,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.0, 0.0, 0.0, 1.0), 0.46)
            .with_alpha(0.11),
    );
    add_stop(
        &trench,
        1.00,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.0, 0.0, 0.0, 1.0), 0.30)
            .with_alpha(0.08),
    );
    cr.move_to(top_center_x, top + 0.6);
    cr.line_to(lip_center_x, lip_y);
    cr.line_to(bottom_center_x, bottom - 0.6);
    cr.set_line_width(3.0);
    let _ = cr.set_source(&trench);
    let _ = cr.stroke();

    let shadow = LinearGradient::new(0.0, top, 0.0, bottom);
    add_stop(
        &shadow,
        0.00,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.02, 0.03, 0.05, 1.0), 0.48)
            .with_alpha(0.10),
    );
    add_stop(
        &shadow,
        0.52,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.02, 0.03, 0.05, 1.0), 0.60)
            .with_alpha(0.24),
    );
    add_stop(
        &shadow,
        1.00,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.02, 0.03, 0.05, 1.0), 0.50)
            .with_alpha(0.12),
    );
    cr.move_to(top_center_x - 0.9, top + 1.0);
    cr.line_to(lip_center_x - 0.9, lip_y);
    cr.line_to(bottom_center_x - 0.9, bottom - 0.8);
    cr.set_line_width(1.0);
    let _ = cr.set_source(&shadow);
    let _ = cr.stroke();

    let highlight = LinearGradient::new(0.0, top, 0.0, bottom);
    add_stop(
        &highlight,
        0.00,
        theme.shelf_highlight.with_alpha(0.05 * theme.highlight_strength),
    );
    add_stop(
        &highlight,
        0.42,
        theme.shelf_highlight.with_alpha(0.14 * theme.highlight_strength),
    );
    add_stop(
        &highlight,
        0.78,
        theme.shelf_highlight.with_alpha(0.18 * theme.highlight_strength),
    );
    add_stop(
        &highlight,
        1.00,
        theme.shelf_highlight.with_alpha(0.10 * theme.highlight_strength),
    );
    cr.move_to(top_center_x + 0.8, top + 1.0);
    cr.line_to(lip_center_x + 0.8, lip_y);
    cr.line_to(bottom_center_x + 0.8, bottom - 0.8);
    cr.set_line_width(0.9);
    let _ = cr.set_source(&highlight);
    let _ = cr.stroke();

    cr.restore().ok();
}

fn lerp(start: f64, end: f64, t: f64) -> f64 {
    start + (end - start) * t
}