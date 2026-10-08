//! Input protocols beyond the core seat.
//!
//! - `zwp_relative_pointer_manager_v1`: raw mouse motion for games and 3D
//!   viewports, also while the pointer is locked.
//! - `zwp_pointer_constraints_v1`: lock the pointer in place or confine it
//!   to a region of the surface under it. A constraint becomes active when
//!   the pointer is over its surface (and inside its region), and Smithay
//!   ends it when the surface loses the pointer.
//! - `wp_pointer_warp_v1`: move the pointer within the surface that has it.
//! - `zwp_keyboard_shortcuts_inhibit_manager_v1`: a remote desktop or VM
//!   viewer gets the keys the shell would otherwise take, while it has the
//!   keyboard and [`Shell::inhibit_shortcuts`] allows it.
//! - `ext_idle_notifier_v1` and `zwp_idle_inhibit_manager_v1`: idle daemons
//!   learn when the user stops using the seat, and a visible surface (a
//!   playing video) can hold that off; the shell hears the latter through
//!   [`Shell::idle_inhibited`].
//! - `zwp_tablet_manager_v2`: pens and their tablets on the bare seat
//!   (libinput). Over a client's surface a pen speaks the tablet protocol;
//!   over the chrome or an in-process app it moves and clicks like the
//!   pointer, since egui knows no pens.

use smithay::{
    backend::input::{
        AbsolutePositionEvent, ButtonState, Device, DeviceCapability, Event, InputBackend,
        ProximityState, TabletToolButtonEvent, TabletToolEvent, TabletToolProximityEvent,
        TabletToolTipEvent, TabletToolTipState,
    },
    delegate_idle_inhibit, delegate_idle_notify, delegate_keyboard_shortcuts_inhibit,
    delegate_pointer_constraints, delegate_relative_pointer, delegate_tablet_manager,
    input::pointer::{PointerHandle, RelativeMotionEvent},
    reexports::{
        calloop::LoopHandle,
        wayland_protocols::wp::pointer_warp::v1::server::wp_pointer_warp_v1::{
            self, WpPointerWarpV1,
        },
        wayland_server::{
            Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
            protocol::wl_surface::WlSurface,
        },
    },
    utils::{Logical, Point, SERIAL_COUNTER, Serial},
    wayland::{
        compositor::get_parent,
        idle_inhibit::{IdleInhibitHandler, IdleInhibitManagerState},
        idle_notify::{IdleNotifierHandler, IdleNotifierState},
        keyboard_shortcuts_inhibit::{
            KeyboardShortcutsInhibitHandler, KeyboardShortcutsInhibitState,
            KeyboardShortcutsInhibitor, KeyboardShortcutsInhibitorSeat,
        },
        pointer_constraints::{
            PointerConstraint, PointerConstraintsHandler, PointerConstraintsState,
            with_pointer_constraint,
        },
        relative_pointer::RelativePointerManagerState,
        seat::WaylandFocus,
        tablet_manager::{TabletDescriptor, TabletManagerState, TabletSeatTrait},
    },
};

use super::{BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, Host};
use crate::Shell;

/// The globals and the state they need.
pub(super) struct Inputs<S: Shell + 'static> {
    _relative_pointer: RelativePointerManagerState,
    _pointer_constraints: PointerConstraintsState,
    shortcuts_inhibit: KeyboardShortcutsInhibitState,
    idle: IdleNotifierState<Host<S>>,
    _idle_inhibit: IdleInhibitManagerState,
    _tablets: TabletManagerState,
    /// Surfaces asking for the session to stay awake while visible.
    inhibitors: Vec<WlSurface>,
    /// What the shell was last told about them.
    idle_inhibited: bool,
    /// Whether a pen is over a client surface, so its tip and buttons speak
    /// the tablet protocol rather than move the pointer.
    pen_on_client: bool,
    /// Where the last absolute motion put the host's pointer (nested, the
    /// session window's own pointer). Deltas are taken from it rather than
    /// from the session's pointer, which stays put while a client has it
    /// locked.
    pub(super) absolute: Option<Point<f64, Logical>>,
}

impl<S: Shell + 'static> Inputs<S> {
    pub(super) fn new(dh: &DisplayHandle, handle: LoopHandle<'static, Host<S>>) -> Self {
        dh.create_global::<Host<S>, WpPointerWarpV1, ()>(1, ());
        Self {
            _relative_pointer: RelativePointerManagerState::new::<Host<S>>(dh),
            _pointer_constraints: PointerConstraintsState::new::<Host<S>>(dh),
            shortcuts_inhibit: KeyboardShortcutsInhibitState::new::<Host<S>>(dh),
            idle: IdleNotifierState::new(dh, handle),
            _idle_inhibit: IdleInhibitManagerState::new::<Host<S>>(dh),
            _tablets: TabletManagerState::new::<Host<S>>(dh),
            inhibitors: Vec::new(),
            idle_inhibited: false,
            pen_on_client: false,
            absolute: None,
        }
    }
}

