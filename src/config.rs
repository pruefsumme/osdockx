use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub dock: DockConfig,
    pub theme: ThemeConfig,
    pub pinned: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct DockConfig {
    pub edge: DockEdge,
    pub monitor: Option<String>,
    pub icon_size: u32,
    pub zoom_strength: f64,
    pub autohide: bool,
    pub hide_delay_ms: u32,
    pub unhide_delay_ms: u32,
    pub reserve_space: bool,
    pub refresh_ms: u32,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DockEdge {
    #[default]
    Bottom,
    Top,
    Left,
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ThemeConfig {
    pub preset: String,
    pub renderer: Option<RenderMode>,
    pub shelf_style: ShelfStyle,
    pub shelf_top: String,
    pub shelf_bottom: String,
    pub shelf_stroke: String,
    pub shelf_highlight: String,
    pub indicator: String,
    pub badge: String,
    pub reflection_opacity: f64,
    pub reflection_height: f64,
    pub shelf_height_ratio: f64,
    pub shelf_slant_ratio: f64,
    pub icon_gap_ratio: f64,
    #[serde(default = "default_side_margin_ratio")]
    pub side_margin_ratio: f64,
    #[serde(default = "default_shelf_horizon_ratio")]
    pub shelf_horizon_ratio: f64,
    #[serde(default = "default_front_lip_ratio")]
    pub front_lip_ratio: f64,
    #[serde(default = "default_reflection_band_ratio")]
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
    pub shelf_texture: Option<String>,
    pub shelf_overlay: Option<String>,
    pub noise_texture: Option<String>,
    pub normal_map: Option<String>,
    pub fallback_texture: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum RenderMode {
    #[serde(rename = "scene-3d")]
    Scene3d,
    #[serde(rename = "texture-2d")]
    Texture2d,
    #[default]
    #[serde(rename = "procedural-2d")]
    Procedural2d,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum ShelfStyle {
    #[default]
    #[serde(rename = "leopard-plank")]
    LeopardPlank,
    #[serde(rename = "crystal-glass")]
    CrystalGlass,
    #[serde(rename = "legacy-glass")]
    LegacyGlass,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            dock: DockConfig::default(),
            theme: ThemeConfig::default(),
            pinned: vec![
                "xfce4-terminal.desktop".to_string(),
                "thunar.desktop".to_string(),
                "firefox.desktop".to_string(),
                "xfce-settings-manager.desktop".to_string(),
            ],
        }
    }
}

impl Default for DockConfig {
    fn default() -> Self {
        Self {
            edge: DockEdge::Bottom,
            monitor: None,
            icon_size: 64,
            zoom_strength: 0.72,
            autohide: false,
            hide_delay_ms: 350,
            unhide_delay_ms: 40,
            reserve_space: true,
            refresh_ms: 500,
        }
    }
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            preset: "leopard".to_string(),
            renderer: Some(RenderMode::Procedural2d),
            shelf_style: ShelfStyle::LeopardPlank,
            shelf_top: "#f2f7fdff".to_string(),
            shelf_bottom: "#9aadbeff".to_string(),
            shelf_stroke: "#73889cff".to_string(),
            shelf_highlight: "#ffffffff".to_string(),
            indicator: "#6fd3ffff".to_string(),
            badge: "#e4202dff".to_string(),
            reflection_opacity: 0.34,
            reflection_height: 0.66,
            shelf_height_ratio: 0.78,
            shelf_slant_ratio: 0.50,
            icon_gap_ratio: 0.04,
            side_margin_ratio: 0.56,
            shelf_horizon_ratio: 0.46,
            front_lip_ratio: 0.05,
            reflection_band_ratio: 0.20,
            tilt: 0.58,
            depth: 0.76,
            bevel: 0.16,
            floor_opacity: 0.78,
            shadow_strength: 0.36,
            highlight_strength: 0.74,
            reflection_blur: 0.32,
            material_roughness: 0.18,
            icon_floor_offset: 0.0,
            shelf_texture: None,
            shelf_overlay: None,
            noise_texture: None,
            normal_map: None,
            fallback_texture: None,
        }
    }
}

