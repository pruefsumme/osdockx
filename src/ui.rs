use crate::backend::x11::X11Backend;
use crate::backend::{DockGeometry, PlatformBackend};
use crate::config::Config;
use crate::desktop::DesktopIndex;
use crate::layout::Point;
use crate::model::{DockItem, DockModel};
use crate::renderer::{IconCache, Renderer};
use crate::theme::Theme;
use gdk_x11::X11Surface;
use gtk::glib::{self, object::Cast};
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, DrawingArea, EventControllerMotion, GestureClick, gdk};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

const APP_ID: &str = "dev.osdockx.OSDockX";
const EDGE_VISIBLE_PIXELS: i32 = 4;

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
    icons: IconCache,
    hover: Option<Point>,
    dock_xid: Option<u32>,
    hidden: bool,
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
        Renderer::desired_size(&self.model, &self.config.dock, &self.theme)
    }

    fn desired_geometry(&self) -> Option<DockGeometry> {
        let backend = self.backend.as_ref()?;
        let mut geometry = backend
            .monitor_geometry(self.config.dock.monitor.as_deref())
            .dock_geometry(
                self.desired_size(),
                self.config.dock.edge,
                self.config.dock.reserve_space && !self.config.dock.autohide,
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
    let theme = if composited {
        Theme::from_config(&config.theme)
    } else {
        tracing::warn!("display is not composited; using opaque shelf fallback");
        Theme::from_config(&config.theme).opaque_fallback()
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
        icons: IconCache::new(),
        hover: None,
        dock_xid: None,
        hidden: false,
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

    let drawing = DrawingArea::new();
    drawing.set_hexpand(false);
    drawing.set_vexpand(false);
    update_window_size(&state, &window, &drawing);
    window.set_child(Some(&drawing));

    {
        let state = Rc::clone(&state);
        drawing.set_draw_func(move |_, cr, _, _| {
            let mut state = state.borrow_mut();
            let hover = state.hover;
            let model = state.model.clone();
            let config = state.config.dock.clone();
            let theme = state.theme.clone();
            let mut icons = std::mem::take(&mut state.icons);
            state
                .renderer
                .draw(cr, &model, &config, &theme, hover, &mut icons);
            state.icons = icons;
        });
    }

    wire_motion(&state, &window, &drawing);
    wire_clicks(&state, &drawing);
    wire_realize(&state, &window);
    wire_refresh(&state, &window, &drawing);

    window.present();
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
            background: transparent;
            box-shadow: none;
        }
        drawingarea {
            background: transparent;
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
    });
}

fn wire_motion(state: &Rc<RefCell<Runtime>>, window: &ApplicationWindow, drawing: &DrawingArea) {
    let motion = EventControllerMotion::new();
    {
        let state = Rc::clone(state);
        let window = window.clone();
        let drawing = drawing.clone();
        motion.connect_motion(move |_, x, y| {
            {
                let mut state = state.borrow_mut();
                state.hover = Some(Point { x, y });
                if state.hidden {
                    state.hidden = false;
                    move_dock(&mut state);
                }
            }
            update_window_size(&state, &window, &drawing);
            drawing.queue_draw();
        });
    }
    {
        let state = Rc::clone(state);
        let drawing = drawing.clone();
        motion.connect_leave(move |_| {
            let autohide;
            let delay;
            {
                let mut state = state.borrow_mut();
                state.hover = None;
                autohide = state.config.dock.autohide;
                delay = state.config.dock.hide_delay_ms;
            }
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

fn wire_refresh(state: &Rc<RefCell<Runtime>>, window: &ApplicationWindow, drawing: &DrawingArea) {
    let refresh = state.borrow().config.dock.refresh_ms;
    let state = Rc::clone(state);
    let window = window.clone();
    let drawing = drawing.clone();
    glib::timeout_add_local(Duration::from_millis(refresh as u64), move || {
        {
            let mut state = state.borrow_mut();
            state.refresh_model();
        }
        update_window_size(&state, &window, &drawing);
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

fn update_window_size(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
) {
    let size = state.borrow().desired_size();
    drawing.set_content_width(size.0);
    drawing.set_content_height(size.1);
    window.set_default_size(size.0, size.1);

    let mut state = state.borrow_mut();
    move_dock(&mut state);
}

fn move_dock(state: &mut Runtime) {
    let Some(geometry) = state.desired_geometry() else {
        return;
    };
    if let Some(backend) = state.backend.as_mut()
        && let Err(error) = backend.move_dock_window(geometry)
    {
        tracing::warn!("could not move dock window: {error:#}");
    }
}
