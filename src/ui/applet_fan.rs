use super::{
    APPLET_FAN_BOTTOM_PADDING, APPLET_FAN_ICON_SIZE, APPLET_FAN_LABEL_HEIGHT, APPLET_FAN_MAX_ITEMS,
    APPLET_FAN_REVEAL_DURATION, APPLET_FAN_ROW_HEIGHT, APPLET_FAN_TOP_PADDING, APPLET_FAN_WIDTH,
    ICON_ANIMATION_FRAME, ease_out_cubic, open_path_in_default_app, open_uri, rounded_rect_path,
};
use crate::layout::Rect;
use crate::model::DockItem;
use directories::UserDirs;
use gtk::cairo::{Context, FontSlant, FontWeight, LineCap, LinearGradient};
use gtk::gdk::prelude::GdkCairoContextExt;
use gtk::gdk_pixbuf::Pixbuf;
use gtk::glib;
use gtk::prelude::*;
use gtk::{DrawingArea, IconLookupFlags, IconTheme, TextDirection, gdk};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug, Clone)]
pub(super) struct AppletFanSource {
    pub(super) directory_label: String,
    pub(super) empty_label: String,
    pub(super) open_target: Option<AppletFanTarget>,
    pub(super) entries: Vec<AppletFanEntry>,
    pub(super) total_entries: usize,
}

#[derive(Debug, Clone)]
pub(super) struct AppletFanEntry {
    name: String,
    path: PathBuf,
    icon_name: String,
    modified: Duration,
}

#[derive(Debug, Clone)]
pub(super) struct AppletFanDirectoryEntries {
    pub(super) entries: Vec<AppletFanEntry>,
    pub(super) total_entries: usize,
}

#[derive(Debug, Clone)]
pub(super) enum AppletFanTarget {
    Path(PathBuf),
    Uri(String),
}

#[derive(Debug, Clone)]
pub(super) enum AppletFanHitAction {
    OpenPath(PathBuf),
    OpenTarget(AppletFanTarget),
}

#[derive(Debug, Clone)]
pub(super) struct AppletFanHitRegion {
    pub(super) index: usize,
    pub(super) rect: Rect,
    pub(super) action: AppletFanHitAction,
}

pub(super) struct AppletFanDrawFrame<'a> {
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) source: &'a AppletFanSource,
    pub(super) hover_index: Option<usize>,
    pub(super) reveal_progress: f64,
    pub(super) hit_regions: &'a mut Vec<AppletFanHitRegion>,
    pub(super) icon_cache: &'a mut HashMap<String, Option<Pixbuf>>,
}

pub(super) fn applet_fan_source(item: &DockItem) -> Option<AppletFanSource> {
    if item.is_downloads_applet() {
        let downloads_dir = downloads_directory()?;
        return Some(directory_applet_fan_source(
            "Downloads",
            "No recent downloads",
            Some(&downloads_dir),
            AppletFanTarget::Path(downloads_dir.clone()),
        ));
    }

    if item.is_trash_applet() {
        let trash_dir = trash_files_directory();
        return Some(directory_applet_fan_source(
            "Trash",
            "Trash is empty",
            trash_dir.as_deref(),
            AppletFanTarget::Uri("trash:///".to_string()),
        ));
    }

    if item.is_folder_applet() {
        let path = item.folder_applet_path()?;
        return Some(directory_applet_fan_source(
            &item.name,
            "Folder is empty",
            Some(&path),
            AppletFanTarget::Path(path.clone()),
        ));
    }

    None
}

fn directory_applet_fan_source(
    directory_label: &str,
    empty_label: &str,
    entries_dir: Option<&Path>,
    open_target: AppletFanTarget,
) -> AppletFanSource {
    let AppletFanDirectoryEntries {
        entries,
        total_entries,
    } = entries_dir
        .map(|dir| recent_applet_entries_from_dir(dir, APPLET_FAN_MAX_ITEMS))
        .unwrap_or_else(|| AppletFanDirectoryEntries {
            entries: Vec::new(),
            total_entries: 0,
        });
    AppletFanSource {
        directory_label: directory_label.to_string(),
        empty_label: empty_label.to_string(),
        open_target: Some(open_target),
        entries,
        total_entries,
    }
}

