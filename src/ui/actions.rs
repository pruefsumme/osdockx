use super::{
    Runtime, ensure_icon_animation_if_needed, queue_gl_render_if_enabled, save_runtime_config,
    sync_dock_window,
};
use crate::config::AppletConfig;
use gtk::gio;
use gtk::gio::prelude::FileExt;
use gtk::prelude::*;
use gtk::{ApplicationWindow, DrawingArea, FileDialog, FileFilter, GLArea};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub(super) fn toggle_keep_in_dock(
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

pub(super) fn hide_application_from_dock(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    item_key: &str,
) {
    {
        let mut state = state.borrow_mut();
        if !state
            .config
            .hidden
            .iter()
            .any(|id| id.eq_ignore_ascii_case(item_key))
        {
            state.config.hidden.push(item_key.to_string());
        }
        state
            .config
            .pinned
            .retain(|id| !id.eq_ignore_ascii_case(item_key));
        save_runtime_config(&state);
        state.refresh_model();
        state.icons.clear();
    }
    ensure_icon_animation_if_needed(state, window, drawing, gl_area);
    sync_dock_window(state, window, drawing, gl_area, true);
    queue_gl_render_if_enabled(state, gl_area);
    drawing.queue_draw();
}

pub(super) fn reset_custom_icon(
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
        state.sync_custom_icons();
    }
    sync_dock_window(state, window, drawing, gl_area, true);
    queue_gl_render_if_enabled(state, gl_area);
    drawing.queue_draw();
}

pub(super) fn set_custom_icon_value(
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
        state.sync_custom_icons();
    }
    sync_dock_window(state, window, drawing, gl_area, true);
    queue_gl_render_if_enabled(state, gl_area);
    drawing.queue_draw();
}

pub(super) fn pin_application(
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

pub(super) fn select_folder_applet(
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

pub(super) fn remove_folder_applet(
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
            applet
                .path
                .as_ref()
                .is_none_or(|path| path.to_string_lossy().to_ascii_lowercase() != target)
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
        state.sync_custom_icons();
    }
    ensure_icon_animation_if_needed(state, window, drawing, gl_area);
    sync_dock_window(state, window, drawing, gl_area, true);
    queue_gl_render_if_enabled(state, gl_area);
    drawing.queue_draw();
}

pub(super) fn select_custom_icon(
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
