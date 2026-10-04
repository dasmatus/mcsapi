//! The compositor state, Wayland protocol handlers, input routing and
//! rendering.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    ffi::OsString,
    fs::File,
    io,
    os::fd::{AsRawFd, OwnedFd},
    process::{Child, Command as Process},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use mcsapi::WindowId;
use smithay::{
    backend::{
        egl,
        input::{
            AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, GestureBeginEvent as _,
            GestureEndEvent as _, GesturePinchUpdateEvent as _, GestureSwipeUpdateEvent as _,
            InputBackend, InputEvent, KeyState, KeyboardKeyEvent, PointerAxisEvent,
            PointerButtonEvent, TouchEvent, TouchSlot,
        },
        renderer::{
            Color32F, Frame, Renderer,
            element::{
                AsRenderElements, Element, RenderElement, surface::WaylandSurfaceRenderElement,
            },
            gles::GlesRenderer,
            utils::on_commit_buffer_handler,
        },
        winit::{self, WinitEvent, WinitGraphicsBackend},
    },
    delegate_compositor, delegate_data_device, delegate_output, delegate_pointer_gestures,
    delegate_seat, delegate_shm, delegate_xdg_decoration, delegate_xdg_shell,
    desktop::{PopupKind, PopupManager, Space, Window, WindowSurfaceType},
    input::{
        Seat, SeatHandler, SeatState,
        keyboard::{FilterResult, Keycode, Keysym, KeysymHandle, ModifiersState, XkbConfig},
        pointer::{self, AxisFrame, ButtonEvent, CursorImageStatus, MotionEvent},
    },
    output::{Mode as OutputMode, Output, PhysicalProperties, Subpixel},
    reexports::{
        calloop::{
            EventLoop, Interest, LoopSignal, Mode as CalloopMode, PostAction, channel,
            generic::Generic,
            timer::{TimeoutAction, Timer},
        },
        wayland_protocols::xdg::{
            decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode as DecorationMode,
            shell::server::xdg_toplevel::State as ToplevelState,
        },
        wayland_server::{
            Client, Display, DisplayHandle, Resource,
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::{wl_buffer, wl_seat, wl_surface::WlSurface},
        },
        winit::{dpi::LogicalSize, window::Window as WinitWindow},
    },
    utils::{Logical, Physical, Point, Rectangle, SERIAL_COUNTER, Scale, Serial, Size, Transform},
    wayland::{
        buffer::BufferHandler,
        compositor::{
            CompositorClientState, CompositorHandler, CompositorState, get_parent,
            is_sync_subsurface, with_states,
        },
        output::{OutputHandler, OutputManagerState},
        pointer_gestures::PointerGesturesState,
        selection::{
            SelectionHandler,
            data_device::{
                ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
                set_data_device_focus, set_data_device_selection,
            },
        },
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
            XdgToplevelSurfaceData,
            decoration::{XdgDecorationHandler, XdgDecorationState},
        },
        shm::{ShmHandler, ShmState},
        socket::ListeningSocketSource,
    },
};

use crate::{
    Apps, Blur, Capture, ClientRequest, Command, Compositor, GestureEvent, Input, InstanceId, Job,
    KeyInput, KeyRoute, Modifiers, MouseButton, OutputTiming, Placement, Press, Shell, a11y,
    accesskit, blur, egui,
};
use mcsapi_ui::gesture::EguiBridge;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;
const MAX_SELECTION_TRANSFERS: usize = 4;
const SELECTION_WRITE_TIMEOUT: Duration = Duration::from_secs(5);

static ACTIVE_SELECTION_TRANSFERS: AtomicUsize = AtomicUsize::new(0);

struct SelectionTransferGuard;

impl SelectionTransferGuard {
    fn acquire() -> Option<Self> {
        let mut active = ACTIVE_SELECTION_TRANSFERS.load(Ordering::Relaxed);
        loop {
            if active >= MAX_SELECTION_TRANSFERS {
                return None;
            }
            match ACTIVE_SELECTION_TRANSFERS.compare_exchange_weak(
                active,
                active + 1,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Some(Self),
                Err(current) => active = current,
            }
        }
    }
}

impl Drop for SelectionTransferGuard {
    fn drop(&mut self) {
        ACTIVE_SELECTION_TRANSFERS.fetch_sub(1, Ordering::Release);
    }
}

fn write_selection(fd: OwnedFd, text: &[u8], timeout: Duration) -> io::Result<()> {
    let file = File::from(fd);
    let raw_fd = file.as_raw_fd();
    // SAFETY: fcntl is called with a valid owned file descriptor.
    let flags = unsafe { libc::fcntl(raw_fd, libc::F_GETFL) };
    if flags == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fcntl is called with a valid owned file descriptor and its existing flags.
    if unsafe { libc::fcntl(raw_fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1 {
        return Err(io::Error::last_os_error());
    }

    let deadline = Instant::now() + timeout;
    let mut remaining = text;
    while !remaining.is_empty() {
        if Instant::now() >= deadline {
            return Err(io::Error::from(io::ErrorKind::TimedOut));
        }
        // SAFETY: the byte slice is valid for the duration of this write call.
        let written = unsafe { libc::write(raw_fd, remaining.as_ptr().cast(), remaining.len()) };
        if written > 0 {
            remaining = &remaining[written as usize..];
            continue;
        }
        if written == 0 {
            return Err(io::Error::from(io::ErrorKind::WriteZero));
        }

        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::Interrupted {
            continue;
        }
        if error.kind() != io::ErrorKind::WouldBlock {
            return Err(error);
        }

        let remaining_time = deadline.saturating_duration_since(Instant::now());
        if remaining_time.is_zero() {
            return Err(io::Error::from(io::ErrorKind::TimedOut));
        }
        let timeout_ms = remaining_time.as_millis().clamp(1, i32::MAX as u128) as i32;
        let mut descriptor = libc::pollfd {
            fd: raw_fd,
            events: libc::POLLOUT,
            revents: 0,
        };
        // SAFETY: descriptor points to one initialized pollfd and the fd remains owned.
        let ready = unsafe { libc::poll(&mut descriptor, 1, timeout_ms) };
        if ready == -1 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if ready == 0 {
            return Err(io::Error::from(io::ErrorKind::TimedOut));
        }
        if descriptor.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return Err(io::Error::from(io::ErrorKind::BrokenPipe));
        }
    }
    Ok(())
}
/// Touchpad scrolling without stop events (nested under winit) ends after
/// this long without motion.
const SCROLL_IDLE_MS: u32 = 50;

/// An egui context plus its GL painter (created once GL is available).
struct Egui {
    ctx: egui::Context,
    painter: Option<egui_glow::Painter>,
}

impl Egui {
    fn new() -> Self {
        Self {
            ctx: egui::Context::default(),
            painter: None,
        }
    }

    /// A context that also builds an AccessKit tree each frame.
    fn accessible() -> Self {
        let egui = Self::new();
        egui.ctx.enable_accesskit();
        egui
    }
}

/// Tessellated egui output with the texture updates it needs.
struct Pass {
    primitives: Vec<egui::ClippedPrimitive>,
    textures: egui::TexturesDelta,
}

/// Window content. Few windows exist, so the variant size gap is harmless.
#[allow(clippy::large_enum_variant)]
enum Content {
    /// A Wayland client toplevel.
    Wayland(Window),
    /// An in-process app instance with its own egui context and input queue.
    Internal {
        instance: InstanceId,
        egui: Egui,
        events: Vec<egui::Event>,
        title: String,
        app_id: String,
        /// The app's accessibility tree from its last frame.
        access: Option<accesskit::TreeUpdate>,
    },
}

/// Who receives pointer input until all buttons are released.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Route {
    Chrome,
    Shell,
    Content(Option<WindowId>),
}

/// Who receives a touchpad gesture from its begin to its end.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GestureRoute {
    Shell,
    Chrome,
    Internal(WindowId),
    Client,
}

/// Compositor state, generic over the desktop shell.
pub(crate) struct Host<S: Shell> {
    start: Instant,
    display: DisplayHandle,
    signal: LoopSignal,
    socket_name: OsString,

    compositor_state: CompositorState,
    xdg_shell_state: XdgShellState,
    _xdg_decoration_state: XdgDecorationState,
    shm_state: ShmState,
    _output_manager_state: OutputManagerState,
    _pointer_gestures_state: PointerGesturesState,
    seat_state: SeatState<Self>,
    data_device_state: DataDeviceState,
    popups: PopupManager,
    seat: Seat<Self>,
    space: Space<Window>,
    output: Output,
    backend: WinitGraphicsBackend<GlesRenderer>,
    gl: Option<Arc<glow::Context>>,
    retired: Vec<egui_glow::Painter>,
    blurrer: Option<blur::Blurrer>,
    /// Set once creating the blurrer failed, so it isn't retried every frame.
    blur_unavailable: bool,
    vrr: bool,
    /// Refresh rate of the monitor showing the session window.
    refresh_mhz: u32,
    timing_checked: Option<Instant>,

    shell: S,
    apps: Option<Box<dyn Apps>>,
    windows: HashMap<WindowId, Content>,
    /// Toplevels that have not committed yet.
    unmanaged: Vec<Window>,
    keyboard_focus: Option<WindowId>,

