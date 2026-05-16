use super::*;
use crate::config::{Config, DockConfig};
use crate::model::{DockItem, DockModel};
use crate::theme::Theme;
use gtk::cairo::Format;
use std::fs::File;

fn single_item_model() -> DockModel {
    DockModel {
        items: vec![DockItem {
            id: "test.desktop".to_string(),
            name: "Test".to_string(),
            desktop_id: Some("test.desktop".to_string()),
            startup_wm_class: None,
            icon_name: None,
            window_icon: None,
            pinned: true,
            windows: Vec::new(),
            active: false,
            urgent: false,
            badge: None,
        }],
    }
}

#[test]
fn renderer_paints_non_empty_surface() {
    let config = Config::default().normalized();
    let theme = Theme::from_config(&config.theme);
    let model = DockModel {
        items: vec![DockItem {
            id: "test.desktop".to_string(),
            name: "Test".to_string(),
            desktop_id: Some("test.desktop".to_string()),
            startup_wm_class: None,
            icon_name: None,
            window_icon: None,
            pinned: true,
            windows: Vec::new(),
            active: false,
            urgent: false,
            badge: None,
        }],
    };
    let size = Renderer::desired_size(&model, &DockConfig::default(), &theme, None);
    let mut surface = ImageSurface::create(Format::ARgb32, size.0, size.1).unwrap();
    let mut renderer = Renderer::new();

    renderer.draw_for_test(&surface, &model, &config.dock, &theme);

    let data = surface.data().unwrap();
    assert!(data.iter().any(|byte| *byte != 0));
}

#[test]
#[ignore = "writes a local PNG preview for renderer tuning"]
fn export_leopard_preview_png() {
    let config = Config::default().normalized();
    let theme = Theme::from_config(&config.theme);
    let model = DockModel {
        items: vec![
            preview_item("terminal.desktop", "Terminal", None),
            preview_item("browser.desktop", "Browser", None),
            preview_item("mail.desktop", "Mail", Some(1)),
            preview_item("calendar.desktop", "Calendar", None),
            preview_item("editor.desktop", "Editor", None),
            preview_item("notes.desktop", "Notes", None),
            preview_item("settings.desktop", "Settings", Some(2)),
        ],
    };
    let size = Renderer::desired_size(&model, &config.dock, &theme, None);
    let surface = ImageSurface::create(Format::ARgb32, size.0, size.1).unwrap();
    let mut renderer = Renderer::new();

    renderer.draw_for_test(&surface, &model, &config.dock, &theme);

    let output = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("most-recent-render.png");
    let mut file = File::create(output).unwrap();
    surface.write_to_png(&mut file).unwrap();
}

#[test]
fn reserved_thickness_stays_compact_for_leopard_theme() {
    let config = Config::default().normalized();
    let theme = Theme::from_config(&config.theme);
    let model = DockModel::default();

    let reserved = Renderer::reserved_thickness(&model, &config.dock, &theme);

    assert!(reserved < config.dock.icon_size + 40);
}

#[test]
fn hover_starts_when_pointer_reaches_visible_icon_edge() {
    let config = Config::default().normalized();
    let theme = Theme::from_config(&config.theme);
    let model = single_item_model();
    let layout = Renderer::layout_for(&model, &config.dock, &theme, None);
    let icon = layout.icons[0].rect;
    let point = Point {
        x: icon.x + 0.5,
        y: icon.y + icon.height * 0.5,
    };

    assert_eq!(
        Renderer::hover_point_for(&model, &config.dock, &theme, point, false),
        Some(point)
    );
}