pub(super) fn applet_fan_size(source: &AppletFanSource) -> (i32, i32) {
    let row_count = source.entries.len() + 1 + usize::from(source.entries.is_empty());
    let height = APPLET_FAN_TOP_PADDING
        + APPLET_FAN_BOTTOM_PADDING
        + APPLET_FAN_ROW_HEIGHT * row_count as f64;
    (APPLET_FAN_WIDTH, height.ceil() as i32)
}

pub(super) fn start_applet_fan_reveal_tick(fan: &DrawingArea, started: Rc<Instant>) {
    let fan = fan.clone();
    glib::timeout_add_local(ICON_ANIMATION_FRAME, move || {
        fan.queue_draw();
        if started.elapsed() < APPLET_FAN_REVEAL_DURATION {
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    });
}

pub(super) fn applet_fan_reveal_progress(elapsed: Duration) -> f64 {
    (elapsed.as_secs_f64() / APPLET_FAN_REVEAL_DURATION.as_secs_f64()).clamp(0.0, 1.0)
}

pub(super) fn applet_fan_row_reveal(progress: f64, index: usize, row_count: usize) -> f64 {
    let bottom_first_delay = row_count.saturating_sub(1 + index) as f64 * 0.026;
    let local =
        ((progress - bottom_first_delay) / (1.0 - bottom_first_delay).max(0.20)).clamp(0.0, 1.0);
    ease_out_cubic(local)
}

fn downloads_directory() -> Option<PathBuf> {
    UserDirs::new()
        .and_then(|user_dirs| user_dirs.download_dir().map(PathBuf::from))
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join("Downloads")))
}

fn trash_files_directory() -> Option<PathBuf> {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .map(|data_home| data_home.join("Trash/files"))
}

pub(super) fn recent_applet_entries_from_dir(
    dir: &Path,
    limit: usize,
) -> AppletFanDirectoryEntries {
    let Ok(entries) = fs::read_dir(dir) else {
        return AppletFanDirectoryEntries {
            entries: Vec::new(),
            total_entries: 0,
        };
    };

    let mut files = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            let file_type = entry.file_type().ok()?;
            let path = entry.path();
            let modified = entry
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|modified| modified.duration_since(SystemTime::UNIX_EPOCH).ok())
                .unwrap_or_default();
            Some(AppletFanEntry {
                name,
                path: path.clone(),
                icon_name: applet_entry_icon_name(&path, file_type.is_dir()).to_string(),
                modified,
            })
        })
        .collect::<Vec<_>>();

    files.sort_by(|left, right| {
        right
            .modified
            .cmp(&left.modified)
            .then_with(|| left.name.cmp(&right.name))
    });
    let total_entries = files.len();
    files.truncate(limit);
    AppletFanDirectoryEntries {
        entries: files,
        total_entries,
    }
}

fn applet_entry_icon_name(path: &Path, is_directory: bool) -> &'static str {
    if is_directory {
        return "folder";
    }

    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "svg") => "image-x-generic",
        Some("pdf") => "application-pdf",
        Some("zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar") => "package-x-generic",
        Some("mp3" | "wav" | "flac" | "ogg") => "audio-x-generic",
        Some("mp4" | "mkv" | "webm" | "mov") => "video-x-generic",
        _ => "text-x-generic",
    }
}

