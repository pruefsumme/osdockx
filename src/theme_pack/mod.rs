mod assets;
mod discovery;
mod overrides;

use self::assets::ThemeAssetsToml;
use self::discovery::{find_theme_pack, normalized_theme_id};
use self::overrides::apply_user_theme_overrides;
use crate::config::{RenderMode, ThemeConfig, config_dir};
use crate::theme::{Theme, ThemeAssets};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

const REPO_THEME_PACKS: &[(&str, &str)] =
    &[("leopard", include_str!("../../themes/leopard/theme.toml"))];

fn builtin_theme_contents(id: &str) -> Option<&'static str> {
    REPO_THEME_PACKS
        .iter()
        .find_map(|(builtin_id, contents)| (*builtin_id == id).then_some(*contents))
}

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

impl ThemePack {
    pub fn watch_directories(&self) -> Vec<PathBuf> {
        let mut directories = Vec::new();
        if let Some(root) = self.root.as_ref() {
            directories.push(root.clone());
        }
        for path in [
            self.assets.shelf_texture.as_ref(),
            self.assets.shelf_overlay.as_ref(),
            self.assets.noise_texture.as_ref(),
            self.assets.normal_map.as_ref(),
            self.assets.fallback_texture.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            if let Some(parent) = path.parent() {
                let parent = parent.to_path_buf();
                if !directories.contains(&parent) {
                    directories.push(parent);
                }
            }
        }
        directories
    }

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

    pub fn restore_builtin_theme_pack(id: &str) -> anyhow::Result<()> {
        let id = normalized_theme_id(id);
        let Some(contents) = builtin_theme_contents(&id) else {
            anyhow::bail!("unknown built-in theme id: {id}");
        };

        let path = config_dir()?.join("themes").join(&id).join("theme.toml");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, contents)?;
        Ok(())
    }

    pub fn builtin(id: &str, config: &ThemeConfig) -> Self {
        let mut config = config.clone();
        config.preset = normalized_theme_id(id);
        let renderer = config.renderer.unwrap_or(RenderMode::Procedural2d);
        config.renderer = Some(renderer);
        let theme = Theme::from_config(&config).with_renderer(renderer);
        let name = match config.preset.as_str() {
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
        crate::perf::record_config_theme_parse();
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

#[cfg(test)]
mod tests;
