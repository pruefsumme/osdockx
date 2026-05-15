use crate::config::{DockConfig, ShelfStyle};
use crate::layout::{DockLayout, LayoutParams, Point, Rect, compute_layout};
use crate::model::{DockItem, DockModel, WindowIcon};
use crate::theme::{Color, Theme};
use gtk::cairo::{Context, FontSlant, FontWeight, Format, ImageSurface, LinearGradient};
use gtk::gdk::prelude::GdkCairoContextExt;
use gtk::gdk_pixbuf::Pixbuf;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const ICON_CACHE_SIZE: i32 = 192;
const SLOW_DRAW: Duration = Duration::from_millis(8);

#[derive(Debug, Default)]
pub struct IconCache {
    enabled: bool,
    cache: HashMap<String, Option<Pixbuf>>,
}

#[derive(Debug, Default)]
pub struct Renderer {
    last_layout: DockLayout,
}

impl IconCache {
    pub fn new() -> Self {
        Self {
            enabled: true,
            cache: HashMap::new(),
        }
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            cache: HashMap::new(),
        }
    }

    fn pixbuf_for(&mut self, item: &DockItem) -> Option<Pixbuf> {
        if !self.enabled {
            return None;
        }
        let key = item
            .icon_name
            .clone()
            .or_else(|| item.startup_wm_class.clone())
            .unwrap_or_else(|| item.id.clone());
        if let Some(value) = self.cache.get(&key) {
            return value.clone();
        }

        let started = Instant::now();
        let loaded = load_icon(&key, ICON_CACHE_SIZE);
        tracing::debug!(
            target: "osdockx::perf",
            icon = %key,
            found = loaded.is_some(),
            elapsed_ms = elapsed_ms(started.elapsed()),
            "loaded dock icon"
        );
        self.cache.insert(key, loaded.clone());
        loaded
    }
}

impl Renderer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn layout(&self) -> &DockLayout {
        &self.last_layout
    }

    pub fn desired_size(
        model: &DockModel,
        config: &DockConfig,
        theme: &Theme,
        hover: Option<Point>,
    ) -> (i32, i32) {
        let params = layout_params(config, theme);
        compute_layout(model, hover, params).size
    }

    pub fn reserved_thickness(model: &DockModel, config: &DockConfig, theme: &Theme) -> u32 {
        let _ = model;
        let icon_size = config.icon_size as f64;
        let shelf_height = icon_size * theme.shelf_height_ratio;
        let visible_shelf = shelf_height * (1.0 - theme.shelf_horizon_ratio);
        (icon_size + visible_shelf + 12.0).ceil() as u32
    }

    pub fn visual_regions(
        model: &DockModel,
        config: &DockConfig,
        theme: &Theme,
        hover: Option<Point>,
    ) -> Vec<Rect> {
        let params = layout_params(config, theme);
        let layout = compute_layout(model, hover, params);
        let icon_expansion = config.icon_size as f64 * config.zoom_strength + 10.0;
        let mut regions = Vec::with_capacity(layout.icons.len() * 2 + 4);
        regions.push(expand(layout.shelf, 5.0));
        if uses_shelf_plane_reflections(theme) && theme.reflection_opacity > 0.0 {
            regions.push(expand(shelf_plane_reflection_rect(&layout, theme), 3.0));
        }
        for icon in &layout.icons {
            regions.push(expand(icon.rect, icon_expansion));
            if theme.reflection_height > 0.0 && !uses_shelf_plane_reflections(theme) {
                regions.push(expand(
                    Rect {
                        x: icon.rect.x,
                        y: icon.rect.y + icon.rect.height,
                        width: icon.rect.width,
                        height: icon.rect.height * theme.reflection_height,
                    },
                    4.0,
                ));
            }
        }
        if let Some(label) = layout.label {
            regions.push(expand(label.rect, 4.0));
        }
        regions
    }

    pub fn layout_for(
        model: &DockModel,
        config: &DockConfig,
        theme: &Theme,
        hover: Option<Point>,
    ) -> DockLayout {
        compute_layout(model, hover, layout_params(config, theme))
    }

    pub fn draw(
        &mut self,
        cr: &Context,
        model: &DockModel,
        config: &DockConfig,
        theme: &Theme,
        hover: Option<Point>,
        icons: &mut IconCache,
    ) {
        let started = Instant::now();
        let layout = Self::layout_for(model, config, theme, hover);
        self.draw_layout(cr, model, &layout, theme, icons, ShelfLayer::Procedural);
        self.last_layout = layout;
        self.log_draw_time(started.elapsed(), model.items.len());
    }

    pub fn draw_overlay(&mut self, cr: &Context, frame: RenderFrame<'_>, icons: &mut IconCache) {
        let started = Instant::now();
        let layout = Self::layout_for(frame.model, frame.config, frame.theme, frame.hover);
        self.draw_layout(
            cr,
            frame.model,
            &layout,
            frame.theme,
            icons,
            frame.shelf_layer,
        );
        self.last_layout = layout;
        self.log_draw_time(started.elapsed(), frame.model.items.len());
    }

    fn draw_layout(
        &self,
        cr: &Context,
        model: &DockModel,
        layout: &DockLayout,
        theme: &Theme,
        icons: &mut IconCache,
        shelf_layer: ShelfLayer,
    ) {
        clear(cr);
        if theme.shelf_style == ShelfStyle::LeopardPlank && shelf_layer != ShelfLayer::None {
            draw_shadow(cr, &layout.shelf);
            draw_glass_shelf_base(cr, &layout.shelf, theme);
            if theme.reflection_opacity > 0.0 {
                draw_icon_reflections_on_shelf(cr, model, layout, theme, icons);
            }
            draw_glass_highlight_overlay(cr, &layout.shelf, theme);
            draw_front_lip(cr, &layout.shelf, theme);
            draw_leopard_shelf_strokes(cr, &layout.shelf, theme);
            draw_icons(cr, model, layout, theme, icons);
            draw_hover_label(cr, model, layout);
            return;
        }

        let mut shelf_icon_surface = if uses_shelf_plane_reflections(theme)
            && theme.reflection_opacity > 0.0
            && shelf_layer != ShelfLayer::None
            && theme.shelf_style != ShelfStyle::LeopardPlank
        {
            render_icon_surface(model, layout, icons)
        } else {
            None
        };
        match shelf_layer {
            ShelfLayer::None => {}
            ShelfLayer::Procedural => draw_procedural_shelf_layer(cr, &layout.shelf, theme),
            ShelfLayer::Texture2d => {
                if !draw_texture_shelf_layer(cr, &layout.shelf, theme) {
                    draw_procedural_shelf_layer(cr, &layout.shelf, theme);
                }
            }
        }
        if theme.shelf_style == ShelfStyle::LeopardPlank && theme.reflection_opacity > 0.0 {
            draw_icon_reflections_on_shelf(cr, model, layout, theme, icons);
        } else if let Some(icon_surface) = shelf_icon_surface.as_mut() {
            draw_shelf_plane_reflections(cr, layout, theme, icon_surface);
        } else {
            draw_reflections(cr, model, layout, theme, icons);
        }
        draw_icons(cr, model, layout, theme, icons);
        draw_hover_label(cr, model, layout);
    }

    fn log_draw_time(&self, elapsed: Duration, icon_count: usize) {
        if elapsed >= SLOW_DRAW {
            tracing::debug!(
                target: "osdockx::perf",
                icons = icon_count,
                elapsed_ms = elapsed_ms(elapsed),
                "slow dock draw"
            );
        } else {
            tracing::trace!(
                target: "osdockx::perf",
                icons = icon_count,
                elapsed_ms = elapsed_ms(elapsed),
                "dock draw"
            );
        }
    }

    pub fn draw_for_test(
        &mut self,
        surface: &ImageSurface,
        model: &DockModel,
        config: &DockConfig,
        theme: &Theme,
    ) {
        let cr = Context::new(surface).expect("cairo context");
        let mut icons = IconCache::disabled();
        self.draw(&cr, model, config, theme, None, &mut icons);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShelfLayer {
    None,
    Procedural,
    Texture2d,
}

pub struct RenderFrame<'a> {
    pub model: &'a DockModel,
    pub config: &'a DockConfig,
    pub theme: &'a Theme,
    pub hover: Option<Point>,
    pub shelf_layer: ShelfLayer,
}

