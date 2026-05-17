use crate::model::WindowInfo;
use gio_unix::DesktopAppInfo;
use gtk::gio;
use gtk::gio::prelude::{AppInfoExt, IconExt};
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopApp {
    pub desktop_id: String,
    pub name: String,
    pub icon_name: Option<String>,
    pub startup_wm_class: Option<String>,
    pub exec: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DesktopIndex {
    apps: HashMap<String, DesktopApp>,
}

impl DesktopIndex {
    pub fn load() -> Self {
        let mut apps = HashMap::new();
        for applications_dir in application_dirs() {
            collect_desktop_files(&applications_dir, &applications_dir, &mut apps);
        }
        Self { apps }
    }

    pub fn from_apps(apps: Vec<DesktopApp>) -> Self {
        Self {
            apps: apps
                .into_iter()
                .map(|app| (app.desktop_id.to_ascii_lowercase(), app))
                .collect(),
        }
    }

    pub fn by_id(&self, desktop_id: &str) -> Option<&DesktopApp> {
        self.apps.get(&desktop_id.to_ascii_lowercase())
    }

    pub fn apps(&self) -> Vec<&DesktopApp> {
        let mut apps = self.apps.values().collect::<Vec<_>>();
        apps.sort_by(|left, right| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
                .then_with(|| left.desktop_id.cmp(&right.desktop_id))
        });
        apps
    }

    pub fn match_window(&self, window: &WindowInfo) -> Option<&DesktopApp> {
        self.apps.values().find(|app| app.matches_window(window))
    }

    pub fn launch(&self, desktop_id: &str) -> anyhow::Result<()> {
        let launch_id = self.resolve_launch_id(desktop_id).unwrap_or(desktop_id);
        let info = DesktopAppInfo::new(launch_id)
            .ok_or_else(|| anyhow::anyhow!("desktop entry not found: {desktop_id}"))?;
        info.launch(&[], None::<&gio::AppLaunchContext>)?;
        Ok(())
    }

    fn resolve_launch_id(&self, desktop_id: &str) -> Option<&str> {
        self.by_id(desktop_id)
            .map(|app| app.desktop_id.as_str())
            .or_else(|| {
                self.match_desktop_alias(desktop_id)
                    .map(|app| app.desktop_id.as_str())
            })
    }

    fn match_desktop_alias(&self, desktop_id: &str) -> Option<&DesktopApp> {
        let query = desktop_alias_key(desktop_id);
        if query.is_empty() {
            return None;
        }

        self.apps.values().find(|app| {
            let id = app.desktop_id.to_ascii_lowercase();
            let name = app.name.to_ascii_lowercase();
            let wm_class = app
                .startup_wm_class
                .as_deref()
                .unwrap_or_default()
                .to_ascii_lowercase();
            id.contains(&query) || name == query || name.contains(&query) || wm_class == query
        })
    }
}

impl DesktopApp {
    pub fn placeholder(desktop_id: &str) -> Self {
        let stem = desktop_id.trim_end_matches(".desktop");
        let menu_tail = stem
            .rsplit_once('-')
            .filter(|(_, tail)| tail.contains('.'))
            .map(|(_, tail)| tail)
            .unwrap_or(stem);
        let name = menu_tail
            .rsplit_once('.')
            .map(|(_, tail)| tail)
            .unwrap_or(menu_tail)
            .replace(['_', '-'], " ");
        Self {
            desktop_id: desktop_id.to_string(),
            name: title_case(&name),
            icon_name: Some(desktop_id.trim_end_matches(".desktop").to_ascii_lowercase()),
            startup_wm_class: None,
            exec: None,
        }
    }

    pub fn matches_window(&self, window: &WindowInfo) -> bool {
        let class_matches = window.class.as_deref().is_some_and(|class| {
            self.startup_wm_class
                .as_deref()
                .is_some_and(|wm_class| wm_class.eq_ignore_ascii_case(class))
                || self
                    .desktop_id
                    .trim_end_matches(".desktop")
                    .eq_ignore_ascii_case(class)
                || self.name.eq_ignore_ascii_case(class)
        });

        class_matches || executable_matches(self.exec.as_deref(), window.executable.as_deref())
    }

    fn from_info(desktop_id: String, info: DesktopAppInfo) -> Self {
        let icon_name = info
            .string("Icon")
            .map(|value| value.to_string())
            .or_else(|| {
                info.icon()
                    .and_then(|icon| icon.to_string().map(|value| value.to_string()))
            });

        Self {
            desktop_id,
            name: info.name().to_string(),
            icon_name,
            startup_wm_class: info.startup_wm_class().map(|value| value.to_string()),
            exec: info.string("Exec").map(|value| value.to_string()),
        }
    }
}

fn collect_desktop_files(root: &Path, dir: &Path, apps: &mut HashMap<String, DesktopApp>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_desktop_files(root, &path, apps);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("desktop") {
            continue;
        }
        let Some(desktop_id) = desktop_id_for_path(root, &path) else {
            continue;
        };
        if apps.contains_key(&desktop_id.to_ascii_lowercase()) {
            continue;
        }
        if let Some(info) = DesktopAppInfo::from_filename(&path) {
            apps.insert(
                desktop_id.to_ascii_lowercase(),
                DesktopApp::from_info(desktop_id, info),
            );
        }
    }
}

