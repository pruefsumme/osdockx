use anyhow::Context;
use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt, Window};

#[derive(Debug, Clone, Copy)]
struct ProcessSample {
    ticks: u64,
    rss_kib: u64,
    pss_kib: u64,
    private_kib: u64,
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

    let xorg_pid = find_process(&["Xorg", "X"]);
    let before_dock = sample_process(pid)?;
    let before_xorg = xorg_pid.map(sample_process).transpose()?;
    let (conn, screen_num) = x11rb::connect(None).context("connect to X11")?;
    let root = conn.setup().roots[screen_num].root;
    let dock = find_dock_window(&conn, root, pid)?;
    let geometry = conn.get_geometry(dock)?.reply()?;

    let steps = seconds.saturating_mul(60).max(1);
    for step in 0..steps {
        let phase = (step % 240) as f64 / 239.0;
        let sweep = if phase <= 0.5 {
            phase * 2.0
        } else {
            (1.0 - phase) * 2.0
        };
        let x = (sweep * f64::from(geometry.width.saturating_sub(1))) as i16;
        let y = i16::try_from(geometry.height / 2).unwrap_or(i16::MAX);
        conn.warp_pointer(AtomEnum::NONE, dock, 0, 0, 0, 0, x, y)?;
        conn.flush()?;
        thread::sleep(Duration::from_millis(16));
    }

    let after_dock = sample_process(pid)?;
    let after_xorg = xorg_pid.map(sample_process).transpose()?;
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let output = PathBuf::from(format!("/tmp/osdockx-perf-{pid}-{timestamp}.txt"));
    let clock_ticks = clock_ticks_per_second();
    let dock_cpu = cpu_percent(before_dock, after_dock, seconds, clock_ticks);
    let xorg_cpu = before_xorg
        .zip(after_xorg)
        .map(|(before, after)| cpu_percent(before, after, seconds, clock_ticks));
    let report = format!(
        "pid={pid}\ndock_window={dock}\nduration_seconds={seconds}\n\
         dock_cpu_percent={dock_cpu:.3}\nxorg_pid={}\nxorg_cpu_percent={}\n\
         rss_kib={}\npss_kib={}\nprivate_kib={}\n",
        xorg_pid.map_or_else(|| "unavailable".to_string(), |value| value.to_string()),
        xorg_cpu.map_or_else(|| "unavailable".to_string(), |value| format!("{value:.3}")),
        after_dock.rss_kib,
        after_dock.pss_kib,
        after_dock.private_kib,
    );
    fs::write(&output, report)?;
    println!("{}", output.display());
    Ok(())
}

fn find_dock_window(conn: &impl Connection, root: Window, pid: u32) -> anyhow::Result<Window> {
    let pid_atom = intern(conn, b"_NET_WM_PID")?;
    let type_atom = intern(conn, b"_NET_WM_WINDOW_TYPE")?;
    let dock_atom = intern(conn, b"_NET_WM_WINDOW_TYPE_DOCK")?;
    for window in conn.query_tree(root)?.reply()?.children {
        let window_pid = property_u32(conn, window, pid_atom, AtomEnum::CARDINAL.into());
        let window_types = property_list(conn, window, type_atom, AtomEnum::ATOM.into());
        if window_pid == Some(pid) && window_types.contains(&dock_atom) {
            return Ok(window);
        }
    }
    anyhow::bail!("could not find an X11 dock window for PID {pid}")
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

fn cpu_percent(before: ProcessSample, after: ProcessSample, seconds: u64, ticks: f64) -> f64 {
    after.ticks.saturating_sub(before.ticks) as f64 / ticks / seconds.max(1) as f64 * 100.0
}
