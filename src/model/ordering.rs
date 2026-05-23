use super::types::{DockItem, DockModel};

impl DockModel {
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
