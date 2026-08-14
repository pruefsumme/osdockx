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
    }
}

pub fn record_redraw_requested() {
    REDRAWS_REQUESTED.fetch_add(1, Ordering::Relaxed);
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
    let average_draw_us = counters
        .draw_micros
        .checked_div(counters.redraws_completed)
        .unwrap_or(0);
    tracing::debug!(
        target: "osdockx::perf",
        redraws_requested = counters.redraws_requested,
        redraws_completed = counters.redraws_completed,
        average_draw_us,
        max_draw_us = counters.draw_max_micros,
        reflection_builds = counters.reflection_builds,
        reflection_hits = counters.reflection_hits,
        shelf_builds = counters.shelf_builds,
        shelf_hits = counters.shelf_hits,
        config_theme_parses = counters.config_theme_parses,
        x11_property_requests = counters.x11_property_requests,
        x11_reconciliations = counters.x11_reconciliations,
        "aggregate dock performance"
    );
}