#[test]
fn hover_retains_when_pointer_stays_on_live_magnified_icon() {
    let config = Config::default().normalized();
    let theme = Theme::from_config(&config.theme);
    let model = single_item_model();
    let rest_layout = Renderer::layout_for(&model, &config.dock, &theme, None);
    let rest_icon = rest_layout.icons[0].rect;
    let point = Point {
        x: rest_icon.center_x() + rest_icon.width * 0.625,
        y: rest_icon.y + rest_icon.height * 0.5,
    };
    let live_layout = Renderer::layout_for(&model, &config.dock, &theme, Some(point));
    let live_icon = live_layout.icons[0].rect;
    let input_regions = Renderer::input_regions(&model, &config.dock, &theme, Some(point));

    assert!(live_icon.contains(point));
    assert!(!center_ratio_rect(rest_icon, ICON_HOVER_RETAIN_RATIO).contains(point));
    assert_eq!(
        Renderer::hover_point_for(&model, &config.dock, &theme, point, true),
        Some(point)
    );
    assert!(input_regions.iter().any(|region| region.contains(point)));
}

#[test]
fn leopard_plank_has_transparent_top_corner_and_visible_front_body() {
    let config = Config::default().normalized();
    let theme = Theme::from_config(&config.theme);
    let mut surface = ImageSurface::create(Format::ARgb32, 240, 110).unwrap();
    let cr = Context::new(&surface).unwrap();
    let shelf = Rect {
        x: 24.0,
        y: 20.0,
        width: 192.0,
        height: 48.0,
    };

    draw_procedural_shelf_layer(&cr, &shelf, &theme);
    drop(cr);

    assert_eq!(alpha_at(&mut surface, 25, 21), 0);
    assert!(alpha_at(&mut surface, 120, 22) > 0);
    assert!(alpha_at(&mut surface, 120, 34) > 40);
    assert!(alpha_at(&mut surface, 120, 66) > 120);
    assert!(brightness_at(&mut surface, 120, 37) > brightness_at(&mut surface, 120, 25));
    assert!(alpha_at(&mut surface, 120, 66) > alpha_at(&mut surface, 120, 40));
}

#[test]
fn leopard_icon_reflections_stay_above_lip() {
    let config = Config::default().normalized();
    let theme = Theme::from_config(&config.theme);
    let model = DockModel {
        items: vec![DockItem {
            id: "test.desktop".to_string(),
            name: "Test".to_string(),
            desktop_id: Some("test.desktop".to_string()),
            startup_wm_class: None,
            icon_name: None,
            window_icon: None,
            pinned: true,
            windows: Vec::new(),
            active: false,
            urgent: false,
            badge: None,
        }],
    };
    let layout = Renderer::layout_for(&model, &config.dock, &theme, None);
    let geom = compute_perspective_shelf_geometry(&layout.shelf, &theme);
    let mut icons = IconCache::disabled();
    let mut surface = ImageSurface::create(Format::ARgb32, layout.size.0, layout.size.1).unwrap();
    let cr = Context::new(&surface).unwrap();

    draw_icon_reflections_on_shelf(&cr, &model, &layout, &theme, &mut icons);
    drop(cr);

    assert!(rect_has_alpha(
        &mut surface,
        Rect {
            x: layout.icons[0].rect.x,
            y: layout.icons[0].rect.y + layout.icons[0].rect.height,
            width: layout.icons[0].rect.width,
            height: (geom.lip_y - (layout.icons[0].rect.y + layout.icons[0].rect.height) - 2.0)
                .max(1.0),
        }
    ));
    assert!(!rect_has_alpha(
        &mut surface,
        Rect {
            x: layout.shelf.x,
            y: geom.lip_y + 1.0,
            width: layout.shelf.width,
            height: (layout.shelf.y + layout.shelf.height - geom.lip_y - 1.0).max(1.0),
        }
    ));
}

