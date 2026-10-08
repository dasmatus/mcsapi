//! Globals for tools that drive the session from outside its windows, all
//! hidden from sandboxed clients (see `security`):
//!
//! - `ext_data_control_manager_v1` and `zwlr_data_control_manager_v1`: the
//!   clipboard and primary selection without a focused window (wl-copy and
//!   wl-paste, clipboard managers).
//! - `zwp_virtual_keyboard_manager_v1`: keys with the tool's own keymap
//!   (wtype, on-screen keyboards). Smithay sends them straight to the
//!   focused client, so they reach neither the shell's shortcuts nor an
//!   in-process app.
//! - `zwlr_virtual_pointer_manager_v1`: pointer motion, buttons and scrolling
//!   (wayvnc, ydotool-like tools). These take the same path as a real mouse,
//!   so they reach the chrome, in-process apps and clients alike, and the
//!   shell is told the input is synthetic (`Shell::input_source`), as with
//!   `Command::Input`.

use std::sync::Mutex;

use smithay::{
    backend::input::{AxisSource, ButtonState},
    delegate_data_control, delegate_ext_data_control, delegate_virtual_keyboard_manager,
    reexports::{
        wayland_protocols_wlr::virtual_pointer::v1::server::{
            zwlr_virtual_pointer_manager_v1::{self, ZwlrVirtualPointerManagerV1},
            zwlr_virtual_pointer_v1::{self, ZwlrVirtualPointerV1},
        },
        wayland_server::{
            Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New,
            protocol::wl_pointer::{self, Axis},
        },
    },
    utils::{Logical, Point},
    wayland::{
        selection::{
            ext_data_control::{
                DataControlHandler as ExtDataControlHandler,
                DataControlState as ExtDataControlState,
            },
            primary_selection::PrimarySelectionState,
            wlr_data_control::{DataControlHandler, DataControlState},
        },
        virtual_keyboard::VirtualKeyboardManagerState,
    },
};

use super::{Host, security::unsandboxed};
use crate::Shell;

pub(super) struct Automation {
    wlr_data_control: DataControlState,
    ext_data_control: ExtDataControlState,
    _virtual_keyboard: VirtualKeyboardManagerState,
}

impl Automation {
    pub(super) fn new<S: Shell + 'static>(
        dh: &DisplayHandle,
        primary: &PrimarySelectionState,
    ) -> Self {
        dh.create_global::<Host<S>, ZwlrVirtualPointerManagerV1, ()>(2, ());
        Self {
            wlr_data_control: DataControlState::new::<Host<S>, _>(dh, Some(primary), unsandboxed),
            ext_data_control: ExtDataControlState::new::<Host<S>, _>(
                dh,
                Some(primary),
                unsandboxed,
            ),
            _virtual_keyboard: VirtualKeyboardManagerState::new::<Host<S>, _>(dh, unsandboxed),
        }
    }
}

impl<S: Shell + 'static> DataControlHandler for Host<S> {
    fn data_control_state(&self) -> &DataControlState {
        &self.automation.wlr_data_control
    }
}

impl<S: Shell + 'static> ExtDataControlHandler for Host<S> {
    fn data_control_state(&self) -> &ExtDataControlState {
        &self.automation.ext_data_control
    }
}

/// A virtual pointer's scroll source, which applies to the axis events that
/// follow it in the same frame.
#[derive(Default)]
pub(super) struct VirtualPointer(Mutex<Option<AxisSource>>);

impl<S: Shell + 'static> GlobalDispatch<ZwlrVirtualPointerManagerV1, ()> for Host<S> {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrVirtualPointerManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }

    fn can_view(client: Client, _global_data: &()) -> bool {
        unsandboxed(&client)
    }
}

