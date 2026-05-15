use super::{
    LeopardWedgeBodyGeometry, PerspectiveShelfGeometry, compute_perspective_shelf_geometry,
    fill_crystal_material, leopard_front_face_path, leopard_glass_plane_path,
    leopard_wedge_body_geometry, leopard_wedge_body_path,
};
use super::super::{add_stop, set_color};
use crate::layout::Rect;
use crate::theme::{Color, Theme};
use gtk::cairo::{Context, LinearGradient};

pub(crate) fn draw_leopard_plank(cr: &Context, shelf: &Rect, theme: &Theme) {
    cr.save().ok();
    draw_glass_shelf_base(cr, shelf, theme);
    draw_glass_highlight_overlay(cr, shelf, theme);
    draw_front_lip(cr, shelf, theme);
    draw_leopard_shelf_strokes(cr, shelf, theme);
    cr.restore().ok();
}

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

    let glass = LinearGradient::new(0.0, geom.back_left.y, 0.0, geom.lip_left.y);
    add_stop(
        &glass,
        0.00,
        theme
            .shelf_top
            .mix(theme.shelf_bottom, 0.34)
            .with_alpha(0.30),
    );
    add_stop(
        &glass,
        0.22,
        theme
            .shelf_top
            .mix(theme.shelf_bottom, 0.24)
            .mix(theme.shelf_highlight, 0.02)
                .with_alpha(0.38),
    );
    add_stop(
        &glass,
        0.70,
        theme
            .shelf_top
            .mix(theme.shelf_bottom, 0.16)
            .mix(theme.shelf_highlight, 0.05)
                .with_alpha(0.48),
    );
    add_stop(
        &glass,
        1.00,
        theme
            .shelf_top
            .mix(theme.shelf_highlight, 0.02)
            .mix(theme.shelf_bottom, 0.12)
                .with_alpha(0.62),
    );
    let _ = cr.set_source(&glass);
    let _ = cr.paint();

    let center = LinearGradient::new(geom.back_left.x, 0.0, geom.back_right.x, 0.0);
    center.add_color_stop_rgba(0.00, 0.83, 0.85, 0.88, 0.0);
    center.add_color_stop_rgba(0.26, 0.83, 0.85, 0.88, 0.006);
    center.add_color_stop_rgba(0.50, 0.83, 0.85, 0.88, 0.012);
    center.add_color_stop_rgba(0.74, 0.83, 0.85, 0.88, 0.006);
    center.add_color_stop_rgba(1.00, 0.83, 0.85, 0.88, 0.0);
    let _ = cr.set_source(&center);
    let _ = cr.paint();

    let edge_vignette = LinearGradient::new(geom.back_left.x, 0.0, geom.back_right.x, 0.0);
    edge_vignette.add_color_stop_rgba(0.00, 0.30, 0.38, 0.48, 0.012);
    edge_vignette.add_color_stop_rgba(0.12, 0.30, 0.38, 0.48, 0.004);
    edge_vignette.add_color_stop_rgba(0.50, 1.0, 1.0, 1.0, 0.0);
    edge_vignette.add_color_stop_rgba(0.88, 0.30, 0.38, 0.48, 0.004);
    edge_vignette.add_color_stop_rgba(1.00, 0.30, 0.38, 0.48, 0.012);
    let _ = cr.set_source(&edge_vignette);
    let _ = cr.paint();

    let front_gloss = LinearGradient::new(0.0, geom.back_left.y, 0.0, geom.lip_left.y);
    front_gloss.add_color_stop_rgba(0.00, 0.84, 0.86, 0.89, 0.0);
    front_gloss.add_color_stop_rgba(0.44, 0.84, 0.86, 0.89, 0.018);
    front_gloss.add_color_stop_rgba(0.82, 0.84, 0.86, 0.89, 0.050);
    front_gloss.add_color_stop_rgba(1.00, 0.84, 0.86, 0.89, 0.0);
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

    fill_crystal_material(cr, shelf, base, 0.0007 + theme.material_roughness * 0.0012, |cr| {
        leopard_wedge_body_path(cr, geom, body);
    });

    cr.save().ok();
    leopard_wedge_body_path(cr, geom, body);
    cr.clip();

    let wall = LinearGradient::new(
        0.0,
        geom.lip_y - face_height * 0.16,
        0.0,
        bottom_y,
    );
    add_stop(
        &wall,
        0.00,
        theme
            .shelf_top
            .mix(theme.shelf_bottom, 0.34)
            .mix(Color::rgba(0.82, 0.86, 0.90, 1.0), 0.10)
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
            .mix(Color::rgba(0.50, 0.58, 0.68, 1.0), 0.34)
            .with_alpha(0.84),
    );
    add_stop(
        &wall,
        1.00,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.21, 0.27, 0.36, 1.0), 0.50)
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
    front_curve.add_color_stop_rgba(0.00, 0.85, 0.87, 0.90, 0.0);
    front_curve.add_color_stop_rgba(0.16, 0.85, 0.87, 0.90, 0.09);
    front_curve.add_color_stop_rgba(0.28, 0.85, 0.87, 0.90, 0.12);
    front_curve.add_color_stop_rgba(0.54, 0.85, 0.87, 0.90, 0.025);
    front_curve.add_color_stop_rgba(1.00, 0.85, 0.87, 0.90, 0.0);
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

    cr.move_to(geom.back_left.x, geom.back_left.y + 0.6);
    cr.line_to(geom.back_right.x, geom.back_right.y + 0.6);
    cr.set_line_width(1.0);
    set_color(
        cr,
        theme
            .shelf_highlight
            .with_alpha(0.22 * theme.highlight_strength),
    );
    let _ = cr.stroke();

    leopard_glass_plane_path(cr, shelf, theme);
    cr.set_line_width(0.9);
    let side_fade = LinearGradient::new(geom.front_left.x, 0.0, geom.front_right.x, 0.0);
    add_stop(&side_fade, 0.00, theme.shelf_stroke.with_alpha(0.0));
    add_stop(&side_fade, 0.14, theme.shelf_stroke.with_alpha(0.020));
    add_stop(&side_fade, 0.50, theme.shelf_stroke.with_alpha(0.050));
    add_stop(&side_fade, 0.86, theme.shelf_stroke.with_alpha(0.020));
    add_stop(&side_fade, 1.00, theme.shelf_stroke.with_alpha(0.0));
    let _ = cr.set_source(&side_fade);
    let _ = cr.stroke();

    cr.move_to(geom.back_left.x + 0.4, geom.back_left.y + 0.4);
    cr.line_to(geom.lip_left.x + 0.3, geom.lip_left.y - 0.1);
    cr.move_to(geom.back_right.x - 0.4, geom.back_right.y + 0.4);
    cr.line_to(geom.lip_right.x - 0.3, geom.lip_right.y - 0.1);
    cr.set_line_width(0.9);
    set_color(
        cr,
        theme
            .shelf_highlight
            .with_alpha(0.10 * theme.highlight_strength),
    );
    let _ = cr.stroke();

    cr.move_to(body.face_left_bottom.x + 0.8, body.face_left_bottom.y - 0.6);
    cr.line_to(body.face_right_bottom.x - 0.8, body.face_right_bottom.y - 0.6);
    cr.set_line_width(1.0);
    set_color(
        cr,
        theme
            .shelf_stroke
            .mix(Color::rgba(0.0, 0.0, 0.0, 1.0), 0.18)
                .with_alpha(0.26),
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
            .mix(Color::rgba(0.84, 0.87, 0.91, 1.0), 0.20)
            .with_alpha(0.50),
    );
    add_stop(
        &face_fill,
        0.34,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.52, 0.60, 0.69, 1.0), 0.32)
            .with_alpha(0.78),
    );
    add_stop(
        &face_fill,
        1.00,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.24, 0.30, 0.38, 1.0), 0.50)
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
    face_roll.add_color_stop_rgba(0.00, 0.85, 0.87, 0.90, 0.0);
    face_roll.add_color_stop_rgba(0.14, 0.85, 0.87, 0.90, 0.11);
    face_roll.add_color_stop_rgba(0.30, 0.85, 0.87, 0.90, 0.07);
    face_roll.add_color_stop_rgba(0.54, 0.85, 0.87, 0.90, 0.015);
    face_roll.add_color_stop_rgba(1.00, 0.85, 0.87, 0.90, 0.0);
    let _ = cr.set_source(&face_roll);
    let _ = cr.paint();

    let recess = LinearGradient::new(0.0, geom.lip_y + 0.2, 0.0, geom.bottom_y);
    recess.add_color_stop_rgba(0.00, 0.12, 0.14, 0.18, 0.12);
    recess.add_color_stop_rgba(0.22, 0.12, 0.14, 0.18, 0.06);
    recess.add_color_stop_rgba(0.72, 0.12, 0.14, 0.18, 0.02);
    recess.add_color_stop_rgba(1.00, 0.12, 0.14, 0.18, 0.0);
    let _ = cr.set_source(&recess);
    let _ = cr.paint();

    let glaze = LinearGradient::new(0.0, geom.lip_y + 0.7, 0.0, geom.bottom_y);
    glaze.add_color_stop_rgba(0.00, 0.86, 0.88, 0.91, 0.028);
    glaze.add_color_stop_rgba(0.28, 0.86, 0.88, 0.91, 0.010);
    glaze.add_color_stop_rgba(1.00, 0.86, 0.88, 0.91, 0.0);
    let _ = cr.set_source(&glaze);
    let _ = cr.paint();
    cr.restore().ok();
}