    chrome: Egui,
    decorations: Egui,
    chrome_events: Vec<egui::Event>,
    /// The chrome's accessibility tree from its last frame.
    chrome_access: Option<accesskit::TreeUpdate>,
    merger: a11y::Merger,
    #[cfg(feature = "atspi")]
    atspi: Option<crate::atspi::Bridge>,
    /// [`Command::Capture`] requests waiting for the next frame.
    captures: Vec<u64>,
    /// Agent commands, carried out one per frame: egui sees only one click
    /// per frame, and a tree or capture taken right after input should show
    /// its result.
    agent_queue: VecDeque<Command>,
    /// Frames to wait after injected input before reading the tree or the
    /// screen: one for the app to handle it, one to draw the result.
    settle: u8,
    /// Whether the latest input was injected; see [`Shell::input_source`].
    synthetic: bool,
    pointer: Point<f64, Logical>,
    route: Option<Route>,
    buttons: u32,
    pointer_target: Option<WindowId>,
    egui_mods: egui::Modifiers,
    gesture: Option<GestureRoute>,
    gesture_bridge: EguiBridge,
    /// The in-process app being scrolled with fingers, and when it last moved.
    scroll: Option<(WindowId, u32)>,
    /// The touch point driving the pointer, from its down to its up.
    touch: Option<TouchSlot>,
    /// Keys whose press the shell consumed; their release is consumed too.
    consumed_keys: HashSet<u32>,

    children: Vec<Child>,
}

/// Per-client Wayland state.
#[derive(Default)]
struct ClientState {
    compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

pub(crate) fn run<S: Shell + 'static>(config: Compositor<S>) -> Result {
    let Compositor {
        shell,
        apps,
        size: (w, h),
        title,
        vrr,
        launch,
        jobs,
    } = config;
    let merger = a11y::Merger::new(title.clone());
    let mut event_loop: EventLoop<Host<S>> = EventLoop::try_new()?;
    let display: Display<Host<S>> = Display::new()?;
    let dh = display.handle();

    let (backend, winit_loop) = winit::init_from_attributes::<GlesRenderer>(
        WinitWindow::default_attributes()
            .with_title(title)
            .with_inner_size(LogicalSize::new(w, h))
            .with_visible(true),
    )
    .map_err(|e| format!("cannot open a window for the session: {e}"))?;
    let size = backend.window_size();

    let output = Output::new(
        "mcsapi-0".into(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "mcsapi".into(),
            model: "nested".into(),
        },
    );
    let _global = output.create_global::<Host<S>>(&dh);
    let mode = OutputMode {
        size,
        refresh: 60_000,
    };
    output.change_current_state(
        Some(mode),
        Some(Transform::Normal),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(mode);

    let mut seat_state = SeatState::new();
    let mut seat = seat_state.new_wl_seat(&dh, "seat0");
    seat.add_keyboard(XkbConfig::default(), 400, 30)?;
    seat.add_pointer();

    let mut space = Space::default();
    space.map_output(&output, (0, 0));

    let socket = ListeningSocketSource::new_auto()?;
    let socket_name = socket.socket_name().to_os_string();
    event_loop
        .handle()
        .insert_source(socket, |stream, _, host| {
            if let Err(e) = host
                .display
                .insert_client(stream, Arc::new(ClientState::default()))
            {
                eprintln!("mcsapi-compositor: rejected client: {e}");
            }
        })?;
    event_loop.handle().insert_source(
        Generic::new(display, Interest::READ, CalloopMode::Level),
        |_, display, host| {
            // SAFETY: the display outlives the event loop's sources.
            unsafe { display.get_mut().dispatch_clients(host)? };
            Ok(PostAction::Continue)
        },
    )?;

    let mut host = Host {
        start: Instant::now(),
        display: dh.clone(),
        signal: event_loop.get_signal(),
        socket_name,
        compositor_state: CompositorState::new::<Host<S>>(&dh),
        xdg_shell_state: XdgShellState::new::<Host<S>>(&dh),
        _xdg_decoration_state: XdgDecorationState::new::<Host<S>>(&dh),
        shm_state: ShmState::new::<Host<S>>(&dh, vec![]),
        _output_manager_state: OutputManagerState::new_with_xdg_output::<Host<S>>(&dh),
        _pointer_gestures_state: PointerGesturesState::new::<Host<S>>(&dh),
        data_device_state: DataDeviceState::new::<Host<S>>(&dh),
        seat_state,
        popups: PopupManager::default(),
        seat,
        space,
        output,
        backend,
        gl: None,
        retired: Vec::new(),
        blurrer: None,
        blur_unavailable: false,
        vrr,
        refresh_mhz: 60_000,
        timing_checked: None,
        shell,
        apps,
        windows: HashMap::new(),
        unmanaged: Vec::new(),
        keyboard_focus: None,
        chrome: Egui::accessible(),
        decorations: Egui::new(),
        chrome_events: Vec::new(),
        chrome_access: None,
        merger,
        #[cfg(feature = "atspi")]
        atspi: None,
        captures: Vec::new(),
        agent_queue: VecDeque::new(),
        settle: 0,
        synthetic: false,
        pointer: (0.0, 0.0).into(),
        route: None,
        buttons: 0,
        pointer_target: None,
        egui_mods: egui::Modifiers::default(),
        gesture: None,
        gesture_bridge: EguiBridge::default(),
        scroll: None,
        touch: None,
        consumed_keys: HashSet::new(),
        children: Vec::new(),
    };
    #[cfg(feature = "atspi")]
    {
        let (sender, requests) = channel::channel();
        host.atspi = Some(crate::atspi::Bridge::new(sender));
        event_loop
            .handle()
            .insert_source(requests, |event, _, host| {
                if let channel::Event::Msg(request) = event {
                    host.atspi_request(request);
                    host.run_commands();
                }
            })?;
    }
    host.shell.set_output((size.w, size.h));
    host.shell
        .session_started(&host.socket_name.to_string_lossy());

    event_loop
        .handle()
        .insert_source(winit_loop, |event, _, host| host.winit_event(event))?;
    if let Some(jobs) = jobs {
        event_loop.handle().insert_source(jobs, |event, _, host| {
            if let channel::Event::Msg(job) = event {
                let job: Job<S> = job;
                job(&mut host.shell);
                host.run_commands();
            }
        })?;
    }
    event_loop.handle().insert_source(
        Timer::from_duration(Duration::from_millis(300)),
        move |_, _, host| {
            for app in &launch {
                host.launch(app);
            }
            TimeoutAction::Drop
        },
    )?;
    event_loop.handle().insert_source(
        Timer::from_duration(Duration::from_secs(1)),
        |_, _, host| {
            host.reap_children();
            TimeoutAction::ToDuration(Duration::from_secs(1))
        },
    )?;
    event_loop
        .handle()
        .insert_source(Timer::immediate(), |_, _, host| {
            host.render();
            let interval = host.shell.frame_interval(&host.timing());
            TimeoutAction::ToDuration(
                interval.clamp(Duration::from_millis(4), Duration::from_secs(1)),
            )
        })?;

    let run_result = event_loop.run(None, &mut host, |host| {
        host.space.refresh();
        host.popups.cleanup();
        let _ = host.display.flush_clients();
    });
    for child in &mut host.children {
        let _ = child.kill();
        let _ = child.wait();
    }
    run_result?;
    Ok(())
}

impl<S: Shell> Host<S> {
    fn now_ms(&self) -> u32 {
        self.start.elapsed().as_millis() as u32
    }

    /// Carries out commands the shell queued.
    fn run_commands(&mut self) {
        loop {
            let commands = self.shell.take_commands();
            if commands.is_empty() {
                return;
            }
            for command in commands {
                match command {
                    Command::Launch(app) => self.launch(&app),
                    Command::Close(window) => self.close(window),
                    Command::Quit => self.signal.stop(),
                    command @ (Command::Describe(_) | Command::Capture(_)) => {
                        // The shell may just have changed; show it drawn.
                        self.settle = self.settle.max(2);
                        self.agent_queue.push_back(command);
                    }
                    command @ (Command::Act { .. } | Command::Input(_)) => {
                        self.agent_queue.push_back(command);
                    }
                    // Commands added later are ignored until handled.
                    #[allow(unreachable_patterns)]
                    _ => {}
                }
            }
        }
    }

    fn launch(&mut self, name: &str) {
        if let Some(apps) = &mut self.apps
            && let Some(app) = apps.resolve(name)
        {
            match apps.launch(&app) {
                Ok(instance) => {
                    let title = apps
                        .app_mut(instance)
                        .map(|a| a.title().to_owned())
                        .unwrap_or_default();
                    let id = self.shell.map_window(app.as_str(), &title);
                    self.windows.insert(
                        id,
                        Content::Internal {
                            instance,
                            egui: Egui::accessible(),
                            events: Vec::new(),
                            title,
                            app_id: app.as_str().to_owned(),
                            access: None,
                        },
                    );
                }
                Err(e) => eprintln!("mcsapi-compositor: cannot launch {name}: {e}"),
            }
            return;
        }
        let argv = self.shell.spawn_argv(name);
        let Some((program, args)) = argv.split_first() else {
            return;
        };
        let lang = std::env::var("LANG")
            .ok()
            .filter(|l| l.to_uppercase().contains("UTF-8"))
            .unwrap_or_else(|| "C.UTF-8".into());
        let spawned = Process::new(program)
            .args(args)
            .env("WAYLAND_DISPLAY", &self.socket_name)
            .env("XDG_SESSION_TYPE", "wayland")
            .env("GDK_BACKEND", "wayland")
            .env("QT_QPA_PLATFORM", "wayland")
            .env("LANG", lang)
            .env_remove("DISPLAY")
            .spawn();
        match spawned {
            Ok(child) => self.children.push(child),
            Err(e) => eprintln!("mcsapi-compositor: cannot launch {name}: {e}"),
        }
    }

