use super::STARTUP_REVEAL_DURATION;
use crate::layout::{DockLayout, Point, Rect};
use crate::model::DockItem;
use crate::renderer::IconMotionRect;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub(super) struct IconDrag {
    pub(super) item_key: String,
    pub(super) origin: Point,
    pub(super) current: Point,
    pub(super) grab_offset: Point,
    pub(super) moved: bool,
    pub(super) changed: bool,
}

#[derive(Debug, Clone)]
pub(super) struct SeparatorResize {
    pub(super) start_mouse_y: f64,
    pub(super) start_window_y: i32,
    pub(super) start_icon_size: u32,
    pub(super) target_icon_size: u32,
    pub(super) render_icon_size: f64,
    pub(super) current_icon_size: u32,
}

#[derive(Debug, Clone)]
pub(super) struct DockSizeTransition {
    pub(super) from_icon_size: f64,
    pub(super) to_icon_size: f64,
    pub(super) started: Instant,
    pub(super) duration: Duration,
}

#[derive(Debug, Clone)]
pub(super) struct IconSlide {
    pub(super) from: Vec<IconMotionRect>,
    pub(super) started: Instant,
}

#[derive(Debug, Clone)]
pub(super) struct IconPresenceTransition {
    pub(super) from: Vec<IconMotionRect>,
    pub(super) ghosts: Vec<IconPresenceGhost>,
    pub(super) from_layout: DockLayout,
    pub(super) to_layout: DockLayout,
    pub(super) has_insertions: bool,
    pub(super) has_removals: bool,
    pub(super) started: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct IndicatorVisual {
    pub(super) visibility: f64,
    pub(super) emphasis: f64,
}

#[derive(Debug, Clone)]
pub(super) struct IndicatorAnimation {
    pub(super) from: IndicatorVisual,
    pub(super) to: IndicatorVisual,
    pub(super) started: Instant,
}

#[derive(Debug, Clone)]
pub(super) struct IconPresenceGhost {
    pub(super) item: DockItem,
    pub(super) rect: Rect,
}

#[derive(Debug, Clone)]
pub(super) struct StartupReveal {
    started: Instant,
}

impl StartupReveal {
    pub(super) fn new() -> Self {
        Self {
            started: Instant::now(),
        }
    }

    pub(super) fn progress(&self) -> f64 {
        (self.started.elapsed().as_secs_f64() / STARTUP_REVEAL_DURATION.as_secs_f64())
            .clamp(0.0, 1.0)
    }

    pub(super) fn finished(&self) -> bool {
        self.progress() >= 1.0
    }
}
