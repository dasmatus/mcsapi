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
//!
//! The field's surrounding text, content type and cursor rectangle are kept
//! for an input method client (fcitx5, squeekboard) connected through
//! `zwp_input_method_v2` (see `host::input_method`), whose preedit, commits
//! and deletions reach the field with [`TextInputs::apply`].

use smithay::reexports::{
    wayland_protocols::wp::text_input::zv3::server::{
        zwp_text_input_manager_v3::{self, ZwpTextInputManagerV3},
        zwp_text_input_v3::{self, ChangeCause, ContentHint, ContentPurpose, ZwpTextInputV3},
    },
    wayland_server::{
        Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
        backend::ClientId, protocol::wl_surface::WlSurface,
    },
};
use smithay::utils::{Logical, Rectangle};

use crate::{Shell, TextField, host::Host};

/// One client's text input object, with its double-buffered state.
struct Instance {
    object: ZwpTextInputV3,
    /// The surface it was sent `enter` for, while it has one.
    entered: Option<WlSurface>,
    pending: Pending,
    enabled: bool,
    password: bool,
    field: Field,
    /// Commit requests so far; `done` echoes it back.
    serial: u32,
}

#[derive(Default)]
struct Pending {
    enable: Option<bool>,
    password: Option<bool>,
    surrounding: Option<(String, u32, u32)>,
    cause: Option<ChangeCause>,
    content: Option<(ContentHint, ContentPurpose)>,
    cursor: Option<Rectangle<i32, Logical>>,
}

/// What an enabled field last committed about itself, for an input method.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Field {
    /// The text around the cursor, with the cursor's and the selection
    /// anchor's byte offsets in it.
    pub(crate) surrounding: Option<(String, u32, u32)>,
    pub(crate) cause: ChangeCause,
    pub(crate) hint: ContentHint,
    pub(crate) purpose: ContentPurpose,
    /// The cursor, in the coordinates of the surface it entered.
    pub(crate) cursor: Option<Rectangle<i32, Logical>>,
}

impl Default for Field {
    fn default() -> Self {
        Self {
            surrounding: None,
            cause: ChangeCause::InputMethod,
            hint: ContentHint::None,
            purpose: ContentPurpose::Normal,
            cursor: None,
        }
    }
}

/// An input method's edit to the enabled field, applied in one `done`.
#[derive(Clone, Debug, Default)]
pub(crate) struct Edit {
    /// Text being composed, with the cursor's byte range in it.
    pub(crate) preedit: Option<(String, i32, i32)>,
    pub(crate) commit: Option<String>,
    /// Bytes to delete before and after the cursor.
    pub(crate) delete: Option<(u32, u32)>,
}

/// Every text input object, and which one is enabled.
#[derive(Default)]
pub(crate) struct TextInputs {
    instances: Vec<Instance>,
    focus: Option<WlSurface>,
    /// What the shell was last told, so it hears only changes.
    reported: Option<TextField>,
    /// Whether the enabled field committed or focus moved since an input
    /// method last heard.
    dirty: bool,
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
        self.dirty = true;
    }

    /// The enabled field on the focused surface, if any.
    fn active(&self) -> Option<&Instance> {
        self.instances
            .iter()
            .find(|i| i.enabled && i.entered.is_some())
    }

    /// The enabled field on the focused surface: its object, which tells
    /// one field from another, the surface it is on and its state.
    pub(crate) fn field(&self) -> Option<(&ZwpTextInputV3, &WlSurface, &Field)> {
        let i = self.active()?;
        Some((&i.object, i.entered.as_ref()?, &i.field))
    }

    /// Whether the enabled field changed since this was last asked.
    pub(crate) fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// Applies an input method's edit to the enabled field. Returns
    /// `false` when there is none.
    pub(crate) fn apply(&self, edit: Edit) -> bool {
        let Some(instance) = self.active() else {
            return false;
        };
        // The protocol's order: delete, then commit, then the preedit,
        // which is cleared unless the edit sets one.
        if let Some((before, after)) = edit.delete {
            instance.object.delete_surrounding_text(before, after);
        }
        if let Some(text) = edit.commit {
            instance.object.commit_string(Some(text));
        }
        match edit.preedit {
            Some((text, begin, end)) => instance.object.preedit_string(Some(text), begin, end),
            None => instance.object.preedit_string(None, 0, 0),
        }
        instance.object.done(instance.serial);
        true
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
                field: Field::default(),
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
        let mut committed = false;
        if let zwp_text_input_v3::Request::Destroy = request {
            inputs.instances.retain(|i| i.object != *resource);
        } else if let Some(instance) = inputs.instances.iter_mut().find(|i| i.object == *resource) {
            match request {
                zwp_text_input_v3::Request::Enable => instance.pending.enable = Some(true),
                zwp_text_input_v3::Request::Disable => instance.pending.enable = Some(false),
                zwp_text_input_v3::Request::SetContentType { hint, purpose } => {
                    // A keyboard that learns words must not learn passwords
                    // or PINs.
                    instance.pending.password = Some(matches!(
                        purpose.into_result(),
                        Ok(ContentPurpose::Password | ContentPurpose::Pin)
                    ));
                    instance.pending.content = Some((
                        hint.into_result().unwrap_or(ContentHint::None),
                        purpose.into_result().unwrap_or(ContentPurpose::Normal),
                    ));
                }
                zwp_text_input_v3::Request::SetSurroundingText {
                    text,
                    cursor,
                    anchor,
                } => {
                    instance.pending.surrounding = Some((
                        text,
                        u32::try_from(cursor).unwrap_or(0),
                        u32::try_from(anchor).unwrap_or(0),
                    ));
                }
                zwp_text_input_v3::Request::SetTextChangeCause { cause } => {
                    instance.pending.cause = cause.into_result().ok();
                }
                zwp_text_input_v3::Request::SetCursorRectangle {
                    x,
                    y,
                    width,
                    height,
                } => {
                    instance.pending.cursor =
                        Some(Rectangle::new((x, y).into(), (width, height).into()));
                }
                zwp_text_input_v3::Request::Commit => {
                    instance.serial = instance.serial.wrapping_add(1);
                    let pending = std::mem::take(&mut instance.pending);
                    if let Some(enable) = pending.enable {
                        // Enabling resets the content type, surrounding
                        // text and cursor, unless the same commit sets them
                        // again.
                        instance.enabled = enable && instance.entered.is_some();
                        instance.password = false;
                        instance.field = Field::default();
                    }
                    if let Some(password) = pending.password {
                        instance.password = password;
                    }
                    let field = &mut instance.field;
                    if let Some(surrounding) = pending.surrounding {
                        field.surrounding = Some(surrounding);
                    }
                    // The cause is per commit: anything but the input
                    // method's own edit says so every time.
                    field.cause = pending.cause.unwrap_or(ChangeCause::InputMethod);
                    if let Some((hint, purpose)) = pending.content {
                        field.hint = hint;
                        field.purpose = purpose;
                    }
                    if let Some(cursor) = pending.cursor {
                        field.cursor = Some(cursor);
                    }
                    committed = true;
                }
                _ => {}
            }
        }
        inputs.dirty |= committed;
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
