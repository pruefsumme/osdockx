use super::geometry::{
    LeopardWedgeBodyGeometry, PerspectiveShelfGeometry, compute_perspective_shelf_geometry,
    crystal_shelf_geometry,
};
use crate::layout::{Point, Rect};
use crate::theme::Theme;
use gtk::cairo::Context;

pub(crate) fn leopard_glass_plane_path(cr: &Context, shelf: &Rect, theme: &Theme) {
    let geom = compute_perspective_shelf_geometry(shelf, theme);
    rounded_polygon_path(
        cr,
        &[
            geom.back_left,
            geom.back_right,
            geom.lip_right,
            geom.lip_left,
        ],
        (shelf.height * 0.26).clamp(4.8, 11.0),
    );
}

pub(crate) fn leopard_wedge_body_path(
    cr: &Context,
    geom: &PerspectiveShelfGeometry,
    body: &LeopardWedgeBodyGeometry,
) {
    let radius = leopard_front_face_radius(geom, body);
    cr.new_path();
    cr.move_to(geom.back_left.x, geom.back_left.y);
    cr.line_to(geom.back_right.x, geom.back_right.y);
    cr.line_to(geom.lip_right.x - radius, geom.lip_right.y);
    leopard_right_fascia_corner(cr, geom, body, radius);
    cr.line_to(body.face_left_bottom.x + radius, body.face_left_bottom.y);
    leopard_left_fascia_corner(cr, geom, body, radius);
    cr.line_to(geom.back_left.x, geom.back_left.y);
    cr.close_path();
}

pub(crate) fn leopard_front_face_path(
    cr: &Context,
    geom: &PerspectiveShelfGeometry,
    body: &LeopardWedgeBodyGeometry,
) {
    let radius = leopard_front_face_radius(geom, body);
    cr.new_path();
    cr.move_to(geom.lip_left.x + radius, geom.lip_left.y);
    cr.line_to(geom.lip_right.x - radius, geom.lip_right.y);
    leopard_right_fascia_corner(cr, geom, body, radius);
    cr.line_to(body.face_left_bottom.x + radius, body.face_left_bottom.y);
    leopard_left_fascia_corner(cr, geom, body, radius);
    cr.close_path();
}

pub(crate) fn leopard_front_lip_top_path(
    cr: &Context,
    geom: &PerspectiveShelfGeometry,
    body: &LeopardWedgeBodyGeometry,
) {
    let radius = leopard_front_face_radius(geom, body);
    cr.new_path();
    let _ = body;
    cr.move_to(geom.lip_left.x + radius * 1.18, geom.lip_left.y + 0.45);
    cr.line_to(geom.lip_right.x - radius * 1.18, geom.lip_right.y + 0.45);
}

pub(crate) fn leopard_front_lip_bottom_path(
    cr: &Context,
    geom: &PerspectiveShelfGeometry,
    body: &LeopardWedgeBodyGeometry,
) {
    let radius = leopard_front_face_radius(geom, body);
    let y = body.face_left_bottom.y - 0.55;
    cr.new_path();
    cr.move_to(body.face_left_bottom.x + radius * 1.24, y);
    cr.line_to(body.face_right_bottom.x - radius * 1.24, y);
}

fn leopard_front_face_radius(
    geom: &PerspectiveShelfGeometry,
    body: &LeopardWedgeBodyGeometry,
) -> f64 {
    let face_height = (body.face_left_bottom.y - geom.front_left.y).max(1.0);
    let face_width = (geom.lip_right.x - geom.lip_left.x).max(1.0);
    (face_height * 0.86).clamp(3.2, 8.4).min(face_width * 0.08)
}

fn leopard_right_fascia_corner(
    cr: &Context,
    geom: &PerspectiveShelfGeometry,
    body: &LeopardWedgeBodyGeometry,
    radius: f64,
) {
    let face_height = (body.face_right_bottom.y - geom.lip_right.y).max(1.0);
    cr.curve_to(
        geom.lip_right.x - radius * 0.18,
        geom.lip_right.y + face_height * 0.22,
        body.face_right_bottom.x + radius * 0.72,
        body.face_right_bottom.y - face_height * 0.20,
        body.face_right_bottom.x,
        body.face_right_bottom.y,
    );
}

fn leopard_left_fascia_corner(
    cr: &Context,
    geom: &PerspectiveShelfGeometry,
    body: &LeopardWedgeBodyGeometry,
    radius: f64,
) {
    let face_height = (body.face_left_bottom.y - geom.lip_left.y).max(1.0);
    cr.curve_to(
        body.face_left_bottom.x - radius * 0.72,
        body.face_left_bottom.y - face_height * 0.20,
        geom.lip_left.x + radius * 0.18,
        geom.lip_left.y + face_height * 0.22,
        geom.lip_left.x + radius,
        geom.lip_left.y,
    );
}