fn layout_params(config: &DockConfig, theme: &Theme) -> LayoutParams {
    let icon_size = config.icon_size as f64;
    LayoutParams {
        icon_size,
        zoom_strength: config.zoom_strength,
        gap: icon_size * theme.icon_gap_ratio,
        reflection_height: icon_size * theme.reflection_height,
        shelf_height: icon_size * theme.shelf_height_ratio,
        side_margin: icon_size * theme.side_margin_ratio,
        shelf_horizon_ratio: theme.shelf_horizon_ratio,
        icon_floor_offset: icon_size * theme.icon_floor_offset,
        label_height: 24.0_f64.max(icon_size * 0.34),
    }
}

fn expand(rect: Rect, amount: f64) -> Rect {
    Rect {
        x: rect.x - amount,
        y: rect.y - amount,
        width: rect.width + amount * 2.0,
        height: rect.height + amount * 2.0,
    }
}

fn clear(cr: &Context) {
    cr.save().ok();
    cr.set_operator(gtk::cairo::Operator::Clear);
    let _ = cr.paint();
    cr.restore().ok();
    cr.set_operator(gtk::cairo::Operator::Over);
}

fn draw_procedural_shelf_layer(cr: &Context, shelf: &Rect, theme: &Theme) {
    draw_shadow(cr, shelf);
    draw_shelf(cr, shelf, theme);
}

fn draw_shadow(cr: &Context, shelf: &Rect) {
    cr.save().ok();
    let shadow_y = shelf.y + shelf.height * 0.70;
    for pass in 0..5 {
        let grow = pass as f64 * shelf.height * 0.10;
        rounded_rect(
            cr,
            shelf.x + shelf.height * 0.18 - grow,
            shadow_y - grow * 0.15,
            shelf.width - shelf.height * 0.36 + grow * 2.0,
            shelf.height * 0.46 + grow * 0.42,
            shelf.height * 0.18 + grow * 0.35,
        );
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.10 / (pass as f64 + 1.0));
        let _ = cr.fill();
    }
    cr.restore().ok();
}

fn draw_texture_shelf_layer(cr: &Context, shelf: &Rect, theme: &Theme) -> bool {
    let Some(path) = theme.assets.fallback_texture.as_ref() else {
        return false;
    };
    let Ok(pixbuf) = Pixbuf::from_file(path) else {
        return false;
    };

    cr.save().ok();
    cr.translate(shelf.x, shelf.y);
    cr.scale(
        shelf.width / pixbuf.width().max(1) as f64,
        shelf.height / pixbuf.height().max(1) as f64,
    );
    cr.set_source_pixbuf(&pixbuf, 0.0, 0.0);
    let painted = cr.paint().is_ok();
    cr.restore().ok();
    painted
}

fn draw_shelf(cr: &Context, shelf: &Rect, theme: &Theme) {
    match theme.shelf_style {
        ShelfStyle::LeopardPlank => draw_leopard_plank(cr, shelf, theme),
        ShelfStyle::CrystalGlass => draw_crystal_shelf(cr, shelf, theme),
        ShelfStyle::LegacyGlass => draw_legacy_shelf(cr, shelf, theme),
    }
}

fn draw_leopard_plank(cr: &Context, shelf: &Rect, theme: &Theme) {
    cr.save().ok();
    draw_glass_shelf_base(cr, shelf, theme);
    draw_glass_highlight_overlay(cr, shelf, theme);
    draw_front_lip(cr, shelf, theme);
    draw_leopard_shelf_strokes(cr, shelf, theme);
    cr.restore().ok();
}

fn draw_glass_shelf_base(cr: &Context, shelf: &Rect, theme: &Theme) {
    let geom = compute_perspective_shelf_geometry(shelf, theme);

    fill_crystal_material(cr, shelf, theme.shelf_top.with_alpha(0.38), 0.018, |cr| {
        leopard_top_path(cr, shelf, theme);
    });

    cr.save().ok();
    leopard_top_path(cr, shelf, theme);
    cr.clip();
    let glass = LinearGradient::new(0.0, geom.back_left.y, 0.0, geom.front_left.y);
    add_stop(&glass, 0.00, theme.shelf_highlight.with_alpha(0.32));
    add_stop(&glass, 0.20, theme.shelf_top.with_alpha(0.20));
    add_stop(&glass, 0.58, theme.shelf_bottom.with_alpha(0.18));
    add_stop(&glass, 1.00, theme.shelf_bottom.with_alpha(0.36));
    let _ = cr.set_source(&glass);
    let _ = cr.paint();

    let center = LinearGradient::new(geom.front_left.x, 0.0, geom.front_right.x, 0.0);
    center.add_color_stop_rgba(0.00, 1.0, 1.0, 1.0, 0.00);
    center.add_color_stop_rgba(0.18, 1.0, 1.0, 1.0, 0.035);
    center.add_color_stop_rgba(0.50, 1.0, 1.0, 1.0, 0.15);
    center.add_color_stop_rgba(0.82, 1.0, 1.0, 1.0, 0.035);
    center.add_color_stop_rgba(1.00, 1.0, 1.0, 1.0, 0.00);
    let _ = cr.set_source(&center);
    let _ = cr.paint();
    cr.restore().ok();
}

fn draw_glass_highlight_overlay(cr: &Context, shelf: &Rect, theme: &Theme) {
    let geom = compute_perspective_shelf_geometry(shelf, theme);

    cr.save().ok();
    leopard_top_path(cr, shelf, theme);
    cr.clip();

    let band_y = geom.back_left.y + (geom.front_left.y - geom.back_left.y) * 0.30;
    let band = LinearGradient::new(
        0.0,
        band_y - shelf.height * 0.12,
        0.0,
        band_y + shelf.height * 0.22,
    );
    band.add_color_stop_rgba(0.00, 1.0, 1.0, 1.0, 0.0);
    band.add_color_stop_rgba(0.40, 1.0, 1.0, 1.0, 0.34 * theme.highlight_strength);
    band.add_color_stop_rgba(1.00, 1.0, 1.0, 1.0, 0.0);
    let _ = cr.set_source(&band);
    let _ = cr.paint();

    let front_glow = LinearGradient::new(
        0.0,
        geom.front_left.y - shelf.height * 0.18,
        0.0,
        geom.front_left.y,
    );
    front_glow.add_color_stop_rgba(0.00, 1.0, 1.0, 1.0, 0.0);
    front_glow.add_color_stop_rgba(1.00, 1.0, 1.0, 1.0, 0.12 * theme.highlight_strength);
    let _ = cr.set_source(&front_glow);
    let _ = cr.paint();
    cr.restore().ok();
}

