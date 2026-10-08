//! `zwp_input_method_v2`: an input method client (fcitx5 for CJK and
//! compose, squeekboard as a phone keyboard) edits the focused text field
//! through `zwp_text_input_v3` (see `text_input`).
//!
//! One input method is connected at a time; a second gets `unavailable`.
//! It is activated while a field on the keyboard's surface is enabled and
//! told that field's surrounding text, content type and why it changed; its
//! preedit, commits and deletions go back to the field when it commits. A
//! keyboard grab gets the seat's keys while it is active, after the shell's
//! shortcuts, and the input method sends back what it does not use through
//! its virtual keyboard. Its popup surface (a candidate list) is drawn
//! under the field's cursor, flipped above when there is no room below,
//! and takes the pointer like a layer surface. Sandboxed clients do not see
//! the global, since an input method sees everything typed.

use std::{collections::HashSet, sync::Mutex};

use smithay::{
    backend::renderer::{
        element::{
            Kind,
            surface::{WaylandSurfaceRenderElement, render_elements_from_surface_tree},
        },
        gles::GlesRenderer,
    },
    desktop::{
        WindowSurfaceType, layer_map_for_output,
        utils::{bbox_from_surface_tree, send_frames_surface_tree, under_from_surface_tree},
    },
    input::keyboard::{KeymapFile, SerializedMods},
    reexports::{
        wayland_protocols::wp::text_input::zv3::server::zwp_text_input_v3::ZwpTextInputV3,
        wayland_protocols_misc::zwp_input_method_v2::server::{
            zwp_input_method_keyboard_grab_v2::{self, ZwpInputMethodKeyboardGrabV2},
            zwp_input_method_manager_v2::{self, ZwpInputMethodManagerV2},
            zwp_input_method_v2::{self, ZwpInputMethodV2},
            zwp_input_popup_surface_v2::{self, ZwpInputPopupSurfaceV2},
        },
        wayland_server::{
            Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
            backend::ClientId,
            protocol::{
                wl_keyboard::{KeyState, KeymapFormat},
                wl_surface::WlSurface,
            },
        },
    },
    utils::{Logical, Point, Rectangle, SERIAL_COUNTER, Scale},
    wayland::compositor::give_role,
};
use tracing::warn;

use super::{Host, security::unsandboxed};
use crate::{
    Shell,
    text_input::{Edit, Field},
};

/// The role a popup surface takes.
const POPUP_ROLE: &str = "zwp_input_popup_surface_v2";

/// The seat's key repeat, as `add_keyboard` sets it: 30 a second after
/// 400 ms.
const REPEAT: (i32, i32) = (30, 400);

#[derive(Default)]
pub(super) struct InputMethod {
    im: Option<Connected>,
    /// Keys pressed into the grab, whose releases go there too even if it
    /// has gone inactive since; the client never saw their presses.
    grabbed: HashSet<u32>,
}

/// The connected input method.
struct Connected {
    object: ZwpInputMethodV2,
    /// The field it is active for.
    active: Option<ZwpTextInputV3>,
    grab: Option<ZwpInputMethodKeyboardGrabV2>,
    /// The modifiers the grab was last told.
    mods: Option<SerializedMods>,
    popups: Vec<Popup>,
}

struct Popup {
    object: ZwpInputPopupSurfaceV2,
    surface: WlSurface,
    /// Where it is drawn, while its input method is active.
    at: Option<Point<i32, Logical>>,
    /// The field's cursor relative to it, as last sent.
    sent: Option<Rectangle<i32, Logical>>,
}

