mod cache;
mod geometry;
mod leopard;
mod material;
mod paths;
mod separator;

#[cfg(test)]
pub(super) use self::geometry::leopard_glass_plane_front_corner_radius;
pub(super) use self::geometry::{
    LeopardWedgeBodyGeometry, PerspectiveShelfGeometry, compute_perspective_shelf_geometry,
    leopard_wedge_body_geometry, shelf_horizon_y,
};
pub(super) use self::leopard::{
    draw_front_lip, draw_glass_highlight_overlay, draw_glass_shelf_base, draw_leopard_shelf_strokes,
};
pub(super) use self::material::fill_glass_material;
pub(super) use self::paths::{
    leopard_front_face_path, leopard_front_lip_bottom_path, leopard_front_lip_top_path,
    leopard_glass_plane_path, leopard_wedge_body_path,
};
pub(super) use self::separator::draw_shelf_section_separator;
pub(super) use self::cache::ProceduralShelfCache;
