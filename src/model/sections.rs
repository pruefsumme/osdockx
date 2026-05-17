use super::types::{DockModel, DockSection, DockSectionKind, DockSections, WindowId};

impl DockModel {
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