impl<S: Shell + 'static> Host<S> {
    /// Input from the seat: the user is here.
    pub(super) fn seat_activity(&mut self) {
        self.inputs.idle.notify_activity(&self.seat);
    }

    /// Whether the focused client holds the keyboard's shortcuts.
    pub(super) fn shortcuts_inhibited(&self) -> bool {
        self.seat.keyboard_shortcuts_inhibited()
    }

    /// Recomputes whether a visible surface inhibits idling.
    pub(super) fn update_idle_inhibit(&mut self) {
        self.inputs.inhibitors.retain(|s| s.is_alive());
        let visible = self.inputs.inhibitors.iter().any(|surface| {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            self.space
                .elements()
                .any(|w| w.wl_surface().is_some_and(|s| *s == root))
        });
        self.inputs.idle.set_is_inhibited(visible);
        if visible != self.inputs.idle_inhibited {
            self.inputs.idle_inhibited = visible;
            self.shell.idle_inhibited(visible);
        }
    }

    /// Moves the pointer by a relative motion from the seat (or the delta
    /// between two absolute positions nested), honouring a lock or
    /// confinement on the surface under it, and reports the raw motion to
    /// relative-pointer clients. Returns where the pointer goes.
    pub(super) fn relative_motion(
        &mut self,
        to: Point<f64, Logical>,
        delta: Point<f64, Logical>,
        delta_unaccel: Point<f64, Logical>,
        utime: u64,
    ) -> Point<f64, Logical> {
        let Some(pointer) = self.seat.get_pointer() else {
            return to;
        };
        let focus = self.surface_under_pointer_focus(&pointer);
        pointer.relative_motion(
            self,
            focus.clone(),
            &RelativeMotionEvent {
                delta,
                delta_unaccel,
                utime,
            },
        );
        let Some((surface, origin)) = focus else {
            return to;
        };
        let from = self.pointer;
        with_pointer_constraint(&surface, &pointer, |constraint| {
            let Some(constraint) = constraint.filter(|c| c.is_active()) else {
                return to;
            };
            match &*constraint {
                PointerConstraint::Locked(_) => from,
                // Outside the region the pointer stays where it was, as
                // weston and wlroots do; the region is usually a rectangle
                // the client draws its cursor in.
                PointerConstraint::Confined(_) => {
                    let local = (to - origin).to_i32_round();
                    let inside = match constraint.region() {
                        Some(region) => region.contains(local),
                        None => surface_contains(&surface, local),
                    };
                    if inside { to } else { from }
                }
            }
        })
    }

    /// The pointer's focus surface and its origin, if the pointer is on a
    /// client.
    fn surface_under_pointer_focus(
        &self,
        pointer: &PointerHandle<Self>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        let focus = pointer.current_focus()?;
        self.surface_under().filter(|(s, _)| *s == focus)
    }

    /// Activates a constraint on the surface under the pointer once the
    /// pointer is inside its region.
    pub(super) fn activate_constraint(&mut self) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let Some((surface, origin)) = self.surface_under_pointer_focus(&pointer) else {
            return;
        };
        let local = (self.pointer - origin).to_i32_round();
        with_pointer_constraint(&surface, &pointer, |constraint| {
            if let Some(constraint) = constraint
                && !constraint.is_active()
                && constraint.region().is_none_or(|r| r.contains(local))
            {
                constraint.activate();
            }
        });
    }

    /// A pen on the bare seat.
    pub(super) fn tablet_event<B: InputBackend>(&mut self, event: TabletInput<B>) {
        let size = self.backend.size();
        let tablet_seat = self.seat.tablet_seat();
        let dh = self.display.clone();
        match event {
            TabletInput::Added(device) => {
                if device.has_capability(DeviceCapability::TabletTool) {
                    tablet_seat.add_tablet::<Self>(&dh, &TabletDescriptor::from(&device));
                }
            }
            TabletInput::Removed(device) => {
                if device.has_capability(DeviceCapability::TabletTool) {
                    tablet_seat.remove_tablet(&TabletDescriptor::from(&device));
                    if tablet_seat.count_tablets() == 0 {
                        tablet_seat.clear_tools();
                    }
                }
            }
            TabletInput::Axis(event) => {
                self.pointer = event.position_transformed((size.w, size.h).into());
                self.pen_moved(&event, event.time_msec());
            }
            TabletInput::Proximity(event) => {
                self.pointer = event.position_transformed((size.w, size.h).into());
                let tool = tablet_seat.add_tool::<Self>(self, &dh, &event.tool());
                let tablet = tablet_seat.get_tablet(&TabletDescriptor::from(&event.device()));
                match (event.state(), self.surface_under(), tablet) {
                    (ProximityState::In, Some(focus), Some(tablet))
                        if self.pen_targets_client() =>
                    {
                        self.inputs.pen_on_client = true;
                        tool.proximity_in(
                            self.pointer,
                            focus,
                            &tablet,
                            SERIAL_COUNTER.next_serial(),
                            event.time_msec(),
                        );
                    }
                    (ProximityState::Out, ..) => {
                        if self.inputs.pen_on_client {
                            tool.proximity_out(event.time_msec());
                        }
                        self.inputs.pen_on_client = false;
                    }
                    _ => self.pointer_motion(event.time_msec()),
                }
            }
            TabletInput::Tip(event) => {
                let pressed = event.tip_state() == TabletToolTipState::Down;
                let time = event.time_msec();
                if self.inputs.pen_on_client {
                    if let Some(tool) = tablet_seat.get_tool(&event.tool()) {
                        if pressed {
                            tool.tip_down(SERIAL_COUNTER.next_serial(), time);
                        } else {
                            tool.tip_up(time);
                        }
                    }
                    // Clicking into a window with a pen focuses it, as a
                    // click does.
                    if pressed && let Some(window) = self.content_under() {
                        self.shell.focus(window);
                    }
                } else {
                    let state = if pressed {
                        ButtonState::Pressed
                    } else {
                        ButtonState::Released
                    };
                    self.pointer_button(BTN_LEFT, state, time);
                }
            }
            TabletInput::Button(event) => {
                let time = event.time_msec();
                if self.inputs.pen_on_client {
                    if let Some(tool) = tablet_seat.get_tool(&event.tool()) {
                        tool.button(
                            event.button(),
                            event.button_state(),
                            SERIAL_COUNTER.next_serial(),
                            time,
                        );
                    }
                } else {
                    // The pen's lower and upper buttons click as right and
                    // middle (BTN_STYLUS, BTN_STYLUS2).
                    let button = match event.button() {
                        0x14b => BTN_RIGHT,
                        0x14c => BTN_MIDDLE,
                        other => other,
                    };
                    self.pointer_button(button, event.button_state(), time);
                }
            }
        }
    }

    /// Whether a pen at the pointer is over a client surface rather than
    /// the chrome, an in-process app or a shell press in progress.
    fn pen_targets_client(&self) -> bool {
        self.route.is_none()
            && (self.pointer_on_layer()
                || (!self.chrome_wants_pointer()
                    && self.content_under().is_some_and(|w| !self.is_internal(w))))
    }

    fn pen_moved<B: InputBackend>(&mut self, event: &impl TabletToolEvent<B>, time: u32) {
        let tablet_seat = self.seat.tablet_seat();
        let tool = tablet_seat.get_tool(&event.tool());
        let tablet = tablet_seat.get_tablet(&TabletDescriptor::from(&event.device()));
        let on_client = self.pen_targets_client() || self.inputs.pen_on_client;
        let (Some(tool), Some(tablet), true) = (tool, tablet, on_client) else {
            self.pointer_motion(time);
            return;
        };
        if event.pressure_has_changed() {
            tool.pressure(event.pressure());
        }
        if event.distance_has_changed() {
            tool.distance(event.distance());
        }
        if event.tilt_has_changed() {
            tool.tilt(event.tilt());
        }
        if event.slider_has_changed() {
            tool.slider_position(event.slider_position());
        }
        if event.rotation_has_changed() {
            tool.rotation(event.rotation());
        }
        if event.wheel_has_changed() {
            tool.wheel(event.wheel_delta(), event.wheel_delta_discrete());
        }
        let focus = self.surface_under();
        self.inputs.pen_on_client = focus.is_some();
        tool.motion(
            self.pointer,
            focus,
            &tablet,
            SERIAL_COUNTER.next_serial(),
            time,
        );
    }
}

