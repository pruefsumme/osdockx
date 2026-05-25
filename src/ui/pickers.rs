use super::{
    ADD_APPLICATION_MENU_VISIBLE_ROWS, ADD_APPLICATION_MENU_WIDTH, CONTEXT_MENU_GAP,
    CONTEXT_MENU_ITEM_HEIGHT, Runtime, context_menu_anchor_rect, context_menu_icon_button,
    dismiss_context_menu, dock_layout_for_state, menu_height, pin_application,
    present_runtime_popover, set_custom_icon_value,
};
use crate::layout::Rect;
use gtk::prelude::*;
use gtk::{
    Align, ApplicationWindow, Box as GtkBox, Button, DrawingArea, FlowBox, GLArea, IconLookupFlags,
    IconTheme, Image, Justification, Label, Orientation, PolicyType, Popover, PositionType,
    ScrolledWindow, SearchEntry, SelectionMode, TextDirection, gdk,
};
use std::cell::RefCell;
use std::rc::Rc;

const THEME_ICON_PICKER_WIDTH: i32 = 560;
const THEME_ICON_PICKER_HEIGHT: i32 = 560;
const THEME_ICON_PICKER_ICON_SIZE: i32 = 48;
const THEME_ICON_PICKER_TILE_WIDTH: i32 = 124;
const THEME_ICON_PICKER_TILE_HEIGHT: i32 = 104;
const THEME_ICON_PICKER_MAX_MATCHES: usize = 96;

pub(super) fn show_add_application_menu(
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

pub(super) fn show_theme_icon_menu(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    item_key: String,
) {
    let icon_names = Rc::new(theme_icon_names());

    let picker = ApplicationWindow::builder()
        .title("Use Theme Icon")
        .transient_for(window)
        .modal(true)
        .default_width(THEME_ICON_PICKER_WIDTH)
        .default_height(THEME_ICON_PICKER_HEIGHT)
        .resizable(true)
        .build();
    if let Some(app) = window.application() {
        picker.set_application(Some(&app));
    }
    picker.add_css_class("osdock-theme-icon-window");

    let shell = GtkBox::new(Orientation::Vertical, 10);
    shell.add_css_class("osdock-context-menu");
    shell.add_css_class("osdock-theme-icon-picker");
    shell.set_margin_top(10);
    shell.set_margin_bottom(10);
    shell.set_margin_start(10);
    shell.set_margin_end(10);

    let title = Label::new(Some("Use Theme Icon"));
    title.add_css_class("osdock-menu-title");
    title.set_xalign(0.0);
    shell.append(&title);

    let search = SearchEntry::new();
    search.add_css_class("osdock-menu-search");
    search.set_placeholder_text(Some("Search icon theme"));
    search.set_hexpand(true);
    search.set_sensitive(!icon_names.is_empty());
    shell.append(&search);

    let status = Label::new(None);
    status.add_css_class("osdock-menu-title");
    status.set_xalign(0.0);
    shell.append(&status);

    let scrolled = ScrolledWindow::new();
    scrolled.set_policy(PolicyType::Never, PolicyType::Automatic);
    scrolled.set_vexpand(true);

    let grid = FlowBox::new();
    grid.add_css_class("osdock-theme-icon-grid");
    grid.set_column_spacing(10);
    grid.set_row_spacing(10);
    grid.set_homogeneous(true);
    grid.set_max_children_per_line(4);
    grid.set_min_children_per_line(2);
    grid.set_selection_mode(SelectionMode::None);
    grid.set_valign(Align::Start);
    scrolled.set_child(Some(&grid));
    shell.append(&scrolled);
    picker.set_child(Some(&shell));

    populate_theme_icon_grid(
        &grid,
        &status,
        icon_names.as_ref(),
        "",
        state,
        window,
        drawing,
        gl_area,
        &picker,
        &item_key,
    );

    {
        let grid = grid.clone();
        let status = status.clone();
        let icon_names = Rc::clone(&icon_names);
        let state = Rc::clone(state);
        let window = window.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        let picker = picker.clone();
        search.connect_search_changed(move |entry| {
            populate_theme_icon_grid(
                &grid,
                &status,
                icon_names.as_ref(),
                entry.text().as_str(),
                &state,
                &window,
                &drawing,
                &gl_area,
                &picker,
                &item_key,
            );
        });
    }

    picker.present();
    let _ = search.grab_focus();
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

#[allow(clippy::too_many_arguments)]
fn populate_theme_icon_grid(
    grid: &FlowBox,
    status: &Label,
    icon_names: &[String],
    query: &str,
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    picker: &ApplicationWindow,
    item_key: &str,
) {
    grid.remove_all();

    let query = query.trim().to_ascii_lowercase();
    if icon_names.is_empty() {
        status.set_text("No theme icons found");
        return;
    }

    let Some(icon_theme) = gdk::Display::default().map(|display| IconTheme::for_display(&display))
    else {
        status.set_text("No icon theme available");
        return;
    };
    let mut match_count = 0;
    for icon_name in icon_names
        .iter()
        .filter(|name| query.is_empty() || name.to_ascii_lowercase().contains(&query))
    {
        if !theme_icon_is_loadable(&icon_theme, icon_name) {
            continue;
        }

        let button = theme_icon_choice_button(icon_name);
        {
            let state = Rc::clone(state);
            let window = window.clone();
            let drawing = drawing.clone();
            let gl_area = gl_area.clone();
            let picker = picker.clone();
            let item_key = item_key.to_string();
            let icon_name = icon_name.clone();
            button.connect_clicked(move |_| {
                set_custom_icon_value(
                    &state,
                    &window,
                    &drawing,
                    &gl_area,
                    &item_key,
                    icon_name.clone(),
                );
                picker.close();
            });
        }
        grid.append(&button);
        match_count += 1;
        if match_count >= THEME_ICON_PICKER_MAX_MATCHES {
            break;
        }
    }

    if match_count == 0 {
        status.set_text("No matching icons");
    } else if query.is_empty() && match_count >= THEME_ICON_PICKER_MAX_MATCHES {
        status.set_text(&format!("Showing first {match_count} icons"));
    } else if query.is_empty() {
        status.set_text(&format!("Showing {match_count} icons"));
    } else if match_count >= THEME_ICON_PICKER_MAX_MATCHES {
        status.set_text(&format!("Showing first {match_count} matches"));
    } else {
        status.set_text(&format!("Showing {match_count} matches"));
    }
}

fn theme_icon_choice_button(icon_name: &str) -> Button {
    let button = Button::new();
    button.add_css_class("osdock-icon-choice");
    button.set_size_request(THEME_ICON_PICKER_TILE_WIDTH, THEME_ICON_PICKER_TILE_HEIGHT);
    button.set_tooltip_text(Some(icon_name));

    let tile = GtkBox::new(Orientation::Vertical, 6);
    tile.set_halign(Align::Center);
    tile.set_valign(Align::Center);

    let image = Image::from_icon_name(icon_name);
    image.set_pixel_size(THEME_ICON_PICKER_ICON_SIZE);
    image.set_halign(Align::Center);
    tile.append(&image);

    let label = Label::new(Some(icon_name));
    label.add_css_class("osdock-icon-choice-label");
    label.set_xalign(0.5);
    label.set_justify(Justification::Center);
    label.set_wrap(true);
    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    label.set_lines(2);
    label.set_max_width_chars(16);
    tile.append(&label);

    button.set_child(Some(&tile));
    button
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
