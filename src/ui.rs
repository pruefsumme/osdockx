mod actions;
mod applet_fan;
mod customizer;
mod hover;
mod menus;
mod pickers;
mod state;

use self::actions::{
    hide_application_from_dock, pin_application, remove_folder_applet, reset_custom_icon,
    select_custom_icon, select_folder_applet, set_custom_icon_value, toggle_keep_in_dock,
};
use self::applet_fan::{
    AppletFanDrawFrame, AppletFanHitRegion, AppletFanSource, applet_fan_reveal_progress,
    applet_fan_size, applet_fan_source, draw_applet_fan, run_applet_fan_action,
    start_applet_fan_reveal_tick,
};
#[cfg(test)]
use self::applet_fan::{
    applet_fan_more_label, applet_fan_row_reveal, recent_applet_entries_from_dir,
};
use self::customizer::show_customizer_debug_window;
#[cfg(test)]
use self::customizer::{
    CustomizerColorField, CustomizerSliderField, rgba_from_config_color, rgba_to_config_color,
};
use self::hover::show_hover_settings_menu;
#[cfg(test)]
use self::menus::{application_context_actions, context_menu_height};
use self::menus::{
    context_menu_anchor_rect, context_menu_icon_button, context_menu_separator, menu_height,
    present_runtime_popover, show_context_menu, show_dock_context_menu,
};
use self::pickers::{show_add_application_menu, show_theme_icon_menu};
use self::state::{
    DockSizeTransition, IconDrag, IconPresenceGhost, IconPresenceTransition, IconSlide,
    IndicatorAnimation, IndicatorVisual, SeparatorResize, StartupReveal,
};

use crate::autostart;
use crate::backend::x11::X11Backend;
use crate::backend::{DockGeometry, PlatformBackend};
use crate::config::{Config, DockConfig, RenderMode};
use crate::desktop::DesktopIndex;
use crate::layout::{DockLayout, Point, Rect, separator_hover_rect};
use crate::model::{DockItem, DockModel};
use crate::renderer::{
    GhostIcon, IconCache, IconMotionFrame, IconMotionRect, IconPresenceFrame, IconPresenceRect,
    IndicatorAnimationFrame, IndicatorAnimationState, RenderFrame, Renderer, ShelfLayer,
};
use crate::scene3d::Scene3dRenderer;
use crate::shelf::ShelfRenderer;
use crate::theme::Theme;
use crate::theme_pack::ThemePack;
use gdk_x11::X11Surface;
use gtk::cairo::Context;
use gtk::gdk_pixbuf::Pixbuf;
use gtk::gio;
use gtk::glib::{self, Propagation, object::Cast};
use gtk::prelude::*;
use gtk::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, CheckButton, DrawingArea,
    EventControllerMotion, GLArea, GestureClick, GestureDrag, Label, Orientation, Overlay, Popover,
    PositionType, gdk,
};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
#[cfg(test)]
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

const APP_ID: &str = autostart::APP_ID;
const EDGE_VISIBLE_PIXELS: i32 = 4;
const SLOW_UI_OP: Duration = Duration::from_millis(4);
const CONTEXT_MENU_WIDTH: i32 = 198;
const CONTEXT_MENU_ITEM_HEIGHT: i32 = 24;
const CONTEXT_MENU_SETTINGS_COUNT: usize = 5;
const CONTEXT_MENU_SEPARATOR_HEIGHT: i32 = 12;
const CONTEXT_MENU_CHROME_HEIGHT: i32 = 12;
const CONTEXT_MENU_GAP: f64 = 18.0;
const DOCK_CONTEXT_MENU_WIDTH: i32 = 228;
const DOCK_CONTEXT_MENU_ACTIONS: usize = 13;
const HOVER_SETTINGS_MENU_WIDTH: i32 = 272;
const CUSTOMIZER_WIDTH: i32 = 420;
const CUSTOMIZER_HEIGHT: i32 = 620;
const CUSTOMIZER_PREVIEW_DEBOUNCE: Duration = Duration::from_millis(120);
const ADD_APPLICATION_MENU_WIDTH: i32 = 292;
const ADD_APPLICATION_MENU_VISIBLE_ROWS: usize = 12;
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
const ICON_PRESENCE_DURATION: Duration = Duration::from_millis(460);
const DOCK_SIZE_TRANSITION_DURATION: Duration = Duration::from_millis(260);
const INDICATOR_ANIMATION_DURATION: Duration = Duration::from_millis(180);
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
    customizer_open: bool,
    drag: Option<IconDrag>,
    separator_resize: Option<SeparatorResize>,
    dock_size_transition: Option<DockSizeTransition>,
    icon_slide: Option<IconSlide>,
    icon_presence: Option<IconPresenceTransition>,
    indicator_animations: HashMap<String, IndicatorAnimation>,
    startup_reveal: Option<StartupReveal>,
    animation_tick_running: bool,
    startup_reveal_tick_running: bool,
    suppress_next_left_click: bool,
}

