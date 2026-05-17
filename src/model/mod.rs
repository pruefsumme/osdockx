mod build;
mod matching;
mod ordering;
mod sections;
mod types;

pub use matching::classes_match;
pub use types::{
    DockItem, DockItemKind, DockModel, DockSection, DockSectionKind, DockSections, WindowIcon,
    WindowId, WindowInfo,
};

#[cfg(test)]
mod tests;