impl InputMethod {
    pub(super) fn new<S: Shell + 'static>(dh: &DisplayHandle) -> Self {
        dh.create_global::<Host<S>, ZwpInputMethodManagerV2, ()>(1, ());
        Self::default()
    }

    /// The popups being shown, with where.
    fn shown(&self) -> impl Iterator<Item = (&WlSurface, Point<i32, Logical>)> {
        self.im
            .iter()
            .filter(|im| im.active.is_some())
            .flat_map(|im| &im.popups)
            .filter_map(|p| Some((&p.surface, p.at?)))
    }

    /// The popup surface under `pointer`, with where it is.
    pub(super) fn surface_under(
        &self,
        pointer: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        self.shown().find_map(|(surface, at)| {
            under_from_surface_tree(surface, pointer, at, WindowSurfaceType::ALL)
                .map(|(surface, at)| (surface, at.to_f64()))
        })
    }

    pub(super) fn elements(
        &self,
        renderer: &mut GlesRenderer,
        scale: Scale<f64>,
    ) -> Vec<WaylandSurfaceRenderElement<GlesRenderer>> {
        self.shown()
            .flat_map(|(surface, at)| {
                render_elements_from_surface_tree(
                    renderer,
                    surface,
                    at.to_physical_precise_round(scale),
                    scale,
                    1.0,
                    Kind::Unspecified,
                )
            })
            .collect()
    }

    pub(super) fn send_frames(&self, output: &smithay::output::Output, now: std::time::Duration) {
        for (surface, _) in self.shown() {
            send_frames_surface_tree(
                surface,
                output,
                now,
                Some(std::time::Duration::ZERO),
                |_, _| Some(output.clone()),
            );
        }
    }
}

/// Tells the input method about `field`.
fn send_field(im: &ZwpInputMethodV2, field: &Field) {
    if let Some((text, cursor, anchor)) = &field.surrounding {
        im.surrounding_text(text.clone(), *cursor, *anchor);
    }
    im.text_change_cause(field.cause);
    im.content_type(field.hint, field.purpose);
}