    fn reap_children(&mut self) {
        self.children
            .retain_mut(|child| child.try_wait().map_or(true, |status| status.is_none()));
    }

    fn close(&mut self, window: WindowId) {
        match self.windows.get(&window) {
            Some(Content::Wayland(w)) => {
                if let Some(toplevel) = w.toplevel() {
                    toplevel.send_close();
                }
            }
            Some(Content::Internal { instance, .. }) => {
                let instance = *instance;
                if let Some(apps) = &mut self.apps {
                    apps.stop(instance);
                }
                if let Some(Content::Internal {
                    egui:
                        Egui {
                            painter: Some(painter),
                            ..
                        },
                    ..
                }) = self.windows.remove(&window)
                {
                    self.retired.push(painter);
                }
                self.shell.unmap_window(window);
            }
            None => {}
        }
    }

    fn wayland_window_of(&self, surface: &WlSurface) -> Option<(WindowId, &Window)> {
        self.windows.iter().find_map(|(id, content)| match content {
            Content::Wayland(w) if w.toplevel().is_some_and(|t| t.wl_surface() == surface) => {
                Some((*id, w))
            }
            _ => None,
        })
    }

    /// Pushes placements to toplevels and the space, and syncs keyboard focus.
    fn sync(&mut self) {
        let placements = self.shell.placements();
        for (id, content) in &self.windows {
            if let Content::Wayland(window) = content
                && !placements.iter().any(|p| p.window == *id)
            {
                self.space.unmap_elem(window);
            }
        }
        for p in &placements {
            let Some(Content::Wayland(window)) = self.windows.get(&p.window) else {
                continue;
            };
            if let Some(toplevel) = window.toplevel() {
                toplevel.with_pending_state(|state| {
                    state.size = Some(p.client.size);
                    let edges = p.tiled;
                    for (s, on) in [
                        (ToplevelState::TiledLeft, edges.left),
                        (ToplevelState::TiledRight, edges.right),
                        (ToplevelState::TiledTop, edges.top),
                        (ToplevelState::TiledBottom, edges.bottom),
                        (ToplevelState::Maximized, p.maximized),
                    ] {
                        if on {
                            state.states.set(s);
                        } else {
                            state.states.unset(s);
                        }
                    }
                    if p.focused {
                        state.states.set(ToplevelState::Activated);
                    } else {
                        state.states.unset(ToplevelState::Activated);
                    }
                });
                toplevel.send_pending_configure();
            }
            let offset = window.geometry().loc;
            self.space
                .map_element(window.clone(), p.client.loc - offset, false);
            self.space.raise_element(window, false);
        }

        let focused = self.shell.focused();
        if focused != self.keyboard_focus {
            self.keyboard_focus = focused;
            let surface = focused.and_then(|id| match self.windows.get(&id) {
                Some(Content::Wayland(w)) => w.toplevel().map(|t| t.wl_surface().clone()),
                _ => None,
            });
            if let Some(keyboard) = self.seat.get_keyboard() {
                keyboard.set_focus(self, surface, SERIAL_COUNTER.next_serial());
            }
        }
    }

    fn winit_event(&mut self, event: WinitEvent) {
        match event {
            WinitEvent::Resized { size, .. } => {
                let mode = OutputMode {
                    size,
                    refresh: self.refresh_mhz as i32,
                };
                self.output
                    .change_current_state(Some(mode), None, None, None);
                self.output.set_preferred(mode);
                self.shell.set_output((size.w, size.h));
            }
            WinitEvent::Input(event) => self.input(event),
            WinitEvent::CloseRequested => self.signal.stop(),
            #[cfg(feature = "atspi")]
            WinitEvent::Focus(focused) => {
                if let Some(atspi) = &mut self.atspi
                    && !atspi.focused(focused)
                {
                    self.atspi = None;
                }
            }
            #[cfg(not(feature = "atspi"))]
            WinitEvent::Focus(_) => {}
            WinitEvent::Redraw => {}
        }
        self.run_commands();
    }

    fn point(&self) -> (i32, i32) {
        (self.pointer.x as i32, self.pointer.y as i32)
    }

    fn egui_pos(&self) -> egui::Pos2 {
        egui::pos2(self.pointer.x as f32, self.pointer.y as f32)
    }

    fn chrome_wants_pointer(&self) -> bool {
        self.shell.chrome_wants_pointer(self.point())
            || self.chrome.ctx.egui_wants_pointer_input()
            || egui::Popup::is_any_open(&self.chrome.ctx)
    }

    /// The topmost window whose content is under the pointer.
    fn content_under(&self) -> Option<WindowId> {
        let (x, y) = self.point();
        self.shell
            .placements()
            .into_iter()
            .rev()
            .find(|p| {
                let g = p.frame;
                x >= g.loc.x && y >= g.loc.y && x < g.loc.x + g.size.w && y < g.loc.y + g.size.h
            })
            .filter(|p| {
                let g = p.client;
                x >= g.loc.x && y >= g.loc.y && x < g.loc.x + g.size.w && y < g.loc.y + g.size.h
            })
            .map(|p| p.window)
    }

    fn internal_events(&mut self, window: Option<WindowId>) -> Option<&mut Vec<egui::Event>> {
        match self.windows.get_mut(&window?) {
            Some(Content::Internal { events, .. }) => Some(events),
            _ => None,
        }
    }

    /// Routes one input event. Generic so a libinput backend, which also
    /// reports touchpad gestures, can share it; winit reports none.
    fn input<B: InputBackend>(&mut self, event: InputEvent<B>) {
        self.set_synthetic(false);
        match event {
            InputEvent::Keyboard { event } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time_msec(&event);
                let pressed = event.state() == KeyState::Pressed;
                let keycode: u32 = event.key_code().into();
                let Some(keyboard) = self.seat.get_keyboard() else {
                    return;
                };
                keyboard.input::<(), _>(
                    self,
                    event.key_code(),
                    event.state(),
                    serial,
                    time,
                    |host, modifiers, handle| host.filter_key(modifiers, &handle, pressed, keycode),
                );
            }
            InputEvent::PointerMotionAbsolute { event } => {
                let size = self.backend.window_size();
                self.pointer = event.position_transformed((size.w, size.h).into());
                self.pointer_motion(Event::time_msec(&event));
            }
            InputEvent::PointerButton { event } => {
                self.pointer_button(event.button_code(), event.state(), Event::time_msec(&event));
            }
            // A touchscreen (a phone, or a laptop's screen) drives the pointer
            // with its first finger: down presses the primary button where it
            // lands, motion drags, and up or cancel releases. The chrome, the
            // shell's own hit testing and in-process apps then work by touch
            // as they do by mouse. Further fingers are ignored until the first
            // lifts. Clients get pointer events, not wl_touch.
            InputEvent::TouchDown { event } if self.touch.is_none() => {
                let size = self.backend.window_size();
                self.touch = Some(event.slot());
                self.pointer = event.position_transformed((size.w, size.h).into());
                let time = Event::time_msec(&event);
                self.pointer_motion(time);
                self.pointer_button(BTN_LEFT, ButtonState::Pressed, time);
            }
            InputEvent::TouchMotion { event } if self.touch == Some(event.slot()) => {
                let size = self.backend.window_size();
                self.pointer = event.position_transformed((size.w, size.h).into());
                self.pointer_motion(Event::time_msec(&event));
            }
            InputEvent::TouchUp { event } if self.touch == Some(event.slot()) => {
                self.touch = None;
                self.pointer_button(BTN_LEFT, ButtonState::Released, Event::time_msec(&event));
            }
            InputEvent::TouchCancel { event } if self.touch == Some(event.slot()) => {
                self.touch = None;
                self.pointer_button(BTN_LEFT, ButtonState::Released, Event::time_msec(&event));
            }
            InputEvent::PointerAxis { event } => {
                let amount = |axis| {
                    event
                        .amount(axis)
                        .or_else(|| event.amount_v120(axis).map(|v| v * 15.0 / 120.0))
                        .unwrap_or(0.0)
                };
                let (h, v) = (amount(Axis::Horizontal), amount(Axis::Vertical));
                self.axis(h, v, event.source(), Event::time_msec(&event));
            }
            InputEvent::GestureSwipeBegin { event } => self.gesture(
                GestureEvent::SwipeBegin {
                    fingers: event.fingers(),
                },
                Event::time_msec(&event),
            ),
            InputEvent::GestureSwipeUpdate { event } => self.gesture(
                GestureEvent::SwipeUpdate {
                    delta: egui::vec2(event.delta_x() as f32, event.delta_y() as f32),
                },
                Event::time_msec(&event),
            ),
            InputEvent::GestureSwipeEnd { event } => self.gesture(
                GestureEvent::SwipeEnd {
                    cancelled: event.cancelled(),
                },
                Event::time_msec(&event),
            ),
            InputEvent::GesturePinchBegin { event } => self.gesture(
                GestureEvent::PinchBegin {
                    fingers: event.fingers(),
                },
                Event::time_msec(&event),
            ),
            InputEvent::GesturePinchUpdate { event } => self.gesture(
                GestureEvent::PinchUpdate {
                    delta: egui::vec2(event.delta_x() as f32, event.delta_y() as f32),
                    scale: event.scale() as f32,
                    rotation: (event.rotation() as f32).to_radians(),
                },
                Event::time_msec(&event),
            ),
            InputEvent::GesturePinchEnd { event } => self.gesture(
                GestureEvent::PinchEnd {
                    cancelled: event.cancelled(),
                },
                Event::time_msec(&event),
            ),
            InputEvent::GestureHoldBegin { event } => self.gesture(
                GestureEvent::HoldBegin {
                    fingers: event.fingers(),
                },
                Event::time_msec(&event),
            ),
            InputEvent::GestureHoldEnd { event } => self.gesture(
                GestureEvent::HoldEnd {
                    cancelled: event.cancelled(),
                },
                Event::time_msec(&event),
            ),
            _ => {}
        }
    }

