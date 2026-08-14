//! Low-overhead aggregate performance instrumentation.
//!
//! The counters deliberately avoid per-frame logging. A summary is emitted at
//! most once every ten seconds when a draw completes; tests and benchmark tools
//! can also take an explicit snapshot.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const SUMMARY_INTERVAL: Duration = Duration::from_secs(10);

static STARTED: OnceLock<Instant> = OnceLock::new();
static LAST_SUMMARY_MS: AtomicU64 = AtomicU64::new(0);
static REDRAWS_REQUESTED: AtomicU64 = AtomicU64::new(0);
static REDRAWS_COMPLETED: AtomicU64 = AtomicU64::new(0);
static DRAW_MICROS: AtomicU64 = AtomicU64::new(0);
static DRAW_MAX_MICROS: AtomicU64 = AtomicU64::new(0);
static REFLECTION_BUILDS: AtomicU64 = AtomicU64::new(0);
static REFLECTION_HITS: AtomicU64 = AtomicU64::new(0);
static SHELF_BUILDS: AtomicU64 = AtomicU64::new(0);
static SHELF_HITS: AtomicU64 = AtomicU64::new(0);
static CONFIG_THEME_PARSES: AtomicU64 = AtomicU64::new(0);
static X11_PROPERTY_REQUESTS: AtomicU64 = AtomicU64::new(0);
static X11_RECONCILIATIONS: AtomicU64 = AtomicU64::new(0);
static MOTION_EVENTS: AtomicU64 = AtomicU64::new(0);
static FRAME_TICKS: AtomicU64 = AtomicU64::new(0);
static VISIBLE_LAYOUT_CHANGES: AtomicU64 = AtomicU64::new(0);
static PAINT_REQUESTS: AtomicU64 = AtomicU64::new(0);
static WINDOW_SYNCHRONIZATIONS: AtomicU64 = AtomicU64::new(0);
static SHAPE_UPDATES: AtomicU64 = AtomicU64::new(0);
static ANIMATION_FRAMES: AtomicU64 = AtomicU64::new(0);
static LAST_SUMMARY: OnceLock<std::sync::Mutex<PerfSnapshot>> = OnceLock::new();

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PerfSnapshot {
    pub redraws_requested: u64,
    pub redraws_completed: u64,
    pub draw_micros: u64,
    pub draw_max_micros: u64,
    pub reflection_builds: u64,
    pub reflection_hits: u64,
    pub shelf_builds: u64,
    pub shelf_hits: u64,
    pub config_theme_parses: u64,
    pub x11_property_requests: u64,
    pub x11_reconciliations: u64,
    pub motion_events: u64,
    pub frame_ticks: u64,
    pub visible_layout_changes: u64,
    pub paint_requests: u64,
    pub window_synchronizations: u64,
    pub shape_updates: u64,
    pub animation_frames: u64,
}

impl PerfSnapshot {
    pub fn saturating_sub(self, earlier: Self) -> Self {
        Self {
            redraws_requested: self
                .redraws_requested
                .saturating_sub(earlier.redraws_requested),
            redraws_completed: self
                .redraws_completed
                .saturating_sub(earlier.redraws_completed),
            draw_micros: self.draw_micros.saturating_sub(earlier.draw_micros),
            draw_max_micros: self.draw_max_micros,
            reflection_builds: self
                .reflection_builds
                .saturating_sub(earlier.reflection_builds),
            reflection_hits: self.reflection_hits.saturating_sub(earlier.reflection_hits),
            shelf_builds: self.shelf_builds.saturating_sub(earlier.shelf_builds),
            shelf_hits: self.shelf_hits.saturating_sub(earlier.shelf_hits),
            config_theme_parses: self
                .config_theme_parses
                .saturating_sub(earlier.config_theme_parses),
            x11_property_requests: self
                .x11_property_requests
                .saturating_sub(earlier.x11_property_requests),
            x11_reconciliations: self
                .x11_reconciliations
                .saturating_sub(earlier.x11_reconciliations),
            motion_events: self.motion_events.saturating_sub(earlier.motion_events),
            frame_ticks: self.frame_ticks.saturating_sub(earlier.frame_ticks),
            visible_layout_changes: self
                .visible_layout_changes
                .saturating_sub(earlier.visible_layout_changes),
            paint_requests: self.paint_requests.saturating_sub(earlier.paint_requests),
            window_synchronizations: self
                .window_synchronizations
                .saturating_sub(earlier.window_synchronizations),
            shape_updates: self.shape_updates.saturating_sub(earlier.shape_updates),
            animation_frames: self
                .animation_frames
                .saturating_sub(earlier.animation_frames),
        }
    }
}

pub fn snapshot() -> PerfSnapshot {
    PerfSnapshot {
        redraws_requested: REDRAWS_REQUESTED.load(Ordering::Relaxed),
        redraws_completed: REDRAWS_COMPLETED.load(Ordering::Relaxed),
        draw_micros: DRAW_MICROS.load(Ordering::Relaxed),
        draw_max_micros: DRAW_MAX_MICROS.load(Ordering::Relaxed),
        reflection_builds: REFLECTION_BUILDS.load(Ordering::Relaxed),
        reflection_hits: REFLECTION_HITS.load(Ordering::Relaxed),
        shelf_builds: SHELF_BUILDS.load(Ordering::Relaxed),
        shelf_hits: SHELF_HITS.load(Ordering::Relaxed),
        config_theme_parses: CONFIG_THEME_PARSES.load(Ordering::Relaxed),
        x11_property_requests: X11_PROPERTY_REQUESTS.load(Ordering::Relaxed),
        x11_reconciliations: X11_RECONCILIATIONS.load(Ordering::Relaxed),
        motion_events: MOTION_EVENTS.load(Ordering::Relaxed),
        frame_ticks: FRAME_TICKS.load(Ordering::Relaxed),
        visible_layout_changes: VISIBLE_LAYOUT_CHANGES.load(Ordering::Relaxed),
        paint_requests: PAINT_REQUESTS.load(Ordering::Relaxed),
        window_synchronizations: WINDOW_SYNCHRONIZATIONS.load(Ordering::Relaxed),
        shape_updates: SHAPE_UPDATES.load(Ordering::Relaxed),
        animation_frames: ANIMATION_FRAMES.load(Ordering::Relaxed),
    }
}

