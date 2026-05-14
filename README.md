# OSDockX

OSDockX is a clean-room Rust dock for Linux desktops, currently focused on
XFCE4 on X11. The first milestone prioritizes the pre-Mavericks OSX dock look:
a translucent glass shelf, icon magnification, reflections, running indicators,
and notification badges.

The code under `resources/` is reference material only. The implementation in
`src/` is new code.

## Build

```sh
cargo run
```

Runtime configuration is read from:

```text
$XDG_CONFIG_HOME/osdockx/config.toml
```

If no file exists, OSDockX writes a default config on first launch.

## Current Scope

- GTK4 custom cairo dock window
- X11 EWMH dock setup through `x11rb`, including dock struts, active window,
  workspace, urgency, PID, executable, and `_NET_WM_ICON` metadata
- RandR monitor selection via `dock.monitor` (`primary`, a monitor name, or an index)
- Desktop launcher discovery through GIO desktop entries
- App matching through `StartupWMClass`, desktop IDs, names, and executable fallback
- OSX-style glass shelf rendering with icon reflections, magnification, running
  indicators, badges, and an opaque shelf fallback for non-composited sessions
- Configurable theme/model/layout boundaries for a later Wayland backend

Wayland support is intentionally not part of v1. The backend boundary is shaped
so a later implementation can use layer-shell and a compositor taskbar protocol.
