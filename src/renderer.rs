mod badges;
mod icons;
mod indicators;
mod primitives;
mod reflections;
mod shelf;

pub use self::icons::IconCache;

use self::badges::draw_badge;
use self::icons::draw_icon_source;
use self::indicators::{
    draw_leopard_active_indicator, draw_leopard_running_indicator,
    leopard_active_indicator_center_y, leopard_running_indicator_center_y,
    leopard_running_indicator_size,
};
use self::primitives::{add_stop, elapsed_ms, rounded_rect, set_color};
use self::reflections::{
    draw_icon_reflections_on_shelf, draw_reflections, draw_shelf_plane_reflections,
    render_icon_surface, shelf_plane_reflection_rect, uses_shelf_plane_reflections,
};
use self::shelf::{
    compute_perspective_shelf_geometry, crystal_shelf_geometry, draw_front_lip,
    draw_glass_highlight_overlay, draw_glass_shelf_base, draw_leopard_shelf_strokes, draw_shadow,
    draw_shelf_section_separator, leopard_front_face_path, leopard_glass_plane_path,
    leopard_wedge_body_geometry,
};
use crate::config::DockConfig;
use crate::layout::{DockLayout, LayoutParams, Point, Rect, compute_layout};
use crate::model::{DockItem, DockModel};
use crate::theme::{Color, Theme};
use gtk::cairo::{Context, FontSlant, FontWeight, ImageSurface, LinearGradient};
use std::borrow::Cow;
use std::time::{Duration, Instant};

const SLOW_DRAW: Duration = Duration::from_millis(8);
const ICON_CLICK_RATIO: f64 = 1.0;
const ICON_HOVER_ENTER_RATIO: f64 = 1.0;
const ICON_HOVER_RETAIN_RATIO: f64 = 1.08;

