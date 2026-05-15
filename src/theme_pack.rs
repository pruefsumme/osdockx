use crate::config::{RenderMode, ThemeConfig, config_dir};
use crate::theme::{Theme, ThemeAssets};
use directories::ProjectDirs;
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const REPO_THEME_PACKS: &[(&str, &str)] = &[
    ("leopard", include_str!("../themes/leopard/theme.toml")),
    (
        "osx-glass-3d",
        include_str!("../themes/osx-glass-3d/theme.toml"),
    ),
];

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
    pub fn export_builtin_theme_packs() -> anyhow::Result<()> {
        for (id, contents) in REPO_THEME_PACKS {
            let path = config_dir()?.join("themes").join(id).join("theme.toml");
            if path.exists() {
                continue;
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, contents)?;
        }
        Ok(())
    }

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
        let renderer = config.renderer.unwrap_or(match config.preset.as_str() {
            "osx-glass-3d" => RenderMode::Scene3d,
            _ => RenderMode::Procedural2d,
        });
        config.renderer = Some(renderer);
        let theme = Theme::from_config(&config).with_renderer(renderer);
        let name = match config.preset.as_str() {
            "osx-glass-3d" => "OSX Glass 3D",
            "leopard" => "Leopard",
            _ => "OSDockX Theme",
        };
        Self {
            id: config.preset.clone(),
            name: name.to_string(),
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
            renderer: RenderMode::Procedural2d,
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
    let defaults = ThemeConfig::default();
    macro_rules! override_string {
        ($field:ident) => {
            if overrides.$field != defaults.$field {
                theme.$field = overrides.$field.clone();
            }
        };
    }
    macro_rules! override_copy {
        ($field:ident) => {
            if overrides.$field != defaults.$field {
                theme.$field = overrides.$field;
            }
        };
    }

    override_string!(shelf_top);
    override_string!(shelf_bottom);
    override_string!(shelf_stroke);
    override_string!(shelf_highlight);
    override_copy!(shelf_style);
    override_string!(indicator);
    override_string!(badge);
    override_copy!(reflection_opacity);
    override_copy!(reflection_height);
    override_copy!(shelf_height_ratio);
    override_copy!(shelf_slant_ratio);
    override_copy!(icon_gap_ratio);
    override_copy!(side_margin_ratio);
    override_copy!(shelf_horizon_ratio);
    override_copy!(front_lip_ratio);
    override_copy!(reflection_band_ratio);
    override_copy!(tilt);
    override_copy!(depth);
    override_copy!(bevel);
    override_copy!(floor_opacity);
    override_copy!(shadow_strength);
    override_copy!(highlight_strength);
    override_copy!(reflection_blur);
    override_copy!(material_roughness);
    override_copy!(icon_floor_offset);
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
        "" | "osx-glass" | "osx-crystal-2.5d" => "leopard".to_string(),
        value => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Color;

    #[test]
    fn builtin_migrates_old_osx_id_to_crystal_default() {
        let mut config = ThemeConfig {
            preset: "osx-glass".to_string(),
            ..ThemeConfig::default()
        };
        config.renderer = None;

        let pack = ThemePack::builtin(&config.preset, &config);

        assert_eq!(pack.id, "leopard");
        assert_eq!(pack.renderer, RenderMode::Procedural2d);
    }

    #[test]
    fn repo_theme_packs_are_valid_toml() {
        for (_, contents) in REPO_THEME_PACKS {
            let parsed = toml::from_str::<ThemePackToml>(contents).unwrap();
            assert!(!parsed.id.is_empty());
            assert!(!parsed.name.is_empty());
        }
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
        assert_eq!(pack.theme.shelf_bottom, Color::rgba(0.0, 0.0, 0.0, 1.0));
        assert_eq!(
            pack.assets.fallback_texture,
            Some(dir.path().join("assets/shelf.png"))
        );
    }
}
