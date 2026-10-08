//! `ext_background_effect_manager_v1`: a client asks for what is behind
//! part of its surface to be blurred (a translucent terminal, a frosted
//! bar), with the same dual Kawase blur as `Shell::blur_regions`. The blur
//! is applied to what was drawn before the surface, just before it is
//! drawn, for windows and layer-shell surfaces; only the region set on the
//! root surface counts, clipped to where the window's content is placed.

use std::sync::Mutex;

use smithay::{
    reexports::{
        wayland_protocols::ext::background_effect::v1::server::{
            ext_background_effect_manager_v1::{self, Capability, ExtBackgroundEffectManagerV1},
            ext_background_effect_surface_v1::{self, ExtBackgroundEffectSurfaceV1},
        },
        wayland_server::{
            Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, Weak,
            backend::ClientId, protocol::wl_surface::WlSurface,
        },
    },
    utils::{Logical, Point, Rectangle},
    wayland::compositor::{Cacheable, RectangleKind, get_region_attributes, with_states},
};

use super::Host;
use crate::{Blur, Shell};

/// How strongly client-requested backgrounds are blurred (1–10).
const STRENGTH: u8 = 6;

/// A surface's blur region, double-buffered like the rest of its state:
/// rectangles in surface coordinates, none for no blur.
#[derive(Clone, Debug, Default)]
pub(crate) struct BlurCachedState {
    region: Vec<Rectangle<i32, Logical>>,
}

impl Cacheable for BlurCachedState {
    fn commit(&mut self, _dh: &DisplayHandle) -> Self {
        self.clone()
    }

    fn merge_into(self, into: &mut Self, _dh: &DisplayHandle) {
        *into = self;
    }
}

/// Marks a surface that has a background effect object, which the protocol
/// allows only one of.
#[derive(Default)]
struct HasBackgroundEffect(Mutex<bool>);

pub(super) fn create_global<S: Shell + 'static>(dh: &DisplayHandle) {
    dh.create_global::<Host<S>, ExtBackgroundEffectManagerV1, ()>(1, ());
}

/// The blurs behind `surface`, whose origin is at `origin`, kept inside
/// `clip`.
pub(super) fn surface_blurs(
    surface: &WlSurface,
    origin: Point<i32, Logical>,
    clip: Rectangle<i32, Logical>,
) -> Vec<Blur> {
    with_states(surface, |states| {
        states
            .cached_state
            .get::<BlurCachedState>()
            .current()
            .region
            .iter()
            .filter_map(|r| Rectangle::new(r.loc + origin, r.size).intersection(clip))
            .map(|area| Blur {
                area,
                corner_radius: 0,
                strength: STRENGTH,
            })
            .collect()
    })
}

impl<S: Shell + 'static> GlobalDispatch<ExtBackgroundEffectManagerV1, ()> for Host<S> {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ExtBackgroundEffectManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        let manager = data_init.init(resource, ());
        manager.capabilities(Capability::Blur);
    }
}

impl<S: Shell + 'static> Dispatch<ExtBackgroundEffectManagerV1, ()> for Host<S> {
    fn request(
        _state: &mut Self,
        _client: &Client,
        manager: &ExtBackgroundEffectManagerV1,
        request: ext_background_effect_manager_v1::Request,
        _data: &(),
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let ext_background_effect_manager_v1::Request::GetBackgroundEffect { id, surface } =
            request
        {
            let taken = with_states(&surface, |states| {
                states
                    .data_map
                    .insert_if_missing_threadsafe(HasBackgroundEffect::default);
                let flag = states
                    .data_map
                    .get::<HasBackgroundEffect>()
                    .expect("inserted above");
                std::mem::replace(&mut *flag.0.lock().unwrap_or_else(|e| e.into_inner()), true)
            });
            if taken {
                manager.post_error(
                    ext_background_effect_manager_v1::Error::BackgroundEffectExists,
                    "the surface already has a background effect object",
                );
                return;
            }
            data_init.init(id, surface.downgrade());
        }
    }
}

impl<S: Shell + 'static> Dispatch<ExtBackgroundEffectSurfaceV1, Weak<WlSurface>> for Host<S> {
    fn request(
        _state: &mut Self,
        _client: &Client,
        effect: &ExtBackgroundEffectSurfaceV1,
        request: ext_background_effect_surface_v1::Request,
        surface: &Weak<WlSurface>,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        let Ok(surface) = surface.upgrade() else {
            if matches!(
                request,
                ext_background_effect_surface_v1::Request::SetBlurRegion { .. }
            ) {
                effect.post_error(
                    ext_background_effect_surface_v1::Error::SurfaceDestroyed,
                    "the surface was destroyed",
                );
            }
            return;
        };
        let region = match request {
            // The region is copied now; later changes to it do not count.
            ext_background_effect_surface_v1::Request::SetBlurRegion { region } => region
                .map(|region| {
                    let mut rects: Vec<Rectangle<i32, Logical>> = Vec::new();
                    for (kind, rect) in get_region_attributes(&region).rects {
                        match kind {
                            RectangleKind::Add => {
                                let new =
                                    Rectangle::subtract_rects_many([rect], rects.iter().copied());
                                rects.extend(new);
                            }
                            RectangleKind::Subtract => {
                                rects = Rectangle::subtract_rects_many(rects, [rect]);
                            }
                        }
                    }
                    rects
                })
                .unwrap_or_default(),
            // Removes the effect on the next commit.
            ext_background_effect_surface_v1::Request::Destroy => Vec::new(),
            _ => return,
        };
        with_states(&surface, |states| {
            states
                .cached_state
                .get::<BlurCachedState>()
                .pending()
                .region = region;
        });
    }

    fn destroyed(
        _state: &mut Self,
        _client: ClientId,
        _effect: &ExtBackgroundEffectSurfaceV1,
        surface: &Weak<WlSurface>,
    ) {
        if let Ok(surface) = surface.upgrade() {
            with_states(&surface, |states| {
                if let Some(flag) = states.data_map.get::<HasBackgroundEffect>() {
                    *flag.0.lock().unwrap_or_else(|e| e.into_inner()) = false;
                }
            });
        }
    }
}
