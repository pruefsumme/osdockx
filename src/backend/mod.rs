use crate::config::DockEdge;
use crate::layout::Rect;
use crate::model::{WindowId, WindowInfo};

pub mod x11;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DockGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub edge: DockEdge,
    pub reserve_space: bool,
    pub reserved_thickness: u32,
}

pub trait PlatformBackend {
    fn monitor_geometry(&self, preferred: Option<&str>) -> MonitorGeometry;
    fn set_dock_window(&mut self, xid: WindowId, geometry: DockGeometry) -> anyhow::Result<()>;
    fn move_dock_window(
        &mut self,
        geometry: DockGeometry,
        update_reserved_space: bool,
    ) -> anyhow::Result<()>;
    fn set_dock_shape(
        &mut self,
        size: (i32, i32),
        visual_regions: &[Rect],
        input_regions: &[Rect],
    ) -> anyhow::Result<()>;
    fn poll_windows(&mut self) -> anyhow::Result<Vec<WindowInfo>>;
    fn focus_window(&self, xid: WindowId) -> anyhow::Result<()>;
    fn minimize_window(&self, xid: WindowId) -> anyhow::Result<()>;
    fn close_window(&self, xid: WindowId) -> anyhow::Result<()>;
}

impl MonitorGeometry {
    pub fn dock_geometry(
        &self,
        size: (i32, i32),
        edge: DockEdge,
        reserve_space: bool,
        reserved_thickness: u32,
    ) -> DockGeometry {
        let width = size.0.max(1) as u32;
        let height = size.1.max(1) as u32;
        let monitor_width = self.width as i32;
        let monitor_height = self.height as i32;

        let (x, y) = match edge {
            DockEdge::Bottom => (
                self.x + (monitor_width - width as i32) / 2,
                self.y + monitor_height - height as i32,
            ),
            DockEdge::Top => (self.x + (monitor_width - width as i32) / 2, self.y),
            DockEdge::Left => (self.x, self.y + (monitor_height - height as i32) / 2),
            DockEdge::Right => (
                self.x + monitor_width - width as i32,
                self.y + (monitor_height - height as i32) / 2,
            ),
        };

        DockGeometry {
            x,
            y,
            width,
            height,
            edge,
            reserve_space,
            reserved_thickness,
        }
    }
}

impl From<DockGeometry> for Rect {
    fn from(value: DockGeometry) -> Self {
        Self {
            x: value.x as f64,
            y: value.y as f64,
            width: value.width as f64,
            height: value.height as f64,
        }
    }
}
