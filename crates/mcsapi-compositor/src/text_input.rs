//! `zwp_text_input_v3`: lets the shell's own on-screen keyboard know when a
//! client's text field has focus, and type into it as committed text.
//!
//! Smithay ships this protocol too, but its implementation only forwards to
//! an `input_method_v2` client and drops every request while none is
//! connected. The shell draws its keyboard itself rather than running one as
//! a separate client, so this is a small implementation of the client-facing
//! half on its own: enter and leave follow keyboard focus, an enabled field
//! is reported through [`Shell::text_input`], and text typed on the
//! on-screen keyboard reaches it with `commit_string`.
//!
//! GTK 3 and 4 and Qt 6 all enable text-input-v3 on the focused field when
//! the compositor offers it.

use smithay::reexports::{
    wayland_protocols::wp::text_input::zv3::server::{
        zwp_text_input_manager_v3::{self, ZwpTextInputManagerV3},
        zwp_text_input_v3::{self, ContentPurpose, ZwpTextInputV3},
    },
    wayland_server::{
        Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
        backend::ClientId, protocol::wl_surface::WlSurface,
    },
};

use crate::{Shell, TextField, host::Host};

/// One client's text input object, with its double-buffered state.
struct Instance {
    object: ZwpTextInputV3,
    /// The surface it was sent `enter` for, while it has one.
    entered: Option<WlSurface>,
    pending: Pending,
    enabled: bool,
    password: bool,
    /// Commit requests so far; `done` echoes it back.
    serial: u32,
}

#[derive(Default)]
struct Pending {
    enable: Option<bool>,
    password: Option<bool>,
}

/// Every text input object, and which one is enabled.
#[derive(Default)]
pub(crate) struct TextInputs {
    instances: Vec<Instance>,
    focus: Option<WlSurface>,
    /// What the shell was last told, so it hears only changes.
    reported: Option<TextField>,
}

impl TextInputs {
    /// Advertises the global.
    pub(crate) fn global<S: Shell + 'static>(display: &DisplayHandle) {
        display.create_global::<Host<S>, ZwpTextInputManagerV3, ()>(1, ());
    }

    /// Follows keyboard focus: inputs on the old surface's client leave it,
    /// and inputs on the new one's client enter it.
    pub(crate) fn set_focus(&mut self, surface: Option<WlSurface>) {
        if self.focus == surface {
            return;
        }
        for instance in &mut self.instances {
            if let Some(old) = instance.entered.take() {
                instance.object.leave(&old);
                // Leaving disables, per the protocol; the client enables
                // again after its next enter.
                instance.enabled = false;
            }
        }
        if let Some(surface) = &surface {
            let client = surface.client().map(|c| c.id());
            for instance in &mut self.instances {
                if instance.object.client().map(|c| c.id()) == client {
                    instance.object.enter(surface);
                    instance.entered = Some(surface.clone());
                }
            }
        }
        self.focus = surface;
    }

    /// The enabled field on the focused surface, if any.
    fn active(&self) -> Option<&Instance> {
        self.instances
            .iter()
            .find(|i| i.enabled && i.entered.is_some())
    }

    /// The field to report to the shell, if it changed since last time.
    pub(crate) fn changed(&mut self) -> Option<Option<TextField>> {
        let now = self.active().map(|i| TextField {
            password: i.password,
        });
        (now != self.reported).then(|| {
            self.reported = now;
            now
        })
    }

    /// Commits `text` to the enabled field. Returns `false` when there is
    /// none, so the caller can fall back to synthesized key presses.
    pub(crate) fn commit_string(&self, text: &str) -> bool {
        let Some(instance) = self.active() else {
            return false;
        };
        instance.object.commit_string(Some(text.to_owned()));
        instance.object.done(instance.serial);
        true
    }
}

impl<S: Shell + 'static> GlobalDispatch<ZwpTextInputManagerV3, ()> for Host<S> {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwpTextInputManagerV3>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl<S: Shell + 'static> Dispatch<ZwpTextInputManagerV3, ()> for Host<S> {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &ZwpTextInputManagerV3,
        request: zwp_text_input_manager_v3::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        // There is one seat, so the seat argument needs no lookup.
        if let zwp_text_input_manager_v3::Request::GetTextInput { id, .. } = request {
            let object = data_init.init(id, ());
            let inputs = state.text_inputs_mut();
            let entered = inputs
                .focus
                .clone()
                .filter(|s| s.client().map(|c| c.id()) == object.client().map(|c| c.id()));
            if let Some(surface) = &entered {
                object.enter(surface);
            }
            inputs.instances.push(Instance {
                object,
                entered,
                pending: Pending::default(),
                enabled: false,
                password: false,
                serial: 0,
            });
        }
    }
}

impl<S: Shell + 'static> Dispatch<ZwpTextInputV3, ()> for Host<S> {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &ZwpTextInputV3,
        request: zwp_text_input_v3::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        let inputs = state.text_inputs_mut();
        if let zwp_text_input_v3::Request::Destroy = request {
            inputs.instances.retain(|i| i.object != *resource);
        } else if let Some(instance) = inputs.instances.iter_mut().find(|i| i.object == *resource) {
            match request {
                zwp_text_input_v3::Request::Enable => instance.pending.enable = Some(true),
                zwp_text_input_v3::Request::Disable => instance.pending.enable = Some(false),
                zwp_text_input_v3::Request::SetContentType { purpose, .. } => {
                    // A keyboard that learns words must not learn passwords
                    // or PINs.
                    instance.pending.password = Some(matches!(
                        purpose.into_result(),
                        Ok(ContentPurpose::Password | ContentPurpose::Pin)
                    ));
                }
                zwp_text_input_v3::Request::Commit => {
                    instance.serial = instance.serial.wrapping_add(1);
                    let pending = std::mem::take(&mut instance.pending);
                    if let Some(enable) = pending.enable {
                        // Enabling resets the content type, unless the same
                        // commit sets it again.
                        instance.enabled = enable && instance.entered.is_some();
                        instance.password = false;
                    }
                    if let Some(password) = pending.password {
                        instance.password = password;
                    }
                }
                // Surrounding text, its change cause and the cursor
                // rectangle are for input methods that edit around the
                // cursor; this keyboard only appends and backspaces.
                _ => {}
            }
        }
        state.text_input_changed();
    }

    fn destroyed(state: &mut Self, _client: ClientId, resource: &ZwpTextInputV3, _data: &()) {
        state
            .text_inputs_mut()
            .instances
            .retain(|i| i.object != *resource);
        state.text_input_changed();
    }
}
