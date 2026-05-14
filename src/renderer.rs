use crate::config::DockConfig;
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
        (icon_size + icon_size * theme.shelf_height_ratio * 0.72 + 8.0).ceil() as u32
    }

    pub fn visual_regions(
        model: &DockModel,
        config: &DockConfig,
        theme: &Theme,
        _hover: Option<Point>,
    ) -> Vec<Rect> {
        let params = layout_params(config, theme);
        let layout = compute_layout(model, None, params);
        let icon_expansion = config.icon_size as f64 * config.zoom_strength + 10.0;
        let mut regions = Vec::with_capacity(layout.icons.len() * 2 + 4);
        regions.push(Rect {
            x: 0.0,
            y: 0.0,
            width: layout.size.0 as f64,
            height: params.label_height + 10.0,
        });
        regions.push(expand(layout.shelf, 8.0));
        for icon in &layout.icons {
            regions.push(expand(icon.rect, icon_expansion));
            if theme.reflection_height > 0.0 {
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
        match shelf_layer {
            ShelfLayer::None => {}
            ShelfLayer::Procedural => draw_procedural_shelf_layer(cr, &layout.shelf, theme),
            ShelfLayer::Texture2d => {
                if !draw_texture_shelf_layer(cr, &layout.shelf, theme) {
                    draw_procedural_shelf_layer(cr, &layout.shelf, theme);
                }
            }
        }
        draw_reflections(cr, model, layout, theme, icons);
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
    rounded_rect(
        cr,
        shelf.x + shelf.height * 0.18,
        shelf.y + shelf.height * 0.58,
        shelf.width - shelf.height * 0.36,
        shelf.height * 0.54,
        shelf.height * 0.18,
    );
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.22);
    let _ = cr.fill();
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
    for icon in &layout.icons {
        let item = &model.items[icon.item_index];
        cr.save().ok();
        cr.translate(icon.rect.x, icon.rect.y);
        cr.scale(icon.rect.width / icon.rect.height, 1.0);
        draw_icon_source(cr, item, icon.rect.height as i32, icons, 1.0);
        cr.restore().ok();

        if item.is_running() {
            draw_indicator(cr, icon.rect, theme.indicator, item.active);
        }
        if let Some(badge) = item.badge {
            draw_badge(cr, icon.rect, badge, theme.badge);
        }
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
}
