use super::{
    CUSTOMIZER_HEIGHT, CUSTOMIZER_PREVIEW_DEBOUNCE, CUSTOMIZER_WIDTH, Runtime,
    context_menu_separator, ensure_icon_animation_if_needed, queue_gl_render_if_enabled,
    resolve_runtime_theme, save_runtime_config, sync_dock_window,
};
use crate::config::Config;
use crate::theme::Color as ThemeColor;
use gtk::glib::{self, Propagation};
use gtk::prelude::*;
use gtk::{
    Align, ApplicationWindow, Box as GtkBox, Button, CheckButton, ColorDialog,
    ColorDialogButton, DrawingArea, GLArea, Label, Orientation, PolicyType, Scale,
    ScrolledWindow, gdk,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CustomizerSliderField {
    IconSize,
    ZoomStrength,
    ShelfHeightRatio,
    ShelfSlantRatio,
    SideMarginRatio,
    ShelfHorizonRatio,
    FrontLipRatio,
    ReflectionOpacity,
    ReflectionHeight,
    ReflectionBandRatio,
    ReflectionBlur,
    Tilt,
    Depth,
    Bevel,
    FloorOpacity,
    ShadowStrength,
    HighlightStrength,
    MaterialRoughness,
    IconFloorOffset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CustomizerColorField {
    ShelfTop,
    ShelfBottom,
    ShelfStroke,
    ShelfHighlight,
    Indicator,
    Badge,
}

impl CustomizerSliderField {
    fn label(self) -> &'static str {
        match self {
            Self::IconSize => "Icon Size",
            Self::ZoomStrength => "Hover Strength",
            Self::ShelfHeightRatio => "Shelf Height",
            Self::ShelfSlantRatio => "Shelf Slant",
            Self::SideMarginRatio => "Side Margin",
            Self::ShelfHorizonRatio => "Shelf Horizon",
            Self::FrontLipRatio => "Front Lip",
            Self::ReflectionOpacity => "Reflection Opacity",
            Self::ReflectionHeight => "Reflection Height",
            Self::ReflectionBandRatio => "Reflection Band",
            Self::ReflectionBlur => "Reflection Blur",
            Self::Tilt => "Tilt",
            Self::Depth => "Depth",
            Self::Bevel => "Bevel",
            Self::FloorOpacity => "Floor Opacity",
            Self::ShadowStrength => "Shadow Strength",
            Self::HighlightStrength => "Highlight Strength",
            Self::MaterialRoughness => "Material Roughness",
            Self::IconFloorOffset => "Icon Floor Offset",
        }
    }

    fn range(self) -> (f64, f64, f64, i32) {
        match self {
            Self::IconSize => (24.0, 160.0, 1.0, 0),
            Self::ZoomStrength => (0.0, 1.6, 0.02, 2),
            Self::ShelfHeightRatio => (0.12, 0.9, 0.01, 2),
            Self::ShelfSlantRatio => (0.0, 0.8, 0.01, 2),
            Self::SideMarginRatio => (0.12, 1.2, 0.01, 2),
            Self::ShelfHorizonRatio => (0.2, 0.8, 0.01, 2),
            Self::FrontLipRatio => (0.01, 0.3, 0.005, 3),
            Self::ReflectionOpacity => (0.0, 0.7, 0.01, 2),
            Self::ReflectionHeight => (0.0, 0.8, 0.01, 2),
            Self::ReflectionBandRatio => (0.0, 0.6, 0.01, 2),
            Self::ReflectionBlur => (0.0, 1.0, 0.01, 2),
            Self::Tilt => (0.0, 1.2, 0.01, 2),
            Self::Depth => (0.0, 1.4, 0.01, 2),
            Self::Bevel => (0.0, 0.8, 0.01, 2),
            Self::FloorOpacity => (0.0, 1.0, 0.01, 2),
            Self::ShadowStrength => (0.0, 1.0, 0.01, 2),
            Self::HighlightStrength => (0.0, 1.0, 0.01, 2),
            Self::MaterialRoughness => (0.0, 1.0, 0.01, 2),
            Self::IconFloorOffset => (-0.4, 0.4, 0.01, 2),
        }
    }

    fn value(self, config: &Config) -> f64 {
        match self {
            Self::IconSize => config.dock.icon_size as f64,
            Self::ZoomStrength => config.dock.zoom_strength,
            Self::ShelfHeightRatio => config.theme.shelf_height_ratio,
            Self::ShelfSlantRatio => config.theme.shelf_slant_ratio,
            Self::SideMarginRatio => config.theme.side_margin_ratio,
            Self::ShelfHorizonRatio => config.theme.shelf_horizon_ratio,
            Self::FrontLipRatio => config.theme.front_lip_ratio,
            Self::ReflectionOpacity => config.theme.reflection_opacity,
            Self::ReflectionHeight => config.theme.reflection_height,
            Self::ReflectionBandRatio => config.theme.reflection_band_ratio,
            Self::ReflectionBlur => config.theme.reflection_blur,
            Self::Tilt => config.theme.tilt,
            Self::Depth => config.theme.depth,
            Self::Bevel => config.theme.bevel,
            Self::FloorOpacity => config.theme.floor_opacity,
            Self::ShadowStrength => config.theme.shadow_strength,
            Self::HighlightStrength => config.theme.highlight_strength,
            Self::MaterialRoughness => config.theme.material_roughness,
            Self::IconFloorOffset => config.theme.icon_floor_offset,
        }
    }

    pub(super) fn set(self, config: &mut Config, value: f64) {
        match self {
            Self::IconSize => config.dock.icon_size = value.round() as u32,
            Self::ZoomStrength => config.dock.zoom_strength = value,
            Self::ShelfHeightRatio => config.theme.shelf_height_ratio = value,
            Self::ShelfSlantRatio => config.theme.shelf_slant_ratio = value,
            Self::SideMarginRatio => config.theme.side_margin_ratio = value,
            Self::ShelfHorizonRatio => config.theme.shelf_horizon_ratio = value,
            Self::FrontLipRatio => config.theme.front_lip_ratio = value,
            Self::ReflectionOpacity => config.theme.reflection_opacity = value,
            Self::ReflectionHeight => config.theme.reflection_height = value,
            Self::ReflectionBandRatio => config.theme.reflection_band_ratio = value,
            Self::ReflectionBlur => config.theme.reflection_blur = value,
            Self::Tilt => config.theme.tilt = value,
            Self::Depth => config.theme.depth = value,
            Self::Bevel => config.theme.bevel = value,
            Self::FloorOpacity => config.theme.floor_opacity = value,
            Self::ShadowStrength => config.theme.shadow_strength = value,
            Self::HighlightStrength => config.theme.highlight_strength = value,
            Self::MaterialRoughness => config.theme.material_roughness = value,
            Self::IconFloorOffset => config.theme.icon_floor_offset = value,
        }
    }
}

impl CustomizerColorField {
    fn label(self) -> &'static str {
        match self {
            Self::ShelfTop => "Shelf Top",
            Self::ShelfBottom => "Shelf Bottom",
            Self::ShelfStroke => "Shelf Stroke",
            Self::ShelfHighlight => "Shelf Highlight",
            Self::Indicator => "Indicator",
            Self::Badge => "Badge",
        }
    }

    fn value(self, config: &Config) -> &str {
        match self {
            Self::ShelfTop => &config.theme.shelf_top,
            Self::ShelfBottom => &config.theme.shelf_bottom,
            Self::ShelfStroke => &config.theme.shelf_stroke,
            Self::ShelfHighlight => &config.theme.shelf_highlight,
            Self::Indicator => &config.theme.indicator,
            Self::Badge => &config.theme.badge,
        }
    }

    pub(super) fn set(self, config: &mut Config, value: String) {
        match self {
            Self::ShelfTop => config.theme.shelf_top = value,
            Self::ShelfBottom => config.theme.shelf_bottom = value,
            Self::ShelfStroke => config.theme.shelf_stroke = value,
            Self::ShelfHighlight => config.theme.shelf_highlight = value,
            Self::Indicator => config.theme.indicator = value,
            Self::Badge => config.theme.badge = value,
        }
    }
}

