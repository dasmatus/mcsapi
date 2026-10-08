//! Running on the bare seat: DRM/KMS output, libinput input, and seat access
//! through libseat (logind, or seatd).
//!
//! The nested backend draws into a window of another session. This one is
//! for when there is no other session: a display manager's greeter, or a
//! desktop started from a login. It drives the first connected display at
//! its preferred mode through GBM buffers, and gives up the GPU and input
//! devices whenever the seat moves to another VT.
//!
//! Frames are triple buffered: one buffer is on screen, one may wait for
//! its page flip, and one more may be queued behind it, which Smithay's
//! surface flips at the vblank that shows the one before. Drawing is paced
//! by a timer at the refresh rate, not by the vblank, so with only one
//! frame allowed past the screen a tick that lands just before the vblank
//! finds the flip still pending and the frame is dropped; on a GPU that
//! takes most of a refresh to draw, every other one is. The cost is a
//! frame of latency when the GPU is ahead, and one more scanout buffer.
//!
//! The frame is drawn exactly as for the nested window, into an offscreen
//! texture, and then copied onto the scanout buffer. The chrome and the blur
//! are painted with raw GL, which puts row 0 at the bottom as a window
//! surface does, while a scanout buffer shows row 0 at the top; drawing them
//! straight into it would turn them upside down under the client windows.
//! One extra full-screen copy keeps every layer of the frame identical
//! between the two backends.
//!
//! Clients that draw on the GPU hand over their buffers as dma-bufs
//! (`zwp_linux_dmabuf_v1`), which are only offered here: the nested window
//! has no DRM device to name in the feedback. Where the device can wait on
//! a syncobj through an eventfd, clients may also pass explicit fences
//! (`wp_linux_drm_syncobj_v1`). NVIDIA's EGL Wayland platform needs the
//! first to present at all, and uses the second where it can.

use std::{collections::VecDeque, io, path::PathBuf};

use smithay::{
    backend::{
        allocator::{
            Format, Fourcc, Modifier,
            gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
        },
        drm::{DrmDevice, DrmDeviceFd, DrmDeviceNotifier, DrmNode, GbmBufferedSurface, NodeType},
        egl::{EGLContext, EGLDisplay},
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{ImportDma, element::surface::WaylandSurfaceRenderElement, gles::GlesRenderer},
        session::{Session, libseat::LibSeatSession, libseat::LibSeatSessionNotifier},
        udev,
    },
    reexports::{
        drm::control::{Device as _, Mode, ModeTypeFlags, connector, crtc},
        input::Libinput,
        rustix::fs::OFlags,
    },
    utils::{DeviceFd, Physical, Size},
    wayland::{
        dmabuf::{DmabufFeedback, DmabufFeedbackBuilder},
        drm_syncobj::supports_syncobj_eventfd,
    },
};
use tracing::{error, warn};

/// Frames past the one on screen: one waiting for its flip and one queued
/// behind it, which is triple buffering. Smithay's surface holds no more.
const QUEUED_FRAMES: usize = 2;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

/// The GPU, display and seat of a session on the bare seat.
pub(crate) struct Kms {
    pub(crate) session: LibSeatSession,
    pub(crate) drm: DrmDevice,
    pub(crate) renderer: GlesRenderer,
    pub(crate) surface: GbmBufferedSurface<GbmAllocator<DrmDeviceFd>, ()>,
    pub(crate) crtc: crtc::Handle,
    pub(crate) mode: Mode,
    pub(crate) libinput: Libinput,
    /// The node clients should allocate their buffers on: the GPU's render
    /// node, else (a display-only device) the node this session opened.
    node: DrmNode,
    /// Client surfaces drawn into the frame being drawn. Each holds its
    /// client's buffer, and Smithay releases a buffer (`wl_buffer.release`,
    /// and the syncobj release point) as soon as nothing holds it, which is
    /// when the CPU is done with it, not the GPU. They move to `queued`
    /// with their frame, and are let go at the vblank that puts it on
    /// screen: its scanout buffer waited on a fence put after the sampling
    /// on the same context, so the GPU is done with them then. Without this
    /// a client could draw into a buffer the GPU still reads: harmless where
    /// the kernel orders the two through the dma-buf's implicit fences, torn
    /// frames where nothing does.
    pub(crate) in_flight: Vec<WaylandSurfaceRenderElement<GlesRenderer>>,
    /// The frames handed to the surface whose vblank has not come yet,
    /// oldest first, each with the client surfaces it drew. The first waits
    /// for its page flip, a second for the vblank that shows the first.
    queued: VecDeque<Vec<WaylandSurfaceRenderElement<GlesRenderer>>>,
    /// Whether this session holds the seat (it is on the active VT).
    pub(crate) active: bool,
}

/// Event sources the host puts on its loop.
pub(crate) struct Sources {
    pub(crate) session: LibSeatSessionNotifier,
    pub(crate) drm: DrmDeviceNotifier,
    pub(crate) input: LibinputInputBackend,
}

