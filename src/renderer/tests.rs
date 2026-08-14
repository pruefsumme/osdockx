use super::*;
use crate::config::{Config, DockConfig};
use crate::model::{DockItem, DockModel};
use crate::model::WindowIcon;
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

fn deterministic_fixture_model(count: usize) -> DockModel {
    DockModel {
        items: (0..count)
            .map(|index| DockItem {
                id: format!("fixture-{index}.desktop"),
                name: format!("Fixture {index}"),
                desktop_id: Some(format!("fixture-{index}.desktop")),
                startup_wm_class: None,
                icon_name: None,
                window_icon: Some(deterministic_fixture_icon(index)),
                pinned: true,
                windows: Vec::new(),
                active: index % 4 == 0,
                urgent: index == 11,
                badge: (index == 7).then_some(3),
            })
            .collect(),
    }
}

fn deterministic_fixture_icon(seed: usize) -> WindowIcon {
    let size = 16_u32;
    let pixels = (0..size * size)
        .map(|offset| {
            let x = offset % size;
            let y = offset / size;
            let red = ((x * 13 + seed as u32 * 29) & 0xff) as u8;
            let green = ((y * 17 + seed as u32 * 11) & 0xff) as u8;
            let blue = (((x + y) * 9 + seed as u32 * 7) & 0xff) as u8;
            0xff00_0000 | u32::from(red) << 16 | u32::from(green) << 8 | u32::from(blue)
        })
        .collect();
    WindowIcon::from_argb(size, size, pixels)
}