#[test]
fn leopard_reflections_are_visible_at_icon_bottom() {
    let config = Config::default().normalized();
    let theme = Theme::from_config(&config.theme);
    let model = DockModel {
        items: vec![DockItem {
            id: "test.desktop".to_string(),
            name: "Test".to_string(),
            desktop_id: Some("test.desktop".to_string()),
            startup_wm_class: None,
            icon_name: None,
            window_icon: None,
            pinned: true,
            windows: Vec::new(),
            active: false,
            urgent: false,
            badge: None,
        }],
    };
    let layout = Renderer::layout_for(&model, &config.dock, &theme, None);
    let mut icons = IconCache::disabled();
    let mut surface = ImageSurface::create(Format::ARgb32, layout.size.0, layout.size.1).unwrap();
    let cr = Context::new(&surface).unwrap();
    let icon = layout.icons[0].rect;

    draw_icon_reflections_on_shelf(&cr, &model, &layout, &theme, &mut icons);
    drop(cr);

    assert!(rect_has_alpha(
        &mut surface,
        Rect {
            x: icon.x,
            y: icon.y + icon.height,
            width: icon.width,
            height: (icon.height * 0.10).max(2.0),
        }
    ));
}

#[test]
fn leopard_default_layout_has_visible_floor_below_icons() {
    let config = Config::default().normalized();
    let theme = Theme::from_config(&config.theme);
    let model = DockModel {
        items: vec![DockItem {
            id: "test.desktop".to_string(),
            name: "Test".to_string(),
            desktop_id: Some("test.desktop".to_string()),
            startup_wm_class: None,
            icon_name: None,
            window_icon: None,
            pinned: true,
            windows: Vec::new(),
            active: false,
            urgent: false,
            badge: None,
        }],
    };

    let layout = Renderer::layout_for(&model, &config.dock, &theme, None);
    let geom = compute_perspective_shelf_geometry(&layout.shelf, &theme);
    let icon = layout.icons[0].rect;
    let floor_depth = geom.lip_y - (icon.y + icon.height);

    assert!(floor_depth > icon.height * 0.08);
    assert!(floor_depth < icon.height * 0.23);
}

#[test]
fn leopard_default_layout_has_visible_front_body_thickness() {
    let config = Config::default().normalized();
    let theme = Theme::from_config(&config.theme);
    let model = DockModel {
        items: vec![DockItem {
            id: "test.desktop".to_string(),
            name: "Test".to_string(),
            desktop_id: Some("test.desktop".to_string()),
            startup_wm_class: None,
            icon_name: None,
            window_icon: None,
            pinned: true,
            windows: Vec::new(),
            active: false,
            urgent: false,
            badge: None,
        }],
    };

    let layout = Renderer::layout_for(&model, &config.dock, &theme, None);
    let geom = compute_perspective_shelf_geometry(&layout.shelf, &theme);
    let front_body = geom.bottom_y - geom.lip_y;
    let (_, active_led_height) = leopard_running_indicator_size(true);
    let fascia_to_led_ratio = front_body / active_led_height;

    assert!(fascia_to_led_ratio > 1.10);
    assert!(fascia_to_led_ratio < 1.32);
}

#[test]
fn leopard_front_face_is_inset_for_closed_side_caps() {
    let config = Config::default().normalized();
    let theme = Theme::from_config(&config.theme);
    let model = DockModel {
        items: vec![DockItem {
            id: "test.desktop".to_string(),
            name: "Test".to_string(),
            desktop_id: Some("test.desktop".to_string()),
            startup_wm_class: None,
            icon_name: None,
            window_icon: None,
            pinned: true,
            windows: Vec::new(),
            active: false,
            urgent: false,
            badge: None,
        }],
    };

    let layout = Renderer::layout_for(&model, &config.dock, &theme, None);
    let geom = compute_perspective_shelf_geometry(&layout.shelf, &theme);
    let body = leopard_wedge_body_geometry(&layout.shelf, &theme);
    let cap_width = geom.front_left.x - geom.lip_left.x;

    assert!(geom.front_left.x > geom.lip_left.x);
    assert!(geom.front_right.x < geom.lip_right.x);
    assert!(cap_width > layout.icons[0].rect.height * 0.010);
    assert!(cap_width < layout.icons[0].rect.height * 0.06);
    assert!((body.face_left_bottom.x - geom.front_left.x).abs() < 0.001);
    assert!((body.face_right_bottom.x - geom.front_right.x).abs() < 0.001);
    assert!((body.face_left_bottom.y - geom.bottom_y).abs() < 0.001);
}