fn draw_leopard_shelf_strokes(cr: &Context, shelf: &Rect, theme: &Theme) {
    let geom = compute_perspective_shelf_geometry(shelf, theme);

    cr.move_to(geom.back_left.x, geom.back_left.y + 0.6);
    cr.line_to(geom.back_right.x, geom.back_right.y + 0.6);
    cr.set_line_width(1.4);
    set_color(
        cr,
        theme
            .shelf_highlight
            .with_alpha(0.82 * theme.highlight_strength),
    );
    let _ = cr.stroke();

    cr.move_to(geom.front_left.x, geom.front_left.y - 0.5);
    cr.line_to(geom.front_right.x, geom.front_right.y - 0.5);
    cr.set_line_width(1.0);
    set_color(
        cr,
        theme
            .shelf_highlight
            .with_alpha(0.14 * theme.highlight_strength),
    );
    let _ = cr.stroke();

    leopard_top_path(cr, shelf, theme);
    cr.set_line_width(1.0);
    set_color(cr, theme.shelf_stroke.with_alpha(0.32));
    let _ = cr.stroke();
}

fn draw_front_lip(cr: &Context, shelf: &Rect, theme: &Theme) {
    let geom = compute_perspective_shelf_geometry(shelf, theme);

    draw_leopard_side_facet(cr, shelf, theme, true);
    draw_leopard_side_facet(cr, shelf, theme, false);

    let lip = LinearGradient::new(0.0, geom.front_left.y, 0.0, geom.bottom_y);
    add_stop(
        &lip,
        0.00,
        theme
            .shelf_bottom
            .mix(theme.shelf_top, 0.18)
            .with_alpha(0.74),
    );
    add_stop(
        &lip,
        0.45,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.015, 0.018, 0.024, 1.0), 0.34)
            .with_alpha(0.88),
    );
    add_stop(&lip, 1.00, Color::rgba(0.004, 0.005, 0.007, 0.94));

    leopard_front_path(cr, shelf, theme);
    let _ = cr.set_source(&lip);
    let _ = cr.fill();

    let dark_lip = LinearGradient::new(0.0, geom.lip_y, 0.0, geom.bottom_y);
    add_stop(
        &dark_lip,
        0.00,
        theme
            .shelf_bottom
            .mix(Color::rgba(0.01, 0.012, 0.015, 1.0), 0.32)
            .with_alpha(0.70),
    );
    add_stop(&dark_lip, 1.00, Color::rgba(0.0, 0.0, 0.0, 0.82));
    leopard_lip_path(cr, shelf, theme);
    let _ = cr.set_source(&dark_lip);
    let _ = cr.fill();

    cr.save().ok();
    leopard_front_path(cr, shelf, theme);
    cr.clip();
    draw_plank_texture(cr, shelf, theme.shelf_bottom, 0.040);
    cr.restore().ok();

    cr.move_to(geom.lip_left.x, geom.lip_left.y + 0.5);
    cr.line_to(geom.lip_right.x, geom.lip_right.y + 0.5);
    cr.set_line_width(1.0);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.22 * theme.highlight_strength);
    let _ = cr.stroke();

    cr.move_to(geom.bottom_left.x, geom.bottom_left.y - 0.6);
    cr.line_to(geom.bottom_right.x, geom.bottom_right.y - 0.6);
    cr.set_line_width(0.8);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.38);
    let _ = cr.stroke();
}

fn draw_crystal_shelf(cr: &Context, shelf: &Rect, theme: &Theme) {
    let geom = crystal_shelf_geometry(shelf, theme);
    cr.save().ok();

    let top_material = theme
        .shelf_top
        .mix(theme.shelf_bottom, 0.38)
        .with_alpha(1.0);
    fill_crystal_material(cr, shelf, top_material, 0.24, |cr| {
        crystal_top_path(cr, shelf, theme);
    });
    crystal_top_path(cr, shelf, theme);
    cr.set_line_width(1.0);
    set_color(cr, theme.shelf_stroke.with_alpha(0.86));
    let _ = cr.stroke();

    let face_material = theme
        .shelf_bottom
        .mix(theme.shelf_top, 0.10)
        .with_alpha(1.0);
    fill_crystal_material(cr, shelf, face_material, 0.18, |cr| {
        crystal_floor_path(cr, shelf, theme);
    });

    draw_crystal_side_facet(cr, shelf, theme, true);
    draw_crystal_side_facet(cr, shelf, theme, false);

    let lip_material = theme
        .shelf_bottom
        .mix(Color::rgba(0.02, 0.03, 0.04, 1.0), 0.56)
        .with_alpha(1.0);
    fill_crystal_material(cr, shelf, lip_material, 0.10, |cr| {
        crystal_lip_path(cr, shelf, theme);
    });

    cr.move_to(shelf.x + geom.slant, shelf.y + 0.7);
    cr.line_to(shelf.x + shelf.width - geom.slant, shelf.y + 0.7);
    cr.set_line_width(1.5);
    set_color(
        cr,
        theme
            .shelf_highlight
            .with_alpha(0.62 * theme.highlight_strength),
    );
    let _ = cr.stroke();

    cr.move_to(shelf.x + geom.slant * 0.48, geom.horizon_y);
    cr.line_to(shelf.x + shelf.width - geom.slant * 0.48, geom.horizon_y);
    cr.set_line_width(1.0);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.22 * theme.highlight_strength);
    let _ = cr.stroke();

    cr.move_to(shelf.x + 5.0, geom.bottom_y - 0.5);
    cr.line_to(shelf.x + shelf.width - 5.0, geom.bottom_y - 0.5);
    cr.set_line_width(1.0);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.64);
    let _ = cr.stroke();

    cr.restore().ok();
}

fn draw_crystal_side_facet(cr: &Context, shelf: &Rect, theme: &Theme, left: bool) {
    let side_material = theme
        .shelf_bottom
        .mix(Color::rgba(0.0, 0.0, 0.0, 1.0), 0.36)
        .with_alpha(1.0);
    fill_crystal_material(cr, shelf, side_material, 0.12, |cr| {
        crystal_side_path(cr, shelf, theme, left);
    });
}

fn draw_leopard_side_facet(cr: &Context, shelf: &Rect, theme: &Theme, left: bool) {
    let side_material = theme
        .shelf_bottom
        .mix(Color::rgba(0.0, 0.0, 0.0, 1.0), 0.18)
        .with_alpha(1.0);
    fill_crystal_material(cr, shelf, side_material, 0.040, |cr| {
        leopard_side_path(cr, shelf, theme, left);
    });
}

fn fill_crystal_material<F>(
    cr: &Context,
    bounds: &Rect,
    base: Color,
    texture_strength: f64,
    path: F,
) where
    F: Fn(&Context),
{
    path(cr);
    set_color(cr, base);
    let _ = cr.fill();

    cr.save().ok();
    path(cr);
    cr.clip();
    draw_plank_texture(cr, bounds, base, texture_strength);
    cr.restore().ok();
}