    /// Carries out the next queued agent command, if it is its turn.
    fn agent_step(&mut self) {
        self.settle = self.settle.saturating_sub(1);
        let reads = matches!(
            self.agent_queue.front(),
            Some(Command::Describe(_) | Command::Capture(_))
        );
        if reads && self.settle > 0 {
            return;
        }
        match self.agent_queue.pop_front() {
            Some(Command::Describe(request)) => {
                let tree = self.build_tree();
                let snapshot = a11y::snapshot(&self.merger, &tree);
                self.shell.described(request, snapshot);
            }
            Some(Command::Capture(request)) => self.captures.push(request),
            Some(Command::Act {
                element,
                action,
                value,
            }) => {
                self.set_synthetic(true);
                self.act(accesskit::NodeId(element), action, value);
                self.settle = 2;
            }
            Some(Command::Input(input)) => {
                self.inject(input);
                self.settle = 2;
            }
            _ => {}
        }
        self.run_commands();
    }

    /// Tells the shell when input switches between injected and real.
    fn set_synthetic(&mut self, synthetic: bool) {
        if self.synthetic != synthetic {
            self.synthetic = synthetic;
            self.shell.input_source(synthetic);
        }
    }

    /// A window's title and app ID, as its client or app reports them.
    fn window_label(&self, window: WindowId) -> (String, String) {
        match self.windows.get(&window) {
            Some(Content::Internal { title, app_id, .. }) => (title.clone(), app_id.clone()),
            Some(Content::Wayland(w)) => w
                .toplevel()
                .map(|t| {
                    with_states(t.wl_surface(), |states| {
                        states
                            .data_map
                            .get::<XdgToplevelSurfaceData>()
                            .and_then(|d| d.lock().ok())
                            .map(|d| {
                                (
                                    d.title.clone().unwrap_or_default(),
                                    d.app_id.clone().unwrap_or_default(),
                                )
                            })
                            .unwrap_or_default()
                    })
                })
                .unwrap_or_default(),
            None => Default::default(),
        }
    }

    /// The merged accessibility tree of the chrome and the visible windows.
    fn build_tree(&mut self) -> accesskit::TreeUpdate {
        let placements = self.shell.placements();
        let labels: Vec<(String, String)> = placements
            .iter()
            .map(|p| self.window_label(p.window))
            .collect();
        let subtrees = self.shell.access_subtrees();
        let windows: Vec<a11y::WindowInfo<'_>> = placements
            .iter()
            .zip(&labels)
            .map(|(p, (title, app_id))| a11y::WindowInfo {
                window: p.window,
                title,
                app_id,
                frame: p.frame,
                focused: p.focused,
                content: match self.windows.get(&p.window) {
                    Some(Content::Internal { access, .. }) => access.as_ref(),
                    _ => None,
                },
            })
            .collect();
        self.merger
            .build(self.chrome_access.as_ref(), &windows, &subtrees)
    }

    /// An action from an AT-SPI client (a screen reader, or an agent
    /// driving the desktop through AT-SPI).
    #[cfg(feature = "atspi")]
    fn atspi_request(&mut self, request: accesskit::ActionRequest) {
        self.set_synthetic(true);
        let value = match &request.data {
            Some(accesskit::ActionData::Value(v)) => Some(v.to_string()),
            _ => None,
        };
        self.act(request.target_node, request.action, value);
    }

