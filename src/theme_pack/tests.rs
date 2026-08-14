use super::*;
use crate::theme::Color;

#[test]
fn builtin_migrates_old_osx_id_to_leopard_default() {
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
    std::fs::write(
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
    assert_eq!(
        pack.watch_directories(),
        vec![dir.path().to_path_buf(), dir.path().join("assets")]
    );
}