fn draw_plank_texture(cr: &Context, bounds: &Rect, base: Color, strength: f64) {
    if strength <= 0.0 {
        return;
    }

    cr.save().ok();
    cr.set_line_width(1.0);
    let min_y = bounds.y.floor() as i32;
    let max_y = (bounds.y + bounds.height).ceil() as i32;
    for y in min_y..=max_y {
        let noise = (((y * 37 + 17).rem_euclid(23)) as f64 / 22.0) - 0.5;
        let mix = (noise.abs() * 0.075 + 0.018).min(0.09);
        let color = if noise >= 0.0 {
            base.mix(Color::rgba(1.0, 1.0, 1.0, 1.0), mix)
        } else {
            base.mix(Color::rgba(0.0, 0.0, 0.0, 1.0), mix)
        };
        set_color(cr, color.with_alpha(strength * (0.22 + noise.abs() * 0.16)));
        let yy = y as f64 + 0.5;
        cr.move_to(bounds.x, yy);
        cr.line_to(bounds.x + bounds.width, yy);
        let _ = cr.stroke();
    }

    cr.set_line_width(1.0);
    let min_x = bounds.x.floor() as i32;
    let max_x = (bounds.x + bounds.width).ceil() as i32;
    for x in (min_x..=max_x).step_by(13) {
        let alpha = strength * 0.018;
        cr.set_source_rgba(1.0, 1.0, 1.0, alpha);
        let xx = x as f64 + 0.5;
        cr.move_to(xx, bounds.y);
        cr.line_to(xx, bounds.y + bounds.height);
        let _ = cr.stroke();
    }
    cr.restore().ok();
}

fn draw_legacy_shelf(cr: &Context, shelf: &Rect, theme: &Theme) {
    let slant = shelf.height * theme.shelf_slant_ratio;
    let horizon_y = shelf.y + shelf.height * 0.40;
    let bottom_y = shelf.y + shelf.height;
    cr.save().ok();

    cr.move_to(shelf.x + slant, shelf.y);
    cr.line_to(shelf.x + shelf.width - slant, shelf.y);
    cr.line_to(shelf.x + shelf.width, bottom_y);
    cr.line_to(shelf.x, bottom_y);
    cr.close_path();

    let base_gradient = LinearGradient::new(0.0, shelf.y, 0.0, bottom_y);
    add_stop(&base_gradient, 0.00, theme.shelf_top.with_alpha(0.96));
    add_stop(&base_gradient, 0.28, Color::rgba(0.88, 0.94, 0.98, 0.84));
    add_stop(&base_gradient, 0.52, theme.shelf_bottom.with_alpha(0.76));
    add_stop(&base_gradient, 1.00, Color::rgba(0.30, 0.38, 0.47, 0.90));
    let _ = cr.set_source(&base_gradient);
    let _ = cr.fill_preserve();
    cr.set_line_width(1.0);
    set_color(cr, theme.shelf_stroke.with_alpha(0.72));
    let _ = cr.stroke();

    cr.move_to(shelf.x + slant * 0.58, horizon_y);
    cr.line_to(shelf.x + shelf.width - slant * 0.58, horizon_y);
    cr.line_to(shelf.x + shelf.width - slant * 0.15, bottom_y - 1.0);
    cr.line_to(shelf.x + slant * 0.15, bottom_y - 1.0);
    cr.close_path();

    let face_gradient = LinearGradient::new(0.0, horizon_y, 0.0, bottom_y);
    add_stop(&face_gradient, 0.00, Color::rgba(0.72, 0.82, 0.91, 0.42));
    add_stop(&face_gradient, 0.55, Color::rgba(0.56, 0.67, 0.78, 0.38));
    add_stop(&face_gradient, 1.00, Color::rgba(0.18, 0.24, 0.31, 0.42));
    let _ = cr.set_source(&face_gradient);
    let _ = cr.fill();

    cr.move_to(shelf.x + slant, shelf.y);
    cr.line_to(shelf.x + shelf.width - slant, shelf.y);
    cr.set_line_width(1.4);
    set_color(cr, theme.shelf_highlight.with_alpha(0.90));
    let _ = cr.stroke();

    cr.move_to(shelf.x + slant * 0.65, horizon_y);
    cr.line_to(shelf.x + shelf.width - slant * 0.65, horizon_y);
    cr.set_line_width(1.0);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.34);
    let _ = cr.stroke();

    cr.move_to(shelf.x + 4.0, bottom_y - 1.0);
    cr.line_to(shelf.x + shelf.width - 4.0, bottom_y - 1.0);
    cr.set_line_width(1.0);
    cr.set_source_rgba(0.06, 0.08, 0.11, 0.42);
    let _ = cr.stroke();
    cr.restore().ok();
}

fn render_icon_surface(
    model: &DockModel,
    layout: &DockLayout,
    icons: &mut IconCache,
) -> Option<ImageSurface> {
    let width = layout.size.0.max(1);
    let height = layout.size.1.max(1);
    let surface = ImageSurface::create(Format::ARgb32, width, height).ok()?;
    let cr = Context::new(&surface).ok()?;
    draw_icon_art(&cr, model, layout, icons, 1.0);
    surface.flush();
    Some(surface)
}