    /// Performs an accessibility action on a node of the merged tree.
    fn act(&mut self, node: accesskit::NodeId, action: accesskit::Action, value: Option<String>) {
        use a11y::Source;
        let Some(source) = self.merger.source(node) else {
            return;
        };
        let (window, target) = match source {
            Source::Root => return,
            Source::Window(window) => {
                if matches!(action, accesskit::Action::Focus | accesskit::Action::Click) {
                    self.shell.focus(window);
                }
                return;
            }
            Source::Shell(window, node, _) => {
                self.shell
                    .access_action(window, node, action, value.as_deref());
                return;
            }
            Source::Chrome(node) => (None, node),
            Source::App(window, node) => {
                // Keys only reach the focused app.
                self.shell.focus(window);
                (Some(window), node)
            }
        };
        let request = |action, data| {
            egui::Event::AccessKitActionRequest(accesskit::ActionRequest {
                action,
                target_tree: accesskit::TreeId::ROOT,
                target_node: target,
                data,
            })
        };
        let events = if action == accesskit::Action::SetValue {
            // Focus the field, select everything in it and type over it;
            // egui applies the focus before the field reads the keys.
            let select_all = egui::Modifiers::COMMAND;
            vec![
                request(accesskit::Action::Focus, None),
                egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: Some(egui::Key::A),
                    pressed: true,
                    repeat: false,
                    modifiers: select_all,
                },
                egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: Some(egui::Key::A),
                    pressed: false,
                    repeat: false,
                    modifiers: select_all,
                },
                egui::Event::Text(value.unwrap_or_default()),
            ]
        } else {
            vec![request(action, None)]
        };
        let queue = match window {
            None => Some(&mut self.chrome_events),
            Some(w) => self.internal_events(Some(w)),
        };
        if let Some(queue) = queue {
            queue.extend(events);
        }
    }

    /// Feeds injected input through the same paths as the seat's.
    fn inject(&mut self, input: Input) {
        self.set_synthetic(true);
        let time = self.now_ms();
        let code = |button| match button {
            MouseButton::Left => BTN_LEFT,
            MouseButton::Right => BTN_RIGHT,
            MouseButton::Middle => BTN_MIDDLE,
        };
        match input {
            Input::Move { x, y } => self.move_pointer(x, y, time),
            Input::Button { button, pressed } => {
                let state = if pressed {
                    ButtonState::Pressed
                } else {
                    ButtonState::Released
                };
                self.pointer_button(code(button), state, time);
            }
            Input::Click { x, y, button } => {
                self.move_pointer(x, y, time);
                self.pointer_button(code(button), ButtonState::Pressed, time);
                self.pointer_button(code(button), ButtonState::Released, time);
            }
            Input::Scroll { dx, dy } => {
                self.axis(f64::from(dx), f64::from(dy), AxisSource::Wheel, time);
            }
            Input::Key { sym, mods } => {
                if !self.press_key(sym, mods, time) {
                    eprintln!("mcsapi-compositor: no key for {sym:?} in the keymap");
                }
            }
            Input::Text(text) => {
                for c in text.chars() {
                    let sym = match c {
                        '\n' => Keysym::Return,
                        '\t' => Keysym::Tab,
                        c => Keysym::from_char(c),
                    };
                    if !self.press_key(sym, Modifiers::default(), time) {
                        self.type_into_egui(c);
                    }
                }
            }
        }
    }

    fn move_pointer(&mut self, x: i32, y: i32, time: u32) {
        let size = self.backend.window_size();
        self.pointer = (
            f64::from(x.clamp(0, (size.w - 1).max(0))),
            f64::from(y.clamp(0, (size.h - 1).max(0))),
        )
            .into();
        self.pointer_motion(time);
    }

    /// Presses and releases the key that produces `sym`, holding `mods`
    /// (and Shift, if the layout puts `sym` on the shifted level). Returns
    /// `false` if the keymap has no such key.
    fn press_key(&mut self, sym: Keysym, mods: Modifiers, time: u32) -> bool {
        let Some((keycode, shifted)) = self.keycode_for(sym) else {
            return false;
        };
        let mut held = Vec::new();
        for (on, modifier) in [
            (mods.ctrl, Keysym::Control_L),
            (mods.alt, Keysym::Alt_L),
            (mods.logo, Keysym::Super_L),
            (mods.shift || shifted, Keysym::Shift_L),
        ] {
            if on && let Some((code, _)) = self.keycode_for(modifier) {
                held.push(code);
            }
        }
        for &code in &held {
            self.key_event(code, KeyState::Pressed, time);
        }
        self.key_event(keycode, KeyState::Pressed, time);
        self.key_event(keycode, KeyState::Released, time);
        for &code in held.iter().rev() {
            self.key_event(code, KeyState::Released, time);
        }
        true
    }

    /// The key producing `sym` in the active layout, and whether it needs
    /// Shift.
    fn keycode_for(&mut self, sym: Keysym) -> Option<(Keycode, bool)> {
        let keyboard = self.seat.get_keyboard()?;
        keyboard.with_xkb_state(self, |context| {
            let xkb = context.xkb().lock().ok()?;
            let layout = xkb.active_layout();
            // SAFETY: the keymap is only borrowed while the lock is held.
            let keymap = unsafe { xkb.keymap() };
            let mut found = None;
            keymap.key_for_each(|_, code| {
                if found.is_some() {
                    return;
                }
                for level in 0..2 {
                    if keymap
                        .key_get_syms_by_level(code, layout.0, level)
                        .contains(&sym)
                    {
                        found = Some((code, level == 1));
                        return;
                    }
                }
            });
            found
        })
    }

    fn key_event(&mut self, keycode: Keycode, state: KeyState, time: u32) {
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        let pressed = state == KeyState::Pressed;
        let raw: u32 = keycode.into();
        keyboard.input::<(), _>(
            self,
            keycode,
            state,
            SERIAL_COUNTER.next_serial(),
            time,
            |host, modifiers, handle| host.filter_key(modifiers, &handle, pressed, raw),
        );
    }

    /// Types a character the keymap lacks into the egui context that would
    /// receive keys: the chrome if it has keyboard focus, else the focused
    /// in-process app. Wayland clients only take keys, so they miss it.
    fn type_into_egui(&mut self, c: char) {
        let chrome = self.chrome.ctx.egui_wants_keyboard_input()
            || self.chrome.ctx.memory(|m| m.focused()).is_some();
        let focused = self.shell.focused();
        let events = if chrome {
            Some(&mut self.chrome_events)
        } else {
            self.internal_events(focused)
        };
        if let Some(events) = events {
            events.push(egui::Event::Text(c.to_string()));
        }
    }

    /// Scrolls whatever is under the pointer (or the chrome), from a wheel,
    /// a touchpad or injected input.
    fn axis(&mut self, h: f64, v: f64, source: AxisSource, time: u32) {
        let fingers = matches!(source, AxisSource::Finger | AxisSource::Continuous);
        // libinput ends finger scrolling with an empty event.
        let stop = fingers && h == 0.0 && v == 0.0;
        let modifiers = self.egui_mods;
        let wheel = |phase| egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(-h as f32, -v as f32),
            phase,
            modifiers,
        };
        if self.chrome_wants_pointer() {
            self.chrome_events.push(wheel(egui::TouchPhase::Move));
            return;
        }
        let under = self.content_under();
        if fingers && let Some(window) = under.filter(|w| self.is_internal(*w)) {
            // Bracket finger scrolling in Start and End so apps can
            // follow it 1:1 and coast (mcsapi_ui::gesture).
            let now = self.now_ms();
            let ongoing = self.scroll.is_some_and(|(w, _)| w == window);
            if !ongoing {
                self.end_scroll();
            }
            let mut events = Vec::with_capacity(2);
            if !ongoing && !stop {
                events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::Vec2::ZERO,
                    phase: egui::TouchPhase::Start,
                    modifiers,
                });
            }
            events.push(wheel(if stop {
                egui::TouchPhase::End
            } else {
                egui::TouchPhase::Move
            }));
            self.scroll = (!stop).then_some((window, now));
            if stop && !ongoing {
                return;
            }
            self.internal_events(Some(window))
                .expect("checked above")
                .extend(events);
            return;
        }
        if let Some(events) = self.internal_events(under) {
            events.push(wheel(egui::TouchPhase::Move));
            return;
        }
        let mut frame = AxisFrame::new(time).source(source);
        if h != 0.0 {
            frame = frame.value(Axis::Horizontal, h);
        }
        if v != 0.0 {
            frame = frame.value(Axis::Vertical, v);
        }
        if stop {
            frame = frame.stop(Axis::Horizontal).stop(Axis::Vertical);
        }
        if let Some(pointer) = self.seat.get_pointer() {
            pointer.axis(self, frame);
            pointer.frame(self);
        }
    }

    fn is_internal(&self, window: WindowId) -> bool {
        matches!(self.windows.get(&window), Some(Content::Internal { .. }))
    }

    /// Ends finger scrolling of an in-process app.
    fn end_scroll(&mut self) {
        let Some((window, _)) = self.scroll.take() else {
            return;
        };
        let modifiers = self.egui_mods;
        if let Some(events) = self.internal_events(Some(window)) {
            events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::Vec2::ZERO,
                phase: egui::TouchPhase::End,
                modifiers,
            });
        }
    }

    /// Routes a touchpad gesture: the shell may take it at its begin;
    /// otherwise it goes to the chrome or the content under the pointer, and
    /// stays there until it ends.
    fn gesture(&mut self, event: GestureEvent, time: u32) {
        if event.is_begin() {
            self.end_scroll();
            self.gesture_bridge = EguiBridge::default();
            self.gesture = Some(if self.shell.gesture(&event) {
                GestureRoute::Shell
            } else if self.chrome_wants_pointer() {
                GestureRoute::Chrome
            } else {
                match self.content_under() {
                    Some(window) if self.is_internal(window) => GestureRoute::Internal(window),
                    _ => GestureRoute::Client,
                }
            });
        }
        let Some(route) = self.gesture else {
            return;
        };
        if event.is_end() {
            self.gesture = None;
        }
        match route {
            GestureRoute::Shell => {
                if !event.is_begin() {
                    self.shell.gesture(&event);
                }
            }
            GestureRoute::Chrome => {
                let events = self.gesture_bridge.events(event, self.egui_mods);
                self.chrome_events.extend(events);
            }
            GestureRoute::Internal(window) => {
                let events = self.gesture_bridge.events(event, self.egui_mods);
                if let Some(queue) = self.internal_events(Some(window)) {
                    queue.extend(events);
                }
            }
            GestureRoute::Client => self.client_gesture(event, time),
        }
    }

    /// Forwards a gesture to the focused Wayland client
    /// (`zwp_pointer_gestures_v1`).
    fn client_gesture(&mut self, event: GestureEvent, time: u32) {
        let Some(handle) = self.seat.get_pointer() else {
            return;
        };
        let serial = SERIAL_COUNTER.next_serial();
        let delta = |d: egui::Vec2| Point::from((f64::from(d.x), f64::from(d.y)));
        match event {
            GestureEvent::SwipeBegin { fingers } => handle.gesture_swipe_begin(
                self,
                &pointer::GestureSwipeBeginEvent {
                    serial,
                    time,
                    fingers,
                },
            ),
            GestureEvent::SwipeUpdate { delta: d } => handle.gesture_swipe_update(
                self,
                &pointer::GestureSwipeUpdateEvent {
                    time,
                    delta: delta(d),
                },
            ),
            GestureEvent::SwipeEnd { cancelled } => handle.gesture_swipe_end(
                self,
                &pointer::GestureSwipeEndEvent {
                    serial,
                    time,
                    cancelled,
                },
            ),
            GestureEvent::PinchBegin { fingers } => handle.gesture_pinch_begin(
                self,
                &pointer::GesturePinchBeginEvent {
                    serial,
                    time,
                    fingers,
                },
            ),
            GestureEvent::PinchUpdate {
                delta: d,
                scale,
                rotation,
            } => handle.gesture_pinch_update(
                self,
                &pointer::GesturePinchUpdateEvent {
                    time,
                    delta: delta(d),
                    scale: f64::from(scale),
                    rotation: f64::from(rotation.to_degrees()),
                },
            ),
            GestureEvent::PinchEnd { cancelled } => handle.gesture_pinch_end(
                self,
                &pointer::GesturePinchEndEvent {
                    serial,
                    time,
                    cancelled,
                },
            ),
            GestureEvent::HoldBegin { fingers } => handle.gesture_hold_begin(
                self,
                &pointer::GestureHoldBeginEvent {
                    serial,
                    time,
                    fingers,
                },
            ),
            GestureEvent::HoldEnd { cancelled } => handle.gesture_hold_end(
                self,
                &pointer::GestureHoldEndEvent {
                    serial,
                    time,
                    cancelled,
                },
            ),
            _ => {}
        }
    }

    fn surface_under(&self) -> Option<(WlSurface, Point<f64, Logical>)> {
        let (window, loc) = self.space.element_under(self.pointer)?;
        window
            .surface_under(self.pointer - loc.to_f64(), WindowSurfaceType::ALL)
            .map(|(surface, offset)| (surface, (offset + loc).to_f64()))
    }

    fn pointer_motion(&mut self, time: u32) {
        let pos = self.egui_pos();
        self.chrome_events.push(egui::Event::PointerMoved(pos));
        let route = self.route.unwrap_or(if self.chrome_wants_pointer() {
            Route::Chrome
        } else {
            Route::Content(None)
        });
        if route == Route::Shell {
            self.shell.pointer_motion(self.point());
        }
        let mut focus = None;
        let target = match self.route {
            Some(Route::Content(target)) => target,
            None if route == Route::Content(None) => self.content_under(),
            _ => None,
        };
        let target = target.filter(|id| {
            self.shell.placements().iter().any(|p| p.window == *id)
                && matches!(self.windows.get(id), Some(Content::Internal { .. }))
        });
        if self.pointer_target != target {
            if let Some(events) = self.internal_events(self.pointer_target) {
                events.push(egui::Event::PointerGone);
            }
            self.pointer_target = target;
        }
        if let Some(events) = self.internal_events(target) {
            events.push(egui::Event::PointerMoved(pos));
        } else if (route == Route::Content(None)
            || matches!(
                route,
                Route::Content(Some(id))
                    if matches!(self.windows.get(&id), Some(Content::Wayland(_)))
            ))
            && !matches!(
                self.content_under().and_then(|id| self.windows.get(&id)),
                Some(Content::Internal { .. })
            )
        {
            focus = self.surface_under();
        }
        if let Some(pointer) = self.seat.get_pointer() {
            pointer.motion(
                self,
                focus,
                &MotionEvent {
                    location: self.pointer,
                    serial: SERIAL_COUNTER.next_serial(),
                    time,
                },
            );
            pointer.frame(self);
        }
    }

    fn pointer_button(&mut self, button: u32, state: ButtonState, time: u32) {
        let pressed = state == ButtonState::Pressed;
        if pressed {
            self.buttons += 1;
        } else {
            self.buttons = self.buttons.saturating_sub(1);
        }
        let egui_button = match button {
            BTN_LEFT => Some(egui::PointerButton::Primary),
            BTN_RIGHT => Some(egui::PointerButton::Secondary),
            BTN_MIDDLE => Some(egui::PointerButton::Middle),
            _ => None,
        };
        if pressed && self.route.is_none() {
            self.route = Some(if self.chrome_wants_pointer() {
                Route::Chrome
            } else if button == BTN_LEFT {
                match self.shell.pointer_down(self.point(), u64::from(time)) {
                    Press::Handled => Route::Shell,
                    Press::Client => Route::Content(self.content_under()),
                }
            } else {
                if let Some(window) = self.content_under() {
                    self.shell.focus(window);
                }
                Route::Content(self.content_under())
            });
        }
        let pointer_event = egui_button.map(|button| egui::Event::PointerButton {
            pos: self.egui_pos(),
            button,
            pressed,
            modifiers: self.egui_mods,
        });
        match self.route.unwrap_or(Route::Content(None)) {
            Route::Chrome => self.chrome_events.extend(pointer_event),
            Route::Shell => {
                if !pressed && self.buttons == 0 {
                    self.shell.pointer_up();
                }
            }
            Route::Content(target) => {
                let under = if self.route.is_some() {
                    target
                } else {
                    self.content_under()
                };
                if let Some(events) = self.internal_events(under) {
                    events.extend(pointer_event);
                } else if let Some(pointer) = self.seat.get_pointer() {
                    pointer.button(
                        self,
                        &ButtonEvent {
                            button,
                            state,
                            serial: SERIAL_COUNTER.next_serial(),
                            time,
                        },
                    );
                    pointer.frame(self);
                }
            }
        }
        if self.buttons == 0 {
            self.route = None;
            if !pressed {
                self.pointer_motion(time);
            }
        }
    }

    /// Routes a key: shell shortcut, chrome, an in-process app, or the
    /// focused Wayland client.
    fn filter_key(
        &mut self,
        modifiers: &ModifiersState,
        handle: &KeysymHandle<'_>,
        pressed: bool,
        keycode: u32,
    ) -> FilterResult<()> {
        let consumed_release = !pressed && self.consumed_keys.remove(&keycode);
        self.egui_mods = egui::Modifiers {
            alt: modifiers.alt,
            ctrl: modifiers.ctrl,
            shift: modifiers.shift,
            mac_cmd: false,
            command: modifiers.ctrl,
        };
        let modified = handle.modified_sym();
        let sym = handle
            .raw_latin_sym_or_raw_current_sym()
            .unwrap_or(modified);
        let text = modified
            .key_char()
            .filter(|c| !c.is_control() && !modifiers.ctrl && !modifiers.alt && !modifiers.logo);
        let key = KeyInput {
            sym,
            text,
            pressed,
            mods: Modifiers {
                logo: modifiers.logo,
                shift: modifiers.shift,
                ctrl: modifiers.ctrl,
                alt: modifiers.alt,
            },
        };
        let chrome_focus = self.chrome.ctx.memory(|m| m.focused());
        let route = match shell_key_route(&mut self.shell, &key, consumed_release) {
            // A chrome widget focused from the keyboard (Tab) or by a
            // screen reader keeps the keys until Escape hands them back.
            KeyRoute::Client if chrome_focus.is_some() && pressed && sym == Keysym::Escape => {
                if let Some(id) = chrome_focus {
                    self.chrome.ctx.memory_mut(|m| m.surrender_focus(id));
                }
                self.consumed_keys.insert(keycode);
                return FilterResult::Intercept(());
            }
            KeyRoute::Client
                if self.chrome.ctx.egui_wants_keyboard_input() || chrome_focus.is_some() =>
            {
                KeyRoute::Chrome
            }
            route => route,
        };
        let mods = self.egui_mods;
        let events = match route {
            KeyRoute::Consume => {
                if pressed {
                    self.consumed_keys.insert(keycode);
                }
                return FilterResult::Intercept(());
            }
            KeyRoute::Chrome => &mut self.chrome_events,
            KeyRoute::Client => {
                let focused = self.shell.focused();
                match self.internal_events(focused) {
                    Some(events) => events,
                    None => return FilterResult::Forward,
                }
            }
        };
        if let Some(egui_key) = egui_key(sym) {
            events.push(egui::Event::Key {
                key: egui_key,
                physical_key: Some(egui_key),
                pressed,
                repeat: false,
                modifiers: mods,
            });
        }
        if pressed && let Some(c) = text {
            events.push(egui::Event::Text(c.to_string()));
        }
        FilterResult::Intercept(())
    }

    fn timing(&self) -> OutputTiming {
        OutputTiming {
            refresh_mhz: self.refresh_mhz,
            vrr: self.vrr,
        }
    }

    /// Follows the refresh rate of the monitor the session window is on
    /// (checked about once a second; the window can move between monitors)
    /// and advertises it to clients through the output mode.
    fn update_refresh(&mut self) {
        if self
            .timing_checked
            .is_some_and(|t| t.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        self.timing_checked = Some(Instant::now());
        let refresh = self
            .backend
            .window()
            .current_monitor()
            .and_then(|m| m.refresh_rate_millihertz())
            .filter(|&r| r >= 1_000)
            .unwrap_or(60_000);
        if refresh != self.refresh_mhz {
            self.refresh_mhz = refresh;
            if let Some(mode) = self.output.current_mode() {
                let mode = OutputMode {
                    refresh: refresh as i32,
                    ..mode
                };
                self.output
                    .change_current_state(Some(mode), None, None, None);
                self.output.set_preferred(mode);
            }
        }
    }

    /// Draws one frame: background, then per window its decoration and
    /// content, then the chrome and the pointer.
    fn render(&mut self) {
        self.update_refresh();
        self.shell.tick();
        if self
            .scroll
            .is_some_and(|(_, at)| self.now_ms().wrapping_sub(at) > SCROLL_IDLE_MS)
        {
            self.end_scroll();
        }
        self.run_commands();
        self.agent_step();
        self.sync();
        let size = self.backend.window_size();
        let screen =
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size.w as f32, size.h as f32));
        let elapsed = self.now_ms();
        let time = self.start.elapsed().as_secs_f64();

        let input = egui::RawInput {
            screen_rect: Some(screen),
            events: std::mem::take(&mut self.chrome_events),
            time: Some(time),
            focused: true,
            ..Default::default()
        };
        let pointer = self.egui_pos();
        let accent = self.shell.theme().accent;
        let shell = &mut self.shell;
        let mut output = self.chrome.ctx.run_ui(input, |root| {
            shell.chrome(root, elapsed);
            mcsapi_ui::paint_focus_ring(root.ctx(), accent);
            paint_cursor(root.ctx(), pointer);
        });
        self.chrome_access = output.platform_output.accesskit_update.take();
        let chrome = Pass {
            primitives: self
                .chrome
                .ctx
                .tessellate(output.shapes, output.pixels_per_point),
            textures: output.textures_delta,
        };

        let blurs = self.shell.blur_regions();
        let placements = self.shell.placements();
        let background = self.paint_only(screen, time, |shell, painter| {
            shell.paint_background(painter, screen)
        });
        let mut decorations = Vec::with_capacity(placements.len());
        let mut contents = Vec::with_capacity(placements.len());
        for p in &placements {
            decorations.push(self.paint_only(screen, time, |shell, painter| {
                shell.paint_decoration(painter, p)
            }));
            contents.push(self.run_internal(p, time));
        }

        let drawn = self.draw(
            size,
            &placements,
            background,
            decorations,
            contents,
            &blurs,
            chrome,
        );
        let capture = match drawn {
            Ok(capture) => capture.map(Ok),
            Err(e) => {
                eprintln!("mcsapi-compositor: render failed: {e}");
                Some(Err(e.to_string()))
            }
        };
        if let Some(capture) = capture {
            for request in std::mem::take(&mut self.captures) {
                self.shell.captured(request, capture.clone());
            }
        }
        #[cfg(feature = "atspi")]
        if let Some(mut atspi) = self.atspi.take()
            && atspi.update(|| self.build_tree())
        {
            self.atspi = Some(atspi);
        }

        let now = self.start.elapsed();
        for window in self.space.elements() {
            window.send_frame(&self.output, now, Some(Duration::ZERO), |_, _| {
                Some(self.output.clone())
            });
        }
        self.run_commands();
    }

    fn paint_only(
        &mut self,
        screen: egui::Rect,
        time: f64,
        mut paint: impl FnMut(&mut S, &egui::Painter),
    ) -> Pass {
        let input = egui::RawInput {
            screen_rect: Some(screen),
            time: Some(time),
            ..Default::default()
        };
        let shell = &mut self.shell;
        let output = self
            .decorations
            .ctx
            .run_ui(input, |root| paint(shell, root.painter()));
        Pass {
            primitives: self
                .decorations
                .ctx
                .tessellate(output.shapes, output.pixels_per_point),
            textures: output.textures_delta,
        }
    }

    /// Runs an in-process app's frame inside its content area.
    fn run_internal(&mut self, p: &Placement, time: f64) -> Option<Pass> {
        let theme = self.shell.theme();
        let focused = p.focused;
        let Some(Content::Internal {
            instance,
            egui,
            events,
            title,
            access,
            ..
        }) = self.windows.get_mut(&p.window)
        else {
            return None;
        };
        let apps = self.apps.as_mut()?;
        apps.prepare(&egui.ctx, &theme);
        let app = apps.app_mut(*instance)?;
        let g = p.client;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(g.loc.x as f32, g.loc.y as f32),
                egui::vec2(g.size.w as f32, g.size.h as f32),
            )),
            events: std::mem::take(events),
            time: Some(time),
            focused,
            ..Default::default()
        };
        let mut output = mcsapi_ui::run_frame(app, &egui.ctx, input, &theme);
        *access = output.platform_output.accesskit_update.take();
        for command in output.platform_output.commands {
            if let egui::OutputCommand::CopyText(text) = command {
                set_data_device_selection(
                    &self.display,
                    &self.seat,
                    vec![
                        "text/plain;charset=utf-8".into(),
                        "text/plain".into(),
                        "UTF8_STRING".into(),
                    ],
                    text,
                );
            }
        }
        // Fill the whole content area, not only what the app laid out.
        let area = egui::Rect::from_min_size(
            egui::pos2(g.loc.x as f32, g.loc.y as f32),
            egui::vec2(g.size.w as f32, g.size.h as f32),
        );
        output.shapes.insert(
            0,
            egui::epaint::ClippedShape {
                clip_rect: area,
                shape: egui::Shape::rect_filled(area, 0, theme.background),
            },
        );
        let new_title = (app.title() != title.as_str()).then(|| app.title().to_owned());
        let pass = Pass {
            primitives: egui.ctx.tessellate(output.shapes, output.pixels_per_point),
            textures: output.textures_delta,
        };
        if let Some(new_title) = new_title {
            title.clone_from(&new_title);
            self.shell.set_title(p.window, &new_title);
        }
        Some(pass)
    }

    #[allow(clippy::too_many_arguments)] // one per layer of the frame
    fn draw(
        &mut self,
        size: Size<i32, Physical>,
        placements: &[Placement],
        mut background: Pass,
        decorations: Vec<Pass>,
        contents: Vec<Option<Pass>>,
        blurs: &[Blur],
        mut chrome: Pass,
    ) -> std::result::Result<Option<Capture>, Box<dyn std::error::Error>> {
        let scale = Scale::from(1.0);
        let screen_px = [size.w as u32, size.h as u32];
        let full = Rectangle::from_size(size);

        let (renderer, mut framebuffer) = self.backend.bind()?;
        let gl = match &self.gl {
            Some(gl) => gl.clone(),
            None => {
                // SAFETY: the renderer's EGL context is current inside
                // with_context; symbols are resolved through EGL.
                let gl = renderer.with_context(|_| unsafe {
                    glow::Context::from_loader_function(|s| egl::get_proc_address(s))
                })?;
                let gl = Arc::new(gl);
                self.gl = Some(gl.clone());
                gl
            }
        };
        renderer.with_context(|_| {
            for mut painter in self.retired.drain(..) {
                painter.destroy();
            }
        })?;
        let new_painter =
            || egui_glow::Painter::new(gl.clone(), "", None, false).map_err(|e| e.to_string());
        if self.chrome.painter.is_none() {
            self.chrome.painter = Some(new_painter()?);
        }
        if self.decorations.painter.is_none() {
            self.decorations.painter = Some(new_painter()?);
        }
        for content in self.windows.values_mut() {
            if let Content::Internal { egui, .. } = content
                && egui.painter.is_none()
            {
                egui.painter = Some(new_painter()?);
            }
        }

        // Import client buffers before starting the frame.
        let mut surfaces: Vec<Vec<WaylandSurfaceRenderElement<GlesRenderer>>> = Vec::new();
        for p in placements {
            let elements = match self.windows.get(&p.window) {
                Some(Content::Wayland(w)) => self
                    .space
                    .element_location(w)
                    .map(|loc| {
                        w.render_elements(
                            renderer,
                            loc.to_physical_precise_round(scale),
                            scale,
                            1.0,
                        )
                    })
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            surfaces.push(elements);
        }

        let mut frame = renderer.render(&mut framebuffer, size, Transform::Flipped180)?;
        frame.clear(Color32F::new(0.06, 0.09, 0.16, 1.0), &[full])?;
        let deco = self.decorations.painter.as_mut().expect("created above");
        paint(&gl, deco, screen_px, &mut background);
        for (((p, mut decoration), content), elements) in placements
            .iter()
            .zip(decorations)
            .zip(contents)
            .zip(&surfaces)
        {
            let deco = self.decorations.painter.as_mut().expect("created above");
            paint(&gl, deco, screen_px, &mut decoration);
            if let (Some(mut pass), Some(Content::Internal { egui, .. })) =
                (content, self.windows.get_mut(&p.window))
            {
                let painter = egui.painter.as_mut().expect("created above");
                paint(&gl, painter, screen_px, &mut pass);
            }
            // Elements come topmost first.
            for element in elements.iter().rev() {
                let dst = element.geometry(scale);
                element.draw(
                    &mut frame,
                    element.src(),
                    dst,
                    &[Rectangle::from_size(dst.size)],
                    &[],
                )?;
            }
        }
        if !blurs.is_empty() {
            if self.blurrer.is_none() && !self.blur_unavailable {
                // SAFETY: the frame's context is current while it is open.
                match unsafe { blur::Blurrer::new(&gl) } {
                    Ok(b) => self.blurrer = Some(b),
                    Err(e) => {
                        self.blur_unavailable = true;
                        eprintln!("mcsapi-compositor: blur unavailable: {e}");
                    }
                }
            }
            if let Some(blurrer) = &mut self.blurrer {
                // SAFETY: as above, with the frame's framebuffer bound.
                if let Err(e) = unsafe { blurrer.apply(&gl, screen_px, blurs) } {
                    eprintln!("mcsapi-compositor: blur failed: {e}");
                }
                reset_gl(&gl, screen_px);
            }
        }
        let chrome_painter = self.chrome.painter.as_mut().expect("created above");
        paint(&gl, chrome_painter, screen_px, &mut chrome);
        let capture = (!self.captures.is_empty()).then(|| read_frame(&gl, screen_px));
        let _sync = frame.finish()?;
        drop(framebuffer);
        self.backend.submit(Some(&[full]))?;
        Ok(capture)
    }

    /// Starts managing a toplevel on its initial commit, when its app ID and
    /// title are known, so the first configure already has its final size.
    fn manage_on_first_commit(&mut self, surface: &WlSurface) {
        let Some(i) = self
            .unmanaged
            .iter()
            .position(|w| w.toplevel().is_some_and(|t| t.wl_surface() == surface))
        else {
            return;
        };
        let window = self.unmanaged.remove(i);
        let Some(toplevel) = window.toplevel().cloned() else {
            return;
        };
        let (app_id, title) = with_states(surface, |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .and_then(|d| d.lock().ok())
                .map(|d| (d.app_id.clone(), d.title.clone()))
                .unwrap_or_default()
        });
        let id = self.shell.map_window(
            app_id.as_deref().unwrap_or("app"),
            title.as_deref().unwrap_or_default(),
        );
        self.windows.insert(id, Content::Wayland(window));
        self.sync();
        if !toplevel.is_initial_configure_sent() {
            toplevel.send_configure();
        }
        self.run_commands();
    }
}

