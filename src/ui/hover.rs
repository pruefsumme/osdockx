use super::{
    CONTEXT_MENU_GAP, HOVER_SETTINGS_MENU_WIDTH, Runtime, context_menu_anchor_rect,
    dock_layout_for_state, present_runtime_popover, queue_gl_render_if_enabled,
    save_runtime_config, sync_dock_window,
};
use crate::layout::Rect;
use gtk::prelude::*;
use gtk::{
    ApplicationWindow, Box as GtkBox, DrawingArea, GLArea, Label, Orientation, Popover,
    PositionType, Scale,
};
use std::cell::RefCell;
use std::rc::Rc;

pub(super) fn show_hover_settings_menu(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
) {
    let (dock_width, current_zoom) = {
        let state = state.borrow();
        (
            dock_layout_for_state(&state, None).size.0,
            state.config.dock.zoom_strength,
        )
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