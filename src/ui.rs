use crate::backend::x11::X11Backend;
use crate::backend::{DockGeometry, PlatformBackend};
use crate::config::{Config, RenderMode};
use crate::desktop::DesktopIndex;
use crate::layout::Point;
use crate::model::{DockItem, DockModel};
use crate::renderer::{IconCache, RenderFrame, Renderer, ShelfLayer};
use crate::scene3d::Scene3dRenderer;
use crate::shelf::ShelfRenderer;
use crate::theme::Theme;
use crate::theme_pack::ThemePack;
use gdk_x11::X11Surface;
use gtk::glib::{self, Propagation, object::Cast};
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, DrawingArea, EventControllerMotion, GLArea, GestureClick,
    Overlay, gdk,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

const APP_ID: &str = "dev.osdockx.OSDockX";
const EDGE_VISIBLE_PIXELS: i32 = 4;
const SLOW_UI_OP: Duration = Duration::from_millis(4);

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
    last_shape_size: Option<(i32, i32)>,
    last_shape_label: Option<usize>,
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
            match self.config.dock.edge {
                crate::config::DockEdge::Bottom => {
                    geometry.y += geometry.height as i32 - EDGE_VISIBLE_PIXELS;
                }
                crate::config::DockEdge::Top => {
                    geometry.y -= geometry.height as i32 - EDGE_VISIBLE_PIXELS;
                }
                crate::config::DockEdge::Left => {
                    geometry.x -= geometry.width as i32 - EDGE_VISIBLE_PIXELS;
                }
                crate::config::DockEdge::Right => {
                    geometry.x += geometry.width as i32 - EDGE_VISIBLE_PIXELS;
                }
            }
        }
        Some(geometry)
    }
}

fn build_ui(app: &Application) -> anyhow::Result<()> {
    let (config, config_path) = Config::load_or_create()?;
    tracing::info!("using config {}", config_path.display());

    install_css();

    let composited = gdk::Display::default().is_some_and(|display| display.is_composited());
    let theme_pack = ThemePack::load(&config.theme);
    tracing::info!("using theme {} ({:?})", theme_pack.id, theme_pack.renderer);
    let theme = if composited {
        theme_pack.theme.clone()
    } else {
        tracing::warn!("display is not composited; using opaque shelf fallback");
        theme_pack.theme.clone().opaque_fallback()
    };
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
        last_shape_size: None,
        last_shape_label: None,
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
            let theme = state.theme.clone();
            let mut icons = std::mem::take(&mut state.icons);
            let shelf_layer = shelf_layer_for(&state);
            state.renderer.draw_overlay(
                cr,
                RenderFrame {
                    model: &model,
                    config: &config,
                    theme: &theme,
                    hover,
                    shelf_layer,
                },
                &mut icons,
            );
            state.icons = icons;
        });
    }
    wire_gl_area(&state, &gl_area, &drawing);

    wire_motion(&state, &window, &drawing, &gl_area);
    wire_clicks(&state, &drawing);
    wire_realize(&state, &window);
    wire_refresh(&state, &window, &drawing, &gl_area);

    window.present();
    queue_gl_render_if_enabled(&state, &gl_area);
    Ok(())
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
        ",
    );
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn wire_realize(state: &Rc<RefCell<Runtime>>, window: &ApplicationWindow) {
    let state = Rc::clone(state);
    window.connect_realize(move |window| {
        let Some(surface) = window.surface() else {
            return;
        };
        let Ok(surface) = surface.downcast::<X11Surface>() else {
            tracing::warn!("GTK surface is not an X11 surface");
            return;
        };
        let xid = surface.xid() as u32;
        let mut state = state.borrow_mut();
        state.dock_xid = Some(xid);
        let Some(geometry) = state.desired_geometry() else {
            return;
        };
        if let Some(backend) = state.backend.as_mut()
            && let Err(error) = backend.set_dock_window(xid, geometry)
        {
            tracing::warn!("could not configure X11 dock window: {error:#}");
        }
        state.last_geometry = Some(geometry);
        shape_dock(&mut state);
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
            {
                let mut state = state.borrow_mut();
                state.hover = Some(Point { x, y });
                if state.hidden {
                    state.hidden = false;
                    move_dock(&mut state);
                }
            }
            sync_dock_window(&state, &window, &drawing, &gl_area, false);
            queue_gl_render_if_enabled(&state, &gl_area);
            drawing.queue_draw();
            log_slow("motion", started.elapsed());
        });
    }
    {
        let state = Rc::clone(state);
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        motion.connect_leave(move |_| {
            let autohide;
            let delay;
            {
                let mut state = state.borrow_mut();
                state.hover = None;
                autohide = state.config.dock.autohide;
                delay = state.config.dock.hide_delay_ms;
            }
            queue_gl_render_if_enabled(&state, &gl_area);
            drawing.queue_draw();

            if autohide {
                let state = Rc::clone(&state);
                glib::timeout_add_local_once(Duration::from_millis(delay as u64), move || {
                    let mut state = state.borrow_mut();
                    if state.hover.is_none() {
                        state.hidden = true;
                        move_dock(&mut state);
                    }
                });
            }
        });
    }
    drawing.add_controller(motion);
}

