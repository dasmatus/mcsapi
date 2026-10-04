//! A Smithay Wayland compositor host for mcsapi desktops.
//!
//! The host owns the display, the Wayland socket, rendering and input. A
//! desktop shell plugs in through [`Shell`]: it decides where windows go
//! ([`Shell::placements`]), what keys and title-bar clicks do, and paints
//! its own wallpaper, decorations and chrome with egui. Windows come from two
//! places:
//!
//! - **Wayland clients** (foot, GTK, Qt, ...) connecting to the socket. They
//!   are asked to use server-side decorations.
//! - **In-process apps** registered with [`mcsapi_runtime`] and drawn with
//!   [`mcsapi_ui::App`], through an [`Apps`] provider. They get the same
//!   title bars, tiling and focus as Wayland clients.
//!
//! Each frame paints the wallpaper, then for every window from bottom to top
//! its decoration and its content, then blurs the areas under translucent
//! panels ([`Shell::blur_regions`]), then the chrome. Frames come every
//! [`Shell::frame_interval`], by default once per refresh of the monitor the
//! session window is on. The session runs nested
//! in a window of the current X11 or Wayland session (Smithay's winit
//! backend); a DRM/KMS backend is future work.
//!
//! ```no_run
//! use mcsapi::{Desktop, WindowId, WorkspaceId};
//! use mcsapi_compositor::{Compositor, Placement, Shell};
//!
//! struct Tiling {
//!     desktop: Desktop,
//!     next: u64,
//!     size: (i32, i32),
//! }
//!
//! impl Shell for Tiling {
//!     fn map_window(&mut self, _app_id: &str, _title: &str) -> WindowId {
//!         self.next += 1;
//!         let id = WindowId::new(self.next).unwrap();
//!         self.desktop.insert(id).unwrap();
//!         id
//!     }
//!     fn unmap_window(&mut self, window: WindowId) {
//!         let _ = self.desktop.remove(window);
//!     }
//!     fn set_output(&mut self, size: (i32, i32)) {
//!         self.size = size;
//!     }
//!     fn focused(&self) -> Option<WindowId> {
//!         self.desktop.active().focused()
//!     }
//!     fn placements(&self) -> Vec<Placement> {
//!         let bounds = mcsapi::Geometry::new((0, 0).into(), self.size.into());
//!         let focused = self.focused();
//!         self.desktop
//!             .active()
//!             .arrange(bounds)
//!             .into_iter()
//!             .flatten()
//!             .map(|p| Placement::undecorated(p.window, p.geometry, focused == Some(p.window)))
//!             .collect()
//!     }
//! }
//!
//! let desktop = Desktop::new((1..=4).filter_map(WorkspaceId::new))?;
//! Compositor::new(Tiling { desktop, next: 0, size: (1280, 800) })
//!     .launch("foot")
//!     .run()?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]

mod blur;
mod host;
mod hot;

use std::{fmt, time::Duration};

use mcsapi::{Geometry, WindowId};
pub use mcsapi_runtime::{AppId, InstanceId};
pub use mcsapi_ui::{App, GestureEvent, Theme, egui};
pub use smithay::input::keyboard::Keysym;
use smithay::reexports::calloop::channel;

pub use hot::Hot;

/// Where a window is drawn this frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Placement {
    /// The window.
    pub window: WindowId,
    /// Full frame, including any server-side decoration.
    pub frame: Geometry,
    /// Area for the window content; toplevels are configured to this size.
    pub client: Geometry,
    /// Whether the window has keyboard focus.
    pub focused: bool,
    /// Edges that touch a neighbour or the screen edge; clients square their
    /// corners and drop shadows there.
    pub tiled: Edges,
    /// Whether the window is maximized.
    pub maximized: bool,
}

impl Placement {
    /// A tiled placement without decorations: content fills the frame.
    pub fn undecorated(window: WindowId, frame: Geometry, focused: bool) -> Self {
        Self {
            window,
            frame,
            client: frame,
            focused,
            tiled: Edges::ALL,
            maximized: false,
        }
    }
}

