use crate::desktop::{DesktopApp, DesktopIndex};
use std::collections::HashSet;

pub type WindowId = u32;

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

impl DockModel {
    pub fn from_sources(
        pinned_ids: &[String],
        desktop_index: &DesktopIndex,
        windows: Vec<WindowInfo>,
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

        Self { items }
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
}

impl DockItem {
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
}

fn find_matching_item(
    items: &[DockItem],
    desktop_index: &DesktopIndex,
    window: &WindowInfo,
) -> Option<usize> {
    items.iter().position(|item| {
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

        assert_eq!(model.items.len(), 1);
        assert!(model.items[0].pinned);
        assert!(model.items[0].active);
        assert_eq!(model.items[0].windows[0].xid, 42);
    }
}
