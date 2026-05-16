use crate::backend::x11::X11Backend;
use crate::backend::{DockGeometry, PlatformBackend};
use crate::config::{Config, RenderMode};
use crate::desktop::DesktopIndex;
use crate::layout::{Point, Rect};
use crate::model::{DockItem, DockModel, DockSectionKind};
use crate::renderer::{
    IconCache, IconMotionFrame, IconMotionRect, RenderFrame, Renderer, ShelfLayer,
};
use crate::scene3d::Scene3dRenderer;
use crate::shelf::ShelfRenderer;
use crate::theme::Theme;
use crate::theme_pack::ThemePack;
use directories::UserDirs;
use gdk_x11::X11Surface;
use gtk::gio;
use gtk::gio::prelude::FileExt;
use gtk::glib::{self, Propagation, object::Cast};
use gtk::pango::EllipsizeMode;
use gtk::prelude::*;
use gtk::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, DrawingArea,
    EventControllerMotion, FileDialog, FileFilter, GLArea, GestureClick, GestureDrag, Image,
    Label,
    Orientation, Overlay, Popover, PositionType, gdk,
};
use std::cell::RefCell;
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
const CONTEXT_MENU_SETTINGS_COUNT: usize = 3;
const CONTEXT_MENU_SEPARATOR_HEIGHT: i32 = 12;
const CONTEXT_MENU_CHROME_HEIGHT: i32 = 12;
const CONTEXT_MENU_GAP: f64 = 18.0;
const DOWNLOADS_STACK_WIDTH: i32 = 268;
const DOWNLOADS_STACK_MAX_ITEMS: usize = 7;
const DOWNLOADS_STACK_GAP: f64 = 22.0;
const ICON_DRAG_THRESHOLD: f64 = 6.0;
const ICON_SLIDE_DURATION: Duration = Duration::from_millis(150);
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
struct StartupReveal {
    started: Instant,
}

#[derive(Debug, Clone)]
struct DownloadStackEntry {
    name: String,
    path: PathBuf,
    icon_name: String,
    modified: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApplicationContextAction {
    Launch,
    Focus,
    Minimize,
    Close,
}

impl Runtime {
    fn refresh_model(&mut self) {
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
        self.model = DockModel::from_sources(&self.config.pinned, &self.desktop_index, windows);
        self.model.apply_order(&self.config.item_order);
    }

    fn desired_size(&self) -> (i32, i32) {
        Renderer::desired_size(&self.model, &self.config.dock, &self.theme, self.hover)
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
            apply_edge_offset(
                &mut geometry,
                self.config.dock.edge,
                hidden_offset,
            );
        }

