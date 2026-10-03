# mcsapi-compositor

A Smithay Wayland compositor host for mcsapi desktops. It owns the display,
the Wayland socket, rendering and input; a desktop shell plugs in through the
`Shell` trait and decides placements, shortcuts, title-bar behavior, and
paints its wallpaper, decorations and chrome with egui.

Windows come from three places, and get the same decorations, tiling and focus:

- **Wayland clients** connecting to the socket (asked to use server-side
  decorations through xdg-decoration).
- **X11 apps** through Xwayland, on demand (see below).
- **In-process apps**: `mcsapi_ui::App`s registered with `mcsapi-runtime`,
  provided through the `Apps` trait (for example the derisk core apps).

Supported today: xdg-shell toplevels and popups, xdg-decoration, wl_shm,
seat (keyboard and pointer), data device (clipboard), wl_output/xdg-output.
The session runs nested in a window of the current X11 or Wayland session
(Smithay's winit backend, GLES renderer, egui painted with `egui_glow`).

Not yet: a DRM/KMS + libinput backend for running on a bare TTY,
layer-shell, popup grabs, linux-dmabuf, X11 move/resize grabs.

## X11 apps

When `Xwayland` is on `PATH`, the compositor reserves an X11 display at
startup (`Shell::x11_display_reserved` gets it, and launched apps get
`DISPLAY`) but does not start a server. The first X11 client to connect
starts Xwayland; that client is passed through to the new server, file
descriptors included, so it never notices. Once no X11 client has been
connected for 5 seconds the server is shut down, and the display waits for
the next client. X11 windows tile like any other; override-redirect windows
(menus, tooltips) are drawn where the client puts them. Turn it off with
`Compositor::xwayland(false)`.

## Try it

```console
$ cargo run -p mcsapi-compositor --example tiling -- foot clock
```

`clock` is a built-in in-process app; anything else is spawned with
`WAYLAND_DISPLAY` (and `DISPLAY`, for X11 apps like `xterm`) set to the
session. Super+Enter opens foot, Super+C the
clock, Super+J/K move focus, Super+Space promotes, Super+1…4 switch
workspaces, Super+Q closes, Super+Escape quits.

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
