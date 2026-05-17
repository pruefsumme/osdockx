use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub dock: DockConfig,
    pub theme: ThemeConfig,
    pub pinned: Vec<String>,
    pub hidden: Vec<String>,
    pub applets: Vec<AppletConfig>,
    pub item_order: Vec<String>,
    pub custom_icons: BTreeMap<String, String>,
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
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ShelfStyle {
    CrystalGlass,
    #[default]
    LeopardPlank,
    Legacy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppletConfig {
    pub kind: AppletKind,
    pub label: String,
    pub path: Option<PathBuf>,
    pub icon_name: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AppletKind {
    #[default]
    Folder,
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
            hidden: Vec::new(),
            applets: Vec::new(),
            item_order: Vec::new(),
            custom_icons: BTreeMap::new(),
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
            renderer: None,
            shelf_style: ShelfStyle::LeopardPlank,
            shelf_top: "#cfd5dbff".to_string(),
            shelf_bottom: "#97a4b0ff".to_string(),
            shelf_stroke: "#5b6a79ff".to_string(),
            shelf_highlight: "#dbe2e8ff".to_string(),
            indicator: "#6fd3ffff".to_string(),
            badge: "#e4202dff".to_string(),
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
        }
    }
}

impl Default for AppletConfig {
    fn default() -> Self {
        Self {
            kind: AppletKind::Folder,
            label: String::new(),
            path: None,
            icon_name: None,
        }
    }
}

