use super::super::{add_stop, set_color};
use super::{
    LeopardWedgeBodyGeometry, PerspectiveShelfGeometry, compute_perspective_shelf_geometry,
    fill_crystal_material, leopard_front_face_path, leopard_glass_plane_path,
    leopard_wedge_body_geometry, leopard_wedge_body_path,
};
use crate::layout::Rect;
use crate::theme::{Color, Theme};
use gtk::cairo::{Context, LinearGradient};

pub(crate) fn draw_glass_shelf_base(cr: &Context, shelf: &Rect, theme: &Theme) {
    let geom = compute_perspective_shelf_geometry(shelf, theme);

    draw_leopard_undercarriage(cr, shelf, theme);

    fill_crystal_material(
        cr,
        shelf,
        theme
            .shelf_top
            .mix(theme.shelf_bottom, 0.30)
            .with_alpha(0.64 + theme.floor_opacity * 0.10),
        0.0006 + theme.material_roughness * 0.0012,
        |cr| {
            leopard_glass_plane_path(cr, shelf, theme);
        },
    );

    cr.save().ok();
    leopard_glass_plane_path(cr, shelf, theme);
    cr.clip();

    let glass = LinearGradient::new(0.0, geom.lip_left.y, 0.0, geom.back_left.y);
    add_stop(
        &glass,
        0.00,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.58, 0.67, 0.78, 1.0), 0.18)
            .with_alpha(0.62),
    );
    add_stop(
        &glass,
        0.32,
        theme
            .shelf_top
            .mix(theme.shelf_bottom, 0.34)
            .mix(theme.shelf_highlight, 0.06)
            .with_alpha(0.52),
    );
    add_stop(
        &glass,
        0.72,
        theme
            .shelf_top
            .mix(theme.shelf_highlight, 0.12)
            .with_alpha(0.42),
    );
    add_stop(
        &glass,
        1.00,
        theme
            .shelf_highlight
            .mix(theme.shelf_top, 0.22)
            .with_alpha(0.34),
    );
    let _ = cr.set_source(&glass);
    let _ = cr.paint();

    let center = LinearGradient::new(geom.back_left.x, 0.0, geom.back_right.x, 0.0);
    center.add_color_stop_rgba(0.00, 0.88, 0.92, 0.96, 0.0);
    center.add_color_stop_rgba(0.26, 0.88, 0.92, 0.96, 0.010);
    center.add_color_stop_rgba(0.50, 0.88, 0.92, 0.96, 0.022);
    center.add_color_stop_rgba(0.74, 0.88, 0.92, 0.96, 0.010);
    center.add_color_stop_rgba(1.00, 0.88, 0.92, 0.96, 0.0);
    let _ = cr.set_source(&center);
    let _ = cr.paint();

    let edge_vignette = LinearGradient::new(geom.back_left.x, 0.0, geom.back_right.x, 0.0);
    edge_vignette.add_color_stop_rgba(0.00, 0.24, 0.31, 0.40, 0.018);
    edge_vignette.add_color_stop_rgba(0.12, 0.24, 0.31, 0.40, 0.006);
    edge_vignette.add_color_stop_rgba(0.50, 1.0, 1.0, 1.0, 0.0);
    edge_vignette.add_color_stop_rgba(0.88, 0.24, 0.31, 0.40, 0.006);
    edge_vignette.add_color_stop_rgba(1.00, 0.24, 0.31, 0.40, 0.018);
    let _ = cr.set_source(&edge_vignette);
    let _ = cr.paint();

    let front_gloss = LinearGradient::new(0.0, geom.back_left.y, 0.0, geom.lip_left.y);
    front_gloss.add_color_stop_rgba(0.00, 0.90, 0.94, 0.98, 0.0);
    front_gloss.add_color_stop_rgba(0.44, 0.90, 0.94, 0.98, 0.022);
    front_gloss.add_color_stop_rgba(0.82, 0.90, 0.94, 0.98, 0.060);
    front_gloss.add_color_stop_rgba(1.00, 0.90, 0.94, 0.98, 0.0);
    let _ = cr.set_source(&front_gloss);
    let _ = cr.paint();
    cr.restore().ok();
}

