use directories::ProjectDirs;
use std::env;
use std::path::PathBuf;

pub(crate) fn find_theme_pack(id: &str) -> Option<PathBuf> {
    theme_roots()
        .into_iter()
        .map(|root| root.join(id).join("theme.toml"))
        .find(|path| path.exists())
}

pub(crate) fn normalized_theme_id(id: &str) -> String {
    match id.trim() {
        "" | "osx-glass" => "leopard".to_string(),
        value => value.to_string(),
    }
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