impl Config {
    pub fn load_or_create() -> anyhow::Result<(Self, PathBuf)> {
        let path = config_path()?;
        if path.exists() {
            return Ok((Self::load_from_path(&path)?, path));
        }

        let config = Self::default().normalized();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, toml::to_string_pretty(&config)?)?;
        Ok((config, path))
    }

    pub fn load_from_path(path: &Path) -> anyhow::Result<Self> {
        let raw = fs::read_to_string(path)?;
        Ok(toml::from_str::<Self>(&raw)?.normalized())
    }

    pub fn normalized(mut self) -> Self {
        self.dock.icon_size = self.dock.icon_size.clamp(24, 160);
        self.dock.zoom_strength = self.dock.zoom_strength.clamp(0.0, 1.6);
        self.dock.refresh_ms = self.dock.refresh_ms.clamp(100, 5_000);
        self.theme.reflection_opacity = self.theme.reflection_opacity.clamp(0.0, 0.7);
        self.theme.reflection_height = self.theme.reflection_height.clamp(0.0, 0.8);
        self.theme.shelf_height_ratio = self.theme.shelf_height_ratio.clamp(0.12, 0.9);
        self.theme.shelf_slant_ratio = self.theme.shelf_slant_ratio.clamp(0.0, 0.8);
        self.theme.icon_gap_ratio = self.theme.icon_gap_ratio.clamp(0.0, 0.6);
        self.theme.side_margin_ratio = self.theme.side_margin_ratio.clamp(0.0, 1.2);
        self.theme.shelf_horizon_ratio = self.theme.shelf_horizon_ratio.clamp(0.25, 0.75);
        self.theme.front_lip_ratio = self.theme.front_lip_ratio.clamp(0.02, 0.30);
        self.theme.reflection_band_ratio = self.theme.reflection_band_ratio.clamp(0.0, 0.8);
        self.theme.tilt = self.theme.tilt.clamp(0.0, 1.2);
        self.theme.depth = self.theme.depth.clamp(0.0, 1.4);
        self.theme.bevel = self.theme.bevel.clamp(0.0, 0.8);
        self.theme.floor_opacity = self.theme.floor_opacity.clamp(0.0, 1.0);
        self.theme.shadow_strength = self.theme.shadow_strength.clamp(0.0, 1.0);
        self.theme.highlight_strength = self.theme.highlight_strength.clamp(0.0, 1.0);
        self.theme.reflection_blur = self.theme.reflection_blur.clamp(0.0, 1.0);
        self.theme.material_roughness = self.theme.material_roughness.clamp(0.0, 1.0);
        self.theme.icon_floor_offset = self.theme.icon_floor_offset.clamp(-0.4, 0.4);
        migrate_old_osx_defaults(&mut self.theme);
        for pinned in &mut self.pinned {
            *pinned = normalize_pinned_id(pinned);
        }
        self.pinned.retain(|id| !id.trim().is_empty());
        self
    }
}

pub fn config_dir() -> anyhow::Result<PathBuf> {
    let dirs = ProjectDirs::from("", "", "osdockx")
        .ok_or_else(|| anyhow::anyhow!("could not resolve XDG config directory"))?;
    Ok(dirs.config_dir().to_path_buf())
}

pub fn config_path() -> anyhow::Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

fn normalize_pinned_id(id: &str) -> String {
    match id.trim().to_ascii_lowercase().as_str() {
        "org.xfce.terminal.desktop" => "xfce4-terminal.desktop".to_string(),
        "thunar.desktop" => "thunar.desktop".to_string(),
        "org.xfce.settings.manager.desktop" => "xfce-settings-manager.desktop".to_string(),
        _ => id.trim().to_string(),
    }
}

fn migrate_old_osx_defaults(theme: &mut ThemeConfig) {
    if is_previous_generated_crystal_default(theme) {
        *theme = ThemeConfig::default();
        return;
    }

    if theme.preset == "osx-glass" {
        theme.preset = "osx-glass-3d".to_string();
        if theme.renderer.is_none() {
            theme.renderer = Some(RenderMode::Scene3d);
        }
    }

    if theme.preset != "osx-glass-3d" {
        return;
    }

    if approx(theme.reflection_height, 0.42) {
        theme.reflection_height = 0.30;
    }
    if approx(theme.shelf_height_ratio, 0.42) {
        theme.shelf_height_ratio = 0.34;
    }
    if approx(theme.shelf_slant_ratio, 0.34) {
        theme.shelf_slant_ratio = 0.30;
    }
    if approx(theme.icon_gap_ratio, 0.16) {
        theme.icon_gap_ratio = 0.12;
    }
    if theme.shelf_top.eq_ignore_ascii_case("#f7fbffff") {
        theme.shelf_top = "#f8fcffff".to_string();
    }
    if theme.shelf_bottom.eq_ignore_ascii_case("#7890a8dd") {
        theme.shelf_bottom = "#4f6072e8".to_string();
    }

    if is_previous_generated_gl_default(theme) {
        *theme = ThemeConfig::default();
    }
}

