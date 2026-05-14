use crate::config::{RenderMode, ThemeConfig};
use crate::theme::{Theme, ThemeAssets};
use directories::ProjectDirs;
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct ThemePack {
    pub id: String,
    pub name: String,
    pub renderer: RenderMode,
    pub root: Option<PathBuf>,
    pub assets: ThemeAssets,
    pub theme: Theme,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct ThemePackToml {
    id: String,
    name: String,
    renderer: RenderMode,
    theme: ThemeConfig,
    assets: ThemeAssetsToml,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct ThemeAssetsToml {
    shelf_texture: Option<String>,
    shelf_overlay: Option<String>,
    noise_texture: Option<String>,
    normal_map: Option<String>,
    fallback_texture: Option<String>,
}

impl ThemePack {
    pub fn load(config: &ThemeConfig) -> Self {
        let id = normalized_theme_id(&config.preset);
        if let Some(path) = find_theme_pack(&id) {
            match Self::from_path(&path, config) {
                Ok(pack) => return pack,
                Err(error) => {
                    tracing::warn!("could not load theme pack {}: {error:#}", path.display())
                }
            }
        }
        Self::builtin(&id, config)
    }

    pub fn builtin(id: &str, config: &ThemeConfig) -> Self {
        let mut config = config.clone();
        config.preset = normalized_theme_id(id);
        let renderer = config.renderer.unwrap_or(RenderMode::Scene3d);
        config.renderer = Some(renderer);
        let theme = Theme::from_config(&config).with_renderer(renderer);
        Self {
            id: config.preset.clone(),
            name: "OSX Glass 3D".to_string(),
            renderer,
            root: None,
            assets: ThemeAssets::default(),
            theme,
        }
    }

    pub fn from_path(path: &Path, overrides: &ThemeConfig) -> anyhow::Result<Self> {
        let raw = fs::read_to_string(path)?;
        let mut parsed = toml::from_str::<ThemePackToml>(&raw)?;
        let root = path.parent().map(Path::to_path_buf);
        if parsed.id.trim().is_empty() {
            parsed.id = path
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .unwrap_or("theme")
                .to_string();
        }
        if parsed.name.trim().is_empty() {
            parsed.name = parsed.id.clone();
        }

        parsed.theme.preset = normalized_theme_id(&parsed.id);
        parsed.theme.renderer = Some(parsed.renderer);
        apply_user_theme_overrides(&mut parsed.theme, overrides);
        let assets = parsed.assets.resolve(root.as_deref());
        let theme = Theme::from_config(&parsed.theme)
            .with_assets(assets.clone())
            .with_renderer(parsed.renderer);

        Ok(Self {
            id: parsed.theme.preset,
            name: parsed.name,
            renderer: parsed.renderer,
            root,
            assets,
            theme,
        })
    }
}

impl Default for ThemePackToml {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            renderer: RenderMode::Scene3d,
            theme: ThemeConfig::default(),
            assets: ThemeAssetsToml::default(),
        }
    }
}

impl ThemeAssetsToml {
    fn resolve(&self, root: Option<&Path>) -> ThemeAssets {
        ThemeAssets {
            shelf_texture: resolve_asset(root, self.shelf_texture.as_deref()),
            shelf_overlay: resolve_asset(root, self.shelf_overlay.as_deref()),
            noise_texture: resolve_asset(root, self.noise_texture.as_deref()),
            normal_map: resolve_asset(root, self.normal_map.as_deref()),
            fallback_texture: resolve_asset(root, self.fallback_texture.as_deref()),
        }
    }
}

fn apply_user_theme_overrides(theme: &mut ThemeConfig, overrides: &ThemeConfig) {
    theme.shelf_top = overrides.shelf_top.clone();
    theme.shelf_bottom = overrides.shelf_bottom.clone();
    theme.shelf_stroke = overrides.shelf_stroke.clone();
    theme.shelf_highlight = overrides.shelf_highlight.clone();
    theme.indicator = overrides.indicator.clone();
    theme.badge = overrides.badge.clone();
    theme.reflection_opacity = overrides.reflection_opacity;
    theme.reflection_height = overrides.reflection_height;
    theme.shelf_height_ratio = overrides.shelf_height_ratio;
    theme.shelf_slant_ratio = overrides.shelf_slant_ratio;
    theme.icon_gap_ratio = overrides.icon_gap_ratio;
    theme.tilt = overrides.tilt;
    theme.depth = overrides.depth;
    theme.bevel = overrides.bevel;
    theme.floor_opacity = overrides.floor_opacity;
    theme.shadow_strength = overrides.shadow_strength;
    theme.highlight_strength = overrides.highlight_strength;
    theme.reflection_blur = overrides.reflection_blur;
    theme.material_roughness = overrides.material_roughness;
    theme.icon_floor_offset = overrides.icon_floor_offset;
}

fn resolve_asset(root: Option<&Path>, asset: Option<&str>) -> Option<PathBuf> {
    let asset = asset?.trim();
    if asset.is_empty() {
        return None;
    }
    let path = PathBuf::from(asset);
    if path.is_absolute() {
        Some(path)
    } else {
        root.map(|root| root.join(path))
    }
}

fn find_theme_pack(id: &str) -> Option<PathBuf> {
    theme_roots()
        .into_iter()
        .map(|root| root.join(id).join("theme.toml"))
        .find(|path| path.exists())
}

fn theme_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(dirs) = ProjectDirs::from("", "", "osdockx") {
        roots.push(dirs.config_dir().join("themes"));
        roots.push(dirs.data_dir().join("themes"));
    }
    if let Some(data_dirs) = env::var_os("XDG_DATA_DIRS") {
        roots.extend(env::split_paths(&data_dirs).map(|path| path.join("osdockx/themes")));
    }
    roots
}

fn normalized_theme_id(id: &str) -> String {
    match id.trim() {
        "" | "osx-glass" => "osx-glass-3d".to_string(),
        value => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_migrates_old_osx_id() {
        let mut config = ThemeConfig {
            preset: "osx-glass".to_string(),
            ..ThemeConfig::default()
        };
        config.renderer = None;

        let pack = ThemePack::builtin(&config.preset, &config);

        assert_eq!(pack.id, "osx-glass-3d");
        assert_eq!(pack.renderer, RenderMode::Scene3d);
    }

    #[test]
    fn resolves_relative_assets_from_theme_file() {
        let dir = tempfile::tempdir().unwrap();
        let theme_path = dir.path().join("theme.toml");
        fs::write(
            &theme_path,
            r##"
id = "cairo-ish"
name = "Cairo-ish"
renderer = "texture-2d"

[assets]
fallback_texture = "assets/shelf.png"

[theme]
preset = "ignored"
shelf_top = "#ffffffff"
shelf_bottom = "#000000ff"
shelf_stroke = "#000000ff"
shelf_highlight = "#ffffffff"
indicator = "#ffffffff"
badge = "#ff0000ff"
reflection_opacity = 0.2
reflection_height = 0.3
shelf_height_ratio = 0.4
shelf_slant_ratio = 0.3
icon_gap_ratio = 0.1
"##,
        )
        .unwrap();

        let pack = ThemePack::from_path(&theme_path, &ThemeConfig::default()).unwrap();

        assert_eq!(pack.renderer, RenderMode::Texture2d);
        assert_eq!(
            pack.assets.fallback_texture,
            Some(dir.path().join("assets/shelf.png"))
        );
    }
}
