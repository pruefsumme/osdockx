use super::{DockGeometry, MonitorGeometry, PlatformBackend};
use crate::config::DockEdge;
use crate::layout::Rect;
use crate::model::{WindowIcon, WindowId, WindowInfo};
use anyhow::Context;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use x11rb::CURRENT_TIME;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::randr::{ConnectionExt as RandrConnectionExt, MonitorInfo};
use x11rb::protocol::shape::{ConnectionExt as ShapeConnectionExt, SK, SO};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ChangeWindowAttributesAux, ClientMessageData, ClientMessageEvent, ClipOrdering,
    ConfigureWindowAux, ConnectionExt, EventMask, GetPropertyReply, InputFocus, PropMode,
    Rectangle, StackMode, Window,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as WrapperConnectionExt;

#[derive(Debug)]
pub struct X11Backend {
    conn: RustConnection,
    screen_num: usize,
    root: Window,
    atoms: Atoms,
    dock_window: Option<WindowId>,
    metadata: WindowMetadataCache,
    reconciliation_interval: Duration,
}

const FULL_RECONCILIATION_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Default)]
struct WindowMetadataCache {
    client_ids: Vec<WindowId>,
    active_window: Option<WindowId>,
    windows: HashMap<WindowId, CachedWindowMetadata>,
    initialized: bool,
    last_reconciliation: Option<Instant>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct CachedWindowMetadata {
    net_title: Option<String>,
    legacy_title: Option<String>,
    class: Option<String>,
    pid: Option<u32>,
    executable: Option<String>,
    workspace: Option<u32>,
    icon: Option<WindowIcon>,
    urgent: bool,
    minimized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowMetadataField {
    State,
    NetTitle,
    LegacyTitle,
    Class,
    Pid,
    Workspace,
    Icon,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WindowCacheChange {
    ClientList,
    ActiveWindow,
    Field {
        xid: WindowId,
        field: WindowMetadataField,
    },
    Destroyed(WindowId),
}

#[derive(Debug, Default)]
pub struct X11WindowUpdate {
    pub windows: Option<Vec<WindowInfo>>,
    pub invalidated_icon_signatures: Vec<u64>,
    changes: Vec<WindowCacheChange>,
}

#[derive(Debug, Clone, Copy)]
struct Atoms {
    net_active_window: Atom,
    net_client_list: Atom,
    net_client_list_stacking: Atom,
    net_wm_desktop: Atom,
    net_wm_icon: Atom,
    net_wm_name: Atom,
    net_wm_pid: Atom,
    net_wm_state: Atom,
    net_wm_state_above: Atom,
    net_wm_state_demands_attention: Atom,
    net_wm_state_hidden: Atom,
    net_wm_state_skip_pager: Atom,
    net_wm_state_skip_taskbar: Atom,
    net_wm_state_sticky: Atom,
    net_wm_strut: Atom,
    net_wm_strut_partial: Atom,
    net_wm_window_type: Atom,
    net_wm_window_type_dock: Atom,
    osdockx_perf_request: Atom,
    osdockx_perf_snapshot: Atom,
    utf8_string: Atom,
    wm_change_state: Atom,
    wm_class: Atom,
    wm_delete_window: Atom,
    wm_name: Atom,
    wm_protocols: Atom,
}

impl CachedWindowMetadata {
    fn title(&self) -> Option<String> {
        self.net_title
            .clone()
            .or_else(|| self.legacy_title.clone())
    }

    fn as_window_info(&self, xid: WindowId, active_window: Option<WindowId>) -> Option<WindowInfo> {
        let title = self.title();
        if title.is_none() && self.class.is_none() {
            return None;
        }
        Some(WindowInfo {
            xid,
            title,
            class: self.class.clone(),
            pid: self.pid,
            executable: self.executable.clone(),
            workspace: self.workspace,
            icon: self.icon.clone(),
            active: active_window == Some(xid),
            urgent: self.urgent,
            minimized: self.minimized,
        })
    }
}

impl WindowMetadataCache {
    fn visible_windows(&self, dock_window: Option<WindowId>) -> Vec<WindowInfo> {
        self.client_ids
            .iter()
            .filter(|xid| Some(**xid) != dock_window)
            .filter_map(|xid| {
                self.windows
                    .get(xid)
                    .and_then(|metadata| metadata.as_window_info(*xid, self.active_window))
            })
            .collect()
    }

    fn icon_signature(&self, xid: WindowId) -> Option<u64> {
        self.windows
            .get(&xid)
            .and_then(|metadata| metadata.icon.as_ref())
            .map(WindowIcon::signature)
    }

    fn replace_from_reconciliation(
        &mut self,
        client_ids: Vec<WindowId>,
        active_window: Option<WindowId>,
        windows: HashMap<WindowId, CachedWindowMetadata>,
        reconciled_at: Instant,
    ) {
        self.client_ids = client_ids;
        self.active_window = active_window;
        self.windows = windows;
        self.initialized = true;
        self.last_reconciliation = Some(reconciled_at);
    }
}

impl X11Backend {
    pub fn new() -> anyhow::Result<Self> {
        let (conn, screen_num) = x11rb::connect(None).context("connect to X11")?;
        let root = conn.setup().roots[screen_num].root;
        let atoms = Atoms::intern(&conn)?;
        conn.change_window_attributes(
            root,
            &ChangeWindowAttributesAux::new()
                .event_mask(EventMask::PROPERTY_CHANGE | EventMask::SUBSTRUCTURE_NOTIFY),
        )?;
        conn.flush()?;

        Ok(Self {
            conn,
            screen_num,
            root,
            atoms,
            dock_window: None,
            metadata: WindowMetadataCache::default(),
            reconciliation_interval: FULL_RECONCILIATION_INTERVAL,
        })
    }

    pub fn set_reconciliation_interval(&mut self, interval: Duration) {
        self.reconciliation_interval = interval.max(FULL_RECONCILIATION_INTERVAL);
    }

    pub(crate) fn root_pointer_position(&self) -> anyhow::Result<Option<(i32, i32)>> {
        let pointer = self.conn.query_pointer(self.root)?.reply()?;
        Ok(pointer
            .same_screen
            .then_some((i32::from(pointer.root_x), i32::from(pointer.root_y))))
    }

    fn configure_dock(
        &self,
        xid: WindowId,
        geometry: DockGeometry,
        update_reserved_space: bool,
    ) -> anyhow::Result<()> {
        self.conn.configure_window(
            xid,
            &ConfigureWindowAux::new()
                .x(geometry.x)
                .y(geometry.y)
                .width(geometry.width)
                .height(geometry.height)
                .stack_mode(StackMode::ABOVE),
        )?;

        if update_reserved_space {
            self.conn.change_property32(
                PropMode::REPLACE,
                xid,
                self.atoms.net_wm_window_type,
                AtomEnum::ATOM,
                &[self.atoms.net_wm_window_type_dock],
            )?;
            self.conn.change_property32(
                PropMode::REPLACE,
                xid,
                self.atoms.net_wm_state,
                AtomEnum::ATOM,
                &[
                    self.atoms.net_wm_state_above,
                    self.atoms.net_wm_state_skip_taskbar,
                    self.atoms.net_wm_state_skip_pager,
                    self.atoms.net_wm_state_sticky,
                ],
            )?;

            let strut = self.struts_for(geometry);
            self.conn.change_property32(
                PropMode::REPLACE,
                xid,
                self.atoms.net_wm_strut_partial,
                AtomEnum::CARDINAL,
                &strut,
            )?;
            self.conn.change_property32(
                PropMode::REPLACE,
                xid,
                self.atoms.net_wm_strut,
                AtomEnum::CARDINAL,
                &strut[0..4],
            )?;
        }
        self.conn.flush()?;
        Ok(())
    }

    fn publish_perf_snapshot(&self) -> anyhow::Result<()> {
        let Some(xid) = self.dock_window else {
            return Ok(());
        };
        let nonce = self
            .property_u32(
                xid,
                self.atoms.osdockx_perf_request,
                AtomEnum::CARDINAL.into(),
            )?
            .unwrap_or(0);
        let words = crate::perf::snapshot().encode_x11(nonce);
        self.conn.change_property32(
            PropMode::REPLACE,
            xid,
            self.atoms.osdockx_perf_snapshot,
            AtomEnum::CARDINAL,
            &words,
        )?;
        self.conn.flush()?;
        Ok(())
    }

    fn struts_for(&self, geometry: DockGeometry) -> [u32; 12] {
        if !geometry.reserve_space {
            return [0; 12];
        }

        let screen = &self.conn.setup().roots[self.screen_num];
        let root_width = screen.width_in_pixels as i32;
        let root_height = screen.height_in_pixels as i32;
        let reserved = geometry.reserved_thickness.max(1) as i32;
        let x_start = geometry.x.max(0) as u32;
        let x_end = (geometry.x + geometry.width as i32 - 1).clamp(0, root_width - 1) as u32;
        let y_start = geometry.y.max(0) as u32;
        let y_end = (geometry.y + geometry.height as i32 - 1).clamp(0, root_height - 1) as u32;

        let mut strut = [0_u32; 12];
        match geometry.edge {
            DockEdge::Left => {
                strut[0] = reserved as u32;
                strut[4] = y_start;
                strut[5] = y_end;
            }
            DockEdge::Right => {
                strut[1] = reserved as u32;
                strut[6] = y_start;
                strut[7] = y_end;
            }
            DockEdge::Top => {
                strut[2] = reserved as u32;
                strut[8] = x_start;
                strut[9] = x_end;
            }
            DockEdge::Bottom => {
                strut[3] = reserved as u32;
                strut[10] = x_start;
                strut[11] = x_end;
            }
        }
        strut
    }

    fn active_window(&self) -> Option<WindowId> {
        self.property_u32(
            self.root,
            self.atoms.net_active_window,
            AtomEnum::WINDOW.into(),
        )
        .ok()
        .flatten()
    }

    fn window_list(&self) -> anyhow::Result<Vec<WindowId>> {
        self.property_u32_list(
            self.root,
            self.atoms.net_client_list_stacking,
            AtomEnum::WINDOW.into(),
        )
        .or_else(|_| {
            self.property_u32_list(
                self.root,
                self.atoms.net_client_list,
                AtomEnum::WINDOW.into(),
            )
        })
    }

    fn window_list_for_property(&self, property: Atom) -> anyhow::Result<Vec<WindowId>> {
        if property == self.atoms.net_client_list {
            self.property_u32_list(self.root, property, AtomEnum::WINDOW.into())
        } else {
            self.window_list()
        }
    }

    fn fetch_window_metadata(&self, xid: WindowId) -> CachedWindowMetadata {
        let states = self
            .property_u32_list(xid, self.atoms.net_wm_state, AtomEnum::ATOM.into())
            .unwrap_or_default()
            .into_iter()
            .collect::<HashSet<_>>();
        let pid = self
            .property_u32(xid, self.atoms.net_wm_pid, AtomEnum::CARDINAL.into())
            .ok()
            .flatten();
        CachedWindowMetadata {
            net_title: self
                .property_string(xid, self.atoms.net_wm_name, self.atoms.utf8_string)
                .ok()
                .flatten(),
            legacy_title: self
                .property_string(xid, self.atoms.wm_name, AtomEnum::STRING.into())
                .ok()
                .flatten(),
            class: self.window_class(xid),
            pid,
            executable: pid.and_then(executable_for_pid),
            workspace: self
                .property_u32(xid, self.atoms.net_wm_desktop, AtomEnum::CARDINAL.into())
                .ok()
                .flatten(),
            icon: self.fetch_window_icon(xid),
            urgent: states.contains(&self.atoms.net_wm_state_demands_attention),
            minimized: states.contains(&self.atoms.net_wm_state_hidden),
        }
    }

    fn window_class(&self, xid: WindowId) -> Option<String> {
        let reply = self
            .get_property(xid, self.atoms.wm_class, AtomEnum::STRING.into(), 1024)
            .ok()?;
        let bytes = reply.value;
        let mut parts = bytes
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty());
        let instance = parts.next();
        let class = parts.next().or(instance)?;
        String::from_utf8(class.to_vec()).ok()
    }

    fn fetch_window_icon(&self, xid: WindowId) -> Option<WindowIcon> {
        let values = self
            .property_u32_list(xid, self.atoms.net_wm_icon, AtomEnum::CARDINAL.into())
            .ok()?;
        parse_window_icon(&values)
    }

    fn select_client_events(&self, xid: WindowId) {
        let _ = self.conn.change_window_attributes(
            xid,
            &ChangeWindowAttributesAux::new()
                .event_mask(EventMask::PROPERTY_CHANGE | EventMask::STRUCTURE_NOTIFY),
        );
    }

    pub fn poll_window_updates(&mut self) -> anyhow::Result<X11WindowUpdate> {
        let mut update = self.drain_window_events()?;
        let needs_reconciliation = !self.metadata.initialized
            || self
                .metadata
                .last_reconciliation
                .is_none_or(|last| last.elapsed() >= self.reconciliation_interval);
        if needs_reconciliation {
            let reconciliation = self.reconcile_window_metadata()?;
            merge_window_updates(&mut update, reconciliation);
        }
        Ok(update)
    }

    pub fn cached_windows(&self) -> Vec<WindowInfo> {
        self.metadata.visible_windows(self.dock_window)
    }

    fn drain_window_events(&mut self) -> anyhow::Result<X11WindowUpdate> {
        let mut update = X11WindowUpdate::default();
        while let Some(event) = self.conn.poll_for_event()? {
            match event {
                Event::PropertyNotify(event) if event.window == self.root => {
                    if event.atom == self.atoms.net_client_list
                        || event.atom == self.atoms.net_client_list_stacking
                    {
                        let changed = self.update_client_list(event.atom, &mut update)?;
                        if changed {
                            update.changes.push(WindowCacheChange::ClientList);
                        }
                    } else if event.atom == self.atoms.net_active_window
                        && self.update_active_window()
                    {
                        update.changes.push(WindowCacheChange::ActiveWindow);
                    }
                }
                Event::PropertyNotify(event) => {
                    if Some(event.window) == self.dock_window
                        && event.atom == self.atoms.osdockx_perf_request
                    {
                        if let Err(error) = self.publish_perf_snapshot() {
                            tracing::debug!("could not publish performance snapshot: {error:#}");
                        }
                    } else if let Some(field) = self.property_field(event.atom)
                        && self.update_window_field(event.window, field, &mut update)
                    {
                        update.changes.push(WindowCacheChange::Field {
                            xid: event.window,
                            field,
                        });
                    }
                }
                Event::DestroyNotify(event) => {
                    if self.remove_destroyed_window(event.window, &mut update) {
                        update
                            .changes
                            .push(WindowCacheChange::Destroyed(event.window));
                    }
                }
                _ => {}
            }
        }
        if !update.changes.is_empty() {
            update.windows = Some(self.cached_windows());
        }
        Ok(update)
    }

    fn reconcile_window_metadata(&mut self) -> anyhow::Result<X11WindowUpdate> {
        crate::perf::record_x11_reconciliation();
        let was_initialized = self.metadata.initialized;
        let before = self.cached_windows();
        let old_icons = self
            .metadata
            .windows
            .iter()
            .filter_map(|(xid, metadata)| {
                metadata
                    .icon
                    .as_ref()
                    .map(|icon| (*xid, icon.signature()))
            })
            .collect::<HashMap<_, _>>();
        let active_window = self.active_window();
        let client_ids = self.window_list()?;
        let mut windows = HashMap::with_capacity(client_ids.len());
        for xid in &client_ids {
            if Some(*xid) == self.dock_window {
                continue;
            }
            self.select_client_events(*xid);
            windows.insert(*xid, self.fetch_window_metadata(*xid));
        }
        let _ = self.conn.flush();

        let mut invalidated_icon_signatures = Vec::new();
        for (xid, signature) in old_icons {
            let current = windows
                .get(&xid)
                .and_then(|metadata| metadata.icon.as_ref())
                .map(WindowIcon::signature);
            if current != Some(signature) {
                push_unique_signature(&mut invalidated_icon_signatures, signature);
            }
        }
        self.metadata.replace_from_reconciliation(
            client_ids,
            active_window,
            windows,
            Instant::now(),
        );
        let after = self.cached_windows();
        let changed = !was_initialized || before != after;
        Ok(X11WindowUpdate {
            windows: changed.then_some(after),
            invalidated_icon_signatures,
            changes: changed.then_some(WindowCacheChange::ClientList).into_iter().collect(),
        })
    }

    fn update_client_list(
        &mut self,
        property: Atom,
        update: &mut X11WindowUpdate,
    ) -> anyhow::Result<bool> {
        let before = self.cached_windows();
        let previous_ids = self.metadata.client_ids.clone();
        let client_ids = self.window_list_for_property(property)?;
        let current_ids = client_ids.iter().copied().collect::<HashSet<_>>();
        for xid in previous_ids {
            if !current_ids.contains(&xid) {
                if let Some(signature) = self.metadata.icon_signature(xid) {
                    push_unique_signature(&mut update.invalidated_icon_signatures, signature);
                }
                self.metadata.windows.remove(&xid);
            }
        }
        for xid in &client_ids {
            if Some(*xid) == self.dock_window || self.metadata.windows.contains_key(xid) {
                continue;
            }
            self.select_client_events(*xid);
            self.metadata
                .windows
                .insert(*xid, self.fetch_window_metadata(*xid));
        }
        self.metadata.client_ids = client_ids;
        let _ = self.conn.flush();
        Ok(before != self.cached_windows())
    }

    fn update_active_window(&mut self) -> bool {
        let active_window = self.active_window();
        if active_window == self.metadata.active_window {
            return false;
        }
        self.metadata.active_window = active_window;
        true
    }

    fn update_window_field(
        &mut self,
        xid: WindowId,
        field: WindowMetadataField,
        update: &mut X11WindowUpdate,
    ) -> bool {
        let Some(before) = self.metadata.windows.get(&xid).cloned() else {
            return false;
        };
        let mut after = before.clone();
        match field {
            WindowMetadataField::State => {
                let states = self
                    .property_u32_list(xid, self.atoms.net_wm_state, AtomEnum::ATOM.into())
                    .unwrap_or_default()
                    .into_iter()
                    .collect::<HashSet<_>>();
                after.urgent = states.contains(&self.atoms.net_wm_state_demands_attention);
                after.minimized = states.contains(&self.atoms.net_wm_state_hidden);
            }
            WindowMetadataField::NetTitle => {
                after.net_title = self
                    .property_string(xid, self.atoms.net_wm_name, self.atoms.utf8_string)
                    .ok()
                    .flatten();
            }
            WindowMetadataField::LegacyTitle => {
                after.legacy_title = self
                    .property_string(xid, self.atoms.wm_name, AtomEnum::STRING.into())
                    .ok()
                    .flatten();
            }
            WindowMetadataField::Class => after.class = self.window_class(xid),
            WindowMetadataField::Pid => {
                after.pid = self
                    .property_u32(xid, self.atoms.net_wm_pid, AtomEnum::CARDINAL.into())
                    .ok()
                    .flatten();
                after.executable = after.pid.and_then(executable_for_pid);
            }
            WindowMetadataField::Workspace => {
                after.workspace = self
                    .property_u32(xid, self.atoms.net_wm_desktop, AtomEnum::CARDINAL.into())
                    .ok()
                    .flatten();
            }
            WindowMetadataField::Icon => {
                after.icon = self.fetch_window_icon(xid);
                if before.icon != after.icon
                    && let Some(signature) = before.icon.as_ref().map(WindowIcon::signature)
                {
                    push_unique_signature(&mut update.invalidated_icon_signatures, signature);
                }
            }
        }
        if before == after {
            return false;
        }
        self.metadata.windows.insert(xid, after);
        true
    }

    fn remove_destroyed_window(
        &mut self,
        xid: WindowId,
        update: &mut X11WindowUpdate,
    ) -> bool {
        let Some(removed) = self.metadata.windows.remove(&xid) else {
            return false;
        };
        if let Some(signature) = removed.icon.as_ref().map(WindowIcon::signature) {
            push_unique_signature(&mut update.invalidated_icon_signatures, signature);
        }
        self.metadata.client_ids.retain(|candidate| *candidate != xid);
        true
    }

    fn property_field(&self, atom: Atom) -> Option<WindowMetadataField> {
        metadata_field_for_atom(
            atom,
            [
                (self.atoms.net_wm_state, WindowMetadataField::State),
                (self.atoms.net_wm_name, WindowMetadataField::NetTitle),
                (self.atoms.wm_name, WindowMetadataField::LegacyTitle),
                (self.atoms.wm_class, WindowMetadataField::Class),
                (self.atoms.net_wm_pid, WindowMetadataField::Pid),
                (self.atoms.net_wm_desktop, WindowMetadataField::Workspace),
                (self.atoms.net_wm_icon, WindowMetadataField::Icon),
            ],
        )
    }

    fn randr_monitor_geometry(&self, preferred: Option<&str>) -> anyhow::Result<MonitorGeometry> {
        let reply = self.conn.randr_get_monitors(self.root, true)?.reply()?;
        let monitors = reply
            .monitors
            .into_iter()
            .filter(|monitor| monitor.width > 0 && monitor.height > 0)
            .collect::<Vec<_>>();
        if monitors.is_empty() {
            anyhow::bail!("RandR returned no active monitors");
        }

        if let Some(preferred) = preferred.map(str::trim).filter(|value| !value.is_empty()) {
            if let Some(monitor) = self.preferred_monitor(&monitors, preferred) {
                return Ok(monitor_geometry(monitor));
            }
            tracing::warn!("configured monitor '{preferred}' was not found; using primary monitor");
        }

        let monitor = monitors
            .iter()
            .find(|monitor| monitor.primary)
            .unwrap_or(&monitors[0]);
        Ok(monitor_geometry(monitor))
    }

    fn preferred_monitor<'a>(
        &self,
        monitors: &'a [MonitorInfo],
        preferred: &str,
    ) -> Option<&'a MonitorInfo> {
        if preferred.eq_ignore_ascii_case("primary") {
            return monitors.iter().find(|monitor| monitor.primary);
        }

        if let Ok(index) = preferred.parse::<usize>() {
            return monitors.get(index);
        }

        monitors.iter().find(|monitor| {
            self.atom_name(monitor.name)
                .is_some_and(|name| name.eq_ignore_ascii_case(preferred))
        })
    }