fn shell_key_route(shell: &mut impl Shell, key: &KeyInput, consumed_release: bool) -> KeyRoute {
    let route = shell.key(key);
    if consumed_release {
        KeyRoute::Consume
    } else {
        route
    }
}

/// Draws the pointer above everything (the nested window hides the host's).
fn paint_cursor(ctx: &egui::Context, at: egui::Pos2) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Debug,
        egui::Id::new("mcsapi-cursor"),
    ));
    let points = [
        (0.0, 0.0),
        (0.0, 17.0),
        (4.5, 13.0),
        (7.5, 19.5),
        (10.0, 18.5),
        (7.0, 12.0),
        (12.5, 12.0),
    ]
    .map(|(x, y)| at + egui::vec2(x, y))
    .to_vec();
    painter.add(egui::Shape::convex_polygon(
        points.clone(),
        egui::Color32::BLACK,
        egui::Stroke::NONE,
    ));
    painter.add(egui::Shape::closed_line(
        points,
        egui::Stroke::new(1.5, egui::Color32::WHITE),
    ));
}

/// Paints an egui pass into the bound framebuffer, then restores the GL
/// state Smithay's renderer relies on.
fn paint(gl: &glow::Context, painter: &mut egui_glow::Painter, screen: [u32; 2], pass: &mut Pass) {
    painter.paint_and_update_textures(screen, 1.0, &pass.primitives, &mut pass.textures);
    reset_gl(gl, screen);
}