impl Kms {
    /// Takes the seat, the primary GPU and its first connected display.
    pub(crate) fn open() -> Result<(Self, Sources)> {
        let (mut session, session_notifier) = LibSeatSession::new()
            .map_err(|e| format!("cannot take the seat (is this a logind session?): {e}"))?;
        let seat = session.seat();

        let path = gpu(&seat)?;
        let fd = session
            .open(
                &path,
                OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
            )
            .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
        let fd = DrmDeviceFd::new(DeviceFd::from(fd));
        let node = DrmNode::from_file(&fd)?;
        let node = node
            .node_with_type(NodeType::Render)
            .and_then(std::result::Result::ok)
            .unwrap_or(node);
        let (mut drm, drm_notifier) = DrmDevice::new(fd.clone(), true)?;
        let gbm = GbmDevice::new(fd)?;
        // SAFETY: the GBM device lives as long as the display, which keeps a
        // clone of it.
        let egl = unsafe { EGLDisplay::new(gbm.clone())? };
        let context = EGLContext::new(&egl)?;
        let formats = context.dmabuf_render_formats().clone();
        // SAFETY: the context is new and used by nothing else.
        let renderer = unsafe { GlesRenderer::new(context)? };

        let (connector, crtc, mode) = display(&drm)?;
        let allocator = GbmAllocator::new(gbm, GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT);
        // The scanout buffers are allocated with a modifier both the plane
        // and the renderer list, and Smithay test-commits one before taking
        // it. A driver can still list a modifier its display engine refuses
        // for this mode, so, as Smithay's own `DrmOutputManager` does, try
        // again with the implicit modifier only, leaving the layout to the
        // driver's GBM, which knows the buffer is for scanout.
        let surface = match GbmBufferedSurface::new(
            drm.create_surface(crtc, mode, &[connector])?,
            allocator.clone(),
            COLOR_FORMATS,
            formats.iter().copied(),
        ) {
            Ok(surface) => surface,
            Err(e) => {
                warn!(error = %e, "no scanout buffer with explicit modifiers, trying implicit");
                GbmBufferedSurface::new(
                    drm.create_surface(crtc, mode, &[connector])?,
                    allocator,
                    COLOR_FORMATS,
                    implicit(formats.iter().copied()),
                )?
            }
        };

        let mut libinput = Libinput::new_with_udev(LibinputSessionInterface::from(session.clone()));
        libinput
            .udev_assign_seat(&seat)
            .map_err(|()| format!("libinput cannot use seat {seat}"))?;
        let input = LibinputInputBackend::new(libinput.clone());

        let active = session.is_active();
        Ok((
            Self {
                session,
                drm,
                renderer,
                surface,
                crtc,
                mode,
                libinput,
                node,
                in_flight: Vec::new(),
                queued: VecDeque::new(),
                active,
            },
            Sources {
                session: session_notifier,
                drm: drm_notifier,
                input,
            },
        ))
    }

    /// The display's size in pixels.
    pub(crate) fn size(&self) -> Size<i32, Physical> {
        let (w, h) = self.mode.size();
        (i32::from(w), i32::from(h)).into()
    }

    /// The display's refresh rate in millihertz.
    pub(crate) fn refresh_mhz(&self) -> u32 {
        refresh_mhz(&self.mode)
    }

    /// Whether a frame may be drawn now: the seat is ours and the surface
    /// can take another frame (see the module's note on triple buffering).
    pub(crate) fn can_draw(&self) -> bool {
        self.active && self.queued.len() < QUEUED_FRAMES
    }

    /// The frame just drawn was handed to the surface.
    pub(crate) fn queued(&mut self) {
        self.queued.push_back(std::mem::take(&mut self.in_flight));
    }

    /// The seat moved to another VT: let go of the GPU and input devices.
    pub(crate) fn pause(&mut self) {
        self.active = false;
        self.libinput.suspend();
        self.drm.pause();
    }

    /// The seat came back: take the devices again and redraw everything.
    pub(crate) fn resume(&mut self) {
        self.active = true;
        if let Err(e) = self.drm.activate(false) {
            error!(error = %e, "cannot take the display back");
        }
        if self.libinput.resume().is_err() {
            error!("cannot take the input devices back");
        }
        // Buffers queued before the switch never flipped; start clean.
        self.surface.reset_buffers();
        // Their vblank will not come either. The GPU finished those frames
        // while the seat was away, so their clients' buffers can go back.
        self.queued.clear();
        self.in_flight.clear();
    }

    /// A vblank: the queued buffer is on screen.
    pub(crate) fn vblank(&mut self, crtc: crtc::Handle) {
        if crtc != self.crtc {
            return;
        }
        // The frame waiting for this flip is on screen, so the GPU has read
        // every client buffer it drew (see `in_flight`). Smithay then flips
        // the frame queued behind it, if there is one.
        match self.surface.frame_submitted() {
            Ok(Some(())) => {
                self.queued.pop_front();
            }
            Ok(None) => {}
            Err(e) => {
                warn!(error = %e, "page flip failed");
                self.queued.pop_front();
                // The queued frame's flip is what failed, and the surface
                // dropped it. The GPU may still be drawing it, so its
                // client buffers ride with the next frame instead.
                if let Some(lost) = self.queued.pop_front() {
                    self.in_flight.extend(lost);
                }
            }
        }
    }