pub(super) fn show_customizer_debug_window(
    state: &Rc<RefCell<Runtime>>,
    parent: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
) {
    let original = state.borrow().config.clone();
    state.borrow_mut().customizer_open = true;
    let draft = Rc::new(RefCell::new(original.clone()));
    let saved = Rc::new(Cell::new(false));
    let preview_revision = Rc::new(Cell::new(0_u64));

    let customizer = ApplicationWindow::builder()
        .title("OSDockX Customizer (Debug)")
        .transient_for(parent)
        .default_width(CUSTOMIZER_WIDTH)
        .default_height(CUSTOMIZER_HEIGHT)
        .resizable(true)
        .build();
    if let Some(app) = parent.application() {
        customizer.set_application(Some(&app));
    }
    customizer.add_css_class("osdock-customizer-window");

    let shell = GtkBox::new(Orientation::Vertical, 10);
    shell.add_css_class("osdock-context-menu");
    shell.set_margin_top(10);
    shell.set_margin_bottom(10);
    shell.set_margin_start(10);
    shell.set_margin_end(10);

    let title = Label::new(Some("Customizer (Debug)"));
    title.add_css_class("osdock-menu-title");
    title.set_xalign(0.0);
    shell.append(&title);

    let scrolled = ScrolledWindow::new();
    scrolled.set_policy(PolicyType::Never, PolicyType::Automatic);
    scrolled.set_vexpand(true);

    let controls = GtkBox::new(Orientation::Vertical, 8);
    controls.set_margin_start(8);
    controls.set_margin_end(8);
    controls.set_margin_bottom(8);

    let colors_title = Label::new(Some("Colors"));
    colors_title.add_css_class("osdock-menu-title");
    colors_title.set_xalign(0.0);
    controls.append(&colors_title);
    for field in [
        CustomizerColorField::ShelfTop,
        CustomizerColorField::ShelfBottom,
        CustomizerColorField::ShelfStroke,
        CustomizerColorField::ShelfHighlight,
        CustomizerColorField::Indicator,
        CustomizerColorField::Badge,
    ] {
        append_customizer_color_row(&controls, field, &draft, state, parent, drawing, gl_area);
    }

    controls.append(&context_menu_separator());
    let layout_title = Label::new(Some("Layout and Material"));
    layout_title.add_css_class("osdock-menu-title");
    layout_title.set_xalign(0.0);
    controls.append(&layout_title);
    for field in [
        CustomizerSliderField::IconSize,
        CustomizerSliderField::ZoomStrength,
        CustomizerSliderField::ShelfHeightRatio,
        CustomizerSliderField::ShelfSlantRatio,
        CustomizerSliderField::SideMarginRatio,
        CustomizerSliderField::ShelfHorizonRatio,
        CustomizerSliderField::FrontLipRatio,
        CustomizerSliderField::ReflectionOpacity,
        CustomizerSliderField::ReflectionHeight,
        CustomizerSliderField::ReflectionBandRatio,
        CustomizerSliderField::ReflectionBlur,
        CustomizerSliderField::Tilt,
        CustomizerSliderField::Depth,
        CustomizerSliderField::Bevel,
        CustomizerSliderField::FloorOpacity,
        CustomizerSliderField::ShadowStrength,
        CustomizerSliderField::HighlightStrength,
        CustomizerSliderField::MaterialRoughness,
        CustomizerSliderField::IconFloorOffset,
    ] {
        append_customizer_slider_row(
            &controls,
            field,
            &draft,
            &preview_revision,
            state,
            parent,
            drawing,
            gl_area,
        );
    }

    controls.append(&context_menu_separator());
    append_customizer_toggle_row(
        &controls,
        "Auto Hide",
        original.dock.autohide,
        &draft,
        &preview_revision,
        state,
        parent,
        drawing,
        gl_area,
        |config, value| config.dock.autohide = value,
    );
    append_customizer_toggle_row(
        &controls,
        "Reserve Screen Space",
        original.dock.reserve_space,
        &draft,
        &preview_revision,
        state,
        parent,
        drawing,
        gl_area,
        |config, value| config.dock.reserve_space = value,
    );

    scrolled.set_child(Some(&controls));
    shell.append(&scrolled);

    let footer = GtkBox::new(Orientation::Horizontal, 8);
    footer.set_halign(Align::End);
    let close_button = Button::with_label("Close");
    let save_button = Button::with_label("Save");
    footer.append(&close_button);
    footer.append(&save_button);
    shell.append(&footer);
    customizer.set_child(Some(&shell));

    {
        let state = Rc::clone(state);
        let parent = parent.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        let draft = Rc::clone(&draft);
        let saved = Rc::clone(&saved);
        let customizer = customizer.clone();
        save_button.connect_clicked(move |_| {
            saved.set(true);
            let config = draft.borrow().clone().normalized();
            apply_customizer_config(&state, &parent, &drawing, &gl_area, config, true);
            customizer.close();
        });
    }
    {
        let customizer = customizer.clone();
        close_button.connect_clicked(move |_| {
            customizer.close();
        });
    }
    {
        let state = Rc::clone(state);
        let parent = parent.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        let saved = Rc::clone(&saved);
        customizer.connect_close_request(move |_| {
            state.borrow_mut().customizer_open = false;
            if !saved.get() {
                apply_customizer_config(
                    &state,
                    &parent,
                    &drawing,
                    &gl_area,
                    original.clone(),
                    false,
                );
            }
            Propagation::Proceed
        });
    }

    customizer.present();
}

