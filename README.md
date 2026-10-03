# mcsapi

A small Rust scaffold for xmonad-like desktop policy on
[Smithay](https://smithay.github.io/). It is a library, **not yet a runnable
Wayland compositor**.

## Workspace layout

This repository is a Cargo workspace. Every directory under `crates/` is a
member:

| Crate | Path | Purpose |
| --- | --- | --- |
| `mcsapi` | `crates/mcsapi` | Desktop policy, layouts, toolkit selection, and shell widgets (the original library; public API unchanged). |
| `mcsapi-ui` | `crates/mcsapi-ui` | UI toolkit for apps: an `App` trait drawn with egui and the shared shell `Theme`. Starter. |
| `mcsapi-runtime` | `crates/mcsapi-runtime` | Separate runtime for running apps: app registration and instance lifecycle. Starter. |
| `derisk-apps` | `crates/derisk-apps` | Catalog of the derisk core apps: registers them with `mcsapi-runtime` and keeps running app objects next to their instances. |
| `derisk-settings` | `crates/derisk-settings` | Settings app and the settings model the shell reads. |
| `derisk-files` | `crates/derisk-files` | Files app (file manager). |
| `derisk-editor` | `crates/derisk-editor` | Text Editor app. |
| `derisk-monitor` | `crates/derisk-monitor` | System Monitor app. |
| `derisk-calculator` | `crates/derisk-calculator` | Calculator app. |
| `x2mcsapi` | `crates/x2mcsapi` | Adapter that restyles foreign apps (web pages and Electron via an injected script, GTK 3/4, Qt Widgets) from the shell `Theme`, so they look coherent with derisk. |

Shared package metadata and dependency versions live in the root
`Cargo.toml` under `[workspace.package]` and `[workspace.dependencies]`; a new
crate inherits them with `version.workspace = true` and `dep.workspace = true`.

`mcsapi-runtime` does not depend on the UI toolkit, so apps may use any
toolkit and a compositor can use `mcsapi` without either starter crate.

## What is here (`mcsapi`)

- Distinct nonzero `WindowId` and `WorkspaceId` types.
- Fixed workspaces, unique window membership, focus cycling, promotion to the
  main pane, and moving windows between workspaces.
- Tall and monocle layouts using Smithay's logical-coordinate rectangles.
- Iterator-first APIs: ordered trees own state; window/workspace enumeration
  and lazy placement generation need no temporary collections. No `Vec`
  storage is used by mcsapi; upstream toolkits may use their own collections.
- Optional GPUI integration and an always-available egui fallback context.
- Matching native workspace bars with shared theme colors, rounded surfaces,
  active labels, and keyboard activation.

## Quick start

Requires Rust 1.95 or later. On Linux, Smithay needs the xkbcommon development
library (`libxkbcommon-dev` on Debian/Ubuntu).

```rust
use mcsapi::{Desktop, Geometry, WindowId, WorkspaceId};

fn main() -> Result<(), mcsapi::Error> {
    let mut desktop =
        Desktop::new((1..=9).map(|id| WorkspaceId::new(id).unwrap()))?;
    desktop.insert(WindowId::new(1).unwrap())?;
    desktop.insert(WindowId::new(2).unwrap())?;
    desktop.active_mut().focus_previous();
    desktop.active_mut().promote_focused();

    let bounds = Geometry::new((0, 0).into(), (1920, 1080).into());
    for placement in desktop.active().arrange(bounds)? {
        // Apply this logical rectangle to the host's corresponding surface.
        println!("{}: {:?}", placement.window, placement.geometry);
    }
    Ok(())
}
```

Workspaces iterate by ID; the first supplied ID is initially active. Within a
workspace the main window comes first, then the other windows by ID. Insertion
focuses the new window. Focus wraps in this same order. Removing a focused window
focuses its next neighbor, wrapping when needed. IDs are assigned by the host:
do not reuse an ID while stale commands for it can still arrive.

`arrange` validates bounds before returning an exact-size iterator. Tall puts
one window in the left half and stacks the others on the right, distributing
remainder pixels from the top. Tiny outputs return `InsufficientSpace`, rather
than emitting zero-sized tiles. Monocle configures all windows to the same
bounds; the host displays only the focused one.

## derisk core apps

The `derisk-*` crates are the core apps of the derisk desktop. Each is an
`mcsapi_ui::App`, keeps its logic in a UI-free model with its own tests, and
has a reverse-DNS ID under `org.derisk.*`.

| App | ID | What it does |
| --- | --- | --- |
| Files | `org.derisk.files` | Places sidebar, back/forward/up history, sort by name, size, or date, filter, hidden files, new folder/file, rename (never overwrites), copy/cut/paste with `name (copy).ext` on clashes, and the freedesktop.org trash. Files open with `xdg-open`. |
| Settings | `org.derisk.settings` | Appearance (dark/light, accent, text size, reduced motion), desktop (layout, gaps, workspaces, adaptive profile), keyboard and pointer, notifications, and power. Saves `key = value` lines to `$XDG_CONFIG_HOME/derisk/settings.conf`; `Settings::theme()` gives the shell its colors. |
| Text Editor | `org.derisk.editor` | Opens UTF-8 files up to 8 MiB, atomic saves that keep file permissions, find and replace, line/column status, unsaved-change prompts. |
| System Monitor | `org.derisk.monitor` | CPU history graph, memory, swap, load, uptime, and a sortable, filterable process table with End process (SIGTERM), read from `/proc`. |
| Calculator | `org.derisk.calculator` | Keypad and expression line with precedence, `^`, `%`, functions (`sqrt`, `sin`, `ln`, ...), `pi`, `e`, `ans`, and history. |

A compositor host launches apps through `derisk_apps::Session`, which keeps
the runtime and the app objects in step:

```rust,ignore
let mut session = derisk_apps::Session::new()?;
let files = session.launch(&AppId::new(derisk_apps::FILES).unwrap())?;
// Each frame, for each instance's surface:
let app = session.app_mut(files).unwrap();
let output = mcsapi_ui::run_frame(app, &context, input, &theme);
```

Until the host can give apps their own surfaces, try them in one window:

```console
$ cargo run -p derisk-apps --features preview --bin derisk-preview -- org.derisk.files
```

The preview follows the Settings app's colors and text size live. It is
behind a feature so CI and library users do not build windowing crates.

## Smithay integration boundary

The `mcsapi::smithay` re-export and `Geometry = Rectangle<i32, Logical>` use the
actual Smithay types, with no coordinate conversion or extra protocol wrapper.
Only `wayland_frontend` is enabled by default. A host can enable additional
Smithay features in its own dependency declaration.

The compositor host must implement:

1. Display/socket setup, protocol handlers, output discovery, seats, and input.
2. A mapping from `WindowId` to live surfaces/toplevels; call `insert` when a
   window is managed and `remove` when it is destroyed.
3. Keybindings calling the focus, promotion, workspace, and move methods.
4. XDG configure/commit handling, placement, focus synchronization, stacking,
   rendering, damage tracking, and repaint scheduling.

Policy state does not claim a surface is already configured, nor manage its
lifetime. A useful host starting point is Smithay's
[Smallvil example](https://github.com/Smithay/smithay/tree/v0.7.0/smallvil).
Multi-output policy, floating windows, IPC, and session management are future
extensions, not implemented features.

## Toolkit selection and widgets

Enable the native Wayland GPUI integration with `--features gpui`. On
Debian/Ubuntu its build prerequisites include `libwayland-dev`,
`libxkbcommon-dev`, `libxkbcommon-x11-dev`, `libfontconfig1-dev`,
`libssl-dev`, and `libvulkan-dev`, plus a C/C++ compiler and `pkg-config`.
Runtime use also requires a display and compatible graphics drivers.
The GPUI feature constrains `libc` for compatibility with GPUI's old transitive
`xattr` dependency; remove that constraint when upstream updates the dependency.

`Toolkit::initialize(capabilities, initialize_gpui)` prefers GPUI only if the
feature is compiled, the host has verified hardware acceleration and required
drivers, and the callback succeeds. Otherwise it supplies an egui context and a
`FallbackReason`, retaining initialization error text. Default/unprobed
capabilities conservatively select egui. Device files and environment variables
alone are not capability probes.

The callback returns the host's initialized GPUI handle, not a placeholder
success value. Run it inside the host's application context and include fallible
window/renderer setup. Panics are not caught; GPUI's `Application::new()` is not
itself a fallible capability probe. The library does not open a second event
loop or automatically migrate an already-running UI after a driver failure.

- `widgets::gpui_workspace_bar` produces GPUI elements and sends requested IDs
  to a host callback.
- `widgets::egui_workspace_bar` returns an optional workspace ID; apply it with
  `Desktop::switch_to`.
- `widgets::Theme` supplies the shared colors. Selection has a text label, not
  only a color change. egui uses native button focus/activation. GPUI supports
  focus and Enter/Space activation; host-level focus traversal and accessibility
  integration remain the host's responsibility.

Visual inspiration: [React Bits](https://reactbits.dev) (card emphasis),
[Aceternity UI](https://ui.aceternity.com) (workspace-like tabs), and
[shadcn/ui](https://ui.shadcn.com) (restrained composable controls). These are
original native widgets, not copied React implementations. No decorative
animations, pointer-following effects, or continuously ticking transitions are
introduced.

**egui is not a software renderer.** A real fallback host must feed `RawInput`
into `Context::run_ui`, paint/tessellate shapes, handle texture upload/free,
process platform output, and schedule requested repaints. GL/wgpu painters still
need functioning drivers; a machine with no usable graphics backend needs a
separately supplied software painter or a headless control path. The example
intentionally clears texture deltas because it does not present a window.

## TypeScript bindings

[`bindings/node`](bindings/node) is a native Node.js addon (napi-rs) exposing
`Desktop`, the layouts, and typed errors to TypeScript, with generated
`index.d.ts` types. A TypeScript program can then drive workspace, focus, and
layout policy while the compositor host applies the placements. See its README
for build steps and usage.

## Development

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p mcsapi --example desktop
cargo check --workspace --all-targets --features mcsapi/gpui
cargo test --workspace --features mcsapi/gpui
cargo doc --workspace --no-deps
```

The root is a virtual manifest, so features are named per crate
(`mcsapi/gpui`), or use `-p mcsapi --features gpui`.

The example is headless: it prints placements and generates an egui workspace
bar frame without requiring a GPU or display.

## API and performance principles

The [Rust API Guidelines checklist](https://rust-lang.github.io/api-guidelines/checklist.html)
informs the typed IDs, private invariants, standard traits, explicit errors,
documented behavior, and borrowed/generic iterator interfaces. Unsafe code is
forbidden and public documentation is checked by the compiler.

The [Rust Performance Book](https://nnethercote.github.io/perf-book/) informs the
allocation-free placement iterator and deliberate separation of policy from
rendering. Owned trees still allocate when membership changes; they are not
claimed to outperform contiguous storage. Toolkit text/layout also allocates.
Desktop operations target small workspace/window counts: membership checks are
tree lookups, while focus traversal and finding a window's workspace visit
state as needed.

Measure representative release-build workloads (focus/workspace switching,
resize bursts, window churn, frame latency, allocations, and idle repainting)
before changing data structures or adding caches, parallelism, custom hashers,
inlining, or unsafe fast paths. Keep logical coordinates distinct from physical
pixels. There is no benchmark-backed speed claim in this scaffold.

Licensed under GPL-3.0-only; see [LICENSE](LICENSE).