fn is_previous_generated_crystal_default(theme: &ThemeConfig) -> bool {
    is_previous_thin_crystal_default(theme) || is_previous_full_crystal_default(theme)
}

fn has_generated_crystal_base(theme: &ThemeConfig) -> bool {
    theme.preset == "osx-crystal-2.5d"
        && theme.renderer == Some(RenderMode::Procedural2d)
        && theme.shelf_style == ShelfStyle::CrystalGlass
        && theme.shelf_highlight.eq_ignore_ascii_case("#ffffffff")
        && theme.badge.eq_ignore_ascii_case("#e4202dff")
        && approx(theme.reflection_height, 0.34)
        && approx(theme.icon_gap_ratio, 0.12)
        && approx(theme.tilt, 0.58)
        && approx(theme.floor_opacity, 0.62)
        && approx(theme.reflection_blur, 0.18)
        && approx(theme.material_roughness, 0.34)
}

fn is_previous_thin_crystal_default(theme: &ThemeConfig) -> bool {
    has_generated_crystal_base(theme)
        && theme.shelf_top.eq_ignore_ascii_case("#f8fcffff")
        && theme.shelf_bottom.eq_ignore_ascii_case("#4f6072e8")
        && theme.shelf_stroke.eq_ignore_ascii_case("#2f4055cc")
        && theme.indicator.eq_ignore_ascii_case("#7dd7ffff")
        && approx(theme.reflection_opacity, 0.30)
        && approx(theme.shelf_height_ratio, 0.36)
        && approx(theme.shelf_slant_ratio, 0.30)
        && approx(theme.side_margin_ratio, 0.60)
        && approx(theme.shelf_horizon_ratio, 0.50)
        && approx(theme.front_lip_ratio, 0.10)
        && approx(theme.reflection_band_ratio, 0.42)
        && approx(theme.depth, 0.78)
        && approx(theme.bevel, 0.28)
        && approx(theme.shadow_strength, 0.42)
        && approx(theme.highlight_strength, 0.72)
        && approx(theme.icon_floor_offset, 0.0)
}

fn is_previous_full_crystal_default(theme: &ThemeConfig) -> bool {
    has_generated_crystal_base(theme)
        && theme.shelf_top.eq_ignore_ascii_case("#edf3faff")
        && theme.shelf_bottom.eq_ignore_ascii_case("#566270ff")
        && theme.shelf_stroke.eq_ignore_ascii_case("#263442ff")
        && theme.indicator.eq_ignore_ascii_case("#7dd7ffff")
        && approx(theme.reflection_opacity, 0.24)
        && approx(theme.shelf_height_ratio, 0.52)
        && approx(theme.shelf_slant_ratio, 0.34)
        && approx(theme.side_margin_ratio, 0.68)
        && approx(theme.shelf_horizon_ratio, 0.44)
        && approx(theme.front_lip_ratio, 0.22)
        && approx(theme.reflection_band_ratio, 0.34)
        && approx(theme.depth, 0.98)
        && approx(theme.bevel, 0.34)
        && approx(theme.shadow_strength, 0.56)
        && approx(theme.highlight_strength, 0.80)
        && approx(theme.icon_floor_offset, 0.0)
}

fn is_previous_generated_gl_default(theme: &ThemeConfig) -> bool {
    theme.preset == "osx-glass-3d"
        && theme.renderer == Some(RenderMode::Scene3d)
        && theme.shelf_top.eq_ignore_ascii_case("#f8fcffff")
        && theme.shelf_bottom.eq_ignore_ascii_case("#4f6072e8")
        && theme.shelf_stroke.eq_ignore_ascii_case("#2f4055cc")
        && theme.shelf_highlight.eq_ignore_ascii_case("#ffffffff")
        && theme.indicator.eq_ignore_ascii_case("#7dd7ffff")
        && theme.badge.eq_ignore_ascii_case("#e4202dff")
        && approx(theme.reflection_opacity, 0.26)
        && approx(theme.reflection_height, 0.34)
        && approx(theme.shelf_height_ratio, 0.36)
        && approx(theme.shelf_slant_ratio, 0.30)
        && approx(theme.icon_gap_ratio, 0.12)
        && approx(theme.tilt, 0.58)
        && approx(theme.depth, 0.78)
        && approx(theme.bevel, 0.28)
        && approx(theme.floor_opacity, 0.62)
        && approx(theme.shadow_strength, 0.42)
        && approx(theme.highlight_strength, 0.72)
        && approx(theme.reflection_blur, 0.18)
        && approx(theme.material_roughness, 0.34)
        && approx(theme.icon_floor_offset, 0.08)
}

