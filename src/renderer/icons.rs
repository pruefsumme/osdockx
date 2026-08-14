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
const SURFACE_CACHE_LIMIT: usize = 32 * 1024 * 1024;
const FALLBACK_ICON_CACHE_KEY: &str = "__osdockx_fallback_icon__";
const FALLBACK_ICON_NAMES: &[&str] = &[
    "dialog-question",
    "help-browser",
    "unknown",
    "image-missing",
    "application-x-executable",
];

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum IconSourceIdentity {
    Lookup { key: String, generation: u64 },
    Window { signature: u64 },
    Fallback { generation: u64 },
    Placeholder { key: String, generation: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct RasterIdentity {
    source: IconSourceIdentity,
    pixel_width: i32,
    pixel_height: i32,
    scale_x_bits: u64,
    scale_y_bits: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct ReflectionCacheKey {
    pub(super) raster: RasterIdentity,
    pub(super) icon_pixel_width: i32,
    pub(super) icon_pixel_height: i32,
    pub(super) reflection_pixel_height: i32,
    pub(super) blur_bits: u64,
    pub(super) opacity_bits: u64,
    pub(super) source_ratio_bits: u64,
    pub(super) origin_phase_x_bits: u64,
    pub(super) origin_phase_y_bits: u64,
    pub(super) generation: u64,
}

#[derive(Debug)]
struct SurfaceEntry {
    surface: ImageSurface,
    bytes: usize,
    last_used: u64,
}

#[derive(Debug, Clone)]
enum IconSource {
    Pixbuf(Pixbuf),
    Window(WindowIcon),
    Placeholder,
}

#[derive(Debug)]
pub struct IconCache {
    enabled: bool,
    cache: HashMap<String, Option<Pixbuf>>,
    custom_icons: BTreeMap<String, String>,
    generation: u64,
    raster_cache: HashMap<RasterIdentity, SurfaceEntry>,
    reflection_cache: HashMap<ReflectionCacheKey, SurfaceEntry>,
    window_surfaces: HashMap<u64, SurfaceEntry>,
    surface_bytes: usize,
    surface_limit: usize,
    use_clock: u64,
}

impl Default for IconCache {
    fn default() -> Self {
        Self::new()
    }
}

impl IconCache {
    pub fn new() -> Self {
        Self {
            enabled: true,
            cache: HashMap::new(),
            custom_icons: BTreeMap::new(),
            generation: 0,
            raster_cache: HashMap::new(),
            reflection_cache: HashMap::new(),
            window_surfaces: HashMap::new(),
            surface_bytes: 0,
            surface_limit: SURFACE_CACHE_LIMIT,
            use_clock: 0,
        }
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            cache: HashMap::new(),
            custom_icons: BTreeMap::new(),
            generation: 0,
            raster_cache: HashMap::new(),
            reflection_cache: HashMap::new(),
            window_surfaces: HashMap::new(),
            surface_bytes: 0,
            surface_limit: 0,
            use_clock: 0,
        }
    }

    pub fn clear(&mut self) {
        self.cache.clear();
        self.invalidate_surfaces();
    }

    pub fn invalidate_surfaces(&mut self) {
        self.raster_cache.clear();
        self.reflection_cache.clear();
        self.window_surfaces.clear();
        self.surface_bytes = 0;
        self.generation = self.generation.wrapping_add(1);
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

    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    pub(super) fn raster_surface(
        &mut self,
        item: &DockItem,
        size: i32,
        device_scale: (f64, f64),
    ) -> Option<(RasterIdentity, ImageSurface)> {
        if !self.enabled || size <= 0 {
            return None;
        }
        let scale_x = device_scale.0.max(1.0);
        let scale_y = device_scale.1.max(1.0);
        let (source_identity, source) = self.source_for(item);
        let identity = RasterIdentity {
            source: source_identity,
            pixel_width: (size as f64 * scale_x).ceil().max(1.0) as i32,
            pixel_height: (size as f64 * scale_y).ceil().max(1.0) as i32,
            scale_x_bits: scale_x.to_bits(),
            scale_y_bits: scale_y.to_bits(),
        };
        let used = self.next_use();
        if let Some(entry) = self.raster_cache.get_mut(&identity) {
            entry.last_used = used;
            return Some((identity, entry.surface.clone()));
        }

        let surface = self.build_raster(item, &source, &identity, scale_x, scale_y)?;
        let bytes = surface_bytes(&surface);
        self.insert_raster(identity.clone(), surface.clone(), bytes, used);
        Some((identity, surface))
    }

    fn paint_source(&mut self, cr: &Context, item: &DockItem, size: i32, alpha: f64) -> bool {
        if !self.enabled {
            return false;
        }
        let (_, source) = self.source_for(item);
        match source {
            IconSource::Pixbuf(pixbuf) => paint_pixbuf(cr, &pixbuf, size as f64, alpha),
            IconSource::Window(icon) => {
                let Some(surface) = self.raw_window_surface(&icon) else {
                    return false;
                };
                paint_surface(cr, &surface, size as f64, icon.width, icon.height, alpha);
            }
            IconSource::Placeholder => draw_placeholder(cr, item, size as f64, alpha),
        }
        true
    }

    pub(super) fn reflection_surface(
        &mut self,
        key: &ReflectionCacheKey,
    ) -> Option<ImageSurface> {
        let used = self.next_use();
        let entry = self.reflection_cache.get_mut(key)?;
        entry.last_used = used;
        crate::perf::record_reflection_hit();
        Some(entry.surface.clone())
    }

    pub(super) fn insert_reflection(
        &mut self,
        key: ReflectionCacheKey,
        surface: ImageSurface,
    ) {
        let bytes = surface_bytes(&surface);
        if bytes > self.surface_limit {
            return;
        }
        let used = self.next_use();
        if let Some(previous) = self.reflection_cache.insert(
            key,
            SurfaceEntry {
                surface,
                bytes,
                last_used: used,
            },
        ) {
            self.surface_bytes = self.surface_bytes.saturating_sub(previous.bytes);
        }
        self.surface_bytes = self.surface_bytes.saturating_add(bytes);
        self.evict_to_limit();
    }

    fn source_for(&mut self, item: &DockItem) -> (IconSourceIdentity, IconSource) {
        if let Some((key, pixbuf)) = self.pixbuf_source_for(item) {
            return (
                IconSourceIdentity::Lookup {
                    key,
                    generation: self.generation,
                },
                IconSource::Pixbuf(pixbuf),
            );
        }
        if let Some(icon) = item.window_icon.as_ref() {
            return (
                IconSourceIdentity::Window {
                    signature: icon.signature(),
                },
                IconSource::Window(icon.clone()),
            );
        }
        if let Some(pixbuf) = self.fallback_pixbuf() {
            return (
                IconSourceIdentity::Fallback {
                    generation: self.generation,
                },
                IconSource::Pixbuf(pixbuf),
            );
        }
        (
            IconSourceIdentity::Placeholder {
                key: item.id.clone(),
                generation: self.generation,
            },
            IconSource::Placeholder,
        )
    }

    fn pixbuf_source_for(&mut self, item: &DockItem) -> Option<(String, Pixbuf)> {
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
                if let Some(pixbuf) = value {
                    return Some((key, pixbuf.clone()));
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
            self.cache.insert(key.clone(), loaded.clone());
            if let Some(pixbuf) = loaded {
                return Some((key, pixbuf));
            }
        }
        None
    }

    fn build_raster(
        &mut self,
        item: &DockItem,
        source: &IconSource,
        identity: &RasterIdentity,
        scale_x: f64,
        scale_y: f64,
    ) -> Option<ImageSurface> {
        let surface = ImageSurface::create(
            Format::ARgb32,
            identity.pixel_width,
            identity.pixel_height,
        )
        .ok()?;
        surface.set_device_scale(scale_x, scale_y);
        let cr = Context::new(&surface).ok()?;
        let size = identity.pixel_height as f64 / scale_y;
        match source {
            IconSource::Pixbuf(pixbuf) => paint_pixbuf(&cr, pixbuf, size, 1.0),
            IconSource::Window(icon) => {
                let raw = self.raw_window_surface(icon)?;
                paint_surface(&cr, &raw, size, icon.width, icon.height, 1.0);
            }
            IconSource::Placeholder => draw_placeholder(&cr, item, size, 1.0),
        }
        surface.flush();
        Some(surface)
    }

    fn raw_window_surface(&mut self, icon: &WindowIcon) -> Option<ImageSurface> {
        let signature = icon.signature();
        let used = self.next_use();
        if let Some(entry) = self.window_surfaces.get_mut(&signature) {
            entry.last_used = used;
            return Some(entry.surface.clone());
        }
        let surface = window_icon_surface(icon)?;
        let bytes = surface_bytes(&surface);
        if bytes <= self.surface_limit {
            self.window_surfaces.insert(
                signature,
                SurfaceEntry {
                    surface: surface.clone(),
                    bytes,
                    last_used: used,
                },
            );
            self.surface_bytes = self.surface_bytes.saturating_add(bytes);
            self.evict_to_limit();
        }
        Some(surface)
    }

    fn insert_raster(
        &mut self,
        identity: RasterIdentity,
        surface: ImageSurface,
        bytes: usize,
        used: u64,
    ) {
        if bytes > self.surface_limit {
            return;
        }
        self.raster_cache.insert(
            identity,
            SurfaceEntry {
                surface,
                bytes,
                last_used: used,
            },
        );
        self.surface_bytes = self.surface_bytes.saturating_add(bytes);
        self.evict_to_limit();
    }

    fn next_use(&mut self) -> u64 {
        self.use_clock = self.use_clock.wrapping_add(1);
        self.use_clock
    }

    fn evict_to_limit(&mut self) {
        while self.surface_bytes > self.surface_limit {
            enum Oldest {
                Raster(RasterIdentity),
                Reflection(ReflectionCacheKey),
                Window(u64),
            }
            let oldest_raster = self
                .raster_cache
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, entry)| (entry.last_used, Oldest::Raster(key.clone())));
            let oldest_reflection = self
                .reflection_cache
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, entry)| (entry.last_used, Oldest::Reflection(key.clone())));
            let oldest_window = self
                .window_surfaces
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, entry)| (entry.last_used, Oldest::Window(*key)));
            let Some((_, oldest)) = [oldest_raster, oldest_reflection, oldest_window]
                .into_iter()
                .flatten()
                .min_by_key(|(used, _)| *used)
            else {
                self.surface_bytes = 0;
                break;
            };
            let removed = match oldest {
                Oldest::Raster(key) => self.raster_cache.remove(&key),
                Oldest::Reflection(key) => self.reflection_cache.remove(&key),
                Oldest::Window(key) => self.window_surfaces.remove(&key),
            };
            if let Some(entry) = removed {
                self.surface_bytes = self.surface_bytes.saturating_sub(entry.bytes);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn test_surface_counts(&self) -> (usize, usize, usize) {
        (
            self.raster_cache.len(),
            self.reflection_cache.len(),
            self.window_surfaces.len(),
        )
    }

    #[cfg(test)]
    fn set_test_surface_limit(&mut self, limit: usize) {
        self.surface_limit = limit;
        self.evict_to_limit();
    }

    #[cfg(test)]
    fn test_surface_bytes(&self) -> usize {
        self.surface_bytes
    }
}

