use crate::model::DockModel;

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
pub struct LabelLayout {
    pub item_index: usize,
    pub rect: Rect,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DockLayout {
    pub icons: Vec<IconLayout>,
    pub label: Option<LabelLayout>,
    pub shelf: Rect,
    pub size: (i32, i32),
}

impl DockLayout {
    pub fn hit_test(&self, point: Point) -> Option<usize> {
        self.icons
            .iter()
            .find(|icon| icon.rect.contains(point))
            .map(|icon| icon.item_index)
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
    pub label_height: f64,
}

pub fn compute_layout(model: &DockModel, hover: Option<Point>, params: LayoutParams) -> DockLayout {
    let count = model.items.len();
    if count == 0 {
        let height = (params.shelf_height + 10.0).ceil() as i32;
        return DockLayout {
            icons: Vec::new(),
            label: None,
            shelf: Rect {
                x: 8.0,
                y: 5.0,
                width: 180.0,
                height: params.shelf_height,
            },
            size: (196, height.max(64)),
        };
    }

    let max_scale = 1.0 + params.zoom_strength;
    let influence = params.icon_size * 2.3;
    let rest_step = params.icon_size + params.gap;
    let content_width = rest_step * count as f64 - params.gap;
    let padding = params.icon_size * max_scale * 0.30 + 12.0;
    let width = content_width + padding * 2.0;

    let centers = (0..count)
        .map(|index| padding + params.icon_size / 2.0 + index as f64 * rest_step)
        .collect::<Vec<_>>();
    let scales = centers
        .iter()
        .map(|center| {
            hover
                .map(|point| magnification(point.x, *center, influence, params.zoom_strength))
                .unwrap_or(1.0)
        })
        .collect::<Vec<_>>();
    let label_band = params.label_height + 8.0;
    let top_padding = 5.0;
    let baseline_y = top_padding + label_band + params.icon_size * (max_scale - 1.0);
    let icon_bottom = baseline_y + params.icon_size;
    let shelf_y = icon_bottom - params.icon_size * 0.12;
    let height = shelf_y + params.shelf_height + 5.0;

    let mut icons = Vec::with_capacity(count);
    for (index, (rest_center, scale)) in centers.iter().zip(scales).enumerate() {
        let size = params.icon_size * scale;
        icons.push(IconLayout {
            item_index: index,
            rect: Rect {
                x: *rest_center - size / 2.0,
                y: baseline_y + params.icon_size - size,
                width: size,
                height: size,
            },
            scale,
        });
    }

    let label = hover.and_then(|point| {
        icons
            .iter()
            .find(|icon| point.x >= icon.rect.x && point.x <= icon.rect.x + icon.rect.width)
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

    DockLayout {
        icons,
        label,
        shelf: Rect {
            x: 10.0,
            y: shelf_y,
            width: width - 20.0,
            height: params.shelf_height,
        },
        size: (width.ceil() as i32, height.ceil() as i32),
    }
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
    use crate::model::{DockItem, DockModel};

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
            label_height: 24.0,
        };
        let layout = compute_layout(&model, Some(Point { x: 160.0, y: 40.0 }), params);

        assert!(layout.size.1 < 190);
        assert!(layout.shelf.y > layout.icons[0].rect.y);
    }
}
