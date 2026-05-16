# OSDockX

OSDockX is a clean-room Rust dock for Linux desktops, currently focused on
XFCE4 on X11. The first milestone prioritizes the pre-Mavericks OSX dock look:
an opaque shelf, icon magnification, reflections, running indicators,
and notification badges.

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
- App icons are resolved through GTK's current user icon theme, with inherited
  theme lookup, absolute `Icon=` paths, `_NET_WM_ICON`, and placeholder fallbacks
- App matching through `StartupWMClass`, desktop IDs, names, and executable fallback
- Default `leopard` theme with an editable cairo Leopard-style plank,
  mirrored icon-band reflections, magnification, running indicators, and badges
- Opt-in `osx-glass-3d` / `scene-3d` renderer remains available for GL shelf
  experiments
- Theme packs checked into `themes/<theme-id>/theme.toml`, plus user theme packs
  under `$XDG_CONFIG_HOME/osdockx/themes/<theme-id>/theme.toml` or
  `$XDG_DATA_HOME/osdockx/themes/<theme-id>/theme.toml`, with
  `renderer = "scene-3d"`, `"texture-2d"`, or `"procedural-2d"`
- Checked-in theme packs are exported to `$XDG_CONFIG_HOME/osdockx/themes/`
  when missing, and config/theme TOML edits are reloaded while the dock runs
- Configurable theme/model/layout boundaries for a later Wayland backend

Wayland support is intentionally not part of v1. The backend boundary is shaped
so a later implementation can use layer-shell and a compositor taskbar protocol.