pub(super) fn draw_applet_fan(cr: &Context, frame: AppletFanDrawFrame<'_>) {
    let AppletFanDrawFrame {
        width,
        height,
        source,
        hover_index,
        reveal_progress,
        hit_regions,
        icon_cache,
    } = frame;
    hit_regions.clear();
    cr.set_operator(gtk::cairo::Operator::Clear);
    let _ = cr.paint();
    cr.set_operator(gtk::cairo::Operator::Over);

    let row_count = source.entries.len() + 1 + usize::from(source.entries.is_empty());
    let row_step = if row_count > 1 {
        ((height as f64 - APPLET_FAN_TOP_PADDING - APPLET_FAN_BOTTOM_PADDING) / row_count as f64)
            .max(54.0)
    } else {
        APPLET_FAN_ROW_HEIGHT
    };
    let top_center_y = APPLET_FAN_TOP_PADDING + row_step * 0.5;
    let visible_count = source.entries.len();
    let more_label = applet_fan_more_label(source, visible_count);
    let more_action = source
        .open_target
        .clone()
        .map(AppletFanHitAction::OpenTarget);
    draw_applet_fan_row(
        cr,
        width as f64,
        top_center_y,
        0,
        row_count,
        &more_label,
        None,
        more_action,
        hover_index == Some(0),
        applet_fan_row_reveal(reveal_progress, 0, row_count),
        hit_regions,
        icon_cache,
    );

    if source.entries.is_empty() {
        draw_applet_fan_empty(
            cr,
            width as f64,
            top_center_y + row_step,
            1,
            row_count,
            &source.empty_label,
            applet_fan_row_reveal(reveal_progress, 1, row_count),
        );
        return;
    }

    for (entry_index, entry) in source.entries.iter().enumerate() {
        let index = entry_index + 1;
        draw_applet_fan_row(
            cr,
            width as f64,
            top_center_y + row_step * index as f64,
            index,
            row_count,
            &entry.name,
            Some(entry),
            Some(AppletFanHitAction::OpenPath(entry.path.clone())),
            hover_index == Some(index),
            applet_fan_row_reveal(reveal_progress, index, row_count),
            hit_regions,
            icon_cache,
        );
    }
}