fn draw_leopard_undercarriage(cr: &Context, shelf: &Rect, theme: &Theme) {
    let geom = compute_perspective_shelf_geometry(shelf, theme);
    let body = leopard_wedge_body_geometry(shelf, theme);

    draw_leopard_connector_wall(
        cr,
        shelf,
        theme,
        &geom,
        &body,
        geom.back_left.y,
        body.face_left_bottom.y,
    );
}

fn draw_leopard_connector_wall(
    cr: &Context,
    shelf: &Rect,
    theme: &Theme,
    geom: &PerspectiveShelfGeometry,
    body: &LeopardWedgeBodyGeometry,
    top_y: f64,
    bottom_y: f64,
) {
    if bottom_y <= top_y {
        return;
    }

    let face_height = (bottom_y - geom.lip_y).max(1.0);

    let base = theme
        .shelf_top
        .mix(theme.shelf_bottom, 0.44)
        .with_alpha(0.88 + theme.floor_opacity * 0.06);

    fill_crystal_material(
        cr,
        shelf,
        base,
        0.0007 + theme.material_roughness * 0.0012,
        |cr| {
            leopard_wedge_body_path(cr, geom, body);
        },
    );

    cr.save().ok();
    leopard_wedge_body_path(cr, geom, body);
    cr.clip();

    let wall = LinearGradient::new(0.0, geom.lip_y - face_height * 0.16, 0.0, bottom_y);
    add_stop(
        &wall,
        0.00,
        theme
            .shelf_top
            .mix(theme.shelf_bottom, 0.34)
            .mix(Color::rgba(0.84, 0.90, 0.96, 1.0), 0.18)
            .with_alpha(0.46),
    );
    add_stop(
        &wall,
        0.18,
        theme
            .shelf_top
            .mix(theme.shelf_bottom, 0.40)
            .with_alpha(0.66),
    );
    add_stop(
        &wall,
        0.56,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.48, 0.58, 0.70, 1.0), 0.36)
            .with_alpha(0.84),
    );
    add_stop(
        &wall,
        1.00,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.24, 0.31, 0.40, 1.0), 0.42)
            .with_alpha(0.99),
    );
    let _ = cr.set_source(&wall);
    let _ = cr.paint();

    let front_curve = LinearGradient::new(
        0.0,
        geom.lip_y - face_height * 0.10,
        0.0,
        geom.lip_y + face_height * 0.56,
    );
    front_curve.add_color_stop_rgba(0.00, 0.88, 0.92, 0.96, 0.0);
    front_curve.add_color_stop_rgba(0.16, 0.88, 0.92, 0.96, 0.10);
    front_curve.add_color_stop_rgba(0.28, 0.88, 0.92, 0.96, 0.13);
    front_curve.add_color_stop_rgba(0.54, 0.88, 0.92, 0.96, 0.030);
    front_curve.add_color_stop_rgba(1.00, 0.88, 0.92, 0.96, 0.0);
    let _ = cr.set_source(&front_curve);
    let _ = cr.paint();

    let cap_shadow = LinearGradient::new(geom.lip_left.x, 0.0, geom.lip_right.x, 0.0);
    add_stop(
        &cap_shadow,
        0.00,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.08, 0.11, 0.16, 1.0), 0.34)
            .with_alpha(0.18),
    );
    add_stop(
        &cap_shadow,
        0.12,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.08, 0.11, 0.16, 1.0), 0.24)
            .with_alpha(0.08),
    );
    add_stop(&cap_shadow, 0.24, theme.shelf_bottom.with_alpha(0.0));
    add_stop(&cap_shadow, 0.76, theme.shelf_bottom.with_alpha(0.0));
    add_stop(
        &cap_shadow,
        0.88,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.08, 0.11, 0.16, 1.0), 0.24)
            .with_alpha(0.08),
    );
    add_stop(
        &cap_shadow,
        1.00,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.08, 0.11, 0.16, 1.0), 0.34)
            .with_alpha(0.18),
    );
    let _ = cr.set_source(&cap_shadow);
    let _ = cr.paint();

    let sheen = LinearGradient::new(0.0, top_y, 0.0, bottom_y);
    sheen.add_color_stop_rgba(0.00, 0.85, 0.87, 0.90, 0.06);
    sheen.add_color_stop_rgba(0.20, 0.85, 0.87, 0.90, 0.025);
    sheen.add_color_stop_rgba(1.00, 0.85, 0.87, 0.90, 0.0);
    let _ = cr.set_source(&sheen);
    let _ = cr.paint();
    cr.restore().ok();
}

