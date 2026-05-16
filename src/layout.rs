use crate::model::{DockModel, DockSectionKind};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IconLayout {
    pub item_index: usize,
    pub rect: Rect,
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DockSectionLayout {
    pub kind: DockSectionKind,
    pub rect: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DockSeparatorLayout {
    pub rect: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LabelLayout {
    pub item_index: usize,
    pub rect: Rect,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DockLayout {
    pub icons: Vec<IconLayout>,
    pub label: Option<LabelLayout>,
    pub shelf: Rect,
    pub sections: Vec<DockSectionLayout>,
    pub separator: Option<DockSeparatorLayout>,
    pub size: (i32, i32),
}

impl DockLayout {
    pub fn hit_test(&self, point: Point) -> Option<usize> {
        self.icons
            .iter()
            .find(|icon| icon.rect.contains(point))
            .map(|icon| icon.item_index)
    }

    pub fn section(&self, kind: DockSectionKind) -> Option<&DockSectionLayout> {
        self.sections.iter().find(|section| section.kind == kind)
    }
}

impl Rect {
    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.x
            && point.x <= self.x + self.width
            && point.y >= self.y
            && point.y <= self.y + self.height
    }

    pub fn center_x(&self) -> f64 {
        self.x + self.width / 2.0
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LayoutParams {
    pub icon_size: f64,
    pub zoom_strength: f64,
    pub gap: f64,
    pub reflection_height: f64,
    pub shelf_height: f64,
    pub side_margin: f64,
    pub shelf_horizon_ratio: f64,
    pub icon_floor_offset: f64,
    pub label_height: f64,
}

pub fn compute_layout(model: &DockModel, hover: Option<Point>, params: LayoutParams) -> DockLayout {
    let sections = model.sections();
    let application_indices = &sections.applications.item_indices;
    let applet_indices = &sections.applets.item_indices;
    let application_count = application_indices.len();
    let applet_count = applet_indices.len();
    let max_scale = 1.0 + params.zoom_strength;
    let influence = params.icon_size * 2.3;
    let rest_step = params.icon_size + params.gap;
    let applications_width = occupied_width(application_count, rest_step, params.gap);
    let separator_slot_width = separator_slot_width(params);
    let applet_width = applet_section_width(applet_count, rest_step, params.gap);
    let content_width = applications_width + separator_slot_width + applet_width;
    let zoom_padding = params.icon_size * params.zoom_strength * 0.40 + 8.0;
    let padding = params.side_margin.max(zoom_padding);
    let width = content_width + padding * 2.0;
    let label_band = params.label_height + 8.0;
    let top_padding = 5.0;
    let baseline_y = top_padding + label_band + params.icon_size * (max_scale - 1.0);
    let icon_bottom = baseline_y + params.icon_size;
    let shelf_y =
        icon_bottom + params.icon_floor_offset - params.shelf_height * params.shelf_horizon_ratio;
    let height = (shelf_y + params.shelf_height + 5.0).max(64.0);
    let shelf_overhang = params.side_margin * 0.78;
    let separator_section_x = padding + applications_width;
    let applets_x = separator_section_x + separator_slot_width;

    let mut icon_slots = application_indices
        .iter()
        .enumerate()
        .map(|(slot, item_index)| {
            (
                *item_index,
                padding + params.icon_size / 2.0 + slot as f64 * rest_step,
            )
        })
        .collect::<Vec<_>>();
    icon_slots.extend(applet_indices.iter().enumerate().map(|(slot, item_index)| {
        (
            *item_index,
            applets_x + params.icon_size / 2.0 + slot as f64 * rest_step,
        )
    }));
    let scales = icon_slots
        .iter()
        .map(|(_, center)| {
            hover
                .map(|point| magnification(point.x, *center, influence, params.zoom_strength))
                .unwrap_or(1.0)
        })
        .collect::<Vec<_>>();

    let icons = layout_icons_on_floor_plane(&icon_slots, &scales, params.icon_size, icon_bottom);

    let label = hover.and_then(|point| {
        icons
            .iter()
            .find(|icon| point.x >= icon.rect.x && point.x <= icon.rect.x + icon.rect.width)
            .filter(|icon| {
                model
                    .items
                    .get(icon.item_index)
                    .is_some_and(|item| item.is_application())
            })
            .map(|icon| LabelLayout {
                item_index: icon.item_index,
                rect: Rect {
                    x: (icon.rect.center_x() - params.icon_size * 0.9).max(2.0),
                    y: 3.0,
                    width: params.icon_size * 1.8,
                    height: params.label_height,
                },
            })
    });

    let sections = vec![
        DockSectionLayout {
            kind: DockSectionKind::Applications,
            rect: Rect {
                x: padding,
                y: 0.0,
                width: applications_width,
                height,
            },
        },
        DockSectionLayout {
            kind: DockSectionKind::Separator,
            rect: Rect {
                x: separator_section_x,
                y: 0.0,
                width: separator_slot_width,
                height,
            },
        },
        DockSectionLayout {
            kind: DockSectionKind::Applets,
            rect: Rect {
                x: applets_x,
                y: 0.0,
                width: applet_width,
                height,
            },
        },
    ];

    DockLayout {
        icons,
        label,
        shelf: Rect {
            x: (padding - shelf_overhang).max(0.0),
            y: shelf_y,
            width: content_width + shelf_overhang * 2.0,
            height: params.shelf_height,
        },
        sections,
        separator: Some(DockSeparatorLayout {
            rect: separator_rect(separator_section_x, shelf_y, params),
        }),
        size: (width.ceil() as i32, height.ceil() as i32),
    }
}

fn occupied_width(count: usize, rest_step: f64, gap: f64) -> f64 {
    if count == 0 {
        0.0
    } else {
        rest_step * count as f64 - gap
    }
}

fn separator_slot_width(params: LayoutParams) -> f64 {
    (params.icon_size * 0.08 + params.gap * 0.55).max(12.0)
}

fn applet_section_width(count: usize, rest_step: f64, gap: f64) -> f64 {
    if count == 0 {
        0.0
    } else {
        occupied_width(count, rest_step, gap)
    }
}

fn separator_rect(section_x: f64, shelf_y: f64, params: LayoutParams) -> Rect {
    let slot_width = separator_slot_width(params);
    let groove_width = (params.icon_size * 0.072).clamp(4.0, 5.5);
    let groove_height = (params.shelf_height * 1.02).max(18.0);
    Rect {
        x: section_x + slot_width * 0.50 - groove_width / 2.0,
        y: shelf_y - params.shelf_height * 0.10,
        width: groove_width,
        height: groove_height,
    }
}

fn layout_icons_on_floor_plane(
    slots: &[(usize, f64)],
    scales: &[f64],
    icon_size: f64,
    floor_y: f64,
) -> Vec<IconLayout> {
    slots
        .iter()
        .zip(scales)
        .map(|((item_index, rest_center), scale)| {
            let size = icon_size * scale;
            IconLayout {
                item_index: *item_index,
                rect: Rect {
                    x: *rest_center - size / 2.0,
                    y: floor_y - size,
                    width: size,
                    height: size,
                },
                scale: *scale,
            }
        })
        .collect()
}

fn magnification(pointer_x: f64, center_x: f64, influence: f64, zoom_strength: f64) -> f64 {
    let distance = (pointer_x - center_x).abs();
    if distance >= influence {
        return 1.0;
    }
    let t = 1.0 - distance / influence;
    1.0 + zoom_strength * (0.5 - 0.5 * (std::f64::consts::PI * t).cos())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DockItem, DockModel, DockSectionKind};

    fn item(id: &str) -> DockItem {
        DockItem {
            id: id.to_string(),
            name: id.to_string(),
            desktop_id: Some(id.to_string()),
            startup_wm_class: None,
            icon_name: None,
            window_icon: None,
            pinned: true,
            windows: Vec::new(),
            active: false,
            urgent: false,
            badge: None,
        }
    }

    fn downloads_applet() -> DockItem {
        DockItem::downloads_applet()
    }

    fn trash_applet() -> DockItem {
        DockItem::trash_applet()
    }

    #[test]
    fn hovered_icon_magnifies_more_than_distant_icons() {
        let model = DockModel {
            items: vec![item("a"), item("b"), item("c")],
        };
        let params = LayoutParams {
            icon_size: 64.0,
            zoom_strength: 0.75,
            gap: 10.0,
            reflection_height: 24.0,
            shelf_height: 28.0,
            side_margin: 38.0,
            shelf_horizon_ratio: 0.50,
            icon_floor_offset: 0.0,
            label_height: 24.0,
        };
        let rest = compute_layout(&model, None, params);
        let hover = compute_layout(
            &model,
            Some(Point {
                x: rest.icons[1].rect.center_x(),
                y: 20.0,
            }),
            params,
        );

        assert!(hover.icons[1].scale > hover.icons[0].scale);
        assert!(hover.icons[1].scale > 1.2);
    }

    #[test]
    fn layout_stays_compact_with_pixel_reflection_param() {
        let model = DockModel {
            items: vec![item("a"), item("b"), item("c"), item("d")],
        };
        let params = LayoutParams {
            icon_size: 64.0,
            zoom_strength: 0.72,
            gap: 8.0,
            reflection_height: 27.0,
            shelf_height: 22.0,
            side_margin: 38.0,
            shelf_horizon_ratio: 0.50,
            icon_floor_offset: 0.0,
            label_height: 24.0,
        };
        let layout = compute_layout(&model, Some(Point { x: 160.0, y: 40.0 }), params);

        assert!(layout.size.1 < 190);
        assert!(layout.shelf.y > layout.icons[0].rect.y);
    }

    #[test]
    fn shelf_horizon_uses_icon_floor_offset_and_extends_past_icons() {
        let model = DockModel {
            items: vec![item("a"), item("b")],
        };
        let params = LayoutParams {
            icon_size: 64.0,
            zoom_strength: 0.72,
            gap: 8.0,
            reflection_height: 27.0,
            shelf_height: 24.0,
            side_margin: 64.0 * 0.74,
            shelf_horizon_ratio: 0.48,
            icon_floor_offset: 64.0 * 0.05,
            label_height: 24.0,
        };

        let layout = compute_layout(&model, None, params);
        let horizon_y = layout.shelf.y + layout.shelf.height * params.shelf_horizon_ratio;
        let icon_bottom = layout.icons[0].rect.y + layout.icons[0].rect.height;
        let last_icon = layout.icons.last().unwrap();

        assert!((horizon_y - icon_bottom - params.icon_floor_offset).abs() < 0.001);
        assert!(layout.shelf.x < layout.icons[0].rect.x);
        assert!(layout.shelf.x + layout.shelf.width > last_icon.rect.x + last_icon.rect.width);
    }

    #[test]
    fn shelf_adds_side_overhang_without_changing_icon_spacing() {
        let model = DockModel {
            items: vec![item("a"), item("b"), item("c")],
        };
        let params = LayoutParams {
            icon_size: 64.0,
            zoom_strength: 0.72,
            gap: 8.0,
            reflection_height: 27.0,
            shelf_height: 24.0,
            side_margin: 64.0 * 0.82,
            shelf_horizon_ratio: 0.50,
            icon_floor_offset: 0.0,
            label_height: 24.0,
        };

        let layout = compute_layout(&model, None, params);
        let first_icon = &layout.icons[0].rect;
        let last_icon = &layout.icons[2].rect;
        let left_overhang = first_icon.x - layout.shelf.x;
        let right_overhang = (layout.shelf.x + layout.shelf.width) - (last_icon.x + last_icon.width);
        let icon_spacing = layout.icons[1].rect.center_x() - layout.icons[0].rect.center_x();
        let applets = layout
            .section(DockSectionKind::Applets)
            .expect("applets section");

        assert!(left_overhang > params.side_margin * 0.70);
        assert!(right_overhang > left_overhang);
        assert!(right_overhang > left_overhang + applets.rect.width * 0.40);
        assert!((icon_spacing - (params.icon_size + params.gap)).abs() < 0.001);
    }

    #[test]
    fn separator_sits_after_application_icons() {
        let model = DockModel {
            items: vec![item("a"), item("b"), item("c")],
        };
        let params = LayoutParams {
            icon_size: 64.0,
            zoom_strength: 0.72,
            gap: 8.0,
            reflection_height: 27.0,
            shelf_height: 24.0,
            side_margin: 64.0 * 0.82,
            shelf_horizon_ratio: 0.50,
            icon_floor_offset: 0.0,
            label_height: 24.0,
        };

        let layout = compute_layout(&model, None, params);
        let separator = layout.separator.expect("separator layout");
        let last_icon = layout
            .icons
            .iter()
            .filter(|icon| model.items[icon.item_index].is_application())
            .last()
            .expect("app icons");

        assert!(separator.rect.x > last_icon.rect.x + last_icon.rect.width);
        assert_eq!(
            layout.section(DockSectionKind::Separator).map(|section| section.kind),
            Some(DockSectionKind::Separator)
        );
    }

    #[test]
    fn applets_layout_to_right_of_separator() {
        let model = DockModel {
            items: vec![item("a"), item("b"), downloads_applet(), trash_applet()],
        };
        let params = LayoutParams {
            icon_size: 64.0,
            zoom_strength: 0.72,
            gap: 8.0,
            reflection_height: 27.0,
            shelf_height: 24.0,
            side_margin: 64.0 * 0.82,
            shelf_horizon_ratio: 0.50,
            icon_floor_offset: 0.0,
            label_height: 24.0,
        };

        let layout = compute_layout(&model, None, params);
        let separator = layout.separator.expect("separator layout");
        let first_applet = layout
            .icons
            .iter()
            .find(|icon| model.items[icon.item_index].is_downloads_applet())
            .expect("downloads applet");
        let second_applet = layout
            .icons
            .iter()
            .find(|icon| model.items[icon.item_index].is_trash_applet())
            .expect("trash applet");
        let applets = layout
            .section(DockSectionKind::Applets)
            .expect("applets section");

        assert!(applets.rect.width > 0.0);
        assert!(first_applet.rect.x > separator.rect.x + separator.rect.width);
        assert!(second_applet.rect.x > first_applet.rect.x);
    }

    #[test]
    fn layout_reserves_future_applet_section() {
        let model = DockModel {
            items: vec![item("a"), item("b")],
        };
        let params = LayoutParams {
            icon_size: 64.0,
            zoom_strength: 0.72,
            gap: 8.0,
            reflection_height: 27.0,
            shelf_height: 24.0,
            side_margin: 64.0 * 0.82,
            shelf_horizon_ratio: 0.50,
            icon_floor_offset: 0.0,
            label_height: 24.0,
        };

        let layout = compute_layout(&model, None, params);
        let applications = layout
            .section(DockSectionKind::Applications)
            .expect("applications section");
        let applets = layout
            .section(DockSectionKind::Applets)
            .expect("applets section");

        assert_eq!(
            layout.sections.iter().map(|section| section.kind).collect::<Vec<_>>(),
            vec![
                DockSectionKind::Applications,
                DockSectionKind::Separator,
                DockSectionKind::Applets,
            ]
        );
        assert_eq!(applets.rect.width, 0.0);
        assert!(applets.rect.x >= applications.rect.x + applications.rect.width);
    }

    #[test]
    fn empty_applet_section_only_adds_small_trailing_reserve() {
        let model = DockModel {
            items: vec![item("a"), item("b"), item("c")],
        };
        let params = LayoutParams {
            icon_size: 64.0,
            zoom_strength: 0.72,
            gap: 8.0,
            reflection_height: 27.0,
            shelf_height: 24.0,
            side_margin: 64.0 * 0.82,
            shelf_horizon_ratio: 0.50,
            icon_floor_offset: 0.0,
            label_height: 24.0,
        };

        let layout = compute_layout(&model, None, params);
        let separator = layout.separator.expect("separator layout");
        let applets = layout
            .section(DockSectionKind::Applets)
            .expect("applets section");
        let first_icon = layout.icons.first().expect("first app icon");
        let last_icon = layout.icons.last().expect("app icons");
        let right_overhang =
            (layout.shelf.x + layout.shelf.width) - (last_icon.rect.x + last_icon.rect.width);
        let left_overhang = first_icon.rect.x - layout.shelf.x;

        assert!(separator.rect.x > last_icon.rect.x + last_icon.rect.width);
        assert_eq!(applets.rect.width, 0.0);
        assert!(separator.rect.x < applets.rect.x + 8.0);
        assert!((right_overhang - left_overhang - separator_slot_width(params)).abs() < 0.001);
    }
}