fn draw_shelf_plane_reflections(
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
        if theme.shelf_style == ShelfStyle::LeopardPlank {
            fade.add_color_stop_rgba(0.00, 1.0, 1.0, 1.0, alpha * 0.12);
            fade.add_color_stop_rgba(0.58, 1.0, 1.0, 1.0, alpha * 0.42);
            fade.add_color_stop_rgba(1.00, 1.0, 1.0, 1.0, alpha);
        } else {
            fade.add_color_stop_rgba(0.00, 1.0, 1.0, 1.0, alpha);
            fade.add_color_stop_rgba(0.65, 1.0, 1.0, 1.0, alpha * 0.38);
            fade.add_color_stop_rgba(1.00, 1.0, 1.0, 1.0, 0.0);
        }
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

fn shelf_plane_reflection_rect(layout: &DockLayout, theme: &Theme) -> Rect {
    let geom = crystal_shelf_geometry(&layout.shelf, theme);
    match theme.shelf_style {
        ShelfStyle::LeopardPlank => {
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
        _ => {
            let height = (layout.shelf.height * theme.reflection_band_ratio)
                .min(geom.bottom_y - geom.horizon_y)
                .max(0.0);
            Rect {
                x: layout.shelf.x,
                y: geom.horizon_y,
                width: layout.shelf.width,
                height,
            }
        }
    }
}

fn shelf_plane_reflection_clip_path(cr: &Context, shelf: &Rect, theme: &Theme) {
    match theme.shelf_style {
        ShelfStyle::LeopardPlank => leopard_top_path(cr, shelf, theme),
        _ => crystal_floor_path(cr, shelf, theme),
    }
}

fn uses_shelf_plane_reflections(theme: &Theme) -> bool {
    matches!(
        theme.shelf_style,
        ShelfStyle::LeopardPlank | ShelfStyle::CrystalGlass
    )
}

#[derive(Debug, Clone, Copy)]
struct PerspectiveShelfGeometry {
    back_left: Point,
    back_right: Point,
    front_left: Point,
    front_right: Point,
    lip_left: Point,
    lip_right: Point,
    bottom_left: Point,
    bottom_right: Point,
    slant: f64,
    horizon_y: f64,
    lip_y: f64,
    bottom_y: f64,
}

#[derive(Debug, Clone, Copy)]
struct CrystalShelfGeometry {
    slant: f64,
    horizon_y: f64,
    lip_y: f64,
    bottom_y: f64,
}

fn crystal_shelf_geometry(shelf: &Rect, theme: &Theme) -> CrystalShelfGeometry {
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

fn compute_perspective_shelf_geometry(shelf: &Rect, theme: &Theme) -> PerspectiveShelfGeometry {
    let slant = (shelf.height * theme.shelf_slant_ratio).max(shelf.height * 0.78);
    let back_y = shelf.y - shelf.height * 0.06;
    let horizon_y = shelf.y + shelf.height * theme.shelf_horizon_ratio;
    let bottom_y = shelf.y + shelf.height;
    let lip_height = (shelf.height * theme.front_lip_ratio)
        .max(shelf.height * 0.14)
        .min(shelf.height * 0.22);
    let lip_y = bottom_y - lip_height;
    let bottom_inset = shelf.height * 0.07;
    PerspectiveShelfGeometry {
        back_left: Point {
            x: shelf.x + slant * 1.26,
            y: back_y,
        },
        back_right: Point {
            x: shelf.x + shelf.width - slant * 1.26,
            y: back_y,
        },
        front_left: Point {
            x: shelf.x + slant * 0.02,
            y: horizon_y,
        },
        front_right: Point {
            x: shelf.x + shelf.width - slant * 0.02,
            y: horizon_y,
        },
        lip_left: Point {
            x: shelf.x + shelf.height * 0.04,
            y: lip_y,
        },
        lip_right: Point {
            x: shelf.x + shelf.width - shelf.height * 0.04,
            y: lip_y,
        },
        bottom_left: Point {
            x: shelf.x + bottom_inset,
            y: bottom_y,
        },
        bottom_right: Point {
            x: shelf.x + shelf.width - bottom_inset,
            y: bottom_y,
        },
        slant,
        horizon_y,
        lip_y,
        bottom_y,
    }
}

fn leopard_top_path(cr: &Context, shelf: &Rect, theme: &Theme) {
    let geom = compute_perspective_shelf_geometry(shelf, theme);
    cr.new_path();
    cr.move_to(geom.back_left.x, geom.back_left.y);
    cr.line_to(geom.back_right.x, geom.back_right.y);
    cr.line_to(geom.front_right.x, geom.front_right.y);
    cr.line_to(geom.front_left.x, geom.front_left.y);
    cr.close_path();
}

fn leopard_front_path(cr: &Context, shelf: &Rect, theme: &Theme) {
    let geom = compute_perspective_shelf_geometry(shelf, theme);
    cr.new_path();
    cr.move_to(geom.front_left.x, geom.front_left.y);
    cr.line_to(geom.front_right.x, geom.front_right.y);
    cr.line_to(geom.bottom_right.x, geom.bottom_right.y);
    cr.line_to(geom.bottom_left.x, geom.bottom_left.y);
    cr.close_path();
}

fn leopard_lip_path(cr: &Context, shelf: &Rect, theme: &Theme) {
    let geom = compute_perspective_shelf_geometry(shelf, theme);
    cr.new_path();
    cr.move_to(geom.lip_left.x, geom.lip_left.y);
    cr.line_to(geom.lip_right.x, geom.lip_right.y);
    cr.line_to(geom.bottom_right.x, geom.bottom_right.y);
    cr.line_to(geom.bottom_left.x, geom.bottom_left.y);
    cr.close_path();
}

fn leopard_side_path(cr: &Context, shelf: &Rect, theme: &Theme, left: bool) {
    let geom = compute_perspective_shelf_geometry(shelf, theme);
    cr.new_path();
    if left {
        cr.move_to(geom.back_left.x, geom.back_left.y);
        cr.line_to(geom.front_left.x, geom.front_left.y);
        cr.line_to(geom.bottom_left.x, geom.bottom_left.y);
        cr.line_to(geom.lip_left.x + geom.slant * 0.10, geom.lip_left.y);
    } else {
        cr.move_to(geom.back_right.x, geom.back_right.y);
        cr.line_to(geom.front_right.x, geom.front_right.y);
        cr.line_to(geom.bottom_right.x, geom.bottom_right.y);
        cr.line_to(geom.lip_right.x - geom.slant * 0.10, geom.lip_right.y);
    }
    cr.close_path();
}

fn crystal_top_path(cr: &Context, shelf: &Rect, theme: &Theme) {
    let geom = crystal_shelf_geometry(shelf, theme);
    cr.new_path();
    cr.move_to(shelf.x + geom.slant, shelf.y);
    cr.line_to(shelf.x + shelf.width - geom.slant, shelf.y);
    cr.line_to(shelf.x + shelf.width - geom.slant * 0.45, geom.horizon_y);
    cr.line_to(shelf.x + geom.slant * 0.45, geom.horizon_y);
    cr.close_path();
}

fn crystal_floor_path(cr: &Context, shelf: &Rect, theme: &Theme) {
    let geom = crystal_shelf_geometry(shelf, theme);
    cr.new_path();
    cr.move_to(shelf.x + geom.slant * 0.45, geom.horizon_y);
    cr.line_to(shelf.x + shelf.width - geom.slant * 0.45, geom.horizon_y);
    cr.line_to(shelf.x + shelf.width, geom.bottom_y);
    cr.line_to(shelf.x, geom.bottom_y);
    cr.close_path();
}

fn crystal_lip_path(cr: &Context, shelf: &Rect, theme: &Theme) {
    let geom = crystal_shelf_geometry(shelf, theme);
    cr.new_path();
    cr.move_to(shelf.x + 2.0, geom.lip_y);
    cr.line_to(shelf.x + shelf.width - 2.0, geom.lip_y);
    cr.line_to(shelf.x + shelf.width - 5.0, geom.bottom_y);
    cr.line_to(shelf.x + 5.0, geom.bottom_y);
    cr.close_path();
}

fn crystal_side_path(cr: &Context, shelf: &Rect, theme: &Theme, left: bool) {
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

fn draw_icon_reflections_on_shelf(
    cr: &Context,
    model: &DockModel,
    layout: &DockLayout,
    theme: &Theme,
    icons: &mut IconCache,
) {
    let geom = compute_perspective_shelf_geometry(&layout.shelf, theme);
    cr.save().ok();
    create_polygon_mask(
        cr,
        &[
            geom.back_left,
            geom.back_right,
            geom.front_right,
            geom.front_left,
        ],
    );
    cr.clip();

    for icon in &layout.icons {
        let item = &model.items[icon.item_index];
        draw_icon_reflection(cr, item, icon.rect, theme, icons);
    }

    cr.restore().ok();
}

fn draw_icon_reflection(
    cr: &Context,
    item: &DockItem,
    icon_rect: Rect,
    theme: &Theme,
    icons: &mut IconCache,
) {
    let reflection_height = (icon_rect.height * (theme.reflection_height * 1.18).max(0.56))
        .min(icon_rect.height * 0.72);
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
    draw_icon_source(&icon_cr, item, icon_size, icons, 1.0);
    icon_surface.flush();

    let reflection_y = icon_rect.y + icon_rect.height;
    let alpha = (theme.reflection_opacity * 1.75).max(0.38).min(0.58);
    let passes = [
        (0.0, 0.0, alpha),
        (-0.9, 0.7, alpha * 0.24),
        (0.9, 0.7, alpha * 0.24),
        (0.0, 1.8, alpha * 0.18),
        (-1.5, 1.5, alpha * 0.10),
        (1.5, 1.5, alpha * 0.10),
    ];

    for (dx, dy, pass_alpha) in passes {
        cr.save().ok();
        cr.rectangle(
            icon_rect.x - 2.0,
            reflection_y,
            icon_rect.width + 4.0,
            reflection_height,
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
            fade.add_color_stop_rgba(0.36, 1.0, 1.0, 1.0, pass_alpha * 0.20);
            fade.add_color_stop_rgba(0.72, 1.0, 1.0, 1.0, pass_alpha * 0.62);
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

fn draw_reflections(
    cr: &Context,
    model: &DockModel,
    layout: &DockLayout,
    theme: &Theme,
    icons: &mut IconCache,
) {
    if theme.reflection_opacity <= 0.0 {
        return;
    }

    for icon in &layout.icons {
        let item = &model.items[icon.item_index];
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
            theme.reflection_opacity,
        );
        cr.restore().ok();
    }
}

fn draw_icons(
    cr: &Context,
    model: &DockModel,
    layout: &DockLayout,
    theme: &Theme,
    icons: &mut IconCache,
) {
    draw_icon_art(cr, model, layout, icons, 1.0);
    for icon in &layout.icons {
        let item = &model.items[icon.item_index];
        if item.is_running() {
            if theme.shelf_style == ShelfStyle::LeopardPlank {
                draw_leopard_indicator(cr, icon.rect, layout, theme, item.active);
            } else {
                draw_indicator(cr, icon.rect, theme.indicator, item.active);
            }
        }
        if let Some(badge) = item.badge {
            draw_badge(cr, icon.rect, badge, theme.badge);
        }
    }
}

fn draw_icon_art(
    cr: &Context,
    model: &DockModel,
    layout: &DockLayout,
    icons: &mut IconCache,
    alpha: f64,
) {
    for icon in &layout.icons {
        let item = &model.items[icon.item_index];
        cr.save().ok();
        cr.translate(icon.rect.x, icon.rect.y);
        cr.scale(icon.rect.width / icon.rect.height, 1.0);
        draw_icon_source(cr, item, icon.rect.height as i32, icons, alpha);
        cr.restore().ok();
    }
}

fn draw_hover_label(cr: &Context, model: &DockModel, layout: &DockLayout) {
    let Some(label) = layout.label.as_ref() else {
        return;
    };
    let Some(item) = model.items.get(label.item_index) else {
        return;
    };

    cr.save().ok();
    cr.select_font_face("Sans", FontSlant::Normal, FontWeight::Bold);
    cr.set_font_size(13.0);
    let extents = cr.text_extents(&item.name).ok();
    let text_width = extents.as_ref().map(|e| e.width()).unwrap_or(64.0);
    let text_height = extents.as_ref().map(|e| e.height()).unwrap_or(12.0);
    let width = (text_width + 18.0).max(42.0);
    let height = label.rect.height.min(28.0);
    let max_x = (layout.size.0 as f64 - width - 4.0).max(4.0);
    let x = (label.rect.center_x() - width / 2.0).clamp(4.0, max_x);
    let y = label.rect.y;

    rounded_rect(cr, x, y, width, height, 7.0);
    cr.set_source_rgba(0.05, 0.06, 0.07, 0.82);
    let _ = cr.fill_preserve();
    cr.set_line_width(1.0);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.22);
    let _ = cr.stroke();

    let text_x = extents
        .as_ref()
        .map(|e| x + width / 2.0 - (e.width() / 2.0 + e.x_bearing()))
        .unwrap_or(x + 9.0);
    let text_y = extents
        .as_ref()
        .map(|e| y + height / 2.0 - (text_height / 2.0 + e.y_bearing()))
        .unwrap_or(y + 17.0);
    cr.move_to(text_x, text_y);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.96);
    let _ = cr.show_text(&item.name);
    cr.restore().ok();
}

fn draw_icon_source(cr: &Context, item: &DockItem, size: i32, icons: &mut IconCache, alpha: f64) {
    if let Some(pixbuf) = icons.pixbuf_for(item) {
        let scale_x = size as f64 / pixbuf.width() as f64;
        let scale_y = size as f64 / pixbuf.height() as f64;
        cr.save().ok();
        cr.scale(scale_x, scale_y);
        cr.set_source_pixbuf(&pixbuf, 0.0, 0.0);
        let _ = cr.paint_with_alpha(alpha);
        cr.restore().ok();
        return;
    }

    if let Some(icon) = item.window_icon.as_ref()
        && draw_window_icon(cr, icon, size, alpha)
    {
        return;
    }

    draw_placeholder(cr, item, size as f64, alpha);
}

fn draw_window_icon(cr: &Context, icon: &WindowIcon, size: i32, alpha: f64) -> bool {
    if icon.width == 0 || icon.height == 0 {
        return false;
    }
    let Ok(mut surface) =
        ImageSurface::create(Format::ARgb32, icon.width as i32, icon.height as i32)
    else {
        return false;
    };
    let stride = surface.stride() as usize;
    {
        let Ok(mut data) = surface.data() else {
            return false;
        };
        for y in 0..icon.height as usize {
            for x in 0..icon.width as usize {
                let Some(&argb) = icon.argb.get(y * icon.width as usize + x) else {
                    return false;
                };
                let alpha = ((argb >> 24) & 0xff) as u8;
                let red = premultiply(((argb >> 16) & 0xff) as u8, alpha);
                let green = premultiply(((argb >> 8) & 0xff) as u8, alpha);
                let blue = premultiply((argb & 0xff) as u8, alpha);
                let pixel = u32::from(alpha) << 24
                    | u32::from(red) << 16
                    | u32::from(green) << 8
                    | u32::from(blue);
                let offset = y * stride + x * 4;
                data[offset..offset + 4].copy_from_slice(&pixel.to_ne_bytes());
            }
        }
    }
    surface.mark_dirty();

    let scale_x = size as f64 / icon.width as f64;
    let scale_y = size as f64 / icon.height as f64;
    cr.save().ok();
    cr.scale(scale_x, scale_y);
    if cr.set_source_surface(&surface, 0.0, 0.0).is_ok() {
        let _ = cr.paint_with_alpha(alpha);
    }
    cr.restore().ok();
    true
}

fn premultiply(channel: u8, alpha: u8) -> u8 {
    ((u16::from(channel) * u16::from(alpha) + 127) / 255) as u8
}

fn draw_placeholder(cr: &Context, item: &DockItem, size: f64, alpha: f64) {
    cr.save().ok();
    rounded_rect(cr, 0.0, 0.0, size, size, size * 0.18);
    let hash = item.id.bytes().fold(0_u32, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(byte as u32)
    });
    let hue = (hash % 360) as f64 / 360.0;
    let (red, green, blue) = hsl_to_rgb(hue, 0.54, 0.48);
    cr.set_source_rgba(red, green, blue, alpha);
    let _ = cr.fill_preserve();
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.42 * alpha);
    cr.set_line_width(1.4);
    let _ = cr.stroke();

    let label = item
        .name
        .chars()
        .find(|ch| ch.is_alphanumeric())
        .unwrap_or('?')
        .to_uppercase()
        .collect::<String>();
    cr.select_font_face("Sans", FontSlant::Normal, FontWeight::Bold);
    cr.set_font_size(size * 0.48);
    let extents = cr.text_extents(&label).ok();
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.92 * alpha);
    let x = extents
        .as_ref()
        .map(|e| size / 2.0 - (e.width() / 2.0 + e.x_bearing()))
        .unwrap_or(size * 0.34);
    let y = extents
        .as_ref()
        .map(|e| size / 2.0 - (e.height() / 2.0 + e.y_bearing()))
        .unwrap_or(size * 0.62);
    cr.move_to(x, y);
    let _ = cr.show_text(&label);
    cr.restore().ok();
}

fn draw_indicator(cr: &Context, rect: Rect, color: Color, active: bool) {
    let y = rect.y + rect.height + 7.0;
    let radius_x = if active { 7.0 } else { 4.5 };
    let radius_y = if active { 2.8 } else { 2.2 };
    cr.save().ok();
    cr.translate(rect.center_x(), y);
    cr.scale(radius_x, radius_y);
    cr.arc(0.0, 0.0, 1.0, 0.0, std::f64::consts::TAU);
    cr.set_source_rgba(
        color.red,
        color.green,
        color.blue,
        if active { 0.95 } else { 0.55 },
    );
    let _ = cr.fill();
    cr.restore().ok();
}

fn draw_leopard_indicator(
    cr: &Context,
    rect: Rect,
    layout: &DockLayout,
    theme: &Theme,
    active: bool,
) {
    let y = leopard_indicator_center_y(&layout.shelf, theme);
    let radius_x = if active { 5.4 } else { 3.4 };
    let radius_y = if active { 1.75 } else { 1.25 };
    let color = theme.indicator;

    cr.save().ok();
    cr.translate(rect.center_x(), y);
    cr.scale(radius_x * 2.1, radius_y * 2.4);
    cr.arc(0.0, 0.0, 1.0, 0.0, std::f64::consts::TAU);
    cr.set_source_rgba(
        color.red,
        color.green,
        color.blue,
        if active { 0.18 } else { 0.10 },
    );
    let _ = cr.fill();
    cr.restore().ok();

    cr.save().ok();
    cr.translate(rect.center_x(), y);
    cr.scale(radius_x, radius_y);
    cr.arc(0.0, 0.0, 1.0, 0.0, std::f64::consts::TAU);
    cr.set_source_rgba(
        color.red,
        color.green,
        color.blue,
        if active { 0.92 } else { 0.58 },
    );
    let _ = cr.fill();
    cr.restore().ok();

    cr.save().ok();
    cr.translate(rect.center_x(), y - radius_y * 0.22);
    cr.scale(radius_x * 0.48, radius_y * 0.30);
    cr.arc(0.0, 0.0, 1.0, 0.0, std::f64::consts::TAU);
    cr.set_source_rgba(1.0, 1.0, 1.0, if active { 0.58 } else { 0.34 });
    let _ = cr.fill();
    cr.restore().ok();
}

fn leopard_indicator_center_y(shelf: &Rect, theme: &Theme) -> f64 {
    let geom = compute_perspective_shelf_geometry(shelf, theme);
    (geom.lip_y - shelf.height * 0.08).clamp(geom.horizon_y + 2.0, geom.bottom_y - 2.5)
}

fn draw_badge(cr: &Context, rect: Rect, count: u32, color: Color) {
    let text = count.min(99).to_string();
    let width = 22.0_f64.max(14.0 + text.len() as f64 * 8.0);
    let height = 20.0;
    let x = rect.x + rect.width - width * 0.82;
    let y = rect.y + 2.0;

    cr.save().ok();
    rounded_rect(cr, x, y, width, height, height / 2.0);
    set_color(cr, color);
    let _ = cr.fill_preserve();
    cr.set_line_width(1.2);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.84);
    let _ = cr.stroke();
    cr.select_font_face("Sans", FontSlant::Normal, FontWeight::Bold);
    cr.set_font_size(12.0);
    let extents = cr.text_extents(&text).ok();
    cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
    let text_x = extents
        .as_ref()
        .map(|e| x + width / 2.0 - (e.width() / 2.0 + e.x_bearing()))
        .unwrap_or(x + 6.0);
    let text_y = extents
        .as_ref()
        .map(|e| y + height / 2.0 - (e.height() / 2.0 + e.y_bearing()))
        .unwrap_or(y + 14.0);
    cr.move_to(text_x, text_y);
    let _ = cr.show_text(&text);
    cr.restore().ok();
}