#[test]
fn leopard_front_body_reaches_outer_corner_join() {
    let config = Config::default().normalized();
    let theme = Theme::from_config(&config.theme);
    let mut surface = ImageSurface::create(Format::ARgb32, 240, 110).unwrap();
    let cr = Context::new(&surface).unwrap();
    let shelf = Rect {
        x: 24.0,
        y: 20.0,
        width: 192.0,
        height: 48.0,
    };
    let geom = compute_perspective_shelf_geometry(&shelf, &theme);

    draw_procedural_shelf_layer(&cr, &shelf, &theme);
    drop(cr);

    assert!(
        alpha_at(
            &mut surface,
            (geom.lip_left.x + 1.5).round() as i32,
            (geom.lip_y + (geom.bottom_y - geom.lip_y) * 0.45).round() as i32,
        ) > 0
    );
    assert!(
        alpha_at(
            &mut surface,
            (geom.lip_right.x - 1.5).round() as i32,
            (geom.lip_y + (geom.bottom_y - geom.lip_y) * 0.45).round() as i32,
        ) > 0
    );
}

#[test]
fn leopard_default_layout_has_raised_rear_edge() {
    let config = Config::default().normalized();
    let theme = Theme::from_config(&config.theme);
    let model = DockModel {
        items: vec![DockItem {
            id: "test.desktop".to_string(),
            name: "Test".to_string(),
            desktop_id: Some("test.desktop".to_string()),
            startup_wm_class: None,
            icon_name: None,
            window_icon: None,
            pinned: true,
            windows: Vec::new(),
            active: false,
            urgent: false,
            badge: None,
        }],
    };

    let layout = Renderer::layout_for(&model, &config.dock, &theme, None);
    let geom = compute_perspective_shelf_geometry(&layout.shelf, &theme);
    let icon = layout.icons[0].rect;
    let rear_rise = (icon.y + icon.height) - geom.back_left.y;

    assert!(rear_rise > icon.height * 0.44);
}

#[test]
fn leopard_reflection_is_clipped_to_reflection_band() {
    let config = Config::default().normalized();
    let theme = Theme::from_config(&config.theme);
    let model = DockModel {
        items: vec![DockItem {
            id: "test.desktop".to_string(),
            name: "Test".to_string(),
            desktop_id: Some("test.desktop".to_string()),
            startup_wm_class: None,
            icon_name: None,
            window_icon: None,
            pinned: true,
            windows: Vec::new(),
            active: false,
            urgent: false,
            badge: None,
        }],
    };
    let layout = Renderer::layout_for(&model, &config.dock, &theme, None);
    let mut icons = IconCache::disabled();
    let mut icon_surface = render_icon_surface(&model, &layout, &mut icons).unwrap();
    let mut surface = ImageSurface::create(Format::ARgb32, layout.size.0, layout.size.1).unwrap();
    let cr = Context::new(&surface).unwrap();
    let reflection = shelf_plane_reflection_rect(&layout, &theme);

    draw_shelf_plane_reflections(&cr, &layout, &theme, &mut icon_surface);
    drop(cr);

    assert!(rect_has_alpha(&mut surface, reflection));
    assert!(!rect_has_alpha(
        &mut surface,
        Rect {
            x: reflection.x,
            y: reflection.y + reflection.height + 1.0,
            width: reflection.width,
            height: 3.0,
        }
    ));
}

