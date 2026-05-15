use crate::config::{RenderMode, ShelfStyle, ThemeConfig};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub alpha: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub id: String,
    pub renderer: RenderMode,
    pub shelf_style: ShelfStyle,
    pub shelf_top: Color,
    pub shelf_bottom: Color,
    pub shelf_stroke: Color,
    pub shelf_highlight: Color,
    pub indicator: Color,
    pub badge: Color,
    pub reflection_opacity: f64,
    pub reflection_height: f64,
    pub shelf_height_ratio: f64,
    pub shelf_slant_ratio: f64,
    pub icon_gap_ratio: f64,
    pub side_margin_ratio: f64,
    pub shelf_horizon_ratio: f64,
    pub front_lip_ratio: f64,
    pub reflection_band_ratio: f64,
    pub tilt: f64,
    pub depth: f64,
    pub bevel: f64,
    pub floor_opacity: f64,
    pub shadow_strength: f64,
    pub highlight_strength: f64,
    pub reflection_blur: f64,
    pub material_roughness: f64,
    pub icon_floor_offset: f64,
    pub assets: ThemeAssets,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThemeAssets {
    pub shelf_texture: Option<PathBuf>,
    pub shelf_overlay: Option<PathBuf>,
    pub noise_texture: Option<PathBuf>,
    pub normal_map: Option<PathBuf>,
    pub fallback_texture: Option<PathBuf>,
}

impl Theme {
    pub fn from_config(config: &ThemeConfig) -> Self {
        Self {
            id: config.preset.clone(),
            renderer: config.renderer.unwrap_or(RenderMode::Procedural2d),
            shelf_style: config.shelf_style,
            shelf_top: Color::parse(&config.shelf_top).unwrap_or(Color::rgba(0.97, 0.99, 1.0, 1.0)),
            shelf_bottom: Color::parse(&config.shelf_bottom)
                .unwrap_or(Color::rgba(0.47, 0.56, 0.66, 0.86)),
            shelf_stroke: Color::parse(&config.shelf_stroke)
                .unwrap_or(Color::rgba(0.18, 0.25, 0.33, 0.8)),
            shelf_highlight: Color::parse(&config.shelf_highlight)
                .unwrap_or(Color::rgba(1.0, 1.0, 1.0, 1.0)),
            indicator: Color::parse(&config.indicator).unwrap_or(Color::rgba(0.49, 0.84, 1.0, 1.0)),
            badge: Color::parse(&config.badge).unwrap_or(Color::rgba(0.89, 0.13, 0.18, 1.0)),
            reflection_opacity: config.reflection_opacity,
            reflection_height: config.reflection_height,
            shelf_height_ratio: config.shelf_height_ratio,
            shelf_slant_ratio: config.shelf_slant_ratio,
            icon_gap_ratio: config.icon_gap_ratio,
            side_margin_ratio: config.side_margin_ratio,
            shelf_horizon_ratio: config.shelf_horizon_ratio,
            front_lip_ratio: config.front_lip_ratio,
            reflection_band_ratio: config.reflection_band_ratio,
            tilt: config.tilt,
            depth: config.depth,
            bevel: config.bevel,
            floor_opacity: config.floor_opacity,
            shadow_strength: config.shadow_strength,
            highlight_strength: config.highlight_strength,
            reflection_blur: config.reflection_blur,
            material_roughness: config.material_roughness,
            icon_floor_offset: config.icon_floor_offset,
            assets: ThemeAssets {
                shelf_texture: config.shelf_texture.as_ref().map(PathBuf::from),
                shelf_overlay: config.shelf_overlay.as_ref().map(PathBuf::from),
                noise_texture: config.noise_texture.as_ref().map(PathBuf::from),
                normal_map: config.normal_map.as_ref().map(PathBuf::from),
                fallback_texture: config.fallback_texture.as_ref().map(PathBuf::from),
            },
        }
    }

    pub fn opaque_fallback(mut self) -> Self {
        self.shelf_top = self.shelf_top.with_alpha(1.0);
        self.shelf_bottom = self.shelf_bottom.with_alpha(1.0);
        self.shelf_stroke = self.shelf_stroke.with_alpha(1.0);
        self.shelf_highlight = self.shelf_highlight.with_alpha(0.92);
        self.reflection_opacity = self.reflection_opacity.min(0.18);
        self.renderer = RenderMode::Procedural2d;
        self
    }

    pub fn with_assets(mut self, assets: ThemeAssets) -> Self {
        self.assets = assets;
        self
    }

    pub fn with_renderer(mut self, renderer: RenderMode) -> Self {
        self.renderer = renderer;
        self
    }
}

impl Color {
    pub const fn rgba(red: f64, green: f64, blue: f64, alpha: f64) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        let hex = value.trim().strip_prefix('#').unwrap_or(value.trim());
        if hex.len() != 6 && hex.len() != 8 {
            return None;
        }

        let red = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let green = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let blue = u8::from_str_radix(&hex[4..6], 16).ok()?;
        let alpha = if hex.len() == 8 {
            u8::from_str_radix(&hex[6..8], 16).ok()?
        } else {
            255
        };

        Some(Self::rgba(
            red as f64 / 255.0,
            green as f64 / 255.0,
            blue as f64 / 255.0,
            alpha as f64 / 255.0,
        ))
    }

    pub fn with_alpha(self, alpha: f64) -> Self {
        Self { alpha, ..self }
    }

    pub fn mix(self, other: Color, amount: f64) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        Self::rgba(
            self.red + (other.red - self.red) * amount,
            self.green + (other.green - self.green) * amount,
            self.blue + (other.blue - self.blue) * amount,
            self.alpha + (other.alpha - self.alpha) * amount,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rgb_and_rgba_hex() {
        assert_eq!(
            Color::parse("#ff8000").unwrap(),
            Color::rgba(1.0, 128.0 / 255.0, 0.0, 1.0)
        );
        assert_eq!(Color::parse("#00000080").unwrap().alpha, 128.0 / 255.0);
    }

    #[test]
    fn color_mix_interpolates_channels() {
        let mixed = Color::rgba(0.0, 0.0, 0.0, 1.0).mix(Color::rgba(1.0, 0.5, 0.0, 0.5), 0.5);
        assert_eq!(mixed, Color::rgba(0.5, 0.25, 0.0, 0.75));
    }
}
