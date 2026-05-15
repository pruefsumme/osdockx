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

- GTK4 overlay dock window with cairo shelf/icon/label rendering by default and
  an opt-in GLArea shelf layer for `scene-3d` themes
- X11 EWMH dock setup through `x11rb`, including dock struts, active window,
  workspace, urgency, PID, executable, and `_NET_WM_ICON` metadata
- RandR monitor selection via `dock.monitor` (`primary`, a monitor name, or an index)
- Desktop launcher discovery through GIO desktop entries
- App matching through `StartupWMClass`, desktop IDs, names, and executable fallback
- Default `osx-crystal-2.5d` theme with an opaque cairo Crystal-style plank,
  mirrored icon-band reflections, magnification, running indicators, and badges
- Opt-in `osx-glass-3d` / `scene-3d` renderer remains available for GL shelf
  experiments
- Theme packs under `$XDG_CONFIG_HOME/osdockx/themes/<theme-id>/theme.toml`
  or `$XDG_DATA_HOME/osdockx/themes/<theme-id>/theme.toml`, with
  `renderer = "scene-3d"`, `"texture-2d"`, or `"procedural-2d"`
- Configurable theme/model/layout boundaries for a later Wayland backend

Wayland support is intentionally not part of v1. The backend boundary is shaped
so a later implementation can use layer-shell and a compositor taskbar protocol.