fn deterministic_render_hash(hover_index: Option<usize>, scale: i32) -> u64 {
    let mut config = Config::default().normalized();
    config.dock.icon_size = 64;
    let theme = Theme::from_config(&config.theme);
    let model = deterministic_fixture_model(15);
    let rest = Renderer::layout_for(&model, &config.dock, &theme, None);
    let hover = hover_index.map(|index| Point {
        x: rest.icons[index].rect.center_x(),
        y: rest.icons[index].rect.y + rest.icons[index].rect.height * 0.5,
    });
    let layout = Renderer::layout_for(&model, &config.dock, &theme, hover);
    let mut surface = ImageSurface::create(
        Format::ARgb32,
        layout.size.0 * scale,
        layout.size.1 * scale,
    )
    .unwrap();
    surface.set_device_scale(scale as f64, scale as f64);
    let cr = Context::new(&surface).unwrap();
    let mut renderer = Renderer::new();
    let mut icons = IconCache::disabled();
    renderer.draw(&cr, &model, &config.dock, &theme, hover, &mut icons);
    drop(cr);
    surface.flush();

    let label = layout.label.map(|label| label.rect);
    let stride = surface.stride() as usize;
    let width = surface.width() as usize;
    let height = surface.height() as usize;
    let data = surface.data().unwrap();
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for y in 0..height {
        for x in 0..width {
            let css_x = x as f64 / scale as f64;
            let css_y = y as f64 / scale as f64;
            if label.is_some_and(|rect| {
                css_x >= rect.x - 3.0
                    && css_x <= rect.x + rect.width + 3.0
                    && css_y >= rect.y - 3.0
                    && css_y <= rect.y + rect.height + 3.0
            }) {
                continue;
            }
            for byte in &data[y * stride + x * 4..y * stride + x * 4 + 4] {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }
    hash
}

#[test]
fn deterministic_rest_and_hover_renders_match_golden_hashes() {
    let hashes = [
        deterministic_render_hash(None, 1),
        deterministic_render_hash(Some(7), 1),
        deterministic_render_hash(None, 2),
        deterministic_render_hash(Some(7), 2),
    ];
    assert_eq!(
        hashes,
        [
            16_098_008_435_556_604_288,
            2_165_020_988_576_021_856,
            9_880_531_414_050_164_059,
            10_299_436_036_222_587_131,
        ]
    );
}

#[test]
fn warm_frames_reuse_completed_reflection_composites() {
    let mut config = Config::default().normalized();
    config.dock.icon_size = 64;
    let theme = Theme::from_config(&config.theme);
    let model = deterministic_fixture_model(15);
    let size = Renderer::desired_size(&model, &config.dock, &theme, None);
    let surface = ImageSurface::create(Format::ARgb32, size.0, size.1).unwrap();
    let cr = Context::new(&surface).unwrap();
    let mut renderer = Renderer::new();
    let mut icons = IconCache::new();

    renderer.draw(&cr, &model, &config.dock, &theme, None, &mut icons);
    let first = icons.test_surface_counts();
    renderer.draw(&cr, &model, &config.dock, &theme, None, &mut icons);
    let second = icons.test_surface_counts();

    assert!(first.1 >= model.items.len());
    assert_eq!(first.1, second.1);
}

#[test]
fn reflection_theme_changes_build_distinct_composites() {
    let mut config = Config::default().normalized();
    config.dock.icon_size = 64;
    let theme = Theme::from_config(&config.theme);
    let mut changed_theme = theme.clone();
    changed_theme.reflection_blur += 0.05;
    let model = deterministic_fixture_model(1);
    let size = Renderer::desired_size(&model, &config.dock, &theme, None);
    let surface = ImageSurface::create(Format::ARgb32, size.0, size.1).unwrap();
    let cr = Context::new(&surface).unwrap();
    let mut renderer = Renderer::new();
    let mut icons = IconCache::new();

    renderer.draw(&cr, &model, &config.dock, &theme, None, &mut icons);
    let initial = icons.test_surface_counts().1;
    renderer.draw(
        &cr,
        &model,
        &config.dock,
        &changed_theme,
        None,
        &mut icons,
    );

    assert!(icons.test_surface_counts().1 > initial);
}

#[test]
fn translucent_presence_icons_bypass_reflection_composites() {
    let mut config = Config::default().normalized();
    config.dock.icon_size = 64;
    let theme = Theme::from_config(&config.theme);
    let model = deterministic_fixture_model(1);
    let layout = Renderer::layout_for(&model, &config.dock, &theme, None);
    let mut resolved = resolve_icons(&model, &layout, None, None, None);
    resolved[0].alpha = 0.5;
    let surface = ImageSurface::create(Format::ARgb32, layout.size.0, layout.size.1).unwrap();
    let cr = Context::new(&surface).unwrap();
    let mut icons = IconCache::new();

    draw_icon_reflections_on_shelf(&cr, &resolved, &layout, &theme, &mut icons);

    assert_eq!(icons.test_surface_counts().1, 0);
}

#[test]
fn cached_rasters_and_reflections_match_immediate_rendering() {
    let mut config = Config::default().normalized();
    config.dock.icon_size = 64;
    let theme = Theme::from_config(&config.theme);
    let model = deterministic_fixture_model(4);
    let size = Renderer::desired_size(&model, &config.dock, &theme, None);
    let mut immediate = ImageSurface::create(Format::ARgb32, size.0, size.1).unwrap();
    let mut cached = ImageSurface::create(Format::ARgb32, size.0, size.1).unwrap();

    let immediate_cr = Context::new(&immediate).unwrap();
    let mut renderer = Renderer::new();
    renderer.draw(
        &immediate_cr,
        &model,
        &config.dock,
        &theme,
        None,
        &mut IconCache::disabled(),
    );
    drop(immediate_cr);
    immediate.flush();

    let cached_cr = Context::new(&cached).unwrap();
    let mut renderer = Renderer::new();
    renderer.draw(
        &cached_cr,
        &model,
        &config.dock,
        &theme,
        None,
        &mut IconCache::new(),
    );
    drop(cached_cr);
    cached.flush();

    let immediate_data = immediate.data().unwrap();
    let cached_data = cached.data().unwrap();
    let (max_index, max_difference) = immediate_data
        .iter()
        .zip(cached_data.iter())
        .enumerate()
        .map(|(index, (left, right))| (index, left.abs_diff(*right)))
        .max_by_key(|(_, difference)| *difference)
        .unwrap_or_default();
    let signed_range = immediate_data
        .iter()
        .zip(cached_data.iter())
        .map(|(left, right)| i16::from(*right) - i16::from(*left))
        .fold((0_i16, 0_i16), |(min, max), value| {
            (min.min(value), max.max(value))
        });
    assert!(
        max_difference <= 4,
        "maximum channel delta was {max_difference} at byte {max_index}: {} versus {}; signed range {signed_range:?}",
        immediate_data[max_index], cached_data[max_index]
    );
}

#[test]
fn warm_frames_reuse_static_shelf_layers() {
    let mut config = Config::default().normalized();
    config.dock.icon_size = 64;
    let theme = Theme::from_config(&config.theme);
    let model = deterministic_fixture_model(4);
    let size = Renderer::desired_size(&model, &config.dock, &theme, None);
    let surface = ImageSurface::create(Format::ARgb32, size.0, size.1).unwrap();
    let cr = Context::new(&surface).unwrap();
    let mut renderer = Renderer::new();
    let mut icons = IconCache::new();

    renderer.draw(&cr, &model, &config.dock, &theme, None, &mut icons);
    assert_eq!(renderer.shelf_cache.test_stats(), (1, 0));
    renderer.draw(&cr, &model, &config.dock, &theme, None, &mut icons);
    assert_eq!(renderer.shelf_cache.test_stats(), (1, 1));
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
#[ignore = "writes a local PNG shelf-only preview for renderer tuning"]
fn export_leopard_shelf_only_preview_png() {
    let config = Config::default().normalized();
    let theme = Theme::from_config(&config.theme);
    let surface = ImageSurface::create(Format::ARgb32, 1295, 192).unwrap();
    let cr = Context::new(&surface).unwrap();
    let shelf = Rect {
        x: 2.0,
        y: 96.0,
        width: 1291.0,
        height: 56.0,
    };

    draw_procedural_shelf_layer(&cr, &shelf, &theme);
    drop(cr);

    let output = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("most-recent-shelf-only.png");
    let mut file = File::create(output).unwrap();
    surface.write_to_png(&mut file).unwrap();
}

#[test]
fn leopard_shelf_only_silhouette_matches_reference_shape() {
    let config = Config::default().normalized();
    let theme = Theme::from_config(&config.theme);
    let mut surface = ImageSurface::create(Format::ARgb32, 1295, 192).unwrap();
    let cr = Context::new(&surface).unwrap();
    let shelf = Rect {
        x: 2.0,
        y: 96.0,
        width: 1291.0,
        height: 56.0,
    };

    draw_procedural_shelf_layer(&cr, &shelf, &theme);
    drop(cr);

    let (min_x, min_y, max_x, max_y) = alpha_bounds(&mut surface).unwrap();
    assert_eq!((min_x, min_y, max_x), (2, 96, 1292));
    assert!((150..=151).contains(&max_y));

    assert_row_span_near(&mut surface, 98, 28, 1264, 2);
    assert_row_span_near(&mut surface, 120, 15, 1278, 2);
    assert_row_span_near(&mut surface, 145, 2, 1292, 2);
    assert_row_span_near(&mut surface, 150, 5, 1289, 2);
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
fn layout_for_container_centers_smaller_dock() {
    let config = Config::default().normalized();
    let theme = Theme::from_config(&config.theme);
    let model = DockModel {
        items: vec![
            preview_item("a.desktop", "A", None),
            preview_item("b.desktop", "B", None),
        ],
    };
    let layout = Renderer::layout_for(&model, &config.dock, &theme, None);
    let container = (layout.size.0 + 64, layout.size.1 + 10);
    let centered =
        Renderer::layout_for_container(&model, &config.dock, &theme, None, Some(container));

    assert_eq!(centered.size, container);
    assert!((centered.shelf.x - layout.shelf.x - 32.0).abs() < 0.01);
    assert!((centered.icons[0].rect.x - layout.icons[0].rect.x - 32.0).abs() < 0.01);
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
    let top = color_at(&mut surface, 120, 25);
    let lower = color_at(&mut surface, 120, 37);
    assert!(top.0 > top.2);
    assert!(brightness(top) < brightness(lower));
    assert!(alpha_at(&mut surface, 120, 66) > 120);
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

    let resolved_icons = resolve_icons(&model, &layout, None, None, None);

    draw_icon_reflections_on_shelf(&cr, &resolved_icons, &layout, &theme, &mut icons);
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

    let resolved_icons = resolve_icons(&model, &layout, None, None, None);

    draw_icon_reflections_on_shelf(&cr, &resolved_icons, &layout, &theme, &mut icons);
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

    assert!(floor_depth > icon.height * 0.18);
    assert!(floor_depth < icon.height * 0.30);
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
    assert!(fascia_to_led_ratio < 2.30);
}

#[test]
fn leopard_front_face_narrows_into_ground_edge() {
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
    assert!((geom.front_left.x - geom.lip_left.x).abs() < 0.001);
    assert!((geom.front_right.x - geom.lip_right.x).abs() < 0.001);
    assert!(body.face_left_bottom.x > geom.front_left.x);
    assert!(body.face_right_bottom.x < geom.front_right.x);
    assert!(body.face_left_bottom.x - geom.front_left.x >= layout.shelf.height * 0.030);
    assert!(geom.front_right.x - body.face_right_bottom.x >= layout.shelf.height * 0.030);
    assert!((body.face_left_bottom.y - geom.bottom_y).abs() < 0.001);
}

#[test]
fn leopard_front_face_uses_glass_corner_radius() {
    let config = Config::default().normalized();
    let theme = Theme::from_config(&config.theme);
    let shelf = Rect {
        x: 24.0,
        y: 20.0,
        width: 192.0,
        height: 48.0,
    };

    let geom = compute_perspective_shelf_geometry(&shelf, &theme);
    let glass_radius = super::shelf::leopard_glass_plane_front_corner_radius(&shelf, &geom);
    let front_face_height = geom.bottom_y - geom.lip_y;

    assert!(glass_radius < front_face_height);
    assert!(glass_radius > front_face_height * 0.35);
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
fn leopard_front_lip_stays_inside_trapezoid_bounds() {
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
    let body = leopard_wedge_body_geometry(&shelf, &theme);
    let glass_radius = super::shelf::leopard_glass_plane_front_corner_radius(&shelf, &geom);

    draw_front_lip(&cr, &shelf, &theme);
    drop(cr);

    let y = (geom.lip_y + (geom.bottom_y - geom.lip_y) * 0.50).round() as i32;
    assert_eq!(
        alpha_at(
            &mut surface,
            (body.face_left_bottom.x - 4.0).round() as i32,
            y
        ),
        0
    );
    assert_eq!(
        alpha_at(
            &mut surface,
            (body.face_right_bottom.x + 4.0).round() as i32,
            y
        ),
        0
    );
    assert!(
        alpha_at(
            &mut surface,
            (geom.lip_left.x + glass_radius).round() as i32,
            y
        ) > 0
    );
    assert!(
        alpha_at(
            &mut surface,
            (geom.lip_right.x - glass_radius).round() as i32,
            y
        ) > 0
    );
    assert!(
        alpha_at(
            &mut surface,
            (body.face_left_join.x + (body.face_left_inner_bottom.x - body.face_left_join.x) * 0.35)
                .round() as i32,
            y
        ) > 0
    );
    assert!(
        alpha_at(
            &mut surface,
            (body.face_right_join.x
                - (body.face_right_join.x - body.face_right_inner_bottom.x) * 0.35)
                .round() as i32,
            y
        ) > 0
    );
}

#[test]
fn leopard_front_corners_scan_without_spiky_alpha() {
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

    let body = leopard_wedge_body_geometry(&shelf, &theme);
    let start_y = (geom.lip_y + 1.0).round() as i32;
    let end_y = (geom.bottom_y - 1.0).round() as i32;
    let face_height = geom.bottom_y - geom.lip_y;

    for y in start_y..=end_y {
        let t = ((f64::from(y) - geom.lip_y) / face_height).clamp(0.0, 1.0);
        let left_edge = geom.lip_left.x + (body.face_left_bottom.x - geom.lip_left.x) * t;
        let right_edge = geom.lip_right.x + (body.face_right_bottom.x - geom.lip_right.x) * t;
        let left_outside = (left_edge - 3.0).round() as i32;
        let right_outside = (right_edge + 3.0).round() as i32;

        assert!(alpha_at(&mut surface, left_outside, y) <= 40);
        assert!(alpha_at(&mut surface, right_outside, y) <= 40);
    }

    let mid_y = (geom.lip_y + (geom.bottom_y - geom.lip_y) * 0.72).round() as i32;
    let t = ((f64::from(mid_y) - geom.lip_y) / face_height).clamp(0.0, 1.0);
    let left_edge = geom.lip_left.x + (body.face_left_bottom.x - geom.lip_left.x) * t;
    let right_edge = geom.lip_right.x + (body.face_right_bottom.x - geom.lip_right.x) * t;
    let left_inside = (left_edge + 3.0).round() as i32;
    let right_inside = (right_edge - 3.0).round() as i32;
    assert!(alpha_at(&mut surface, left_inside, mid_y) > 70);
    assert!(alpha_at(&mut surface, right_inside, mid_y) > 70);
}

#[test]
fn leopard_front_corners_do_not_leave_bright_lip_beads() {
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

    let body = leopard_wedge_body_geometry(&shelf, &theme);
    let y = (geom.bottom_y - 1.0).round() as i32;
    let left_bead = color_at(
        &mut surface,
        (body.face_left_bottom.x + 1.0).round() as i32,
        y,
    );
    let right_bead = color_at(
        &mut surface,
        (body.face_right_bottom.x - 1.0).round() as i32,
        y,
    );
    let left_body = color_at(
        &mut surface,
        (body.face_left_bottom.x + 9.0).round() as i32,
        y,
    );
    let right_body = color_at(
        &mut surface,
        (body.face_right_bottom.x - 9.0).round() as i32,
        y,
    );

    assert!(brightness(left_bead) <= brightness(left_body) + 28);
    assert!(brightness(right_bead) <= brightness(right_body) + 28);
    assert!(alpha_at(&mut surface, (geom.lip_left.x + 1.0).round() as i32, y) <= 96);
    assert!(alpha_at(&mut surface, (geom.lip_right.x - 1.0).round() as i32, y) <= 96);
    assert!(
        alpha_at(
            &mut surface,
            (body.face_left_bottom.x + 1.0).round() as i32,
            y
        ) > 40
    );
    assert!(
        alpha_at(
            &mut surface,
            (body.face_right_bottom.x - 1.0).round() as i32,
            y
        ) > 40
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

    assert!(rear_rise > icon.height * 0.34);
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
    let resolved_icons = resolve_icons(&model, &layout, None, None, None);
    let mut icon_surface = render_icon_surface(&resolved_icons, &layout, &mut icons).unwrap();
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

    draw_leopard_running_indicator(&cr, icon, &layout, &theme, true, 1.0);
    drop(cr);

    assert!(y > geom.lip_y);
    assert!(y < shelf.y + shelf.height);
    assert!(
        alpha_at(
            &mut surface,
            icon.center_x().round() as i32,
            y.round() as i32
        ) > 0
    );
}

#[test]
fn leopard_active_indicator_lands_inside_front_body() {
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
    let geom = compute_perspective_shelf_geometry(&shelf, &theme);

    draw_leopard_active_indicator(&cr, icon, &layout, &theme, 1.0);
    drop(cr);

    assert!(y > geom.lip_y);
    assert!(y < shelf.y + shelf.height);
    assert!(
        alpha_at(
            &mut surface,
            icon.center_x().round() as i32,
            y.round() as i32
        ) > 0
    );
}

#[test]
fn leopard_active_indicator_has_larger_glow_than_core() {
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
    let y = leopard_active_indicator_center_y(icon, &shelf, &theme).round() as i32;
    let center_x = icon.center_x().round() as i32;

    draw_leopard_active_indicator(&cr, icon, &layout, &theme, 1.0);
    drop(cr);

    let core = alpha_at(&mut surface, center_x, y);
    let glow = alpha_at(&mut surface, center_x - 16, y);

    assert!(core > 220);
    assert!(glow > 0);
    assert!(glow < core);
}

#[test]
fn leopard_active_indicator_is_bright_white_blue_lip_light() {
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
    let geom = compute_perspective_shelf_geometry(&shelf, &theme);

    draw_leopard_active_indicator(&cr, icon, &layout, &theme, 1.0);
    drop(cr);

    let center_x = icon.center_x().round() as i32;
    let center_y = leopard_active_indicator_center_y(icon, &shelf, &theme).round() as i32;
    let face_top = geom.lip_y.ceil() as i32;
    let face_bottom = geom.bottom_y.floor() as i32;
    let core_alpha = alpha_at(&mut surface, center_x, center_y);
    let top_alpha = alpha_at(&mut surface, center_x, face_top);
    let bottom_alpha = alpha_at(&mut surface, center_x, face_bottom);
    let color = color_at(&mut surface, center_x, center_y);

    assert!(core_alpha > 180);
    assert!(top_alpha > 80);
    assert!(bottom_alpha < core_alpha);
    assert!(color.0 > 220);
    assert!(color.1 > 190);
    assert!(color.2 > 150);
    assert!(color.0 > color.2);
}

#[test]
fn leopard_separator_paints_deeper_theme_aware_trench() {
    let config = Config::default().normalized();
    let theme = Theme::from_config(&config.theme);
    let shelf = Rect {
        x: 18.0,
        y: 28.0,
        width: 180.0,
        height: 40.0,
    };
    let separator = crate::layout::DockSeparatorLayout {
        rect: Rect {
            x: 98.0,
            y: shelf.y,
            width: 8.0,
            height: shelf.height,
        },
    };
    let geom = compute_perspective_shelf_geometry(&shelf, &theme);
    let mut surface = ImageSurface::create(Format::ARgb32, 220, 90).unwrap();
    let cr = Context::new(&surface).unwrap();

    draw_procedural_shelf_layer(&cr, &shelf, &theme);
    draw_shelf_section_separator(&cr, &shelf, &separator, &theme);
    drop(cr);

    let sample_y = (geom.back_left.y + (geom.lip_y - geom.back_left.y) * 0.54).round() as i32;
    let lip_y = (geom.lip_y + (geom.bottom_y - geom.lip_y) * 0.56).round() as i32;
    let center_x = separator.rect.center_x().round() as i32;
    let trench = color_at(&mut surface, center_x, sample_y);
    let panel = color_at(&mut surface, center_x + 8, sample_y);
    let lip = color_at(&mut surface, center_x, lip_y);
    let lip_panel = color_at(&mut surface, center_x + 8, lip_y);

    assert!(alpha_at(&mut surface, center_x, sample_y) > 0);
    assert!(brightness(trench) + 4 < brightness(panel));
    assert!((i32::from(brightness(lip)) - i32::from(brightness(lip_panel))).abs() <= 4);
}

fn alpha_at(surface: &mut ImageSurface, x: i32, y: i32) -> u8 {
    surface.flush();
    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    data[y as usize * stride + x as usize * 4 + 3]
}

fn color_at(surface: &mut ImageSurface, x: i32, y: i32) -> (u8, u8, u8, u8) {
    surface.flush();
    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    let offset = y as usize * stride + x as usize * 4;
    (
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    )
}

fn brightness(color: (u8, u8, u8, u8)) -> u16 {
    u16::from(color.0) + u16::from(color.1) + u16::from(color.2)
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

fn alpha_bounds(surface: &mut ImageSurface) -> Option<(i32, i32, i32, i32)> {
    surface.flush();
    let stride = surface.stride() as usize;
    let width = surface.width().max(0);
    let height = surface.height().max(0);
    let data = surface.data().unwrap();
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = -1;
    let mut max_y = -1;

    for y in 0..height {
        for x in 0..width {
            let offset = y as usize * stride + x as usize * 4 + 3;
            if data[offset] == 0 {
                continue;
            }
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }

    (max_x >= min_x && max_y >= min_y).then_some((min_x, min_y, max_x, max_y))
}

fn assert_row_span_near(
    surface: &mut ImageSurface,
    y: i32,
    expected_min_x: i32,
    expected_max_x: i32,
    tolerance: i32,
) {
    surface.flush();
    let stride = surface.stride() as usize;
    let width = surface.width().max(0);
    let data = surface.data().unwrap();
    let mut min_x = width;
    let mut max_x = -1;

    for x in 0..width {
        let offset = y as usize * stride + x as usize * 4 + 3;
        if data[offset] == 0 {
            continue;
        }
        min_x = min_x.min(x);
        max_x = max_x.max(x);
    }

    assert!(
        (min_x - expected_min_x).abs() <= tolerance,
        "row {y} min_x {min_x} differed from {expected_min_x}"
    );
    assert!(
        (max_x - expected_max_x).abs() <= tolerance,
        "row {y} max_x {max_x} differed from {expected_max_x}"
    );
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