    fn screen_geometry(&self) -> MonitorGeometry {
        let screen = &self.conn.setup().roots[self.screen_num];
        MonitorGeometry {
            x: 0,
            y: 0,
            width: screen.width_in_pixels.into(),
            height: screen.height_in_pixels.into(),
        }
    }

    fn atom_name(&self, atom: Atom) -> Option<String> {
        if atom == AtomEnum::NONE.into() {
            return None;
        }
        self.conn
            .get_atom_name(atom)
            .ok()?
            .reply()
            .ok()
            .and_then(|reply| String::from_utf8(reply.name).ok())
    }

    fn property_u32(
        &self,
        window: Window,
        property: Atom,
        ty: Atom,
    ) -> anyhow::Result<Option<u32>> {
        let reply = self.get_property(window, property, ty, 1)?;
        Ok(reply.value32().and_then(|mut values| values.next()))
    }

    fn property_u32_list(
        &self,
        window: Window,
        property: Atom,
        ty: Atom,
    ) -> anyhow::Result<Vec<u32>> {
        let reply = self.get_property(window, property, ty, u32::MAX)?;
        Ok(reply
            .value32()
            .map(|values| values.collect())
            .unwrap_or_default())
    }

    fn property_string(
        &self,
        window: Window,
        property: Atom,
        ty: Atom,
    ) -> anyhow::Result<Option<String>> {
        let reply = self.get_property(window, property, ty, 4096)?;
        if reply.value.is_empty() {
            return Ok(None);
        }
        Ok(String::from_utf8(reply.value).ok())
    }