pub(super) fn applet_fan_more_label(source: &AppletFanSource, visible_count: usize) -> String {
    let hidden_count = source.total_entries.saturating_sub(visible_count);
    if hidden_count > 0 {
        format!("{hidden_count} More in {}", source.directory_label)
    } else {
        format!("Open {}", source.directory_label)
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_applet_fan_row(
    cr: &Context,
    width: f64,
    center_y: f64,
    index: usize,
    row_count: usize,
    label: &str,
    entry: Option<&AppletFanEntry>,
    action: Option<AppletFanHitAction>,
    hovered: bool,
    reveal: f64,
    hit_regions: &mut Vec<AppletFanHitRegion>,
    icon_cache: &mut HashMap<String, Option<Pixbuf>>,
) {
    let progress = if row_count > 1 {
        index as f64 / (row_count - 1) as f64
    } else {
        0.0
    };
    let icon_center_x = width * 0.54 + (1.0 - progress).powf(0.85) * 50.0;
    let label_right = icon_center_x - 47.0 - progress * 12.0;
    let max_label_width = label_right - 10.0;
    let rotation = -0.075 + progress * 0.045;
    let center_y = center_y + (1.0 - reveal) * 18.0;
    let alpha = (reveal * reveal).clamp(0.0, 1.0);

    cr.save().ok();
    cr.select_font_face("Sans", FontSlant::Normal, FontWeight::Bold);
    cr.set_font_size(13.0);
    let fitted = fit_middle_text(cr, label, (max_label_width - 24.0).max(32.0));
    let extents = cr.text_extents(&fitted).ok();
    let text_width = extents
        .as_ref()
        .map(|extents| extents.width())
        .unwrap_or(70.0);
    let label_width = (text_width + 23.0).max(48.0).min(max_label_width);
    let label_center_x = label_right - label_width / 2.0;
    cr.restore().ok();

    let hit_x =
        (label_center_x - label_width / 2.0).min(icon_center_x - APPLET_FAN_ICON_SIZE / 2.0);
    let hit_right = icon_center_x + APPLET_FAN_ICON_SIZE / 2.0 + 10.0;
    if let Some(action) = action {
        hit_regions.push(AppletFanHitRegion {
            index,
            rect: Rect {
                x: hit_x - 4.0,
                y: center_y - APPLET_FAN_ROW_HEIGHT * 0.42,
                width: hit_right - hit_x + 8.0,
                height: APPLET_FAN_ROW_HEIGHT * 0.84,
            },
            action,
        });
    }

    cr.save().ok();
    cr.push_group();
    draw_fan_label(
        cr,
        FanLabelFrame {
            center_x: label_center_x,
            center_y,
            width: label_width,
            height: APPLET_FAN_LABEL_HEIGHT,
            text: &fitted,
            hovered,
            rotation: rotation * 0.55,
        },
    );

    if let Some(entry) = entry {
        draw_fan_entry_icon(
            cr,
            icon_center_x,
            center_y,
            APPLET_FAN_ICON_SIZE,
            rotation,
            entry,
            icon_cache,
        );
    } else {
        draw_fan_more_icon(cr, icon_center_x, center_y, APPLET_FAN_ICON_SIZE, hovered);
    }
    let _ = cr.pop_group_to_source();
    let _ = cr.paint_with_alpha(alpha);
    cr.restore().ok();
}

fn draw_applet_fan_empty(
    cr: &Context,
    width: f64,
    center_y: f64,
    index: usize,
    row_count: usize,
    label: &str,
    reveal: f64,
) {
    let progress = if row_count > 1 {
        index as f64 / (row_count - 1) as f64
    } else {
        0.0
    };
    let icon_center_x = width * 0.54 + (1.0 - progress).powf(0.85) * 50.0;
    let label_right = icon_center_x - 47.0 - progress * 12.0;
    let max_label_width = label_right - 10.0;
    let center_y = center_y + (1.0 - reveal) * 18.0;
    let alpha = (reveal * reveal).clamp(0.0, 1.0);
    cr.save().ok();
    cr.select_font_face("Sans", FontSlant::Normal, FontWeight::Bold);
    cr.set_font_size(13.0);
    let fitted = fit_middle_text(cr, label, (max_label_width - 24.0).max(32.0));
    let extents = cr.text_extents(&fitted).ok();
    let text_width = extents
        .as_ref()
        .map(|extents| extents.width())
        .unwrap_or(92.0);
    let label_width = (text_width + 23.0).max(48.0).min(max_label_width);
    cr.restore().ok();
    cr.save().ok();
    cr.push_group();
    draw_fan_label(
        cr,
        FanLabelFrame {
            center_x: label_right - label_width / 2.0,
            center_y,
            width: label_width,
            height: APPLET_FAN_LABEL_HEIGHT,
            text: &fitted,
            hovered: false,
            rotation: -0.03,
        },
    );
    let _ = cr.pop_group_to_source();
    let _ = cr.paint_with_alpha(alpha);
    cr.restore().ok();
}

struct FanLabelFrame<'a> {
    center_x: f64,
    center_y: f64,
    width: f64,
    height: f64,
    text: &'a str,
    hovered: bool,
    rotation: f64,
}

fn draw_fan_label(cr: &Context, frame: FanLabelFrame<'_>) {
    let FanLabelFrame {
        center_x,
        center_y,
        width,
        height,
        text,
        hovered,
        rotation,
    } = frame;
    cr.save().ok();
    cr.translate(center_x, center_y);
    cr.rotate(rotation);
    rounded_rect_path(cr, -width / 2.0, -height / 2.0, width, height, height / 2.0);
    if hovered {
        let gradient = LinearGradient::new(0.0, -height / 2.0, 0.0, height / 2.0);
        gradient.add_color_stop_rgba(0.0, 0.36, 0.50, 0.76, 0.96);
        gradient.add_color_stop_rgba(1.0, 0.09, 0.27, 0.60, 0.94);
        let _ = cr.set_source(&gradient);
    } else {
        let gradient = LinearGradient::new(0.0, -height / 2.0, 0.0, height / 2.0);
        gradient.add_color_stop_rgba(0.0, 0.26, 0.24, 0.30, 0.90);
        gradient.add_color_stop_rgba(1.0, 0.06, 0.06, 0.08, 0.88);
        let _ = cr.set_source(&gradient);
    }
    let _ = cr.fill_preserve();
    cr.set_source_rgba(1.0, 1.0, 1.0, if hovered { 0.20 } else { 0.07 });
    cr.set_line_width(1.0);
    let _ = cr.stroke();

    cr.select_font_face("Sans", FontSlant::Normal, FontWeight::Bold);
    cr.set_font_size(13.0);
    let extents = cr.text_extents(text).ok();
    let text_x = extents
        .as_ref()
        .map(|extents| -extents.width() / 2.0 - extents.x_bearing())
        .unwrap_or(-width * 0.35);
    let text_y = extents
        .as_ref()
        .map(|extents| -extents.height() / 2.0 - extents.y_bearing())
        .unwrap_or(4.0);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.58);
    cr.move_to(text_x, text_y + 1.0);
    let _ = cr.show_text(text);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.95);
    cr.move_to(text_x, text_y);
    let _ = cr.show_text(text);
    cr.restore().ok();
}

