use super::super::add_stop;
use super::{compute_perspective_shelf_geometry, leopard_wedge_body_geometry};
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
    cr.move_to(center_x, top + 0.6);
    cr.line_to(center_x, bottom - 0.6);
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
        1.00,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.02, 0.03, 0.05, 1.0), 0.50)
            .with_alpha(0.12),
    );
    cr.move_to(center_x - 0.9, top + 1.0);
    cr.line_to(center_x - 0.9, bottom - 0.8);
    cr.set_line_width(1.0);
    let _ = cr.set_source(&shadow);
    let _ = cr.stroke();

    let highlight = LinearGradient::new(0.0, top, 0.0, bottom);
    add_stop(
        &highlight,
        0.00,
        theme
            .shelf_highlight
            .with_alpha(0.05 * theme.highlight_strength),
    );
    add_stop(
        &highlight,
        1.00,
        theme
            .shelf_highlight
            .with_alpha(0.10 * theme.highlight_strength),
    );
    cr.move_to(center_x + 0.8, top + 1.0);
    cr.line_to(center_x + 0.8, bottom - 0.8);
    cr.set_line_width(0.9);
    let _ = cr.set_source(&highlight);
    let _ = cr.stroke();

    cr.restore().ok();
}