impl<S: Shell + 'static> Host<S> {
    /// Activates the input method for the enabled field, deactivates it when
    /// there is none, and passes on what the field committed.
    pub(super) fn update_input_method(&mut self) {
        let dirty = self.text_inputs.take_dirty();
        let field = self
            .text_inputs
            .field()
            .map(|(object, _, field)| (object.clone(), field.clone()));
        let Some(im) = &mut self.input_method.im else {
            return;
        };
        let now = field.as_ref().map(|(object, _)| object.clone());
        if im.active != now {
            // Activating again resets its state, so a switch from one field
            // to another needs no deactivate.
            match &field {
                Some((_, field)) => {
                    im.object.activate();
                    send_field(&im.object, field);
                }
                None => im.object.deactivate(),
            }
            im.object.done();
            im.active = now;
        } else if dirty && let Some((_, field)) = &field {
            send_field(&im.object, field);
            im.object.done();
        }
        self.place_input_popups();
    }

    /// The origin of a toplevel or layer surface, wherever it is shown.
    fn surface_origin(&self, surface: &WlSurface) -> Option<Point<i32, Logical>> {
        if let Some(window) = self
            .space
            .elements()
            .find(|w| w.toplevel().is_some_and(|t| t.wl_surface() == surface))
        {
            return self.space.element_location(window);
        }
        let map = layer_map_for_output(&self.output);
        let layer = map.layer_for_surface(surface, WindowSurfaceType::TOPLEVEL)?;
        map.layer_geometry(layer).map(|g| g.loc)
    }

    /// Puts the popups under the field's cursor, or above it where they
    /// would run off the bottom, and tells them where the cursor is.
    pub(super) fn place_input_popups(&mut self) {
        let anchor = self.text_inputs.field().and_then(|(_, surface, field)| {
            let origin = self.surface_origin(surface)?;
            let cursor = field.cursor.unwrap_or_default();
            Some(Rectangle::new(origin + cursor.loc, cursor.size))
        });
        let screen = self.backend.size();
        let Some(im) = &mut self.input_method.im else {
            return;
        };
        for popup in &mut im.popups {
            let Some(anchor) = anchor.filter(|_| im.active.is_some()) else {
                popup.at = None;
                continue;
            };
            let size = bbox_from_surface_tree(&popup.surface, (0, 0)).size;
            let below = anchor.loc.y + anchor.size.h;
            let y = if below + size.h > screen.h && anchor.loc.y >= size.h {
                anchor.loc.y - size.h
            } else {
                below
            };
            let x = anchor.loc.x.min(screen.w - size.w).max(0);
            let at = Point::from((x, y));
            popup.at = Some(at);
            let relative = Rectangle::new(anchor.loc - at, anchor.size);
            if popup.sent != Some(relative) {
                popup.sent = Some(relative);
                popup.object.text_input_rectangle(
                    relative.loc.x,
                    relative.loc.y,
                    relative.size.w,
                    relative.size.h,
                );
            }
        }
    }

    /// Places popups again after one of them commits a new size.
    pub(super) fn input_popup_commit(&mut self, surface: &WlSurface) {
        let ours = self
            .input_method
            .im
            .as_ref()
            .is_some_and(|im| im.popups.iter().any(|p| &p.surface == surface));
        if ours {
            self.place_input_popups();
        }
    }

    /// Hands a key from the seat to the input method's keyboard grab while
    /// it is active. Returns whether the grab took it.
    pub(super) fn input_method_key(&mut self, keycode: u32, pressed: bool, time: u32) -> bool {
        let grab = self
            .input_method
            .im
            .as_ref()
            .and_then(|im| im.grab.clone().filter(|_| im.active.is_some()));
        let take = if pressed {
            grab.is_some() && self.input_method.grabbed.insert(keycode)
        } else {
            self.input_method.grabbed.remove(&keycode)
        };
        if !take {
            return false;
        }
        let Some(grab) = grab.or_else(|| self.input_method.im.as_ref()?.grab.clone()) else {
            // The grab went away mid-press: the release is swallowed, as
            // the press was.
            return true;
        };
        let state = if pressed {
            KeyState::Pressed
        } else {
            KeyState::Released
        };
        // The grab speaks wl_keyboard: evdev codes, 8 below xkb's.
        grab.key(
            SERIAL_COUNTER.next_serial().into(),
            time,
            keycode.saturating_sub(8),
            state,
        );
        let mods = self
            .seat
            .get_keyboard()
            .map(|k| k.modifier_state().serialized);
        if let Some(im) = &mut self.input_method.im
            && let Some(mods) = mods
            && im.mods != Some(mods)
        {
            im.mods = Some(mods);
            grab.modifiers(
                SERIAL_COUNTER.next_serial().into(),
                mods.depressed,
                mods.latched,
                mods.locked,
                mods.layout_effective,
            );
        }
        true
    }

    /// Sends the seat's keymap to the input method's keyboard grab.
    pub(super) fn send_input_method_keymap(&mut self) {
        let Some(grab) = self.input_method.im.as_ref().and_then(|im| im.grab.clone()) else {
            return;
        };
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        let file = keyboard.with_xkb_state(self, |context| {
            let xkb = context.xkb().lock().ok()?;
            // SAFETY: the keymap is only borrowed while the lock is held.
            Some(KeymapFile::new(unsafe { xkb.keymap() }))
        });
        let Some(file) = file else {
            return;
        };
        if let Err(e) = file.with_fd(true, |fd, size| {
            grab.keymap(KeymapFormat::XkbV1, fd, size as u32);
        }) {
            warn!(error = %e, "cannot send the keymap to the input method");
        }
        if let Some(im) = &mut self.input_method.im {
            im.mods = None;
        }
    }
}

impl<S: Shell + 'static> GlobalDispatch<ZwpInputMethodManagerV2, ()> for Host<S> {
    fn bind(
        _host: &mut Self,
        _dh: &DisplayHandle,
        _client: &Client,
        resource: New<ZwpInputMethodManagerV2>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }

    fn can_view(client: Client, _global_data: &()) -> bool {
        unsandboxed(&client)
    }
}

impl<S: Shell + 'static> Dispatch<ZwpInputMethodManagerV2, ()> for Host<S> {
    fn request(
        host: &mut Self,
        _client: &Client,
        _manager: &ZwpInputMethodManagerV2,
        request: zwp_input_method_manager_v2::Request,
        _data: &(),
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        // There is one seat, so the seat argument needs no lookup.
        if let zwp_input_method_manager_v2::Request::GetInputMethod { input_method, .. } = request {
            let object = data_init.init(input_method, Mutex::new(Edit::default()));
            if host.input_method.im.is_some() {
                object.unavailable();
                return;
            }
            host.input_method.im = Some(Connected {
                object,
                active: None,
                grab: None,
                mods: None,
                popups: Vec::new(),
            });
            host.update_input_method();
        }
    }
}

