use anyhow::Context;
use osdockx::perf::{PerfSnapshot, X11_PERF_REQUEST_PROPERTY, X11_PERF_SNAPSHOT_PROPERTY};
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use x11rb::connection::Connection;
use x11rb::protocol::shape::{ConnectionExt as ShapeConnectionExt, SK};
use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt, PropMode, Window};
use x11rb::wrapper::ConnectionExt as WrapperConnectionExt;

static PERF_NONCE: AtomicU32 = AtomicU32::new(1);
const SWEEP_SAMPLES_PER_SECOND: f64 = 2.0;
const SWEEP_CYCLE: Duration = Duration::from_secs(4);
const PASSIVE_SETTLE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy)]
struct ProcessSample {
    ticks: u64,
    rss_kib: u64,
    pss_kib: u64,
    private_kib: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RootWindowGeometry {
    x: i32,
    y: i32,
    width: u16,
    height: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HoverPath {
    lane: i32,
    start: i32,
    end: i32,
}

#[derive(Debug, Clone, Copy)]
struct PhaseResult {
    name: &'static str,
    elapsed: Duration,
    dock_before: ProcessSample,
    dock_after: ProcessSample,
    xorg_before: Option<ProcessSample>,
    xorg_after: Option<ProcessSample>,
    counters: Option<PerfSnapshot>,
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let pid = args
        .next()
        .context("usage: osdockx-perf <dock-pid> [seconds]")?
        .parse::<u32>()
        .context("dock PID must be an integer")?;
    let seconds = args
        .next()
        .map(|value| value.parse::<u64>())
        .transpose()?
        .unwrap_or(60);

    let (conn, screen_num) = x11rb::connect(None).context("connect to X11")?;
    let root = conn.setup().roots[screen_num].root;
    let dock = find_dock_window(&conn, root, pid)?;
    let geometry = window_geometry_on_root(&conn, root, dock)?;
    let pointer = conn.query_pointer(root)?.reply()?;
    anyhow::ensure!(pointer.same_screen, "root pointer is on another X11 screen");
    let original_pointer = (i32::from(pointer.root_x), i32::from(pointer.root_y));
    let screen = &conn.setup().roots[screen_num];
    let screen_size = (
        i32::from(screen.width_in_pixels),
        i32::from(screen.height_in_pixels),
    );
    let xorg_pid = find_process(&["Xorg", "X"]);
    let phase_duration = Duration::from_secs(seconds.max(1));
    let warmup_duration = Duration::from_secs(seconds.clamp(1, 10));
    let hover_path = input_shape_hover_path(&conn, dock, geometry)
        .unwrap_or_else(|_| default_hover_path(geometry));
    let stationary = stationary_hover_point(geometry, hover_path);
    let idle = idle_point(geometry, screen_size, original_pointer);

    let phases = with_pointer_restore(
        || {
            let _ = warp_root_pointer(&conn, root, original_pointer);
        },
        || -> anyhow::Result<Vec<PhaseResult>> {
            let warmup = measure_phase(
                &conn,
                root,
                dock,
                pid,
                xorg_pid,
                "warm-up",
                warmup_duration,
                |duration| run_controlled_sweep(&conn, root, geometry, hover_path, duration),
            )?;

            settle_pointer(&conn, root, stationary)?;
            let stationary = measure_phase(
                &conn,
                root,
                dock,
                pid,
                xorg_pid,
                "stationary-hover",
                phase_duration,
                |duration| {
                    sleep_for(duration);
                    Ok(())
                },
            )?;

            let sweep = measure_phase(
                &conn,
                root,
                dock,
                pid,
                xorg_pid,
                "controlled-sweep",
                phase_duration,
                |duration| run_controlled_sweep(&conn, root, geometry, hover_path, duration),
            )?;

            settle_pointer(&conn, root, idle)?;
            let idle = measure_phase(
                &conn,
                root,
                dock,
                pid,
                xorg_pid,
                "post-sweep-idle",
                phase_duration,
                |duration| {
                    sleep_for(duration);
                    Ok(())
                },
            )?;

            Ok(vec![warmup, stationary, sweep, idle])
        },
    )?;

    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let output = PathBuf::from(format!("/tmp/osdockx-perf-{pid}-{timestamp}.txt"));
    let report = format_report(pid, dock, geometry, xorg_pid, seconds, &phases);
    fs::write(&output, report)?;
    println!("{}", output.display());
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn measure_phase(
    conn: &impl Connection,
    _root: Window,
    dock: Window,
    pid: u32,
    xorg_pid: Option<u32>,
    name: &'static str,
    duration: Duration,
    workload: impl FnOnce(Duration) -> anyhow::Result<()>,
) -> anyhow::Result<PhaseResult> {
    let counters_before = request_perf_snapshot(conn, dock).ok();
    let dock_before = sample_process(pid)?;
    let xorg_before = xorg_pid.map(sample_process).transpose()?;
    let started = Instant::now();
    workload(duration).with_context(|| format!("run {name} phase"))?;
    let elapsed = started.elapsed();
    let dock_after = sample_process(pid)?;
    let xorg_after = xorg_pid.map(sample_process).transpose()?;
    let counters_after = request_perf_snapshot(conn, dock).ok();
    Ok(PhaseResult {
        name,
        elapsed,
        dock_before,
        dock_after,
        xorg_before,
        xorg_after,
        counters: counters_before
            .zip(counters_after)
            .map(|(before, after)| after.saturating_sub(before)),
    })
}

fn request_perf_snapshot(conn: &impl Connection, dock: Window) -> anyhow::Result<PerfSnapshot> {
    let request_atom = intern(conn, X11_PERF_REQUEST_PROPERTY)?;
    let snapshot_atom = intern(conn, X11_PERF_SNAPSHOT_PROPERTY)?;
    let nonce = PERF_NONCE.fetch_add(1, Ordering::Relaxed);
    conn.change_property32(
        PropMode::REPLACE,
        dock,
        request_atom,
        AtomEnum::CARDINAL,
        &[nonce],
    )?;
    conn.flush()?;

    let timeout = Instant::now() + Duration::from_secs(2);
    loop {
        let words = property_list(conn, dock, snapshot_atom, AtomEnum::CARDINAL.into());
        if let Some((response_nonce, snapshot)) = PerfSnapshot::decode_x11(&words)
            && response_nonce == nonce
        {
            return Ok(snapshot);
        }
        if Instant::now() >= timeout {
            anyhow::bail!("dock did not answer a performance snapshot request");
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn run_controlled_sweep(
    conn: &impl Connection,
    root: Window,
    geometry: RootWindowGeometry,
    hover_path: HoverPath,
    duration: Duration,
) -> anyhow::Result<()> {
    let started = Instant::now();
    let mut step = 0_u64;
    while started.elapsed() < duration {
        let progress = controlled_sweep_progress(started.elapsed());
        warp_root_pointer(conn, root, sweep_point(geometry, hover_path, progress))?;
        step = step.saturating_add(1);
        let next_sample = started + Duration::from_secs_f64(step as f64 / SWEEP_SAMPLES_PER_SECOND);
        if let Some(remaining) = next_sample.checked_duration_since(Instant::now()) {
            thread::sleep(remaining);
        }
    }
    Ok(())
}

fn controlled_sweep_progress(elapsed: Duration) -> f64 {
    let cycle = (elapsed.as_secs_f64() % SWEEP_CYCLE.as_secs_f64()) / SWEEP_CYCLE.as_secs_f64();
    if cycle <= 0.5 {
        cycle * 2.0
    } else {
        (1.0 - cycle) * 2.0
    }
}

fn sweep_point(geometry: RootWindowGeometry, hover_path: HoverPath, progress: f64) -> (i32, i32) {
    let progress = progress.clamp(0.0, 1.0);
    let position = f64::from(hover_path.start)
        + progress * f64::from(hover_path.end.saturating_sub(hover_path.start));
    if geometry.width >= geometry.height {
        (
            geometry.x + position.round() as i32,
            geometry.y + hover_path.lane,
        )
    } else {
        (
            geometry.x + hover_path.lane,
            geometry.y + position.round() as i32,
        )
    }
}

fn stationary_hover_point(geometry: RootWindowGeometry, hover_path: HoverPath) -> (i32, i32) {
    sweep_point(geometry, hover_path, 0.5)
}

fn input_shape_hover_path(
    conn: &impl Connection,
    dock: Window,
    geometry: RootWindowGeometry,
) -> anyhow::Result<HoverPath> {
    let rectangles = conn
        .shape_get_rectangles(dock, SK::INPUT)?
        .reply()?
        .rectangles;
    let path = if geometry.width >= geometry.height {
        rectangles
            .iter()
            .max_by_key(|rect| rect.width)
            .map(|rect| hover_path_for_rect(rect.x, rect.y, rect.width, rect.height, true))
    } else {
        rectangles
            .iter()
            .max_by_key(|rect| rect.height)
            .map(|rect| hover_path_for_rect(rect.x, rect.y, rect.width, rect.height, false))
    };
    path.context("dock input shape did not contain rectangles")
}

fn hover_path_for_rect(x: i16, y: i16, width: u16, height: u16, horizontal: bool) -> HoverPath {
    let (cross, cross_size, along, along_size) = if horizontal {
        (
            i32::from(y),
            i32::from(height),
            i32::from(x),
            i32::from(width),
        )
    } else {
        (
            i32::from(x),
            i32::from(width),
            i32::from(y),
            i32::from(height),
        )
    };
    let margin = 2.min(along_size.saturating_sub(1) / 2);
    HoverPath {
        lane: cross + cross_size.min(4) / 2,
        start: along + margin,
        end: along + along_size.saturating_sub(1 + margin),
    }
}

fn default_hover_path(geometry: RootWindowGeometry) -> HoverPath {
    let short_axis = if geometry.width >= geometry.height {
        geometry.height
    } else {
        geometry.width
    };
    let long_axis = if geometry.width >= geometry.height {
        geometry.width
    } else {
        geometry.height
    };
    HoverPath {
        lane: (f64::from(short_axis.saturating_sub(1)) * 0.72).round() as i32,
        start: 2.min(i32::from(long_axis.saturating_sub(1)) / 2),
        end: i32::from(long_axis.saturating_sub(3)),
    }
}

fn idle_point(
    geometry: RootWindowGeometry,
    screen_size: (i32, i32),
    original: (i32, i32),
) -> (i32, i32) {
    if !geometry.contains(original) {
        return original;
    }
    let max_x = screen_size.0.saturating_sub(1).max(0);
    let max_y = screen_size.1.saturating_sub(1).max(0);
    [(0, 0), (max_x, 0), (0, max_y), (max_x, max_y)]
        .into_iter()
        .find(|point| !geometry.contains(*point))
        .unwrap_or((0, 0))
}

impl RootWindowGeometry {
    fn contains(self, point: (i32, i32)) -> bool {
        point.0 >= self.x
            && point.0 < self.x + i32::from(self.width)
            && point.1 >= self.y
            && point.1 < self.y + i32::from(self.height)
    }
}

fn warp_root_pointer(
    conn: &impl Connection,
    root: Window,
    point: (i32, i32),
) -> anyhow::Result<()> {
    conn.warp_pointer(
        AtomEnum::NONE,
        root,
        0,
        0,
        0,
        0,
        clamp_i16(point.0),
        clamp_i16(point.1),
    )?;
    conn.flush()?;
    Ok(())
}

fn clamp_i16(value: i32) -> i16 {
    value.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

fn sleep_for(duration: Duration) {
    let started = Instant::now();
    while let Some(remaining) = duration.checked_sub(started.elapsed()) {
        thread::sleep(remaining.min(Duration::from_millis(250)));
    }
}

fn settle_pointer(conn: &impl Connection, root: Window, point: (i32, i32)) -> anyhow::Result<()> {
    warp_root_pointer(conn, root, point)?;
    sleep_for(PASSIVE_SETTLE);
    Ok(())
}

fn with_pointer_restore<T>(restore: impl FnOnce(), sample: impl FnOnce() -> T) -> T {
    struct Restore<F: FnOnce()>(Option<F>);
    impl<F: FnOnce()> Drop for Restore<F> {
        fn drop(&mut self) {
            if let Some(restore) = self.0.take() {
                restore();
            }
        }
    }

    let _restore = Restore(Some(restore));
    sample()
}

fn format_report(
    pid: u32,
    dock: Window,
    geometry: RootWindowGeometry,
    xorg_pid: Option<u32>,
    requested_seconds: u64,
    phases: &[PhaseResult],
) -> String {
    let mut report = String::new();
    let _ = writeln!(report, "pid={pid}");
    let _ = writeln!(report, "dock_window={dock}");
    let _ = writeln!(
        report,
        "phase_duration_seconds={}",
        requested_seconds.max(1)
    );
    let _ = writeln!(
        report,
        "dock_geometry_root={},{},{},{}",
        geometry.x, geometry.y, geometry.width, geometry.height
    );
    let _ = writeln!(
        report,
        "dock_orientation={}",
        if geometry.width >= geometry.height {
            "horizontal"
        } else {
            "vertical"
        }
    );
    let _ = writeln!(
        report,
        "xorg_pid={}",
        xorg_pid.map_or_else(|| "unavailable".to_string(), |value| value.to_string())
    );
    let _ = writeln!(
        report,
        "controlled_sweep_samples_per_second={SWEEP_SAMPLES_PER_SECOND:.1}"
    );
    let _ = writeln!(
        report,
        "passive_settle_seconds={:.1}",
        PASSIVE_SETTLE.as_secs_f64()
    );

    let clock_ticks = clock_ticks_per_second();
    for phase in phases {
        let _ = writeln!(report, "\n[phase {}]", phase.name);
        let _ = writeln!(report, "elapsed_seconds={:.3}", phase.elapsed.as_secs_f64());
        let _ = writeln!(
            report,
            "dock_cpu_percent={:.3}",
            cpu_percent(
                phase.dock_before,
                phase.dock_after,
                phase.elapsed,
                clock_ticks,
            )
        );
        let xorg_cpu = phase
            .xorg_before
            .zip(phase.xorg_after)
            .map(|(before, after)| cpu_percent(before, after, phase.elapsed, clock_ticks));
        let _ = writeln!(
            report,
            "xorg_cpu_percent={}",
            xorg_cpu.map_or_else(|| "unavailable".to_string(), |value| format!("{value:.3}"))
        );
        let _ = writeln!(report, "rss_kib={}", phase.dock_after.rss_kib);
        let _ = writeln!(report, "pss_kib={}", phase.dock_after.pss_kib);
        let _ = writeln!(report, "private_kib={}", phase.dock_after.private_kib);
        if let Some(counters) = phase.counters {
            let _ = writeln!(report, "motion_events={}", counters.motion_events);
            let _ = writeln!(report, "frame_ticks={}", counters.frame_ticks);
            let _ = writeln!(
                report,
                "visible_layout_changes={}",
                counters.visible_layout_changes
            );
            let _ = writeln!(report, "paint_requests={}", counters.paint_requests);
            let _ = writeln!(report, "redraws_completed={}", counters.redraws_completed);
            let _ = writeln!(
                report,
                "average_draw_ms={:.3}",
                counters.draw_micros as f64 / counters.redraws_completed.max(1) as f64 / 1_000.0
            );
            let _ = writeln!(
                report,
                "process_cpu_ms={:.3}",
                phase
                    .dock_after
                    .ticks
                    .saturating_sub(phase.dock_before.ticks) as f64
                    / clock_ticks
                    * 1_000.0
            );
            let _ = writeln!(
                report,
                "window_synchronizations={}",
                counters.window_synchronizations
            );
            let _ = writeln!(report, "shape_updates={}", counters.shape_updates);
            let _ = writeln!(report, "animation_frames={}", counters.animation_frames);
            let _ = writeln!(report, "x11_model_updates={}", counters.x11_model_updates);
            let _ = writeln!(
                report,
                "visual_model_updates={}",
                counters.visual_model_updates
            );
            let _ = writeln!(
                report,
                "presence_model_updates={}",
                counters.presence_model_updates
            );
            let _ = writeln!(
                report,
                "reflection_cache_hit_percent={}",
                hit_rate(counters.reflection_hits, counters.reflection_builds)
            );
            let _ = writeln!(
                report,
                "shelf_cache_hit_percent={}",
                hit_rate(counters.shelf_hits, counters.shelf_builds)
            );
        } else {
            let _ = writeln!(report, "performance_counters=unavailable");
        }
    }

    if let (Some(first), Some(last)) = (phases.first(), phases.last()) {
        let _ = writeln!(report, "\n[summary]");
        let _ = writeln!(
            report,
            "private_growth_percent={:.3}",
            growth_percent(first.dock_before.private_kib, last.dock_after.private_kib)
        );
        let _ = writeln!(
            report,
            "pss_growth_percent={:.3}",
            growth_percent(first.dock_before.pss_kib, last.dock_after.pss_kib)
        );
    }
    report
}

fn growth_percent(before: u64, after: u64) -> f64 {
    if before == 0 {
        return 0.0;
    }
    (after as f64 - before as f64) / before as f64 * 100.0
}

fn hit_rate(hits: u64, builds: u64) -> String {
    let total = hits.saturating_add(builds);
    if total == 0 {
        "unavailable".to_string()
    } else {
        format!("{:.3}", hits as f64 / total as f64 * 100.0)
    }
}

fn find_dock_window(conn: &impl Connection, root: Window, pid: u32) -> anyhow::Result<Window> {
    let pid_atom = intern(conn, b"_NET_WM_PID")?;
    let type_atom = intern(conn, b"_NET_WM_WINDOW_TYPE")?;
    let dock_atom = intern(conn, b"_NET_WM_WINDOW_TYPE_DOCK")?;

    let mut ewmh_available = false;
    for property_name in [b"_NET_CLIENT_LIST_STACKING".as_slice(), b"_NET_CLIENT_LIST"] {
        let property = intern(conn, property_name)?;
        if let Some(clients) =
            optional_property_list(conn, root, property, AtomEnum::WINDOW.into())?
        {
            ewmh_available = true;
            if let Some(window) = find_matching_dock(&clients, pid, dock_atom, |window| {
                (
                    property_u32(conn, window, pid_atom, AtomEnum::CARDINAL.into()),
                    property_list(conn, window, type_atom, AtomEnum::ATOM.into()),
                )
            }) {
                return Ok(window);
            }
        }
    }

    if !ewmh_available
        && let Some(window) = find_dock_in_tree(conn, root, pid, pid_atom, type_atom, dock_atom)?
    {
        return Ok(window);
    }
    anyhow::bail!("could not find an X11 dock window for PID {pid}")
}

fn find_matching_dock(
    candidates: &[Window],
    pid: u32,
    dock_atom: Atom,
    mut metadata: impl FnMut(Window) -> (Option<u32>, Vec<u32>),
) -> Option<Window> {
    candidates.iter().copied().find(|window| {
        let (window_pid, window_types) = metadata(*window);
        window_pid == Some(pid) && window_types.contains(&dock_atom)
    })
}

fn find_dock_in_tree(
    conn: &impl Connection,
    root: Window,
    pid: u32,
    pid_atom: Atom,
    type_atom: Atom,
    dock_atom: Atom,
) -> anyhow::Result<Option<Window>> {
    let mut pending = conn.query_tree(root)?.reply()?.children;
    while let Some(window) = pending.pop() {
        if find_matching_dock(&[window], pid, dock_atom, |window| {
            (
                property_u32(conn, window, pid_atom, AtomEnum::CARDINAL.into()),
                property_list(conn, window, type_atom, AtomEnum::ATOM.into()),
            )
        })
        .is_some()
        {
            return Ok(Some(window));
        }
        if let Ok(cookie) = conn.query_tree(window)
            && let Ok(tree) = cookie.reply()
        {
            pending.extend(tree.children);
        }
    }
    Ok(None)
}

fn window_geometry_on_root(
    conn: &impl Connection,
    root: Window,
    window: Window,
) -> anyhow::Result<RootWindowGeometry> {
    let geometry = conn.get_geometry(window)?.reply()?;
    let translated = conn.translate_coordinates(window, root, 0, 0)?.reply()?;
    Ok(RootWindowGeometry {
        x: i32::from(translated.dst_x),
        y: i32::from(translated.dst_y),
        width: geometry.width,
        height: geometry.height,
    })
}

fn intern(conn: &impl Connection, name: &[u8]) -> anyhow::Result<Atom> {
    Ok(conn.intern_atom(false, name)?.reply()?.atom)
}

fn property_u32(conn: &impl Connection, window: Window, property: Atom, ty: Atom) -> Option<u32> {
    property_list(conn, window, property, ty).into_iter().next()
}

fn property_list(conn: &impl Connection, window: Window, property: Atom, ty: Atom) -> Vec<u32> {
    conn.get_property(false, window, property, ty, 0, u32::MAX)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .and_then(|reply| reply.value32().map(Iterator::collect))
        .unwrap_or_default()
}

fn optional_property_list(
    conn: &impl Connection,
    window: Window,
    property: Atom,
    ty: Atom,
) -> anyhow::Result<Option<Vec<u32>>> {
    let reply = conn
        .get_property(false, window, property, ty, 0, u32::MAX)?
        .reply()?;
    if reply.type_ == AtomEnum::NONE.into() || reply.format == 0 {
        return Ok(None);
    }
    Ok(Some(
        reply.value32().map(Iterator::collect).unwrap_or_default(),
    ))
}

fn sample_process(pid: u32) -> anyhow::Result<ProcessSample> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let end = stat.rfind(')').context("malformed /proc stat")?;
    let fields = stat[end + 2..].split_whitespace().collect::<Vec<_>>();
    let user_ticks = fields.get(11).context("missing utime")?.parse::<u64>()?;
    let system_ticks = fields.get(12).context("missing stime")?.parse::<u64>()?;
    let rollup = fs::read_to_string(format!("/proc/{pid}/smaps_rollup"))?;
    Ok(ProcessSample {
        ticks: user_ticks + system_ticks,
        rss_kib: rollup_value(&rollup, "Rss:"),
        pss_kib: rollup_value(&rollup, "Pss:"),
        private_kib: rollup_value(&rollup, "Private_Clean:")
            + rollup_value(&rollup, "Private_Dirty:"),
    })
}

fn rollup_value(rollup: &str, key: &str) -> u64 {
    rollup
        .lines()
        .find(|line| line.starts_with(key))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn find_process(names: &[&str]) -> Option<u32> {
    fs::read_dir("/proc")
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_string_lossy().parse::<u32>().ok())
        .find(|pid| {
            fs::read_to_string(format!("/proc/{pid}/comm"))
                .ok()
                .is_some_and(|name| names.iter().any(|candidate| name.trim() == *candidate))
        })
}

fn clock_ticks_per_second() -> f64 {
    // Linux uses 100 ticks/sec on the supported deployment targets. Keeping
    // this helper dependency-free also makes it usable in minimal installs.
    100.0
}

fn cpu_percent(before: ProcessSample, after: ProcessSample, elapsed: Duration, ticks: f64) -> f64 {
    after.ticks.saturating_sub(before.ticks) as f64 / ticks / elapsed.as_secs_f64().max(0.001)
        * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ewmh_discovery_finds_client_hidden_under_reparenting_frame() {
        let pid = 4242;
        let dock_atom = 91;
        let frame = 100;
        let client = 101;
        let metadata = |window| match window {
            101 => (Some(pid), vec![dock_atom]),
            _ => (None, Vec::new()),
        };

        assert_eq!(find_matching_dock(&[frame], pid, dock_atom, metadata), None);
        assert_eq!(
            find_matching_dock(&[client], pid, dock_atom, metadata),
            Some(client)
        );
    }

    #[test]
    fn sweep_uses_long_axis_for_horizontal_and_vertical_docks() {
        let horizontal = RootWindowGeometry {
            x: 100,
            y: 800,
            width: 900,
            height: 160,
        };
        let vertical = RootWindowGeometry {
            x: 0,
            y: 100,
            width: 160,
            height: 900,
        };

        let horizontal_path = default_hover_path(horizontal);
        let vertical_path = default_hover_path(vertical);
        assert_eq!(sweep_point(horizontal, horizontal_path, 0.0), (102, 914));
        assert_eq!(sweep_point(horizontal, horizontal_path, 1.0), (997, 914));
        assert_eq!(sweep_point(vertical, vertical_path, 0.0), (114, 102));
        assert_eq!(sweep_point(vertical, vertical_path, 1.0), (114, 997));
        assert_eq!(horizontal_path.lane, 114);
        assert_eq!(vertical_path.lane, 114);
        assert_eq!(controlled_sweep_progress(Duration::ZERO), 0.0);
        assert_eq!(controlled_sweep_progress(Duration::from_secs(1)), 0.5);
        assert_eq!(controlled_sweep_progress(Duration::from_secs(2)), 1.0);
        assert_eq!(controlled_sweep_progress(Duration::from_secs(3)), 0.5);
    }

    #[test]
    fn shaped_hover_path_stays_inside_its_longest_rectangle() {
        assert_eq!(
            hover_path_for_rect(40, 90, 800, 30, true),
            HoverPath {
                lane: 92,
                start: 42,
                end: 837,
            }
        );
        assert_eq!(
            hover_path_for_rect(90, 40, 30, 800, false),
            HoverPath {
                lane: 92,
                start: 42,
                end: 837,
            }
        );
    }

    #[test]
    fn sampler_restores_pointer_even_when_workload_fails() {
        let restored = std::cell::Cell::new(false);
        let result: anyhow::Result<()> = with_pointer_restore(
            || restored.set(true),
            || anyhow::bail!("simulated sampling failure"),
        );

        assert!(result.is_err());
        assert!(restored.get());
    }
}
