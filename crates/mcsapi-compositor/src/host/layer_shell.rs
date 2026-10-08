//! `zwlr_layer_shell_v1`: bars, docks, wallpapers, notifications and
//! launchers from other projects (waybar, swaybg, mako, fuzzel) anchored to
//! the output's edges. Smithay's layer map places them and works out the
//! area their exclusive zones leave; that area reaches the shell through
//! `Shell::set_reserved`, together with what runtime panels cover.
//!
//! Stacking, bottom to top: the shell's wallpaper, the background and
//! bottom layers, windows, the top layer with runtime panels, the chrome,
//! runtime overlays, the overlay layer. Pointer input follows the same
//! order, except that the top layer, like runtime panels, gives way to the
//! chrome while one of its popups is open, and the bottom and background
//! layers get only what neither a window nor the shell wants.
//!
//! The keyboard goes to the topmost surface on the top or overlay layer that
//! asks for it exclusively; one that takes it on demand gets it when
//! clicked, as a runtime panel that takes the keyboard does. The shell's
//! shortcuts still come first for both.

use smithay::{
    backend::renderer::{
        element::{AsRenderElements, surface::WaylandSurfaceRenderElement},
        gles::GlesRenderer,
    },
    delegate_layer_shell,
    desktop::{LayerSurface, WindowSurfaceType, layer_map_for_output},
    output::Output,
    reexports::wayland_server::{DisplayHandle, protocol::wl_surface::WlSurface},
    utils::{Logical, Point, Scale},
    wayland::{
        compositor::with_states,
        shell::{
            wlr_layer::{
                self, KeyboardInteractivity, Layer as WlrLayer, LayerSurfaceCachedState,
                LayerSurfaceData, WlrLayerShellHandler, WlrLayerShellState,
            },
            xdg::PopupSurface,
        },
    },
};
use std::time::Duration;
use tracing::warn;

use super::{Host, Layer, Route, security::unsandboxed};
use crate::{Reserved, Role, Shell, egui};

pub(super) struct LayerShell {
    state: WlrLayerShellState,
}

impl LayerShell {
    pub(super) fn new<S: Shell + 'static>(dh: &DisplayHandle) -> Self {
        Self {
            state: WlrLayerShellState::new_with_filter::<Host<S>, _>(dh, unsandboxed),
        }
    }
}

/// What a layer client under the pointer is.
enum Hit<'a> {
    /// A runtime panel or overlay.
    Runtime(&'a Layer),
    /// A layer-shell surface on this layer.
    Wlr(LayerSurface, WlrLayer),
}