/// Restores the GL state Smithay's renderer relies on.
/// Reads the bound framebuffer back as top-to-bottom RGBA rows.
fn read_frame(gl: &glow::Context, [w, h]: [u32; 2]) -> Capture {
    use glow::HasContext as _;
    let row = w as usize * 4;
    let mut rgba = vec![0; row * h as usize];
    // SAFETY: the buffer holds exactly w×h RGBA pixels and the frame's
    // context is current.
    unsafe {
        gl.read_pixels(
            0,
            0,
            w as i32,
            h as i32,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut rgba)),
        );
    }
    // GL rows run bottom to top; the framebuffer's alpha is meaningless.
    let mut flipped = Vec::with_capacity(rgba.len());
    for line in rgba.chunks_exact(row).rev() {
        let (pixels, _) = line.as_chunks::<4>();
        flipped.extend(pixels.iter().flat_map(|p| [p[0], p[1], p[2], 255]));
    }
    Capture {
        width: w,
        height: h,
        rgba: flipped,
    }
}

fn reset_gl(gl: &glow::Context, screen: [u32; 2]) {
    use glow::HasContext as _;
    // SAFETY: plain state resets on the current context.
    unsafe {
        gl.disable(glow::SCISSOR_TEST);
        gl.enable(glow::BLEND);
        gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        gl.use_program(None);
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.viewport(0, 0, screen[0] as i32, screen[1] as i32);
    }
}

