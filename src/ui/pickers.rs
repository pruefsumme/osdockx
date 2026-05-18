use super::{
    ADD_APPLICATION_MENU_VISIBLE_ROWS, ADD_APPLICATION_MENU_WIDTH, CONTEXT_MENU_GAP,
    CONTEXT_MENU_ITEM_HEIGHT, Runtime, THEME_ICON_MENU_MAX_MATCHES,
    THEME_ICON_MENU_VISIBLE_ROWS, THEME_ICON_MENU_WIDTH, context_menu_anchor_rect,
    context_menu_icon_button, dismiss_context_menu, dock_layout_for_state, menu_height,
    pin_application, present_runtime_popover, set_custom_icon_value,
};
use crate::layout::Rect;
use gtk::prelude::*;
use gtk::{
    ApplicationWindow, Box as GtkBox, DrawingArea, GLArea, IconLookupFlags, IconTheme, Label,
    Orientation, PolicyType, Popover, PositionType, ScrolledWindow, SearchEntry, TextDirection,
    gdk,
};
use std::cell::RefCell;
use std::rc::Rc;

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