fn application_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = env::var_os("XDG_DATA_HOME") {
        dirs.push(PathBuf::from(home).join("applications"));
    } else if let Some(home) = env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/applications"));
    }

    let data_dirs = env::var_os("XDG_DATA_DIRS")
        .map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_else(|| {
            vec![
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share"),
            ]
        });
    dirs.extend(data_dirs.into_iter().map(|path| path.join("applications")));
    dirs
}

fn desktop_id_for_path(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let mut parts = relative
        .iter()
        .map(|part| part.to_string_lossy())
        .collect::<Vec<_>>();
    let file = parts.pop()?;
    if parts.is_empty() {
        Some(file.to_string())
    } else {
        Some(format!("{}-{file}", parts.join("-")))
    }
}

fn title_case(value: &str) -> String {
    value
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn desktop_alias_key(desktop_id: &str) -> String {
    let stem = desktop_id
        .trim()
        .trim_end_matches(".desktop")
        .to_ascii_lowercase();
    let stem = stem.rsplit_once('-').map(|(_, tail)| tail).unwrap_or(&stem);
    stem.rsplit_once('.')
        .map(|(_, tail)| tail)
        .unwrap_or(stem)
        .replace(['_', '-'], " ")
}

fn executable_matches(exec: Option<&str>, window_executable: Option<&str>) -> bool {
    let Some(window_executable) = window_executable.and_then(command_basename) else {
        return false;
    };
    let Some(exec) = exec else {
        return false;
    };

    exec_command_candidates(exec)
        .into_iter()
        .filter_map(|token| command_basename(&token).map(str::to_ascii_lowercase))
        .any(|candidate| candidate == window_executable.to_ascii_lowercase())
}

fn exec_command_candidates(exec: &str) -> Vec<String> {
    let mut tokens = shell_words(exec);
    tokens.retain(|token| !token.starts_with('%'));
    while tokens
        .first()
        .is_some_and(|token| token == "env" || token.contains('='))
    {
        tokens.remove(0);
    }
    tokens.into_iter().take(2).collect()
}

fn command_basename(command: &str) -> Option<&str> {
    let command = command.trim();
    if command.is_empty() {
        return None;
    }
    Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .or(Some(command))
}

fn shell_words(value: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;

    for ch in value.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        match quote {
            Some(active) if ch == active => quote = None,
            Some(_) => current.push(ch),
            None if ch == '\'' || ch == '"' => quote = Some(ch),
            None if ch.is_whitespace() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            None => current.push(ch),
        }
    }

    if !current.is_empty() {
        words.push(current);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_id_uses_menu_spec_prefix_shape() {
        let root = Path::new("/usr/share/applications");
        let path = Path::new("/usr/share/applications/kde/org.kde.foo.desktop");
        assert_eq!(
            desktop_id_for_path(root, path),
            Some("kde-org.kde.foo.desktop".to_string())
        );
    }

    #[test]
    fn placeholder_names_are_readable() {
        assert_eq!(
            DesktopApp::placeholder("org.xfce.Terminal.desktop").name,
            "Terminal"
        );
    }

    #[test]
    fn executable_fallback_matches_proc_exe_basename() {
        let app = DesktopApp {
            desktop_id: "org.mozilla.firefox.desktop".to_string(),
            name: "Firefox".to_string(),
            icon_name: Some("firefox".to_string()),
            startup_wm_class: None,
            exec: Some("env MOZ_ENABLE_WAYLAND=0 /usr/lib/firefox/firefox %u".to_string()),
        };
        let window = WindowInfo {
            xid: 12,
            title: None,
            class: Some("Navigator".to_string()),
            pid: Some(30),
            executable: Some("/usr/lib/firefox/firefox".to_string()),
            workspace: None,
            icon: None,
            active: false,
            urgent: false,
            minimized: false,
        };

        assert!(app.matches_window(&window));
    }

    #[test]
    fn resolves_common_xfce_terminal_alias() {
        let index = DesktopIndex::from_apps(vec![DesktopApp {
            desktop_id: "xfce4-terminal.desktop".to_string(),
            name: "Terminal".to_string(),
            icon_name: Some("utilities-terminal".to_string()),
            startup_wm_class: Some("Xfce4-terminal".to_string()),
            exec: Some("xfce4-terminal".to_string()),
        }]);

        assert_eq!(
            index.resolve_launch_id("org.xfce.Terminal.desktop"),
            Some("xfce4-terminal.desktop")
        );
    }

    #[test]
    fn apps_are_sorted_by_display_name() {
        let index = DesktopIndex::from_apps(vec![
            DesktopApp {
                desktop_id: "z.desktop".to_string(),
                name: "Zulu".to_string(),
                icon_name: None,
                startup_wm_class: None,
                exec: None,
            },
            DesktopApp {
                desktop_id: "a.desktop".to_string(),
                name: "Alpha".to_string(),
                icon_name: None,
                startup_wm_class: None,
                exec: None,
            },
        ]);

        assert_eq!(
            index
                .apps()
                .into_iter()
                .map(|app| app.desktop_id.as_str())
                .collect::<Vec<_>>(),
            vec!["a.desktop", "z.desktop"]
        );
    }
}
