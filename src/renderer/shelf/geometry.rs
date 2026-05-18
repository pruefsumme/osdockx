use crate::layout::{Point, Rect};
use crate::theme::Theme;

#[derive(Debug, Clone, Copy)]
pub(crate) struct PerspectiveShelfGeometry {
    pub(crate) back_left: Point,
    pub(crate) back_right: Point,
    pub(crate) front_left: Point,
    pub(crate) front_right: Point,
    pub(crate) lip_left: Point,
    pub(crate) lip_right: Point,
    pub(crate) lip_y: f64,
    pub(crate) bottom_y: f64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LeopardWedgeBodyGeometry {
    pub(crate) face_left_bottom: Point,
    pub(crate) face_right_bottom: Point,
    pub(crate) face_left_join: Point,
    pub(crate) face_right_join: Point,
    pub(crate) face_left_inner_bottom: Point,
    pub(crate) face_right_inner_bottom: Point,
    pub(crate) front_corner_radius: f64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CrystalShelfGeometry {
    pub(crate) slant: f64,
    pub(crate) horizon_y: f64,
    pub(crate) lip_y: f64,
    pub(crate) bottom_y: f64,
}

pub(crate) fn crystal_shelf_geometry(shelf: &Rect, theme: &Theme) -> CrystalShelfGeometry {
    let slant = shelf.height * theme.shelf_slant_ratio;
    let horizon_y = shelf.y + shelf.height * theme.shelf_horizon_ratio;
    let bottom_y = shelf.y + shelf.height;
    let lip_height = (shelf.height * theme.front_lip_ratio)
        .max(2.0)
        .min(shelf.height * 0.34);
    CrystalShelfGeometry {
        slant,
        horizon_y,
        lip_y: bottom_y - lip_height,
        bottom_y,
    }
}

pub(crate) fn compute_perspective_shelf_geometry(
    shelf: &Rect,
    theme: &Theme,
) -> PerspectiveShelfGeometry {
    let rear_inset = (shelf.height * (0.50 + theme.depth * 0.055))
        .clamp(shelf.height * 0.48, shelf.height * 0.58);
    let front_inset = (shelf.height * 0.070).clamp(3.0, 5.2);
    let back_rise = (shelf.height * (0.080 + theme.tilt * 0.020))
        .clamp(shelf.height * 0.080, shelf.height * 0.115);
    let back_y = shelf.y - back_rise;
    let front_lip_ratio = (theme.front_lip_ratio * 0.68).clamp(0.070, 0.095);
    let front_face_height = shelf.height * front_lip_ratio;
    let lip_y = shelf.y + shelf.height - front_face_height;
    let bottom_y = shelf.y + shelf.height + (shelf.height * 0.031).clamp(1.2, 1.55);
    PerspectiveShelfGeometry {
        back_left: Point {
            x: shelf.x + rear_inset,
            y: back_y,
        },
        back_right: Point {
            x: shelf.x + shelf.width - rear_inset,
            y: back_y,
        },
        front_left: Point {
            x: shelf.x + front_inset,
            y: lip_y,
        },
        front_right: Point {
            x: shelf.x + shelf.width - front_inset,
            y: lip_y,
        },
        lip_left: Point {
            x: shelf.x + front_inset,
            y: lip_y,
        },
        lip_right: Point {
            x: shelf.x + shelf.width - front_inset,
            y: lip_y,
        },
        lip_y,
        bottom_y,
    }
}

pub(crate) fn leopard_wedge_body_geometry(shelf: &Rect, theme: &Theme) -> LeopardWedgeBodyGeometry {
    let geom = compute_perspective_shelf_geometry(shelf, theme);
    let face_height = (geom.bottom_y - geom.lip_y).max(1.0);
    let bottom_inset = (shelf.height * 0.012).clamp(0.7, 1.3);
    let side_span = (geom.lip_left.x - (shelf.x + bottom_inset)).max(0.0);
    let join_drop = (face_height * 0.42).clamp(1.8, face_height * 0.62);
    let join_outset = side_span * 0.22;
    let nose_width = (face_height * 1.35).clamp(4.8, shelf.height * 0.16);
    let front_corner_radius = leopard_glass_plane_front_corner_radius(shelf, &geom);
    LeopardWedgeBodyGeometry {
        face_left_bottom: Point {
            x: shelf.x + bottom_inset,
            y: geom.bottom_y,
        },
        face_right_bottom: Point {
            x: shelf.x + shelf.width - bottom_inset,
            y: geom.bottom_y,
        },
        face_left_join: Point {
            x: geom.lip_left.x - join_outset,
            y: geom.lip_y + join_drop,
        },
        face_right_join: Point {
            x: geom.lip_right.x + join_outset,
            y: geom.lip_y + join_drop,
        },
        face_left_inner_bottom: Point {
            x: shelf.x + bottom_inset + nose_width,
            y: geom.bottom_y,
        },
        face_right_inner_bottom: Point {
            x: shelf.x + shelf.width - bottom_inset - nose_width,
            y: geom.bottom_y,
        },
        front_corner_radius,
    }
}

pub(crate) fn leopard_glass_plane_front_corner_radius(
    shelf: &Rect,
    geom: &PerspectiveShelfGeometry,
) -> f64 {
    let radius = (shelf.height * 0.048).clamp(1.6, 2.8);
    radius
        .min(distance(geom.lip_right, geom.lip_left) * 0.45)
        .min(distance(geom.lip_left, geom.back_left) * 0.45)
}

fn distance(a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    (dx * dx + dy * dy).sqrt()
}
