use super::super::add_stop;
use super::{
    compute_perspective_shelf_geometry, leopard_front_face_path, leopard_wedge_body_geometry,
    leopard_wedge_body_path,
};
use crate::layout::{DockSeparatorLayout, Rect};
use crate::theme::{Color, Theme};
use gtk::cairo::{Context, LinearGradient};

pub(crate) fn draw_shelf_section_separator(
    cr: &Context,
    shelf: &Rect,
    separator: &DockSeparatorLayout,
    theme: &Theme,
) {
    draw_leopard_separator(cr, shelf, separator, theme);
}

fn draw_leopard_separator(
    cr: &Context,
    shelf: &Rect,
    separator: &DockSeparatorLayout,
    theme: &Theme,
) {
    let geom = compute_perspective_shelf_geometry(shelf, theme);
    let body = leopard_wedge_body_geometry(shelf, theme);
    let center_x = separator.rect.x + separator.rect.width * 0.5;

    let top = geom.back_left.y + (geom.lip_y - geom.back_left.y) * 0.10;
    let bottom = body.face_left_bottom.y - shelf.height * 0.03;
    if bottom <= top {
        return;
    }

    cr.save().ok();
    leopard_wedge_body_path(cr, &geom, &body);
    cr.clip();

    let trench = LinearGradient::new(0.0, top, 0.0, bottom);
    add_stop(
        &trench,
        0.00,
        theme
            .shelf_top
            .mix(theme.shelf_stroke, 0.32)
            .mix(Color::rgba(0.0, 0.0, 0.0, 1.0), 0.18)
            .with_alpha(0.08),
    );
    add_stop(
        &trench,
        0.54,
        theme
            .shelf_bottom
            .mix(theme.shelf_stroke, 0.42)
            .mix(Color::rgba(0.0, 0.0, 0.0, 1.0), 0.38)
            .with_alpha(0.20),
    );
    add_stop(
        &trench,
        1.00,
        theme
            .shelf_bottom
            .mix(theme.shelf_stroke, 0.34)
            .mix(Color::rgba(0.0, 0.0, 0.0, 1.0), 0.30)
            .with_alpha(0.16),
    );
    cr.move_to(center_x, top + 0.6);
    cr.line_to(center_x, bottom - 0.6);
    cr.set_line_width(4.2);
    cr.set_line_cap(gtk::cairo::LineCap::Round);
    let _ = cr.set_source(&trench);
    let _ = cr.stroke();

    let shadow = LinearGradient::new(0.0, top, 0.0, bottom);
    add_stop(
        &shadow,
        0.00,
        theme
            .shelf_stroke
            .mix(Color::rgba(0.02, 0.03, 0.05, 1.0), 0.54)
            .with_alpha(0.18),
    );
    add_stop(
        &shadow,
        1.00,
        theme
            .shelf_stroke
            .mix(Color::rgba(0.02, 0.03, 0.05, 1.0), 0.48)
            .with_alpha(0.22),
    );
    cr.move_to(center_x - 1.15, top + 1.1);
    cr.line_to(center_x - 1.15, bottom - 0.8);
    cr.set_line_width(1.25);
    let _ = cr.set_source(&shadow);
    let _ = cr.stroke();

    let highlight = LinearGradient::new(0.0, top, 0.0, bottom);
    add_stop(
        &highlight,
        0.00,
        theme
            .shelf_highlight
            .mix(theme.shelf_top, 0.18)
            .with_alpha(0.09 * theme.highlight_strength),
    );
    add_stop(
        &highlight,
        1.00,
        theme
            .shelf_highlight
            .mix(theme.shelf_bottom, 0.14)
            .with_alpha(0.16 * theme.highlight_strength),
    );
    cr.move_to(center_x + 1.05, top + 1.1);
    cr.line_to(center_x + 1.05, bottom - 0.8);
    cr.set_line_width(1.05);
    let _ = cr.set_source(&highlight);
    let _ = cr.stroke();

    cr.restore().ok();

    cr.save().ok();
    leopard_front_face_path(cr, &geom, &body);
    cr.clip();

    let lip_top = geom.lip_y + (bottom - geom.lip_y).max(1.0) * 0.08;
    let lip_bottom = bottom - 0.6;
    let lip_trench = LinearGradient::new(0.0, lip_top, 0.0, lip_bottom);
    add_stop(
        &lip_trench,
        0.00,
        theme
            .shelf_stroke
            .mix(theme.shelf_bottom, 0.30)
            .mix(Color::rgba(0.0, 0.0, 0.0, 1.0), 0.30)
            .with_alpha(0.18),
    );
    add_stop(
        &lip_trench,
        1.00,
        theme
            .shelf_stroke
            .mix(theme.shelf_bottom, 0.42)
            .mix(Color::rgba(0.0, 0.0, 0.0, 1.0), 0.22)
            .with_alpha(0.13),
    );
    cr.move_to(center_x, lip_top);
    cr.line_to(center_x, lip_bottom);
    cr.set_line_width(3.2);
    cr.set_line_cap(gtk::cairo::LineCap::Round);
    let _ = cr.set_source(&lip_trench);
    let _ = cr.stroke();

    cr.move_to(center_x + 0.9, lip_top + 0.3);
    cr.line_to(center_x + 0.9, lip_bottom - 0.2);
    cr.set_line_width(0.8);
    cr.set_source_rgba(
        theme.shelf_highlight.red,
        theme.shelf_highlight.green,
        theme.shelf_highlight.blue,
        0.13 * theme.highlight_strength,
    );
    let _ = cr.stroke();

    cr.restore().ok();
}
