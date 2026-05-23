use super::super::add_stop;
use super::{compute_perspective_shelf_geometry, leopard_glass_plane_path};
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
    let center_x = separator.rect.x + separator.rect.width * 0.5;

    let top = geom.back_left.y + (geom.lip_y - geom.back_left.y) * 0.12;
    let bottom = geom.lip_y - shelf.height * 0.08;
    if bottom <= top {
        return;
    }

    cr.save().ok();
    leopard_glass_plane_path(cr, shelf, theme);
    cr.clip();

    let trench = LinearGradient::new(0.0, top, 0.0, bottom);
    add_stop(
        &trench,
        0.00,
        theme
            .shelf_top
            .mix(theme.shelf_stroke, 0.22)
            .mix(Color::rgba(0.0, 0.0, 0.0, 1.0), 0.20)
            .with_alpha(0.30),
    );
    add_stop(
        &trench,
        0.54,
        theme
            .shelf_bottom
            .mix(theme.shelf_stroke, 0.36)
            .mix(Color::rgba(0.0, 0.0, 0.0, 1.0), 0.40)
            .with_alpha(0.55),
    );
    add_stop(
        &trench,
        1.00,
        theme
            .shelf_bottom
            .mix(theme.shelf_stroke, 0.28)
            .mix(Color::rgba(0.0, 0.0, 0.0, 1.0), 0.28)
            .with_alpha(0.38),
    );
    cr.move_to(center_x, top + 0.3);
    cr.line_to(center_x, bottom);
    cr.set_line_width((separator.rect.width * 0.50).max(2.6));
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
            .with_alpha(0.20),
    );
    add_stop(
        &shadow,
        1.00,
        theme
            .shelf_stroke
            .mix(Color::rgba(0.02, 0.03, 0.05, 1.0), 0.48)
            .with_alpha(0.24),
    );
    cr.move_to(center_x - separator.rect.width * 0.10, top + 0.8);
    cr.line_to(center_x - separator.rect.width * 0.10, bottom - 0.2);
    cr.set_line_width(0.95);
    let _ = cr.set_source(&shadow);
    let _ = cr.stroke();

    let highlight = LinearGradient::new(0.0, top, 0.0, bottom);
    add_stop(
        &highlight,
        0.00,
        theme
            .shelf_highlight
            .mix(theme.shelf_top, 0.18)
            .with_alpha(0.24 * theme.highlight_strength),
    );
    add_stop(
        &highlight,
        1.00,
        theme
            .shelf_highlight
            .mix(theme.shelf_bottom, 0.14)
            .with_alpha(0.30 * theme.highlight_strength),
    );
    cr.move_to(center_x + separator.rect.width * 0.10, top + 0.8);
    cr.line_to(center_x + separator.rect.width * 0.10, bottom - 0.2);
    cr.set_line_width(0.85);
    let _ = cr.set_source(&highlight);
    let _ = cr.stroke();

    cr.restore().ok();
}