impl<S: Shell + 'static> Host<S> {
    /// The topmost layer-shell surface on `layers` whose input region is
    /// under the pointer, with the surface hit and where it is.
    fn wlr_under(
        &self,
        layers: &[WlrLayer],
    ) -> Option<(LayerSurface, WlSurface, Point<f64, Logical>)> {
        let map = layer_map_for_output(&self.output);
        layers.iter().find_map(|&layer| {
            map.layers_on(layer).rev().find_map(|l| {
                let loc = map.layer_geometry(l)?.loc;
                let (surface, offset) =
                    l.surface_under(self.pointer - loc.to_f64(), WindowSurfaceType::ALL)?;
                Some((l.clone(), surface, (offset + loc).to_f64()))
            })
        })
    }

    /// What layer client, runtime or layer-shell, the pointer is over, in
    /// stacking order.
    fn layer_hit(&self) -> Option<Hit<'_>> {
        let wlr = |layers: &[WlrLayer]| {
            self.wlr_under(layers)
                .map(|(l, _, _)| Hit::Wlr(l.clone(), l.layer()))
        };
        wlr(&[WlrLayer::Overlay])
            .or_else(|| self.layer_under().map(Hit::Runtime))
            .or_else(|| wlr(&[WlrLayer::Top]))
            .or_else(|| wlr(&[WlrLayer::Bottom, WlrLayer::Background]))
    }

    /// Whether pointer input at the pointer goes to a panel, overlay or
    /// layer-shell surface instead of the chrome or windows.
    pub(super) fn pointer_on_layer(&self) -> bool {
        let popup = || egui::Popup::is_any_open(&self.chrome.ctx);
        match self.layer_hit() {
            Some(Hit::Runtime(layer)) if layer.role == Role::Overlay => true,
            Some(Hit::Wlr(_, WlrLayer::Overlay)) => true,
            Some(Hit::Runtime(_) | Hit::Wlr(_, WlrLayer::Top)) => !popup(),
            // Under the windows: only where there is none and the shell
            // leaves the pointer alone.
            Some(Hit::Wlr(..)) => {
                self.space.element_under(self.pointer).is_none() && !self.chrome_wants_pointer()
            }
            None => false,
        }
    }

    /// The surface a press on a layer client gives the keyboard to: a
    /// runtime panel that takes it, or a layer-shell surface that asks for
    /// it.
    pub(super) fn layer_click_focus(&self) -> Option<WlSurface> {
        match self.layer_hit()? {
            Hit::Runtime(Layer {
                role: Role::Panel { keyboard: true, .. },
                window,
            }) => window.toplevel().map(|t| t.wl_surface().clone()),
            Hit::Runtime(_) => None,
            Hit::Wlr(layer, _) => layer
                .can_receive_keyboard_focus()
                .then(|| layer.wl_surface().clone()),
        }
    }

    /// The topmost surface under the pointer, of any client, in stacking
    /// order.
    pub(super) fn surface_under(&self) -> Option<(WlSurface, Point<f64, Logical>)> {
        let wlr = |layers: &[WlrLayer]| self.wlr_under(layers).map(|(_, s, at)| (s, at));
        let space = || {
            let (window, loc) = self.space.element_under(self.pointer)?;
            window
                .surface_under(self.pointer - loc.to_f64(), WindowSurfaceType::ALL)
                .map(|(surface, offset)| (surface, (offset + loc).to_f64()))
        };
        wlr(&[WlrLayer::Overlay])
            .or_else(|| self.overlay().and_then(|_| space()))
            .or_else(|| wlr(&[WlrLayer::Top]))
            .or_else(space)
            .or_else(|| wlr(&[WlrLayer::Bottom, WlrLayer::Background]))
    }

    /// The layer-shell surface holding the keyboard exclusively: the
    /// topmost on the overlay or top layer that asks to.
    pub(super) fn wlr_exclusive_keyboard(&self) -> Option<WlSurface> {
        let map = layer_map_for_output(&self.output);
        [WlrLayer::Overlay, WlrLayer::Top]
            .into_iter()
            .find_map(|layer| {
                map.layers_on(layer).rev().find(|l| {
                    with_states(l.wl_surface(), |states| {
                        states
                            .cached_state
                            .get::<LayerSurfaceCachedState>()
                            .current()
                            .keyboard_interactivity
                            == KeyboardInteractivity::Exclusive
                    })
                })
            })
            .map(|l| l.wl_surface().clone())
    }

    /// Re-places layer-shell surfaces for the output's size, and returns
    /// what their exclusive zones cover.
    pub(super) fn arrange_wlr_layers(&self) -> Reserved {
        let size = self.backend.size();
        let mut map = layer_map_for_output(&self.output);
        map.arrange();
        let zone = map.non_exclusive_zone();
        Reserved {
            top: zone.loc.y,
            left: zone.loc.x,
            bottom: size.h - zone.loc.y - zone.size.h,
            right: size.w - zone.loc.x - zone.size.w,
        }
    }

    pub(super) fn wlr_send_frames(&self, now: Duration) {
        for layer in layer_map_for_output(&self.output).layers() {
            layer.send_frame(&self.output, now, Some(Duration::ZERO), |_, _| {
                Some(self.output.clone())
            });
        }
    }

    /// Sends a layer surface its first configure, sized by where the map
    /// puts it, after the commit that set its anchors and size (or after it
    /// unmapped itself with a null buffer and commits again).
    pub(super) fn wlr_commit(&mut self, surface: &WlSurface) {
        let mut map = layer_map_for_output(&self.output);
        if map
            .layer_for_surface(surface, WindowSurfaceType::TOPLEVEL)
            .is_none()
        {
            return;
        }
        let configured = with_states(surface, |states| {
            states
                .data_map
                .get::<LayerSurfaceData>()
                .and_then(|d| d.lock().ok().map(|d| d.initial_configure_sent))
                .unwrap_or(true)
        });
        map.arrange();
        if !configured
            && let Some(layer) = map.layer_for_surface(surface, WindowSurfaceType::TOPLEVEL)
        {
            layer.layer_surface().send_configure();
        }
        drop(map);
        // Its exclusive zone or keyboard interactivity may have changed.
        self.sync();
    }
}

/// The render elements of the layer-shell surfaces on `layer`, bottom
/// to top, one list per surface (each front to back, as
/// `draw_surfaces` takes them).
pub(super) fn wlr_elements(
    output: &Output,
    renderer: &mut GlesRenderer,
    layer: WlrLayer,
) -> Vec<Vec<WaylandSurfaceRenderElement<GlesRenderer>>> {
    let scale = Scale::from(1.0);
    let map = layer_map_for_output(output);
    map.layers_on(layer)
        .filter_map(|l| {
            let loc = map.layer_geometry(l)?.loc;
            Some(l.render_elements(renderer, loc.to_physical(1), scale, 1.0))
        })
        .collect()
}

impl<S: Shell + 'static> WlrLayerShellHandler for Host<S> {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell.state
    }

    /// There is one output, so a surface goes there whichever it asked for.
    fn new_layer_surface(
        &mut self,
        surface: wlr_layer::LayerSurface,
        _output: Option<smithay::reexports::wayland_server::protocol::wl_output::WlOutput>,
        _layer: WlrLayer,
        namespace: String,
    ) {
        let layer = LayerSurface::new(surface, namespace);
        if let Err(e) = layer_map_for_output(&self.output).map_layer(&layer) {
            warn!(error = %e, "cannot map layer surface");
        }
    }

    fn new_popup(&mut self, _parent: wlr_layer::LayerSurface, _popup: PopupSurface) {
        // Tracked by the xdg-shell handler like every popup; the layer map
        // finds it through its parent.
    }

    fn layer_destroyed(&mut self, surface: wlr_layer::LayerSurface) {
        let mut map = layer_map_for_output(&self.output);
        let layer = map
            .layers()
            .find(|l| l.layer_surface() == &surface)
            .cloned();
        if let Some(layer) = layer {
            map.unmap_layer(&layer);
        }
        drop(map);
        if self.layer_focus.as_ref() == Some(surface.wl_surface()) {
            self.layer_focus = None;
        }
        if self.route == Some(Route::Layer) {
            self.route = None;
        }
        self.sync();
    }
}

delegate_layer_shell!(@<S: Shell + 'static> Host<S>);
