# mcsapi-compositor

A Smithay Wayland compositor host for mcsapi desktops. It owns the display,
the Wayland socket, rendering and input; a desktop shell plugs in through the
`Shell` trait and decides placements, shortcuts, title-bar behavior, and
paints its wallpaper, decorations and chrome with egui.

Windows come from two places, and get the same decorations, tiling and focus:

- **Wayland clients** connecting to the socket (asked to use server-side
  decorations through xdg-decoration).
- **In-process apps**: `mcsapi_ui::App`s registered with `mcsapi-runtime`,
  provided through the `Apps` trait (for example the derisk core apps).

Supported today: xdg-shell toplevels and popups, xdg-decoration, wl_shm,
seat (keyboard and pointer), data device (clipboard), wl_output/xdg-output.
The session runs nested in a window of the current X11 or Wayland session
(Smithay's winit backend, GLES renderer, egui painted with `egui_glow`), or,
with the `kms` feature, on the bare seat: the first connected display at its
preferred mode through DRM/KMS and GBM, input from libinput, and the GPU and
input devices taken through libseat (logind or seatd) and given back on a VT
switch (Ctrl+Alt+F1–F12). The bare seat is used when neither `WAYLAND_DISPLAY`
nor `DISPLAY` is set; `MCSAPI_BACKEND=kms` or `=winit` decides instead. On the
bare seat each frame is drawn offscreen and copied to the scanout buffer, so
every layer comes out the same as nested; `MCSAPI_OFFSCREEN=1` takes the same
path in a window, to compare the two.

Effects: shells can ask for frosted-glass panels by returning areas from
`Shell::blur_regions`; the host blurs what is behind them (dual Kawase, with
rounded corners) before painting the chrome, and the panel's own fill alpha
sets how much shows through. Frames follow the refresh rate of the monitor the
session window is on; `Shell::frame_interval` can slow them down, and
`OutputTiming::interval_for` picks a rate the display shows evenly (any rate
with VRR, a whole number of refreshes without; declare VRR with
`Compositor::vrr`, since a nested session cannot detect it).

Not yet: more than one display on the bare seat, hotplug, direct scanout of
client buffers, layer-shell, XWayland, popup grabs, linux-dmabuf.

## Try it

```console
$ cargo run -p mcsapi-compositor --example tiling -- foot clock
```

`clock` is a built-in in-process app; anything else is spawned with
`WAYLAND_DISPLAY` set to the session. Super+Enter opens foot, Super+C the
clock, Super+J/K move focus, Super+Space promotes, Super+1…4 switch
workspaces, Super+Q closes, Super+B toggles the frosted dock's blur, Super+L
toggles a ≤30 fps low power mode, Super+Escape quits.

## Hot reload in a nested instance

Run a nested session that picks up code changes while it keeps running:

```console
$ cargo install --locked dioxus-cli@0.7.10   # once
$ dx serve --hot-patch --platform linux -p mcsapi-compositor --example tiling \
    --features hotpatch --args clock
```

Edit a `Shell` method in `examples/tiling.rs` (placements, keys, painting,
chrome) and save: about a second later the window draws with the new code.
The window, its Wayland clients and the shell's state stay alive. This is
[Subsecond](https://crates.io/crates/subsecond) hot-patching, wired in through
the `hotpatch` feature:

- Wrap the shell in `Hot` (`Compositor::new(Hot(shell))`). Each call into
  the shell then goes through Subsecond's jump table and runs the newest
  version. Without the feature, or in release builds, `Hot` is a plain
  pass-through and nothing extra is compiled in.
- `Compositor::run` connects to `dx serve` for patches; run normally, it
  does nothing.
- Only the binary's own crate is patched (the example, or a desktop's
  `main.rs` crate). Changes to a struct's fields, to this library or to
  other crates need a rebuild; press `r` in `dx serve` to restart.

Without the Dioxus CLI, `bacon nested` (from the root `bacon.toml`) rebuilds
and restarts the nested instance on every save of any workspace crate.

The nested window opens on whatever `WAYLAND_DISPLAY` or `DISPLAY` the
command starts with, so a terminal inside another session (an mcsapi one
included) nests the instance there.

## Using it

Implement `Shell` (only window bookkeeping and `placements` are required),
optionally an `Apps` provider, then:

```rust,ignore
let mut compositor = Compositor::new(shell).apps(apps).size(1600, 900);
let remote = compositor.remote(); // run jobs on the shell from other threads
compositor.launch("foot").run()?;
```

Building needs `libxkbcommon-dev`. Running needs EGL/GLES drivers (Mesa's
llvmpipe works, also under Xvfb).
