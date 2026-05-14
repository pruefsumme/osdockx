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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            dock: DockConfig::default(),
            theme: ThemeConfig::default(),
            pinned: vec![
                "org.xfce.Terminal.desktop".to_string(),
                "Thunar.desktop".to_string(),
                "firefox.desktop".to_string(),
                "org.xfce.settings.manager.desktop".to_string(),
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
            preset: "osx-glass".to_string(),
            shelf_top: "#f7fbffff".to_string(),
            shelf_bottom: "#7890a8dd".to_string(),
            shelf_stroke: "#2f4055cc".to_string(),
            shelf_highlight: "#ffffffff".to_string(),
            indicator: "#7dd7ffff".to_string(),
            badge: "#e4202dff".to_string(),
            reflection_opacity: 0.26,
            reflection_height: 0.42,
            shelf_height_ratio: 0.42,
            shelf_slant_ratio: 0.34,
            icon_gap_ratio: 0.16,
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
        self.pinned.retain(|id| !id.trim().is_empty());
        self
    }
}

pub fn config_path() -> anyhow::Result<PathBuf> {
    let dirs = ProjectDirs::from("", "", "osdockx")
        .ok_or_else(|| anyhow::anyhow!("could not resolve XDG config directory"))?;
    Ok(dirs.config_dir().join("config.toml"))
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

        let config = config.normalized();

        assert_eq!(config.dock.icon_size, 24);
        assert_eq!(config.dock.zoom_strength, 1.6);
        assert_eq!(config.dock.refresh_ms, 100);
        assert_eq!(config.theme.reflection_opacity, 0.7);
        assert_eq!(config.theme.shelf_height_ratio, 0.12);
    }

    #[test]
    fn round_trips_default_toml() {
        let config = Config::default().normalized();
        let encoded = toml::to_string(&config).unwrap();
        let decoded = toml::from_str::<Config>(&encoded).unwrap().normalized();
        assert_eq!(decoded, config);
    }
}
