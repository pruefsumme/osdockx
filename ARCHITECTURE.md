# OSDockX Architecture

This document is for developers who want to understand the app quickly without reading every module first.

## One-Screen Summary

OSDockX is a GTK4 application that builds a dock window, polls X11 for open windows, merges those windows with pinned launchers, computes a dock layout, and then renders the result with Cairo or an optional GL shelf layer.

The shortest mental model is:

`config + theme + desktop entries + X11 windows -> model -> layout -> render -> shaped dock window`

## Startup Flow

1. `src/main.rs`
   - Initializes tracing.
   - Calls `osdockx::ui::run()`.

2. `src/ui.rs::run()`
   - Creates the GTK application.
   - Calls `build_ui()` on activation.

3. `src/ui.rs::build_ui()`
   - Loads config from XDG config.
   - Exports built-in theme packs when missing.
   - Resolves the active theme pack and renderer mode.
   - Loads desktop-entry metadata.
   - Creates the X11 backend when available.
   - Builds the runtime state, window, overlay, drawing area, and optional `GLArea`.
   - Wires realize, motion, click, refresh, and GL callbacks.

## Runtime State

The central state lives in `src/ui.rs` inside `Runtime`.

Important fields:

- `config`: user config and dock behavior
- `theme`: resolved runtime theme with parsed colors and asset paths
- `desktop_index`: launcher metadata from desktop files
- `backend`: current platform backend, currently X11 only
- `model`: current dock items
- `renderer`: Cairo renderer facade
- `scene3d`: optional GL shelf renderer
- `icons`: icon cache for Cairo drawing
- `hover`: current pointer position in dock coordinates
- `hidden`: autohide state

Most UI behavior is a function of this state plus a refresh timer.

## Data Flow

### 1. Config and Theme

`src/config.rs` defines the user-facing config format.

- `Config::load_or_create()` creates a default config on first launch.
- `Config::normalized()` clamps user values and migrates old defaults.
- `ThemeConfig` holds all theme-controlled visual values.

`src/theme_pack.rs` turns a theme id into a runtime theme pack.

- Built-in theme packs are checked into `themes/`.
- User theme packs can override them through XDG config/data paths.
- User config values override the theme pack values.

`src/theme.rs` converts `ThemeConfig` into a runtime `Theme` with parsed colors and resolved asset paths.

### 2. Desktop Apps and Live Windows

`src/desktop.rs` builds the launcher index.

`src/backend/x11.rs` provides live window data.

- Monitor geometry
- Dock struts and window type configuration
- Active window tracking
- Current window list
- `_NET_WM_ICON`, title, class, PID, workspace, urgency, minimized state
- Focus, minimize, and close window actions

### 3. Model

`src/model.rs` merges pinned launchers and live windows into `DockModel`.

- Pinned launchers are inserted first.
- Windows are matched back onto existing items when possible.
- Unmatched windows become standalone dock items.
- Running, active, urgent, and badge state are derived from the attached windows.

If a change is about grouping, launcher matching, or window actions, this is usually the first place to inspect.

### 4. Layout

`src/layout.rs` is where the dock becomes geometry.

It computes:

- icon rectangles
- magnification scale per icon
- hover label rectangle
- shelf rectangle
- final dock size

This file owns spacing, shelf overhang, icon growth, and hit testing. If hover feels wrong or the shelf is in the wrong place, start here.

### 5. Rendering

Rendering is split into shelf rendering and overlay rendering.

#### Cairo path

`src/renderer.rs` is the main Cairo facade.

It is responsible for:

- choosing the active shelf layer behavior
- computing the current layout from model/config/theme/hover
- drawing the shelf when Cairo owns it
- drawing icons, reflections, indicators, badges, and labels
- exposing `visual_regions()` for dock shaping

The renderer was refactored so `src/renderer.rs` stays the orchestrator and detail work lives in leaf modules under `src/renderer/`.

Important leaf modules:

- `icons.rs`: icon loading, icon cache, icon drawing
- `reflections.rs`: reflection drawing and clipping
- `indicators.rs`: running and active indicators
- `badges.rs`: badge rendering
- `primitives.rs`: shared Cairo helpers
- `tests.rs`: renderer tests

Shelf rendering helpers live under `src/renderer/shelf/`:

- `geometry.rs`: geometry structs and formulas
- `paths.rs`: shelf path builders
- `material.rs`: shared shelf material helpers
- `crystal.rs`: crystal shelf rendering
- `legacy.rs`: legacy shelf rendering
- `leopard.rs`: Leopard shelf rendering

#### GL path

`src/scene3d.rs` implements the optional GL shelf renderer.

- It draws the shelf only.
- It plugs into the `ShelfRenderer` abstraction from `src/shelf.rs`.
- If GL initialization or rendering fails, the app falls back to Cairo shelf drawing.

### 6. Shelf Mode Selection

`src/shelf.rs` defines the renderer-mode abstraction:

- `Procedural2dRenderer`
- `Texture2dRenderer`
- `Scene3dRenderer` via `src/scene3d.rs`

`src/ui.rs::shelf_layer_for()` chooses whether Cairo should draw the shelf or whether GL has already handled it.

In practice:

- `procedural-2d`: Cairo draws shelf + overlay
- `texture-2d`: Cairo uses texture-oriented shelf path, then draws overlay
- `scene-3d`: GL tries to draw the shelf, Cairo still draws overlay

## User Interaction Wiring

All of this lives in `src/ui.rs`.

### Hover

- `wire_motion()` updates `state.hover` on pointer motion.
- That hover feeds back into layout, which drives magnification and label placement.
- On leave, hover is cleared and autohide may start.

### Clicks

- `wire_clicks()` uses the renderer's current layout for hit testing.
- Left click:
  - focuses the window if inactive
  - minimizes it if already active
  - launches the app if it is not running
- Middle click closes the primary window for the item.

### Refresh and Reload

- `wire_refresh()` runs on `dock.refresh_ms`.
- Each tick reloads config/theme if changed, refreshes the window model, resizes/repositions the dock if needed, and redraws.

## Window Management and Shaping

The dock is a GTK window, but X11 still matters heavily.

`src/ui.rs` uses the backend to:

- register the dock window as a dock window
- move it to the correct monitor/edge position
- apply EWMH struts when reserving screen space
- apply a shaped input/visual region based on `Renderer::visual_regions()`

That shaping step is why the dock can feel visually tighter than a plain rectangular GTK window.

## Where To Edit Common Problems

Use this table as a shortcut:

- App matching is wrong: `src/desktop.rs`, `src/model.rs`, `src/backend/x11.rs`
- Hover or magnification is wrong: `src/layout.rs`, then `src/ui.rs`
- Shelf geometry is wrong: `src/renderer/shelf/geometry.rs` and `src/renderer/shelf/paths.rs`
- Shelf material, colors, or gradients are wrong: `src/renderer/shelf/material.rs` and the style-specific shelf module
- Icons/reflections/badges/labels are wrong: `src/renderer/`
- Theme pack or asset loading is wrong: `src/theme_pack.rs` and `src/theme.rs`
- Dock position, reserve space, or shaping is wrong: `src/ui.rs` and `src/backend/x11.rs`

## Developer Notes

- `resources/` is reference material only.
- The app is intentionally X11-first today.
- The renderer facade split is deliberate; avoid moving leaf helpers back into `src/renderer.rs`.
- For renderer work, run targeted renderer tests before the full library suite.

Recommended validation sequence:

1. `cargo test renderer_paints_non_empty_surface --lib`
2. `cargo test renderer::tests::leopard --lib`
3. `cargo test --lib`