fn load_icon(name: &str, size: i32) -> Option<Pixbuf> {
    let path = Path::new(name);
    if path.is_absolute() && path.exists() {
        return Pixbuf::from_file_at_scale(path, size, size, true).ok();
    }

    for candidate in icon_candidates(name) {
        if candidate.exists()
            && let Ok(pixbuf) = Pixbuf::from_file_at_scale(&candidate, size, size, true)
        {
            return Some(pixbuf);
        }
    }
    None
}

fn icon_candidates(name: &str) -> Vec<PathBuf> {
    let clean = name
        .trim()
        .trim_end_matches(".png")
        .trim_end_matches(".svg")
        .trim_end_matches(".xpm");
    if clean.is_empty() {
        return Vec::new();
    }

    let data_dirs = env::var_os("XDG_DATA_DIRS")
        .map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_else(|| {
            vec![
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share"),
            ]
        });
    let themes = [
        "hicolor",
        "Papirus",
        "Papirus-Dark",
        "elementary-xfce",
        "Adwaita",
        "gnome",
    ];
    let buckets = [
        "scalable/apps",
        "symbolic/apps",
        "256x256/apps",
        "128x128/apps",
        "64x64/apps",
        "48x48/apps",
        "32x32/apps",
    ];
    let extensions = ["svg", "png", "xpm"];

    let mut candidates = Vec::new();
    for data_dir in data_dirs {
        for theme in themes {
            for bucket in buckets {
                for extension in extensions {
                    candidates.push(
                        data_dir
                            .join("icons")
                            .join(theme)
                            .join(bucket)
                            .join(format!("{clean}.{extension}")),
                    );
                }
            }
        }
        for extension in extensions {
            candidates.push(
                data_dir
                    .join("pixmaps")
                    .join(format!("{clean}.{extension}")),
            );
        }
    }
    candidates
}

