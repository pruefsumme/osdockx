# AGENTS.md

This file gives AI contributors a fast, repo-specific mental model for OSDockX.

## What This Repo Is

OSDockX is a clean-room Rust dock for Linux desktops, currently centered on GTK4 + X11.

- `src/` is the real implementation.
- `resources/` is reference material only and should not be treated as source code to edit or port directly.
- The current default experience is a Cairo-rendered Leopard-style dock with optional GL shelf rendering for `scene-3d` themes.

## Fast Start

- Run the app: `cargo run`
- Main validation: `cargo test --lib`
- Focused renderer smoke test: `cargo test renderer_paints_non_empty_surface --lib`
- Focused Leopard renderer tests: `cargo test renderer::tests::leopard --lib`

## How The App Is Wired

1. `src/main.rs` initializes tracing and calls `ui::run()`.
2. `src/ui.rs` builds the GTK window, overlay, drawing area, and optional `GLArea`.
3. `Config::load_or_create()` loads XDG config, then `ThemePack::load()` resolves the active theme pack and applies user overrides.
4. `DesktopIndex::load()` provides launcher metadata and `X11Backend::poll_windows()` provides live window state.
5. `DockModel::from_sources()` merges pinned launchers and current windows into the dock item list.
6. `Renderer::layout_for()` turns the model + hover state + theme/config values into a `DockLayout`.
7. Rendering then splits into:
   - `Scene3dRenderer` for the shelf only when `renderer = "scene-3d"` works
   - Cairo overlay rendering for icons, reflections, indicators, badges, labels, and all fallback shelf drawing

## Module Ownership

- `src/config.rs`: XDG config loading, defaults, normalization, old-value migration.
- `src/theme.rs`: runtime `Theme` and color math.
- `src/theme_pack.rs`: theme pack discovery, export of built-in packs, asset resolution, user overrides.
- `src/desktop.rs`: launcher discovery and desktop-entry matching.
- `src/model.rs`: dock item model built from pinned apps + live windows.
- `src/layout.rs`: icon positions, magnification, shelf rect, label rects.
- `src/ui.rs`: GTK app/runtime shell, refresh loop, hover, clicks, autohide, shaping, dock window sync.
- `src/backend/x11.rs`: X11/EWMH integration, monitor geometry, struts, window polling, focus/minimize/close.
- `src/renderer.rs`: Cairo renderer facade and orchestration.
- `src/renderer/`: extracted renderer helpers and shelf submodules.
- `src/scene3d.rs`: optional GL shelf renderer.
- `src/shelf.rs`: abstraction layer for shelf renderer modes.

## Renderer Rules

The renderer was intentionally refactored into a facade plus leaf modules. Keep that split intact.

- `src/renderer.rs` should stay the orchestrator/facade.
- Shared leaf modules live under `src/renderer/`.
- Shelf-specific leaves live under `src/renderer/shelf/`.
- Existing parent-scope aliases in `src/renderer.rs` are sometimes still used by sibling modules or tests. Preserve them unless you also update all dependents.

Current shelf split:

- `geometry.rs`: pure shelf geometry structs and formulas
- `paths.rs`: Cairo path builders for shelf shapes
- `material.rs`: shared shelf material helpers
- `leopard.rs`: Leopard-specific shelf renderer passes

## Practical Editing Guidance

- If the bug is about item matching, launch/focus/minimize, or window metadata, start in `src/model.rs`, `src/desktop.rs`, and `src/backend/x11.rs`.
- If the bug is about spacing, hover growth, shelf size, or hit testing, start in `src/layout.rs`.
- If the bug is about colors, gradients, reflections, indicators, badges, or shelf visuals, start in `src/renderer.rs` and `src/renderer/`.
- If the bug is about renderer mode selection or fallback behavior, start in `src/ui.rs`, `src/shelf.rs`, and `src/scene3d.rs`.
- If the bug is about theme values or assets not being picked up, start in `src/config.rs`, `src/theme.rs`, and `src/theme_pack.rs`.

## Runtime Behavior Worth Remembering

- Config and theme files are reloaded on the UI refresh timer (`dock.refresh_ms`).
- Hover is immediate and affects layout and redraws directly.
- Autohide is handled in `ui.rs` after pointer leave.
- Left click focuses/minimizes or launches the item.
- Middle click closes the primary window for the item.
- The dock window shape is derived from `Renderer::visual_regions()` and applied through the X11 SHAPE extension.
- If the GL shelf cannot render, the app falls back to Cairo shelf rendering instead of failing hard.

## Safe Validation Pattern

For most changes, use this order:

1. `cargo test renderer_paints_non_empty_surface --lib` for renderer-adjacent edits
2. `cargo test renderer::tests::leopard --lib` for Leopard shelf work
3. `cargo test --lib`

## Non-Goals Right Now

- Wayland is not part of v1.
- `resources/` is not the implementation.
- Do not collapse the renderer submodules back into one file.