/// The tablet events [`Host::tablet_event`] takes, from any input backend.
pub(super) enum TabletInput<B: InputBackend> {
    Added(B::Device),
    Removed(B::Device),
    Axis(B::TabletToolAxisEvent),
    Proximity(B::TabletToolProximityEvent),
    Tip(B::TabletToolTipEvent),
    Button(B::TabletToolButtonEvent),
}

/// Whether a surface-local point lies on the surface's current buffer.
fn surface_contains(surface: &WlSurface, local: Point<i32, Logical>) -> bool {
    use smithay::backend::renderer::utils::RendererSurfaceStateUserData;
    smithay::wayland::compositor::with_states(surface, |states| {
        states
            .data_map
            .get::<RendererSurfaceStateUserData>()
            .and_then(|d| d.lock().ok()?.surface_size())
            .is_some_and(|size| {
                local.x >= 0 && local.y >= 0 && local.x < size.w && local.y < size.h
            })
    })
}

impl<S: Shell + 'static> PointerConstraintsHandler for Host<S> {
    fn new_constraint(&mut self, _surface: &WlSurface, _pointer: &PointerHandle<Self>) {
        self.activate_constraint();
    }

    /// Where a locked pointer should appear when the lock ends; the
    /// compositor draws its cursor at the pointer, so it moves there.
    fn cursor_position_hint(
        &mut self,
        surface: &WlSurface,
        pointer: &PointerHandle<Self>,
        location: Point<f64, Logical>,
    ) {
        let active =
            with_pointer_constraint(surface, pointer, |c| c.is_some_and(|c| c.is_active()));
        if !active {
            return;
        }
        if let Some((_, origin)) = self.surface_under_pointer_focus(pointer) {
            self.pointer = origin + location;
            pointer.set_location(self.pointer);
        }
    }
}

