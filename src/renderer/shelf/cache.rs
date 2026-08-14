use super::{
    draw_front_lip, draw_glass_highlight_overlay, draw_glass_shelf_base,
    draw_leopard_shelf_strokes, draw_shelf_section_separator,
};
use crate::layout::{DockLayout, Rect};
use crate::theme::{Color, Theme};
use gtk::cairo::{Context, Format, ImageSurface};
use std::path::PathBuf;

#[derive(Debug, Default)]
pub(in crate::renderer) struct ProceduralShelfCache {
    current: Option<CachedShelfLayers>,
    builds: u64,
    hits: u64,
}

#[derive(Debug)]
struct CachedShelfLayers {
    key: ShelfCacheKey,
    back: ImageSurface,
    front: ImageSurface,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ShelfCacheKey {
    container_pixels: (i32, i32),
    shelf: PhysicalRect,
    separator: Option<PhysicalRect>,
    scale_x_bits: u64,
    scale_y_bits: u64,
    theme: ShelfThemeKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PhysicalRect {
    x: i64,
    y: i64,
    width: i64,
    height: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ShelfThemeKey {
    id: String,
    renderer: u8,
    colors: [[u64; 4]; 4],
    values: [u64; 12],
    assets: [Option<PathBuf>; 5],
}

impl ProceduralShelfCache {
    pub(in crate::renderer) fn layers(
        &mut self,
        layout: &DockLayout,
        theme: &Theme,
        device_scale: (f64, f64),
    ) -> Option<(ImageSurface, ImageSurface)> {
        let key = ShelfCacheKey::new(layout, theme, device_scale);
        if let Some(cached) = self.current.as_ref().filter(|cached| cached.key == key) {
            self.hits = self.hits.saturating_add(1);
            crate::perf::record_shelf_hit();
            return Some((cached.back.clone(), cached.front.clone()));
        }

        let back = create_layer(key.container_pixels, device_scale)?;
        let front = create_layer(key.container_pixels, device_scale)?;
        let back_cr = Context::new(&back).ok()?;
        draw_glass_shelf_base(&back_cr, &layout.shelf, theme);
        drop(back_cr);
        back.flush();

        let front_cr = Context::new(&front).ok()?;
        draw_glass_highlight_overlay(&front_cr, &layout.shelf, theme);
        draw_front_lip(&front_cr, &layout.shelf, theme);
        draw_leopard_shelf_strokes(&front_cr, &layout.shelf, theme);
        if let Some(separator) = layout.separator.as_ref() {
            draw_shelf_section_separator(&front_cr, &layout.shelf, separator, theme);
        }
        drop(front_cr);
        front.flush();

        crate::perf::record_shelf_build();
        self.builds = self.builds.saturating_add(1);
        self.current = Some(CachedShelfLayers {
            key,
            back: back.clone(),
            front: front.clone(),
        });
        Some((back, front))
    }

    #[cfg(test)]
    pub(in crate::renderer) fn test_stats(&self) -> (u64, u64) {
        (self.builds, self.hits)
    }
}

impl ShelfCacheKey {
    fn new(layout: &DockLayout, theme: &Theme, device_scale: (f64, f64)) -> Self {
        let scale_x = device_scale.0.max(1.0);
        let scale_y = device_scale.1.max(1.0);
        Self {
            container_pixels: (
                (layout.size.0 as f64 * scale_x).ceil().max(1.0) as i32,
                (layout.size.1 as f64 * scale_y).ceil().max(1.0) as i32,
            ),
            shelf: PhysicalRect::new(layout.shelf, scale_x, scale_y),
            separator: layout
                .separator
                .map(|separator| PhysicalRect::new(separator.rect, scale_x, scale_y)),
            scale_x_bits: scale_x.to_bits(),
            scale_y_bits: scale_y.to_bits(),
            theme: ShelfThemeKey::new(theme),
        }
    }
}

impl PhysicalRect {
    fn new(rect: Rect, scale_x: f64, scale_y: f64) -> Self {
        Self {
            x: (rect.x * scale_x).round() as i64,
            y: (rect.y * scale_y).round() as i64,
            width: (rect.width * scale_x).round() as i64,
            height: (rect.height * scale_y).round() as i64,
        }
    }
}

impl ShelfThemeKey {
    fn new(theme: &Theme) -> Self {
        Self {
            id: theme.id.clone(),
            renderer: theme.renderer as u8,
            colors: [
                color_bits(theme.shelf_top),
                color_bits(theme.shelf_bottom),
                color_bits(theme.shelf_stroke),
                color_bits(theme.shelf_highlight),
            ],
            values: [
                theme.shelf_height_ratio.to_bits(),
                theme.shelf_slant_ratio.to_bits(),
                theme.side_margin_ratio.to_bits(),
                theme.shelf_horizon_ratio.to_bits(),
                theme.front_lip_ratio.to_bits(),
                theme.tilt.to_bits(),
                theme.depth.to_bits(),
                theme.bevel.to_bits(),
                theme.floor_opacity.to_bits(),
                theme.shadow_strength.to_bits(),
                theme.highlight_strength.to_bits(),
                theme.material_roughness.to_bits(),
            ],
            assets: [
                theme.assets.shelf_texture.clone(),
                theme.assets.shelf_overlay.clone(),
                theme.assets.noise_texture.clone(),
                theme.assets.normal_map.clone(),
                theme.assets.fallback_texture.clone(),
            ],
        }
    }
}

fn color_bits(color: Color) -> [u64; 4] {
    [
        color.red.to_bits(),
        color.green.to_bits(),
        color.blue.to_bits(),
        color.alpha.to_bits(),
    ]
}

fn create_layer(
    physical_size: (i32, i32),
    device_scale: (f64, f64),
) -> Option<ImageSurface> {
    let surface = ImageSurface::create(
        Format::ARgb32,
        physical_size.0.max(1),
        physical_size.1.max(1),
    )
    .ok()?;
    surface.set_device_scale(device_scale.0.max(1.0), device_scale.1.max(1.0));
    Some(surface)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::model::{DockItem, DockModel};
    use crate::renderer::Renderer;

    fn layout_and_theme() -> (DockLayout, Theme) {
        let mut config = Config::default().normalized();
        config.dock.icon_size = 64;
        let theme = Theme::from_config(&config.theme);
        let model = DockModel {
            items: vec![DockItem {
                id: "cache.desktop".to_string(),
                name: "Cache".to_string(),
                desktop_id: Some("cache.desktop".to_string()),
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
        (Renderer::layout_for(&model, &config.dock, &theme, None), theme)
    }

    #[test]
    fn key_changes_for_geometry_scale_theme_and_separator() {
        let (layout, theme) = layout_and_theme();
        let base = ShelfCacheKey::new(&layout, &theme, (1.0, 1.0));

        let mut geometry = layout.clone();
        geometry.shelf.width += 1.0;
        assert_ne!(base, ShelfCacheKey::new(&geometry, &theme, (1.0, 1.0)));
        assert_ne!(base, ShelfCacheKey::new(&layout, &theme, (2.0, 2.0)));

        let mut changed_theme = theme.clone();
        changed_theme.highlight_strength += 0.01;
        assert_ne!(
            base,
            ShelfCacheKey::new(&layout, &changed_theme, (1.0, 1.0))
        );

        let mut separator = layout.clone();
        separator.separator.as_mut().unwrap().rect.x += 1.0;
        assert_ne!(base, ShelfCacheKey::new(&separator, &theme, (1.0, 1.0)));
    }
}