fn rounded_polygon_path(cr: &Context, points: &[Point], radius: f64) {
    if points.len() < 3 {
        return;
    }

    let count = points.len();
    let first_radius = corner_radius(points[count - 1], points[0], points[1], radius);
    let first_exit = move_toward(points[0], points[1], first_radius);

    cr.new_path();
    cr.move_to(first_exit.x, first_exit.y);

    for index in 1..=count {
        let corner = points[index % count];
        let prev = points[(index + count - 1) % count];
        let next = points[(index + 1) % count];
        let corner_radius = corner_radius(prev, corner, next, radius);
        let entry = move_toward(corner, prev, corner_radius);
        let exit = move_toward(corner, next, corner_radius);
        let control_in = move_toward(corner, prev, corner_radius * 0.36);
        let control_out = move_toward(corner, next, corner_radius * 0.36);

        cr.line_to(entry.x, entry.y);
        cr.curve_to(
            control_in.x,
            control_in.y,
            control_out.x,
            control_out.y,
            exit.x,
            exit.y,
        );
    }

    cr.close_path();
}

fn corner_radius(prev: Point, corner: Point, next: Point, radius: f64) -> f64 {
    let prev_len = distance(prev, corner);
    let next_len = distance(corner, next);
    radius.min(prev_len * 0.45).min(next_len * 0.45)
}

fn move_toward(from: Point, toward: Point, distance: f64) -> Point {
    let dx = toward.x - from.x;
    let dy = toward.y - from.y;
    let length = (dx * dx + dy * dy).sqrt();
    if length <= f64::EPSILON {
        return from;
    }

    let scale = distance / length;
    Point {
        x: from.x + dx * scale,
        y: from.y + dy * scale,
    }
}

fn distance(a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    (dx * dx + dy * dy).sqrt()
}

pub(crate) fn crystal_top_path(cr: &Context, shelf: &Rect, theme: &Theme) {
    let geom = crystal_shelf_geometry(shelf, theme);
    cr.new_path();
    cr.move_to(shelf.x + geom.slant, shelf.y);
    cr.line_to(shelf.x + shelf.width - geom.slant, shelf.y);
    cr.line_to(shelf.x + shelf.width - geom.slant * 0.45, geom.horizon_y);
    cr.line_to(shelf.x + geom.slant * 0.45, geom.horizon_y);
    cr.close_path();
}

pub(crate) fn crystal_floor_path(cr: &Context, shelf: &Rect, theme: &Theme) {
    let geom = crystal_shelf_geometry(shelf, theme);
    cr.new_path();
    cr.move_to(shelf.x + geom.slant * 0.45, geom.horizon_y);
    cr.line_to(shelf.x + shelf.width - geom.slant * 0.45, geom.horizon_y);
    cr.line_to(shelf.x + shelf.width, geom.bottom_y);
    cr.line_to(shelf.x, geom.bottom_y);
    cr.close_path();
}

pub(crate) fn crystal_lip_path(cr: &Context, shelf: &Rect, theme: &Theme) {
    let geom = crystal_shelf_geometry(shelf, theme);
    cr.new_path();
    cr.move_to(shelf.x + 2.0, geom.lip_y);
    cr.line_to(shelf.x + shelf.width - 2.0, geom.lip_y);
    cr.line_to(shelf.x + shelf.width - 5.0, geom.bottom_y);
    cr.line_to(shelf.x + 5.0, geom.bottom_y);
    cr.close_path();
}

pub(crate) fn crystal_side_path(cr: &Context, shelf: &Rect, theme: &Theme, left: bool) {
    let geom = crystal_shelf_geometry(shelf, theme);
    cr.new_path();
    if left {
        cr.move_to(shelf.x + geom.slant, shelf.y);
        cr.line_to(shelf.x + geom.slant * 0.45, geom.horizon_y);
        cr.line_to(shelf.x, geom.bottom_y);
        cr.line_to(
            shelf.x + geom.slant * 0.22,
            geom.horizon_y + shelf.height * 0.10,
        );
    } else {
        cr.move_to(shelf.x + shelf.width - geom.slant, shelf.y);
        cr.line_to(shelf.x + shelf.width - geom.slant * 0.45, geom.horizon_y);
        cr.line_to(shelf.x + shelf.width, geom.bottom_y);
        cr.line_to(
            shelf.x + shelf.width - geom.slant * 0.22,
            geom.horizon_y + shelf.height * 0.10,
        );
    }
    cr.close_path();
}