fn set_color(cr: &Context, color: Color) {
    cr.set_source_rgba(color.red, color.green, color.blue, color.alpha);
}

fn add_stop(gradient: &LinearGradient, offset: f64, color: Color) {
    gradient.add_color_stop_rgba(offset, color.red, color.green, color.blue, color.alpha);
}

fn elapsed_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn rounded_rect(cr: &Context, x: f64, y: f64, width: f64, height: f64, radius: f64) {
    let radius = radius.min(width / 2.0).min(height / 2.0);
    cr.new_sub_path();
    cr.arc(
        x + width - radius,
        y + radius,
        radius,
        -std::f64::consts::FRAC_PI_2,
        0.0,
    );
    cr.arc(
        x + width - radius,
        y + height - radius,
        radius,
        0.0,
        std::f64::consts::FRAC_PI_2,
    );
    cr.arc(
        x + radius,
        y + height - radius,
        radius,
        std::f64::consts::FRAC_PI_2,
        std::f64::consts::PI,
    );
    cr.arc(
        x + radius,
        y + radius,
        radius,
        std::f64::consts::PI,
        std::f64::consts::PI * 1.5,
    );
    cr.close_path();
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    (
        hue_to_rgb(p, q, h + 1.0 / 3.0),
        hue_to_rgb(p, q, h),
        hue_to_rgb(p, q, h - 1.0 / 3.0),
    )
}