#[allow(clippy::too_many_arguments)]
fn append_customizer_color_row(
    controls: &GtkBox,
    field: CustomizerColorField,
    draft: &Rc<RefCell<Config>>,
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
) {
    let row = GtkBox::new(Orientation::Horizontal, 8);
    row.set_margin_start(8);
    row.set_margin_end(8);

    let label = Label::new(Some(field.label()));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    row.append(&label);

    let dialog = ColorDialog::builder()
        .title(field.label())
        .modal(true)
        .with_alpha(true)
        .build();
    let color_button = ColorDialogButton::new(Some(dialog));
    color_button.set_rgba(&rgba_from_config_color(field.value(&draft.borrow())));
    {
        let draft = Rc::clone(draft);
        let state = Rc::clone(state);
        let window = window.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        color_button.connect_rgba_notify(move |button| {
            field.set(&mut draft.borrow_mut(), rgba_to_config_color(button.rgba()));
            let config = draft.borrow().clone().normalized();
            apply_customizer_config(&state, &window, &drawing, &gl_area, config, false);
        });
    }
    row.append(&color_button);
    controls.append(&row);
}

#[allow(clippy::too_many_arguments)]
fn append_customizer_slider_row(
    controls: &GtkBox,
    field: CustomizerSliderField,
    draft: &Rc<RefCell<Config>>,
    preview_revision: &Rc<Cell<u64>>,
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
) {
    let row = GtkBox::new(Orientation::Vertical, 3);
    row.set_margin_start(8);
    row.set_margin_end(8);

    let label = Label::new(Some(field.label()));
    label.set_xalign(0.0);
    row.append(&label);

    let (min, max, step, digits) = field.range();
    let slider = Scale::with_range(Orientation::Horizontal, min, max, step);
    slider.set_draw_value(true);
    slider.set_digits(digits);
    slider.set_hexpand(true);
    slider.set_value(field.value(&draft.borrow()));
    {
        let draft = Rc::clone(draft);
        let state = Rc::clone(state);
        let window = window.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        let preview_revision = Rc::clone(preview_revision);
        slider.connect_value_changed(move |slider| {
            field.set(&mut draft.borrow_mut(), slider.value());
            schedule_customizer_preview(
                &state,
                &window,
                &drawing,
                &gl_area,
                &draft,
                &preview_revision,
            );
        });
    }
    row.append(&slider);
    controls.append(&row);
}

