use super::primitives::{elapsed_ms, hsl_to_rgb, rounded_rect};
use crate::model::{DockItem, WindowIcon};
use gtk::cairo::{Context, FontSlant, FontWeight, Format, ImageSurface};
use gtk::gdk::prelude::GdkCairoContextExt;
use gtk::gdk_pixbuf::Pixbuf;
use gtk::gio::prelude::FileExt;
use gtk::{IconLookupFlags, IconTheme, TextDirection, gdk};
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::path::{Path, PathBuf};
use std::time::Instant;

const ICON_CACHE_SIZE: i32 = 192;
const FALLBACK_ICON_CACHE_KEY: &str = "__osdockx_fallback_icon__";
const FALLBACK_ICON_NAMES: &[&str] = &[
    "dialog-question",
    "help-browser",
    "unknown",
    "image-missing",
    "application-x-executable",
];

#[derive(Debug, Default)]
pub struct IconCache {
    enabled: bool,
    cache: HashMap<String, Option<Pixbuf>>,
    custom_icons: BTreeMap<String, String>,
}

impl IconCache {
    pub fn new() -> Self {
        Self {
            enabled: true,
            cache: HashMap::new(),
            custom_icons: BTreeMap::new(),
        }
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            cache: HashMap::new(),
            custom_icons: BTreeMap::new(),
        }
    }

    pub fn clear(&mut self) {
        self.cache.clear();
    }

    pub fn set_custom_icons(&mut self, custom_icons: &BTreeMap<String, String>) {
        if &self.custom_icons != custom_icons {
            self.custom_icons = custom_icons.clone();
            self.clear();
        }
    }

    fn pixbuf_for(&mut self, item: &DockItem) -> Option<Pixbuf> {
        if !self.enabled {
            return None;
        }
        let lookup_keys = icon_lookup_keys(item);
        let mut keys = Vec::new();
        if let Some(custom_icon) = lookup_keys
            .iter()
            .find_map(|key| self.custom_icons.get(key).cloned())
        {
            keys.push(custom_icon);
        }
        keys.extend(lookup_keys);

        for key in keys {
            if let Some(value) = self.cache.get(&key) {
                if value.is_some() {
                    return value.clone();
                }
                continue;
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
            if loaded.is_some() {
                return loaded;
            }
        }
        None
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

    if let Some(pixbuf) = icons.fallback_pixbuf() {
        let scale_x = size as f64 / pixbuf.width() as f64;
        let scale_y = size as f64 / pixbuf.height() as f64;
        cr.save().ok();
        cr.scale(scale_x, scale_y);
        cr.set_source_pixbuf(&pixbuf, 0.0, 0.0);
        let _ = cr.paint_with_alpha(alpha);
        cr.restore().ok();
        return;
    }

    draw_placeholder(cr, item, size as f64, alpha);
}

impl IconCache {
    fn fallback_pixbuf(&mut self) -> Option<Pixbuf> {
        if let Some(value) = self.cache.get(FALLBACK_ICON_CACHE_KEY) {
            return value.clone();
        }
        let loaded = load_fallback_themed_icon(ICON_CACHE_SIZE);
        self.cache
            .insert(FALLBACK_ICON_CACHE_KEY.to_string(), loaded.clone());
        loaded
    }
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

    load_themed_icon(name, size).or_else(|| {
        for candidate in pixmap_candidates(name) {
            if candidate.exists()
                && let Ok(pixbuf) = Pixbuf::from_file_at_scale(&candidate, size, size, true)
            {
                return Some(pixbuf);
            }
        }
        None
    })
}

fn load_themed_icon(name: &str, size: i32) -> Option<Pixbuf> {
    let icon_name = themed_icon_name(name)?;
    let display = gdk::Display::default()?;
    let icon_theme = IconTheme::for_display(&display);
    if !icon_theme.has_icon(&icon_name) {
        return None;
    }

    let paintable = icon_theme.lookup_icon(
        &icon_name,
        &[],
        size,
        1,
        TextDirection::None,
        IconLookupFlags::empty(),
    );
    let path = paintable.file()?.path()?;
    Pixbuf::from_file_at_scale(path, size, size, true).ok()
}

fn load_fallback_themed_icon(size: i32) -> Option<Pixbuf> {
    for name in FALLBACK_ICON_NAMES {
        if let Some(icon) = load_themed_icon(name, size) {
            return Some(icon);
        }
    }
    None
}

fn pixmap_candidates(name: &str) -> Vec<PathBuf> {
    let Some(clean) = themed_icon_name(name) else {
        return Vec::new();
    };
    let extensions = ["svg", "png", "xpm"];

    let mut candidates = Vec::new();
    for data_dir in icon_data_dirs() {
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

fn icon_lookup_keys(item: &DockItem) -> Vec<String> {
    let mut keys = Vec::new();
    if item.is_downloads_applet() {
        for icon_name in ["folder-download", "folder-downloads", "folder", "inode-directory"] {
            push_icon_key(&mut keys, Some(icon_name));
        }
    } else if item.is_trash_applet() {
        for icon_name in ["user-trash-full", "user-trash", "trashcan_full", "trashcan_empty"] {
            push_icon_key(&mut keys, Some(icon_name));
        }
    }
    push_icon_key(&mut keys, item.icon_name.as_deref());
    push_icon_key(&mut keys, item.startup_wm_class.as_deref());
    push_icon_key(&mut keys, item.desktop_id.as_deref());
    push_icon_key(&mut keys, Some(&item.config_key()));
    push_icon_key(&mut keys, Some(&item.id));
    keys
}

fn push_icon_key(keys: &mut Vec<String>, value: Option<&str>) {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    push_unique(keys, value.to_string());

    if !Path::new(value).is_absolute() {
        if let Some(stem) = value.strip_suffix(".desktop") {
            push_unique(keys, stem.to_string());
            let lower = stem.to_ascii_lowercase();
            if lower != stem {
                push_unique(keys, lower);
            }
        }
        if let Some(stem) = value
            .trim_end_matches(".desktop")
            .strip_prefix("application-")
        {
            push_unique(keys, stem.to_string());
        }
        let lower = value.to_ascii_lowercase();
        if lower != value {
            push_unique(keys, lower);
        }
    }
}

fn push_unique(keys: &mut Vec<String>, value: String) {
    if !keys.contains(&value) {
        keys.push(value);
    }
}

fn themed_icon_name(name: &str) -> Option<String> {
    let clean = name.trim();
    if clean.is_empty() || Path::new(clean).is_absolute() {
        return None;
    }

    let path = Path::new(clean);
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    if matches!(extension.as_deref(), Some("png" | "svg" | "xpm"))
        && let Some(stem) = path.file_stem().and_then(|stem| stem.to_str())
    {
        return Some(stem.to_string());
    }

    Some(clean.to_string())
}

fn icon_data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(data_home) = env::var_os("XDG_DATA_HOME") {
        dirs.push(PathBuf::from(data_home));
    } else if let Some(home) = env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share"));
    }

    dirs.extend(
        env::var_os("XDG_DATA_DIRS")
            .map(|value| env::split_paths(&value).collect::<Vec<_>>())
            .unwrap_or_else(|| {
                vec![
                    PathBuf::from("/usr/local/share"),
                    PathBuf::from("/usr/share"),
                ]
            }),
    );
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_item() -> DockItem {
        DockItem {
            id: "org.example.App.desktop".to_string(),
            name: "Example".to_string(),
            desktop_id: Some("org.example.App.desktop".to_string()),
            startup_wm_class: Some("ExampleApp".to_string()),
            icon_name: Some("org.example.App.png".to_string()),
            window_icon: None,
            pinned: true,
            windows: Vec::new(),
            active: false,
            urgent: false,
            badge: None,
        }
    }

    #[test]
    fn themed_icon_name_strips_file_extensions_only() {
        assert_eq!(
            themed_icon_name("org.example.App.png"),
            Some("org.example.App".to_string())
        );
        assert_eq!(
            themed_icon_name("org.example.App"),
            Some("org.example.App".to_string())
        );
        assert_eq!(themed_icon_name("/tmp/app.png"), None);
        assert_eq!(themed_icon_name(" "), None);
    }

    #[test]
    fn lookup_keys_keep_desktop_icon_then_theme_fallback_names() {
        let keys = icon_lookup_keys(&test_item());
        assert_eq!(keys[0], "org.example.App.png");
        assert!(keys.contains(&"org.example.app.png".to_string()));
        assert!(keys.contains(&"ExampleApp".to_string()));
        assert!(keys.contains(&"exampleapp".to_string()));
        assert!(keys.contains(&"org.example.App.desktop".to_string()));
        assert!(keys.contains(&"org.example.App".to_string()));
    }
}