fn approx(left: f64, right: f64) -> bool {
    (left - right).abs() < f64::EPSILON
}

const fn default_side_margin_ratio() -> f64 {
    0.74
}

const fn default_shelf_horizon_ratio() -> f64 {
    0.46
}

const fn default_front_lip_ratio() -> f64 {
    0.05
}

const fn default_reflection_band_ratio() -> f64 {
    0.20
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_risky_values() {
        let mut config = Config::default();
        config.dock.icon_size = 8;
        config.dock.zoom_strength = 9.0;
        config.dock.refresh_ms = 1;
        config.theme.reflection_opacity = 2.0;
        config.theme.shelf_height_ratio = 0.01;
        config.theme.side_margin_ratio = 9.0;
        config.theme.shelf_horizon_ratio = 0.01;
        config.theme.front_lip_ratio = 9.0;
        config.theme.reflection_band_ratio = 9.0;
        config.theme.tilt = 9.0;
        config.pinned = vec!["org.xfce.Terminal.desktop".to_string()];

        let config = config.normalized();

        assert_eq!(config.dock.icon_size, 24);
        assert_eq!(config.dock.zoom_strength, 1.6);
        assert_eq!(config.dock.refresh_ms, 100);
        assert_eq!(config.theme.reflection_opacity, 0.7);
        assert_eq!(config.theme.shelf_height_ratio, 0.12);
        assert_eq!(config.theme.side_margin_ratio, 1.2);
        assert_eq!(config.theme.shelf_horizon_ratio, 0.25);
        assert_eq!(config.theme.front_lip_ratio, 0.30);
        assert_eq!(config.theme.reflection_band_ratio, 0.8);
        assert_eq!(config.theme.tilt, 1.2);
        assert_eq!(config.pinned, vec!["xfce4-terminal.desktop"]);
    }

    #[test]
    fn migrates_previous_generated_gl_default_to_leopard_plank() {
        let mut config = Config::default();
        config.theme = ThemeConfig {
            preset: "osx-glass-3d".to_string(),
            renderer: Some(RenderMode::Scene3d),
            shelf_style: ShelfStyle::CrystalGlass,
            shelf_top: "#f8fcffff".to_string(),
            shelf_bottom: "#4f6072e8".to_string(),
            shelf_stroke: "#2f4055cc".to_string(),
            shelf_highlight: "#ffffffff".to_string(),
            indicator: "#7dd7ffff".to_string(),
            badge: "#e4202dff".to_string(),
            reflection_opacity: 0.26,
            reflection_height: 0.34,
            shelf_height_ratio: 0.36,
            shelf_slant_ratio: 0.30,
            icon_gap_ratio: 0.12,
            side_margin_ratio: 0.60,
            shelf_horizon_ratio: 0.50,
            front_lip_ratio: 0.10,
            reflection_band_ratio: 0.42,
            tilt: 0.58,
            depth: 0.78,
            bevel: 0.28,
            floor_opacity: 0.62,
            shadow_strength: 0.42,
            highlight_strength: 0.72,
            reflection_blur: 0.18,
            material_roughness: 0.34,
            icon_floor_offset: 0.08,
            shelf_texture: None,
            shelf_overlay: None,
            noise_texture: None,
            normal_map: None,
            fallback_texture: None,
        };

        let config = config.normalized();

        assert_eq!(config.theme.preset, "leopard");
        assert_eq!(config.theme.renderer, Some(RenderMode::Procedural2d));
        assert_eq!(config.theme.shelf_style, ShelfStyle::LeopardPlank);
        assert_eq!(config.theme.shelf_height_ratio, 0.78);
        assert_eq!(config.theme.icon_floor_offset, 0.0);
    }

    #[test]
    fn preserves_custom_gl_theme() {
        let mut config = Config::default();
        config.theme.preset = "osx-glass-3d".to_string();
        config.theme.renderer = Some(RenderMode::Scene3d);
        config.theme.reflection_opacity = 0.41;

        let config = config.normalized();

        assert_eq!(config.theme.preset, "osx-glass-3d");
        assert_eq!(config.theme.renderer, Some(RenderMode::Scene3d));
        assert_eq!(config.theme.reflection_opacity, 0.41);
    }

    #[test]
    fn migrates_previous_thin_crystal_default_to_leopard_plank() {
        let mut config = Config::default();
        config.theme = ThemeConfig {
            preset: "osx-crystal-2.5d".to_string(),
            renderer: Some(RenderMode::Procedural2d),
            shelf_style: ShelfStyle::CrystalGlass,
            shelf_top: "#f8fcffff".to_string(),
            shelf_bottom: "#4f6072e8".to_string(),
            shelf_stroke: "#2f4055cc".to_string(),
            shelf_highlight: "#ffffffff".to_string(),
            indicator: "#7dd7ffff".to_string(),
            badge: "#e4202dff".to_string(),
            reflection_opacity: 0.30,
            reflection_height: 0.34,
            shelf_height_ratio: 0.36,
            shelf_slant_ratio: 0.30,
            icon_gap_ratio: 0.12,
            side_margin_ratio: 0.60,
            shelf_horizon_ratio: 0.50,
            front_lip_ratio: 0.10,
            reflection_band_ratio: 0.42,
            tilt: 0.58,
            depth: 0.78,
            bevel: 0.28,
            floor_opacity: 0.62,
            shadow_strength: 0.42,
            highlight_strength: 0.72,
            reflection_blur: 0.18,
            material_roughness: 0.34,
            icon_floor_offset: 0.0,
            shelf_texture: None,
            shelf_overlay: None,
            noise_texture: None,
            normal_map: None,
            fallback_texture: None,
        };

        let config = config.normalized();

        assert_eq!(config.theme.preset, "leopard");
        assert_eq!(config.theme.shelf_style, ShelfStyle::LeopardPlank);
        assert_eq!(config.theme.shelf_height_ratio, 0.78);
        assert_eq!(config.theme.shelf_horizon_ratio, 0.46);
        assert_eq!(config.theme.shelf_top, "#f2f7fdff");
        assert_eq!(config.theme.shelf_bottom, "#9aadbeff");
    }

    #[test]
    fn migrates_previous_full_crystal_default_to_leopard_plank() {
        let mut config = Config::default();
        config.theme = ThemeConfig {
            preset: "osx-crystal-2.5d".to_string(),
            renderer: Some(RenderMode::Procedural2d),
            shelf_style: ShelfStyle::CrystalGlass,
            shelf_top: "#edf3faff".to_string(),
            shelf_bottom: "#566270ff".to_string(),
            shelf_stroke: "#263442ff".to_string(),
            shelf_highlight: "#ffffffff".to_string(),
            indicator: "#7dd7ffff".to_string(),
            badge: "#e4202dff".to_string(),
            reflection_opacity: 0.24,
            reflection_height: 0.34,
            shelf_height_ratio: 0.52,
            shelf_slant_ratio: 0.34,
            icon_gap_ratio: 0.12,
            side_margin_ratio: 0.68,
            shelf_horizon_ratio: 0.44,
            front_lip_ratio: 0.22,
            reflection_band_ratio: 0.34,
            tilt: 0.58,
            depth: 0.98,
            bevel: 0.34,
            floor_opacity: 0.62,
            shadow_strength: 0.56,
            highlight_strength: 0.80,
            reflection_blur: 0.18,
            material_roughness: 0.34,
            icon_floor_offset: 0.0,
            shelf_texture: None,
            shelf_overlay: None,
            noise_texture: None,
            normal_map: None,
            fallback_texture: None,
        };

        let config = config.normalized();

        assert_eq!(config.theme.shelf_style, ShelfStyle::LeopardPlank);
        assert_eq!(config.theme.shelf_height_ratio, 0.78);
        assert_eq!(config.theme.reflection_opacity, 0.34);
    }

    #[test]
    fn preserves_custom_crystal_cairo_theme() {
        let mut config = Config::default();
        config.theme.shelf_style = ShelfStyle::CrystalGlass;
        config.theme.shelf_height_ratio = 0.53;
        config.theme.shelf_top = "#dfe8f1ff".to_string();

        let config = config.normalized();

        assert_eq!(config.theme.shelf_style, ShelfStyle::CrystalGlass);
        assert_eq!(config.theme.shelf_height_ratio, 0.53);
        assert_eq!(config.theme.shelf_top, "#dfe8f1ff");
    }

    #[test]
    fn round_trips_default_toml() {
        let config = Config::default().normalized();
        let encoded = toml::to_string(&config).unwrap();
        let decoded = toml::from_str::<Config>(&encoded).unwrap().normalized();
        assert_eq!(decoded, config);
    }
}