fn hue_to_rgb(p: f64, q: f64, mut t: f64) -> f64 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        p + (q - p) * 6.0 * t
    } else if t < 1.0 / 2.0 {
        q
    } else if t < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - t) * 6.0
    } else {
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, DockConfig};
    use crate::model::{DockItem, DockModel};
    use crate::theme::Theme;
    use gtk::cairo::Format;

    #[test]
    fn renderer_paints_non_empty_surface() {
        let config = Config::default().normalized();
        let theme = Theme::from_config(&config.theme);
        let model = DockModel {
            items: vec![DockItem {
                id: "test.desktop".to_string(),
                name: "Test".to_string(),
                desktop_id: Some("test.desktop".to_string()),
                startup_wm_class: None,
                icon_name: None,
                window_icon: None,
                pinned: true,
                windows: Vec::new(),
                active: false,
                urgent: false,
                badge: None,
            }],
        };
        let size = Renderer::desired_size(&model, &DockConfig::default(), &theme, None);
        let mut surface = ImageSurface::create(Format::ARgb32, size.0, size.1).unwrap();
        let mut renderer = Renderer::new();

        renderer.draw_for_test(&surface, &model, &config.dock, &theme);

        let data = surface.data().unwrap();
        assert!(data.iter().any(|byte| *byte != 0));
    }

    #[test]
    fn reserved_thickness_stays_compact_for_leopard_theme() {
        let config = Config::default().normalized();
        let theme = Theme::from_config(&config.theme);
        let model = DockModel::default();

        let reserved = Renderer::reserved_thickness(&model, &config.dock, &theme);

        assert!(reserved < config.dock.icon_size + 40);
    }

    #[test]
    fn leopard_plank_has_transparent_top_corner_and_dark_lip() {
        let config = Config::default().normalized();
        let theme = Theme::from_config(&config.theme);
        let mut surface = ImageSurface::create(Format::ARgb32, 240, 110).unwrap();
        let cr = Context::new(&surface).unwrap();
        let shelf = Rect {
            x: 24.0,
            y: 20.0,
            width: 192.0,
            height: 48.0,
        };

        draw_procedural_shelf_layer(&cr, &shelf, &theme);
        drop(cr);

        assert_eq!(alpha_at(&mut surface, 25, 21), 0);
        assert!(alpha_at(&mut surface, 120, 22) > 0);
        assert!(alpha_at(&mut surface, 120, 34) > 70);
        assert!(alpha_at(&mut surface, 120, 66) > 230);
        assert!(brightness_at(&mut surface, 120, 25) > brightness_at(&mut surface, 120, 37));
        assert!(brightness_at(&mut surface, 120, 66) < brightness_at(&mut surface, 120, 40));
    }

    #[test]
    fn leopard_reflection_is_clipped_to_reflection_band() {
        let config = Config::default().normalized();
        let theme = Theme::from_config(&config.theme);
        let model = DockModel {
            items: vec![DockItem {
                id: "test.desktop".to_string(),
                name: "Test".to_string(),
                desktop_id: Some("test.desktop".to_string()),
                startup_wm_class: None,
                icon_name: None,
                window_icon: None,
                pinned: true,
                windows: Vec::new(),
                active: false,
                urgent: false,
                badge: None,
            }],
        };
        let layout = Renderer::layout_for(&model, &config.dock, &theme, None);
        let mut icons = IconCache::disabled();
        let mut icon_surface = render_icon_surface(&model, &layout, &mut icons).unwrap();
        let mut surface =
            ImageSurface::create(Format::ARgb32, layout.size.0, layout.size.1).unwrap();
        let cr = Context::new(&surface).unwrap();
        let reflection = shelf_plane_reflection_rect(&layout, &theme);

        draw_shelf_plane_reflections(&cr, &layout, &theme, &mut icon_surface);
        drop(cr);

        assert!(rect_has_alpha(&mut surface, reflection));
        assert!(!rect_has_alpha(
            &mut surface,
            Rect {
                x: reflection.x,
                y: reflection.y + reflection.height + 1.0,
                width: reflection.width,
                height: 3.0,
            }
        ));
    }

    #[test]
    fn leopard_indicator_lands_inside_shelf_region() {
        let config = Config::default().normalized();
        let theme = Theme::from_config(&config.theme);
        let shelf = Rect {
            x: 18.0,
            y: 36.0,
            width: 164.0,
            height: 30.0,
        };
        let icon = Rect {
            x: 66.0,
            y: 2.0,
            width: 64.0,
            height: 64.0,
        };
        let layout = DockLayout {
            icons: Vec::new(),
            label: None,
            shelf,
            size: (200, 80),
        };
        let mut surface = ImageSurface::create(Format::ARgb32, 200, 80).unwrap();
        let cr = Context::new(&surface).unwrap();
        let y = leopard_indicator_center_y(&shelf, &theme);

        draw_leopard_indicator(&cr, icon, &layout, &theme, true);
        drop(cr);

        assert!(y > shelf.y);
        assert!(y < shelf.y + shelf.height);
        assert!(
            alpha_at(
                &mut surface,
                icon.center_x().round() as i32,
                y.round() as i32
            ) > 0
        );
    }

    fn alpha_at(surface: &mut ImageSurface, x: i32, y: i32) -> u8 {
        surface.flush();
        let stride = surface.stride() as usize;
        let data = surface.data().unwrap();
        data[y as usize * stride + x as usize * 4 + 3]
    }

    fn brightness_at(surface: &mut ImageSurface, x: i32, y: i32) -> u16 {
        surface.flush();
        let stride = surface.stride() as usize;
        let data = surface.data().unwrap();
        let offset = y as usize * stride + x as usize * 4;
        u16::from(data[offset]) + u16::from(data[offset + 1]) + u16::from(data[offset + 2])
    }

    fn rect_has_alpha(surface: &mut ImageSurface, rect: Rect) -> bool {
        surface.flush();
        let stride = surface.stride() as usize;
        let width = surface.width().max(0);
        let height = surface.height().max(0);
        let min_x = rect.x.max(0.0).floor() as i32;
        let max_x = (rect.x + rect.width).min(width as f64).ceil() as i32;
        let min_y = rect.y.max(0.0).floor() as i32;
        let max_y = (rect.y + rect.height).min(height as f64).ceil() as i32;
        let data = surface.data().unwrap();
        (min_y..max_y).any(|y| {
            (min_x..max_x).any(|x| {
                let offset = y as usize * stride + x as usize * 4 + 3;
                data[offset] != 0
            })
        })
    }
}
