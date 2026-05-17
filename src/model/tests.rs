use super::*;
use crate::config::AppletConfig;
use crate::desktop::{DesktopApp, DesktopIndex};
use std::path::PathBuf;

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
    assert_eq!(model.items[0].desktop_id.as_deref(), Some("browser.desktop"));
    assert_eq!(model.items[1].desktop_id.as_deref(), Some("terminal.desktop"));
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
