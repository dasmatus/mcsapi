//! Per-surface protocols that change how a client's buffer is shown, or hint
//! how it would like it shown. Smithay's surface render elements already
//! honour viewports, single-pixel buffers and alpha multipliers once these
//! globals exist; the rest is recorded in each surface's state.
//!
//! - `wp_viewporter`: crop and scale a buffer (video players, GTK 4's
//!   graphics offload, Firefox).
//! - `wp_fractional_scale_v1`: tells clients the scale to render at. The
//!   compositor draws at scale 1, so that is what every surface is told;
//!   clients that ask stop guessing from `wl_output.scale`.
//! - `wp_single_pixel_buffer_v1`: solid colours without allocating a buffer
//!   (GTK 4 backgrounds, mpv's black bars).
//! - `wp_alpha_modifier_v1`: fade a surface without redrawing it.
//! - `wp_content_type_v1`: photo, video or game.
//! - `wp_tearing_control_v1`: a game that would rather tear than wait for
//!   the next refresh.
//!
//! Content type and the tearing hint are recorded for the surface
//! ([`ContentTypeSurfaceCachedState`], [`TearingCachedState`]); both are
//! hints the protocols let the compositor ignore, and frames stay in step
//! with the refresh.
//!
//! [`ContentTypeSurfaceCachedState`]: smithay::wayland::content_type::ContentTypeSurfaceCachedState

use std::sync::Mutex;

use smithay::{
    delegate_alpha_modifier, delegate_content_type, delegate_fractional_scale,
    delegate_single_pixel_buffer, delegate_viewporter,
    reexports::{
        wayland_protocols::wp::tearing_control::v1::server::{
            wp_tearing_control_manager_v1::{self, WpTearingControlManagerV1},
            wp_tearing_control_v1::{self, PresentationHint, WpTearingControlV1},
        },
        wayland_server::{
            Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, Weak,
            backend::ClientId, protocol::wl_surface::WlSurface,
        },
    },
    wayland::{
        alpha_modifier::AlphaModifierState,
        compositor::{Cacheable, with_states},
        content_type::ContentTypeState,
        fractional_scale::{
            FractionalScaleHandler, FractionalScaleManagerState, with_fractional_scale,
        },
        single_pixel_buffer::SinglePixelBufferState,
        viewporter::ViewporterState,
    },
};

use super::Host;
use crate::Shell;

/// The globals; held so they live as long as the display.
pub(super) struct Hints {
    _viewporter: ViewporterState,
    _fractional_scale: FractionalScaleManagerState,
    _single_pixel_buffer: SinglePixelBufferState,
    _alpha_modifier: AlphaModifierState,
    _content_type: ContentTypeState,
}

impl Hints {
    pub(super) fn new<S: Shell + 'static>(dh: &DisplayHandle) -> Self {
        dh.create_global::<Host<S>, WpTearingControlManagerV1, ()>(1, ());
        Self {
            _viewporter: ViewporterState::new::<Host<S>>(dh),
            _fractional_scale: FractionalScaleManagerState::new::<Host<S>>(dh),
            _single_pixel_buffer: SinglePixelBufferState::new::<Host<S>>(dh),
            _alpha_modifier: AlphaModifierState::new::<Host<S>>(dh),
            _content_type: ContentTypeState::new::<Host<S>>(dh),
        }
    }
}

impl<S: Shell + 'static> FractionalScaleHandler for Host<S> {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        with_states(&surface, |states| {
            with_fractional_scale(states, |scale| scale.set_preferred_scale(1.0));
        });
    }
}

/// A surface's tearing hint (`wp_tearing_control_v1`), double-buffered like
/// the rest of its state: `true` once a commit carried `async`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TearingCachedState {
    pub(crate) tearing: bool,
}

impl Cacheable for TearingCachedState {
    fn commit(&mut self, _dh: &DisplayHandle) -> Self {
        *self
    }

    fn merge_into(self, into: &mut Self, _dh: &DisplayHandle) {
        *into = self;
    }
}

/// Marks a surface that has a tearing control object, which the protocol
/// allows only one of.
#[derive(Default)]
struct HasTearingControl(Mutex<bool>);

impl<S: Shell + 'static> GlobalDispatch<WpTearingControlManagerV1, ()> for Host<S> {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<WpTearingControlManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl<S: Shell + 'static> Dispatch<WpTearingControlManagerV1, ()> for Host<S> {
    fn request(
        _state: &mut Self,
        _client: &Client,
        manager: &WpTearingControlManagerV1,
        request: wp_tearing_control_manager_v1::Request,
        _data: &(),
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            wp_tearing_control_manager_v1::Request::GetTearingControl { id, surface } => {
                let taken = with_states(&surface, |states| {
                    states
                        .data_map
                        .insert_if_missing_threadsafe(HasTearingControl::default);
                    let flag = states
                        .data_map
                        .get::<HasTearingControl>()
                        .expect("inserted above");
                    let mut flag = flag.0.lock().unwrap_or_else(|e| e.into_inner());
                    std::mem::replace(&mut *flag, true)
                });
                if taken {
                    manager.post_error(
                        wp_tearing_control_manager_v1::Error::TearingControlExists,
                        "the surface already has a tearing control object",
                    );
                    return;
                }
                data_init.init(id, surface.downgrade());
            }
            wp_tearing_control_manager_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl<S: Shell + 'static> Dispatch<WpTearingControlV1, Weak<WlSurface>> for Host<S> {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _object: &WpTearingControlV1,
        request: wp_tearing_control_v1::Request,
        surface: &Weak<WlSurface>,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        // Inert once the surface is gone.
        let Ok(surface) = surface.upgrade() else {
            return;
        };
        let tearing = match request {
            wp_tearing_control_v1::Request::SetPresentationHint { hint } => {
                matches!(hint.into_result(), Ok(PresentationHint::Async))
            }
            // Destroying the object reverts to vsync on the next commit.
            wp_tearing_control_v1::Request::Destroy => false,
            _ => return,
        };
        with_states(&surface, |states| {
            states
                .cached_state
                .get::<TearingCachedState>()
                .pending()
                .tearing = tearing;
        });
    }

    fn destroyed(
        _state: &mut Self,
        _client: ClientId,
        _object: &WpTearingControlV1,
        surface: &Weak<WlSurface>,
    ) {
        // A new object may be created for the surface once this one is gone.
        if let Ok(surface) = surface.upgrade() {
            with_states(&surface, |states| {
                if let Some(flag) = states.data_map.get::<HasTearingControl>() {
                    *flag.0.lock().unwrap_or_else(|e| e.into_inner()) = false;
                }
            });
        }
    }
}

delegate_viewporter!(@<S: Shell + 'static> Host<S>);
delegate_fractional_scale!(@<S: Shell + 'static> Host<S>);
delegate_single_pixel_buffer!(@<S: Shell + 'static> Host<S>);
delegate_alpha_modifier!(@<S: Shell + 'static> Host<S>);
delegate_content_type!(@<S: Shell + 'static> Host<S>);
