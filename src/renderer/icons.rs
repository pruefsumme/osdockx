use super::primitives::{elapsed_ms, hsl_to_rgb, rounded_rect};
use crate::model::{DockItem, WindowIcon};
use gtk::cairo::{Context, FontSlant, FontWeight, Format, ImageSurface};
use gtk::gdk::prelude::GdkCairoContextExt;
use gtk::gdk_pixbuf::Pixbuf;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::time::Instant;

const ICON_CACHE_SIZE: i32 = 192;

#[derive(Debug, Default)]
pub struct IconCache {
    enabled: bool,
    cache: HashMap<String, Option<Pixbuf>>,
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

pub(super) fn draw_icon_source(
    cr: &Context,
    item: &DockItem,
    size: i32,
    icons: &mut IconCache,
    alpha: f64,
) {
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
        .unwrap_or_else(|| vec![PathBuf::from("/usr/local/share"), PathBuf::from("/usr/share")]);
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
            candidates.push(data_dir.join("pixmaps").join(format!("{clean}.{extension}")));
        }
    }
    candidates
}