#[allow(clippy::too_many_arguments)]
fn append_customizer_toggle_row(
    controls: &GtkBox,
    label: &str,
    active: bool,
    draft: &Rc<RefCell<Config>>,
    preview_revision: &Rc<Cell<u64>>,
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    setter: fn(&mut Config, bool),
) {
    let check = CheckButton::with_label(label);
    check.set_active(active);
    check.set_margin_start(8);
    check.set_margin_end(8);
    {
        let draft = Rc::clone(draft);
        let state = Rc::clone(state);
        let window = window.clone();
        let drawing = drawing.clone();
        let gl_area = gl_area.clone();
        let preview_revision = Rc::clone(preview_revision);
        check.connect_toggled(move |check| {
            setter(&mut draft.borrow_mut(), check.is_active());
            schedule_customizer_preview(
                &state,
                &window,
                &drawing,
                &gl_area,
                &draft,
                &preview_revision,
            );
        });
    }
    controls.append(&check);
}

fn schedule_customizer_preview(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    draft: &Rc<RefCell<Config>>,
    preview_revision: &Rc<Cell<u64>>,
) {
    let revision = preview_revision.get().wrapping_add(1);
    preview_revision.set(revision);
    let state = Rc::clone(state);
    let window = window.clone();
    let drawing = drawing.clone();
    let gl_area = gl_area.clone();
    let draft = Rc::clone(draft);
    let preview_revision = Rc::clone(preview_revision);
    glib::timeout_add_local_once(CUSTOMIZER_PREVIEW_DEBOUNCE, move || {
        if preview_revision.get() != revision {
            return;
        }
        let config = draft.borrow().clone().normalized();
        apply_customizer_config(&state, &window, &drawing, &gl_area, config, false);
    });
}

