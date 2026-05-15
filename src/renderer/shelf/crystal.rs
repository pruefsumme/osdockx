use super::{
    crystal_floor_path, crystal_lip_path, crystal_shelf_geometry, crystal_side_path,
    crystal_top_path, fill_crystal_material,
};
use crate::layout::Rect;
use crate::theme::{Color, Theme};
use gtk::cairo::Context;

pub(crate) fn draw_crystal_shelf(cr: &Context, shelf: &Rect, theme: &Theme) {
    let geom = crystal_shelf_geometry(shelf, theme);
    cr.save().ok();

    let top_material = theme
        .shelf_top
        .mix(theme.shelf_bottom, 0.38)
        .with_alpha(1.0);
    fill_crystal_material(cr, shelf, top_material, 0.24, |cr| {
        crystal_top_path(cr, shelf, theme);
    });
    crystal_top_path(cr, shelf, theme);
    cr.set_line_width(1.0);
    super::super::set_color(cr, theme.shelf_stroke.with_alpha(0.86));
    let _ = cr.stroke();

    let face_material = theme
        .shelf_bottom
        .mix(theme.shelf_top, 0.10)
        .with_alpha(1.0);
    fill_crystal_material(cr, shelf, face_material, 0.18, |cr| {
        crystal_floor_path(cr, shelf, theme);
    });

    draw_crystal_side_facet(cr, shelf, theme, true);
    draw_crystal_side_facet(cr, shelf, theme, false);

    let lip_material = theme
        .shelf_bottom
        .mix(Color::rgba(0.02, 0.03, 0.04, 1.0), 0.56)
        .with_alpha(1.0);
    fill_crystal_material(cr, shelf, lip_material, 0.10, |cr| {
        crystal_lip_path(cr, shelf, theme);
    });

    cr.move_to(shelf.x + geom.slant, shelf.y + 0.7);
    cr.line_to(shelf.x + shelf.width - geom.slant, shelf.y + 0.7);
    cr.set_line_width(1.5);
    super::super::set_color(
        cr,
        theme
            .shelf_highlight
            .with_alpha(0.62 * theme.highlight_strength),
    );
    let _ = cr.stroke();

    cr.move_to(shelf.x + geom.slant * 0.48, geom.horizon_y);
    cr.line_to(shelf.x + shelf.width - geom.slant * 0.48, geom.horizon_y);
    cr.set_line_width(1.0);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.22 * theme.highlight_strength);
    let _ = cr.stroke();

    cr.move_to(shelf.x + 5.0, geom.bottom_y - 0.5);
    cr.line_to(shelf.x + shelf.width - 5.0, geom.bottom_y - 0.5);
    cr.set_line_width(1.0);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.64);
    let _ = cr.stroke();

    cr.restore().ok();
}

fn draw_crystal_side_facet(cr: &Context, shelf: &Rect, theme: &Theme, left: bool) {
    let side_material = theme
        .shelf_bottom
        .mix(Color::rgba(0.0, 0.0, 0.0, 1.0), 0.36)
        .with_alpha(1.0);
    fill_crystal_material(cr, shelf, side_material, 0.12, |cr| {
        crystal_side_path(cr, shelf, theme, left);
    });
}