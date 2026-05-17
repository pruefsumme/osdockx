use super::{
    IconCache, ResolvedIcon, crystal_shelf_geometry, draw_icon_art,
    draw_icon_source, leopard_glass_plane_path, rounded_rect,
};
use crate::layout::{DockLayout, Point, Rect};
use crate::model::DockItem;
use crate::theme::Theme;
use gtk::cairo::{Context, Format, ImageSurface, LinearGradient};

pub(super) fn render_icon_surface(
    resolved_icons: &[ResolvedIcon<'_>],
    layout: &DockLayout,
    icons: &mut IconCache,
) -> Option<ImageSurface> {
    let width = layout.size.0.max(1);
    let height = layout.size.1.max(1);
    let surface = ImageSurface::create(Format::ARgb32, width, height).ok()?;
    let cr = Context::new(&surface).ok()?;
    draw_icon_art(&cr, resolved_icons, icons);
    surface.flush();
    Some(surface)
}

pub(super) fn draw_shelf_plane_reflections(
    cr: &Context,
    layout: &DockLayout,
    theme: &Theme,
    icon_surface: &mut ImageSurface,
) {
    let reflection = shelf_plane_reflection_rect(layout, theme);
    if reflection.height <= 1.0 || reflection.width <= 1.0 {
        return;
    }

    let horizon_y = crystal_shelf_geometry(&layout.shelf, theme).horizon_y;
    let band_height = reflection.height.ceil().max(1.0) as i32;
    let source_y = (horizon_y - band_height as f64).max(0.0).floor() as i32;
    let Some(mirror) = mirrored_band_surface(icon_surface, source_y, band_height) else {
        return;
    };

    cr.save().ok();
    shelf_plane_reflection_clip_path(cr, &layout.shelf, theme);
    cr.clip();
    cr.rectangle(
        reflection.x,
        reflection.y,
        reflection.width,
        reflection.height,
    );
    cr.clip();
    if cr.set_source_surface(&mirror, 0.0, reflection.y).is_ok() {
        let fade = LinearGradient::new(0.0, reflection.y, 0.0, reflection.y + reflection.height);
        let alpha = theme.reflection_opacity.min(0.30);
        fade.add_color_stop_rgba(0.00, 1.0, 1.0, 1.0, alpha * 0.12);
        fade.add_color_stop_rgba(0.58, 1.0, 1.0, 1.0, alpha * 0.42);
        fade.add_color_stop_rgba(1.00, 1.0, 1.0, 1.0, alpha);
        let _ = cr.mask(&fade);
    }
    cr.restore().ok();
}

fn mirrored_band_surface(
    source: &mut ImageSurface,
    source_y: i32,
    band_height: i32,
) -> Option<ImageSurface> {
    let width = source.width().max(1);
    let source_height = source.height().max(1);
    let source_y = source_y.clamp(0, source_height - 1);
    let band_height = band_height.clamp(1, source_height - source_y);
    let mut mirror = ImageSurface::create(Format::ARgb32, width, band_height).ok()?;
    source.flush();

    {
        let source_stride = source.stride() as usize;
        let mirror_stride = mirror.stride() as usize;
        let row_bytes = width as usize * 4;
        let source_data = source.data().ok()?;
        let mut mirror_data = mirror.data().ok()?;
        for y in 0..band_height as usize {
            let src_y = source_y as usize + band_height as usize - 1 - y;
            let src_start = src_y * source_stride;
            let dst_start = y * mirror_stride;
            mirror_data[dst_start..dst_start + row_bytes]
                .copy_from_slice(&source_data[src_start..src_start + row_bytes]);
        }
    }

    mirror.mark_dirty();
    Some(mirror)
}

pub(super) fn shelf_plane_reflection_rect(layout: &DockLayout, theme: &Theme) -> Rect {
    let geom = crystal_shelf_geometry(&layout.shelf, theme);
    let height = (layout.shelf.height * theme.reflection_band_ratio)
        .min(geom.horizon_y - layout.shelf.y - 1.0)
        .max(0.0);
    Rect {
        x: layout.shelf.x,
        y: geom.horizon_y - height,
        width: layout.shelf.width,
        height,
    }
}

fn shelf_plane_reflection_clip_path(cr: &Context, shelf: &Rect, theme: &Theme) {
    leopard_glass_plane_path(cr, shelf, theme);
}

pub(super) fn uses_shelf_plane_reflections(theme: &Theme) -> bool {
    let _ = theme;
    true
}

pub(super) fn draw_icon_reflections_on_shelf(
    cr: &Context,
    resolved_icons: &[ResolvedIcon<'_>],
    layout: &DockLayout,
    theme: &Theme,
    icons: &mut IconCache,
) {
    let geom = super::compute_perspective_shelf_geometry(&layout.shelf, theme);
    cr.save().ok();
    create_polygon_mask(
        cr,
        &[
            geom.back_left,
            geom.back_right,
            geom.lip_right,
            geom.lip_left,
        ],
    );
    cr.clip();

    for icon in resolved_icons {
        let item = icon.item.as_ref();
        let max_height = (geom.lip_y - (icon.rect.y + icon.rect.height) - layout.shelf.height * 0.055)
            .max(icon.rect.height * 0.07);
        draw_icon_reflection(cr, item, icon.rect, max_height, theme, icons, icon.alpha);
    }

    cr.restore().ok();
}

fn draw_icon_reflection(
    cr: &Context,
    item: &DockItem,
    icon_rect: Rect,
    max_height: f64,
    theme: &Theme,
    icons: &mut IconCache,
    alpha: f64,
) {
    let alpha = alpha.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    let default_height = icon_rect.height * (theme.reflection_height * 0.90).clamp(0.22, 0.44);
    let reflection_height = default_height.min(max_height.max(icon_rect.height * 0.07));
    if reflection_height <= 1.0 {
        return;
    }

    let icon_size = icon_rect.height.ceil().max(1.0) as i32;
    let Ok(icon_surface) = ImageSurface::create(Format::ARgb32, icon_size, icon_size) else {
        return;
    };
    let Ok(icon_cr) = Context::new(&icon_surface) else {
        return;
    };
    draw_icon_source(&icon_cr, item, icon_size, icons, alpha);
    icon_surface.flush();

    let reflection_y = icon_rect.y + icon_rect.height;
    let alpha = (theme.reflection_opacity * 0.94 * alpha).clamp(0.0, 0.26);
    if alpha <= 0.0 {
        return;
    }
    let blur = 1.8 + theme.reflection_blur * 3.0;
    let passes = [
        (0.0, 0.0, alpha),
        (-blur * 0.28, blur * 0.10, alpha * 0.34),
        (blur * 0.28, blur * 0.10, alpha * 0.34),
        (0.0, blur * 0.24, alpha * 0.18),
    ];

    for (dx, dy, pass_alpha) in passes {
        cr.save().ok();
        rounded_rect(
            cr,
            icon_rect.x - 1.6,
            reflection_y,
            icon_rect.width + 3.2,
            reflection_height,
            (icon_rect.width * 0.08).min(5.0),
        );
        cr.clip();
        cr.translate(icon_rect.x + dx, reflection_y + reflection_height + dy);
        cr.scale(
            icon_rect.width / icon_rect.height,
            -reflection_height / icon_rect.height,
        );
        if cr.set_source_surface(&icon_surface, 0.0, 0.0).is_ok() {
            let fade = LinearGradient::new(0.0, 0.0, 0.0, icon_rect.height);
            fade.add_color_stop_rgba(0.00, 1.0, 1.0, 1.0, 0.0);
            fade.add_color_stop_rgba(0.44, 1.0, 1.0, 1.0, pass_alpha * 0.08);
            fade.add_color_stop_rgba(0.76, 1.0, 1.0, 1.0, pass_alpha * 0.36);
            fade.add_color_stop_rgba(1.00, 1.0, 1.0, 1.0, pass_alpha);
            let _ = cr.mask(&fade);
        }
        cr.restore().ok();
    }
}

fn create_polygon_mask(cr: &Context, points: &[Point]) {
    if points.is_empty() {
        return;
    }
    cr.new_path();
    cr.move_to(points[0].x, points[0].y);
    for point in &points[1..] {
        cr.line_to(point.x, point.y);
    }
    cr.close_path();
}

pub(super) fn draw_reflections(
    cr: &Context,
    resolved_icons: &[ResolvedIcon<'_>],
    theme: &Theme,
    icons: &mut IconCache,
) {
    if theme.reflection_opacity <= 0.0 {
        return;
    }

    for icon in resolved_icons {
        let item = icon.item.as_ref();
        let reflect_h = icon.rect.height * theme.reflection_height;
        if reflect_h <= 1.0 {
            continue;
        }
        let reflect_y = icon.rect.y + icon.rect.height + 2.0;
        cr.save().ok();
        cr.rectangle(icon.rect.x, reflect_y, icon.rect.width, reflect_h);
        cr.clip();
        cr.translate(icon.rect.x, reflect_y + reflect_h);
        cr.scale(
            icon.rect.width / icon.rect.height,
            -reflect_h / icon.rect.height,
        );
        draw_icon_source(
            cr,
            item,
            icon.rect.height as i32,
            icons,
            theme.reflection_opacity * icon.alpha,
        );
        cr.restore().ok();
    }
}