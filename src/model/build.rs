use super::matching::find_matching_item;
use super::types::{DockItem, DockModel, WindowInfo};
use crate::config::AppletConfig;
use crate::desktop::{DesktopApp, DesktopIndex};
use std::collections::HashSet;

impl DockModel {
    pub fn from_sources(
        pinned_ids: &[String],
        desktop_index: &DesktopIndex,
        windows: Vec<WindowInfo>,
    ) -> Self {
        Self::from_sources_with_applets(pinned_ids, &[], desktop_index, windows, &[])
    }

    pub fn from_sources_with_applets(
        pinned_ids: &[String],
        hidden_ids: &[String],
        desktop_index: &DesktopIndex,
        windows: Vec<WindowInfo>,
        applets: &[AppletConfig],
    ) -> Self {
        let mut items = Vec::new();
        let mut seen_pins = HashSet::new();
        let hidden_ids = hidden_ids
            .iter()
            .map(|id| id.to_ascii_lowercase())
            .collect::<HashSet<_>>();

        for desktop_id in pinned_ids {
            if hidden_ids.contains(&desktop_id.to_ascii_lowercase()) {
                continue;
            }
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
            if window
                .class
                .as_ref()
                .map(|class| hidden_ids.contains(&class.to_ascii_lowercase()))
                .unwrap_or(false)
            {
                continue;
            }
            if desktop_index
                .match_window(&window)
                .map(|app| hidden_ids.contains(&app.desktop_id.to_ascii_lowercase()))
                .unwrap_or(false)
            {
                continue;
            }
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
}