pub(super) fn draw_icon_source(
    cr: &Context,
    item: &DockItem,
    size: i32,
    icons: &mut IconCache,
    alpha: f64,
) {
    if icons.paint_source(cr, item, size, alpha) {
        return;
    }

    if let Some(pixbuf) = icons.pixbuf_for(item) {
        paint_pixbuf(cr, &pixbuf, size as f64, alpha);
        return;
    }

    if let Some(icon) = item.window_icon.as_ref()
        && draw_window_icon(cr, icon, size, alpha)
    {
        return;
    }

    if let Some(pixbuf) = icons.fallback_pixbuf() {
        paint_pixbuf(cr, &pixbuf, size as f64, alpha);
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

fn window_icon_surface(icon: &WindowIcon) -> Option<ImageSurface> {
    if icon.width == 0 || icon.height == 0 {
        return None;
    }
    let mut surface =
        ImageSurface::create(Format::ARgb32, icon.width as i32, icon.height as i32).ok()?;
    let stride = surface.stride() as usize;
    {
        let mut data = surface.data().ok()?;
        for y in 0..icon.height as usize {
            for x in 0..icon.width as usize {
                let &argb = icon.argb.get(y * icon.width as usize + x)?;
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
    Some(surface)
}

fn draw_window_icon(cr: &Context, icon: &WindowIcon, size: i32, alpha: f64) -> bool {
    let Some(surface) = window_icon_surface(icon) else {
        return false;
    };
    paint_surface(cr, &surface, size as f64, icon.width, icon.height, alpha);
    true
}

fn paint_pixbuf(cr: &Context, pixbuf: &Pixbuf, size: f64, alpha: f64) {
    cr.save().ok();
    cr.scale(
        size / pixbuf.width() as f64,
        size / pixbuf.height() as f64,
    );
    cr.set_source_pixbuf(pixbuf, 0.0, 0.0);
    let _ = cr.paint_with_alpha(alpha);
    cr.restore().ok();
}

fn paint_surface(
    cr: &Context,
    surface: &ImageSurface,
    size: f64,
    source_width: u32,
    source_height: u32,
    alpha: f64,
) {
    cr.save().ok();
    cr.scale(
        size / source_width.max(1) as f64,
        size / source_height.max(1) as f64,
    );
    if cr.set_source_surface(surface, 0.0, 0.0).is_ok() {
        let _ = cr.paint_with_alpha(alpha);
    }
    cr.restore().ok();
}

fn surface_bytes(surface: &ImageSurface) -> usize {
    surface.stride().max(0) as usize * surface.height().max(0) as usize
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
        for icon_name in [
            "folder-download",
            "folder-downloads",
            "folder",
            "inode-directory",
        ] {
            push_icon_key(&mut keys, Some(icon_name));
        }
    } else if item.is_trash_applet() {
        for icon_name in [
            "user-trash-full",
            "user-trash",
            "trashcan_full",
            "trashcan_empty",
        ] {
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

    fn window_icon_item() -> DockItem {
        let pixels = (0..16 * 16)
            .map(|offset| 0xff00_0000 | (offset as u32 * 1_013 & 0x00ff_ffff))
            .collect();
        DockItem {
            icon_name: None,
            startup_wm_class: None,
            window_icon: Some(WindowIcon::from_argb(16, 16, pixels)),
            ..test_item()
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

    #[test]
    fn raster_cache_reuses_raw_window_surface_across_sizes() {
        let mut cache = IconCache::new();
        let item = window_icon_item();

        assert!(cache.raster_surface(&item, 64, (1.0, 1.0)).is_some());
        assert!(cache.raster_surface(&item, 64, (1.0, 1.0)).is_some());
        assert_eq!(cache.test_surface_counts(), (1, 0, 1));

        assert!(cache.raster_surface(&item, 96, (1.0, 1.0)).is_some());
        assert_eq!(cache.test_surface_counts(), (2, 0, 1));
    }

    #[test]
    fn raster_keys_include_device_scale_and_exact_pixel_size() {
        let mut cache = IconCache::new();
        let item = window_icon_item();
        let (one_x, _) = cache.raster_surface(&item, 64, (1.0, 1.0)).unwrap();
        let (two_x, _) = cache.raster_surface(&item, 64, (2.0, 2.0)).unwrap();
        let (larger, _) = cache.raster_surface(&item, 65, (1.0, 1.0)).unwrap();

        assert_ne!(one_x, two_x);
        assert_ne!(one_x, larger);
    }

    #[test]
    fn combined_surface_cache_never_exceeds_limit() {
        let mut cache = IconCache::new();
        cache.set_test_surface_limit(20 * 1024);
        let item = window_icon_item();
        for size in [32, 48, 64, 80, 96, 128] {
            let _ = cache.raster_surface(&item, size, (1.0, 1.0));
            assert!(cache.test_surface_bytes() <= 20 * 1024);
        }
    }

    #[test]
    fn clear_invalidates_all_surface_generations() {
        let mut cache = IconCache::new();
        let item = window_icon_item();
        let _ = cache.raster_surface(&item, 64, (1.0, 1.0)).unwrap();
        cache.clear();
        assert_eq!(cache.test_surface_counts(), (0, 0, 0));
        assert_eq!(cache.test_surface_bytes(), 0);
        let _ = cache.raster_surface(&item, 64, (1.0, 1.0)).unwrap();
        assert_eq!(cache.test_surface_counts(), (1, 0, 1));
    }
}