pub(crate) fn draw_glass_highlight_overlay(cr: &Context, shelf: &Rect, theme: &Theme) {
    let geom = compute_perspective_shelf_geometry(shelf, theme);

    cr.save().ok();
    leopard_glass_plane_path(cr, shelf, theme);
    cr.clip();

    let band_y = geom.back_left.y + (geom.lip_left.y - geom.back_left.y) * 0.54;
    let band = LinearGradient::new(
        0.0,
        band_y - shelf.height * 0.18,
        0.0,
        band_y + shelf.height * 0.24,
    );
    band.add_color_stop_rgba(0.00, 0.85, 0.87, 0.90, 0.0);
    band.add_color_stop_rgba(0.48, 0.85, 0.87, 0.90, 0.075 * theme.highlight_strength);
    band.add_color_stop_rgba(1.00, 0.85, 0.87, 0.90, 0.0);
    let _ = cr.set_source(&band);
    let _ = cr.paint();

    let top_sheen = LinearGradient::new(geom.back_left.x, 0.0, geom.back_right.x, 0.0);
    top_sheen.add_color_stop_rgba(0.00, 0.85, 0.87, 0.90, 0.0);
    top_sheen.add_color_stop_rgba(0.50, 0.85, 0.87, 0.90, 0.010 * theme.highlight_strength);
    top_sheen.add_color_stop_rgba(1.00, 0.85, 0.87, 0.90, 0.0);
    let _ = cr.set_source(&top_sheen);
    let _ = cr.paint();
    cr.restore().ok();
}

pub(crate) fn draw_leopard_shelf_strokes(cr: &Context, shelf: &Rect, theme: &Theme) {
    let geom = compute_perspective_shelf_geometry(shelf, theme);
    let body = leopard_wedge_body_geometry(shelf, theme);

    leopard_glass_plane_path(cr, shelf, theme);
    cr.set_line_width(1.05);
    cr.set_line_join(gtk::cairo::LineJoin::Round);
    let side_fade = LinearGradient::new(geom.front_left.x, 0.0, geom.front_right.x, 0.0);
    add_stop(&side_fade, 0.00, theme.shelf_stroke.with_alpha(0.0));
    add_stop(&side_fade, 0.14, theme.shelf_stroke.with_alpha(0.018));
    add_stop(&side_fade, 0.50, theme.shelf_stroke.with_alpha(0.044));
    add_stop(&side_fade, 0.86, theme.shelf_stroke.with_alpha(0.018));
    add_stop(&side_fade, 1.00, theme.shelf_stroke.with_alpha(0.0));
    let _ = cr.set_source(&side_fade);
    let _ = cr.stroke();

    leopard_glass_plane_path(cr, shelf, theme);
    cr.set_line_width(0.65);
    set_color(
        cr,
        theme
            .shelf_highlight
            .with_alpha(0.08 * theme.highlight_strength),
    );
    let _ = cr.stroke();

    leopard_wedge_body_path(cr, &geom, &body);
    cr.set_line_width(1.0);
    set_color(
        cr,
        theme
            .shelf_stroke
            .mix(Color::rgba(0.0, 0.0, 0.0, 1.0), 0.18)
            .with_alpha(0.18),
    );
    let _ = cr.stroke();
}

