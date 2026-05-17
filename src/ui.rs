use crate::backend::x11::X11Backend;
use crate::backend::{DockGeometry, PlatformBackend};
use crate::config::{AppletConfig, Config, DockConfig, RenderMode};
use crate::desktop::DesktopIndex;
use crate::layout::{DockLayout, Point, Rect};
use crate::model::{DockItem, DockModel, DockSectionKind};
use crate::renderer::{
    GhostIcon, IconCache, IconMotionFrame, IconMotionRect, IconPresenceFrame,
    IconPresenceRect, RenderFrame, Renderer, ShelfLayer,
};
use crate::scene3d::Scene3dRenderer;
use crate::shelf::ShelfRenderer;
use crate::theme::Theme;
use crate::theme_pack::ThemePack;
use directories::UserDirs;
use gdk_x11::X11Surface;
use gtk::cairo::{Context, FontSlant, FontWeight, LineCap, LinearGradient};
use gtk::gdk::prelude::GdkCairoContextExt;
use gtk::gdk_pixbuf::Pixbuf;
use gtk::gio;
use gtk::gio::prelude::FileExt;
use gtk::glib::{self, Propagation, object::Cast};
use gtk::prelude::*;
use gtk::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, DrawingArea,
    EventControllerMotion, FileDialog, FileFilter, GLArea, GestureClick, GestureDrag,
    IconLookupFlags, IconTheme, Image, Label, Orientation, Overlay, PolicyType, Popover,
    PositionType, Scale, ScrolledWindow, SearchEntry, TextDirection, gdk,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};

const APP_ID: &str = "dev.osdockx.OSDockX";
const EDGE_VISIBLE_PIXELS: i32 = 4;
const SLOW_UI_OP: Duration = Duration::from_millis(4);
const CONTEXT_MENU_WIDTH: i32 = 198;
const CONTEXT_MENU_ITEM_HEIGHT: i32 = 24;
const CONTEXT_MENU_SETTINGS_COUNT: usize = 4;
const CONTEXT_MENU_SEPARATOR_HEIGHT: i32 = 12;
const CONTEXT_MENU_CHROME_HEIGHT: i32 = 12;
const CONTEXT_MENU_GAP: f64 = 18.0;
const DOCK_CONTEXT_MENU_WIDTH: i32 = 228;
const DOCK_CONTEXT_MENU_ACTIONS: usize = 11;
const HOVER_SETTINGS_MENU_WIDTH: i32 = 272;
const ADD_APPLICATION_MENU_WIDTH: i32 = 292;
const ADD_APPLICATION_MENU_VISIBLE_ROWS: usize = 12;
const THEME_ICON_MENU_WIDTH: i32 = 292;
const THEME_ICON_MENU_VISIBLE_ROWS: usize = 12;
const THEME_ICON_MENU_MAX_MATCHES: usize = 80;
const APPLET_FAN_WIDTH: i32 = 390;
const APPLET_FAN_MAX_ITEMS: usize = 7;
const APPLET_FAN_GAP: f64 = 10.0;
const APPLET_FAN_ROW_HEIGHT: f64 = 66.0;
const APPLET_FAN_TOP_PADDING: f64 = 16.0;
const APPLET_FAN_BOTTOM_PADDING: f64 = 14.0;
const APPLET_FAN_ICON_SIZE: f64 = 48.0;
const APPLET_FAN_LABEL_HEIGHT: f64 = 25.0;
const APPLET_FAN_REVEAL_DURATION: Duration = Duration::from_millis(170);
const ICON_DRAG_THRESHOLD: f64 = 6.0;
const ICON_SLIDE_DURATION: Duration = Duration::from_millis(150);
const ICON_PRESENCE_DURATION: Duration = Duration::from_millis(360);
const ICON_ANIMATION_FRAME: Duration = Duration::from_millis(16);
const STARTUP_REVEAL_DURATION: Duration = Duration::from_millis(480);
const SEPARATOR_RESIZE_CURSOR: &str = "ns-resize";
const SEPARATOR_RESIZE_PIXELS_PER_ICON: f64 = 2.0;
const SEPARATOR_RESIZE_MIN_ICON_SIZE: u32 = 32;
const SEPARATOR_RESIZE_MAX_ICON_SIZE: u32 = 128;

pub fn run() -> anyhow::Result<()> {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(|app| {
        if let Err(error) = build_ui(app) {
            tracing::error!("{error:#}");
        }
    });
    app.run();
    Ok(())
}

struct Runtime {
    config: Config,
    config_path: PathBuf,
    composited: bool,
    theme: Theme,
    desktop_index: DesktopIndex,
    backend: Option<X11Backend>,
    model: DockModel,
    renderer: Renderer,
    scene3d: Scene3dRenderer,
    icons: IconCache,
    hover: Option<Point>,
    dock_xid: Option<u32>,
    hidden: bool,
    last_size: Option<(i32, i32)>,
    last_geometry: Option<DockGeometry>,
    last_reserved_geometry: Option<DockGeometry>,
    last_shape_size: Option<(i32, i32)>,
    last_shape_label: Option<usize>,
    context_menu: Option<Popover>,
    drag: Option<IconDrag>,
    separator_resize: Option<SeparatorResize>,
    icon_slide: Option<IconSlide>,
    icon_presence: Option<IconPresenceTransition>,
    startup_reveal: Option<StartupReveal>,
    animation_tick_running: bool,
    startup_reveal_tick_running: bool,
    suppress_next_left_click: bool,
}

#[derive(Debug, Clone)]
struct IconDrag {
    item_key: String,
    origin: Point,
    current: Point,
    grab_offset: Point,
    moved: bool,
    changed: bool,
}

#[derive(Debug, Clone)]
struct SeparatorResize {
    start_mouse_y: f64,
    start_window_y: i32,
    start_icon_size: u32,
    current_icon_size: u32,
}

#[derive(Debug, Clone)]
struct IconSlide {
    from: Vec<IconMotionRect>,
    started: Instant,
}

#[derive(Debug, Clone)]
struct IconPresenceTransition {
    from: Vec<IconMotionRect>,
    ghosts: Vec<IconPresenceGhost>,
    from_layout: DockLayout,
    to_layout: DockLayout,
    has_insertions: bool,
    has_removals: bool,
    started: Instant,
}

#[derive(Debug, Clone)]
struct IconPresenceGhost {
    item: DockItem,
    rect: Rect,
}

#[derive(Debug, Clone)]
struct StartupReveal {
    started: Instant,
}

#[derive(Debug, Clone)]
struct AppletFanSource {
    directory_label: String,
    empty_label: String,
    open_target: Option<AppletFanTarget>,
    entries: Vec<AppletFanEntry>,
    total_entries: usize,
}

#[derive(Debug, Clone)]
struct AppletFanEntry {
    name: String,
    path: PathBuf,
    icon_name: String,
    modified: Duration,
}

#[derive(Debug, Clone)]
struct AppletFanDirectoryEntries {
    entries: Vec<AppletFanEntry>,
    total_entries: usize,
}

#[derive(Debug, Clone)]
enum AppletFanTarget {
    Path(PathBuf),
    Uri(String),
}

#[derive(Debug, Clone)]
enum AppletFanHitAction {
    OpenPath(PathBuf),
    OpenTarget(AppletFanTarget),
}