#[derive(Clone, Copy)]
struct DockUiHandles<'a> {
    window: &'a ApplicationWindow,
    drawing: &'a DrawingArea,
    gl_area: &'a GLArea,
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
    CustomizerDebug,
    ToggleAutohide,
    ToggleReserveSpace,
    ToggleAutostart,
    ReloadTheme,
    ResetDefaults,
    ResetCustomIcons,
    OpenConfigFolder,
}

impl Runtime {
    fn refresh_model(&mut self) -> bool {
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
            &self.config.hidden,
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
        let indicator_animations =
            build_indicator_animations(&previous_model, &next_model, &self.indicator_animations);
        let model_changed = previous_model != next_model;

        self.model = next_model;
        if icon_presence.is_some() {
            self.hover = None;
        }
        self.icon_presence = icon_presence;
        self.indicator_animations = indicator_animations;
        model_changed
    }

    fn desired_size(&self) -> (i32, i32) {
        icon_presence_layout(self)
            .map(|layout| layout.size)
            .unwrap_or_else(|| {
                let config = rendered_dock_config(self);
                Renderer::desired_size(&self.model, &config, &self.theme, self.hover)
            })
    }

    fn reserved_thickness(&self) -> u32 {
        let config = rendered_dock_config(self);
        Renderer::reserved_thickness(&self.model, &config, &self.theme)
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

fn dock_layout_for_state(state: &Runtime, hover: Option<Point>) -> DockLayout {
    icon_presence_layout(state).unwrap_or_else(|| {
        let config = rendered_dock_config(state);
        Renderer::layout_for(&state.model, &config, &state.theme, hover)
    })
}

fn rendered_dock_config(state: &Runtime) -> DockConfig {
    let mut config = state.config.dock.clone();
    if let Some(resize) = state.separator_resize.as_ref() {
        config.icon_size = resize.render_icon_size.round() as u32;
    } else if let Some(transition) = active_dock_size_transition(state) {
        config.icon_size = transition_icon_size(transition).round() as u32;
    }
    config
}

fn active_dock_size_transition(state: &Runtime) -> Option<&DockSizeTransition> {
    state
        .dock_size_transition
        .as_ref()
        .filter(|transition| transition.started.elapsed() < transition.duration)
}

fn transition_icon_size(transition: &DockSizeTransition) -> f64 {
    interpolate(
        transition.from_icon_size,
        transition.to_icon_size,
        ease_in_out_cubic(
            (transition.started.elapsed().as_secs_f64() / transition.duration.as_secs_f64())
                .clamp(0.0, 1.0),
        ),
    )
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
            (!contains_item_key(&next_keys, &item_key))
                .then_some(item)
                .and_then(|item| {
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

fn interpolate_presence_layout(transition: &IconPresenceTransition, progress: f64) -> DockLayout {
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
    layout.shelf = interpolate_rect(
        transition.from_layout.shelf,
        transition.to_layout.shelf,
        progress,
    );

    for (section, from_section) in layout
        .sections
        .iter_mut()
        .zip(&transition.from_layout.sections)
    {
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
        .separator
        .map(|separator| separator_hover_rect(separator.rect).contains(point))
        .unwrap_or(false)
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
        start_window_y,
        start_icon_size,
        target_icon_size: start_icon_size,
        render_icon_size: start_icon_size as f64,
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
        customizer_open: false,
        drag: None,
        separator_resize: None,
        dock_size_transition: None,
        icon_slide: None,
        icon_presence: None,
        indicator_animations: HashMap::new(),
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
            let config = rendered_dock_config(&state);
            let custom_icons = state.config.custom_icons.clone();
            let theme = state.theme.clone();
            let layout = dock_layout_for_state(&state, hover);
            let icon_motion = icon_motion_frame(&state);
            let icon_presence = icon_presence_frame(&state);
            let indicator_animation = indicator_animation_frame(&state);
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
                    layout: Some(&layout),
                    shelf_layer,
                    icon_motion: icon_motion.as_ref(),
                    icon_presence: icon_presence.as_ref(),
                    indicator_animation: indicator_animation.as_ref(),
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
    sync_or_prompt_autostart(&state, &window);
    queue_gl_render_if_enabled(&state, &gl_area);
    Ok(())
}

fn sync_or_prompt_autostart(state: &Rc<RefCell<Runtime>>, parent: &ApplicationWindow) {
    if state.borrow().config.startup.prompt_seen {
        apply_autostart_enabled(state.borrow().config.startup.autostart);
        return;
    }

    let state = Rc::clone(state);
    let parent = parent.clone();
    glib::idle_add_local_once(move || show_startup_prompt(&state, &parent));
}

fn show_startup_prompt(state: &Rc<RefCell<Runtime>>, parent: &ApplicationWindow) {
    if state.borrow().config.startup.prompt_seen {
        return;
    }

    let prompt = ApplicationWindow::builder()
        .title("OSDockX Startup")
        .transient_for(parent)
        .modal(true)
        .default_width(340)
        .resizable(false)
        .build();
    if let Some(app) = parent.application() {
        prompt.set_application(Some(&app));
    }

    let shell = GtkBox::new(Orientation::Vertical, 10);
    shell.add_css_class("osdock-context-menu");
    shell.set_margin_top(10);
    shell.set_margin_bottom(10);
    shell.set_margin_start(10);
    shell.set_margin_end(10);

    let title = Label::new(Some("Start OSDockX at Login"));
    title.add_css_class("osdock-menu-title");
    title.set_xalign(0.0);
    shell.append(&title);

    let check = CheckButton::with_label("Start at Login");
    check.set_active(state.borrow().config.startup.autostart);
    check.set_margin_start(8);
    check.set_margin_end(8);
    shell.append(&check);

    let footer = GtkBox::new(Orientation::Horizontal, 8);
    footer.set_halign(Align::End);
    let continue_button = Button::with_label("Continue");
    footer.append(&continue_button);
    shell.append(&footer);
    prompt.set_child(Some(&shell));

    {
        let prompt = prompt.clone();
        continue_button.connect_clicked(move |_| {
            prompt.close();
        });
    }
    {
        let state = Rc::clone(state);
        let check = check.clone();
        let handled = Rc::new(Cell::new(false));
        prompt.connect_close_request(move |_| {
            if !handled.replace(true) {
                finish_startup_prompt(&state, check.is_active());
            }
            Propagation::Proceed
        });
    }

    prompt.present();
}

fn finish_startup_prompt(state: &Rc<RefCell<Runtime>>, enabled: bool) {
    {
        let mut state = state.borrow_mut();
        state.config.startup.autostart = enabled;
        state.config.startup.prompt_seen = true;
        save_runtime_config(&state);
    }
    apply_autostart_enabled(enabled);
}

fn apply_autostart_enabled(enabled: bool) {
    if let Err(error) = autostart::set_enabled(enabled) {
        tracing::warn!("could not update autostart desktop file: {error:#}");
    }
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
        .osdock-theme-icon-picker {
            padding: 8px;
        }
        .osdock-theme-icon-picker .osdock-menu-title {
            margin: 2px 8px;
        }
        flowbox.osdock-theme-icon-grid {
            background: transparent;
            padding: 4px 8px 10px 8px;
        }
        flowbox.osdock-theme-icon-grid flowboxchild {
            padding: 0;
        }
        button.osdock-icon-choice {
            min-width: 124px;
            min-height: 104px;
            padding: 8px;
            border-radius: 6px;
            border: 1px solid alpha(#ffffff, 0.08);
            background: alpha(#ffffff, 0.06);
            box-shadow: none;
            color: #f3f6fb;
        }
        button.osdock-icon-choice:hover,
        button.osdock-icon-choice:focus {
            border-color: alpha(#91c4ff, 0.55);
            background-image: linear-gradient(to bottom, #5aa7ff, #1f68d7);
            color: #ffffff;
        }
        button.osdock-icon-choice image {
            color: #eef4ff;
        }
        button.osdock-icon-choice label {
            color: #edf2f8;
            font-size: 10px;
            font-weight: 600;
            text-shadow: 0 1px alpha(#000000, 0.55);
        }
        button.osdock-icon-choice:hover label,
        button.osdock-icon-choice:focus label {
            color: #ffffff;
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
            let hover_changed;
            let label_changed;
            let hidden_changed;
            {
                let mut state = state.borrow_mut();
                resizing = state.separator_resize.is_some();
                if resizing {
                    separator_hover = false;
                    hover_changed = false;
                    label_changed = false;
                    hidden_changed = false;
                    state.hover = None;
                } else {
                    let previous_hover = state.hover;
                    separator_hover = separator_hit_test(&state, point);
                    let next_label = if state.context_menu.is_some()
                        || state.drag.is_some()
                        || state.icon_presence.is_some()
                        || separator_hover
                    {
                        None
                    } else {
                        Renderer::hovered_item_index_for(
                            &state.model,
                            &state.config.dock,
                            &state.theme,
                            point,
                            state.hover.is_some(),
                        )
                    };
                    let next_hover = next_label.map(|_| point);
                    hidden_changed = state.hidden;
                    if state.hidden {
                        state.hidden = false;
                        state.last_shape_size = None;
                        move_dock(&mut state, true);
                    }
                    state.hover = next_hover;
                    hover_changed = state.hover != previous_hover;
                    label_changed = state.last_shape_label != next_label;
                }
            }
            set_separator_resize_cursor(&drawing, separator_hover || resizing);
            if resizing {
                log_slow("motion", started.elapsed());
                return;
            }
            if hidden_changed || label_changed {
                sync_dock_window(&state, &window, &drawing, &gl_area, false);
            }
            if !(hidden_changed || label_changed || hover_changed) {
                log_slow("motion", started.elapsed());
                return;
            }
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
                            &state,
                            DockUiHandles {
                                window: &window,
                                drawing: &drawing,
                                gl_area: &gl_area,
                            },
                            &item,
                            index,
                            button,
                            Point { x, y },
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
                dock_layout_for_state(&state, None)
                    .hit_test(point)
                    .and_then(|index| {
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
        drag.connect_drag_end(move |_, _, _| {
            let changed = {
                let mut state = state.borrow_mut();
                finish_separator_resize(&mut state)
            };
            if changed {
                let state = state.borrow();
                save_runtime_config(&state);
            }
            ensure_icon_animation_if_needed(&state, &window, &drawing, &gl_area);
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
    if resize.target_icon_size == new_size {
        return false;
    }

    resize.target_icon_size = new_size;
    resize.current_icon_size = new_size;
    state.hover = None;
    true
}

fn finish_separator_resize(state: &mut Runtime) -> bool {
    let Some(resize) = state.separator_resize.take() else {
        return false;
    };

    state.hover = None;
    state.suppress_next_left_click = true;
    state.config.dock.icon_size = resize.target_icon_size;
    if (resize.render_icon_size - resize.target_icon_size as f64).abs() >= 0.5 {
        state.dock_size_transition = Some(DockSizeTransition {
            from_icon_size: resize.render_icon_size,
            to_icon_size: resize.target_icon_size as f64,
            started: Instant::now(),
            duration: DOCK_SIZE_TRANSITION_DURATION,
        });
    } else {
        state.dock_size_transition = None;
    }
    state.last_size = None;
    state.last_geometry = None;
    state.last_reserved_geometry = None;
    state.last_shape_size = None;
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
                        translate_rect(icon.rect, 0.0, interpolate(travel, 0.0, enter_progress)),
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

fn indicator_visual_for_item(item: &DockItem) -> IndicatorVisual {
    if item.active {
        IndicatorVisual {
            visibility: 1.0,
            emphasis: 1.0,
        }
    } else if item.is_running() {
        IndicatorVisual {
            visibility: 1.0,
            emphasis: 0.0,
        }
    } else {
        IndicatorVisual {
            visibility: 0.0,
            emphasis: 0.0,
        }
    }
}

fn build_indicator_animations(
    previous_model: &DockModel,
    next_model: &DockModel,
    existing: &HashMap<String, IndicatorAnimation>,
) -> HashMap<String, IndicatorAnimation> {
    let previous = previous_model
        .items
        .iter()
        .map(|item| {
            (
                item.config_key().to_ascii_lowercase(),
                indicator_visual_for_item(item),
            )
        })
        .collect::<HashMap<_, _>>();
    let next = next_model
        .items
        .iter()
        .map(|item| {
            (
                item.config_key().to_ascii_lowercase(),
                indicator_visual_for_item(item),
            )
        })
        .collect::<HashMap<_, _>>();
    let mut animations = HashMap::new();

    for key in previous.keys().chain(next.keys()) {
        let Some(key) = next
            .get_key_value(key)
            .map(|(key, _)| key)
            .or_else(|| previous.get_key_value(key).map(|(key, _)| key))
        else {
            continue;
        };
        if animations.contains_key(key) {
            continue;
        }

        let from = previous.get(key).copied().unwrap_or(IndicatorVisual {
            visibility: 0.0,
            emphasis: 0.0,
        });
        let to = next.get(key).copied().unwrap_or(IndicatorVisual {
            visibility: 0.0,
            emphasis: 0.0,
        });

        if let Some(animation) = existing.get(key)
            && animation.started.elapsed() < INDICATOR_ANIMATION_DURATION
            && animation.to == to
        {
            animations.insert(key.clone(), animation.clone());
            continue;
        }

        if from == to {
            continue;
        }

        let from = existing
            .get(key)
            .filter(|animation| animation.started.elapsed() < INDICATOR_ANIMATION_DURATION)
            .map(current_indicator_visual)
            .unwrap_or(from);

        animations.insert(
            key.clone(),
            IndicatorAnimation {
                from,
                to,
                started: Instant::now(),
            },
        );
    }

    animations
}

fn indicator_animation_frame(state: &Runtime) -> Option<IndicatorAnimationFrame> {
    let states = state
        .indicator_animations
        .iter()
        .filter(|(_, animation)| animation.started.elapsed() < INDICATOR_ANIMATION_DURATION)
        .map(|(item_key, animation)| {
            let visual = current_indicator_visual(animation);
            IndicatorAnimationState {
                item_key: item_key.clone(),
                visibility: visual.visibility,
                emphasis: visual.emphasis,
            }
        })
        .collect::<Vec<_>>();

    (!states.is_empty()).then_some(IndicatorAnimationFrame { states })
}

fn current_indicator_visual(animation: &IndicatorAnimation) -> IndicatorVisual {
    let raw = (animation.started.elapsed().as_secs_f64()
        / INDICATOR_ANIMATION_DURATION.as_secs_f64())
    .clamp(0.0, 1.0);
    let visibility_progress = ease_out_cubic(raw);
    let emphasis_progress = ease_in_out_cubic(raw);
    IndicatorVisual {
        visibility: interpolate(
            animation.from.visibility,
            animation.to.visibility,
            visibility_progress,
        ),
        emphasis: interpolate(
            animation.from.emphasis,
            animation.to.emphasis,
            emphasis_progress,
        ),
    }
}

fn ease_out_cubic(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

fn ease_in_out_cubic(t: f64) -> f64 {
    if t < 0.5 {
        4.0 * t.powi(3)
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
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
            update_separator_resize_animation(&mut state);
            prune_finished_icon_slide(&mut state);
            prune_finished_icon_presence(&mut state);
            prune_finished_indicator_animations(&mut state);
            prune_finished_dock_size_transition(&mut state);
            state.drag.is_some()
                || state.separator_resize.is_some()
                || state.icon_slide.is_some()
                || state.icon_presence.is_some()
                || !state.indicator_animations.is_empty()
                || state.dock_size_transition.is_some()
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

fn prune_finished_indicator_animations(state: &mut Runtime) {
    state
        .indicator_animations
        .retain(|_, animation| animation.started.elapsed() < INDICATOR_ANIMATION_DURATION);
}

fn prune_finished_dock_size_transition(state: &mut Runtime) {
    if state
        .dock_size_transition
        .as_ref()
        .map(|transition| transition.started.elapsed() >= transition.duration)
        .unwrap_or(false)
    {
        state.dock_size_transition = None;
    }
}

fn update_separator_resize_animation(state: &mut Runtime) {
    let Some(resize) = state.separator_resize.as_mut() else {
        return;
    };
    resize.render_icon_size =
        approach_icon_size(resize.render_icon_size, resize.target_icon_size as f64);
}

fn approach_icon_size(current: f64, target: f64) -> f64 {
    let delta = target - current;
    if delta.abs() < 0.05 {
        target
    } else {
        current + delta * 0.34
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

fn show_applet_fan_for_item(
    state: &Rc<RefCell<Runtime>>,
    ui: DockUiHandles<'_>,
    item: &DockItem,
    index: usize,
    point: Point,
) {
    let Some(source) = applet_fan_source(item) else {
        return;
    };
    show_applet_fan(state, ui, source, index, point);
}

fn show_applet_fan(
    state: &Rc<RefCell<Runtime>>,
    ui: DockUiHandles<'_>,
    source: AppletFanSource,
    index: usize,
    point: Point,
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
        x: point.x,
        y: point.y,
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
            let mut hit_regions = hit_regions.borrow_mut();
            let mut icon_cache = icon_cache.borrow_mut();
            draw_applet_fan(
                cr,
                AppletFanDrawFrame {
                    width,
                    height,
                    source: source.as_ref(),
                    hover_index: *hover_index.borrow(),
                    reveal_progress: applet_fan_reveal_progress(reveal_started.elapsed()),
                    hit_regions: &mut hit_regions,
                    icon_cache: &mut icon_cache,
                },
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
        let window = ui.window.clone();
        let drawing = ui.drawing.clone();
        let gl_area = ui.gl_area.clone();
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
    popover.set_parent(ui.drawing);

    present_runtime_popover(state, ui.window, ui.drawing, ui.gl_area, &popover);
    start_applet_fan_reveal_tick(&fan, reveal_started);
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
        DockContextAction::CustomizerDebug => {
            show_customizer_debug_window(state, window, drawing, gl_area);
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
        DockContextAction::ToggleAutostart => {
            toggle_runtime_autostart(state);
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
        let startup = state.config.startup.clone();
        state.config = Config::default().normalized();
        state.config.custom_icons = custom_icons;
        state.config.startup = startup;
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

fn toggle_runtime_autostart(state: &Rc<RefCell<Runtime>>) {
    let enabled = {
        let mut state = state.borrow_mut();
        state.config.startup.autostart = !state.config.startup.autostart;
        state.config.startup.prompt_seen = true;
        save_runtime_config(&state);
        state.config.startup.autostart
    };
    apply_autostart_enabled(enabled);
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
        let previous_render_icon_size = rendered_dock_config(&state).icon_size as f64;
        update(&mut state);
        let next_icon_size = state.config.dock.icon_size as f64;
        if (previous_render_icon_size - next_icon_size).abs() >= 1.0 {
            state.dock_size_transition = Some(DockSizeTransition {
                from_icon_size: previous_render_icon_size,
                to_icon_size: next_icon_size,
                started: Instant::now(),
                duration: DOCK_SIZE_TRANSITION_DURATION,
            });
        }
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
        let changed = {
            let mut state = state.borrow_mut();
            prune_finished_icon_slide(&mut state);
            prune_finished_icon_presence(&mut state);
            if !should_run_periodic_refresh(&state) {
                false
            } else {
                let config_or_theme_changed = refresh_config_and_theme(&mut state);
                let model_changed = state.refresh_model();
                config_or_theme_changed || model_changed
            }
        };
        if !changed {
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
    let needs_tick = {
        let state = state.borrow();
        state.icon_presence.is_some()
            || !state.indicator_animations.is_empty()
            || state.dock_size_transition.is_some()
    };
    if needs_tick {
        ensure_icon_animation_tick(state, window, drawing, gl_area);
    }
}

fn should_run_periodic_refresh(state: &Runtime) -> bool {
    state.context_menu.is_none()
        && state.drag.is_none()
        && state.separator_resize.is_none()
        && state.icon_slide.is_none()
        && state.icon_presence.is_none()
        && state.indicator_animations.is_empty()
        && state.dock_size_transition.is_none()
        && state.startup_reveal.is_none()
        && !state.customizer_open
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

fn refresh_config_and_theme(state: &mut Runtime) -> bool {
    let mut changed = false;
    match Config::load_from_path(&state.config_path) {
        Ok(config) => {
            if config != state.config {
                tracing::info!("reloaded config {}", state.config_path.display());
                let previous_render_icon_size = rendered_dock_config(state).icon_size as f64;
                let next_icon_size = config.dock.icon_size as f64;
                let startup_changed = state.config.startup != config.startup;
                state.config = config;
                if startup_changed && state.config.startup.prompt_seen {
                    apply_autostart_enabled(state.config.startup.autostart);
                }
                if (previous_render_icon_size - next_icon_size).abs() >= 1.0 {
                    state.dock_size_transition = Some(DockSizeTransition {
                        from_icon_size: previous_render_icon_size,
                        to_icon_size: next_icon_size,
                        started: Instant::now(),
                        duration: DOCK_SIZE_TRANSITION_DURATION,
                    });
                }
                state.hidden = false;
                state.last_size = None;
                state.last_geometry = None;
                state.last_reserved_geometry = None;
                state.last_shape_size = None;
                changed = true;
            }
        }
        Err(error) => {
            tracing::warn!(
                "could not reload config {}: {error:#}",
                state.config_path.display()
            );
            return false;
        }
    }

    let (theme_id, theme_renderer, theme) =
        resolve_runtime_theme(state.composited, &state.config.theme);
    if theme != state.theme {
        tracing::info!("reloaded theme {} ({:?})", theme_id, theme_renderer);
        state.theme = theme;
        state.last_size = None;
        state.last_geometry = None;
        state.last_reserved_geometry = None;
        state.last_shape_size = None;
        changed = true;
    }

    changed
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
    ui: DockUiHandles<'_>,
    item: &DockItem,
    index: usize,
    button: u32,
    point: Point,
) {
    if button == 1
        && (item.is_downloads_applet() || item.is_trash_applet() || item.is_folder_applet())
    {
        show_applet_fan_for_item(state, ui, item, index, point);
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
    let update_reserved_space = state.icon_presence.is_none();
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
    let config = rendered_dock_config(state);
    let mut visual_regions =
        Renderer::visual_regions_for_layout(&state.model, &layout, &config, &state.theme);
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
                + (8 * CONTEXT_MENU_ITEM_HEIGHT)
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
        let separator = layout.separator.expect("separator layout");
        let hover = separator_hover_rect(separator.rect);
        let point = Point {
            x: hover.center_x(),
            y: hover.y + hover.height * 0.18,
        };

        let resize = begin_separator_resize(&layout, point, 480, 64).expect("resize mode");

        assert_eq!(resize.start_window_y, 480);
        assert_eq!(resize.start_icon_size, 64);
        assert_eq!(resize.target_icon_size, 64);
        assert_eq!(resize.render_icon_size, 64.0);
        assert_eq!(resize.current_icon_size, 64);
    }

    #[test]
    fn separator_resize_drag_delta_stays_anchored_when_window_moves() {
        let resize = SeparatorResize {
            start_window_y: 900,
            start_icon_size: 64,
            target_icon_size: 64,
            render_icon_size: 64.0,
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
    fn separator_resize_animation_moves_toward_target_without_snapping() {
        let next = approach_icon_size(64.0, 96.0);

        assert!(next > 64.0);
        assert!(next < 96.0);
        assert_eq!(approach_icon_size(95.98, 96.0), 96.0);
    }

    #[test]
    fn dock_size_transition_easing_is_symmetric() {
        assert_eq!(ease_in_out_cubic(0.0), 0.0);
        assert_eq!(ease_in_out_cubic(1.0), 1.0);
        assert!((ease_in_out_cubic(0.5) - 0.5).abs() < 0.001);
        assert!(ease_in_out_cubic(0.25) < 0.25);
        assert!(ease_in_out_cubic(0.75) > 0.75);
    }

    #[test]
    fn indicator_animation_builds_for_running_transition() {
        let previous = DockModel {
            items: vec![item_with_state(
                Some("xfce4-terminal.desktop"),
                false,
                false,
            )],
        };
        let next = DockModel {
            items: vec![item_with_state(Some("xfce4-terminal.desktop"), false, true)],
        };

        let animations = build_indicator_animations(&previous, &next, &HashMap::new());
        let animation = animations
            .get("xfce4-terminal.desktop")
            .expect("running indicator animation");

        assert_eq!(animation.from.visibility, 0.0);
        assert_eq!(animation.from.emphasis, 0.0);
        assert_eq!(animation.to.visibility, 1.0);
        assert_eq!(animation.to.emphasis, 0.0);
    }

    #[test]
    fn indicator_animation_builds_for_active_growth() {
        let previous = DockModel {
            items: vec![item_with_state(Some("xfce4-terminal.desktop"), false, true)],
        };
        let next = DockModel {
            items: vec![item_with_state(Some("xfce4-terminal.desktop"), true, true)],
        };

        let animations = build_indicator_animations(&previous, &next, &HashMap::new());
        let animation = animations
            .get("xfce4-terminal.desktop")
            .expect("active indicator animation");

        assert_eq!(animation.from.visibility, 1.0);
        assert_eq!(animation.from.emphasis, 0.0);
        assert_eq!(animation.to.visibility, 1.0);
        assert_eq!(animation.to.emphasis, 1.0);
    }

    #[test]
    fn customizer_color_serialization_uses_hex_rgba() {
        let rgba = gdk::RGBA::new(90.0 / 255.0, 131.0 / 255.0, 170.0 / 255.0, 1.0);

        assert_eq!(rgba_to_config_color(rgba), "#5a83aaff");
        assert_eq!(
            rgba_from_config_color("rgb(90, 131, 170)"),
            gdk::RGBA::new(90.0 / 255.0, 131.0 / 255.0, 170.0 / 255.0, 1.0)
        );
    }

    #[test]
    fn customizer_slider_fields_update_draft_config() {
        let mut config = Config::default().normalized();

        CustomizerSliderField::IconSize.set(&mut config, 96.4);
        CustomizerSliderField::FrontLipRatio.set(&mut config, 0.12);
        CustomizerSliderField::IconFloorOffset.set(&mut config, -0.08);

        assert_eq!(config.dock.icon_size, 96);
        assert_eq!(config.theme.front_lip_ratio, 0.12);
        assert_eq!(config.theme.icon_floor_offset, -0.08);
    }

    #[test]
    fn customizer_color_fields_update_draft_config() {
        let mut config = Config::default().normalized();

        CustomizerColorField::ShelfTop.set(&mut config, "#112233ff".to_string());
        CustomizerColorField::Indicator.set(&mut config, "#abcdefcc".to_string());

        assert_eq!(config.theme.shelf_top, "#112233ff");
        assert_eq!(config.theme.indicator, "#abcdefcc");
    }

    #[test]
    fn separator_does_not_hit_icon_magnify_path() {
        let layout = separator_test_layout();
        let separator = layout.separator.expect("separator layout");
        let hover = separator_hover_rect(separator.rect);
        let point = Point {
            x: hover.center_x(),
            y: hover.y + hover.height * 0.18,
        };

        assert!(separator_hit_test_in_layout(&layout, point));
        assert!(layout.hit_test(point).is_none());
    }

    #[test]
    fn separator_hover_rect_extends_above_visual_separator() {
        let layout = separator_test_layout();
        let separator = layout.separator.expect("separator layout");
        let hover = separator_hover_rect(separator.rect);

        assert!(hover.y < separator.rect.y);
        assert!(hover.height > separator.rect.height);
    }
}