    fn get_property(
        &self,
        window: Window,
        property: Atom,
        ty: Atom,
        len: u32,
    ) -> anyhow::Result<GetPropertyReply> {
        crate::perf::record_x11_property_request();
        Ok(self
            .conn
            .get_property(false, window, property, ty, 0, len)?
            .reply()?)
    }

    fn send_root_message(
        &self,
        xid: WindowId,
        message: Atom,
        data: [u32; 5],
    ) -> anyhow::Result<()> {
        let event = ClientMessageEvent::new(32, xid, message, ClientMessageData::from(data));
        self.conn.send_event(
            false,
            self.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            event,
        )?;
        self.conn.flush()?;
        Ok(())
    }

    fn send_client_message(
        &self,
        xid: WindowId,
        message: Atom,
        data: [u32; 5],
    ) -> anyhow::Result<()> {
        let event = ClientMessageEvent::new(32, xid, message, ClientMessageData::from(data));
        self.conn
            .send_event(false, xid, EventMask::NO_EVENT, event)?;
        self.conn.flush()?;
        Ok(())
    }

    fn apply_shape(
        &self,
        xid: WindowId,
        size: (i32, i32),
        kind: SK,
        regions: &[Rect],
    ) -> anyhow::Result<()> {
        let rectangles = shape_rectangles(size, regions);
        self.conn.shape_rectangles(
            SO::SET,
            kind,
            ClipOrdering::UNSORTED,
            xid,
            0,
            0,
            &rectangles,
        )?;
        Ok(())
    }
}