#[derive(Debug, Clone)]
struct AppletFanHitRegion {
    index: usize,
    rect: Rect,
    action: AppletFanHitAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApplicationContextAction {
    Launch,
    Focus,
    Minimize,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DockContextAction {
    AddApplication,
    AddFolderApplet,
    LargerIcons,
    SmallerIcons,
    HoverEffect,
    ToggleAutohide,
    ToggleReserveSpace,
    ReloadTheme,
    ResetDefaults,
    ResetCustomIcons,
    OpenConfigFolder,
}

impl Runtime {
    fn refresh_model(&mut self) {
        let previous_model = self.model.clone();
        let previous_layout = dock_layout_for_state(self, None);
        let previous_rects = current_visible_icon_rects(self);
        let windows = self
            .backend
            .as_mut()
            .and_then(|backend| match backend.poll_windows() {
                Ok(windows) => Some(windows),
                Err(error) => {
                    tracing::warn!("could not refresh X11 windows: {error:#}");
                    None
                }
            })
            .unwrap_or_default();
        let mut next_model = DockModel::from_sources_with_applets(
            &self.config.pinned,
            &self.desktop_index,
            windows,
            &self.config.applets,
        );
        next_model.apply_order(&self.config.item_order);

        let icon_presence = build_icon_presence_transition(
            &previous_model,
            &next_model,
            previous_layout,
            &previous_rects,
            &self.config.dock,
            &self.theme,
        );

        self.model = next_model;
        if icon_presence.is_some() {
            self.hover = None;
        }
        self.icon_presence = icon_presence;
    }

    fn desired_size(&self) -> (i32, i32) {
        icon_presence_layout(self)
            .map(|layout| layout.size)
            .unwrap_or_else(|| {
                Renderer::desired_size(&self.model, &self.config.dock, &self.theme, self.hover)
            })
    }

    fn reserved_thickness(&self) -> u32 {
        Renderer::reserved_thickness(&self.model, &self.config.dock, &self.theme)
    }

    fn desired_geometry(&self) -> Option<DockGeometry> {
        let backend = self.backend.as_ref()?;
        let mut geometry = backend
            .monitor_geometry(self.config.dock.monitor.as_deref())
            .dock_geometry(
                self.desired_size(),
                self.config.dock.edge,
                self.config.dock.reserve_space && !self.config.dock.autohide,
                self.reserved_thickness(),
            );

        if self.hidden {
            let hidden_offset = hidden_edge_offset(&geometry, self.config.dock.edge);
            apply_edge_offset(&mut geometry, self.config.dock.edge, hidden_offset);
        }

        if let Some(startup_reveal) = self.startup_reveal.as_ref() {
            let startup_offset =
                startup_reveal_offset(&geometry, self.config.dock.edge, startup_reveal.progress());
            apply_edge_offset(&mut geometry, self.config.dock.edge, startup_offset);
        }
        Some(geometry)
    }
}

impl StartupReveal {
    fn new() -> Self {
        Self {
            started: Instant::now(),
        }
    }

    fn progress(&self) -> f64 {
        (self.started.elapsed().as_secs_f64() / STARTUP_REVEAL_DURATION.as_secs_f64())
            .clamp(0.0, 1.0)
    }

    fn finished(&self) -> bool {
        self.progress() >= 1.0
    }
}

fn dock_layout_for_state(state: &Runtime, hover: Option<Point>) -> DockLayout {
    icon_presence_layout(state).unwrap_or_else(|| {
        Renderer::layout_for(&state.model, &state.config.dock, &state.theme, hover)
    })
}

fn build_icon_presence_transition(
    previous_model: &DockModel,
    next_model: &DockModel,
    previous_layout: DockLayout,
    previous_rects: &[IconMotionRect],
    config: &DockConfig,
    theme: &Theme,
) -> Option<IconPresenceTransition> {
    if previous_model.items.is_empty() {
        return None;
    }

    let previous_keys = previous_model
        .items
        .iter()
        .map(|item| item.config_key().to_ascii_lowercase())
        .collect::<Vec<_>>();
    let next_keys = next_model
        .items
        .iter()
        .map(|item| item.config_key().to_ascii_lowercase())
        .collect::<Vec<_>>();
    let changed = previous_keys.len() != next_keys.len()
        || previous_keys
            .iter()
            .any(|key| !next_keys.iter().any(|next| next == key))
        || next_keys
            .iter()
            .any(|key| !previous_keys.iter().any(|previous| previous == key));
    if !changed {
        return None;
    }

    let next_layout = Renderer::layout_for(next_model, config, theme, None);
    let has_insertions = next_keys
        .iter()
        .any(|key| !previous_keys.iter().any(|previous| previous == key));
    let has_removals = previous_keys
        .iter()
        .any(|key| !next_keys.iter().any(|next| next == key));

    let from = previous_rects
        .iter()
        .filter(|rect| contains_item_key(&next_keys, &rect.item_key))
        .cloned()
        .collect::<Vec<_>>();
    let ghosts = previous_model
        .items
        .iter()
        .filter_map(|item| {
            let item_key = item.config_key();
            (!contains_item_key(&next_keys, &item_key)).then_some(item).and_then(|item| {
                previous_rects
                    .iter()
                    .find(|rect| rect.item_key.eq_ignore_ascii_case(&item_key))
                    .map(|rect| IconPresenceGhost {
                        item: item.clone(),
                        rect: rect.rect,
                    })
            })
        })
        .collect::<Vec<_>>();
    if from.is_empty() && ghosts.is_empty() {
        return None;
    }

    Some(IconPresenceTransition {
        from,
        ghosts,
        from_layout: previous_layout,
        to_layout: next_layout,
        has_insertions,
        has_removals,
        started: Instant::now(),
    })
}

fn contains_item_key(keys: &[String], item_key: &str) -> bool {
    keys.iter().any(|key| key.eq_ignore_ascii_case(item_key))
}

fn translate_rect(rect: Rect, dx: f64, dy: f64) -> Rect {
    Rect {
        x: rect.x + dx,
        y: rect.y + dy,
        width: rect.width,
        height: rect.height,
    }
}

fn current_visible_icon_rects(state: &Runtime) -> Vec<IconMotionRect> {
    icon_presence_frame(state)
        .map(|frame| {
            frame
                .current
                .into_iter()
                .map(|icon| IconMotionRect {
                    item_key: icon.item_key,
                    rect: icon.rect,
                })
                .collect()
        })
        .unwrap_or_else(|| current_icon_motion_rects(state))
}

fn icon_presence_layout(state: &Runtime) -> Option<DockLayout> {
    let transition = state
        .icon_presence
        .as_ref()
        .filter(|transition| transition.started.elapsed() < ICON_PRESENCE_DURATION)?;
    Some(interpolate_presence_layout(
        transition,
        icon_presence_chrome_progress(transition),
    ))
}

fn interpolate_presence_layout(
    transition: &IconPresenceTransition,
    progress: f64,
) -> DockLayout {
    let mut layout = transition.to_layout.clone();
    layout.label = None;
    layout.size = (
        interpolate(
            transition.from_layout.size.0 as f64,
            transition.to_layout.size.0 as f64,
            progress,
        )
        .round() as i32,
        interpolate(
            transition.from_layout.size.1 as f64,
            transition.to_layout.size.1 as f64,
            progress,
        )
        .round() as i32,
    );
    layout.shelf = interpolate_rect(transition.from_layout.shelf, transition.to_layout.shelf, progress);

    for (section, from_section) in layout.sections.iter_mut().zip(&transition.from_layout.sections) {
        section.rect = interpolate_rect(from_section.rect, section.rect, progress);
    }

    if let (Some(separator), Some(from_separator)) = (
        layout.separator.as_mut(),
        transition.from_layout.separator.as_ref(),
    ) {
        separator.rect = interpolate_rect(from_separator.rect, separator.rect, progress);
    }

    layout
}

fn icon_presence_raw_progress(transition: &IconPresenceTransition) -> f64 {
    (transition.started.elapsed().as_secs_f64() / ICON_PRESENCE_DURATION.as_secs_f64())
        .clamp(0.0, 1.0)
}

fn icon_presence_chrome_progress(transition: &IconPresenceTransition) -> f64 {
    let raw = icon_presence_raw_progress(transition);
    if transition.has_insertions && !transition.has_removals {
        ease_out_cubic(remap_progress(raw, 0.0, 0.78))
    } else if transition.has_removals && !transition.has_insertions {
        ease_out_cubic(remap_progress(raw, 0.20, 1.0))
    } else {
        ease_out_cubic(raw)
    }
}

fn icon_presence_enter_progress(transition: &IconPresenceTransition) -> f64 {
    let raw = icon_presence_raw_progress(transition);
    if transition.has_insertions && !transition.has_removals {
        ease_out_cubic(remap_progress(raw, 0.24, 1.0))
    } else {
        ease_out_cubic(raw)
    }
}

fn icon_presence_exit_progress(transition: &IconPresenceTransition) -> f64 {
    let raw = icon_presence_raw_progress(transition);
    if transition.has_removals && !transition.has_insertions {
        ease_out_cubic(remap_progress(raw, 0.0, 0.72))
    } else {
        ease_out_cubic(raw)
    }
}

fn remap_progress(progress: f64, start: f64, end: f64) -> f64 {
    if end <= start {
        return 1.0;
    }
    ((progress - start) / (end - start)).clamp(0.0, 1.0)
}

fn apply_edge_offset(geometry: &mut DockGeometry, edge: crate::config::DockEdge, distance: i32) {
    match edge {
        crate::config::DockEdge::Bottom => {
            geometry.y += distance;
        }
        crate::config::DockEdge::Top => {
            geometry.y -= distance;
        }
        crate::config::DockEdge::Left => {
            geometry.x -= distance;
        }
        crate::config::DockEdge::Right => {
            geometry.x += distance;
        }
    }
}

fn hidden_edge_offset(geometry: &DockGeometry, edge: crate::config::DockEdge) -> i32 {
    match edge {
        crate::config::DockEdge::Bottom | crate::config::DockEdge::Top => {
            geometry.height as i32 - EDGE_VISIBLE_PIXELS
        }
        crate::config::DockEdge::Left | crate::config::DockEdge::Right => {
            geometry.width as i32 - EDGE_VISIBLE_PIXELS
        }
    }
}

fn startup_reveal_offset(
    geometry: &DockGeometry,
    edge: crate::config::DockEdge,
    progress: f64,
) -> i32 {
    let eased = ease_out_cubic(progress.clamp(0.0, 1.0));
    let travel = hidden_edge_offset(geometry, edge);
    ((1.0 - eased) * travel as f64).round() as i32
}

fn separator_hit_test_in_layout(layout: &crate::layout::DockLayout, point: Point) -> bool {
    layout
        .section(DockSectionKind::Separator)
        .is_some_and(|section| section.rect.contains(point))
}

fn separator_hit_test(state: &Runtime, point: Point) -> bool {
    let layout = dock_layout_for_state(state, None);
    separator_hit_test_in_layout(&layout, point)
}

fn begin_separator_resize(
    layout: &crate::layout::DockLayout,
    point: Point,
    start_window_y: i32,
    start_icon_size: u32,
) -> Option<SeparatorResize> {
    separator_hit_test_in_layout(layout, point).then_some(SeparatorResize {
        start_mouse_y: point.y,
        start_window_y,
        start_icon_size,
        current_icon_size: start_icon_size,
    })
}

fn separator_resize_drag_delta(
    resize: &SeparatorResize,
    current_window_y: i32,
    offset_y: f64,
) -> f64 {
    (current_window_y - resize.start_window_y) as f64 + offset_y
}

fn resize_icon_size_for_drag(start_icon_size: u32, offset_y: f64) -> u32 {
    let size_delta = (-offset_y / SEPARATOR_RESIZE_PIXELS_PER_ICON).round() as i32;
    (start_icon_size as i32 + size_delta).clamp(
        SEPARATOR_RESIZE_MIN_ICON_SIZE as i32,
        SEPARATOR_RESIZE_MAX_ICON_SIZE as i32,
    ) as u32
}

fn set_separator_resize_cursor(drawing: &DrawingArea, enabled: bool) {
    drawing.set_cursor_from_name(enabled.then_some(SEPARATOR_RESIZE_CURSOR));
}

fn build_ui(app: &Application) -> anyhow::Result<()> {
    if let Err(error) = ThemePack::export_builtin_theme_packs() {
        tracing::warn!("could not export built-in theme packs: {error:#}");
    }
    let (config, config_path) = Config::load_or_create()?;
    tracing::info!("using config {}", config_path.display());
    install_css();

    let composited = gdk::Display::default().is_some_and(|display| display.is_composited());
    let (theme_id, theme_renderer, theme) = resolve_runtime_theme(composited, &config.theme);
    tracing::info!("using theme {} ({:?})", theme_id, theme_renderer);
    if !composited {
        tracing::warn!("display is not composited; using opaque shelf fallback");
    }
    let desktop_index = DesktopIndex::load();
    let backend = match X11Backend::new() {
        Ok(backend) => Some(backend),
        Err(error) => {
            tracing::warn!("X11 backend unavailable; running as a plain GTK window: {error:#}");
            None
        }
    };

    let mut runtime = Runtime {
        config,
        config_path,
        composited,
        theme,
        desktop_index,
        backend,
        model: DockModel::default(),
        renderer: Renderer::new(),
        scene3d: Scene3dRenderer::new(),
        icons: IconCache::new(),
        hover: None,
        dock_xid: None,
        hidden: false,
        last_size: None,
        last_geometry: None,
        last_reserved_geometry: None,
        last_shape_size: None,
        last_shape_label: None,
        context_menu: None,
        drag: None,
        separator_resize: None,
        icon_slide: None,
        icon_presence: None,
        startup_reveal: None,
        animation_tick_running: false,
        startup_reveal_tick_running: false,
        suppress_next_left_click: false,
    };
    runtime.refresh_model();

    let state = Rc::new(RefCell::new(runtime));
    let window = ApplicationWindow::builder()
        .application(app)
        .title("OSDockX")
        .decorated(false)
        .resizable(false)
        .focusable(false)
        .build();
    window.set_can_focus(false);
    window.set_opacity(0.0);
    window.add_css_class("osdock-window");

    let overlay = Overlay::new();
    overlay.add_css_class("osdock-surface");
    overlay.set_hexpand(false);
    overlay.set_vexpand(false);

    let gl_area = GLArea::new();
    gl_area.add_css_class("osdock-gl");
    gl_area.set_hexpand(false);
    gl_area.set_vexpand(false);
    gl_area.set_has_depth_buffer(true);
    gl_area.set_auto_render(false);
    gl_area.set_visible(state.borrow().theme.renderer == RenderMode::Scene3d);

    let drawing = DrawingArea::new();
    drawing.add_css_class("osdock-surface");
    drawing.set_hexpand(false);
    drawing.set_vexpand(false);
    sync_dock_window(&state, &window, &drawing, &gl_area, true);
    overlay.set_child(Some(&gl_area));
    overlay.add_overlay(&drawing);
    window.set_child(Some(&overlay));

    {
        let state = Rc::clone(&state);
        drawing.set_draw_func(move |_, cr, _, _| {
            let mut state = state.borrow_mut();
            let hover = state.hover;
            let model = state.model.clone();
            let config = state.config.dock.clone();
            let custom_icons = state.config.custom_icons.clone();
            let theme = state.theme.clone();
            let icon_motion = icon_motion_frame(&state);
            let icon_presence = icon_presence_frame(&state);
            let mut icons = std::mem::take(&mut state.icons);
            icons.set_custom_icons(&custom_icons);
            let shelf_layer = shelf_layer_for(&state);
            state.renderer.draw_overlay(
                cr,
                RenderFrame {
                    model: &model,
                    config: &config,
                    theme: &theme,
                    hover,
                    shelf_layer,
                    icon_motion: icon_motion.as_ref(),
                    icon_presence: icon_presence.as_ref(),
                    container_size: None,
                },
                &mut icons,
            );
            state.icons = icons;
        });
    }
    wire_gl_area(&state, &gl_area, &drawing);

    wire_motion(&state, &window, &drawing, &gl_area);
    wire_clicks(&state, &window, &drawing, &gl_area);
    wire_icon_drag(&state, &window, &drawing, &gl_area);
    wire_separator_resize_drag(&state, &window, &drawing, &gl_area);
    wire_realize(&state, &window, &drawing, &gl_area);
    wire_refresh(&state, &window, &drawing, &gl_area);
    wire_icon_theme_changes(&state, &drawing, &gl_area);

    window.present();
    queue_gl_render_if_enabled(&state, &gl_area);
    Ok(())
}

fn resolve_runtime_theme(
    composited: bool,
    config: &crate::config::ThemeConfig,
) -> (String, RenderMode, Theme) {
    let pack = ThemePack::load(config);
    let theme = pack.theme;
    let id = pack.id;
    let renderer = pack.renderer;
    if composited {
        (id, renderer, theme)
    } else {
        (id, renderer, theme.opaque_fallback())
    }
}

fn install_css() {
    let Some(display) = gdk::Display::default() else {
        return;
    };
    let provider = gtk::CssProvider::new();
    provider.load_from_string(
        "
        window {
            background-color: transparent;
            box-shadow: none;
        }
        window.osdock-window,
        window.osdock-window.background {
            background-color: transparent;
            box-shadow: none;
        }
        drawingarea {
            background-color: transparent;
        }
        glarea,
        .osdock-surface,
        .osdock-gl {
            background-color: transparent;
        }
        popover.osdock-context-popover contents {
            background: transparent;
            border: none;
            box-shadow: none;
            padding: 0;
        }
        popover.osdock-stack-popover contents {
            background: transparent;
            border: none;
            box-shadow: none;
            padding: 0;
        }
        .osdock-context-menu {
            background: alpha(#1b1c1f, 0.90);
            border: 1px solid alpha(#ffffff, 0.18);
            border-radius: 7px;
            box-shadow: 0 8px 20px alpha(#000000, 0.48);
            padding: 5px;
        }
        .osdock-menu-box {
            padding: 1px 0;
        }
        .osdock-menu-title {
            color: alpha(#eef2f7, 0.86);
            font-size: 11px;
            font-weight: 700;
            margin: 5px 10px 4px 10px;
        }
        .osdock-menu-search,
        searchentry.osdock-menu-search,
        entry.osdock-menu-search {
            min-height: 28px;
            margin: 3px 8px 7px 8px;
            padding: 0 12px;
            border-radius: 999px;
            border: 1px solid alpha(#000000, 0.22);
            background: #f8f9fb;
            color: #1d232b;
            box-shadow:
                inset 0 1px 2px alpha(#000000, 0.13),
                0 1px alpha(#ffffff, 0.42);
        }
        .osdock-menu-search:focus,
        searchentry.osdock-menu-search:focus,
        entry.osdock-menu-search:focus {
            border-color: alpha(#3b82f6, 0.58);
            box-shadow:
                inset 0 1px 2px alpha(#000000, 0.11),
                0 0 0 2px alpha(#5aa7ff, 0.22);
        }
        .osdock-menu-search text,
        searchentry.osdock-menu-search text,
        entry.osdock-menu-search text {
            color: #1d232b;
            background: transparent;
        }
        .osdock-menu-search placeholder,
        searchentry.osdock-menu-search placeholder,
        entry.osdock-menu-search placeholder {
            color: alpha(#6f7782, 0.78);
        }
        .osdock-menu-search image,
        searchentry.osdock-menu-search image,
        entry.osdock-menu-search image {
            color: #7a818c;
        }
        button.osdock-menu-item {
            min-height: 24px;
            min-width: 182px;
            padding: 0;
            margin: 0;
            border: none;
            border-radius: 4px;
            background: transparent;
            box-shadow: none;
            color: #f2f2f2;
        }
        .osdock-menu-separator {
            min-height: 12px;
            margin: 3px 8px;
            background-image: linear-gradient(
                to right,
                alpha(#ffffff, 0.0),
                alpha(#f2f5fb, 0.78),
                alpha(#b6c0cf, 0.96),
                alpha(#f2f5fb, 0.78),
                alpha(#ffffff, 0.0)
            );
            background-repeat: no-repeat;
            background-position: center;
            background-size: 100% 1px;
        }
        button.osdock-menu-item:hover,
        button.osdock-menu-item:focus {
            background-image: linear-gradient(to bottom, #5aa7ff, #1f68d7);
            color: #ffffff;
        }
        button.osdock-menu-item label {
            color: #eeeeee;
            font-size: 12px;
            font-weight: 500;
            text-shadow: 0 1px alpha(#000000, 0.58);
        }
        button.osdock-menu-item:hover label,
        button.osdock-menu-item:focus label {
            color: #ffffff;
        }
        .osdock-menu-row {
            padding: 2px 10px 2px 6px;
        }
        .osdock-menu-icon {
            margin-right: 7px;
        }
        .osdock-menu-check {
            min-width: 14px;
            margin-right: 4px;
        }
        ",
    );
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn wire_realize(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
) {
    let state = Rc::clone(state);
    let drawing = drawing.clone();
    let gl_area = gl_area.clone();
    window.connect_realize(move |window| {
        let reveal = {
            let window = window.clone();
            move || {
                let window = window.clone();
                glib::idle_add_local_once(move || {
                    window.set_opacity(1.0);
                });
            }
        };

        let Some(surface) = window.surface() else {
            reveal();
            return;
        };
        let Ok(surface) = surface.downcast::<X11Surface>() else {
            tracing::warn!("GTK surface is not an X11 surface");
            reveal();
            return;
        };
        let xid = surface.xid() as u32;
        let mut runtime = state.borrow_mut();
        runtime.dock_xid = Some(xid);
        if runtime.backend.is_some() {
            runtime.startup_reveal = Some(StartupReveal::new());
        }
        let Some(geometry) = runtime.desired_geometry() else {
            runtime.startup_reveal = None;
            reveal();
            return;
        };
        if let Some(backend) = runtime.backend.as_mut()
            && let Err(error) = backend.set_dock_window(xid, geometry)
        {
            tracing::warn!("could not configure X11 dock window: {error:#}");
        }
        runtime.last_geometry = Some(geometry);
        runtime.last_reserved_geometry = Some(geometry);
        shape_dock(&mut runtime);
        drop(runtime);
        reveal();
        ensure_startup_reveal_tick(&state, window, &drawing, &gl_area);
    });
}

fn wire_gl_area(state: &Rc<RefCell<Runtime>>, gl_area: &GLArea, drawing: &DrawingArea) {
    let state = Rc::clone(state);
    let drawing = drawing.clone();
    gl_area.connect_render(move |area, _| {
        let mut state = state.borrow_mut();
        let hover = state.hover;
        let model = state.model.clone();
        let theme = state.theme.clone();
        let layout = dock_layout_for_state(&state, hover);
        let rendered = theme.renderer == RenderMode::Scene3d
            && state
                .scene3d
                .render_gl_area(area, &layout, &model, &theme, hover);
        if !rendered {
            if let Some(reason) = state.scene3d.fallback_reason() {
                tracing::debug!("using cairo shelf fallback: {reason}");
            }
            drawing.queue_draw();
        }
        Propagation::Stop
    });
}

fn wire_motion(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
) {
    let motion = EventControllerMotion::new();
    {
        let state = Rc::clone(state);
        let window = window.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        motion.connect_motion(move |_, x, y| {
            let started = Instant::now();
            let point = Point { x, y };
            let separator_hover;
            let resizing;
            {
                let mut state = state.borrow_mut();
                resizing = state.separator_resize.is_some();
                if resizing {
                    separator_hover = false;
                    state.hover = None;
                } else {
                    separator_hover = separator_hit_test(&state, point);
                    let next_hover = if state.context_menu.is_some()
                        || state.drag.is_some()
                        || state.icon_presence.is_some()
                        || separator_hover
                    {
                        None
                    } else {
                        Renderer::hover_point_for(
                            &state.model,
                            &state.config.dock,
                            &state.theme,
                            point,
                            state.hover.is_some(),
                        )
                    };
                    if state.hidden {
                        state.hidden = false;
                        state.last_shape_size = None;
                        move_dock(&mut state, true);
                    }
                    state.hover = next_hover;
                }
            }
            set_separator_resize_cursor(&drawing, separator_hover || resizing);
            if resizing {
                log_slow("motion", started.elapsed());
                return;
            }
            sync_dock_window(&state, &window, &drawing, &gl_area, false);
            queue_gl_render_if_enabled(&state, &gl_area);
            drawing.queue_draw();
            log_slow("motion", started.elapsed());
        });
    }
    {
        let state = Rc::clone(state);
        let window = window.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        motion.connect_leave(move |_| {
            let autohide;
            let delay;
            let resizing;
            {
                let mut state = state.borrow_mut();
                resizing = state.separator_resize.is_some();
                state.hover = None;
                autohide = state.config.dock.autohide && state.context_menu.is_none() && !resizing;
                delay = state.config.dock.hide_delay_ms;
            }
            if resizing {
                set_separator_resize_cursor(&drawing, true);
                return;
            }
            set_separator_resize_cursor(&drawing, false);
            queue_gl_render_if_enabled(&state, &gl_area);
            drawing.queue_draw();

            if autohide {
                let state = Rc::clone(&state);
                let window = window.clone();
                let drawing = drawing.clone();
                let gl_area = gl_area.clone();
                glib::timeout_add_local_once(Duration::from_millis(delay as u64), move || {
                    let changed = {
                        let mut state = state.borrow_mut();
                        if state.hover.is_none()
                            && state.context_menu.is_none()
                            && state.separator_resize.is_none()
                            && !state.hidden
                        {
                            state.hidden = true;
                            state.last_shape_size = None;
                            true
                        } else {
                            false
                        }
                    };
                    if changed {
                        sync_dock_window(&state, &window, &drawing, &gl_area, true);
                        queue_gl_render_if_enabled(&state, &gl_area);
                        drawing.queue_draw();
                    }
                });
            }
            sync_dock_window(&state, &window, &drawing, &gl_area, false);
        });
    }
    drawing.add_controller(motion);
}

fn wire_clicks(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
) {
    let click = GestureClick::new();
    click.set_button(0);
    {
        let state = Rc::clone(state);
        let window = window.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        click.connect_released(move |gesture, _, x, y| {
            let button = gesture.current_button();
            if button == 1 && state.borrow().suppress_next_left_click {
                state.borrow_mut().suppress_next_left_click = false;
                drawing.queue_draw();
                return;
            }
            if button == 1 && state.borrow().context_menu.is_some() {
                let dismissed = dismiss_context_menu(&state);
                if dismissed {
                    sync_dock_window(&state, &window, &drawing, &gl_area, true);
                    drawing.queue_draw();
                }
                return;
            }
            let hit = {
                let state = state.borrow();
                dock_layout_for_state(&state, None).hit_test(Point { x, y })
            };
            if let Some(index) = hit {
                let item = {
                    let state = state.borrow();
                    state.model.items.get(index).cloned()
                };
                let Some(item) = item else {
                    drawing.queue_draw();
                    return;
                };
                if button == 3 {
                    show_context_menu(&state, &window, &drawing, &gl_area, index, x, y);
                } else if button == 1 || button == 2 {
                    let dismissed = dismiss_context_menu(&state);
                    if dismissed {
                        sync_dock_window(&state, &window, &drawing, &gl_area, true);
                    }
                    if item.is_application() {
                        activate_item(&state, index, button);
                    } else {
                        activate_applet(
                            &state, &window, &drawing, &gl_area, &item, index, button, x, y,
                        );
                    }
                }
            } else if button == 3 && dock_surface_hit_test(&state, Point { x, y }) {
                show_dock_context_menu(&state, &window, &drawing, &gl_area, x, y);
            } else if button != 0 {
                let dismissed = dismiss_context_menu(&state);
                if dismissed {
                    sync_dock_window(&state, &window, &drawing, &gl_area, true);
                }
            }
            drawing.queue_draw();
        });
    }
    drawing.add_controller(click);
}

fn dock_surface_hit_test(state: &Rc<RefCell<Runtime>>, point: Point) -> bool {
    let state = state.borrow();
    let layout = dock_layout_for_state(&state, None);
    layout.shelf.contains(point)
        || layout
            .sections
            .iter()
            .any(|section| section.rect.contains(point))
}

fn wire_icon_drag(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
) {
    let drag = GestureDrag::new();
    drag.set_button(1);
    {
        let state = Rc::clone(state);
        let window = window.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        drag.connect_drag_begin(move |_, x, y| {
            let point = Point { x, y };
            let drag_item = {
                let state = state.borrow();
                dock_layout_for_state(&state, None).hit_test(point).and_then(|index| {
                        let item = state.model.items.get(index)?;
                        if !item.is_application() && !item.is_applet() {
                            return None;
                        }
                        let rect = dock_layout_for_state(&state, None)
                        .icons
                        .iter()
                        .find(|icon| icon.item_index == index)
                        .map(|icon| icon.rect)?;
                        let item_key = item.config_key();
                        Some((item_key, rect))
                    })
            };
            let Some((item_key, rect)) = drag_item else {
                return;
            };

            let menu = {
                let mut state = state.borrow_mut();
                let menu = take_context_menu(&mut state);
                state.hover = None;
                state.drag = Some(IconDrag {
                    item_key,
                    origin: point,
                    current: point,
                    grab_offset: Point {
                        x: point.x - rect.x,
                        y: point.y - rect.y,
                    },
                    moved: false,
                    changed: false,
                });
                state.icon_slide = None;
                state.icon_presence = None;
                state.suppress_next_left_click = false;
                menu
            };
            if dismiss_popover_menu(menu) {
                sync_dock_window(&state, &window, &drawing, &gl_area, true);
            }
            ensure_icon_animation_tick(&state, &window, &drawing, &gl_area);
            queue_gl_render_if_enabled(&state, &gl_area);
            drawing.queue_draw();
        });
    }
    {
        let state = Rc::clone(state);
        let window = window.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        drag.connect_drag_update(move |_, offset_x, offset_y| {
            let changed = {
                let mut state = state.borrow_mut();
                update_icon_drag(&mut state, offset_x, offset_y)
            };
            if changed {
                ensure_icon_animation_tick(&state, &window, &drawing, &gl_area);
                sync_dock_window(&state, &window, &drawing, &gl_area, true);
                queue_gl_render_if_enabled(&state, &gl_area);
                drawing.queue_draw();
            }
        });
    }
    {
        let state = Rc::clone(state);
        let window = window.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        drag.connect_drag_end(move |_, offset_x, offset_y| {
            let had_drag = {
                let mut state = state.borrow_mut();
                finish_icon_drag(&mut state, offset_x, offset_y)
            };
            if had_drag {
                ensure_icon_animation_tick(&state, &window, &drawing, &gl_area);
                sync_dock_window(&state, &window, &drawing, &gl_area, true);
                queue_gl_render_if_enabled(&state, &gl_area);
                drawing.queue_draw();
            }
        });
    }
    drawing.add_controller(drag);
}

fn wire_separator_resize_drag(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
) {
    let drag = GestureDrag::new();
    drag.set_button(1);
    {
        let state = Rc::clone(state);
        let drawing = drawing.clone();
        drag.connect_drag_begin(move |_, x, y| {
            let point = Point { x, y };
            let resize = {
                let state = state.borrow();
                if state.separator_resize.is_some() {
                    return;
                }
                let layout = dock_layout_for_state(&state, None);
                let start_window_y = state
                    .last_geometry
                    .or_else(|| state.desired_geometry())
                    .map(|geometry| geometry.y)
                    .unwrap_or_default();
                begin_separator_resize(&layout, point, start_window_y, state.config.dock.icon_size)
            };
            let Some(resize) = resize else {
                return;
            };

            let menu = {
                let mut state = state.borrow_mut();
                let menu = take_context_menu(&mut state);
                state.hover = None;
                state.separator_resize = Some(resize);
                menu
            };
            dismiss_popover_menu(menu);
            set_separator_resize_cursor(&drawing, true);
            drawing.queue_draw();
        });
    }
    {
        let state = Rc::clone(state);
        let window = window.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        drag.connect_drag_update(move |_, _, offset_y| {
            let changed = {
                let mut state = state.borrow_mut();
                update_separator_resize(&mut state, offset_y)
            };
            if changed {
                sync_dock_window(&state, &window, &drawing, &gl_area, true);
                queue_gl_render_if_enabled(&state, &gl_area);
                drawing.queue_draw();
            }
        });
    }
    {
        let state = Rc::clone(state);
        let window = window.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        drag.connect_drag_end(move |_, _, _| {
            let changed = {
                let mut state = state.borrow_mut();
                finish_separator_resize(&mut state)
            };
            if changed {
                let state = state.borrow();
                save_runtime_config(&state);
            }
            set_separator_resize_cursor(&drawing, false);
            sync_dock_window(&state, &window, &drawing, &gl_area, true);
            queue_gl_render_if_enabled(&state, &gl_area);
            drawing.queue_draw();
        });
    }
    drawing.add_controller(drag);
}

fn update_separator_resize(state: &mut Runtime, offset_y: f64) -> bool {
    let current_window_y = state
        .last_geometry
        .or_else(|| state.desired_geometry())
        .map(|geometry| geometry.y)
        .unwrap_or_else(|| {
            state
                .separator_resize
                .as_ref()
                .map(|resize| resize.start_window_y)
                .unwrap_or_default()
        });
    let Some(resize) = state.separator_resize.as_mut() else {
        return false;
    };

    let new_size = resize_icon_size_for_drag(
        resize.start_icon_size,
        separator_resize_drag_delta(resize, current_window_y, offset_y),
    );
    resize.current_icon_size = new_size;
    if state.config.dock.icon_size == new_size {
        return false;
    }

    state.config.dock.icon_size = new_size;
    state.hover = None;
    state.last_size = None;
    state.last_geometry = None;
    state.last_shape_size = None;
    true
}

fn finish_separator_resize(state: &mut Runtime) -> bool {
    let Some(resize) = state.separator_resize.take() else {
        return false;
    };

    state.hover = None;
    state.suppress_next_left_click = true;
    resize.current_icon_size != resize.start_icon_size
}

fn update_icon_drag(state: &mut Runtime, offset_x: f64, offset_y: f64) -> bool {
    let point = {
        let Some(drag) = state.drag.as_mut() else {
            return false;
        };
        let point = Point {
            x: drag.origin.x + offset_x,
            y: drag.origin.y + offset_y,
        };
        drag.current = point;
        point
    };
    if drag_distance(offset_x, offset_y) < ICON_DRAG_THRESHOLD {
        return false;
    }

    let Some(drag) = state.drag.as_ref() else {
        return false;
    };
    let item_key = drag.item_key.clone();
    let from_rects = current_icon_motion_rects(state);
    let Some(target_index) = drag_target_index(state, point) else {
        return false;
    };

    let changed = state
        .model
        .move_item_by_key_to_index(&item_key, target_index);
    if changed {
        state.icon_slide = Some(IconSlide {
            from: from_rects,
            started: Instant::now(),
        });
        state.config.item_order = state.model.config_order();
        state.hover = None;
    }
    if let Some(drag) = state.drag.as_mut() {
        drag.moved = true;
        drag.changed |= changed;
    }
    true
}

fn finish_icon_drag(state: &mut Runtime, offset_x: f64, offset_y: f64) -> bool {
    let Some(drag) = state.drag.as_ref() else {
        return false;
    };
    let dragged =
        drag.moved || drag.changed || drag_distance(offset_x, offset_y) >= ICON_DRAG_THRESHOLD;
    let changed = drag.changed;
    if dragged {
        let from = current_icon_motion_rects(state);
        state.icon_slide = Some(IconSlide {
            from,
            started: Instant::now(),
        });
        state.suppress_next_left_click = true;
    }
    state.drag = None;
    if changed {
        save_runtime_config(state);
    }
    state.hover = None;
    true
}

fn drag_target_index(state: &Runtime, point: Point) -> Option<usize> {
    let dragged_key = &state.drag.as_ref()?.item_key;
    let dragged_is_applet = state
        .model
        .items
        .iter()
        .find(|item| item.config_key().eq_ignore_ascii_case(dragged_key))?
        .is_applet();
    let layout = dock_layout_for_state(state, None);
    let mut section_index = 0;
    layout
        .icons
        .iter()
        .filter_map(|icon| {
            let item = state.model.items.get(icon.item_index)?;
            if item.is_applet() != dragged_is_applet {
                return None;
            }
            let candidate = (section_index, icon);
            section_index += 1;
            Some(candidate)
        })
        .min_by(|(_, left), (_, right)| {
            let left_distance = (left.rect.center_x() - point.x).abs();
            let right_distance = (right.rect.center_x() - point.x).abs();
            left_distance.total_cmp(&right_distance)
        })
        .map(|(index, _)| index)
}

fn drag_distance(offset_x: f64, offset_y: f64) -> f64 {
    offset_x.hypot(offset_y)
}

fn icon_motion_frame(state: &Runtime) -> Option<IconMotionFrame> {
    let drag = state.drag.as_ref();
    let slide = state
        .icon_slide
        .as_ref()
        .filter(|slide| slide.started.elapsed() < ICON_SLIDE_DURATION);
    if drag.is_none() && slide.is_none() {
        return None;
    }

    let layout = dock_layout_for_state(state, None);
    let progress = slide.map(icon_slide_progress).unwrap_or(1.0);
    let rects = layout
        .icons
        .iter()
        .filter_map(|icon| {
            let item_key = state.model.items.get(icon.item_index)?.config_key();
            let mut rect = icon.rect;
            if let Some(slide) = slide
                && let Some(from) = slide
                    .from
                    .iter()
                    .find(|from| from.item_key.eq_ignore_ascii_case(&item_key))
            {
                rect = interpolate_rect(from.rect, icon.rect, progress);
            }
            if let Some(drag) = drag
                && drag.item_key.eq_ignore_ascii_case(&item_key)
            {
                rect = Rect {
                    x: drag.current.x - drag.grab_offset.x,
                    y: drag.current.y - drag.grab_offset.y,
                    width: icon.rect.width,
                    height: icon.rect.height,
                };
            }
            Some(IconMotionRect { item_key, rect })
        })
        .collect();

    Some(IconMotionFrame {
        rects,
        floating_item_key: drag.map(|drag| drag.item_key.clone()),
    })
}

fn current_icon_motion_rects(state: &Runtime) -> Vec<IconMotionRect> {
    icon_motion_frame(state)
        .map(|frame| frame.rects)
        .unwrap_or_else(|| layout_icon_motion_rects(state))
}

fn layout_icon_motion_rects(state: &Runtime) -> Vec<IconMotionRect> {
    dock_layout_for_state(state, None)
        .icons
        .iter()
        .filter_map(|icon| {
            let item_key = state.model.items.get(icon.item_index)?.config_key();
            Some(IconMotionRect {
                item_key,
                rect: icon.rect,
            })
        })
        .collect()
}

fn icon_slide_progress(slide: &IconSlide) -> f64 {
    ease_out_cubic(
        (slide.started.elapsed().as_secs_f64() / ICON_SLIDE_DURATION.as_secs_f64()).clamp(0.0, 1.0),
    )
}

fn icon_presence_frame(state: &Runtime) -> Option<IconPresenceFrame> {
    let transition = state
        .icon_presence
        .as_ref()
        .filter(|transition| transition.started.elapsed() < ICON_PRESENCE_DURATION)?;
    let chrome_progress = icon_presence_chrome_progress(transition);
    let enter_progress = icon_presence_enter_progress(transition);
    let exit_progress = icon_presence_exit_progress(transition);
    let layout = dock_layout_for_state(state, None);
    let current = layout
        .icons
        .iter()
        .filter_map(|icon| {
            let item_key = state.model.items.get(icon.item_index)?.config_key();
            let (rect, alpha) = transition
                .from
                .iter()
                .find(|from| from.item_key.eq_ignore_ascii_case(&item_key))
                .map(|from| (interpolate_rect(from.rect, icon.rect, chrome_progress), 1.0))
                .unwrap_or_else(|| {
                    let travel = icon.rect.height * 0.92;
                    (
                        translate_rect(
                            icon.rect,
                            0.0,
                            interpolate(travel, 0.0, enter_progress),
                        ),
                        enter_progress,
                    )
                });
            Some(IconPresenceRect {
                item_key,
                rect,
                alpha,
            })
        })
        .collect::<Vec<_>>();
    let ghosts = transition
        .ghosts
        .iter()
        .map(|ghost| GhostIcon {
            item: ghost.item.clone(),
            rect: translate_rect(
                ghost.rect,
                0.0,
                interpolate(0.0, ghost.rect.height * 0.92, exit_progress),
            ),
            alpha: (1.0 - exit_progress).clamp(0.0, 1.0),
        })
        .collect::<Vec<_>>();

    Some(IconPresenceFrame { current, ghosts })
}

fn ease_out_cubic(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

fn interpolate_rect(from: Rect, to: Rect, progress: f64) -> Rect {
    Rect {
        x: interpolate(from.x, to.x, progress),
        y: interpolate(from.y, to.y, progress),
        width: interpolate(from.width, to.width, progress),
        height: interpolate(from.height, to.height, progress),
    }
}

fn interpolate(from: f64, to: f64, progress: f64) -> f64 {
    from + (to - from) * progress
}

fn ensure_icon_animation_tick(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
) {
    {
        let mut state = state.borrow_mut();
        if state.animation_tick_running {
            return;
        }
        state.animation_tick_running = true;
    }

    let state = Rc::clone(state);
    let window = window.clone();
    let drawing = drawing.clone();
    let gl_area = gl_area.clone();
    glib::timeout_add_local(ICON_ANIMATION_FRAME, move || {
        let keep_running = {
            let mut state = state.borrow_mut();
            prune_finished_icon_slide(&mut state);
            prune_finished_icon_presence(&mut state);
            state.drag.is_some() || state.icon_slide.is_some() || state.icon_presence.is_some()
        };
        sync_dock_window(&state, &window, &drawing, &gl_area, false);
        queue_gl_render_if_enabled(&state, &gl_area);
        drawing.queue_draw();

        if keep_running {
            glib::ControlFlow::Continue
        } else {
            state.borrow_mut().animation_tick_running = false;
            glib::ControlFlow::Break
        }
    });
}

fn ensure_startup_reveal_tick(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
) {
    {
        let mut state = state.borrow_mut();
        if state.startup_reveal_tick_running || state.startup_reveal.is_none() {
            return;
        }
        state.startup_reveal_tick_running = true;
    }

    let state = Rc::clone(state);
    let window = window.clone();
    let drawing = drawing.clone();
    let gl_area = gl_area.clone();
    glib::timeout_add_local(ICON_ANIMATION_FRAME, move || {
        let keep_running = {
            let mut state = state.borrow_mut();
            prune_finished_startup_reveal(&mut state);
            state.startup_reveal.is_some()
        };
        sync_dock_window(&state, &window, &drawing, &gl_area, false);
        queue_gl_render_if_enabled(&state, &gl_area);
        drawing.queue_draw();

        if keep_running {
            glib::ControlFlow::Continue
        } else {
            state.borrow_mut().startup_reveal_tick_running = false;
            glib::ControlFlow::Break
        }
    });
}

fn prune_finished_startup_reveal(state: &mut Runtime) {
    if state
        .startup_reveal
        .as_ref()
        .is_some_and(StartupReveal::finished)
    {
        state.startup_reveal = None;
    }
}

fn prune_finished_icon_presence(state: &mut Runtime) {
    if state
        .icon_presence
        .as_ref()
        .map(|transition| transition.started.elapsed() >= ICON_PRESENCE_DURATION)
        .unwrap_or(false)
    {
        state.icon_presence = None;
    }
}

fn prune_finished_icon_slide(state: &mut Runtime) {
    if state
        .icon_slide
        .as_ref()
        .map(|slide| slide.started.elapsed() >= ICON_SLIDE_DURATION)
        .unwrap_or(false)
    {
        state.icon_slide = None;
    }
}

fn show_context_menu(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    index: usize,
    x: f64,
    y: f64,
) {
    let (item, icon_rect, dock_width) = {
        let state = state.borrow();
        let layout = dock_layout_for_state(&state, None);
        let item = state.model.items.get(index).cloned();
        let icon_rect = layout
            .icons
            .iter()
            .find(|icon| icon.item_index == index)
            .map(|icon| icon.rect);
        (item, icon_rect, layout.size.0)
    };
    let Some(item) = item else {
        return;
    };
    let anchor = icon_rect.unwrap_or(Rect {
        x,
        y,
        width: 1.0,
        height: 1.0,
    });
    if item.is_applet() {
        show_applet_context_menu(state, window, drawing, gl_area, item, anchor, dock_width);
        return;
    }
    if !item.is_application() {
        return;
    }
    let app_actions = application_context_actions(&item);
    let item_key = item.config_key();
    let pinned = item.pinned;
    dismiss_context_menu(state);
    {
        let mut state = state.borrow_mut();
        state.hover = None;
    }

    let menu = GtkBox::new(Orientation::Vertical, 0);
    menu.add_css_class("osdock-context-menu");
    menu.add_css_class("osdock-menu-box");
    menu.set_size_request(CONTEXT_MENU_WIDTH, context_menu_height(app_actions.len()));

    let app_action_count = app_actions.len();
    let has_app_actions = app_action_count > 0;
    for action in app_actions {
        let button = context_menu_button(application_context_action_label(&item, action), false);
        {
            let state = Rc::clone(state);
            let window = window.clone();
            let drawing = drawing.clone();
            let gl_area = gl_area.clone();
            let action_item = item.clone();
            button.connect_clicked(move |_| {
                dismiss_context_menu(&state);
                run_application_context_action(
                    &state,
                    &window,
                    &drawing,
                    &gl_area,
                    &action_item,
                    action,
                );
            });
        }
        menu.append(&button);
    }

    if has_app_actions {
        menu.append(&context_menu_separator());
    }

    let keep = context_menu_button("Keep in Dock", pinned);
    let select = context_menu_button("Select Icon File...", false);
    let theme_icon = context_menu_button("Use Theme Icon...", false);
    let default_icon = context_menu_button("Set Default Icon", false);
    menu.append(&keep);
    menu.append(&select);
    menu.append(&theme_icon);
    menu.append(&default_icon);

    {
        let state = Rc::clone(state);
        let window = window.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        let item_key = item_key.clone();
        keep.connect_clicked(move |_| {
            dismiss_context_menu(&state);
            toggle_keep_in_dock(&state, &window, &drawing, &gl_area, &item_key, pinned);
        });
    }
    {
        let state = Rc::clone(state);
        let window = window.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        let item_key = item_key.clone();
        select.connect_clicked(move |_| {
            dismiss_context_menu(&state);
            sync_dock_window(&state, &window, &drawing, &gl_area, true);
            drawing.queue_draw();
            select_custom_icon(&state, &window, &drawing, &gl_area, item_key.clone());
        });
    }
    {
        let state = Rc::clone(state);
        let window = window.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        let item_key = item_key.clone();
        theme_icon.connect_clicked(move |_| {
            dismiss_context_menu(&state);
            show_theme_icon_menu(&state, &window, &drawing, &gl_area, item_key.clone());
        });
    }
    {
        let state = Rc::clone(state);
        let window = window.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        let item_key = item_key.clone();
        default_icon.connect_clicked(move |_| {
            dismiss_context_menu(&state);
            reset_custom_icon(&state, &window, &drawing, &gl_area, &item_key);
        });
    }

    let popover = Popover::new();
    popover.add_css_class("osdock-context-popover");
    popover.set_autohide(false);
    popover.set_has_arrow(false);
    popover.set_position(PositionType::Top);
    popover.set_offset(0, -(CONTEXT_MENU_GAP.round() as i32));
    popover.set_pointing_to(Some(&context_menu_anchor_rect(anchor, dock_width)));
    popover.set_child(Some(&menu));
    popover.set_parent(drawing);

    present_runtime_popover(state, window, drawing, gl_area, &popover);
}

fn show_applet_context_menu(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    item: DockItem,
    anchor: Rect,
    dock_width: i32,
) {
    let item_key = item.config_key();
    let folder_path = item.folder_applet_path();
    let can_remove = folder_path.is_some();
    dismiss_context_menu(state);
    {
        let mut state = state.borrow_mut();
        state.hover = None;
    }

    let menu = GtkBox::new(Orientation::Vertical, 0);
    menu.add_css_class("osdock-context-menu");
    menu.add_css_class("osdock-menu-box");
    let action_count = 4 + if can_remove { 1 } else { 0 };
    let separator_count = if can_remove { 2 } else { 1 };
    menu.set_size_request(
        CONTEXT_MENU_WIDTH,
        menu_height(action_count, separator_count) + 22,
    );

    let title = Label::new(Some(&item.name));
    title.add_css_class("osdock-menu-title");
    title.set_xalign(0.0);
    menu.append(&title);

    let select = context_menu_icon_button("Select Icon File...", "image-x-generic", false);
    let theme_icon =
        context_menu_icon_button("Use Theme Icon...", "preferences-desktop-icons", false);
    let default_icon = context_menu_icon_button("Set Default Icon", "edit-clear", false);
    menu.append(&select);
    menu.append(&theme_icon);
    menu.append(&default_icon);

    if let Some(path) = folder_path {
        menu.append(&context_menu_separator());
        let remove = context_menu_icon_button("Remove Applet", "list-remove", false);
        {
            let state = Rc::clone(state);
            let window = window.clone();
            let drawing = drawing.clone();
            let gl_area = gl_area.clone();
            let item_key = item_key.clone();
            remove.connect_clicked(move |_| {
                dismiss_context_menu(&state);
                remove_folder_applet(&state, &window, &drawing, &gl_area, &item_key, &path);
            });
        }
        menu.append(&remove);
    }

    menu.append(&context_menu_separator());
    let settings = context_menu_icon_button("OSDockX Settings...", "preferences-system", false);
    menu.append(&settings);

    {
        let state = Rc::clone(state);
        let window = window.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        let item_key = item_key.clone();
        select.connect_clicked(move |_| {
            dismiss_context_menu(&state);
            sync_dock_window(&state, &window, &drawing, &gl_area, true);
            drawing.queue_draw();
            select_custom_icon(&state, &window, &drawing, &gl_area, item_key.clone());
        });
    }
    {
        let state = Rc::clone(state);
        let window = window.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        let item_key = item_key.clone();
        theme_icon.connect_clicked(move |_| {
            dismiss_context_menu(&state);
            show_theme_icon_menu(&state, &window, &drawing, &gl_area, item_key.clone());
        });
    }
    {
        let state = Rc::clone(state);
        let window = window.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        let item_key = item_key.clone();
        default_icon.connect_clicked(move |_| {
            dismiss_context_menu(&state);
            reset_custom_icon(&state, &window, &drawing, &gl_area, &item_key);
        });
    }
    {
        let state = Rc::clone(state);
        let window = window.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        settings.connect_clicked(move |_| {
            dismiss_context_menu(&state);
            show_dock_context_menu(
                &state,
                &window,
                &drawing,
                &gl_area,
                anchor.center_x(),
                anchor.y + anchor.height / 2.0,
            );
        });
    }

    let popover = Popover::new();
    popover.add_css_class("osdock-context-popover");
    popover.set_autohide(false);
    popover.set_has_arrow(false);
    popover.set_position(PositionType::Top);
    popover.set_offset(0, -(CONTEXT_MENU_GAP.round() as i32));
    popover.set_pointing_to(Some(&context_menu_anchor_rect(anchor, dock_width)));
    popover.set_child(Some(&menu));
    popover.set_parent(drawing);

    present_runtime_popover(state, window, drawing, gl_area, &popover);
}

fn show_dock_context_menu(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    x: f64,
    y: f64,
) {
    let dock_width = {
        let state = state.borrow();
        dock_layout_for_state(&state, None).size.0
    };
    dismiss_context_menu(state);
    {
        let mut state = state.borrow_mut();
        state.hover = None;
    }

    let menu = GtkBox::new(Orientation::Vertical, 0);
    menu.add_css_class("osdock-context-menu");
    menu.add_css_class("osdock-menu-box");
    menu.set_size_request(
        DOCK_CONTEXT_MENU_WIDTH,
        menu_height(DOCK_CONTEXT_MENU_ACTIONS, 2) + 22,
    );

    let title = Label::new(Some("OSDockX Settings"));
    title.add_css_class("osdock-menu-title");
    title.set_xalign(0.0);
    menu.append(&title);

    append_dock_context_button(
        &menu,
        state,
        window,
        drawing,
        gl_area,
        DockContextAction::AddApplication,
    );
    append_dock_context_button(
        &menu,
        state,
        window,
        drawing,
        gl_area,
        DockContextAction::AddFolderApplet,
    );
    menu.append(&context_menu_separator());
    append_dock_context_button(
        &menu,
        state,
        window,
        drawing,
        gl_area,
        DockContextAction::LargerIcons,
    );
    append_dock_context_button(
        &menu,
        state,
        window,
        drawing,
        gl_area,
        DockContextAction::SmallerIcons,
    );
    append_dock_context_button(
        &menu,
        state,
        window,
        drawing,
        gl_area,
        DockContextAction::HoverEffect,
    );
    append_dock_context_button(
        &menu,
        state,
        window,
        drawing,
        gl_area,
        DockContextAction::ToggleAutohide,
    );
    append_dock_context_button(
        &menu,
        state,
        window,
        drawing,
        gl_area,
        DockContextAction::ToggleReserveSpace,
    );
    menu.append(&context_menu_separator());

    append_dock_context_button(
        &menu,
        state,
        window,
        drawing,
        gl_area,
        DockContextAction::ReloadTheme,
    );
    append_dock_context_button(
        &menu,
        state,
        window,
        drawing,
        gl_area,
        DockContextAction::ResetDefaults,
    );
    append_dock_context_button(
        &menu,
        state,
        window,
        drawing,
        gl_area,
        DockContextAction::ResetCustomIcons,
    );
    append_dock_context_button(
        &menu,
        state,
        window,
        drawing,
        gl_area,
        DockContextAction::OpenConfigFolder,
    );

    let popover = Popover::new();
    popover.add_css_class("osdock-context-popover");
    popover.set_autohide(true);
    popover.set_has_arrow(false);
    popover.set_position(PositionType::Top);
    popover.set_offset(0, -(CONTEXT_MENU_GAP.round() as i32));
    popover.set_pointing_to(Some(&context_menu_anchor_rect(
        Rect {
            x,
            y,
            width: 1.0,
            height: 1.0,
        },
        dock_width,
    )));
    popover.set_child(Some(&menu));
    popover.set_parent(drawing);

    present_runtime_popover(state, window, drawing, gl_area, &popover);
}

fn append_dock_context_button(
    menu: &GtkBox,
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    action: DockContextAction,
) {
    let checked = dock_context_action_checked(state, action);
    let button = context_menu_icon_button(
        dock_context_action_label(action),
        dock_context_action_icon(action),
        checked,
    );
    {
        let state = Rc::clone(state);
        let window = window.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        button.connect_clicked(move |_| {
            dismiss_context_menu(&state);
            run_dock_context_action(&state, &window, &drawing, &gl_area, action);
        });
    }
    menu.append(&button);
}

fn dock_context_action_checked(state: &Rc<RefCell<Runtime>>, action: DockContextAction) -> bool {
    let state = state.borrow();
    match action {
        DockContextAction::ToggleAutohide => state.config.dock.autohide,
        DockContextAction::ToggleReserveSpace => state.config.dock.reserve_space,
        _ => false,
    }
}

fn dock_context_action_label(action: DockContextAction) -> &'static str {
    match action {
        DockContextAction::AddApplication => "Add Application...",
        DockContextAction::AddFolderApplet => "Add Folder Applet...",
        DockContextAction::LargerIcons => "Larger Icons",
        DockContextAction::SmallerIcons => "Smaller Icons",
        DockContextAction::HoverEffect => "Hover Effect...",
        DockContextAction::ToggleAutohide => "Auto Hide",
        DockContextAction::ToggleReserveSpace => "Reserve Screen Space",
        DockContextAction::ReloadTheme => "Reload Theme",
        DockContextAction::ResetDefaults => "Reset to Dock Defaults",
        DockContextAction::ResetCustomIcons => "Reset Custom Icons",
        DockContextAction::OpenConfigFolder => "Open Config Folder",
    }
}

fn dock_context_action_icon(action: DockContextAction) -> &'static str {
    match action {
        DockContextAction::AddApplication => "list-add",
        DockContextAction::AddFolderApplet => "folder-new",
        DockContextAction::LargerIcons => "zoom-in",
        DockContextAction::SmallerIcons => "zoom-out",
        DockContextAction::HoverEffect => "media-playback-start",
        DockContextAction::ToggleAutohide => "view-fullscreen",
        DockContextAction::ToggleReserveSpace => "view-restore",
        DockContextAction::ReloadTheme => "view-refresh",
        DockContextAction::ResetDefaults => "edit-clear-all",
        DockContextAction::ResetCustomIcons => "edit-delete",
        DockContextAction::OpenConfigFolder => "preferences-system",
    }
}

fn application_context_actions(item: &DockItem) -> Vec<ApplicationContextAction> {
    if !item.is_application() {
        return Vec::new();
    }

    let mut actions = Vec::new();

    if item.desktop_id.is_some() {
        actions.push(ApplicationContextAction::Launch);
    }
    if item.is_running() {
        if item.active {
            actions.push(ApplicationContextAction::Minimize);
        } else if item.primary_window().is_some() {
            actions.push(ApplicationContextAction::Focus);
        }
        actions.push(ApplicationContextAction::Close);
    }

    actions
}

fn application_context_action_label(
    item: &DockItem,
    action: ApplicationContextAction,
) -> &'static str {
    match action {
        ApplicationContextAction::Launch if item.is_running() => "Open New Window",
        ApplicationContextAction::Launch => "Open",
        ApplicationContextAction::Focus => "Show",
        ApplicationContextAction::Minimize => "Hide",
        ApplicationContextAction::Close => "Close Application",
    }
}

fn context_menu_height(app_action_count: usize) -> i32 {
    let separator_count = i32::from(app_action_count > 0);
    menu_height(
        app_action_count + CONTEXT_MENU_SETTINGS_COUNT,
        separator_count as usize,
    )
}

fn menu_height(action_count: usize, separator_count: usize) -> i32 {
    CONTEXT_MENU_CHROME_HEIGHT
        + (action_count as i32 * CONTEXT_MENU_ITEM_HEIGHT)
        + separator_count as i32 * CONTEXT_MENU_SEPARATOR_HEIGHT
}

fn context_menu_anchor_rect(icon_rect: Rect, dock_width: i32) -> gdk::Rectangle {
    let width = icon_rect.width.ceil().max(1.0) as i32;
    let height = icon_rect.height.ceil().max(1.0) as i32;
    let max_x = (dock_width - width - 2).max(2);
    let x = (icon_rect.x.floor() as i32).clamp(2, max_x);
    let y = (icon_rect.y.floor() as i32).max(2);
    gdk::Rectangle::new(x, y, width, height)
}

fn context_menu_button(label: &str, checked: bool) -> Button {
    let button = Button::builder().has_frame(false).build();
    button.add_css_class("osdock-menu-item");
    button.set_focusable(false);

    let row = GtkBox::new(Orientation::Horizontal, 0);
    row.add_css_class("osdock-menu-row");
    let check = Label::new(Some(if checked { "✓" } else { "" }));
    check.add_css_class("osdock-menu-check");
    check.set_halign(Align::Start);
    let text = Label::new(Some(label));
    text.set_xalign(0.0);
    text.set_hexpand(true);
    text.set_halign(Align::Start);
    row.append(&check);
    row.append(&text);
    button.set_child(Some(&row));
    button
}

fn context_menu_icon_button(label: &str, icon_name: &str, checked: bool) -> Button {
    let button = Button::builder().has_frame(false).build();
    button.add_css_class("osdock-menu-item");
    button.set_focusable(false);

    let row = GtkBox::new(Orientation::Horizontal, 0);
    row.add_css_class("osdock-menu-row");
    let check = Label::new(Some(if checked { "✓" } else { "" }));
    check.add_css_class("osdock-menu-check");
    check.set_halign(Align::Start);
    let icon = Image::from_icon_name(icon_name);
    icon.add_css_class("osdock-menu-icon");
    icon.set_pixel_size(16);
    let text = Label::new(Some(label));
    text.set_xalign(0.0);
    text.set_hexpand(true);
    text.set_halign(Align::Start);
    row.append(&check);
    row.append(&icon);
    row.append(&text);
    button.set_child(Some(&row));
    button
}

fn context_menu_separator() -> GtkBox {
    let separator = GtkBox::new(Orientation::Horizontal, 0);
    separator.add_css_class("osdock-menu-separator");
    separator.set_hexpand(true);
    separator.set_halign(Align::Fill);
    separator.set_size_request(-1, CONTEXT_MENU_SEPARATOR_HEIGHT);
    separator
}

fn present_runtime_popover(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    popover: &Popover,
) {
    window.set_focusable(true);
    window.set_can_focus(true);

    {
        let state = Rc::clone(state);
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        let window = window.clone();
        popover.connect_closed(move |popover| {
            if let Ok(mut runtime) = state.try_borrow_mut()
                && runtime
                    .context_menu
                    .as_ref()
                    .is_some_and(|current| current == popover)
            {
                runtime.context_menu = None;
                runtime.hover = None;
            }

            let state = Rc::clone(&state);
            let drawing = drawing.clone();
            let gl_area = gl_area.clone();
            let window = window.clone();
            let popover_for_cleanup = popover.clone();
            glib::idle_add_local_once(move || {
                let mut menu_still_open = false;
                if let Ok(mut runtime) = state.try_borrow_mut()
                    && runtime
                        .context_menu
                        .as_ref()
                        .is_some_and(|current| current == &popover_for_cleanup)
                {
                    runtime.context_menu = None;
                    runtime.hover = None;
                }
                if let Ok(runtime) = state.try_borrow() {
                    menu_still_open = runtime.context_menu.is_some();
                }

                if popover_for_cleanup.parent().is_some() {
                    popover_for_cleanup.unparent();
                }
                if !menu_still_open {
                    window.set_can_focus(false);
                    window.set_focusable(false);
                }
                queue_gl_render_if_enabled(&state, &gl_area);
                drawing.queue_draw();
            });
        });
    }

    {
        let mut state = state.borrow_mut();
        state.context_menu = Some(popover.clone());
    }
    popover.popup();
    sync_dock_window(state, window, drawing, gl_area, true);
    queue_gl_render_if_enabled(state, gl_area);
    drawing.queue_draw();
}

fn show_applet_fan_for_item(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    item: &DockItem,
    index: usize,
    x: f64,
    y: f64,
) {
    let Some(source) = applet_fan_source(item) else {
        return;
    };
    show_applet_fan(state, window, drawing, gl_area, source, index, x, y);
}

fn show_applet_fan(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    source: AppletFanSource,
    index: usize,
    x: f64,
    y: f64,
) {
    let (icon_rect, dock_width) = {
        let state = state.borrow();
        let layout = dock_layout_for_state(&state, None);
        let icon_rect = layout
            .icons
            .iter()
            .find(|icon| icon.item_index == index)
            .map(|icon| icon.rect);
        (icon_rect, layout.size.0)
    };
    let anchor = icon_rect.unwrap_or(Rect {
        x,
        y,
        width: 1.0,
        height: 1.0,
    });

    dismiss_context_menu(state);
    {
        let mut state = state.borrow_mut();
        state.hover = None;
    }

    let (fan_width, fan_height) = applet_fan_size(&source);
    let fan = DrawingArea::new();
    fan.set_content_width(fan_width);
    fan.set_content_height(fan_height);
    fan.set_size_request(fan_width, fan_height);
    fan.set_focusable(false);

    let source = Rc::new(source);
    let hit_regions = Rc::new(RefCell::new(Vec::<AppletFanHitRegion>::new()));
    let hover_index = Rc::new(RefCell::new(None::<usize>));
    let icon_cache = Rc::new(RefCell::new(HashMap::<String, Option<Pixbuf>>::new()));
    let reveal_started = Rc::new(Instant::now());

    {
        let source = Rc::clone(&source);
        let hit_regions = Rc::clone(&hit_regions);
        let hover_index = Rc::clone(&hover_index);
        let icon_cache = Rc::clone(&icon_cache);
        let reveal_started = Rc::clone(&reveal_started);
        fan.set_draw_func(move |_, cr, width, height| {
            draw_applet_fan(
                cr,
                width,
                height,
                &source,
                *hover_index.borrow(),
                applet_fan_reveal_progress(reveal_started.elapsed()),
                &mut hit_regions.borrow_mut(),
                &mut icon_cache.borrow_mut(),
            );
        });
    }

    let motion = EventControllerMotion::new();
    {
        let hit_regions = Rc::clone(&hit_regions);
        let hover_index = Rc::clone(&hover_index);
        let fan = fan.clone();
        motion.connect_motion(move |_, x, y| {
            let point = Point { x, y };
            let next = hit_regions
                .borrow()
                .iter()
                .find(|hit| hit.rect.contains(point))
                .map(|hit| hit.index);
            let mut hover = hover_index.borrow_mut();
            if *hover != next {
                *hover = next;
                fan.queue_draw();
            }
        });
    }
    {
        let hover_index = Rc::clone(&hover_index);
        let fan = fan.clone();
        motion.connect_leave(move |_| {
            let mut hover = hover_index.borrow_mut();
            if hover.is_some() {
                *hover = None;
                fan.queue_draw();
            }
        });
    }
    fan.add_controller(motion);

    let click = GestureClick::new();
    click.set_button(1);
    {
        let state = Rc::clone(state);
        let window = window.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        let hit_regions = Rc::clone(&hit_regions);
        click.connect_released(move |_, _, x, y| {
            let point = Point { x, y };
            let action = hit_regions
                .borrow()
                .iter()
                .find(|hit| hit.rect.contains(point))
                .map(|hit| hit.action.clone());
            let Some(action) = action else {
                return;
            };

            dismiss_context_menu(&state);
            run_applet_fan_action(action);
            sync_dock_window(&state, &window, &drawing, &gl_area, true);
            queue_gl_render_if_enabled(&state, &gl_area);
            drawing.queue_draw();
        });
    }
    fan.add_controller(click);

    let popover = Popover::new();
    popover.add_css_class("osdock-stack-popover");
    popover.set_autohide(true);
    popover.set_has_arrow(false);
    popover.set_position(PositionType::Top);
    popover.set_offset(0, -(APPLET_FAN_GAP.round() as i32));
    popover.set_pointing_to(Some(&context_menu_anchor_rect(anchor, dock_width)));
    popover.set_child(Some(&fan));
    popover.set_parent(drawing);

    present_runtime_popover(state, window, drawing, gl_area, &popover);
    start_applet_fan_reveal_tick(&fan, reveal_started);
}

fn applet_fan_source(item: &DockItem) -> Option<AppletFanSource> {
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

fn applet_fan_size(source: &AppletFanSource) -> (i32, i32) {
    let row_count = source.entries.len() + 1 + usize::from(source.entries.is_empty());
    let height = APPLET_FAN_TOP_PADDING
        + APPLET_FAN_BOTTOM_PADDING
        + APPLET_FAN_ROW_HEIGHT * row_count as f64;
    (APPLET_FAN_WIDTH, height.ceil() as i32)
}

fn start_applet_fan_reveal_tick(fan: &DrawingArea, started: Rc<Instant>) {
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

fn applet_fan_reveal_progress(elapsed: Duration) -> f64 {
    (elapsed.as_secs_f64() / APPLET_FAN_REVEAL_DURATION.as_secs_f64()).clamp(0.0, 1.0)
}

fn applet_fan_row_reveal(progress: f64, index: usize, row_count: usize) -> f64 {
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

fn recent_applet_entries_from_dir(dir: &Path, limit: usize) -> AppletFanDirectoryEntries {
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

fn draw_applet_fan(
    cr: &Context,
    width: i32,
    height: i32,
    source: &AppletFanSource,
    hover_index: Option<usize>,
    reveal_progress: f64,
    hit_regions: &mut Vec<AppletFanHitRegion>,
    icon_cache: &mut HashMap<String, Option<Pixbuf>>,
) {
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

fn applet_fan_more_label(source: &AppletFanSource, visible_count: usize) -> String {
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
        label_center_x,
        center_y,
        label_width,
        APPLET_FAN_LABEL_HEIGHT,
        &fitted,
        hovered,
        rotation * 0.55,
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
        label_right - label_width / 2.0,
        center_y,
        label_width,
        APPLET_FAN_LABEL_HEIGHT,
        &fitted,
        false,
        -0.03,
    );
    let _ = cr.pop_group_to_source();
    let _ = cr.paint_with_alpha(alpha);
    cr.restore().ok();
}

fn draw_fan_label(
    cr: &Context,
    center_x: f64,
    center_y: f64,
    width: f64,
    height: f64,
    text: &str,
    hovered: bool,
    rotation: f64,
) {
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
        let head_len = (keep + 1) / 2;
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

fn rounded_rect_path(cr: &Context, x: f64, y: f64, width: f64, height: f64, radius: f64) {
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

fn run_applet_fan_action(action: AppletFanHitAction) {
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

fn open_path_in_default_app(path: &Path) {
    let file = gio::File::for_path(path);
    let uri = file.uri();
    open_uri(uri.as_str());
}

fn open_uri(uri: &str) {
    if let Err(error) = gio::AppInfo::launch_default_for_uri(uri, None::<&gio::AppLaunchContext>) {
        tracing::warn!("could not open {uri}: {error:#}");
    }
}

fn run_application_context_action(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    item: &DockItem,
    action: ApplicationContextAction,
) {
    match action {
        ApplicationContextAction::Launch => launch_item(state, item),
        ApplicationContextAction::Focus => focus_item_window(state, item),
        ApplicationContextAction::Minimize => minimize_item_window(state, item),
        ApplicationContextAction::Close => close_item_application(state, item),
    }

    sync_dock_window(state, window, drawing, gl_area, true);
    queue_gl_render_if_enabled(state, gl_area);
    drawing.queue_draw();
}

fn run_dock_context_action(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    action: DockContextAction,
) {
    match action {
        DockContextAction::AddApplication => {
            show_add_application_menu(state, window, drawing, gl_area);
        }
        DockContextAction::AddFolderApplet => {
            select_folder_applet(state, window, drawing, gl_area);
        }
        DockContextAction::LargerIcons => {
            change_icon_size(state, window, drawing, gl_area, 8);
        }
        DockContextAction::SmallerIcons => {
            change_icon_size(state, window, drawing, gl_area, -8);
        }
        DockContextAction::HoverEffect => {
            show_hover_settings_menu(state, window, drawing, gl_area);
        }
        DockContextAction::ToggleAutohide => {
            update_dock_config(state, window, drawing, gl_area, |state| {
                state.config.dock.autohide = !state.config.dock.autohide;
                state.hidden = false;
            });
        }
        DockContextAction::ToggleReserveSpace => {
            update_dock_config(state, window, drawing, gl_area, |state| {
                state.config.dock.reserve_space = !state.config.dock.reserve_space;
            });
        }
        DockContextAction::ReloadTheme => {
            {
                let mut state = state.borrow_mut();
                refresh_config_and_theme(&mut state);
                state.refresh_model();
                state.icons.clear();
            }
            ensure_icon_animation_if_needed(state, window, drawing, gl_area);
            sync_dock_window(state, window, drawing, gl_area, true);
            queue_gl_render_if_enabled(state, gl_area);
            drawing.queue_draw();
        }
        DockContextAction::ResetDefaults => {
            reset_runtime_defaults(state, window, drawing, gl_area);
        }
        DockContextAction::ResetCustomIcons => {
            reset_runtime_custom_icons(state, window, drawing, gl_area);
        }
        DockContextAction::OpenConfigFolder => {
            let config_path = state.borrow().config_path.clone();
            let path = config_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or(config_path);
            open_path_in_default_app(&path);
        }
    }
}

fn reset_runtime_defaults(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
) {
    {
        let mut state = state.borrow_mut();
        let custom_icons = state.config.custom_icons.clone();
        state.config = Config::default().normalized();
        state.config.custom_icons = custom_icons;
        if let Err(error) = ThemePack::restore_builtin_theme_pack(&state.config.theme.preset) {
            tracing::warn!(
                "could not restore built-in theme pack {}: {error:#}",
                state.config.theme.preset
            );
        }
        state.hidden = false;
        state.hover = None;
        state.icons.clear();
        state.last_size = None;
        state.last_geometry = None;
        state.last_reserved_geometry = None;
        state.last_shape_size = None;
        state.last_shape_label = None;
        let (_, _, theme) = resolve_runtime_theme(state.composited, &state.config.theme);
        state.theme = theme;
        save_runtime_config(&state);
        state.refresh_model();
    }

    ensure_icon_animation_if_needed(state, window, drawing, gl_area);
    sync_dock_window(state, window, drawing, gl_area, true);
    queue_gl_render_if_enabled(state, gl_area);
    drawing.queue_draw();
}

fn reset_runtime_custom_icons(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
) {
    {
        let mut state = state.borrow_mut();
        state.config.custom_icons.clear();
        state.icons.clear();
        save_runtime_config(&state);
    }

    sync_dock_window(state, window, drawing, gl_area, true);
    queue_gl_render_if_enabled(state, gl_area);
    drawing.queue_draw();
}

fn update_dock_config(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    update: impl FnOnce(&mut Runtime),
) {
    {
        let mut state = state.borrow_mut();
        update(&mut state);
        state.last_size = None;
        state.last_geometry = None;
        state.last_reserved_geometry = None;
        state.last_shape_size = None;
        save_runtime_config(&state);
        state.refresh_model();
    }
    ensure_icon_animation_if_needed(state, window, drawing, gl_area);
    sync_dock_window(state, window, drawing, gl_area, true);
    queue_gl_render_if_enabled(state, gl_area);
    drawing.queue_draw();
}

fn change_icon_size(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    delta: i32,
) {
    update_dock_config(state, window, drawing, gl_area, |state| {
        let icon_size = (state.config.dock.icon_size as i32 + delta).clamp(
            SEPARATOR_RESIZE_MIN_ICON_SIZE as i32,
            SEPARATOR_RESIZE_MAX_ICON_SIZE as i32,
        );
        state.config.dock.icon_size = icon_size as u32;
        state.hover = None;
    });
}

fn show_hover_settings_menu(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
) {
    let (dock_width, current_zoom) = {
        let state = state.borrow();
        (dock_layout_for_state(&state, None).size.0, state.config.dock.zoom_strength)
    };

    let menu = GtkBox::new(Orientation::Vertical, 8);
    menu.add_css_class("osdock-context-menu");
    menu.add_css_class("osdock-menu-box");
    menu.set_size_request(HOVER_SETTINGS_MENU_WIDTH, 122);

    let title = Label::new(Some("Hover Effect Strength"));
    title.add_css_class("osdock-menu-title");
    title.set_xalign(0.0);
    menu.append(&title);

    let slider = Scale::with_range(Orientation::Horizontal, 0.0, 1.6, 0.02);
    slider.set_draw_value(true);
    slider.set_digits(2);
    slider.set_hexpand(true);
    slider.set_margin_start(10);
    slider.set_margin_end(10);
    slider.set_margin_bottom(8);
    slider.set_value(current_zoom);

    {
        let state = Rc::clone(state);
        let window = window.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        slider.connect_value_changed(move |slider| {
            set_hover_strength(&state, &window, &drawing, &gl_area, slider.value());
        });
    }
    menu.append(&slider);

    let popover = Popover::new();
    popover.add_css_class("osdock-context-popover");
    popover.set_autohide(true);
    popover.set_has_arrow(false);
    popover.set_position(PositionType::Top);
    popover.set_offset(0, -(CONTEXT_MENU_GAP.round() as i32));
    popover.set_pointing_to(Some(&context_menu_anchor_rect(
        Rect {
            x: dock_width as f64 / 2.0,
            y: 1.0,
            width: 1.0,
            height: 1.0,
        },
        dock_width,
    )));
    popover.set_child(Some(&menu));
    popover.set_parent(drawing);

    present_runtime_popover(state, window, drawing, gl_area, &popover);
}

fn set_hover_strength(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    value: f64,
) {
    {
        let mut state = state.borrow_mut();
        state.config.dock.zoom_strength = value.clamp(0.0, 1.6);
        state.last_size = None;
        state.last_geometry = None;
        state.last_reserved_geometry = None;
        state.last_shape_size = None;
        save_runtime_config(&state);
    }

    sync_dock_window(state, window, drawing, gl_area, true);
    queue_gl_render_if_enabled(state, gl_area);
    drawing.queue_draw();
}

fn toggle_keep_in_dock(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    item_key: &str,
    pinned: bool,
) {
    {
        let mut state = state.borrow_mut();
        if pinned {
            state
                .config
                .pinned
                .retain(|id| !id.eq_ignore_ascii_case(item_key));
        } else if !state
            .config
            .pinned
            .iter()
            .any(|id| id.eq_ignore_ascii_case(item_key))
        {
            state.config.pinned.push(item_key.to_string());
        }
        save_runtime_config(&state);
        state.refresh_model();
        state.icons.clear();
    }
    ensure_icon_animation_if_needed(state, window, drawing, gl_area);
    sync_dock_window(state, window, drawing, gl_area, true);
    queue_gl_render_if_enabled(state, gl_area);
    drawing.queue_draw();
}

fn reset_custom_icon(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    item_key: &str,
) {
    {
        let mut state = state.borrow_mut();
        state
            .config
            .custom_icons
            .retain(|key, _| !key.eq_ignore_ascii_case(item_key));
        save_runtime_config(&state);
        state.icons.clear();
    }
    sync_dock_window(state, window, drawing, gl_area, true);
    queue_gl_render_if_enabled(state, gl_area);
    drawing.queue_draw();
}

fn set_custom_icon_value(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    item_key: &str,
    value: String,
) {
    {
        let mut state = state.borrow_mut();
        state
            .config
            .custom_icons
            .insert(item_key.to_string(), value);
        save_runtime_config(&state);
        state.icons.clear();
    }
    sync_dock_window(state, window, drawing, gl_area, true);
    queue_gl_render_if_enabled(state, gl_area);
    drawing.queue_draw();
}

fn show_add_application_menu(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
) {
    let (apps, dock_width) = {
        let state = state.borrow();
        let pinned = state
            .config
            .pinned
            .iter()
            .map(|id| id.to_ascii_lowercase())
            .collect::<Vec<_>>();
        let apps = state
            .desktop_index
            .apps()
            .into_iter()
            .filter(|app| {
                !pinned
                    .iter()
                    .any(|id| id == &app.desktop_id.to_ascii_lowercase())
            })
            .cloned()
            .collect::<Vec<_>>();
        let dock_width = dock_layout_for_state(&state, None).size.0;
        (apps, dock_width)
    };

    let menu = GtkBox::new(Orientation::Vertical, 0);
    menu.add_css_class("osdock-context-menu");
    menu.add_css_class("osdock-menu-box");
    let visible_rows = apps.len().clamp(1, ADD_APPLICATION_MENU_VISIBLE_ROWS);
    menu.set_size_request(
        ADD_APPLICATION_MENU_WIDTH,
        menu_height(visible_rows, 0) + 56,
    );

    let title = Label::new(Some("Add Application"));
    title.add_css_class("osdock-menu-title");
    title.set_xalign(0.0);
    menu.append(&title);

    let mut focus_search = None;
    if apps.is_empty() {
        let empty = Label::new(Some("No unpinned applications found"));
        empty.add_css_class("osdock-menu-title");
        empty.set_xalign(0.0);
        menu.append(&empty);
    } else {
        let search = SearchEntry::new();
        search.add_css_class("osdock-menu-search");
        search.set_placeholder_text(Some("Search applications"));
        search.set_hexpand(true);
        menu.append(&search);
        focus_search = Some(search.clone());

        let list = GtkBox::new(Orientation::Vertical, 0);
        let mut rows = Vec::new();
        for app in apps {
            let button = context_menu_icon_button(
                &app.name,
                app.icon_name
                    .as_deref()
                    .unwrap_or("application-x-executable"),
                false,
            );
            {
                let state = Rc::clone(state);
                let window = window.clone();
                let drawing = drawing.clone();
                let gl_area = gl_area.clone();
                let desktop_id = app.desktop_id.clone();
                button.connect_clicked(move |_| {
                    dismiss_context_menu(&state);
                    pin_application(&state, &window, &drawing, &gl_area, &desktop_id);
                });
            }
            rows.push((
                format!("{} {}", app.name, app.desktop_id).to_ascii_lowercase(),
                button.clone(),
            ));
            list.append(&button);
        }
        let scroll = ScrolledWindow::new();
        scroll.set_policy(PolicyType::Never, PolicyType::Automatic);
        scroll.set_size_request(
            -1,
            (visible_rows as i32 * CONTEXT_MENU_ITEM_HEIGHT).max(CONTEXT_MENU_ITEM_HEIGHT),
        );
        scroll.set_child(Some(&list));
        menu.append(&scroll);

        let rows = Rc::new(rows);
        search.connect_search_changed(move |entry| {
            let query = entry.text().trim().to_ascii_lowercase();
            for (haystack, button) in rows.iter() {
                button.set_visible(query.is_empty() || haystack.contains(&query));
            }
        });
    }

    let popover = Popover::new();
    popover.add_css_class("osdock-context-popover");
    popover.set_autohide(true);
    popover.set_has_arrow(false);
    popover.set_position(PositionType::Top);
    popover.set_offset(0, -(CONTEXT_MENU_GAP.round() as i32));
    popover.set_pointing_to(Some(&context_menu_anchor_rect(
        Rect {
            x: dock_width as f64 / 2.0,
            y: 1.0,
            width: 1.0,
            height: 1.0,
        },
        dock_width,
    )));
    popover.set_child(Some(&menu));
    popover.set_parent(drawing);

    present_runtime_popover(state, window, drawing, gl_area, &popover);
    if let Some(search) = focus_search {
        let _ = search.grab_focus();
    }
}

fn show_theme_icon_menu(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    item_key: String,
) {
    let (icon_names, dock_width) = {
        let state = state.borrow();
        let dock_width = dock_layout_for_state(&state, None).size.0;
        (theme_icon_names(), dock_width)
    };
    let visible_rows = if icon_names.is_empty() {
        1
    } else {
        THEME_ICON_MENU_VISIBLE_ROWS
    };

    let menu = GtkBox::new(Orientation::Vertical, 0);
    menu.add_css_class("osdock-context-menu");
    menu.add_css_class("osdock-menu-box");
    menu.set_size_request(THEME_ICON_MENU_WIDTH, menu_height(visible_rows, 0) + 56);

    let title = Label::new(Some("Use Theme Icon"));
    title.add_css_class("osdock-menu-title");
    title.set_xalign(0.0);
    menu.append(&title);

    let mut focus_search = None;
    if icon_names.is_empty() {
        let empty = Label::new(Some("No theme icons found"));
        empty.add_css_class("osdock-menu-title");
        empty.set_xalign(0.0);
        menu.append(&empty);
    } else {
        let search = SearchEntry::new();
        search.add_css_class("osdock-menu-search");
        search.set_placeholder_text(Some("Search icon theme"));
        search.set_hexpand(true);
        menu.append(&search);
        focus_search = Some(search.clone());

        let list = GtkBox::new(Orientation::Vertical, 0);
        let scroll = ScrolledWindow::new();
        scroll.set_policy(PolicyType::Never, PolicyType::Automatic);
        scroll.set_size_request(
            -1,
            (visible_rows as i32 * CONTEXT_MENU_ITEM_HEIGHT).max(CONTEXT_MENU_ITEM_HEIGHT),
        );
        scroll.set_child(Some(&list));
        menu.append(&scroll);

        let icon_names = Rc::new(icon_names);
        populate_theme_icon_list(
            &list,
            icon_names.as_ref(),
            "",
            state,
            window,
            drawing,
            gl_area,
            &item_key,
        );

        {
            let list = list.clone();
            let icon_names = Rc::clone(&icon_names);
            let state = Rc::clone(state);
            let window = window.clone();
            let drawing = drawing.clone();
            let gl_area = gl_area.clone();
            search.connect_search_changed(move |entry| {
                populate_theme_icon_list(
                    &list,
                    icon_names.as_ref(),
                    entry.text().as_str(),
                    &state,
                    &window,
                    &drawing,
                    &gl_area,
                    &item_key,
                );
            });
        }
    }

    let popover = Popover::new();
    popover.add_css_class("osdock-context-popover");
    popover.set_autohide(true);
    popover.set_has_arrow(false);
    popover.set_position(PositionType::Top);
    popover.set_offset(0, -(CONTEXT_MENU_GAP.round() as i32));
    popover.set_pointing_to(Some(&context_menu_anchor_rect(
        Rect {
            x: dock_width as f64 / 2.0,
            y: 1.0,
            width: 1.0,
            height: 1.0,
        },
        dock_width,
    )));
    popover.set_child(Some(&menu));
    popover.set_parent(drawing);

    present_runtime_popover(state, window, drawing, gl_area, &popover);
    if let Some(search) = focus_search {
        let _ = search.grab_focus();
    }
}

fn theme_icon_names() -> Vec<String> {
    let Some(display) = gdk::Display::default() else {
        return Vec::new();
    };
    let icon_theme = IconTheme::for_display(&display);
    let mut names = icon_theme
        .icon_names()
        .into_iter()
        .map(|name| name.to_string())
        .filter(|name| !name.trim().is_empty())
        .collect::<Vec<_>>();
    names.sort_by_key(|name| name.to_ascii_lowercase());
    names.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    names
}

fn populate_theme_icon_list(
    list: &GtkBox,
    icon_names: &[String],
    query: &str,
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    item_key: &str,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        let empty = Label::new(Some("Search for an icon name"));
        empty.add_css_class("osdock-menu-title");
        empty.set_xalign(0.0);
        list.append(&empty);
        return;
    }

    let icon_theme = gdk::Display::default().map(|display| IconTheme::for_display(&display));
    let mut match_count = 0;
    for icon_name in icon_names
        .iter()
        .filter(|name| name.to_ascii_lowercase().contains(&query))
    {
        let Some(icon_theme) = icon_theme.as_ref() else {
            break;
        };
        if !theme_icon_is_loadable(icon_theme, icon_name) {
            continue;
        }

        let button = context_menu_icon_button(icon_name, icon_name, false);
        {
            let state = Rc::clone(state);
            let window = window.clone();
            let drawing = drawing.clone();
            let gl_area = gl_area.clone();
            let item_key = item_key.to_string();
            let icon_name = icon_name.clone();
            button.connect_clicked(move |_| {
                dismiss_context_menu(&state);
                set_custom_icon_value(
                    &state,
                    &window,
                    &drawing,
                    &gl_area,
                    &item_key,
                    icon_name.clone(),
                );
            });
        }
        list.append(&button);
        match_count += 1;
        if match_count >= THEME_ICON_MENU_MAX_MATCHES {
            break;
        }
    }

    if match_count == 0 {
        let empty = Label::new(Some("No matching icons"));
        empty.add_css_class("osdock-menu-title");
        empty.set_xalign(0.0);
        list.append(&empty);
    }
}

fn theme_icon_is_loadable(icon_theme: &IconTheme, icon_name: &str) -> bool {
    if !icon_theme.has_icon(icon_name) {
        return false;
    }
    let paintable = icon_theme.lookup_icon(
        icon_name,
        &[],
        16,
        1,
        TextDirection::None,
        IconLookupFlags::empty(),
    );
    paintable
        .file()
        .and_then(|file| file.path())
        .is_some_and(|path| path.exists())
}

fn pin_application(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    desktop_id: &str,
) {
    {
        let mut state = state.borrow_mut();
        if !state
            .config
            .pinned
            .iter()
            .any(|id| id.eq_ignore_ascii_case(desktop_id))
        {
            state.config.pinned.push(desktop_id.to_string());
        }
        save_runtime_config(&state);
        state.refresh_model();
        state.icons.clear();
    }
    ensure_icon_animation_if_needed(state, window, drawing, gl_area);
    sync_dock_window(state, window, drawing, gl_area, true);
    queue_gl_render_if_enabled(state, gl_area);
    drawing.queue_draw();
}

fn select_folder_applet(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
) {
    let dialog = FileDialog::builder()
        .title("Add Folder Applet")
        .accept_label("Add")
        .modal(true)
        .build();

    let state = Rc::clone(state);
    let parent = window.clone();
    let window = window.clone();
    let drawing = drawing.clone();
    let gl_area = gl_area.clone();
    dialog.select_folder(
        Some(&parent),
        None::<&gio::Cancellable>,
        move |result| match result {
            Ok(file) => {
                let Some(path) = file.path() else {
                    tracing::warn!("selected applet folder did not have a local path");
                    return;
                };
                add_folder_applet(&state, &window, &drawing, &gl_area, path);
            }
            Err(error) => {
                tracing::debug!("folder applet selection cancelled or failed: {error:#}");
            }
        },
    );
}

fn add_folder_applet(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    path: PathBuf,
) {
    {
        let mut state = state.borrow_mut();
        let key = path.to_string_lossy().to_ascii_lowercase();
        if !state.config.applets.iter().any(|applet| {
            applet
                .path
                .as_ref()
                .is_some_and(|path| path.to_string_lossy().to_ascii_lowercase() == key)
        }) {
            state.config.applets.push(AppletConfig::folder(path));
        }
        save_runtime_config(&state);
        state.refresh_model();
        state.icons.clear();
    }
    ensure_icon_animation_if_needed(state, window, drawing, gl_area);
    sync_dock_window(state, window, drawing, gl_area, true);
    queue_gl_render_if_enabled(state, gl_area);
    drawing.queue_draw();
}

fn remove_folder_applet(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    item_key: &str,
    path: &Path,
) {
    {
        let mut state = state.borrow_mut();
        let target = path.to_string_lossy().to_ascii_lowercase();
        state.config.applets.retain(|applet| {
            !applet
                .path
                .as_ref()
                .is_some_and(|path| path.to_string_lossy().to_ascii_lowercase() == target)
        });
        state
            .config
            .item_order
            .retain(|key| !key.eq_ignore_ascii_case(item_key));
        state
            .config
            .custom_icons
            .retain(|key, _| !key.eq_ignore_ascii_case(item_key));
        save_runtime_config(&state);
        state.refresh_model();
        state.icons.clear();
    }
    ensure_icon_animation_if_needed(state, window, drawing, gl_area);
    sync_dock_window(state, window, drawing, gl_area, true);
    queue_gl_render_if_enabled(state, gl_area);
    drawing.queue_draw();
}

fn select_custom_icon(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    item_key: String,
) {
    let filter = image_file_filter();
    let filters = gio::ListStore::new::<FileFilter>();
    filters.append(&filter);
    let dialog = FileDialog::builder()
        .title("Select Icon")
        .accept_label("Select")
        .modal(true)
        .filters(&filters)
        .default_filter(&filter)
        .build();

    let state = Rc::clone(state);
    let parent = window.clone();
    let window = window.clone();
    let drawing = drawing.clone();
    let gl_area = gl_area.clone();
    dialog.open(
        Some(&parent),
        None::<&gio::Cancellable>,
        move |result| match result {
            Ok(file) => {
                let Some(path) = file.path() else {
                    tracing::warn!("selected icon file did not have a local path");
                    return;
                };
                set_custom_icon_value(
                    &state,
                    &window,
                    &drawing,
                    &gl_area,
                    &item_key,
                    path.to_string_lossy().to_string(),
                );
            }
            Err(error) => {
                tracing::debug!("icon selection cancelled or failed: {error:#}");
            }
        },
    );
}

fn image_file_filter() -> FileFilter {
    let filter = FileFilter::new();
    filter.set_name(Some("Image files"));
    filter.add_mime_type("image/png");
    filter.add_mime_type("image/jpeg");
    filter.add_mime_type("image/svg+xml");
    filter.add_mime_type("image/webp");
    filter.add_mime_type("image/x-xpixmap");
    filter.add_pattern("*.png");
    filter.add_pattern("*.jpg");
    filter.add_pattern("*.jpeg");
    filter.add_pattern("*.svg");
    filter.add_pattern("*.webp");
    filter.add_pattern("*.xpm");
    filter
}

fn take_context_menu(state: &mut Runtime) -> Option<Popover> {
    state.context_menu.take()
}

fn dismiss_popover_menu(menu: Option<Popover>) -> bool {
    let Some(menu) = menu else {
        return false;
    };
    menu.popdown();
    true
}

fn dismiss_context_menu(state: &Rc<RefCell<Runtime>>) -> bool {
    let menu = {
        let mut state = state.borrow_mut();
        take_context_menu(&mut state)
    };
    dismiss_popover_menu(menu)
}

fn save_runtime_config(state: &Runtime) {
    if let Err(error) = state.config.save_to_path(&state.config_path) {
        tracing::warn!(
            "could not save config {}: {error:#}",
            state.config_path.display()
        );
    }
}

fn wire_refresh(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
) {
    let refresh = state.borrow().config.dock.refresh_ms;
    let state = Rc::clone(state);
    let window = window.clone();
    let drawing = drawing.clone();
    let gl_area = gl_area.clone();
    glib::timeout_add_local(Duration::from_millis(refresh as u64), move || {
        let menu_open = {
            let mut state = state.borrow_mut();
            prune_finished_icon_slide(&mut state);
            prune_finished_icon_presence(&mut state);
            if state.context_menu.is_some() {
                true
            } else if state.drag.is_none()
                && state.separator_resize.is_none()
                && state.icon_slide.is_none()
                && state.icon_presence.is_none()
            {
                refresh_config_and_theme(&mut state);
                state.refresh_model();
                false
            } else {
                false
            }
        };
        if menu_open {
            return glib::ControlFlow::Continue;
        }
        ensure_icon_animation_if_needed(&state, &window, &drawing, &gl_area);
        sync_dock_window(&state, &window, &drawing, &gl_area, true);
        queue_gl_render_if_enabled(&state, &gl_area);
        drawing.queue_draw();
        glib::ControlFlow::Continue
    });
}

fn ensure_icon_animation_if_needed(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
) {
    if state.borrow().icon_presence.is_some() {
        ensure_icon_animation_tick(state, window, drawing, gl_area);
    }
}

fn wire_icon_theme_changes(state: &Rc<RefCell<Runtime>>, drawing: &DrawingArea, gl_area: &GLArea) {
    let Some(display) = gdk::Display::default() else {
        return;
    };
    let icon_theme = gtk::IconTheme::for_display(&display);
    tracing::info!("using icon theme {}", icon_theme.theme_name());

    let state = Rc::clone(state);
    let drawing = drawing.clone();
    let gl_area = gl_area.clone();
    icon_theme.connect_changed(move |icon_theme| {
        {
            let mut state = state.borrow_mut();
            state.icons.clear();
        }
        tracing::info!("reloaded icon theme {}", icon_theme.theme_name());
        queue_gl_render_if_enabled(&state, &gl_area);
        drawing.queue_draw();
    });
}

fn refresh_config_and_theme(state: &mut Runtime) {
    match Config::load_from_path(&state.config_path) {
        Ok(config) => {
            if config != state.config {
                tracing::info!("reloaded config {}", state.config_path.display());
                state.config = config;
                state.hidden = false;
                state.last_size = None;
                state.last_geometry = None;
                state.last_shape_size = None;
            }
        }
        Err(error) => {
            tracing::warn!(
                "could not reload config {}: {error:#}",
                state.config_path.display()
            );
            return;
        }
    }

    let (theme_id, theme_renderer, theme) = resolve_runtime_theme(state.composited, &state.config.theme);
    if theme != state.theme {
        tracing::info!("reloaded theme {} ({:?})", theme_id, theme_renderer);
        state.theme = theme;
        state.last_size = None;
        state.last_geometry = None;
        state.last_shape_size = None;
    }
}

fn activate_item(state: &Rc<RefCell<Runtime>>, index: usize, button: u32) {
    let item = {
        let state = state.borrow();
        state.model.items.get(index).cloned()
    };
    let Some(item) = item else {
        return;
    };
    if !item.is_application() {
        return;
    }

    if button == 2 {
        close_item_window(state, &item);
        return;
    }

    if item.primary_window().is_some() {
        if item.active {
            minimize_item_window(state, &item);
        } else {
            focus_item_window(state, &item);
        }
        return;
    }

    launch_item(state, &item);
}

fn activate_applet(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    item: &DockItem,
    index: usize,
    button: u32,
    x: f64,
    y: f64,
) {
    if button != 1 {
        return;
    }

    if item.is_downloads_applet() || item.is_trash_applet() || item.is_folder_applet() {
        show_applet_fan_for_item(state, window, drawing, gl_area, item, index, x, y);
        return;
    }
}

fn launch_item(state: &Rc<RefCell<Runtime>>, item: &DockItem) {
    if !item.is_application() {
        return;
    }

    if let Some(desktop_id) = item.desktop_id.as_deref()
        && let Err(error) = state.borrow().desktop_index.launch(desktop_id)
    {
        tracing::warn!("could not launch {desktop_id}: {error:#}");
    }
}

fn focus_item_window(state: &Rc<RefCell<Runtime>>, item: &DockItem) {
    let Some(window) = item.primary_window() else {
        return;
    };
    let mut state = state.borrow_mut();
    if let Some(backend) = state.backend.as_mut()
        && let Err(error) = backend.focus_window(window)
    {
        tracing::warn!("could not focus window {window}: {error:#}");
    }
}

fn minimize_item_window(state: &Rc<RefCell<Runtime>>, item: &DockItem) {
    let Some(window) = item.primary_window() else {
        return;
    };
    let mut state = state.borrow_mut();
    if let Some(backend) = state.backend.as_mut()
        && let Err(error) = backend.minimize_window(window)
    {
        tracing::warn!("could not minimize window {window}: {error:#}");
    }
}

fn close_item_window(state: &Rc<RefCell<Runtime>>, item: &DockItem) {
    let Some(window) = item.primary_window() else {
        return;
    };
    let mut state = state.borrow_mut();
    if let Some(backend) = state.backend.as_mut()
        && let Err(error) = backend.close_window(window)
    {
        tracing::warn!("could not close window {window}: {error:#}");
    }
}

fn close_item_application(state: &Rc<RefCell<Runtime>>, item: &DockItem) {
    if item.windows.is_empty() {
        return;
    }

    let windows = item
        .windows
        .iter()
        .map(|window| window.xid)
        .collect::<Vec<_>>();
    let mut state = state.borrow_mut();
    let Some(backend) = state.backend.as_mut() else {
        return;
    };

    for window in windows {
        if let Err(error) = backend.close_window(window) {
            tracing::warn!("could not close window {window}: {error:#}");
        }
    }
}

fn sync_dock_window(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    force_shape: bool,
) {
    let started = Instant::now();
    let mut size_changed = false;
    let size = {
        let mut state = state.borrow_mut();
        let size = state.desired_size();
        if state.last_size != Some(size) {
            state.last_size = Some(size);
            size_changed = true;
        }
        size
    };

    if size_changed {
        drawing.set_content_width(size.0);
        drawing.set_content_height(size.1);
        gl_area.set_size_request(size.0, size.1);
        window.set_default_size(size.0, size.1);
    }

    let mut state = state.borrow_mut();
    let update_reserved_space = state.separator_resize.is_none() && state.icon_presence.is_none();
    gl_area.set_visible(state.theme.renderer == RenderMode::Scene3d);
    move_dock(&mut state, update_reserved_space);
    let shape_label = current_label_index(&state);
    if force_shape
        || size_changed
        || state.icon_presence.is_some()
        || state.last_shape_size != Some(size)
        || state.last_shape_label != shape_label
    {
        shape_dock(&mut state);
        state.last_shape_size = Some(size);
        state.last_shape_label = shape_label;
    }
    log_slow("sync-window", started.elapsed());
}

fn current_label_index(state: &Runtime) -> Option<usize> {
    dock_layout_for_state(state, state.hover)
        .label
        .map(|label| label.item_index)
}

fn queue_gl_render_if_enabled(state: &Rc<RefCell<Runtime>>, gl_area: &GLArea) {
    if state.borrow().theme.renderer == RenderMode::Scene3d {
        gl_area.queue_render();
    }
}

fn shelf_layer_for(state: &Runtime) -> ShelfLayer {
    match state.theme.renderer {
        RenderMode::Scene3d if state.scene3d.fallback_reason().is_none() => ShelfLayer::None,
        RenderMode::Texture2d => ShelfLayer::Texture2d,
        _ => ShelfLayer::Procedural,
    }
}

fn move_dock(state: &mut Runtime, update_reserved_space: bool) {
    if state.dock_xid.is_none() {
        return;
    }
    let Some(geometry) = state.desired_geometry() else {
        return;
    };
    let geometry_changed = state.last_geometry != Some(geometry);
    let reserved_space_changed = state.last_reserved_geometry != Some(geometry);
    let sync_reserved_space = update_reserved_space && reserved_space_changed;
    if !geometry_changed && !sync_reserved_space {
        return;
    }
    let started = Instant::now();
    if let Some(backend) = state.backend.as_mut()
        && let Err(error) = backend.move_dock_window(geometry, sync_reserved_space)
    {
        tracing::warn!("could not move dock window: {error:#}");
        return;
    }
    state.last_geometry = Some(geometry);
    if sync_reserved_space {
        state.last_reserved_geometry = Some(geometry);
    }
    log_slow("move-dock", started.elapsed());
}

fn shape_dock(state: &mut Runtime) {
    if state.dock_xid.is_none() {
        return;
    }

    let size = state.desired_size();
    let layout = dock_layout_for_state(state, state.hover);
    let mut visual_regions = Renderer::visual_regions_for_layout(
        &state.model,
        &layout,
        &state.config.dock,
        &state.theme,
    );
    let mut input_regions = Renderer::input_regions_for_layout(&layout);
    if state.hidden {
        let reveal_strip = hidden_reveal_input_region(size, state.config.dock.edge);
        visual_regions.push(reveal_strip);
        input_regions.push(reveal_strip);
    }
    if let Some(icon_motion) = icon_motion_frame(state) {
        for motion_rect in icon_motion.rects {
            let rect = padded_rect(motion_rect.rect, 10.0);
            visual_regions.push(rect);
            input_regions.push(rect);
        }
    }
    if let Some(icon_presence) = icon_presence_frame(state) {
        for icon in icon_presence.current {
            let rect = padded_rect(icon.rect, 10.0);
            visual_regions.push(rect);
            input_regions.push(rect);
        }
        for ghost in icon_presence.ghosts {
            let rect = padded_rect(ghost.rect, 10.0);
            visual_regions.push(rect);
            input_regions.push(rect);
        }
    }
    let started = Instant::now();
    if let Some(backend) = state.backend.as_mut()
        && let Err(error) = backend.set_dock_shape(size, &visual_regions, &input_regions)
    {
        tracing::debug!("could not shape dock window: {error:#}");
    }
    log_slow("shape-dock", started.elapsed());
}

fn hidden_reveal_input_region(size: (i32, i32), edge: crate::config::DockEdge) -> Rect {
    let width = size.0.max(1) as f64;
    let height = size.1.max(1) as f64;
    let strip = EDGE_VISIBLE_PIXELS.max(1) as f64;
    match edge {
        crate::config::DockEdge::Bottom => Rect {
            x: 0.0,
            y: 0.0,
            width,
            height: strip,
        },
        crate::config::DockEdge::Top => Rect {
            x: 0.0,
            y: (height - strip).max(0.0),
            width,
            height: strip,
        },
        crate::config::DockEdge::Left => Rect {
            x: (width - strip).max(0.0),
            y: 0.0,
            width: strip,
            height,
        },
        crate::config::DockEdge::Right => Rect {
            x: 0.0,
            y: 0.0,
            width: strip,
            height,
        },
    }
}

fn padded_rect(rect: Rect, amount: f64) -> Rect {
    Rect {
        x: rect.x - amount,
        y: rect.y - amount,
        width: rect.width + amount * 2.0,
        height: rect.height + amount * 2.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::WindowInfo;

    fn separator_test_item(id: &str) -> DockItem {
        DockItem {
            id: id.to_string(),
            name: id.to_string(),
            desktop_id: Some(id.to_string()),
            startup_wm_class: None,
            icon_name: None,
            window_icon: None,
            pinned: true,
            windows: Vec::new(),
            active: false,
            urgent: false,
            badge: None,
        }
    }

    fn separator_test_layout() -> crate::layout::DockLayout {
        let model = DockModel {
            items: vec![
                separator_test_item("a"),
                separator_test_item("b"),
                DockItem::downloads_applet(),
                DockItem::trash_applet(),
            ],
        };
        crate::layout::compute_layout(
            &model,
            None,
            crate::layout::LayoutParams {
                icon_size: 64.0,
                zoom_strength: 0.72,
                gap: 8.0,
                reflection_height: 27.0,
                shelf_height: 24.0,
                side_margin: 64.0 * 0.82,
                shelf_horizon_ratio: 0.50,
                icon_floor_offset: 0.0,
                label_height: 24.0,
            },
        )
    }

    fn item_with_state(desktop_id: Option<&str>, active: bool, running: bool) -> DockItem {
        let windows = running
            .then_some(vec![WindowInfo {
                xid: 42,
                title: Some("Terminal".to_string()),
                class: Some("Xfce4-terminal".to_string()),
                pid: Some(1000),
                executable: Some("xfce4-terminal".to_string()),
                workspace: Some(0),
                icon: None,
                active,
                urgent: false,
                minimized: false,
            }])
            .unwrap_or_default();

        DockItem {
            id: desktop_id.unwrap_or("window:42").to_string(),
            name: "Terminal".to_string(),
            desktop_id: desktop_id.map(str::to_string),
            startup_wm_class: Some("Xfce4-terminal".to_string()),
            icon_name: Some("utilities-terminal".to_string()),
            window_icon: None,
            pinned: desktop_id.is_some(),
            windows,
            active,
            urgent: false,
            badge: None,
        }
    }

    #[test]
    fn application_menu_actions_include_app_and_window_controls() {
        let running_active = item_with_state(Some("xfce4-terminal.desktop"), true, true);
        let running_inactive = item_with_state(Some("xfce4-terminal.desktop"), false, true);
        let pinned_only = item_with_state(Some("xfce4-terminal.desktop"), false, false);
        let downloads = DockItem::downloads_applet();

        assert_eq!(
            application_context_actions(&running_active),
            vec![
                ApplicationContextAction::Launch,
                ApplicationContextAction::Minimize,
                ApplicationContextAction::Close,
            ]
        );
        assert_eq!(
            application_context_actions(&running_inactive),
            vec![
                ApplicationContextAction::Launch,
                ApplicationContextAction::Focus,
                ApplicationContextAction::Close,
            ]
        );
        assert_eq!(
            application_context_actions(&pinned_only),
            vec![ApplicationContextAction::Launch]
        );
        assert!(application_context_actions(&downloads).is_empty());
    }

    #[test]
    fn context_menu_height_expands_when_app_section_is_present() {
        assert_eq!(
            context_menu_height(0),
            CONTEXT_MENU_CHROME_HEIGHT
                + (CONTEXT_MENU_SETTINGS_COUNT as i32 * CONTEXT_MENU_ITEM_HEIGHT)
        );
        assert_eq!(
            context_menu_height(3),
            CONTEXT_MENU_CHROME_HEIGHT
                + (7 * CONTEXT_MENU_ITEM_HEIGHT)
                + CONTEXT_MENU_SEPARATOR_HEIGHT
        );
    }

    #[test]
    fn context_menu_anchor_rect_clamps_to_dock_width() {
        let rect = Rect {
            x: 260.4,
            y: 18.0,
            width: 30.0,
            height: 40.0,
        };
        assert_eq!(
            context_menu_anchor_rect(rect, 280),
            gdk::Rectangle::new(248, 18, 30, 40)
        );
    }

    #[test]
    fn applet_fan_more_label_counts_hidden_entries() {
        let source = AppletFanSource {
            directory_label: "Downloads".to_string(),
            empty_label: "No recent downloads".to_string(),
            open_target: None,
            entries: Vec::new(),
            total_entries: 9,
        };

        assert_eq!(applet_fan_more_label(&source, 7), "2 More in Downloads");
        assert_eq!(applet_fan_more_label(&source, 9), "Open Downloads");
    }

    #[test]
    fn applet_fan_reveal_opens_from_bottom() {
        let early_progress = 0.08;

        assert!(
            applet_fan_row_reveal(early_progress, 6, 7)
                > applet_fan_row_reveal(early_progress, 0, 7)
        );
    }

    #[test]
    fn recent_applet_entries_skip_hidden_files_and_report_total() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("a.txt"), b"a").expect("write a");
        fs::write(dir.path().join("b.pdf"), b"b").expect("write b");
        fs::write(dir.path().join(".hidden"), b"hidden").expect("write hidden");

        let entries = recent_applet_entries_from_dir(dir.path(), 1);

        assert_eq!(entries.total_entries, 2);
        assert_eq!(entries.entries.len(), 1);
    }

    #[test]
    fn startup_reveal_offset_moves_bottom_dock_from_below_screen() {
        let geometry = DockGeometry {
            x: 180,
            y: 920,
            width: 420,
            height: 74,
            edge: crate::config::DockEdge::Bottom,
            reserve_space: false,
            reserved_thickness: 0,
        };

        assert_eq!(
            startup_reveal_offset(&geometry, crate::config::DockEdge::Bottom, 0.0),
            geometry.height as i32 - EDGE_VISIBLE_PIXELS
        );
        assert_eq!(
            startup_reveal_offset(&geometry, crate::config::DockEdge::Bottom, 1.0),
            0
        );
    }

    #[test]
    fn hidden_bottom_dock_keeps_top_input_strip() {
        assert_eq!(
            hidden_reveal_input_region((320, 96), crate::config::DockEdge::Bottom),
            Rect {
                x: 0.0,
                y: 0.0,
                width: 320.0,
                height: EDGE_VISIBLE_PIXELS as f64,
            }
        );
    }

    #[test]
    fn separator_hit_test_starts_resize_mode() {
        let layout = separator_test_layout();
        let separator = layout
            .section(DockSectionKind::Separator)
            .expect("separator section");
        let point = Point {
            x: separator.rect.center_x(),
            y: separator.rect.y + separator.rect.height * 0.5,
        };

        let resize = begin_separator_resize(&layout, point, 480, 64).expect("resize mode");

        assert_eq!(resize.start_mouse_y, point.y);
        assert_eq!(resize.start_window_y, 480);
        assert_eq!(resize.start_icon_size, 64);
        assert_eq!(resize.current_icon_size, 64);
    }

    #[test]
    fn separator_resize_drag_delta_stays_anchored_when_window_moves() {
        let resize = SeparatorResize {
            start_mouse_y: 40.0,
            start_window_y: 900,
            start_icon_size: 64,
            current_icon_size: 64,
        };

        assert_eq!(separator_resize_drag_delta(&resize, 890, 10.0), 0.0);
        assert_eq!(separator_resize_drag_delta(&resize, 880, 18.0), -2.0);
    }

    #[test]
    fn dragging_upward_increases_icon_size() {
        assert!(resize_icon_size_for_drag(64, -7.5) > 64);
    }

    #[test]
    fn dragging_downward_decreases_icon_size() {
        assert!(resize_icon_size_for_drag(64, 7.5) < 64);
    }

    #[test]
    fn separator_resize_clamps_icon_size() {
        assert_eq!(
            resize_icon_size_for_drag(64, -500.0),
            SEPARATOR_RESIZE_MAX_ICON_SIZE
        );
        assert_eq!(
            resize_icon_size_for_drag(64, 500.0),
            SEPARATOR_RESIZE_MIN_ICON_SIZE
        );
    }

    #[test]
    fn small_drag_deltas_keep_same_effective_icon_size() {
        assert_eq!(resize_icon_size_for_drag(64, 0.8), 64);
        assert_eq!(resize_icon_size_for_drag(64, -0.8), 64);
    }

    #[test]
    fn separator_does_not_hit_icon_magnify_path() {
        let layout = separator_test_layout();
        let separator = layout
            .section(DockSectionKind::Separator)
            .expect("separator section");
        let point = Point {
            x: separator.rect.center_x(),
            y: separator.rect.y + separator.rect.height * 0.5,
        };

        assert!(separator_hit_test_in_layout(&layout, point));
        assert!(layout.hit_test(point).is_none());
    }
}

fn log_slow(operation: &'static str, elapsed: Duration) {
    if elapsed >= SLOW_UI_OP {
        tracing::debug!(
            target: "osdockx::perf",
            operation,
            elapsed_ms = elapsed.as_secs_f64() * 1000.0,
            "slow UI operation"
        );
    } else {
        tracing::trace!(
            target: "osdockx::perf",
            operation,
            elapsed_ms = elapsed.as_secs_f64() * 1000.0,
            "UI operation"
        );
    }
}
