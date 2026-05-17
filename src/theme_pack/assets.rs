use crate::theme::ThemeAssets;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub(crate) struct ThemeAssetsToml {
    pub shelf_texture: Option<String>,
    pub shelf_overlay: Option<String>,
    pub noise_texture: Option<String>,
    pub normal_map: Option<String>,
    pub fallback_texture: Option<String>,
}

impl ThemeAssetsToml {
    pub(crate) fn resolve(&self, root: Option<&Path>) -> ThemeAssets {
        ThemeAssets {
            shelf_texture: resolve_asset(root, self.shelf_texture.as_deref()),
            shelf_overlay: resolve_asset(root, self.shelf_overlay.as_deref()),
            noise_texture: resolve_asset(root, self.noise_texture.as_deref()),
            normal_map: resolve_asset(root, self.normal_map.as_deref()),
            fallback_texture: resolve_asset(root, self.fallback_texture.as_deref()),
        }
    }
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
