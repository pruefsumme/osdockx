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
    let slant =
        (shelf.height * theme.shelf_slant_ratio).clamp(shelf.height * 0.32, shelf.height * 0.46);
    let rear_inset = slant * (0.80 + theme.depth * 0.16);
    let front_inset = slant * 0.025;
    let cap_inset =
        ((rear_inset - front_inset) * 0.054).clamp(shelf.height * 0.020, shelf.height * 0.040);
    let back_rise = (shelf.height * (0.104 + theme.tilt * 0.036))
        .clamp(shelf.height * 0.104, shelf.height * 0.145);
    let back_y = shelf.y - back_rise;
    let front_face_height =
        (shelf.height * theme.front_lip_ratio).clamp(shelf.height * 0.030, shelf.height * 0.140);
    let lip_y = shelf.y + shelf.height - front_face_height;
    let bottom_y = shelf.y + shelf.height + (shelf.height * 0.024).clamp(0.9, 1.5);
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
            x: shelf.x + front_inset + cap_inset,
            y: lip_y,
        },
        front_right: Point {
            x: shelf.x + shelf.width - front_inset - cap_inset,
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
    LeopardWedgeBodyGeometry {
        face_left_bottom: Point {
            x: geom.front_left.x,
            y: geom.bottom_y,
        },
        face_right_bottom: Point {
            x: geom.front_right.x,
            y: geom.bottom_y,
        },
    }
}