fn wire_clicks(state: &Rc<RefCell<Runtime>>, drawing: &DrawingArea) {
    let click = GestureClick::new();
    click.set_button(0);
    {
        let state = Rc::clone(state);
        let drawing = drawing.clone();
        click.connect_released(move |gesture, _, x, y| {
            let button = gesture.current_button();
            let hit = {
                let state = state.borrow();
                state.renderer.layout().hit_test(Point { x, y })
            };
            if let Some(index) = hit {
                activate_item(&state, index, button);
            }
            drawing.queue_draw();
        });
    }
    drawing.add_controller(click);
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
            state.refresh_model();
        }
        sync_dock_window(&state, &window, &drawing, &gl_area, true);
        queue_gl_render_if_enabled(&state, &gl_area);
        drawing.queue_draw();
        glib::ControlFlow::Continue
    });
}

fn activate_item(state: &Rc<RefCell<Runtime>>, index: usize, button: u32) {
    let item = {
        let state = state.borrow();
        state.model.items.get(index).cloned()
    };
    let Some(item) = item else {
        return;
    };

    if button == 2 {
        close_item_window(state, &item);
        return;
    }

    if let Some(window) = item.primary_window() {
        let mut state = state.borrow_mut();
        if let Some(backend) = state.backend.as_mut() {
            let result = if item.active {
                backend.minimize_window(window)
            } else {
                backend.focus_window(window)
            };
            if let Err(error) = result {
                tracing::warn!("window action failed: {error:#}");
            }
        }
        return;
    }

    if let Some(desktop_id) = item.desktop_id.as_deref()
        && let Err(error) = state.borrow().desktop_index.launch(desktop_id)
    {
        tracing::warn!("could not launch {desktop_id}: {error:#}");
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
    gl_area.set_visible(state.theme.renderer == RenderMode::Scene3d);
    move_dock(&mut state);
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

fn move_dock(state: &mut Runtime) {
    if state.dock_xid.is_none() {
        return;
    }
    let Some(geometry) = state.desired_geometry() else {
        return;
    };
    if state.last_geometry == Some(geometry) {
        return;
    }
    let started = Instant::now();
    if let Some(backend) = state.backend.as_mut()
        && let Err(error) = backend.move_dock_window(geometry)
    {
        tracing::warn!("could not move dock window: {error:#}");
        return;
    }
    state.last_geometry = Some(geometry);
    log_slow("move-dock", started.elapsed());
}

fn shape_dock(state: &mut Runtime) {
    if state.dock_xid.is_none() {
        return;
    }

    let size = state.desired_size();
    let regions =
        Renderer::visual_regions(&state.model, &state.config.dock, &state.theme, state.hover);
    let started = Instant::now();
    if let Some(backend) = state.backend.as_mut()
        && let Err(error) = backend.set_dock_shape(size, &regions, &regions)
    {
        tracing::debug!("could not shape dock window: {error:#}");
    }
    log_slow("shape-dock", started.elapsed());
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
