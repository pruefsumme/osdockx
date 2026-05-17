use super::types::{DockItem, WindowInfo};
use crate::desktop::DesktopIndex;

pub(crate) fn find_matching_item(
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
