//! The compositor state, Wayland protocol handlers, input routing and
//! rendering.

use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    process::{Child, Command as Process},
    sync::Arc,
    time::{Duration, Instant},
};

use mcsapi::WindowId;
use smithay::{
    backend::{
        egl,
        input::{
            AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputEvent, KeyState,
            KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
        },
        renderer::{
            Color32F, Frame, Renderer,
            element::{
                AsRenderElements, Element, RenderElement, surface::WaylandSurfaceRenderElement,
            },
            gles::GlesRenderer,
            utils::on_commit_buffer_handler,
        },
        winit::{self, WinitEvent, WinitGraphicsBackend, WinitInput},
    },
    delegate_compositor, delegate_data_device, delegate_output, delegate_seat, delegate_shm,
    delegate_xdg_decoration, delegate_xdg_shell,
    desktop::{PopupKind, PopupManager, Space, Window, WindowSurfaceType},
    input::{
        Seat, SeatHandler, SeatState,
        keyboard::{FilterResult, Keysym, KeysymHandle, ModifiersState, XkbConfig},
        pointer::{AxisFrame, ButtonEvent, CursorImageStatus, MotionEvent},
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
        selection::{
            SelectionHandler,
            data_device::{
                ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
                set_data_device_focus,
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
    Apps, Blur, ClientRequest, Command, Compositor, InstanceId, Job, KeyInput, KeyRoute, Modifiers,
    Placement, Press, Shell, blur, egui,
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;

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
    },
}

/// Who receives pointer input until all buttons are released.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Route {
    Chrome,
    Shell,
    Content,
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

    shell: S,
    apps: Option<Box<dyn Apps>>,
    windows: HashMap<WindowId, Content>,
    /// Toplevels that have not committed yet.
    unmanaged: Vec<Window>,
    keyboard_focus: Option<WindowId>,

    chrome: Egui,
    decorations: Egui,
    chrome_events: Vec<egui::Event>,
    pointer: Point<f64, Logical>,
    route: Option<Route>,
    buttons: u32,
    egui_mods: egui::Modifiers,
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
        launch,
        jobs,
    } = config;
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
        shell,
        apps,
        windows: HashMap::new(),
        unmanaged: Vec::new(),
        keyboard_focus: None,
        chrome: Egui::new(),
        decorations: Egui::new(),
        chrome_events: Vec::new(),
        pointer: (0.0, 0.0).into(),
        route: None,
        buttons: 0,
        egui_mods: egui::Modifiers::default(),
        consumed_keys: HashSet::new(),
        children: Vec::new(),
    };
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
    event_loop
        .handle()
        .insert_source(Timer::immediate(), |_, _, host| {
            host.render();
            let interval = host.shell.frame_interval();
            TimeoutAction::ToDuration(
                interval.clamp(Duration::from_millis(4), Duration::from_secs(1)),
            )
        })?;

    event_loop.run(None, &mut host, |host| {
        host.space.refresh();
        host.popups.cleanup();
        let _ = host.display.flush_clients();
    })?;
    for child in &mut host.children {
        let _ = child.kill();
    }
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
                            egui: Egui::new(),
                            events: Vec::new(),
                            title,
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
                    refresh: 60_000,
                };
                self.output
                    .change_current_state(Some(mode), None, None, None);
                self.output.set_preferred(mode);
                self.shell.set_output((size.w, size.h));
            }
            WinitEvent::Input(event) => self.input(event),
            WinitEvent::CloseRequested => self.signal.stop(),
            WinitEvent::Focus(_) | WinitEvent::Redraw => {}
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

    fn input(&mut self, event: InputEvent<WinitInput>) {
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
            InputEvent::PointerAxis { event } => {
                let amount = |axis| {
                    event
                        .amount(axis)
                        .or_else(|| event.amount_v120(axis).map(|v| v * 15.0 / 120.0))
                        .unwrap_or(0.0)
                };
                let (h, v) = (amount(Axis::Horizontal), amount(Axis::Vertical));
                let wheel = egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(-h as f32, -v as f32),
                    phase: egui::TouchPhase::Move,
                    modifiers: self.egui_mods,
                };
                if self.chrome_wants_pointer() {
                    self.chrome_events.push(wheel);
                    return;
                }
                let under = self.content_under();
                if let Some(events) = self.internal_events(under) {
                    events.push(wheel);
                    return;
                }
                let mut frame = AxisFrame::new(Event::time_msec(&event)).source(AxisSource::Wheel);
                if h != 0.0 {
                    frame = frame.value(Axis::Horizontal, h);
                }
                if v != 0.0 {
                    frame = frame.value(Axis::Vertical, v);
                }
                if let Some(pointer) = self.seat.get_pointer() {
                    pointer.axis(self, frame);
                    pointer.frame(self);
                }
            }
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
            Route::Content
        });
        if route == Route::Shell {
            self.shell.pointer_motion(self.point());
        }
        let mut focus = None;
        if route == Route::Content {
            let under = self.content_under();
            for (id, content) in &mut self.windows {
                if let Content::Internal { events, .. } = content {
                    events.push(if Some(*id) == under {
                        egui::Event::PointerMoved(pos)
                    } else {
                        egui::Event::PointerGone
                    });
                }
            }
            if !matches!(
                under.and_then(|w| self.windows.get(&w)),
                Some(Content::Internal { .. })
            ) {
                focus = self.surface_under();
            }
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
                    Press::Client => Route::Content,
                }
            } else {
                if let Some(window) = self.content_under() {
                    self.shell.focus(window);
                }
                Route::Content
            });
        }
        let pointer_event = egui_button.map(|button| egui::Event::PointerButton {
            pos: self.egui_pos(),
            button,
            pressed,
            modifiers: self.egui_mods,
        });
        match self.route.unwrap_or(Route::Content) {
            Route::Chrome => self.chrome_events.extend(pointer_event),
            Route::Shell => {
                if !pressed {
                    self.shell.pointer_up();
                }
            }
            Route::Content => {
                let under = self.content_under();
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
        if !pressed && self.consumed_keys.remove(&keycode) {
            return FilterResult::Intercept(());
        }
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
        let route = match self.shell.key(&key) {
            KeyRoute::Client if self.chrome.ctx.egui_wants_keyboard_input() => KeyRoute::Chrome,
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

    /// Draws one frame: background, then per window its decoration and
    /// content, then the chrome and the pointer.
    fn render(&mut self) {
        self.shell.tick();
        self.run_commands();
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
        let shell = &mut self.shell;
        let output = self.chrome.ctx.run_ui(input, |root| {
            shell.chrome(root, elapsed);
            paint_cursor(root.ctx(), pointer);
        });
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

        if let Err(e) = self.draw(
            size,
            &placements,
            background,
            decorations,
            contents,
            &blurs,
            chrome,
        ) {
            eprintln!("mcsapi-compositor: render failed: {e}");
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
    ) -> Result {
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
            if self.blurrer.is_none() {
                // SAFETY: the frame's context is current while it is open.
                match unsafe { blur::Blurrer::new(&gl) } {
                    Ok(b) => self.blurrer = Some(b),
                    Err(e) => eprintln!("mcsapi-compositor: blur unavailable: {e}"),
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
        let _sync = frame.finish()?;
        drop(framebuffer);
        self.backend.submit(Some(&[full]))?;
        Ok(())
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
    type SelectionUserData = ();
}

impl<S: Shell + 'static> DataDeviceHandler for Host<S> {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
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
delegate_output!(@<S: Shell + 'static> Host<S>);