pub fn record_redraw_requested() {
    REDRAWS_REQUESTED.fetch_add(1, Ordering::Relaxed);
    PAINT_REQUESTS.fetch_add(1, Ordering::Relaxed);
}

pub fn record_draw_completed(elapsed: Duration) {
    let micros = elapsed.as_micros().min(u64::MAX as u128) as u64;
    REDRAWS_COMPLETED.fetch_add(1, Ordering::Relaxed);
    DRAW_MICROS.fetch_add(micros, Ordering::Relaxed);
    DRAW_MAX_MICROS.fetch_max(micros, Ordering::Relaxed);
    maybe_log_summary();
}

pub fn record_reflection_build() {
    REFLECTION_BUILDS.fetch_add(1, Ordering::Relaxed);
}

pub fn record_reflection_hit() {
    REFLECTION_HITS.fetch_add(1, Ordering::Relaxed);
}

pub fn record_shelf_build() {
    SHELF_BUILDS.fetch_add(1, Ordering::Relaxed);
}

pub fn record_shelf_hit() {
    SHELF_HITS.fetch_add(1, Ordering::Relaxed);
}

pub fn record_config_theme_parse() {
    CONFIG_THEME_PARSES.fetch_add(1, Ordering::Relaxed);
}

pub fn record_x11_property_request() {
    X11_PROPERTY_REQUESTS.fetch_add(1, Ordering::Relaxed);
}

pub fn record_x11_reconciliation() {
    X11_RECONCILIATIONS.fetch_add(1, Ordering::Relaxed);
}

pub fn record_motion_event() {
    MOTION_EVENTS.fetch_add(1, Ordering::Relaxed);
}

pub fn record_frame_tick() {
    FRAME_TICKS.fetch_add(1, Ordering::Relaxed);
}

pub fn record_visible_layout_change() {
    VISIBLE_LAYOUT_CHANGES.fetch_add(1, Ordering::Relaxed);
}

pub fn record_window_synchronization() {
    WINDOW_SYNCHRONIZATIONS.fetch_add(1, Ordering::Relaxed);
}

pub fn record_shape_update() {
    SHAPE_UPDATES.fetch_add(1, Ordering::Relaxed);
}

pub fn record_animation_frame() {
    ANIMATION_FRAMES.fetch_add(1, Ordering::Relaxed);
}

fn maybe_log_summary() {
    let elapsed_ms = STARTED
        .get_or_init(Instant::now)
        .elapsed()
        .as_millis()
        .min(u64::MAX as u128) as u64;
    let previous = LAST_SUMMARY_MS.load(Ordering::Relaxed);
    if elapsed_ms.saturating_sub(previous) < SUMMARY_INTERVAL.as_millis() as u64
        || LAST_SUMMARY_MS
            .compare_exchange(previous, elapsed_ms, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
    {
        return;
    }

    let counters = snapshot();
    let previous = LAST_SUMMARY
        .get_or_init(|| std::sync::Mutex::new(PerfSnapshot::default()))
        .lock()
        .map(|mut previous| {
            let delta = counters.saturating_sub(*previous);
            *previous = counters;
            delta
        })
        .unwrap_or(counters);
    let average_draw_us = previous
        .draw_micros
        .checked_div(previous.redraws_completed)
        .unwrap_or(0);
    tracing::debug!(
        target: "osdockx::perf",
        redraws_requested = previous.redraws_requested,
        redraws_completed = previous.redraws_completed,
        average_draw_us,
        max_draw_us = counters.draw_max_micros,
        reflection_builds = previous.reflection_builds,
        reflection_hits = previous.reflection_hits,
        shelf_builds = previous.shelf_builds,
        shelf_hits = previous.shelf_hits,
        config_theme_parses = previous.config_theme_parses,
        x11_property_requests = previous.x11_property_requests,
        x11_reconciliations = previous.x11_reconciliations,
        motion_events = previous.motion_events,
        frame_ticks = previous.frame_ticks,
        visible_layout_changes = previous.visible_layout_changes,
        paint_requests = previous.paint_requests,
        window_synchronizations = previous.window_synchronizations,
        shape_updates = previous.shape_updates,
        animation_frames = previous.animation_frames,
        "dock performance delta"
    );
}

#[cfg(test)]
mod tests {
    use super::PerfSnapshot;

    #[test]
    fn snapshot_delta_saturates_each_counter() {
        let current = PerfSnapshot {
            motion_events: 12,
            frame_ticks: 8,
            paint_requests: 4,
            ..PerfSnapshot::default()
        };
        let earlier = PerfSnapshot {
            motion_events: 9,
            frame_ticks: 10,
            paint_requests: 1,
            ..PerfSnapshot::default()
        };

        let delta = current.saturating_sub(earlier);
        assert_eq!(delta.motion_events, 3);
        assert_eq!(delta.frame_ticks, 0);
        assert_eq!(delta.paint_requests, 3);
    }
}