fn draw_fan_entry_icon(
    cr: &Context,
    center_x: f64,
    center_y: f64,
    size: f64,
    rotation: f64,
    entry: &AppletFanEntry,
    icon_cache: &mut HashMap<String, Option<Pixbuf>>,
) {
    cr.save().ok();
    cr.translate(center_x, center_y);
    cr.rotate(rotation);

    cr.save().ok();
    cr.translate(size * 0.06, size * 0.09);
    rounded_rect_path(
        cr,
        -size * 0.40,
        -size * 0.36,
        size * 0.80,
        size * 0.72,
        size * 0.12,
    );
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.25);
    let _ = cr.fill();
    cr.restore().ok();

    if let Some(pixbuf) = fan_icon_pixbuf(icon_cache, &entry.icon_name, size.ceil() as i32) {
        let scale = size / pixbuf.width().max(pixbuf.height()) as f64;
        let draw_width = pixbuf.width() as f64 * scale;
        let draw_height = pixbuf.height() as f64 * scale;
        cr.save().ok();
        cr.translate(-draw_width / 2.0, -draw_height / 2.0);
        cr.scale(scale, scale);
        cr.set_source_pixbuf(&pixbuf, 0.0, 0.0);
        let _ = cr.paint();
        cr.restore().ok();
    } else {
        draw_fan_file_placeholder(cr, size);
    }

    cr.restore().ok();
}

fn draw_fan_more_icon(cr: &Context, center_x: f64, center_y: f64, size: f64, hovered: bool) {
    cr.save().ok();
    cr.translate(center_x, center_y);
    cr.rotate(-0.08);
    let radius = size * 0.39;
    let gradient = LinearGradient::new(0.0, -radius, 0.0, radius);
    if hovered {
        gradient.add_color_stop_rgba(0.0, 0.78, 0.84, 0.95, 0.98);
        gradient.add_color_stop_rgba(1.0, 0.33, 0.42, 0.60, 0.98);
    } else {
        gradient.add_color_stop_rgba(0.0, 0.45, 0.43, 0.50, 0.98);
        gradient.add_color_stop_rgba(1.0, 0.14, 0.13, 0.18, 0.98);
    }
    cr.arc(0.0, 0.0, radius, 0.0, std::f64::consts::PI * 2.0);
    let _ = cr.set_source(&gradient);
    let _ = cr.fill_preserve();
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.58);
    cr.set_line_width(1.4);
    let _ = cr.stroke();

    cr.set_source_rgba(1.0, 1.0, 1.0, 0.95);
    cr.set_line_width(4.0);
    cr.set_line_cap(LineCap::Round);
    cr.arc(0.0, 0.0, radius * 0.48, -2.35, 0.72);
    let _ = cr.stroke();
    cr.move_to(radius * 0.45, radius * 0.32);
    cr.line_to(radius * 0.68, radius * 0.06);
    cr.line_to(radius * 0.31, -radius * 0.02);
    cr.close_path();
    let _ = cr.fill();
    cr.restore().ok();
}