        if let Some(startup_reveal) = self.startup_reveal.as_ref() {
            let startup_offset = startup_reveal_offset(
                &geometry,
                self.config.dock.edge,
                startup_reveal.progress(),
            );
            apply_edge_offset(
                &mut geometry,
                self.config.dock.edge,
                startup_offset,
            );
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

fn apply_edge_offset(
    geometry: &mut DockGeometry,
    edge: crate::config::DockEdge,
    distance: i32,
) {
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
    let layout = Renderer::layout_for(&state.model, &state.config.dock, &state.theme, None);
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
    (start_icon_size as i32 + size_delta)
        .clamp(
            SEPARATOR_RESIZE_MIN_ICON_SIZE as i32,
            SEPARATOR_RESIZE_MAX_ICON_SIZE as i32,
        ) as u32
}

fn set_separator_resize_cursor(drawing: &DrawingArea, enabled: bool) {
    drawing.set_cursor_from_name(enabled.then_some(SEPARATOR_RESIZE_CURSOR));
}

fn build_ui(app: &Application) -> anyhow::Result<()> {
    let (config, config_path) = Config::load_or_create()?;
    tracing::info!("using config {}", config_path.display());
    if let Err(error) = ThemePack::export_builtin_theme_packs() {
        tracing::warn!("could not export built-in theme packs: {error:#}");
    }

    install_css();

    let composited = gdk::Display::default().is_some_and(|display| display.is_composited());
    let (theme_id, theme_renderer, theme) = resolve_runtime_theme(&config, composited);
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

fn resolve_runtime_theme(config: &Config, composited: bool) -> (String, RenderMode, Theme) {
    let theme_pack = ThemePack::load(&config.theme);
    let id = theme_pack.id.clone();
    let renderer = theme_pack.renderer;
    if composited {
        (id, renderer, theme_pack.theme)
    } else {
        (id, renderer, theme_pack.theme.opaque_fallback())
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
        .osdock-downloads-stack {
            background-image: linear-gradient(
                to bottom,
                alpha(#26282d, 0.98),
                alpha(#0f1116, 0.96)
            );
            border: 1px solid alpha(#ffffff, 0.16);
            border-radius: 12px;
            box-shadow: 0 12px 28px alpha(#000000, 0.56);
            padding: 10px;
        }
        .osdock-stack-title {
            color: #eef1f7;
            font-size: 12px;
            font-weight: 700;
            letter-spacing: 0.04em;
            text-shadow: 0 1px alpha(#000000, 0.60);
            margin: 0 2px 4px 2px;
        }
        .osdock-stack-empty {
            color: alpha(#e4e8ef, 0.72);
            font-size: 12px;
            margin: 4px 6px 6px 6px;
        }
        button.osdock-stack-item {
            min-height: 38px;
            min-width: 248px;
            padding: 0;
            margin: 0;
            border: none;
            border-radius: 8px;
            background: transparent;
            box-shadow: none;
            color: #f2f4f8;
        }
        button.osdock-stack-item:hover,
        button.osdock-stack-item:focus {
            background-image: linear-gradient(
                to bottom,
                alpha(#a3b0c8, 0.28),
                alpha(#6f7d97, 0.20)
            );
        }
        .osdock-stack-row {
            padding: 6px 10px;
        }
        .osdock-stack-icon {
            margin-right: 10px;
        }
        button.osdock-stack-item label {
            color: #edf1f8;
            font-size: 12px;
            font-weight: 500;
            text-shadow: 0 1px alpha(#000000, 0.58);
        }
        .osdock-menu-box {
            padding: 1px 0;
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
        let config = state.config.dock.clone();
        let theme = state.theme.clone();
        let layout = Renderer::layout_for(&model, &config, &theme, hover);
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
                autohide = state.config.dock.autohide
                    && state.context_menu.is_none()
                    && !resizing;
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
                glib::timeout_add_local_once(Duration::from_millis(delay as u64), move || {
                    let mut state = state.borrow_mut();
                    if state.hover.is_none() {
                        state.hidden = true;
                        move_dock(&mut state, true);
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
                Renderer::icon_hit_test(
                    &state.model,
                    &state.config.dock,
                    &state.theme,
                    Point { x, y },
                )
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
                    if item.is_application() {
                        show_context_menu(&state, &window, &drawing, &gl_area, index, x, y);
                    } else {
                        let dismissed = dismiss_context_menu(&state);
                        if dismissed {
                            sync_dock_window(&state, &window, &drawing, &gl_area, true);
                        }
                    }
                } else if button == 1 || button == 2 {
                    let dismissed = dismiss_context_menu(&state);
                    if dismissed {
                        sync_dock_window(&state, &window, &drawing, &gl_area, true);
                    }
                    if item.is_application() {
                        activate_item(&state, index, button);
                    } else {
                        activate_applet(&state, &window, &drawing, &gl_area, &item, index, button, x, y);
                    }
                }
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
                Renderer::icon_hit_test(&state.model, &state.config.dock, &state.theme, point)
                    .and_then(|index| {
                        let item = state.model.items.get(index)?;
                        if !item.is_application() {
                            return None;
                        }
                        let rect = Renderer::layout_for(
                            &state.model,
                            &state.config.dock,
                            &state.theme,
                            None,
                        )
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
                let layout = Renderer::layout_for(
                    &state.model,
                    &state.config.dock,
                    &state.theme,
                    None,
                );
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
    Renderer::layout_for(&state.model, &state.config.dock, &state.theme, None)
        .icons
        .iter()
        .filter(|icon| {
            state
                .model
                .items
                .get(icon.item_index)
                .is_some_and(|item| item.is_application())
        })
        .min_by(|left, right| {
            let left_distance = (left.rect.center_x() - point.x).abs();
            let right_distance = (right.rect.center_x() - point.x).abs();
            left_distance.total_cmp(&right_distance)
        })
        .map(|icon| icon.item_index)
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

    let layout = Renderer::layout_for(&state.model, &state.config.dock, &state.theme, None);
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
    Renderer::layout_for(&state.model, &state.config.dock, &state.theme, None)
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
            state.drag.is_some() || state.icon_slide.is_some()
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
        let layout = Renderer::layout_for(&state.model, &state.config.dock, &state.theme, None);
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
    if !item.is_application() {
        return;
    }
    let app_actions = application_context_actions(&item);
    let item_key = item.config_key();
    let pinned = item.pinned;
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
    let select = context_menu_button("Select Icon", false);
    let default_icon = context_menu_button("Set Default Icon", false);
    menu.append(&keep);
    menu.append(&select);
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
        default_icon.connect_clicked(move |_| {
            dismiss_context_menu(&state);
            reset_custom_icon(&state, &window, &drawing, &gl_area, &item_key);
        });
    }

    let popover = Popover::new();
    popover.add_css_class("osdock-context-popover");
    popover.set_autohide(true);
    popover.set_has_arrow(false);
    popover.set_position(PositionType::Top);
    popover.set_offset(0, -(CONTEXT_MENU_GAP.round() as i32));
    popover.set_pointing_to(Some(&context_menu_anchor_rect(anchor, dock_width)));
    popover.set_child(Some(&menu));
    popover.set_parent(drawing);

    present_runtime_popover(state, window, drawing, gl_area, &popover);
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
    CONTEXT_MENU_CHROME_HEIGHT
        + ((app_action_count + CONTEXT_MENU_SETTINGS_COUNT) as i32 * CONTEXT_MENU_ITEM_HEIGHT)
        + separator_count * CONTEXT_MENU_SEPARATOR_HEIGHT
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

fn context_menu_separator() -> GtkBox {
    let separator = GtkBox::new(Orientation::Horizontal, 0);
    separator.add_css_class("osdock-menu-separator");
    separator.set_hexpand(true);
    separator.set_halign(Align::Fill);
    separator.set_size_request(-1, CONTEXT_MENU_SEPARATOR_HEIGHT);
    separator
}

fn downloads_stack_button(label: &str, icon_name: &str) -> Button {
    let button = Button::builder().has_frame(false).build();
    button.add_css_class("osdock-stack-item");

    let row = GtkBox::new(Orientation::Horizontal, 0);
    row.add_css_class("osdock-stack-row");
    let icon = Image::from_icon_name(icon_name);
    icon.add_css_class("osdock-stack-icon");
    icon.set_pixel_size(24);
    let text = Label::new(Some(label));
    text.set_xalign(0.0);
    text.set_hexpand(true);
    text.set_halign(Align::Start);
    text.set_ellipsize(EllipsizeMode::Middle);
    text.set_max_width_chars(26);
    row.append(&icon);
    row.append(&text);
    button.set_child(Some(&row));
    button
}

fn present_runtime_popover(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    popover: &Popover,
) {
    {
        let state = Rc::clone(state);
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        let popover_for_close = popover.clone();
        popover.connect_closed(move |_| {
            if let Ok(mut runtime) = state.try_borrow_mut()
                && runtime
                    .context_menu
                    .as_ref()
                    .is_some_and(|current| current == &popover_for_close)
            {
                runtime.context_menu = None;
                runtime.hover = None;
            }

            let state = Rc::clone(&state);
            let drawing = drawing.clone();
            let gl_area = gl_area.clone();
            let popover_for_cleanup = popover_for_close.clone();
            glib::idle_add_local_once(move || {
                if let Ok(mut runtime) = state.try_borrow_mut()
                    && runtime
                        .context_menu
                        .as_ref()
                        .is_some_and(|current| current == &popover_for_cleanup)
                {
                    runtime.context_menu = None;
                    runtime.hover = None;
                }

                popover_for_cleanup.set_child(None::<&gtk::Widget>);
                if popover_for_cleanup.parent().is_some() {
                    popover_for_cleanup.unparent();
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

fn show_downloads_stack(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    index: usize,
    x: f64,
    y: f64,
) {
    let (icon_rect, dock_width) = {
        let state = state.borrow();
        let layout = Renderer::layout_for(&state.model, &state.config.dock, &state.theme, None);
        let icon_rect = layout
            .icons
            .iter()
            .find(|icon| icon.item_index == index)
            .map(|icon| icon.rect);
        (icon_rect, layout.size.0)
    };
    let downloads_dir = downloads_directory();
    let entries = downloads_dir
        .as_deref()
        .map(|dir| recent_download_entries_from_dir(dir, DOWNLOADS_STACK_MAX_ITEMS))
        .unwrap_or_default();
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

    let stack = GtkBox::new(Orientation::Vertical, 6);
    stack.add_css_class("osdock-downloads-stack");
    stack.set_size_request(DOWNLOADS_STACK_WIDTH, -1);

    let title = Label::new(Some("Downloads"));
    title.add_css_class("osdock-stack-title");
    title.set_xalign(0.0);
    stack.append(&title);

    if entries.is_empty() {
        let empty = Label::new(Some("No recent downloads"));
        empty.add_css_class("osdock-stack-empty");
        empty.set_xalign(0.0);
        stack.append(&empty);
    } else {
        for entry in entries {
            let button = downloads_stack_button(&entry.name, &entry.icon_name);
            {
                let state = Rc::clone(state);
                let window = window.clone();
                let drawing = drawing.clone();
                let gl_area = gl_area.clone();
                let path = entry.path.clone();
                button.connect_clicked(move |_| {
                    dismiss_context_menu(&state);
                    open_path_in_default_app(&path);
                    sync_dock_window(&state, &window, &drawing, &gl_area, true);
                    queue_gl_render_if_enabled(&state, &gl_area);
                    drawing.queue_draw();
                });
            }
            stack.append(&button);
        }
    }

    if let Some(downloads_dir) = downloads_dir {
        let open_folder = downloads_stack_button("Open Downloads Folder", "folder");
        {
            let state = Rc::clone(state);
            let window = window.clone();
            let drawing = drawing.clone();
            let gl_area = gl_area.clone();
            open_folder.connect_clicked(move |_| {
                dismiss_context_menu(&state);
                open_path_in_default_app(&downloads_dir);
                sync_dock_window(&state, &window, &drawing, &gl_area, true);
                queue_gl_render_if_enabled(&state, &gl_area);
                drawing.queue_draw();
            });
        }
        stack.append(&open_folder);
    }

    let popover = Popover::new();
    popover.add_css_class("osdock-stack-popover");
    popover.set_autohide(true);
    popover.set_has_arrow(false);
    popover.set_position(PositionType::Top);
    popover.set_offset(0, -(DOWNLOADS_STACK_GAP.round() as i32));
    popover.set_pointing_to(Some(&context_menu_anchor_rect(anchor, dock_width)));
    popover.set_child(Some(&stack));
    popover.set_parent(drawing);

    present_runtime_popover(state, window, drawing, gl_area, &popover);
}

fn downloads_directory() -> Option<PathBuf> {
    UserDirs::new()
        .and_then(|user_dirs| user_dirs.download_dir().map(PathBuf::from))
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join("Downloads")))
}

fn recent_download_entries_from_dir(dir: &Path, limit: usize) -> Vec<DownloadStackEntry> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
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
            Some(DownloadStackEntry {
                name,
                path: path.clone(),
                icon_name: downloads_entry_icon_name(&path, file_type.is_dir()).to_string(),
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
    files.truncate(limit);
    files
}

fn downloads_entry_icon_name(path: &Path, is_directory: bool) -> &'static str {
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
        Some("zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar") => {
            "package-x-generic"
        }
        Some("mp3" | "wav" | "flac" | "ogg") => "audio-x-generic",
        Some("mp4" | "mkv" | "webm" | "mov") => "video-x-generic",
        _ => "text-x-generic",
    }
}

fn open_path_in_default_app(path: &Path) {
    let file = gio::File::for_path(path);
    let uri = file.uri();
    open_uri(uri.as_str());
}

fn open_uri(uri: &str) {
    if let Err(error) = gio::AppInfo::launch_default_for_uri(uri, None::<&gio::AppLaunchContext>)
    {
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
                {
                    let mut state = state.borrow_mut();
                    state
                        .config
                        .custom_icons
                        .insert(item_key, path.to_string_lossy().to_string());
                    save_runtime_config(&state);
                    state.icons.clear();
                }
                sync_dock_window(&state, &window, &drawing, &gl_area, true);
                queue_gl_render_if_enabled(&state, &gl_area);
                drawing.queue_draw();
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
        {
            let mut state = state.borrow_mut();
            prune_finished_icon_slide(&mut state);
            if state.drag.is_none()
                && state.separator_resize.is_none()
                && state.icon_slide.is_none()
            {
                refresh_config_and_theme(&mut state);
                state.refresh_model();
            }
        }
        sync_dock_window(&state, &window, &drawing, &gl_area, true);
        queue_gl_render_if_enabled(&state, &gl_area);
        drawing.queue_draw();
        glib::ControlFlow::Continue
    });
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

    let (theme_id, theme_renderer, theme) = resolve_runtime_theme(&state.config, state.composited);
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

    if item.is_downloads_applet() {
        show_downloads_stack(state, window, drawing, gl_area, index, x, y);
        return;
    }

    if item.is_trash_applet() {
        open_uri("trash:///");
        sync_dock_window(state, window, drawing, gl_area, true);
        queue_gl_render_if_enabled(state, gl_area);
        drawing.queue_draw();
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

    let windows = item.windows.iter().map(|window| window.xid).collect::<Vec<_>>();
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
    let update_reserved_space = state.separator_resize.is_none();
    gl_area.set_visible(state.theme.renderer == RenderMode::Scene3d);
    move_dock(&mut state, update_reserved_space);
    let shape_label = current_label_index(&state);
    if force_shape
        || size_changed
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
    Renderer::layout_for(&state.model, &state.config.dock, &state.theme, state.hover)
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
    let mut visual_regions =
        Renderer::visual_regions(&state.model, &state.config.dock, &state.theme, state.hover);
    let mut input_regions =
        Renderer::input_regions(&state.model, &state.config.dock, &state.theme, state.hover);
    if let Some(icon_motion) = icon_motion_frame(state) {
        for motion_rect in icon_motion.rects {
            let rect = padded_rect(motion_rect.rect, 10.0);
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
                + (6 * CONTEXT_MENU_ITEM_HEIGHT)
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
        assert_eq!(resize_icon_size_for_drag(64, -500.0), SEPARATOR_RESIZE_MAX_ICON_SIZE);
        assert_eq!(resize_icon_size_for_drag(64, 500.0), SEPARATOR_RESIZE_MIN_ICON_SIZE);
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
