use crate::backend::x11::X11Backend;
use crate::backend::{DockGeometry, PlatformBackend};
use crate::config::{Config, RenderMode};
use crate::desktop::DesktopIndex;
use crate::layout::{Point, Rect};
use crate::model::{DockItem, DockModel};
use crate::renderer::{IconCache, RenderFrame, Renderer, ShelfLayer};
use crate::scene3d::Scene3dRenderer;
use crate::shelf::ShelfRenderer;
use crate::theme::Theme;
use crate::theme_pack::ThemePack;
use gdk_x11::X11Surface;
use gtk::gio;
use gtk::gio::prelude::FileExt;
use gtk::glib::{self, Propagation, object::Cast};
use gtk::prelude::*;
use gtk::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, DrawingArea,
    EventControllerMotion, FileDialog, FileFilter, GLArea, GestureClick, Label, Orientation,
    Overlay, gdk,
};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

const APP_ID: &str = "dev.osdockx.OSDockX";
const EDGE_VISIBLE_PIXELS: i32 = 4;
const SLOW_UI_OP: Duration = Duration::from_millis(4);
const CONTEXT_MENU_WIDTH: i32 = 164;
const CONTEXT_MENU_HEIGHT: i32 = 72;
const CONTEXT_MENU_GAP: f64 = 8.0;

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
    last_shape_size: Option<(i32, i32)>,
    last_shape_label: Option<usize>,
    last_shape_menu: Option<Rect>,
    context_menu: Option<GtkBox>,
    context_menu_rect: Option<Rect>,
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
        last_shape_size: None,
        last_shape_label: None,
        last_shape_menu: None,
        context_menu: None,
        context_menu_rect: None,
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
            let custom_icons = state.config.custom_icons.clone();
            let theme = state.theme.clone();
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
                },
                &mut icons,
            );
            state.icons = icons;
        });
    }
    wire_gl_area(&state, &gl_area, &drawing);

    wire_motion(&state, &window, &drawing, &gl_area);
    wire_clicks(&state, &window, &overlay, &drawing, &gl_area);
    wire_realize(&state, &window);
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
        .osdock-context-menu {
            background: alpha(#1b1c1f, 0.90);
            border: 1px solid alpha(#ffffff, 0.18);
            border-radius: 7px;
            box-shadow: 0 8px 20px alpha(#000000, 0.48);
            padding: 4px;
        }
        .osdock-menu-box {
            padding: 1px;
        }
        button.osdock-menu-item {
            min-height: 0;
            min-width: 148px;
            padding: 0;
            margin: 0;
            border: none;
            border-radius: 4px;
            background: transparent;
            box-shadow: none;
            color: #f2f2f2;
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
            let point = Point { x, y };
            {
                let mut state = state.borrow_mut();
                let next_hover = if state.context_menu.is_some() {
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
                    move_dock(&mut state);
                }
                state.hover = next_hover;
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
            sync_dock_window(&state, &window, &drawing, &gl_area, false);
        });
    }
    drawing.add_controller(motion);
}

fn wire_clicks(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    overlay: &Overlay,
    drawing: &DrawingArea,
    gl_area: &GLArea,
) {
    let click = GestureClick::new();
    click.set_button(0);
    {
        let state = Rc::clone(state);
        let window = window.clone();
        let overlay = overlay.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        click.connect_released(move |gesture, _, x, y| {
            let button = gesture.current_button();
            if button == 1 && state.borrow().context_menu.is_some() {
                let dismissed = {
                    let mut state = state.borrow_mut();
                    dismiss_context_menu(&mut state, &overlay)
                };
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
                if button == 3 {
                    show_context_menu(&state, &window, &overlay, &drawing, &gl_area, index, x, y);
                } else if button == 1 || button == 2 {
                    let dismissed = {
                        let mut state = state.borrow_mut();
                        dismiss_context_menu(&mut state, &overlay)
                    };
                    if dismissed {
                        sync_dock_window(&state, &window, &drawing, &gl_area, true);
                    }
                    activate_item(&state, index, button);
                }
            } else if button != 0 {
                let dismissed = {
                    let mut state = state.borrow_mut();
                    dismiss_context_menu(&mut state, &overlay)
                };
                if dismissed {
                    sync_dock_window(&state, &window, &drawing, &gl_area, true);
                }
            }
            drawing.queue_draw();
        });
    }
    drawing.add_controller(click);
}

fn show_context_menu(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    overlay: &Overlay,
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
    let item_key = item.config_key();
    let pinned = item.pinned;
    let anchor = icon_rect.unwrap_or(Rect {
        x,
        y,
        width: 1.0,
        height: 1.0,
    });
    let menu_rect = context_menu_rect(anchor, dock_width);

    {
        let mut state = state.borrow_mut();
        dismiss_context_menu(&mut state, overlay);
        state.hover = None;
    }

    let menu = GtkBox::new(Orientation::Vertical, 0);
    menu.add_css_class("osdock-context-menu");
    menu.add_css_class("osdock-menu-box");
    menu.set_halign(Align::Start);
    menu.set_valign(Align::Start);
    menu.set_margin_start(menu_rect.x.round() as i32);
    menu.set_margin_top(menu_rect.y.round() as i32);
    menu.set_size_request(CONTEXT_MENU_WIDTH, -1);
    let keep = context_menu_button("Keep in Dock", pinned);
    let select = context_menu_button("Select Icon", false);
    let default_icon = context_menu_button("Set to Default Icon", false);
    menu.append(&keep);
    menu.append(&select);
    menu.append(&default_icon);

    {
        let state = Rc::clone(state);
        let window = window.clone();
        let overlay = overlay.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        let item_key = item_key.clone();
        keep.connect_clicked(move |_| {
            {
                let mut state = state.borrow_mut();
                dismiss_context_menu(&mut state, &overlay);
            }
            toggle_keep_in_dock(&state, &window, &drawing, &gl_area, &item_key, pinned);
        });
    }
    {
        let state = Rc::clone(state);
        let window = window.clone();
        let overlay = overlay.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        let item_key = item_key.clone();
        select.connect_clicked(move |_| {
            {
                let mut state = state.borrow_mut();
                dismiss_context_menu(&mut state, &overlay);
            }
            sync_dock_window(&state, &window, &drawing, &gl_area, true);
            drawing.queue_draw();
            select_custom_icon(&state, &window, &drawing, &gl_area, item_key.clone());
        });
    }
    {
        let state = Rc::clone(state);
        let window = window.clone();
        let overlay = overlay.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        let item_key = item_key.clone();
        default_icon.connect_clicked(move |_| {
            {
                let mut state = state.borrow_mut();
                dismiss_context_menu(&mut state, &overlay);
            }
            reset_custom_icon(&state, &window, &drawing, &gl_area, &item_key);
        });
    }

    overlay.add_overlay(&menu);
    {
        let mut state = state.borrow_mut();
        state.context_menu_rect = Some(menu_rect);
        state.context_menu = Some(menu);
    }
    sync_dock_window(state, window, drawing, gl_area, true);
    queue_gl_render_if_enabled(state, gl_area);
    drawing.queue_draw();
}

fn context_menu_rect(icon_rect: Rect, dock_width: i32) -> Rect {
    let menu_width = CONTEXT_MENU_WIDTH as f64;
    let menu_height = CONTEXT_MENU_HEIGHT as f64;
    let max_x = (dock_width as f64 - menu_width - 2.0).max(2.0);
    let x = (icon_rect.center_x() - menu_width / 2.0).clamp(2.0, max_x);
    let y = (icon_rect.y - menu_height - CONTEXT_MENU_GAP).max(2.0);
    Rect {
        x,
        y,
        width: menu_width,
        height: menu_height,
    }
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

fn dismiss_context_menu(state: &mut Runtime, overlay: &Overlay) -> bool {
    let Some(menu) = state.context_menu.take() else {
        state.context_menu_rect = None;
        return false;
    };
    overlay.remove_overlay(&menu);
    state.context_menu_rect = None;
    true
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
            refresh_config_and_theme(&mut state);
            state.refresh_model();
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
                state.last_shape_menu = None;
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
        state.last_shape_menu = None;
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
    let shape_menu = state.context_menu_rect;
    if force_shape
        || size_changed
        || state.last_shape_size != Some(size)
        || state.last_shape_label != shape_label
        || state.last_shape_menu != shape_menu
    {
        shape_dock(&mut state);
        state.last_shape_size = Some(size);
        state.last_shape_label = shape_label;
        state.last_shape_menu = shape_menu;
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
    let mut visual_regions =
        Renderer::visual_regions(&state.model, &state.config.dock, &state.theme, state.hover);
    let mut input_regions = Renderer::input_regions(&state.model, &state.config.dock, &state.theme);
    if let Some(rect) = state.context_menu_rect {
        let rect = padded_rect(rect, 8.0);
        visual_regions.push(rect);
        input_regions.push(rect);
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
