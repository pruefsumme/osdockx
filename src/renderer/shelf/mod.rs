mod crystal;
mod geometry;
mod legacy;
mod leopard;
mod material;
mod paths;

pub(super) use self::crystal::draw_crystal_shelf;
pub(super) use self::geometry::{
    LeopardWedgeBodyGeometry, PerspectiveShelfGeometry, compute_perspective_shelf_geometry,
    crystal_shelf_geometry, leopard_wedge_body_geometry,
};
pub(super) use self::legacy::draw_legacy_shelf;
pub(super) use self::leopard::{
    draw_front_lip, draw_glass_highlight_overlay, draw_glass_shelf_base,
    draw_leopard_plank, draw_leopard_shelf_strokes,
};
pub(super) use self::material::{draw_shadow, fill_crystal_material};
pub(super) use self::paths::{
    crystal_floor_path, crystal_lip_path, crystal_side_path, crystal_top_path,
    leopard_front_face_path, leopard_glass_plane_path, leopard_wedge_body_path,
};