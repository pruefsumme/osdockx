use crate::config::{RenderMode, ThemeConfig};
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
    pub fn from_config(_config: &ThemeConfig) -> Self {
        Self::default()
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

impl Default for Theme {
    fn default() -> Self {
        Self {
            id: "default".to_string(),
            renderer: RenderMode::Procedural2d,
            shelf_top: Color::rgba(0.81, 0.84, 0.86, 1.0),
            shelf_bottom: Color::rgba(0.59, 0.64, 0.69, 1.0),
            shelf_stroke: Color::rgba(0.36, 0.42, 0.47, 1.0),
            shelf_highlight: Color::rgba(0.86, 0.89, 0.91, 1.0),
            indicator: Color::rgba(0.44, 0.83, 1.0, 1.0),
            badge: Color::rgba(0.89, 0.13, 0.18, 1.0),
            reflection_opacity: 0.26,
            reflection_height: 0.46,
            shelf_height_ratio: 0.62,
            shelf_slant_ratio: 0.42,
            icon_gap_ratio: 0.04,
            side_margin_ratio: 0.82,
            shelf_horizon_ratio: 0.62,
            front_lip_ratio: 0.18,
            reflection_band_ratio: 0.16,
            tilt: 0.58,
            depth: 0.58,
            bevel: 0.10,
            floor_opacity: 0.72,
            shadow_strength: 0.28,
            highlight_strength: 0.60,
            reflection_blur: 0.44,
            material_roughness: 0.12,
            icon_floor_offset: 0.02,
            assets: ThemeAssets::default(),
        }
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
