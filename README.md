# OSDockX

OSDockX is a clean-room Rust dock for Linux desktops, currently focused on
XFCE4 on X11. The first milestone prioritizes the pre-Mavericks OSX dock look:
an opaque shelf, icon magnification, reflections, running indicators,
and notification badges.

## Build

```sh
cargo run
```

## Install

For a user-local install from this checkout:

```sh
./install.sh
```

This installs `osdockx` to `~/.local/bin` and adds a desktop launcher under
`~/.local/share/applications`. Remove those files with:

```sh
./install.sh --uninstall
```

Arch users can build the live Git package from:

```text
packaging/arch/osdockx-git/PKGBUILD
```

Runtime configuration is read from:

```text
$XDG_CONFIG_HOME/osdockx/config.toml
```

If no file exists, OSDockX writes a default config on first launch.
On first launch, OSDockX asks whether it should start at login. This can later
be toggled from the OSDockX right-click settings menu.

## Current Scope

- GTK4 overlay dock window with cairo shelf/icon/label rendering by default and
  an opt-in GLArea shelf layer for `scene-3d` themes
- X11 EWMH dock setup through `x11rb`, including dock struts, active window,
  workspace, urgency, PID, executable, and `_NET_WM_ICON` metadata
- RandR monitor selection via `dock.monitor` (`primary`, a monitor name, or an index)
- Desktop launcher discovery through GIO desktop entries
- App icons are resolved through GTK's current user icon theme, with inherited
  theme lookup, absolute `Icon=` paths, `_NET_WM_ICON`, and placeholder fallbacks
- Per-app custom icons can be selected from the right-click icon menu and are
  stored under `custom_icons` in the user config
- Dock icons can be reordered by left-click dragging, with the resulting order
  stored under `item_order` in the user config
- App matching through `StartupWMClass`, desktop IDs, names, and executable fallback
- Default `leopard` theme with an editable cairo Leopard-style plank,
  mirrored icon-band reflections, magnification, running indicators, and badges
- Opt-in `scene-3d` renderer remains available for GL shelf experiments
- Theme packs checked into `themes/<theme-id>/theme.toml`, plus user theme packs
  under `$XDG_CONFIG_HOME/osdockx/themes/<theme-id>/theme.toml` or
  `$XDG_DATA_HOME/osdockx/themes/<theme-id>/theme.toml`, with
  `renderer = "scene-3d"`, `"texture-2d"`, or `"procedural-2d"`
- Checked-in theme packs are exported to `$XDG_CONFIG_HOME/osdockx/themes/`
  when missing, and config/theme TOML edits are reloaded while the dock runs
- Configurable theme/model/layout boundaries for a later Wayland backend

Wayland support is intentionally not part of v1. The backend boundary is shaped
so a later implementation can use layer-shell and a compositor taskbar protocol.
