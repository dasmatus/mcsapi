---
name: mcsapi
description: Develop, test, or use the mcsapi Rust workspace (tiling desktop policy for Smithay compositors, its UI/runtime crates, Node bindings, and MCP server). Use for any change in this repo or to check how its desktop policy behaves.
---

# Working on mcsapi

mcsapi is xmonad-like tiling policy for [Smithay](https://smithay.github.io/)
Wayland compositors. It is a library, not a runnable compositor: the host owns
surfaces, input, rendering, and the event loop; mcsapi owns workspace
membership, focus, and logical window placement.

## Layout

A Cargo workspace; every directory under `crates/` is a member and inherits
`[workspace.package]` and `[workspace.dependencies]` from the root `Cargo.toml`.

| Path | What it is |
| --- | --- |
| `crates/mcsapi` | Core policy: `Desktop`, `Workspace`, `WindowId`, `WorkspaceId`, `Layout` (`Tall`, `Monocle`), `Placement`, `toolkit`, `widgets`. |
| `crates/mcsapi-ui` | `App` trait drawn with egui and the shell `Theme`. |
| `crates/mcsapi-components` | shadcn/ui-style native egui widgets. |
| `crates/mcsapi-runtime` | App registration and instance lifecycle; no UI dependency. |
| `crates/x2mcsapi` | Restyles foreign apps (web, Electron, GTK, Qt) from the `Theme`. |
| `crates/mcsapi-mcp` | Stateless MCP server exposing the policy as tools. |
| `bindings/node` | napi-rs Node addon; separate `Cargo.lock`, not a workspace member. |

Read the root `README.md` for the full behavior contract before changing
semantics; it is the spec the tests follow.

## Invariants to keep

- IDs are nonzero (`WindowId::new(0)` is `None`). A window belongs to at most
  one workspace; duplicates are rejected without changing state.
- Tiling order: the main window first, then the rest by ID. Focus cycling wraps
  in that order. Removing the focused window focuses its next neighbor.
- `arrange` validates bounds up front and returns a lazy, exact-size,
  allocation-free iterator. Tiny outputs give `InsufficientSpace`, never
  zero-sized tiles. Coordinates are logical, not physical pixels.
- `#![forbid(unsafe_code)]` and `#![deny(missing_docs)]`: every public item
  needs a doc comment. Prefer iterators over collecting into `Vec` in `mcsapi`.
- Public enums are `#[non_exhaustive]`; downstream matches need a `_` arm.

## Commands

Linux builds need `libxkbcommon-dev` and `pkg-config` (Smithay links
xkbcommon). The GPUI feature needs more system packages; see the README.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p mcsapi --example desktop          # headless; no GPU or display
cargo test --workspace --features mcsapi/gpui  # features are named per crate
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

CI (`.github/workflows/ci.yml`) runs all of these with `--locked` on stable,
beta, and the MSRV (`rust-version`, 1.95), plus `cargo audit`. Run fmt, clippy,
and tests before pushing; commit `Cargo.lock` changes.

## Hot reload

`bacon.toml` defines watch jobs (`cargo install --locked bacon` once):

- `bacon` re-checks the workspace on every save.
- `bacon test`, `bacon clippy`, `bacon doc`, `bacon example` rerun those.
- `bacon mcp` rebuilds and restarts the MCP server on every save.

In a non-interactive shell use `bacon --headless <job>`, or just run the cargo
command directly; bacon needs a terminal UI for its key bindings.

## MCP server

```sh
cargo run -p mcsapi-mcp   # http://127.0.0.1:8787/mcp, override with MCSAPI_MCP_ADDR
```

`.mcp.json` registers it with Claude Code as `mcsapi`; `.vscode/mcp.json` does
the same for VS Code and Copilot. The server keeps no state and issues no
session ID, so each call must carry the full history.

- `simulate`: `{"workspaces": [1, 2], "operations": [{"op": "insert", "window": 1}, {"op": "move_window", "window": 1, "workspace": 2}], "bounds": {"x": 0, "y": 0, "width": 1920, "height": 1080}}`.
  Operations: `insert`, `remove`, `focus` (take `window`), `focus_next`,
  `focus_previous`, `promote_focused`, `switch_to` (`workspace`),
  `move_window` (`window`, `workspace`), `set_layout` (`layout`: `tall` or
  `monocle`). Returns `active`, each workspace's `windows` (tiling order),
  `focused` and `layout`, and `placements` when bounds were given.
- `arrange`: `{"layout": "tall", "bounds": {...}, "windows": [2, 1]}` returns
  placements for windows already in tiling order.

Use `simulate` to confirm expected behavior before writing a test, or to
answer "what happens if..." questions without compiling. Without the MCP
connection, call `mcsapi_mcp::simulate` from a test, or POST JSON-RPC directly:

```sh
curl -s http://127.0.0.1:8787/mcp -H 'content-type: application/json' \
  -H 'accept: application/json, text/event-stream' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"arrange","arguments":{"layout":"tall","bounds":{"x":0,"y":0,"width":1280,"height":720},"windows":[1,2,3]}}}'
```

When `Desktop` gains an operation, add a matching `Operation` variant and an
`apply` arm in `crates/mcsapi-mcp/src/lib.rs`, and a case in its tests.