impl<S: Shell + 'static> Dispatch<ZwpInputMethodV2, Mutex<Edit>> for Host<S> {
    fn request(
        host: &mut Self,
        _client: &Client,
        object: &ZwpInputMethodV2,
        request: zwp_input_method_v2::Request,
        pending: &Mutex<Edit>,
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        let current = host
            .input_method
            .im
            .as_mut()
            .filter(|im| &im.object == object);
        let mut edit = pending.lock().unwrap_or_else(|e| e.into_inner());
        match request {
            zwp_input_method_v2::Request::CommitString { text } => edit.commit = Some(text),
            zwp_input_method_v2::Request::SetPreeditString {
                text,
                cursor_begin,
                cursor_end,
            } => edit.preedit = Some((text, cursor_begin, cursor_end)),
            zwp_input_method_v2::Request::DeleteSurroundingText {
                before_length,
                after_length,
            } => edit.delete = Some((before_length, after_length)),
            // The serial says which of the field's states the edit was
            // made against; a stale one still applies, as wlroots does.
            zwp_input_method_v2::Request::Commit { .. } => {
                let edit = std::mem::take(&mut *edit);
                if current.is_some_and(|im| im.active.is_some()) {
                    host.text_inputs.apply(edit);
                }
            }
            zwp_input_method_v2::Request::GetInputPopupSurface { id, surface } => {
                if give_role(&surface, POPUP_ROLE).is_err() {
                    object.post_error(0u32, "the surface already has a role");
                    return;
                }
                let popup = data_init.init(id, ());
                if let Some(im) = current {
                    im.popups.push(Popup {
                        object: popup,
                        surface,
                        at: None,
                        sent: None,
                    });
                    drop(edit);
                    host.place_input_popups();
                }
            }
            zwp_input_method_v2::Request::GrabKeyboard { keyboard } => {
                let grab = data_init.init(keyboard, ());
                if let Some(im) = current {
                    grab.repeat_info(REPEAT.0, REPEAT.1);
                    im.grab = Some(grab);
                    drop(edit);
                    host.send_input_method_keymap();
                }
            }
            _ => {}
        }
    }

    fn destroyed(
        host: &mut Self,
        _client: ClientId,
        object: &ZwpInputMethodV2,
        _data: &Mutex<Edit>,
    ) {
        if host
            .input_method
            .im
            .as_ref()
            .is_some_and(|im| &im.object == object)
        {
            // Text it was still composing must not stay on the field.
            if host
                .input_method
                .im
                .take()
                .is_some_and(|im| im.active.is_some())
            {
                host.text_inputs.apply(Edit::default());
            }
        }
    }
}

impl<S: Shell + 'static> Dispatch<ZwpInputPopupSurfaceV2, ()> for Host<S> {
    fn request(
        _host: &mut Self,
        _client: &Client,
        _popup: &ZwpInputPopupSurfaceV2,
        _request: zwp_input_popup_surface_v2::Request,
        _data: &(),
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        // Destroy is its only request, handled in `destroyed`.
    }

    fn destroyed(host: &mut Self, _client: ClientId, popup: &ZwpInputPopupSurfaceV2, _data: &()) {
        if let Some(im) = &mut host.input_method.im {
            im.popups.retain(|p| &p.object != popup);
        }
    }
}

impl<S: Shell + 'static> Dispatch<ZwpInputMethodKeyboardGrabV2, ()> for Host<S> {
    fn request(
        _host: &mut Self,
        _client: &Client,
        _grab: &ZwpInputMethodKeyboardGrabV2,
        _request: zwp_input_method_keyboard_grab_v2::Request,
        _data: &(),
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        // Release is its only request, handled in `destroyed`.
    }

    fn destroyed(
        host: &mut Self,
        _client: ClientId,
        grab: &ZwpInputMethodKeyboardGrabV2,
        _data: &(),
    ) {
        if let Some(im) = &mut host.input_method.im
            && im.grab.as_ref() == Some(grab)
        {
            im.grab = None;
        }
    }
}