/// A screen area whose backdrop is blurred before the chrome is painted.
///
/// Paint the panel itself in [`Shell::chrome`] with a translucent fill; the
/// fill's alpha sets how much of the blurred backdrop shows through.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Blur {
    /// Area in logical, output-relative coordinates.
    pub area: Geometry,
    /// Corner radius, matching the panel's rounded corners.
    pub corner_radius: u8,
    /// Blur strength from 1 (slight) to 10 (heavy); 0 draws nothing.
    pub strength: u8,
}

/// How the output refreshes, passed to [`Shell::frame_interval`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputTiming {
    /// Refresh rate in millihertz (60 Hz is 60 000).
    pub refresh_mhz: u32,
    /// Whether the output has variable refresh rate (VRR, Adaptive-Sync), so
    /// a frame can be shown whenever it is ready instead of on the next
    /// fixed refresh.
    pub vrr: bool,
}

impl Default for OutputTiming {
    /// 60 Hz without VRR.
    fn default() -> Self {
        Self {
            refresh_mhz: 60_000,
            vrr: false,
        }
    }
}

impl OutputTiming {
    /// One refresh period, the shortest useful frame interval.
    pub fn refresh_interval(&self) -> Duration {
        Duration::from_nanos(1_000_000_000_000 / u64::from(self.refresh_mhz.max(1_000)))
    }

    /// The frame interval for at most `fps` frames per second that this
    /// output can show evenly. With VRR that is simply `1/fps` (never faster
    /// than the refresh rate). Without it, frames must land on refreshes, so
    /// it is the shortest whole number of refresh periods that stays at or
    /// under `fps`: for 30 fps, 30 on 60 or 120 Hz, 28.8 on 144 Hz and 27.5
    /// on 165 Hz.
    pub fn interval_for(&self, fps: u32) -> Duration {
        let refresh = self.refresh_interval();
        let wanted = Duration::from_nanos(1_000_000_000 / u64::from(fps.max(1)));
        if wanted <= refresh {
            return refresh;
        }
        if self.vrr {
            return wanted;
        }
        // In millihertz, so 120 Hz / 30 fps is exactly 4 refreshes.
        let periods = self
            .refresh_mhz
            .max(1_000)
            .div_ceil(fps.max(1).saturating_mul(1_000));
        refresh * periods
    }
}

/// A set of window edges.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Edges {
    /// Left edge.
    pub left: bool,
    /// Right edge.
    pub right: bool,
    /// Top edge.
    pub top: bool,
    /// Bottom edge.
    pub bottom: bool,
}

impl Edges {
    /// No edges (a floating window).
    pub const NONE: Self = Self {
        left: false,
        right: false,
        top: false,
        bottom: false,
    };
    /// All edges (a tiled window).
    pub const ALL: Self = Self {
        left: true,
        right: true,
        top: true,
        bottom: true,
    };
}

/// What a primary-button press handled by [`Shell::pointer_down`] did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Press {
    /// Forward the press (and the drag that follows) to the window content.
    Client,
    /// The shell took it, for example a title-bar button or a window drag;
    /// motion goes to [`Shell::pointer_motion`] until release.
    Handled,
}

/// Modifier state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Modifiers {
    /// Super (logo) key.
    pub logo: bool,
    /// Shift.
    pub shift: bool,
    /// Control.
    pub ctrl: bool,
    /// Alt.
    pub alt: bool,
}

/// A key press or release, offered to [`Shell::key`] before anyone else.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyInput {
    /// The keysym without modifiers applied (layout-independent shortcuts).
    pub sym: Keysym,
    /// The text the key produces with the current modifiers, if any.
    pub text: Option<char>,
    /// Press or release.
    pub pressed: bool,
    /// Modifiers held.
    pub mods: Modifiers,
}

/// Who receives a key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum KeyRoute {
    /// The shell consumed it (a shortcut).
    Consume,
    /// The chrome's egui context (for example a search field).
    Chrome,
    /// The focused window.
    Client,
}