impl AppletConfig {
    pub fn folder(path: PathBuf) -> Self {
        Self {
            kind: AppletKind::Folder,
            label: folder_applet_label(&path),
            path: Some(path),
            icon_name: Some("folder".to_string()),
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

    pub fn save_to_path(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, toml::to_string_pretty(&self.clone().normalized())?)?;
        Ok(())
    }

    pub fn normalized(mut self) -> Self {
        self.dock.edge = DockEdge::Bottom;
        self.dock.icon_size = self.dock.icon_size.clamp(24, 160);
        self.dock.zoom_strength = self.dock.zoom_strength.clamp(0.0, 1.6);
        self.dock.refresh_ms = self.dock.refresh_ms.clamp(100, 5_000);

        self.theme.preset = self.theme.preset.trim().to_string();
        if self.theme.preset.is_empty() {
            self.theme.preset = ThemeConfig::default().preset;
        }
        self.theme.reflection_opacity = self.theme.reflection_opacity.clamp(0.0, 1.0);
        self.theme.reflection_height = self.theme.reflection_height.clamp(0.0, 1.0);
        self.theme.shelf_height_ratio = self.theme.shelf_height_ratio.clamp(0.18, 1.30);
        self.theme.shelf_slant_ratio = self.theme.shelf_slant_ratio.clamp(0.0, 1.0);
        self.theme.icon_gap_ratio = self.theme.icon_gap_ratio.clamp(0.0, 0.50);
        self.theme.side_margin_ratio = self.theme.side_margin_ratio.clamp(0.0, 2.0);
        self.theme.shelf_horizon_ratio = self.theme.shelf_horizon_ratio.clamp(0.0, 1.0);
        self.theme.front_lip_ratio = self.theme.front_lip_ratio.clamp(0.0, 1.0);
        self.theme.reflection_band_ratio = self.theme.reflection_band_ratio.clamp(0.0, 1.0);
        self.theme.tilt = self.theme.tilt.clamp(0.0, 1.0);
        self.theme.depth = self.theme.depth.clamp(0.0, 1.0);
        self.theme.bevel = self.theme.bevel.clamp(0.0, 1.0);
        self.theme.floor_opacity = self.theme.floor_opacity.clamp(0.0, 1.0);
        self.theme.shadow_strength = self.theme.shadow_strength.clamp(0.0, 1.6);
        self.theme.highlight_strength = self.theme.highlight_strength.clamp(0.0, 1.6);
        self.theme.reflection_blur = self.theme.reflection_blur.clamp(0.0, 1.0);
        self.theme.material_roughness = self.theme.material_roughness.clamp(0.0, 1.0);
        self.theme.icon_floor_offset = self.theme.icon_floor_offset.clamp(-0.4, 0.4);

        for pinned in &mut self.pinned {
            *pinned = normalize_pinned_id(pinned);
        }
        self.pinned.retain(|id| !id.trim().is_empty());

        for hidden in &mut self.hidden {
            *hidden = normalize_custom_icon_key(hidden);
        }
        dedupe_case_insensitive(&mut self.hidden);
        self.hidden.retain(|id| !id.trim().is_empty());

        for applet in &mut self.applets {
            applet.label = applet.label.trim().to_string();
            applet.icon_name = applet
                .icon_name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
        }
        self.applets.retain(|applet| match applet.kind {
            AppletKind::Folder => applet
                .path
                .as_ref()
                .is_some_and(|path| !path.as_os_str().is_empty()),
        });
        dedupe_applets(&mut self.applets);

        for item in &mut self.item_order {
            *item = normalize_custom_icon_key(item);
        }
        dedupe_case_insensitive(&mut self.item_order);
        self.item_order.retain(|id| !id.trim().is_empty());

        self.custom_icons = self
            .custom_icons
            .into_iter()
            .filter_map(|(key, path)| {
                let key = normalize_custom_icon_key(&key);
                let path = path.trim().to_string();
                (!key.is_empty() && !path.is_empty()).then_some((key, path))
            })
            .collect();

        self
    }
}

fn folder_applet_label(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            path.to_str()
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "Folder".to_string())
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

fn normalize_custom_icon_key(key: &str) -> String {
    let trimmed = key.trim();
    if trimmed.to_ascii_lowercase().ends_with(".desktop") {
        normalize_pinned_id(trimmed)
    } else {
        trimmed.to_string()
    }
}

fn dedupe_case_insensitive(values: &mut Vec<String>) {
    let mut seen = Vec::<String>::new();
    values.retain(|value| {
        let normalized = value.to_ascii_lowercase();
        if seen.iter().any(|seen| seen == &normalized) {
            return false;
        }
        seen.push(normalized);
        true
    });
}

fn dedupe_applets(applets: &mut Vec<AppletConfig>) {
    let mut seen = Vec::<String>::new();
    applets.retain(|applet| {
        let key = match applet.kind {
            AppletKind::Folder => applet
                .path
                .as_ref()
                .map(|path| path.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default(),
        };
        if key.is_empty() || seen.iter().any(|seen| seen == &key) {
            return false;
        }
        seen.push(key);
        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_risky_values() {
        let mut config = Config::default();
        config.dock.edge = DockEdge::Left;
        config.dock.icon_size = 8;
        config.dock.zoom_strength = 9.0;
        config.dock.refresh_ms = 1;
        config.pinned = vec!["org.xfce.Terminal.desktop".to_string()];
        config.hidden = vec![
            " org.xfce.Terminal.desktop ".to_string(),
            "Org.Xfce.Terminal.desktop".to_string(),
            " ".to_string(),
        ];
        config.applets = vec![
            AppletConfig {
                kind: AppletKind::Folder,
                label: " Downloads ".to_string(),
                path: Some(PathBuf::from("/tmp/Downloads")),
                icon_name: Some(" folder ".to_string()),
            },
            AppletConfig {
                kind: AppletKind::Folder,
                label: "Duplicate".to_string(),
                path: Some(PathBuf::from("/tmp/Downloads")),
                icon_name: None,
            },
            AppletConfig {
                kind: AppletKind::Folder,
                label: "Broken".to_string(),
                path: None,
                icon_name: None,
            },
        ];
        config.item_order = vec![
            "org.xfce.Terminal.desktop".to_string(),
            "org.xfce.Terminal.desktop".to_string(),
            " ".to_string(),
        ];
        config.custom_icons.insert(
            " org.xfce.Terminal.desktop ".to_string(),
            " /tmp/terminal.png ".to_string(),
        );
        config
            .custom_icons
            .insert("empty.desktop".to_string(), " ".to_string());

        let config = config.normalized();

        assert_eq!(config.dock.edge, DockEdge::Bottom);
        assert_eq!(config.dock.icon_size, 24);
        assert_eq!(config.dock.zoom_strength, 1.6);
        assert_eq!(config.dock.refresh_ms, 100);
        assert_eq!(config.pinned, vec!["xfce4-terminal.desktop"]);
        assert_eq!(config.hidden, vec!["xfce4-terminal.desktop"]);
        assert_eq!(config.applets.len(), 1);
        assert_eq!(config.applets[0].label, "Downloads");
        assert_eq!(config.applets[0].icon_name.as_deref(), Some("folder"));
        assert_eq!(config.item_order, vec!["xfce4-terminal.desktop"]);
        assert_eq!(
            config.custom_icons.get("xfce4-terminal.desktop"),
            Some(&"/tmp/terminal.png".to_string())
        );
        assert!(!config.custom_icons.contains_key("empty.desktop"));
    }

    #[test]
    fn round_trips_default_toml() {
        let config = Config::default().normalized();
        let encoded = toml::to_string(&config).unwrap();
        let decoded = toml::from_str::<Config>(&encoded).unwrap().normalized();
        assert_eq!(decoded, config);
    }
}
