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
}

pub(crate) fn shelf_horizon_y(shelf: &Rect, theme: &Theme) -> f64 {
    shelf.y + shelf.height * theme.shelf_horizon_ratio
}

pub(crate) fn compute_perspective_shelf_geometry(
    shelf: &Rect,
    theme: &Theme,
) -> PerspectiveShelfGeometry {
    let rear_inset = (shelf.height * (0.47 + theme.depth * 0.025))
        .clamp(shelf.height * 0.48, shelf.height * 0.52);
    let front_inset = (shelf.height * 0.006).clamp(0.2, 0.6);
    let back_y = shelf.y + 1.25;
    let front_lip_ratio = theme.front_lip_ratio.clamp(0.12, 0.13);
    let front_face_height = shelf.height * front_lip_ratio;
    let lip_y = shelf.y + shelf.height - front_face_height;
    let bottom_y = shelf.y + shelf.height - (shelf.height * 0.028).clamp(0.65, 1.55);
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
    let bottom_inset = (shelf.height * 0.055).clamp(2.4, 4.8);
    let join_drop = (face_height * 0.42).clamp(face_height * 0.22, face_height * 0.58);
    let join_inset = (bottom_inset * 0.35).clamp(0.8, 1.8);
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
            x: geom.lip_left.x + join_inset,
            y: geom.lip_y + join_drop,
        },
        face_right_join: Point {
            x: geom.lip_right.x - join_inset,
            y: geom.lip_y + join_drop,
        },
        face_left_inner_bottom: Point {
            x: shelf.x + bottom_inset,
            y: geom.bottom_y,
        },
        face_right_inner_bottom: Point {
            x: shelf.x + shelf.width - bottom_inset,
            y: geom.bottom_y,
        },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leopard_wedge_body_geometry_handles_short_shelves() {
        let shelf = Rect {
            x: 0.0,
            y: 48.0,
            width: 320.0,
            height: 29.76,
        };

        let theme = Theme::default();
        let geom = compute_perspective_shelf_geometry(&shelf, &theme);
        let body = leopard_wedge_body_geometry(&shelf, &theme);

        assert!(body.face_left_inner_bottom.x.is_finite());
        assert!(body.face_right_inner_bottom.x.is_finite());
        assert!(body.face_left_bottom.x > geom.lip_left.x);
        assert!(body.face_right_bottom.x < geom.lip_right.x);
    }
}