/// Work for the host, returned by [`Shell::take_commands`].
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Command {
    /// Launch an app: an in-process app if the [`Apps`] provider resolves
    /// the name, otherwise the program from [`Shell::spawn_argv`] with
    /// `WAYLAND_DISPLAY` pointing at this session.
    Launch(String),
    /// Ask a window to close (in-process apps are stopped).
    Close(WindowId),
    /// End the session.
    Quit,
    /// Type text into the focused window, as an on-screen keyboard does.
    /// In-process apps get it as text; Wayland clients get the key presses
    /// that produce it in the session's keymap, so characters the keymap
    /// can't type are skipped.
    TypeText(String),
    /// Press and release a key (Backspace, Return, arrows) in the focused
    /// window.
    Key(Keysym),
}

/// A request a client made about its own window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ClientRequest {
    /// Toggle maximized.
    Maximize,
    /// Minimize.
    Minimize,
}

/// The desktop shell driven by the compositor.
///
/// Coordinates are logical and output-relative. Only window bookkeeping and
/// [`Shell::placements`] are required; everything else has a neutral default.
pub trait Shell: 'static {
    /// Starts managing a new window (on its first commit, when its app ID and
    /// title are known) and returns its ID.
    fn map_window(&mut self, app_id: &str, title: &str) -> WindowId;

    /// Stops managing a destroyed window.
    fn unmap_window(&mut self, window: WindowId);

    /// The output was resized.
    fn set_output(&mut self, size: (i32, i32));

    /// The keyboard-focused window.
    fn focused(&self) -> Option<WindowId>;

    /// Visible windows, bottom to top.
    fn placements(&self) -> Vec<Placement>;

    /// A window changed its title.
    fn set_title(&mut self, _window: WindowId, _title: &str) {}

    /// Focus a window (secondary clicks on window content).
    fn focus(&mut self, _window: WindowId) {}

    /// The Wayland socket is ready; clients connect with this
    /// `WAYLAND_DISPLAY`.
    fn session_started(&mut self, _wayland_display: &str) {}

    /// Called about every frame for clocks and other polled state.
    fn tick(&mut self) {}

    /// Whether the chrome owns the pointer at `at` (bars, overlays).
    fn chrome_wants_pointer(&self, _at: (i32, i32)) -> bool {
        false
    }

    /// A primary-button press outside the chrome.
    fn pointer_down(&mut self, _at: (i32, i32), _time_ms: u64) -> Press {
        Press::Client
    }

    /// Pointer motion during a press the shell handled.
    fn pointer_motion(&mut self, _at: (i32, i32)) {}

    /// Release of a press the shell handled.
    fn pointer_up(&mut self) {}

    /// Decides who gets a key. Called for every press and release. Releases
    /// of consumed presses are still offered here, but remain hidden from clients.
    fn key(&mut self, _key: &KeyInput) -> KeyRoute {
        KeyRoute::Client
    }

    /// A touchpad gesture. Return `true` from a begin event to take the whole
    /// gesture (for example three-finger swipes between workspaces); its later
    /// events then all come here and the return value is ignored. Gestures
    /// the shell leaves go to the chrome or the content under the pointer.
    fn gesture(&mut self, _event: &GestureEvent) -> bool {
        false
    }

    /// A client asked to change its window state.
    fn client_request(&mut self, _window: WindowId, _request: ClientRequest) {}

    /// Colors for in-process apps.
    fn theme(&self) -> Theme {
        Theme::default()
    }

    /// Paints the desktop background.
    fn paint_background(&mut self, _painter: &egui::Painter, _screen: egui::Rect) {}

    /// Paints one window's decoration, below its content.
    fn paint_decoration(&mut self, _painter: &egui::Painter, _placement: &Placement) {}

    /// Shows the chrome above all windows. `elapsed_ms` counts from start.
    fn chrome(&mut self, _ui: &mut egui::Ui, _elapsed_ms: u32) {}

    /// Areas to blur under the chrome this frame, bottom to top. Called right
    /// after [`Shell::chrome`], so it can report panels laid out there.
    fn blur_regions(&self) -> Vec<Blur> {
        Vec::new()
    }

    /// Time until the next frame, given how the output refreshes; for
    /// example longer in a low power mode (see [`OutputTiming::interval_for`]).
    /// Clamped to 4 ms–1 s. The default renders once per refresh.
    fn frame_interval(&self, timing: &OutputTiming) -> Duration {
        timing.refresh_interval()
    }

    /// The command line for launching `app` as a Wayland client.
    fn spawn_argv(&mut self, app: &str) -> Vec<String> {
        vec![app.to_owned()]
    }

    /// Commands queued since the last call.
    fn take_commands(&mut self) -> Vec<Command> {
        Vec::new()
    }
}

