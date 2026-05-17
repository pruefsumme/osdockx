use crate::config::{AppletConfig, AppletKind};
use crate::desktop::{DesktopApp, DesktopIndex};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub type WindowId = u32;

const DOWNLOADS_APPLET_ID: &str = "applet:downloads";
const TRASH_APPLET_ID: &str = "applet:trash";
const FOLDER_APPLET_PREFIX: &str = "applet:folder:";

#[derive(Debug, Clone, PartialEq)]
pub struct WindowIcon {
    pub width: u32,
    pub height: u32,
    pub argb: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WindowInfo {
    pub xid: WindowId,
    pub title: Option<String>,
    pub class: Option<String>,
    pub pid: Option<u32>,
    pub executable: Option<String>,
    pub workspace: Option<u32>,
    pub icon: Option<WindowIcon>,
    pub active: bool,
    pub urgent: bool,
    pub minimized: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DockItem {
    pub id: String,
    pub name: String,
    pub desktop_id: Option<String>,
    pub startup_wm_class: Option<String>,
    pub icon_name: Option<String>,
    pub window_icon: Option<WindowIcon>,
    pub pinned: bool,
    pub windows: Vec<WindowInfo>,
    pub active: bool,
    pub urgent: bool,
    pub badge: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DockModel {
    pub items: Vec<DockItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockItemKind {
    Application,
    DownloadsApplet,
    TrashApplet,
    FolderApplet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockSectionKind {
    Applications,
    Separator,
    Applets,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockSection {
    pub kind: DockSectionKind,
    pub item_indices: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockSections {
    pub applications: DockSection,
    pub separator: DockSection,
    pub applets: DockSection,
}

impl DockSections {
    pub fn ordered(&self) -> [&DockSection; 3] {
        [&self.applications, &self.separator, &self.applets]
    }
}

impl DockModel {
    pub fn from_sources(
        pinned_ids: &[String],
        desktop_index: &DesktopIndex,
        windows: Vec<WindowInfo>,
    ) -> Self {
        Self::from_sources_with_applets(pinned_ids, desktop_index, windows, &[])
    }

    pub fn from_sources_with_applets(
        pinned_ids: &[String],
        desktop_index: &DesktopIndex,
        windows: Vec<WindowInfo>,
        applets: &[AppletConfig],
    ) -> Self {
        let mut items = Vec::new();
        let mut seen_pins = HashSet::new();

        for desktop_id in pinned_ids {
            if !seen_pins.insert(desktop_id.to_ascii_lowercase()) {
                continue;
            }
            let app = desktop_index
                .by_id(desktop_id)
                .cloned()
                .unwrap_or_else(|| DesktopApp::placeholder(desktop_id));
            items.push(DockItem::from_app(&app, true));
        }

        for window in windows {
            if let Some(index) = find_matching_item(&items, desktop_index, &window) {
                items[index].push_window(window);
                continue;
            }

            let item = if let Some(app) = desktop_index.match_window(&window) {
                let mut item = DockItem::from_app(app, false);
                item.push_window(window);
                item
            } else {
                DockItem::from_window(window)
            };
            items.push(item);
        }

        for item in &mut items {
            item.active = item.windows.iter().any(|window| window.active);
            item.urgent = item.windows.iter().any(|window| window.urgent);
            item.badge = (item.windows.len() > 1).then_some(item.windows.len() as u32);
        }

        items.extend([DockItem::downloads_applet(), DockItem::trash_applet()]);
        items.extend(applets.iter().filter_map(DockItem::from_applet_config));

        Self { items }
    }

    pub fn apply_order(&mut self, ordered_keys: &[String]) {
        if ordered_keys.is_empty() || self.items.len() < 2 {
            return;
        }

        let (mut applications, mut applets) =
            split_items_by_section(std::mem::take(&mut self.items));
        sort_items_by_order(&mut applications, ordered_keys);
        sort_items_by_order(&mut applets, ordered_keys);
        applications.extend(applets);
        self.items = applications;
    }

    pub fn config_order(&self) -> Vec<String> {
        self.items.iter().map(DockItem::config_key).collect()
    }

    pub fn move_item_by_key_to_index(&mut self, item_key: &str, target_index: usize) -> bool {
        let (mut applications, mut applets) =
            split_items_by_section(std::mem::take(&mut self.items));
        let moving_applet = applets
            .iter()
            .any(|item| item.config_key().eq_ignore_ascii_case(item_key));
        let target_items = if moving_applet {
            &mut applets
        } else {
            &mut applications
        };
        let Some(current_index) = target_items
            .iter()
            .position(|item| item.config_key().eq_ignore_ascii_case(item_key))
        else {
            self.items = applications.into_iter().chain(applets).collect();
            return false;
        };
        let target_index = target_index.min(target_items.len().saturating_sub(1));
        if current_index == target_index {
            self.items = applications.into_iter().chain(applets).collect();
            return false;
        }

        let item = target_items.remove(current_index);
        target_items.insert(target_index, item);
        self.items = applications.into_iter().chain(applets).collect();
        true
    }

    pub fn active_window_for(&self, index: usize) -> Option<WindowId> {
        let item = self.items.get(index)?;
        item.windows
            .iter()
            .find(|window| window.active)
            .or_else(|| item.windows.iter().find(|window| !window.minimized))
            .or_else(|| item.windows.first())
            .map(|window| window.xid)
    }

    pub fn sections(&self) -> DockSections {
        DockSections {
            applications: DockSection {
                kind: DockSectionKind::Applications,
                item_indices: self
                    .items
                    .iter()
                    .enumerate()
                    .filter_map(|(index, item)| item.is_application().then_some(index))
                    .collect(),
            },
            separator: DockSection {
                kind: DockSectionKind::Separator,
                item_indices: Vec::new(),
            },
            applets: DockSection {
                kind: DockSectionKind::Applets,
                item_indices: self
                    .items
                    .iter()
                    .enumerate()
                    .filter_map(|(index, item)| item.is_applet().then_some(index))
                    .collect(),
            },
        }
    }
}

impl DockItem {
    pub fn downloads_applet() -> Self {
        Self::applet(DOWNLOADS_APPLET_ID, "Downloads", "folder-download")
    }

    pub fn trash_applet() -> Self {
        Self::applet(TRASH_APPLET_ID, "Trash", "user-trash")
    }

    pub fn folder_applet(label: String, path: PathBuf, icon_name: Option<String>) -> Self {
        let name = if label.trim().is_empty() {
            folder_name(&path)
        } else {
            label.trim().to_string()
        };
        Self::applet(
            &format!("{FOLDER_APPLET_PREFIX}{}", path.to_string_lossy()),
            &name,
            icon_name.as_deref().unwrap_or("folder"),
        )
    }

    fn from_applet_config(applet: &AppletConfig) -> Option<Self> {
        match applet.kind {
            AppletKind::Folder => {
                let path = applet.path.clone()?;
                Some(Self::folder_applet(
                    applet.label.clone(),
                    path,
                    applet.icon_name.clone(),
                ))
            }
        }
    }

    pub fn from_app(app: &DesktopApp, pinned: bool) -> Self {
        Self {
            id: app.desktop_id.clone(),
            name: app.name.clone(),
            desktop_id: Some(app.desktop_id.clone()),
            startup_wm_class: app.startup_wm_class.clone(),
            icon_name: app.icon_name.clone(),
            window_icon: None,
            pinned,
            windows: Vec::new(),
            active: false,
            urgent: false,
            badge: None,
        }
    }

    fn applet(id: &str, name: &str, icon_name: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            desktop_id: None,
            startup_wm_class: None,
            icon_name: Some(icon_name.to_string()),
            window_icon: None,
            pinned: true,
            windows: Vec::new(),
            active: false,
            urgent: false,
            badge: None,
        }
    }

    pub fn from_window(window: WindowInfo) -> Self {
        let name = window
            .class
            .clone()
            .or_else(|| window.title.clone())
            .unwrap_or_else(|| "Application".to_string());
        let id = format!("window:{}", window.xid);
        let mut item = Self {
            id,
            name,
            desktop_id: None,
            startup_wm_class: window.class.clone(),
            icon_name: window.class.clone().map(|class| class.to_ascii_lowercase()),
            window_icon: None,
            pinned: false,
            windows: Vec::new(),
            active: false,
            urgent: false,
            badge: None,
        };
        item.push_window(window);
        item
    }

    pub fn push_window(&mut self, window: WindowInfo) {
        if self.window_icon.is_none() {
            self.window_icon = window.icon.clone();
        }
        self.windows.push(window);
    }

    pub fn primary_window(&self) -> Option<WindowId> {
        self.windows
            .iter()
            .find(|window| window.active)
            .or_else(|| self.windows.iter().find(|window| !window.minimized))
            .or_else(|| self.windows.first())
            .map(|window| window.xid)
    }

    pub fn is_running(&self) -> bool {
        !self.windows.is_empty()
    }

    pub fn kind(&self) -> DockItemKind {
        match self.id.as_str() {
            DOWNLOADS_APPLET_ID => DockItemKind::DownloadsApplet,
            TRASH_APPLET_ID => DockItemKind::TrashApplet,
            id if id.starts_with(FOLDER_APPLET_PREFIX) => DockItemKind::FolderApplet,
            _ => DockItemKind::Application,
        }
    }

    pub fn is_application(&self) -> bool {
        self.kind() == DockItemKind::Application
    }

    pub fn is_applet(&self) -> bool {
        self.kind() != DockItemKind::Application
    }

    pub fn is_downloads_applet(&self) -> bool {
        self.kind() == DockItemKind::DownloadsApplet
    }

    pub fn is_trash_applet(&self) -> bool {
        self.kind() == DockItemKind::TrashApplet
    }

    pub fn is_folder_applet(&self) -> bool {
        self.kind() == DockItemKind::FolderApplet
    }

    pub fn folder_applet_path(&self) -> Option<PathBuf> {
        self.id
            .strip_prefix(FOLDER_APPLET_PREFIX)
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
    }

    pub fn config_key(&self) -> String {
        self.desktop_id
            .clone()
            .or_else(|| self.startup_wm_class.clone())
            .unwrap_or_else(|| self.id.clone())
    }
}

fn folder_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "Folder".to_string())
}

fn order_position(ordered_keys: &[String], item_key: &str) -> Option<usize> {
    ordered_keys
        .iter()
        .position(|ordered_key| ordered_key.eq_ignore_ascii_case(item_key))
}

fn split_items_by_section(items: Vec<DockItem>) -> (Vec<DockItem>, Vec<DockItem>) {
    items.into_iter().partition(DockItem::is_application)
}

fn sort_items_by_order(items: &mut [DockItem], ordered_keys: &[String]) {
    items
        .sort_by_key(|item| order_position(ordered_keys, &item.config_key()).unwrap_or(usize::MAX));
}

fn find_matching_item(
    items: &[DockItem],
    desktop_index: &DesktopIndex,
    window: &WindowInfo,
) -> Option<usize> {
    items.iter().position(|item| {
        if !item.is_application() {
            return false;
        }
        if let Some(app) = item
            .desktop_id
            .as_ref()
            .and_then(|id| desktop_index.by_id(id))
        {
            return app.matches_window(window);
        }
        classes_match(item.startup_wm_class.as_deref(), window.class.as_deref())
    })
}

pub fn classes_match(left: Option<&str>, right: Option<&str>) -> bool {
    let Some(left) = left else {
        return false;
    };
    let Some(right) = right else {
        return false;
    };
    left.eq_ignore_ascii_case(right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::DesktopIndex;

    #[test]
    fn merges_running_window_into_pinned_item_by_class() {
        let app = DesktopApp {
            desktop_id: "firefox.desktop".to_string(),
            name: "Firefox".to_string(),
            icon_name: Some("firefox".to_string()),
            startup_wm_class: Some("firefox".to_string()),
            exec: None,
        };
        let index = DesktopIndex::from_apps(vec![app]);
        let windows = vec![WindowInfo {
            xid: 42,
            title: Some("Example".to_string()),
            class: Some("Firefox".to_string()),
            pid: Some(100),
            executable: None,
            workspace: Some(0),
            icon: None,
            active: true,
            urgent: false,
            minimized: false,
        }];

        let model = DockModel::from_sources(&["firefox.desktop".to_string()], &index, windows);

        assert_eq!(model.items.len(), 3);
        assert!(model.items[0].pinned);
        assert!(model.items[0].active);
        assert_eq!(model.items[0].windows[0].xid, 42);
        assert!(model.items[1].is_downloads_applet());
        assert!(model.items[2].is_trash_applet());
    }

    #[test]
    fn applies_saved_item_order_after_merging_sources() {
        let index = DesktopIndex::from_apps(vec![
            DesktopApp {
                desktop_id: "terminal.desktop".to_string(),
                name: "Terminal".to_string(),
                icon_name: Some("terminal".to_string()),
                startup_wm_class: Some("terminal".to_string()),
                exec: None,
            },
            DesktopApp {
                desktop_id: "browser.desktop".to_string(),
                name: "Browser".to_string(),
                icon_name: Some("browser".to_string()),
                startup_wm_class: Some("browser".to_string()),
                exec: None,
            },
        ]);
        let mut model = DockModel::from_sources(
            &[
                "terminal.desktop".to_string(),
                "browser.desktop".to_string(),
            ],
            &index,
            Vec::new(),
        );

        model.apply_order(&[
            "applet:trash".to_string(),
            "browser.desktop".to_string(),
            "terminal.desktop".to_string(),
        ]);

        assert_eq!(
            model.config_order(),
            vec![
                "browser.desktop",
                "terminal.desktop",
                "applet:trash",
                "applet:downloads"
            ]
        );
        assert!(model.items[2].is_trash_applet());
        assert!(model.items[3].is_downloads_applet());
    }

    #[test]
    fn moves_item_by_config_key() {
        let index = DesktopIndex::from_apps(vec![
            DesktopApp {
                desktop_id: "one.desktop".to_string(),
                name: "One".to_string(),
                icon_name: None,
                startup_wm_class: None,
                exec: None,
            },
            DesktopApp {
                desktop_id: "two.desktop".to_string(),
                name: "Two".to_string(),
                icon_name: None,
                startup_wm_class: None,
                exec: None,
            },
            DesktopApp {
                desktop_id: "three.desktop".to_string(),
                name: "Three".to_string(),
                icon_name: None,
                startup_wm_class: None,
                exec: None,
            },
        ]);
        let mut model = DockModel::from_sources(
            &[
                "one.desktop".to_string(),
                "two.desktop".to_string(),
                "three.desktop".to_string(),
            ],
            &index,
            Vec::new(),
        );

        assert!(model.move_item_by_key_to_index("one.desktop", 2));
        assert_eq!(
            model.config_order(),
            vec![
                "two.desktop",
                "three.desktop",
                "one.desktop",
                "applet:downloads",
                "applet:trash"
            ]
        );
        assert!(model.items[3].is_downloads_applet());
        assert!(model.items[4].is_trash_applet());
    }

    #[test]
    fn moves_applet_by_config_key() {
        let index = DesktopIndex::from_apps(vec![DesktopApp {
            desktop_id: "one.desktop".to_string(),
            name: "One".to_string(),
            icon_name: None,
            startup_wm_class: None,
            exec: None,
        }]);
        let mut model = DockModel::from_sources(&["one.desktop".to_string()], &index, Vec::new());

        assert!(model.move_item_by_key_to_index("applet:trash", 0));
        assert_eq!(
            model.config_order(),
            vec!["one.desktop", "applet:trash", "applet:downloads"]
        );
        assert!(model.items[0].is_application());
        assert!(model.items[1].is_trash_applet());
        assert!(model.items[2].is_downloads_applet());
    }

    #[test]
    fn sections_keep_pinned_and_unpinned_running_apps_in_applications() {
        let index = DesktopIndex::from_apps(vec![
            DesktopApp {
                desktop_id: "browser.desktop".to_string(),
                name: "Browser".to_string(),
                icon_name: Some("browser".to_string()),
                startup_wm_class: Some("browser".to_string()),
                exec: None,
            },
            DesktopApp {
                desktop_id: "terminal.desktop".to_string(),
                name: "Terminal".to_string(),
                icon_name: Some("terminal".to_string()),
                startup_wm_class: Some("terminal".to_string()),
                exec: None,
            },
        ]);
        let model = DockModel::from_sources(
            &["browser.desktop".to_string()],
            &index,
            vec![WindowInfo {
                xid: 7,
                title: Some("Terminal".to_string()),
                class: Some("terminal".to_string()),
                pid: Some(100),
                executable: Some("terminal".to_string()),
                workspace: Some(0),
                icon: None,
                active: true,
                urgent: false,
                minimized: false,
            }],
        );

        let sections = model.sections();

        assert_eq!(sections.applications.item_indices, vec![0, 1]);
        assert_eq!(sections.applets.item_indices, vec![2, 3]);
        assert_eq!(
            model.items[0].desktop_id.as_deref(),
            Some("browser.desktop")
        );
        assert_eq!(
            model.items[1].desktop_id.as_deref(),
            Some("terminal.desktop")
        );
        assert!(model.items[1].is_running());
        assert!(!model.items[1].pinned);
        assert!(model.items[2].is_downloads_applet());
        assert!(model.items[3].is_trash_applet());
    }

    #[test]
    fn sections_do_not_duplicate_merged_running_apps() {
        let index = DesktopIndex::from_apps(vec![DesktopApp {
            desktop_id: "firefox.desktop".to_string(),
            name: "Firefox".to_string(),
            icon_name: Some("firefox".to_string()),
            startup_wm_class: Some("firefox".to_string()),
            exec: None,
        }]);
        let model = DockModel::from_sources(
            &["firefox.desktop".to_string()],
            &index,
            vec![WindowInfo {
                xid: 42,
                title: Some("Firefox".to_string()),
                class: Some("Firefox".to_string()),
                pid: Some(10),
                executable: Some("firefox".to_string()),
                workspace: Some(0),
                icon: None,
                active: true,
                urgent: false,
                minimized: false,
            }],
        );

        let sections = model.sections();

        assert_eq!(model.items.len(), 3);
        assert_eq!(sections.applications.item_indices, vec![0]);
        assert_eq!(sections.applets.item_indices, vec![1, 2]);
    }

    #[test]
    fn sections_reserve_separator_and_future_applet_placeholder() {
        let model = DockModel {
            items: vec![DockItem::from_window(WindowInfo {
                xid: 5,
                title: Some("App".to_string()),
                class: Some("app".to_string()),
                pid: Some(22),
                executable: Some("app".to_string()),
                workspace: Some(0),
                icon: None,
                active: true,
                urgent: false,
                minimized: false,
            })],
        };

        let sections = model.sections();
        let ordered = sections.ordered();

        assert_eq!(ordered[0].kind, DockSectionKind::Applications);
        assert_eq!(ordered[1].kind, DockSectionKind::Separator);
        assert_eq!(ordered[2].kind, DockSectionKind::Applets);
        assert_eq!(sections.separator.item_indices, Vec::<usize>::new());
        assert_eq!(sections.applets.item_indices, Vec::<usize>::new());
    }

    #[test]
    fn from_sources_appends_downloads_and_trash_applets_after_applications() {
        let index = DesktopIndex::from_apps(vec![DesktopApp {
            desktop_id: "browser.desktop".to_string(),
            name: "Browser".to_string(),
            icon_name: Some("browser".to_string()),
            startup_wm_class: Some("browser".to_string()),
            exec: None,
        }]);

        let model = DockModel::from_sources(&["browser.desktop".to_string()], &index, Vec::new());

        assert!(model.items[0].is_application());
        assert!(model.items[1].is_downloads_applet());
        assert!(model.items[2].is_trash_applet());
    }

    #[test]
    fn from_sources_appends_configured_folder_applets() {
        let index = DesktopIndex::default();
        let applets = vec![AppletConfig::folder(PathBuf::from("/tmp/projects"))];

        let model = DockModel::from_sources_with_applets(&[], &index, Vec::new(), &applets);

        assert!(model.items[0].is_downloads_applet());
        assert!(model.items[1].is_trash_applet());
        assert!(model.items[2].is_folder_applet());
        assert_eq!(
            model.items[2].folder_applet_path(),
            Some(PathBuf::from("/tmp/projects"))
        );
    }
}