#[test]
fn leopard_running_indicator_lands_inside_front_body() {
    let config = Config::default().normalized();
    let theme = Theme::from_config(&config.theme);
    let shelf = Rect {
        x: 18.0,
        y: 36.0,
        width: 164.0,
        height: 30.0,
    };
    let icon = Rect {
        x: 66.0,
        y: 2.0,
        width: 64.0,
        height: 64.0,
    };
    let layout = DockLayout {
        icons: Vec::new(),
        label: None,
        shelf,
        sections: Vec::new(),
        separator: None,
        size: (200, 80),
    };
    let mut surface = ImageSurface::create(Format::ARgb32, 200, 80).unwrap();
    let cr = Context::new(&surface).unwrap();
    let y = leopard_running_indicator_center_y(&shelf, &theme);
    let geom = compute_perspective_shelf_geometry(&shelf, &theme);

    draw_leopard_running_indicator(&cr, icon, &layout, &theme, true);
    drop(cr);

    assert!(y > geom.lip_y);
    assert!(y < shelf.y + shelf.height);
    assert!(alpha_at(&mut surface, icon.center_x().round() as i32, y.round() as i32) > 0);
}

#[test]
fn leopard_active_indicator_lands_below_shelf() {
    let config = Config::default().normalized();
    let theme = Theme::from_config(&config.theme);
    let shelf = Rect {
        x: 18.0,
        y: 36.0,
        width: 164.0,
        height: 30.0,
    };
    let icon = Rect {
        x: 66.0,
        y: 2.0,
        width: 64.0,
        height: 64.0,
    };
    let layout = DockLayout {
        icons: Vec::new(),
        label: None,
        shelf,
        sections: Vec::new(),
        separator: None,
        size: (200, 80),
    };
    let mut surface = ImageSurface::create(Format::ARgb32, 200, 80).unwrap();
    let cr = Context::new(&surface).unwrap();
    let y = leopard_active_indicator_center_y(icon, &shelf, &theme);
    let running_y = leopard_running_indicator_center_y(&shelf, &theme);

    draw_leopard_active_indicator(&cr, icon, &layout, &theme);
    drop(cr);

    assert!(y > shelf.y + shelf.height);
    assert!(y > icon.y + icon.height + 6.5);
    assert!(y > running_y + 7.0);
    assert!(alpha_at(&mut surface, icon.center_x().round() as i32, y.round() as i32) > 0);
}

fn alpha_at(surface: &mut ImageSurface, x: i32, y: i32) -> u8 {
    surface.flush();
    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    data[y as usize * stride + x as usize * 4 + 3]
}

fn brightness_at(surface: &mut ImageSurface, x: i32, y: i32) -> u16 {
    surface.flush();
    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    let offset = y as usize * stride + x as usize * 4;
    u16::from(data[offset]) + u16::from(data[offset + 1]) + u16::from(data[offset + 2])
}

fn rect_has_alpha(surface: &mut ImageSurface, rect: Rect) -> bool {
    surface.flush();
    let stride = surface.stride() as usize;
    let width = surface.width().max(0);
    let height = surface.height().max(0);
    let min_x = rect.x.max(0.0).floor() as i32;
    let max_x = (rect.x + rect.width).min(width as f64).ceil() as i32;
    let min_y = rect.y.max(0.0).floor() as i32;
    let max_y = (rect.y + rect.height).min(height as f64).ceil() as i32;
    let data = surface.data().unwrap();
    (min_y..max_y).any(|y| {
        (min_x..max_x).any(|x| {
            let offset = y as usize * stride + x as usize * 4 + 3;
            data[offset] != 0
        })
    })
}

fn preview_item(id: &str, name: &str, badge: Option<u32>) -> DockItem {
    DockItem {
        id: id.to_string(),
        name: name.to_string(),
        desktop_id: Some(id.to_string()),
        startup_wm_class: None,
        icon_name: None,
        window_icon: None,
        pinned: true,
        windows: Vec::new(),
        active: false,
        urgent: false,
        badge,
    }
}