pub(crate) fn draw_front_lip(cr: &Context, shelf: &Rect, theme: &Theme) {
    let geom = compute_perspective_shelf_geometry(shelf, theme);
    let body = leopard_wedge_body_geometry(shelf, theme);

    cr.move_to(geom.lip_left.x + 0.4, geom.lip_left.y + 0.46);
    cr.line_to(geom.lip_right.x - 0.4, geom.lip_right.y + 0.46);
    cr.set_line_width(1.55);
    let lip_highlight = LinearGradient::new(geom.lip_left.x, 0.0, geom.lip_right.x, 0.0);
    add_stop(&lip_highlight, 0.00, theme.shelf_highlight.with_alpha(0.0));
    add_stop(&lip_highlight, 0.12, theme.shelf_highlight.with_alpha(0.12));
    add_stop(
        &lip_highlight,
        0.50,
        theme
            .shelf_highlight
            .mix(theme.shelf_top, 0.12)
            .with_alpha(0.24 * theme.highlight_strength + 0.10),
    );
    add_stop(&lip_highlight, 0.88, theme.shelf_highlight.with_alpha(0.12));
    add_stop(&lip_highlight, 1.00, theme.shelf_highlight.with_alpha(0.0));
    let _ = cr.set_source(&lip_highlight);
    let _ = cr.stroke();

    cr.move_to(geom.lip_left.x + 0.6, geom.lip_left.y + 1.55);
    cr.line_to(geom.lip_right.x - 0.6, geom.lip_right.y + 1.55);
    cr.set_line_width(1.00);
    let lip_shadow = LinearGradient::new(geom.lip_left.x, 0.0, geom.lip_right.x, 0.0);
    add_stop(&lip_shadow, 0.00, theme.shelf_stroke.with_alpha(0.0));
    add_stop(&lip_shadow, 0.18, theme.shelf_stroke.with_alpha(0.08));
    add_stop(
        &lip_shadow,
        0.50,
        theme
            .shelf_stroke
            .mix(Color::rgba(0.0, 0.0, 0.0, 1.0), 0.22)
            .with_alpha(0.20),
    );
    add_stop(&lip_shadow, 0.82, theme.shelf_stroke.with_alpha(0.08));
    add_stop(&lip_shadow, 1.00, theme.shelf_stroke.with_alpha(0.0));
    let _ = cr.set_source(&lip_shadow);
    let _ = cr.stroke();

    cr.save().ok();
    leopard_front_face_path(cr, &geom, &body);
    cr.clip();

    let face_height = (geom.bottom_y - geom.front_left.y).max(1.0);
    let face_fill = LinearGradient::new(0.0, geom.front_left.y, 0.0, geom.bottom_y);
    add_stop(
        &face_fill,
        0.00,
        theme
            .shelf_top
            .mix(Color::rgba(0.86, 0.91, 0.96, 1.0), 0.26)
            .with_alpha(0.50),
    );
    add_stop(
        &face_fill,
        0.34,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.50, 0.60, 0.72, 1.0), 0.34)
            .with_alpha(0.78),
    );
    add_stop(
        &face_fill,
        1.00,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.26, 0.33, 0.43, 1.0), 0.44)
            .with_alpha(0.99),
    );
    let _ = cr.set_source(&face_fill);
    let _ = cr.paint();

    let face_roll = LinearGradient::new(
        0.0,
        geom.lip_y - face_height * 0.08,
        0.0,
        geom.lip_y + face_height * 0.62,
    );
    face_roll.add_color_stop_rgba(0.00, 0.88, 0.92, 0.96, 0.0);
    face_roll.add_color_stop_rgba(0.14, 0.88, 0.92, 0.96, 0.12);
    face_roll.add_color_stop_rgba(0.30, 0.88, 0.92, 0.96, 0.08);
    face_roll.add_color_stop_rgba(0.54, 0.88, 0.92, 0.96, 0.018);
    face_roll.add_color_stop_rgba(1.00, 0.88, 0.92, 0.96, 0.0);
    let _ = cr.set_source(&face_roll);
    let _ = cr.paint();

    let recess = LinearGradient::new(0.0, geom.lip_y + 0.2, 0.0, geom.bottom_y);
    recess.add_color_stop_rgba(0.00, 0.13, 0.17, 0.23, 0.10);
    recess.add_color_stop_rgba(0.22, 0.13, 0.17, 0.23, 0.05);
    recess.add_color_stop_rgba(0.72, 0.13, 0.17, 0.23, 0.018);
    recess.add_color_stop_rgba(1.00, 0.13, 0.17, 0.23, 0.0);
    let _ = cr.set_source(&recess);
    let _ = cr.paint();

    let glaze = LinearGradient::new(0.0, geom.lip_y + 0.7, 0.0, geom.bottom_y);
    glaze.add_color_stop_rgba(0.00, 0.90, 0.93, 0.97, 0.034);
    glaze.add_color_stop_rgba(0.28, 0.90, 0.93, 0.97, 0.012);
    glaze.add_color_stop_rgba(1.00, 0.90, 0.93, 0.97, 0.0);
    let _ = cr.set_source(&glaze);
    let _ = cr.paint();
    cr.restore().ok();
}