#[derive(Debug, Default)]
pub struct Renderer {
    last_layout: DockLayout,
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
        let layout = Self::layout_for(model, config, theme, hover);
        Self::visual_regions_for_layout(model, &layout, config, theme)
    }

    pub fn visual_regions_for_layout(
        model: &DockModel,
        layout: &DockLayout,
        config: &DockConfig,
        theme: &Theme,
    ) -> Vec<Rect> {
        let icon_expansion = config.icon_size as f64 * config.zoom_strength + 10.0;
        let mut regions = Vec::with_capacity(layout.icons.len() * 2 + 4);
        regions.push(expand(layout.shelf, 8.0));
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
        if let Some(label) = layout.label.as_ref() {
            regions.push(expand(hover_label_region(model, &layout, label), 4.0));
        }
        regions
    }

    pub fn input_regions(
        model: &DockModel,
        config: &DockConfig,
        theme: &Theme,
        hover: Option<Point>,
    ) -> Vec<Rect> {
        let layout = Self::layout_for(model, config, theme, hover);
        Self::input_regions_for_layout(&layout)
    }

    pub fn input_regions_for_layout(layout: &DockLayout) -> Vec<Rect> {
        let mut regions = Vec::with_capacity(layout.icons.len() + 1);
        regions.push(expand(layout.shelf, 2.0));
        for icon in &layout.icons {
            regions.push(center_ratio_rect(icon.rect, ICON_HOVER_RETAIN_RATIO));
        }
        regions
    }

    pub fn hover_point_for(
        model: &DockModel,
        config: &DockConfig,
        theme: &Theme,
        point: Point,
        retaining: bool,
    ) -> Option<Point> {
        let ratio = if retaining {
            ICON_HOVER_RETAIN_RATIO
        } else {
            ICON_HOVER_ENTER_RATIO
        };
        let hover_layout = retaining.then_some(point);
        icon_hit_test_with_ratio(model, config, theme, hover_layout, point, ratio).map(|_| point)
    }

    pub fn icon_hit_test(
        model: &DockModel,
        config: &DockConfig,
        theme: &Theme,
        point: Point,
    ) -> Option<usize> {
        icon_hit_test_with_ratio(model, config, theme, None, point, ICON_CLICK_RATIO)
    }

    pub fn layout_for(
        model: &DockModel,
        config: &DockConfig,
        theme: &Theme,
        hover: Option<Point>,
    ) -> DockLayout {
        compute_layout(model, hover, layout_params(config, theme))
    }

    pub fn layout_for_container(
        model: &DockModel,
        config: &DockConfig,
        theme: &Theme,
        hover: Option<Point>,
        container_size: Option<(i32, i32)>,
    ) -> DockLayout {
        let layout = Self::layout_for(model, config, theme, hover);
        if let Some(size) = container_size {
            align_layout_to_size(layout, size)
        } else {
            layout
        }
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
        let layout = Self::layout_for_container(model, config, theme, hover, None);
        let resolved_icons = resolve_icons(model, &layout, None, None);
        self.draw_layout(
            cr,
            model,
            &layout,
            &resolved_icons,
            theme,
            icons,
            ShelfLayer::Procedural,
        );
        self.last_layout = layout;
        self.log_draw_time(started.elapsed(), model.items.len());
    }

    pub fn draw_overlay(&mut self, cr: &Context, frame: RenderFrame<'_>, icons: &mut IconCache) {
        let started = Instant::now();
        let mut layout = frame.layout.cloned().unwrap_or_else(|| {
            Self::layout_for_container(
                frame.model,
                frame.config,
                frame.theme,
                frame.hover,
                frame.container_size,
            )
        });
        if frame.icon_motion.is_some() || frame.icon_presence.is_some() {
            layout.label = None;
        }
        let resolved_icons =
            resolve_icons(frame.model, &layout, frame.icon_motion, frame.icon_presence);
        self.draw_layout(
            cr,
            frame.model,
            &layout,
            &resolved_icons,
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
        resolved_icons: &[ResolvedIcon<'_>],
        theme: &Theme,
        icons: &mut IconCache,
        shelf_layer: ShelfLayer,
    ) {
        clear(cr);
        if shelf_layer != ShelfLayer::None {
            draw_shadow(cr, &layout.shelf, theme);
            draw_glass_shelf_base(cr, &layout.shelf, theme);
            if theme.reflection_opacity > 0.0 {
                draw_icon_reflections_on_shelf(cr, resolved_icons, layout, theme, icons);
            }
            draw_glass_highlight_overlay(cr, &layout.shelf, theme);
            draw_front_lip(cr, &layout.shelf, theme);
            draw_leopard_shelf_strokes(cr, &layout.shelf, theme);
            draw_separator(cr, layout, theme);
            draw_icons(cr, layout, resolved_icons, theme, icons);
            draw_hover_label(cr, model, layout);
            return;
        }

        draw_reflections(cr, resolved_icons, theme, icons);
        draw_separator(cr, layout, theme);
        draw_icons(cr, layout, resolved_icons, theme, icons);
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
    pub layout: Option<&'a DockLayout>,
    pub shelf_layer: ShelfLayer,
    pub icon_motion: Option<&'a IconMotionFrame>,
    pub icon_presence: Option<&'a IconPresenceFrame>,
    pub container_size: Option<(i32, i32)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IconMotionFrame {
    pub rects: Vec<IconMotionRect>,
    pub floating_item_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IconMotionRect {
    pub item_key: String,
    pub rect: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IconPresenceFrame {
    pub current: Vec<IconPresenceRect>,
    pub ghosts: Vec<GhostIcon>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IconPresenceRect {
    pub item_key: String,
    pub rect: Rect,
    pub alpha: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GhostIcon {
    pub item: DockItem,
    pub rect: Rect,
    pub alpha: f64,
}

#[derive(Debug, Clone)]
struct ResolvedIcon<'a> {
    item_key: String,
    item: Cow<'a, DockItem>,
    rect: Rect,
    alpha: f64,
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

fn align_layout_to_size(mut layout: DockLayout, container_size: (i32, i32)) -> DockLayout {
    let width = container_size.0.max(layout.size.0);
    let height = container_size.1.max(layout.size.1);
    let dx = ((width - layout.size.0) as f64 / 2.0).max(0.0);
    let dy = ((height - layout.size.1) as f64 / 2.0).max(0.0);
    if dx == 0.0 && dy == 0.0 {
        layout.size = (width, height);
        return layout;
    }

    for icon in &mut layout.icons {
        icon.rect = translate_rect(icon.rect, dx, dy);
    }
    if let Some(label) = layout.label.as_mut() {
        label.rect = translate_rect(label.rect, dx, dy);
    }
    layout.shelf = translate_rect(layout.shelf, dx, dy);
    for section in &mut layout.sections {
        section.rect = translate_rect(section.rect, dx, dy);
    }
    if let Some(separator) = layout.separator.as_mut() {
        separator.rect = translate_rect(separator.rect, dx, dy);
    }
    layout.size = (width, height);
    layout
}

fn expand(rect: Rect, amount: f64) -> Rect {
    Rect {
        x: rect.x - amount,
        y: rect.y - amount,
        width: rect.width + amount * 2.0,
        height: rect.height + amount * 2.0,
    }
}

fn translate_rect(rect: Rect, dx: f64, dy: f64) -> Rect {
    Rect {
        x: rect.x + dx,
        y: rect.y + dy,
        width: rect.width,
        height: rect.height,
    }
}

fn center_ratio_rect(rect: Rect, ratio: f64) -> Rect {
    let ratio = ratio.max(0.0);
    let inset_x = rect.width * (1.0 - ratio) / 2.0;
    let inset_y = rect.height * (1.0 - ratio) / 2.0;
    Rect {
        x: rect.x + inset_x,
        y: rect.y + inset_y,
        width: rect.width * ratio,
        height: rect.height * ratio,
    }
}

fn icon_hit_test_with_ratio(
    model: &DockModel,
    config: &DockConfig,
    theme: &Theme,
    hover: Option<Point>,
    point: Point,
    ratio: f64,
) -> Option<usize> {
    let params = layout_params(config, theme);
    let layout = compute_layout(model, hover, params);
    layout
        .icons
        .iter()
        .find(|icon| center_ratio_rect(icon.rect, ratio).contains(point))
        .map(|icon| icon.item_index)
}

fn resolve_icons<'a>(
    model: &'a DockModel,
    layout: &DockLayout,
    motion: Option<&IconMotionFrame>,
    presence: Option<&'a IconPresenceFrame>,
) -> Vec<ResolvedIcon<'a>> {
    let mut resolved = layout
        .icons
        .iter()
        .filter_map(|icon| {
            let item = model.items.get(icon.item_index)?;
            let item_key = item.config_key();
            let mut rect = icon.rect;
            let mut alpha = 1.0;
            if let Some(presence_rect) = presence.and_then(|frame| {
                frame
                    .current
                    .iter()
                    .find(|current| current.item_key.eq_ignore_ascii_case(&item_key))
            }) {
                rect = presence_rect.rect;
                alpha = presence_rect.alpha;
            } else if let Some(motion_rect) = motion.and_then(|frame| {
                frame
                    .rects
                    .iter()
                    .find(|motion_rect| motion_rect.item_key.eq_ignore_ascii_case(&item_key))
            }) {
                rect = motion_rect.rect;
            }
            Some(ResolvedIcon {
                item_key,
                item: Cow::Borrowed(item),
                rect,
                alpha,
            })
        })
        .collect::<Vec<_>>();

    if let Some(presence) = presence {
        resolved.extend(presence.ghosts.iter().map(|ghost| ResolvedIcon {
            item_key: ghost.item.config_key(),
            item: Cow::Borrowed(&ghost.item),
            rect: ghost.rect,
            alpha: ghost.alpha,
        }));
    }

    if let Some(item_key) = motion.and_then(|frame| frame.floating_item_key.as_deref())
        && let Some(index) = resolved
            .iter()
            .position(|icon| icon.item_key.eq_ignore_ascii_case(item_key))
    {
        let icon = resolved.remove(index);
        resolved.push(icon);
    }

    resolved
}

fn hover_label_region(
    model: &DockModel,
    layout: &DockLayout,
    label: &crate::layout::LabelLayout,
) -> Rect {
    let estimated_text_width = model
        .items
        .get(label.item_index)
        .map(|item| item.name.chars().count() as f64 * 8.0)
        .unwrap_or(56.0);
    let width = (estimated_text_width + 16.0)
        .max(38.0)
        .min((layout.size.0 as f64 - 8.0).max(38.0));
    let max_x = (layout.size.0 as f64 - width - 4.0).max(4.0);
    Rect {
        x: (label.rect.center_x() - width / 2.0).clamp(4.0, max_x),
        y: label.rect.y,
        width,
        height: label.rect.height + 6.0,
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
    draw_shadow(cr, shelf, theme);
    draw_glass_shelf_base(cr, shelf, theme);
    draw_glass_highlight_overlay(cr, shelf, theme);
    draw_front_lip(cr, shelf, theme);
    draw_leopard_shelf_strokes(cr, shelf, theme);
}

fn draw_separator(cr: &Context, layout: &DockLayout, theme: &Theme) {
    let Some(separator) = layout.separator.as_ref() else {
        return;
    };
    draw_shelf_section_separator(cr, &layout.shelf, separator, theme);
}

fn draw_icons(
    cr: &Context,
    layout: &DockLayout,
    resolved_icons: &[ResolvedIcon<'_>],
    theme: &Theme,
    icons: &mut IconCache,
) {
    draw_icon_art(cr, resolved_icons, icons);
    for icon in resolved_icons {
        let item = icon.item.as_ref();
        if !item.is_application() {
            continue;
        }
        if item.active {
            draw_leopard_active_indicator(cr, icon.rect, layout, theme, icon.alpha);
        } else if item.is_running() {
            draw_leopard_running_indicator(cr, icon.rect, layout, theme, false, icon.alpha);
        }
        if let Some(badge) = item.badge {
            draw_badge(cr, icon.rect, badge, theme.badge, icon.alpha);
        }
    }
}

fn draw_icon_art(cr: &Context, resolved_icons: &[ResolvedIcon<'_>], icons: &mut IconCache) {
    for icon in resolved_icons {
        let item = icon.item.as_ref();
        cr.save().ok();
        cr.translate(icon.rect.x, icon.rect.y);
        cr.scale(icon.rect.width / icon.rect.height, 1.0);
        draw_icon_source(cr, item, icon.rect.height as i32, icons, icon.alpha);
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
    cr.set_font_size(12.0);
    let extents = cr.text_extents(&item.name).ok();
    let text_width = extents.as_ref().map(|e| e.width()).unwrap_or(56.0);
    let text_height = extents.as_ref().map(|e| e.height()).unwrap_or(11.0);
    let width = (text_width + 16.0)
        .max(38.0)
        .min((layout.size.0 as f64 - 8.0).max(38.0));
    let height = 20.0_f64.min((label.rect.height - 4.0).max(18.0));
    let pointer_height = 5.0;
    let pointer_width = 10.0;
    let max_x = (layout.size.0 as f64 - width - 4.0).max(4.0);
    let x = (label.rect.center_x() - width / 2.0).clamp(4.0, max_x);
    let y = label.rect.y + 1.0;
    let pointer_x = label.rect.center_x().clamp(x + 10.0, x + width - 10.0);

    hover_label_path(
        cr,
        x,
        y + 1.8,
        width,
        height,
        5.5,
        pointer_x,
        pointer_width,
        pointer_height,
    );
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.30);
    let _ = cr.fill();
    hover_label_path(
        cr,
        x,
        y + 0.8,
        width,
        height,
        5.5,
        pointer_x,
        pointer_width,
        pointer_height,
    );
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.18);
    let _ = cr.fill();

    hover_label_path(
        cr,
        x,
        y,
        width,
        height,
        5.5,
        pointer_x,
        pointer_width,
        pointer_height,
    );
    let fill = LinearGradient::new(0.0, y, 0.0, y + height + pointer_height);
    add_stop(&fill, 0.00, Color::rgba(0.30, 0.31, 0.32, 0.94));
    add_stop(&fill, 0.52, Color::rgba(0.18, 0.19, 0.20, 0.95));
    add_stop(&fill, 1.00, Color::rgba(0.07, 0.08, 0.09, 0.96));
    let _ = cr.set_source(&fill);
    let _ = cr.fill_preserve();
    cr.set_line_width(1.0);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.20);
    let _ = cr.stroke();

    rounded_rect(cr, x + 1.0, y + 1.0, width - 2.0, height * 0.42, 4.5);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.07);
    let _ = cr.fill();

    let text_x = extents
        .as_ref()
        .map(|e| x + width / 2.0 - (e.width() / 2.0 + e.x_bearing()))
        .unwrap_or(x + 8.0);
    let text_y = extents
        .as_ref()
        .map(|e| y + height / 2.0 - (text_height / 2.0 + e.y_bearing()))
        .unwrap_or(y + 14.0);
    cr.move_to(text_x + 0.0, text_y + 1.0);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.45);
    let _ = cr.show_text(&item.name);
    cr.move_to(text_x, text_y);
    cr.set_source_rgba(0.96, 0.97, 0.98, 0.98);
    let _ = cr.show_text(&item.name);
    cr.restore().ok();
}

fn hover_label_path(
    cr: &Context,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    radius: f64,
    pointer_x: f64,
    pointer_width: f64,
    pointer_height: f64,
) {
    rounded_rect(cr, x, y, width, height, radius);
    let pointer_top = y + height - 0.5;
    cr.move_to(pointer_x - pointer_width / 2.0, pointer_top);
    cr.line_to(pointer_x, pointer_top + pointer_height);
    cr.line_to(pointer_x + pointer_width / 2.0, pointer_top);
    cr.close_path();
}

#[cfg(test)]
mod tests;