fn draw_fan_file_placeholder(cr: &Context, size: f64) {
    let width = size * 0.74;
    let height = size * 0.86;
    let x = -width / 2.0;
    let y = -height / 2.0;
    rounded_rect_path(cr, x, y, width, height, size * 0.06);
    let gradient = LinearGradient::new(0.0, y, 0.0, y + height);
    gradient.add_color_stop_rgba(0.0, 0.94, 0.96, 0.98, 1.0);
    gradient.add_color_stop_rgba(1.0, 0.64, 0.72, 0.82, 1.0);
    let _ = cr.set_source(&gradient);
    let _ = cr.fill_preserve();
    cr.set_source_rgba(0.28, 0.34, 0.42, 0.48);
    cr.set_line_width(1.0);
    let _ = cr.stroke();
}

fn fan_icon_pixbuf(
    cache: &mut HashMap<String, Option<Pixbuf>>,
    icon_name: &str,
    size: i32,
) -> Option<Pixbuf> {
    if let Some(cached) = cache.get(icon_name) {
        return cached.clone();
    }

    let loaded = load_fan_icon(icon_name, size);
    cache.insert(icon_name.to_string(), loaded.clone());
    loaded
}

fn load_fan_icon(icon_name: &str, size: i32) -> Option<Pixbuf> {
    let path = Path::new(icon_name);
    if path.is_absolute() && path.exists() {
        return Pixbuf::from_file_at_scale(path, size, size, true).ok();
    }

    let display = gdk::Display::default()?;
    let icon_theme = IconTheme::for_display(&display);
    if !icon_theme.has_icon(icon_name) {
        return None;
    }

    let paintable = icon_theme.lookup_icon(
        icon_name,
        &[],
        size,
        1,
        TextDirection::None,
        IconLookupFlags::empty(),
    );
    let path = paintable.file()?.path()?;
    Pixbuf::from_file_at_scale(path, size, size, true).ok()
}

fn fit_middle_text(cr: &Context, text: &str, max_width: f64) -> String {
    if cr
        .text_extents(text)
        .map(|extents| extents.width() <= max_width)
        .unwrap_or(true)
    {
        return text.to_string();
    }

    let chars = text.chars().collect::<Vec<_>>();
    for keep in (1..chars.len()).rev() {
        let head_len = keep.div_ceil(2);
        let tail_len = keep / 2;
        let head = chars.iter().take(head_len).collect::<String>();
        let tail = chars
            .iter()
            .skip(chars.len().saturating_sub(tail_len))
            .collect::<String>();
        let candidate = format!("{head}...{tail}");
        if cr
            .text_extents(&candidate)
            .map(|extents| extents.width() <= max_width)
            .unwrap_or(true)
        {
            return candidate;
        }
    }

    "...".to_string()
}

pub(super) fn run_applet_fan_action(action: AppletFanHitAction) {
    match action {
        AppletFanHitAction::OpenPath(path) => open_path_in_default_app(&path),
        AppletFanHitAction::OpenTarget(AppletFanTarget::Path(path)) => {
            open_path_in_default_app(&path);
        }
        AppletFanHitAction::OpenTarget(AppletFanTarget::Uri(uri)) => {
            open_uri(&uri);
        }
    }
}