fn apply_customizer_config(
    state: &Rc<RefCell<Runtime>>,
    window: &ApplicationWindow,
    drawing: &DrawingArea,
    gl_area: &GLArea,
    config: Config,
    save: bool,
) {
    {
        let mut state = state.borrow_mut();
        state.config = config.normalized();
        let (_, _, theme) = resolve_runtime_theme(state.composited, &state.config.theme);
        state.theme = theme;
        state.hidden = false;
        state.hover = None;
        state.last_size = None;
        state.last_geometry = None;
        state.last_reserved_geometry = None;
        state.last_shape_size = None;
        state.last_shape_label = None;
        if save {
            save_runtime_config(&state);
        }
        state.refresh_model();
    }
    ensure_icon_animation_if_needed(state, window, drawing, gl_area);
    sync_dock_window(state, window, drawing, gl_area, true);
    queue_gl_render_if_enabled(state, gl_area);
    drawing.queue_draw();
}

pub(super) fn rgba_from_config_color(value: &str) -> gdk::RGBA {
    let color = ThemeColor::parse(value).unwrap_or_else(|| ThemeColor::rgba(1.0, 1.0, 1.0, 1.0));
    gdk::RGBA::new(
        color.red as f32,
        color.green as f32,
        color.blue as f32,
        color.alpha as f32,
    )
}

pub(super) fn rgba_to_config_color(rgba: gdk::RGBA) -> String {
    let channel = |value: f32| -> u8 { (value.clamp(0.0, 1.0) * 255.0).round() as u8 };
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        channel(rgba.red()),
        channel(rgba.green()),
        channel(rgba.blue()),
        channel(rgba.alpha())
    )
}