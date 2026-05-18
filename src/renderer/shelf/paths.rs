use super::geometry::{
    LeopardWedgeBodyGeometry, PerspectiveShelfGeometry, compute_perspective_shelf_geometry,
    crystal_shelf_geometry, leopard_glass_plane_front_corner_radius,
};
use crate::layout::{Point, Rect};
use crate::theme::Theme;
use gtk::cairo::Context;

pub(crate) fn leopard_glass_plane_path(cr: &Context, shelf: &Rect, theme: &Theme) {
    let geom = compute_perspective_shelf_geometry(shelf, theme);
    let radius = leopard_glass_plane_front_corner_radius(shelf, &geom);
    rounded_polygon_path(
        cr,
        &[
            geom.back_left,
            geom.back_right,
            geom.lip_right,
            geom.lip_left,
        ],
        radius,
    );
}

pub(crate) fn leopard_wedge_body_path(
    cr: &Context,
    geom: &PerspectiveShelfGeometry,
    body: &LeopardWedgeBodyGeometry,
) {
    leopard_front_face_path(cr, geom, body);
}

pub(crate) fn leopard_front_face_path(
    cr: &Context,
    geom: &PerspectiveShelfGeometry,
    body: &LeopardWedgeBodyGeometry,
) {
    let edge = leopard_front_edge_geometry(geom, body);
    cr.new_path();
    cr.move_to(edge.left_side.x, edge.left_side.y);
    add_leopard_front_edge(cr, &edge);
    add_leopard_right_cap(cr, &edge, body);
    cr.line_to(body.face_left_bottom.x, body.face_left_bottom.y);
    add_leopard_left_cap(cr, &edge, body);
    cr.close_path();
}

pub(crate) fn leopard_front_lip_top_path(
    cr: &Context,
    geom: &PerspectiveShelfGeometry,
    body: &LeopardWedgeBodyGeometry,
) {
    let edge = leopard_front_edge_geometry(geom, body);
    cr.new_path();
    cr.move_to(edge.left_front.x + 1.0, geom.lip_y + 0.45);
    cr.line_to(edge.right_front.x - 1.0, geom.lip_y + 0.45);
}

pub(crate) fn leopard_front_lip_bottom_path(
    cr: &Context,
    geom: &PerspectiveShelfGeometry,
    body: &LeopardWedgeBodyGeometry,
) {
    let inset = ((body.face_left_bottom.y - geom.lip_y).max(1.0) * 0.42).clamp(1.2, 3.2);
    let y = body.face_left_bottom.y - 0.55;
    cr.new_path();
    cr.move_to(body.face_left_bottom.x + inset, y);
    cr.line_to(body.face_right_bottom.x - inset, y);
}

#[derive(Debug, Clone, Copy)]
struct LeopardFrontEdgeGeometry {
    left_side: Point,
    left_side_control: Point,
    left_front_control: Point,
    left_front: Point,
    right_front: Point,
    right_front_control: Point,
    right_side_control: Point,
    right_side: Point,
}

fn leopard_front_edge_geometry(
    geom: &PerspectiveShelfGeometry,
    body: &LeopardWedgeBodyGeometry,
) -> LeopardFrontEdgeGeometry {
    let radius = leopard_front_face_radius(geom, body);
    let control_radius = radius * 0.36;
    LeopardFrontEdgeGeometry {
        left_side: move_toward(geom.lip_left, geom.back_left, radius),
        left_side_control: move_toward(geom.lip_left, geom.back_left, control_radius),
        left_front_control: move_toward(geom.lip_left, geom.lip_right, control_radius),
        left_front: move_toward(geom.lip_left, geom.lip_right, radius),
        right_front: move_toward(geom.lip_right, geom.lip_left, radius),
        right_front_control: move_toward(geom.lip_right, geom.lip_left, control_radius),
        right_side_control: move_toward(geom.lip_right, geom.back_right, control_radius),
        right_side: move_toward(geom.lip_right, geom.back_right, radius),
    }
}

fn leopard_front_face_radius(
    geom: &PerspectiveShelfGeometry,
    body: &LeopardWedgeBodyGeometry,
) -> f64 {
    let face_width = (geom.lip_right.x - geom.lip_left.x).max(1.0);
    let bottom_width = (body.face_right_bottom.x - body.face_left_bottom.x).max(1.0);
    body.front_corner_radius
        .min(face_width * 0.45)
        .min(bottom_width * 0.45)
}

fn add_leopard_front_edge(cr: &Context, edge: &LeopardFrontEdgeGeometry) {
    cr.curve_to(
        edge.left_side_control.x,
        edge.left_side_control.y,
        edge.left_front_control.x,
        edge.left_front_control.y,
        edge.left_front.x,
        edge.left_front.y,
    );
    cr.line_to(edge.right_front.x, edge.right_front.y);
    cr.curve_to(
        edge.right_front_control.x,
        edge.right_front_control.y,
        edge.right_side_control.x,
        edge.right_side_control.y,
        edge.right_side.x,
        edge.right_side.y,
    );
}

fn add_leopard_right_cap(
    cr: &Context,
    edge: &LeopardFrontEdgeGeometry,
    body: &LeopardWedgeBodyGeometry,
) {
    let cap_span = (body.face_right_bottom.y - edge.right_side.y).max(1.0);
    let cap_smooth = (body.front_corner_radius * 0.44)
        .min(cap_span * 0.45)
        .clamp(2.0, 6.0);
    cr.curve_to(
        edge.right_side.x,
        edge.right_side.y + cap_span * 0.44,
        body.face_right_bottom.x + cap_smooth,
        body.face_right_bottom.y,
        body.face_right_bottom.x,
        body.face_right_bottom.y,
    );
}

fn add_leopard_left_cap(
    cr: &Context,
    edge: &LeopardFrontEdgeGeometry,
    body: &LeopardWedgeBodyGeometry,
) {
    let cap_span = (body.face_left_bottom.y - edge.left_side.y).max(1.0);
    let cap_smooth = (body.front_corner_radius * 0.44)
        .min(cap_span * 0.45)
        .clamp(2.0, 6.0);
    cr.curve_to(
        body.face_left_bottom.x - cap_smooth,
        body.face_left_bottom.y,
        edge.left_side.x,
        edge.left_side.y + cap_span * 0.44,
        edge.left_side.x,
        edge.left_side.y,
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
