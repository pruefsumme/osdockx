use crate::config::{AppletConfig, AppletKind};
use crate::desktop::DesktopApp;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

pub type WindowId = u32;

const DOWNLOADS_APPLET_ID: &str = "applet:downloads";
const TRASH_APPLET_ID: &str = "applet:trash";
const FOLDER_APPLET_PREFIX: &str = "applet:folder:";

#[derive(Debug, Clone)]
pub struct WindowIcon {
    pub width: u32,
    pub height: u32,
    pub argb: Arc<[u32]>,
    signature: u64,
}

impl WindowIcon {
    pub fn from_argb(width: u32, height: u32, argb: Vec<u32>) -> Self {
        let mut hasher = DefaultHasher::new();
        width.hash(&mut hasher);
        height.hash(&mut hasher);
        argb.hash(&mut hasher);
        Self {
            width,
            height,
            argb: Arc::from(argb),
            signature: hasher.finish(),
        }
    }

    pub(crate) fn signature(&self) -> u64 {
        self.signature
    }
}

impl PartialEq for WindowIcon {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width
            && self.height == other.height
            && self.signature == other.signature
    }
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

    pub(crate) fn from_applet_config(applet: &AppletConfig) -> Option<Self> {
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

pub(crate) fn folder_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "Folder".to_string())
}
