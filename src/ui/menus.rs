use super::{
    ApplicationContextAction, CONTEXT_MENU_CHROME_HEIGHT, CONTEXT_MENU_GAP,
    CONTEXT_MENU_ITEM_HEIGHT, CONTEXT_MENU_SEPARATOR_HEIGHT, CONTEXT_MENU_SETTINGS_COUNT,
    CONTEXT_MENU_WIDTH, DOCK_CONTEXT_MENU_ACTIONS, DOCK_CONTEXT_MENU_WIDTH, DockContextAction,
    Runtime, dismiss_context_menu, dock_layout_for_state, hide_application_from_dock,
    queue_gl_render_if_enabled, remove_folder_applet, reset_custom_icon,
    run_application_context_action, run_dock_context_action, select_custom_icon,
    show_theme_icon_menu, sync_dock_window, toggle_keep_in_dock,
};
use crate::layout::Rect;
use crate::model::DockItem;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Align, ApplicationWindow, Box as GtkBox, Button, DrawingArea, GLArea, Image, Label,
    Orientation, Popover, PositionType, gdk,
};
use std::cell::RefCell;
use std::rc::Rc;

pub(super) fn show_context_menu(
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
        let button = context_menu_icon_button(
            application_context_action_label(&item, action),
            application_context_action_icon(&item, action),
            false,
        );
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

    let keep = context_menu_icon_button("Keep in Dock", "list-add", pinned);
    let hide_from_dock =
        context_menu_icon_button("Don't Show in Dock Anymore", "edit-delete", false);
    let select = context_menu_icon_button("Select Icon File...", "image-x-generic", false);
    let theme_icon =
        context_menu_icon_button("Use Theme Icon...", "preferences-desktop-icons", false);
    let default_icon = context_menu_icon_button("Set Default Icon", "edit-clear", false);
    menu.append(&keep);
    menu.append(&hide_from_dock);
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
        hide_from_dock.connect_clicked(move |_| {
            dismiss_context_menu(&state);
            hide_application_from_dock(&state, &window, &drawing, &gl_area, &item_key);
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
    popover.set_autohide(true);
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
    popover.set_autohide(true);
    popover.set_has_arrow(false);
    popover.set_position(PositionType::Top);
    popover.set_offset(0, -(CONTEXT_MENU_GAP.round() as i32));
    popover.set_pointing_to(Some(&context_menu_anchor_rect(anchor, dock_width)));
    popover.set_child(Some(&menu));
    popover.set_parent(drawing);

    present_runtime_popover(state, window, drawing, gl_area, &popover);
}

pub(super) fn show_dock_context_menu(
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
        DockContextAction::CustomizerDebug,
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
        DockContextAction::CustomizerDebug => "Customizer (Debug)",
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
        DockContextAction::CustomizerDebug => "applications-graphics",
        DockContextAction::ToggleAutohide => "view-fullscreen",
        DockContextAction::ToggleReserveSpace => "view-restore",
        DockContextAction::ReloadTheme => "view-refresh",
        DockContextAction::ResetDefaults => "edit-clear-all",
        DockContextAction::ResetCustomIcons => "edit-delete",
        DockContextAction::OpenConfigFolder => "preferences-system",
    }
}

pub(super) fn application_context_actions(item: &DockItem) -> Vec<ApplicationContextAction> {
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

fn application_context_action_icon(
    item: &DockItem,
    action: ApplicationContextAction,
) -> &'static str {
    match action {
        ApplicationContextAction::Launch if item.is_running() => "window-new",
        ApplicationContextAction::Launch => "system-run",
        ApplicationContextAction::Focus => "view-restore",
        ApplicationContextAction::Minimize => "go-down",
        ApplicationContextAction::Close => "window-close",
    }
}

pub(super) fn context_menu_height(app_action_count: usize) -> i32 {
    let separator_count = i32::from(app_action_count > 0);
    menu_height(
        app_action_count + CONTEXT_MENU_SETTINGS_COUNT,
        separator_count as usize,
    )
}

pub(super) fn menu_height(action_count: usize, separator_count: usize) -> i32 {
    CONTEXT_MENU_CHROME_HEIGHT
        + (action_count as i32 * CONTEXT_MENU_ITEM_HEIGHT)
        + separator_count as i32 * CONTEXT_MENU_SEPARATOR_HEIGHT
}

pub(super) fn context_menu_anchor_rect(icon_rect: Rect, dock_width: i32) -> gdk::Rectangle {
    let width = icon_rect.width.ceil().max(1.0) as i32;
    let height = icon_rect.height.ceil().max(1.0) as i32;
    let max_x = (dock_width - width - 2).max(2);
    let x = (icon_rect.x.floor() as i32).clamp(2, max_x);
    let y = (icon_rect.y.floor() as i32).max(2);
    gdk::Rectangle::new(x, y, width, height)
}

pub(super) fn context_menu_icon_button(label: &str, icon_name: &str, checked: bool) -> Button {
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

pub(super) fn context_menu_separator() -> GtkBox {
    let separator = GtkBox::new(Orientation::Horizontal, 0);
    separator.add_css_class("osdock-menu-separator");
    separator.set_hexpand(true);
    separator.set_halign(Align::Fill);
    separator.set_size_request(-1, CONTEXT_MENU_SEPARATOR_HEIGHT);
    separator
}

pub(super) fn present_runtime_popover(
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