/// In-process apps the compositor can launch, usually backed by an
/// [`mcsapi_runtime::Runtime`].
pub trait Apps {
    /// The app a launch name refers to (an app ID, or a friendly name).
    fn resolve(&self, name: &str) -> Option<AppId>;

    /// Starts an instance.
    fn launch(&mut self, app: &AppId) -> Result<InstanceId, mcsapi_runtime::Error>;

    /// Stops an instance.
    fn stop(&mut self, instance: InstanceId);

    /// A running app.
    fn app_mut(&mut self, instance: InstanceId) -> Option<&mut dyn App>;

    /// Styles an app's egui context before each frame (visuals, zoom).
    fn prepare(&mut self, _ctx: &egui::Context, _theme: &Theme) {}
}

/// Work to run on the shell from another thread.
pub type Job<S> = Box<dyn FnOnce(&mut S) + Send>;

/// Sends [`Job`]s to a running compositor, for example from an IPC thread.
pub struct Remote<S>(channel::Sender<Job<S>>);

impl<S> Clone for Remote<S> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<S> fmt::Debug for Remote<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Remote")
    }
}

impl<S> Remote<S> {
    /// Queues `job`; returns `false` once the compositor has stopped.
    pub fn run(&self, job: impl FnOnce(&mut S) + Send + 'static) -> bool {
        self.0.send(Box::new(job)).is_ok()
    }
}

/// Session configuration and entry point.
pub struct Compositor<S> {
    shell: S,
    apps: Option<Box<dyn Apps>>,
    size: (i32, i32),
    title: String,
    vrr: bool,
    launch: Vec<String>,
    jobs: Option<channel::Channel<Job<S>>>,
}

impl<S: Shell + 'static> Compositor<S> {
    /// A compositor for `shell`, 1280×800 when nested.
    pub fn new(shell: S) -> Self {
        Self {
            shell,
            apps: None,
            size: (1280, 800),
            title: "mcsapi".into(),
            vrr: false,
            launch: Vec::new(),
            jobs: None,
        }
    }

    /// Provides in-process apps.
    pub fn apps(mut self, apps: impl Apps + 'static) -> Self {
        self.apps = Some(Box::new(apps));
        self
    }

    /// Window size when nested.
    pub fn size(mut self, width: i32, height: i32) -> Self {
        self.size = (width, height);
        self
    }

    /// Title of the nested window.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Declares that the output has variable refresh rate, for example when
    /// the parent session drives a VRR monitor. Nested sessions cannot detect
    /// it, so [`OutputTiming::vrr`] is `false` unless set here.
    pub fn vrr(mut self, vrr: bool) -> Self {
        self.vrr = vrr;
        self
    }

    /// Launches an app once the session is up (see [`Command::Launch`]).
    pub fn launch(mut self, app: impl Into<String>) -> Self {
        self.launch.push(app.into());
        self
    }

    /// A handle for running jobs on the shell from other threads.
    pub fn remote(&mut self) -> Remote<S> {
        let (sender, channel) = channel::channel();
        self.jobs = Some(channel);
        Remote(sender)
    }

    /// Runs the session until its window closes or the shell quits.
    ///
    /// With the `hotpatch` feature in a debug build, this also listens for
    /// patches from `dx serve --hot-patch`; see [`Hot`].
    pub fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        hot::connect();
        host::run(self)
    }
}