    /// The `zwp_linux_dmabuf_v1` feedback for every surface: the buffers
    /// this renderer can sample from, allocated on the GPU's render node.
    /// Every client surface is composited into the offscreen frame, none
    /// is scanned out directly, so there is no scanout tranche to offer.
    pub(crate) fn dmabuf_feedback(&self) -> io::Result<DmabufFeedback> {
        DmabufFeedbackBuilder::new(self.node.dev_id(), self.renderer.dmabuf_formats()).build()
    }

    /// The device to import clients' syncobj timelines on, when it can
    /// signal a timeline point through an eventfd: without that a commit
    /// cannot wait for its acquire point, so explicit sync is not offered.
    /// It needs kernel 6.6 or newer and a driver with timeline syncobjs;
    /// virtio-gpu and the software drivers may lack them.
    pub(crate) fn syncobj_device(&self) -> Option<DrmDeviceFd> {
        let fd = self.drm.device_fd();
        supports_syncobj_eventfd(fd).then(|| fd.clone())
    }

    /// Ctrl+Alt+F1–F12.
    pub(crate) fn change_vt(&mut self, vt: i32) {
        if let Err(e) = self.session.change_vt(vt) {
            warn!(vt, error = %e, "cannot switch VT");
        }
    }
}

/// The scanout formats to try, in order.
const COLOR_FORMATS: &[Fourcc] = &[Fourcc::Argb8888, Fourcc::Xrgb8888];

/// The formats among `formats` that leave the layout to the driver.
fn implicit(formats: impl IntoIterator<Item = Format>) -> impl Iterator<Item = Format> {
    formats
        .into_iter()
        .filter(|f| f.modifier == Modifier::Invalid)
}

/// The seat's primary GPU, else its first.
fn gpu(seat: &str) -> Result<PathBuf> {
    if let Some(path) = udev::primary_gpu(seat)? {
        return Ok(path);
    }
    udev::all_gpus(seat)?
        .into_iter()
        .next()
        .ok_or_else(|| format!("no GPU on seat {seat}").into())
}

fn refresh_mhz(mode: &Mode) -> u32 {
    refresh_from_timings(
        mode.clock(),
        mode.hsync().2,
        mode.vsync().2,
        mode.vrefresh(),
    )
}

/// The refresh rate in millihertz from a mode's pixel clock (kHz) and
/// totals; `vrefresh` is rounded to whole hertz, the timings are exact.
fn refresh_from_timings(clock_khz: u32, htotal: u16, vtotal: u16, vrefresh: u32) -> u32 {
    let total = u64::from(htotal) * u64::from(vtotal);
    if total == 0 {
        return vrefresh.max(1) * 1000;
    }
    (u64::from(clock_khz) * 1_000_000 / total) as u32
}

/// The first connected connector, its preferred mode (else its first), and a
/// CRTC that can drive it.
fn display(drm: &DrmDevice) -> Result<(connector::Handle, crtc::Handle, Mode)> {
    let resources = drm.resource_handles()?;
    for &handle in resources.connectors() {
        let Ok(info) = drm.get_connector(handle, false) else {
            continue;
        };
        if info.state() != connector::State::Connected || info.modes().is_empty() {
            continue;
        }
        let mode = info
            .modes()
            .iter()
            .find(|m| m.mode_type().contains(ModeTypeFlags::PREFERRED))
            .unwrap_or(&info.modes()[0]);
        for encoder in info.encoders() {
            let Ok(encoder) = drm.get_encoder(*encoder) else {
                continue;
            };
            if let Some(&crtc) = resources.filter_crtcs(encoder.possible_crtcs()).first() {
                return Ok((handle, crtc, *mode));
            }
        }
    }
    Err("no connected display".into())
}

/// Whether to run on the bare seat: `MCSAPI_BACKEND=kms` or `=winit` decides;
/// otherwise when there is no Wayland or X11 session to nest in.
pub(crate) fn wanted() -> bool {
    match std::env::var("MCSAPI_BACKEND").as_deref() {
        Ok("kms" | "drm") => true,
        Ok(_) => false,
        Err(_) => {
            std::env::var_os("WAYLAND_DISPLAY").is_none() && std::env::var_os("DISPLAY").is_none()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_from_the_mode_timings() {
        // 1920x1080@60 (CEA): 148.5 MHz over 2200 x 1125.
        assert_eq!(refresh_from_timings(148_500, 2200, 1125, 60), 60_000);
        // 59.94 Hz shows up as 59 or 60 in vrefresh, exactly here.
        assert_eq!(refresh_from_timings(148_352, 2200, 1125, 60), 59_940);
        assert_eq!(refresh_from_timings(0, 0, 0, 75), 75_000);
    }
}