impl<S: Shell + 'static> KeyboardShortcutsInhibitHandler for Host<S> {
    fn keyboard_shortcuts_inhibit_state(&mut self) -> &mut KeyboardShortcutsInhibitState {
        &mut self.inputs.shortcuts_inhibit
    }

    fn new_inhibitor(&mut self, inhibitor: KeyboardShortcutsInhibitor) {
        let window = self
            .wayland_window_of(inhibitor.wl_surface())
            .map(|(id, _)| id);
        if window.is_some_and(|w| self.shell.inhibit_shortcuts(w)) {
            inhibitor.activate();
        }
    }
}

impl<S: Shell + 'static> IdleNotifierHandler for Host<S> {
    fn idle_notifier_state(&mut self) -> &mut IdleNotifierState<Self> {
        &mut self.inputs.idle
    }
}

impl<S: Shell + 'static> IdleInhibitHandler for Host<S> {
    fn inhibit(&mut self, surface: WlSurface) {
        if !self.inputs.inhibitors.contains(&surface) {
            self.inputs.inhibitors.push(surface);
        }
        self.update_idle_inhibit();
    }

    fn uninhibit(&mut self, surface: WlSurface) {
        self.inputs.inhibitors.retain(|s| *s != surface);
        self.update_idle_inhibit();
    }
}

impl<S: Shell + 'static> GlobalDispatch<WpPointerWarpV1, ()> for Host<S> {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<WpPointerWarpV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl<S: Shell + 'static> Dispatch<WpPointerWarpV1, ()> for Host<S> {
    fn request(
        host: &mut Self,
        _client: &Client,
        _object: &WpPointerWarpV1,
        request: wp_pointer_warp_v1::Request,
        _data: &(),
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        let wp_pointer_warp_v1::Request::WarpPointer {
            surface,
            x,
            y,
            serial,
            ..
        } = request
        else {
            return;
        };
        // Only within the surface that has the pointer, answering the enter
        // it got: a client cannot pull the pointer onto itself.
        let Some(pointer) = host.seat.get_pointer() else {
            return;
        };
        if pointer.last_enter() != Some(Serial::from(serial)) {
            return;
        }
        let Some((_, origin)) = host
            .surface_under_pointer_focus(&pointer)
            .filter(|(s, _)| *s == surface)
        else {
            return;
        };
        let local = Point::<f64, Logical>::from((x, y));
        if !surface_contains(&surface, local.to_i32_round()) {
            return;
        }
        host.pointer = origin + local;
        let time = host.now_ms();
        host.pointer_motion(time);
    }
}

delegate_relative_pointer!(@<S: Shell + 'static> Host<S>);
delegate_pointer_constraints!(@<S: Shell + 'static> Host<S>);
delegate_keyboard_shortcuts_inhibit!(@<S: Shell + 'static> Host<S>);
delegate_idle_notify!(@<S: Shell + 'static> Host<S>);
delegate_idle_inhibit!(@<S: Shell + 'static> Host<S>);
delegate_tablet_manager!(@<S: Shell + 'static> Host<S>);