fn egui_key(sym: Keysym) -> Option<egui::Key> {
    use egui::Key as K;
    Some(match sym {
        Keysym::BackSpace => K::Backspace,
        Keysym::Return | Keysym::KP_Enter | Keysym::Linefeed => K::Enter,
        Keysym::Tab | Keysym::ISO_Left_Tab => K::Tab,
        Keysym::Escape => K::Escape,
        Keysym::Delete => K::Delete,
        Keysym::Home => K::Home,
        Keysym::End => K::End,
        Keysym::Page_Up => K::PageUp,
        Keysym::Page_Down => K::PageDown,
        Keysym::Left => K::ArrowLeft,
        Keysym::Right => K::ArrowRight,
        Keysym::Up => K::ArrowUp,
        Keysym::Down => K::ArrowDown,
        Keysym::space => K::Space,
        _ => {
            let c = sym.key_char()?.to_ascii_uppercase();
            K::from_name(&c.to_string())?
        }
    })
}

impl<S: Shell> BufferHandler for Host<S> {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl<S: Shell + 'static> CompositorHandler for Host<S> {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client
            .get_data::<ClientState>()
            .expect("every client is inserted with ClientState")
            .compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some((_, window)) = self.wayland_window_of(&root) {
                window.on_commit();
            }
        }
        self.manage_on_first_commit(surface);
        self.popups.commit(surface);
        if let Some(PopupKind::Xdg(popup)) = self.popups.find_popup(surface)
            && !popup.is_initial_configure_sent()
        {
            let _ = popup.send_configure();
        }
    }
}

impl<S: Shell + 'static> XdgShellHandler for Host<S> {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        surface.with_pending_state(|state| {
            state.decoration_mode = Some(DecorationMode::ServerSide);
        });
        self.unmanaged.push(Window::new_wayland_window(surface));
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        self.unmanaged
            .retain(|w| w.toplevel().is_some_and(|t| t != &surface));
        if let Some((id, window)) = self
            .wayland_window_of(surface.wl_surface())
            .map(|(id, w)| (id, w.clone()))
        {
            self.space.unmap_elem(&window);
            self.windows.remove(&id);
            self.shell.unmap_window(id);
            if self.keyboard_focus == Some(id) {
                self.keyboard_focus = None;
                if let Some(keyboard) = self.seat.get_keyboard() {
                    keyboard.set_focus(self, None, SERIAL_COUNTER.next_serial());
                }
            }
        }
    }

    fn title_changed(&mut self, surface: ToplevelSurface) {
        let Some((id, _)) = self.wayland_window_of(surface.wl_surface()) else {
            return;
        };
        let title = with_states(surface.wl_surface(), |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .and_then(|d| d.lock().ok())
                .and_then(|d| d.title.clone())
        });
        self.shell.set_title(id, &title.unwrap_or_default());
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        let _ = self.popups.track_popup(PopupKind::Xdg(surface));
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        surface.send_repositioned(token);
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {
        // Popup grabs are not implemented; popups close when their client
        // dismisses them.
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        if let Some((id, _)) = self.wayland_window_of(surface.wl_surface()) {
            self.shell.client_request(id, ClientRequest::Maximize);
        }
    }

    fn minimize_request(&mut self, surface: ToplevelSurface) {
        if let Some((id, _)) = self.wayland_window_of(surface.wl_surface()) {
            self.shell.client_request(id, ClientRequest::Minimize);
        }
    }
}

impl<S: Shell + 'static> XdgDecorationHandler for Host<S> {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(DecorationMode::ServerSide);
        });
        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, _mode: DecorationMode) {
        // The shell always draws the decorations.
        self.new_decoration(toplevel);
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        self.new_decoration(toplevel);
    }
}

impl<S: Shell> ShmHandler for Host<S> {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

impl<S: Shell + 'static> SeatHandler for Host<S> {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, _image: CursorImageStatus) {}

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let client = focused.and_then(|s| self.display.get_client(s.id()).ok());
        set_data_device_focus(&self.display, seat, client);
    }
}

impl<S: Shell + 'static> SelectionHandler for Host<S> {
    type SelectionUserData = String;

    fn send_selection(
        &mut self,
        _ty: smithay::wayland::selection::SelectionTarget,
        mime_type: String,
        fd: OwnedFd,
        _seat: Seat<Self>,
        user_data: &Self::SelectionUserData,
    ) {
        if matches!(
            mime_type.as_str(),
            "text/plain" | "text/plain;charset=utf-8" | "UTF8_STRING"
        ) {
            let Some(guard) = SelectionTransferGuard::acquire() else {
                return;
            };
            let text = user_data.clone();
            if let Err(error) = std::thread::Builder::new()
                .name("mcsapi-selection-send".into())
                .spawn(move || {
                    let _guard = guard;
                    let _ = write_selection(fd, text.as_bytes(), SELECTION_WRITE_TIMEOUT);
                })
            {
                eprintln!("mcsapi-compositor: cannot start clipboard transfer: {error}");
            }
        }
    }
}

impl<S: Shell + 'static> DataDeviceHandler for Host<S> {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::FromRawFd;

    struct TestShell {
        keys: Vec<bool>,
    }

    impl Shell for TestShell {
        fn map_window(&mut self, _app_id: &str, _title: &str) -> WindowId {
            WindowId::new(1).unwrap()
        }

        fn unmap_window(&mut self, _window: WindowId) {}

        fn set_output(&mut self, _size: (i32, i32)) {}

        fn focused(&self) -> Option<WindowId> {
            None
        }

        fn placements(&self) -> Vec<Placement> {
            Vec::new()
        }

        fn key(&mut self, key: &KeyInput) -> KeyRoute {
            self.keys.push(key.pressed);
            if key.pressed {
                KeyRoute::Consume
            } else {
                KeyRoute::Client
            }
        }
    }

    #[test]
    fn consumed_key_release_reaches_shell_but_not_client() {
        let mut shell = TestShell { keys: Vec::new() };
        let key = |pressed| KeyInput {
            sym: Keysym::Escape,
            text: None,
            pressed,
            mods: Modifiers::default(),
        };

        assert_eq!(
            shell_key_route(&mut shell, &key(true), false),
            KeyRoute::Consume
        );
        assert_eq!(
            shell_key_route(&mut shell, &key(false), true),
            KeyRoute::Consume
        );
        assert_eq!(shell.keys, [true, false]);
    }

    #[test]
    fn clipboard_transfers_are_bounded() {
        let guards: Vec<_> = (0..MAX_SELECTION_TRANSFERS)
            .map(|_| SelectionTransferGuard::acquire().unwrap())
            .collect();
        assert!(SelectionTransferGuard::acquire().is_none());
        drop(guards);
        assert!(SelectionTransferGuard::acquire().is_some());
    }

    #[test]
    fn stalled_clipboard_write_times_out() {
        let mut fds = [0; 2];
        // SAFETY: fds points to two writable integers for pipe to initialize.
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        // SAFETY: pipe initialized both descriptors and ownership is transferred exactly once.
        let _read = unsafe { OwnedFd::from_raw_fd(fds[0]) };
        // SAFETY: pipe initialized both descriptors and ownership is transferred exactly once.
        let write = unsafe { OwnedFd::from_raw_fd(fds[1]) };

        let error =
            write_selection(write, &vec![0; 1024 * 1024], Duration::from_millis(20)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    }
}

impl<S: Shell + 'static> ClientDndGrabHandler for Host<S> {}
impl<S: Shell + 'static> ServerDndGrabHandler for Host<S> {}
impl<S: Shell> OutputHandler for Host<S> {}

delegate_compositor!(@<S: Shell + 'static> Host<S>);
delegate_xdg_shell!(@<S: Shell + 'static> Host<S>);
delegate_xdg_decoration!(@<S: Shell + 'static> Host<S>);
delegate_shm!(@<S: Shell + 'static> Host<S>);
delegate_seat!(@<S: Shell + 'static> Host<S>);
delegate_data_device!(@<S: Shell + 'static> Host<S>);
delegate_pointer_gestures!(@<S: Shell + 'static> Host<S>);
delegate_output!(@<S: Shell + 'static> Host<S>);