impl PlatformBackend for X11Backend {
    fn monitor_geometry(&self, preferred: Option<&str>) -> MonitorGeometry {
        self.randr_monitor_geometry(preferred)
            .unwrap_or_else(|error| {
                tracing::debug!("RandR monitor geometry unavailable: {error:#}");
                self.screen_geometry()
            })
    }

    fn set_dock_window(&mut self, xid: WindowId, geometry: DockGeometry) -> anyhow::Result<()> {
        self.dock_window = Some(xid);
        self.conn.change_window_attributes(
            xid,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?;
        self.configure_dock(xid, geometry, true)
    }

    fn move_dock_window(
        &mut self,
        geometry: DockGeometry,
        update_reserved_space: bool,
    ) -> anyhow::Result<()> {
        if let Some(xid) = self.dock_window {
            self.configure_dock(xid, geometry, update_reserved_space)?;
        }
        Ok(())
    }

    fn set_dock_shape(
        &mut self,
        size: (i32, i32),
        visual_regions: &[Rect],
        input_regions: &[Rect],
    ) -> anyhow::Result<()> {
        if let Some(xid) = self.dock_window {
            self.apply_shape(xid, size, SK::BOUNDING, visual_regions)?;
            self.apply_shape(xid, size, SK::INPUT, input_regions)?;
            self.conn.flush()?;
        }
        Ok(())
    }

    fn poll_windows(&mut self) -> anyhow::Result<Vec<WindowInfo>> {
        let update = self.poll_window_updates()?;
        Ok(update.windows.unwrap_or_else(|| self.cached_windows()))
    }

    fn focus_window(&self, xid: WindowId) -> anyhow::Result<()> {
        // Hidden or tray-minimized clients may ignore activation until they are remapped.
        self.conn.map_window(xid)?;
        self.conn
            .configure_window(xid, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
        self.send_root_message(
            xid,
            self.atoms.net_active_window,
            [2, CURRENT_TIME, 0, 0, 0],
        )?;
        self.conn
            .set_input_focus(InputFocus::PARENT, xid, CURRENT_TIME)?;
        self.conn.flush()?;
        Ok(())
    }

    fn minimize_window(&self, xid: WindowId) -> anyhow::Result<()> {
        const ICONIC_STATE: u32 = 3;
        self.send_root_message(xid, self.atoms.wm_change_state, [ICONIC_STATE, 0, 0, 0, 0])
    }

    fn close_window(&self, xid: WindowId) -> anyhow::Result<()> {
        self.send_client_message(
            xid,
            self.atoms.wm_protocols,
            [self.atoms.wm_delete_window, CURRENT_TIME, 0, 0, 0],
        )
    }
}

fn merge_window_updates(target: &mut X11WindowUpdate, source: X11WindowUpdate) {
    if source.windows.is_some() {
        target.windows = source.windows;
    }
    for signature in source.invalidated_icon_signatures {
        push_unique_signature(&mut target.invalidated_icon_signatures, signature);
    }
    target.changes.extend(source.changes);
}

fn push_unique_signature(signatures: &mut Vec<u64>, signature: u64) {
    if !signatures.contains(&signature) {
        signatures.push(signature);
    }
}

fn metadata_field_for_atom<const N: usize>(
    atom: Atom,
    mappings: [(Atom, WindowMetadataField); N],
) -> Option<WindowMetadataField> {
    mappings
        .into_iter()
        .find_map(|(candidate, field)| (atom == candidate).then_some(field))
}

impl Atoms {
    fn intern(conn: &RustConnection) -> anyhow::Result<Self> {
        Ok(Self {
            net_active_window: intern(conn, b"_NET_ACTIVE_WINDOW")?,
            net_client_list: intern(conn, b"_NET_CLIENT_LIST")?,
            net_client_list_stacking: intern(conn, b"_NET_CLIENT_LIST_STACKING")?,
            net_wm_desktop: intern(conn, b"_NET_WM_DESKTOP")?,
            net_wm_icon: intern(conn, b"_NET_WM_ICON")?,
            net_wm_name: intern(conn, b"_NET_WM_NAME")?,
            net_wm_pid: intern(conn, b"_NET_WM_PID")?,
            net_wm_state: intern(conn, b"_NET_WM_STATE")?,
            net_wm_state_above: intern(conn, b"_NET_WM_STATE_ABOVE")?,
            net_wm_state_demands_attention: intern(conn, b"_NET_WM_STATE_DEMANDS_ATTENTION")?,
            net_wm_state_hidden: intern(conn, b"_NET_WM_STATE_HIDDEN")?,
            net_wm_state_skip_pager: intern(conn, b"_NET_WM_STATE_SKIP_PAGER")?,
            net_wm_state_skip_taskbar: intern(conn, b"_NET_WM_STATE_SKIP_TASKBAR")?,
            net_wm_state_sticky: intern(conn, b"_NET_WM_STATE_STICKY")?,
            net_wm_strut: intern(conn, b"_NET_WM_STRUT")?,
            net_wm_strut_partial: intern(conn, b"_NET_WM_STRUT_PARTIAL")?,
            net_wm_window_type: intern(conn, b"_NET_WM_WINDOW_TYPE")?,
            net_wm_window_type_dock: intern(conn, b"_NET_WM_WINDOW_TYPE_DOCK")?,
            osdockx_perf_request: intern(conn, crate::perf::X11_PERF_REQUEST_PROPERTY)?,
            osdockx_perf_snapshot: intern(conn, crate::perf::X11_PERF_SNAPSHOT_PROPERTY)?,
            utf8_string: intern(conn, b"UTF8_STRING")?,
            wm_change_state: intern(conn, b"WM_CHANGE_STATE")?,
            wm_class: intern(conn, b"WM_CLASS")?,
            wm_delete_window: intern(conn, b"WM_DELETE_WINDOW")?,
            wm_name: intern(conn, b"WM_NAME")?,
            wm_protocols: intern(conn, b"WM_PROTOCOLS")?,
        })
    }
}

fn intern(conn: &RustConnection, name: &[u8]) -> anyhow::Result<Atom> {
    Ok(conn.intern_atom(false, name)?.reply()?.atom)
}

fn monitor_geometry(monitor: &MonitorInfo) -> MonitorGeometry {
    MonitorGeometry {
        x: monitor.x.into(),
        y: monitor.y.into(),
        width: monitor.width.into(),
        height: monitor.height.into(),
    }
}

fn executable_for_pid(pid: u32) -> Option<String> {
    let path = fs::read_link(PathBuf::from("/proc").join(pid.to_string()).join("exe")).ok()?;
    Some(path.to_string_lossy().into_owned())
}

fn parse_window_icon(values: &[u32]) -> Option<WindowIcon> {
    let mut offset = 0_usize;
    let mut best: Option<WindowIcon> = None;
    while offset + 2 <= values.len() {
        let width = values[offset];
        let height = values[offset + 1];
        offset += 2;

        let len = width.checked_mul(height)? as usize;
        if width == 0 || height == 0 || offset + len > values.len() {
            return best;
        }

        let argb = values[offset..offset + len].to_vec();
        offset += len;

        let candidate = WindowIcon::from_argb(width, height, argb);
        let candidate_area = candidate.width.saturating_mul(candidate.height);
        let best_area = best
            .as_ref()
            .map(|icon| icon.width.saturating_mul(icon.height))
            .unwrap_or(0);
        if candidate_area > best_area {
            best = Some(candidate);
        }
    }
    best
}

fn shape_rectangles(size: (i32, i32), regions: &[Rect]) -> Vec<Rectangle> {
    let full = [Rect {
        x: 0.0,
        y: 0.0,
        width: size.0.max(1) as f64,
        height: size.1.max(1) as f64,
    }];
    let source = if regions.is_empty() {
        &full[..]
    } else {
        regions
    };

    source
        .iter()
        .filter_map(|region| {
            let x0 = region.x.floor().clamp(0.0, size.0.max(1) as f64) as i32;
            let y0 = region.y.floor().clamp(0.0, size.1.max(1) as f64) as i32;
            let x1 = (region.x + region.width)
                .ceil()
                .clamp(0.0, size.0.max(1) as f64) as i32;
            let y1 = (region.y + region.height)
                .ceil()
                .clamp(0.0, size.1.max(1) as f64) as i32;
            let width = u16::try_from((x1 - x0).max(0)).ok()?;
            let height = u16::try_from((y1 - y0).max(0)).ok()?;
            if width == 0 || height == 0 {
                return None;
            }
            Some(Rectangle {
                x: x0.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
                y: y0.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
                width,
                height,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_largest_net_wm_icon() {
        let values = [
            1, 1, 0xff00ff00, 2, 2, 0xff000000, 0xff111111, 0xff222222, 0xff333333,
        ];
        let icon = parse_window_icon(&values).unwrap();
        assert_eq!(icon.width, 2);
        assert_eq!(icon.height, 2);
        assert_eq!(icon.argb.len(), 4);
    }

    #[test]
    fn clamps_shape_rectangles_to_window() {
        let rectangles = shape_rectangles(
            (100, 40),
            &[Rect {
                x: -10.0,
                y: 4.2,
                width: 30.0,
                height: 80.0,
            }],
        );

        assert_eq!(rectangles[0].x, 0);
        assert_eq!(rectangles[0].y, 4);
        assert_eq!(rectangles[0].width, 20);
        assert_eq!(rectangles[0].height, 36);
    }

    #[test]
    fn property_atoms_map_to_only_their_cached_field() {
        let mappings = [
            (10, WindowMetadataField::State),
            (11, WindowMetadataField::NetTitle),
            (12, WindowMetadataField::LegacyTitle),
            (13, WindowMetadataField::Class),
            (14, WindowMetadataField::Pid),
            (15, WindowMetadataField::Workspace),
            (16, WindowMetadataField::Icon),
        ];
        for (atom, expected) in mappings {
            assert_eq!(metadata_field_for_atom(atom, mappings), Some(expected));
        }
        assert_eq!(metadata_field_for_atom(99, mappings), None);
    }

    #[test]
    fn metadata_cache_tracks_order_focus_destroy_and_reused_xids() {
        let mut cache = WindowMetadataCache::default();
        cache.client_ids = vec![41, 42];
        cache.active_window = Some(42);
        cache.windows.insert(
            41,
            CachedWindowMetadata {
                net_title: Some("First".to_string()),
                class: Some("first".to_string()),
                ..CachedWindowMetadata::default()
            },
        );
        cache.windows.insert(
            42,
            CachedWindowMetadata {
                net_title: Some("Second".to_string()),
                class: Some("second".to_string()),
                ..CachedWindowMetadata::default()
            },
        );

        let visible = cache.visible_windows(None);
        assert_eq!(visible.iter().map(|window| window.xid).collect::<Vec<_>>(), vec![41, 42]);
        assert!(!visible[0].active);
        assert!(visible[1].active);

        cache.client_ids.retain(|xid| *xid != 41);
        cache.windows.remove(&41);
        assert_eq!(cache.visible_windows(None).len(), 1);

        cache.client_ids.insert(0, 41);
        cache.windows.insert(
            41,
            CachedWindowMetadata {
                net_title: Some("Reused".to_string()),
                class: Some("replacement".to_string()),
                ..CachedWindowMetadata::default()
            },
        );
        assert_eq!(cache.visible_windows(None)[0].title.as_deref(), Some("Reused"));
    }

    #[test]
    fn update_merging_preserves_reconciliation_and_icon_invalidations() {
        let mut target = X11WindowUpdate {
            invalidated_icon_signatures: vec![7],
            ..X11WindowUpdate::default()
        };
        let windows = vec![WindowInfo {
            xid: 1,
            title: Some("Window".to_string()),
            class: Some("class".to_string()),
            pid: None,
            executable: None,
            workspace: None,
            icon: None,
            active: false,
            urgent: false,
            minimized: false,
        }];
        merge_window_updates(
            &mut target,
            X11WindowUpdate {
                windows: Some(windows.clone()),
                invalidated_icon_signatures: vec![7, 8],
                changes: vec![WindowCacheChange::ClientList],
            },
        );

        assert_eq!(target.windows, Some(windows));
        assert_eq!(target.invalidated_icon_signatures, vec![7, 8]);
        assert_eq!(target.changes, vec![WindowCacheChange::ClientList]);
    }

    #[test]
    fn full_reconciliation_replaces_intentionally_stale_cache_state() {
        let mut cache = WindowMetadataCache::default();
        cache.client_ids = vec![70];
        cache.windows.insert(
            70,
            CachedWindowMetadata {
                net_title: Some("Stale".to_string()),
                ..CachedWindowMetadata::default()
            },
        );
        let mut repaired = HashMap::new();
        repaired.insert(
            71,
            CachedWindowMetadata {
                net_title: Some("Repaired".to_string()),
                class: Some("app".to_string()),
                ..CachedWindowMetadata::default()
            },
        );

        cache.replace_from_reconciliation(vec![71], Some(71), repaired, Instant::now());

        let visible = cache.visible_windows(None);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].xid, 71);
        assert_eq!(visible[0].title.as_deref(), Some("Repaired"));
        assert!(visible[0].active);
        assert!(cache.initialized);
    }
}