impl<S: Shell + 'static> Dispatch<ZwlrVirtualPointerManagerV1, ()> for Host<S> {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _manager: &ZwlrVirtualPointerManagerV1,
        request: zwlr_virtual_pointer_manager_v1::Request,
        _data: &(),
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        // There is one seat and one output, whichever the tool names.
        match request {
            zwlr_virtual_pointer_manager_v1::Request::CreateVirtualPointer { id, .. }
            | zwlr_virtual_pointer_manager_v1::Request::CreateVirtualPointerWithOutput {
                id, ..
            } => {
                data_init.init(id, VirtualPointer::default());
            }
            zwlr_virtual_pointer_manager_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl<S: Shell + 'static> Dispatch<ZwlrVirtualPointerV1, VirtualPointer> for Host<S> {
    fn request(
        host: &mut Self,
        _client: &Client,
        _pointer: &ZwlrVirtualPointerV1,
        request: zwlr_virtual_pointer_v1::Request,
        data: &VirtualPointer,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        use zwlr_virtual_pointer_v1::Request;
        let source = || {
            data.0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .unwrap_or(AxisSource::Wheel)
        };
        let delta = |axis: Axis, value: f64| match axis {
            Axis::HorizontalScroll => (value, 0.0),
            _ => (0.0, value),
        };
        host.set_synthetic(true);
        host.seat_activity();
        match request {
            Request::Motion { time, dx, dy } => {
                let delta = Point::<f64, Logical>::from((dx, dy));
                host.virtual_motion(host.pointer + delta, delta, time);
            }
            Request::MotionAbsolute {
                time,
                x,
                y,
                x_extent,
                y_extent,
            } => {
                if x_extent == 0 || y_extent == 0 {
                    return;
                }
                let size = host.backend.size();
                let to = Point::<f64, Logical>::from((
                    f64::from(x) / f64::from(x_extent) * f64::from(size.w),
                    f64::from(y) / f64::from(y_extent) * f64::from(size.h),
                ));
                host.virtual_motion(to, to - host.pointer, time);
            }
            Request::Button {
                time,
                button,
                state,
            } => {
                let state = match state.into_result() {
                    Ok(wl_pointer::ButtonState::Pressed) => ButtonState::Pressed,
                    _ => ButtonState::Released,
                };
                host.pointer_button(button, state, time);
            }
            Request::Axis { time, axis, value }
            | Request::AxisDiscrete {
                time, axis, value, ..
            } => {
                let Ok(axis) = axis.into_result() else {
                    return;
                };
                let (h, v) = delta(axis, value);
                host.axis(h, v, source(), time);
            }
            Request::AxisSource { axis_source } => {
                let source = match axis_source.into_result() {
                    Ok(wl_pointer::AxisSource::Finger) => AxisSource::Finger,
                    Ok(wl_pointer::AxisSource::Continuous) => AxisSource::Continuous,
                    Ok(wl_pointer::AxisSource::WheelTilt) => AxisSource::WheelTilt,
                    _ => AxisSource::Wheel,
                };
                *data.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(source);
            }
            // Ends finger scrolling, which the host reads from an empty
            // scroll.
            Request::AxisStop { time, .. } => {
                host.axis(0.0, 0.0, source(), time);
            }
            // Each event above is delivered as it comes, with its own frame.
            Request::Frame => {
                *data.0.lock().unwrap_or_else(|e| e.into_inner()) = None;
            }
            Request::Destroy => {}
            _ => {}
        }
    }
}

impl<S: Shell + 'static> Host<S> {
    /// Moves the pointer for a virtual pointer, kept on the output, as a
    /// mouse on the bare seat does.
    fn virtual_motion(&mut self, to: Point<f64, Logical>, delta: Point<f64, Logical>, time: u32) {
        let size = self.backend.size();
        let to = Point::from((
            to.x.clamp(0.0, f64::from(size.w - 1)),
            to.y.clamp(0.0, f64::from(size.h - 1)),
        ));
        self.pointer = self.relative_motion(to, delta, delta, u64::from(time) * 1000);
        self.pointer_motion(time);
        self.activate_constraint();
    }
}

delegate_data_control!(@<S: Shell + 'static> Host<S>);
delegate_ext_data_control!(@<S: Shell + 'static> Host<S>);
delegate_virtual_keyboard_manager!(@<S: Shell + 'static> Host<S>);
