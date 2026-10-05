//! Running on the bare seat: DRM/KMS output, libinput input, and seat access
//! through libseat (logind, or seatd).
//!
//! The nested backend draws into a window of another session. This one is
//! for when there is no other session: a display manager's greeter, or a
//! desktop started from a login. It drives the first connected display at
//! its preferred mode through GBM buffers, and gives up the GPU and input
//! devices whenever the seat moves to another VT.
//!
//! The frame is drawn exactly as for the nested window, into an offscreen
//! texture, and then copied onto the scanout buffer. The chrome and the blur
//! are painted with raw GL, which puts row 0 at the bottom as a window
//! surface does, while a scanout buffer shows row 0 at the top; drawing them
//! straight into it would turn them upside down under the client windows.
//! One extra full-screen copy keeps every layer of the frame identical
//! between the two backends.

use std::path::PathBuf;

use smithay::{
    backend::{
        allocator::{
            Fourcc,
            gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
        },
        drm::{DrmDevice, DrmDeviceFd, DrmDeviceNotifier, GbmBufferedSurface},
        egl::{EGLContext, EGLDisplay},
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::gles::GlesRenderer,
        session::{Session, libseat::LibSeatSession, libseat::LibSeatSessionNotifier},
        udev,
    },
    reexports::{
        drm::control::{Device as _, Mode, ModeTypeFlags, connector, crtc},
        input::Libinput,
        rustix::fs::OFlags,
    },
    utils::{DeviceFd, Physical, Size},
};

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
    /// A buffer is queued for scanout and its vblank hasn't come yet; the
    /// next frame waits for it, which paces drawing to the display.
    pub(crate) frame_pending: bool,
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
        let surface = drm.create_surface(crtc, mode, &[connector])?;
        let allocator = GbmAllocator::new(gbm, GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT);
        let surface = GbmBufferedSurface::new(
            surface,
            allocator,
            &[Fourcc::Argb8888, Fourcc::Xrgb8888],
            formats,
        )?;

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
                frame_pending: false,
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

    /// Whether a frame may be drawn now.
    pub(crate) fn can_draw(&self) -> bool {
        self.active && !self.frame_pending
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
            eprintln!("mcsapi-compositor: cannot take the display back: {e}");
        }
        if self.libinput.resume().is_err() {
            eprintln!("mcsapi-compositor: cannot take the input devices back");
        }
        // Buffers queued before the switch never flipped; start clean.
        self.surface.reset_buffers();
        self.frame_pending = false;
    }

    /// A vblank: the queued buffer is on screen.
    pub(crate) fn vblank(&mut self, crtc: crtc::Handle) {
        if crtc != self.crtc {
            return;
        }
        if let Err(e) = self.surface.frame_submitted() {
            eprintln!("mcsapi-compositor: page flip failed: {e}");
        }
        self.frame_pending = false;
    }

    /// Ctrl+Alt+F1–F12.
    pub(crate) fn change_vt(&mut self, vt: i32) {
        if let Err(e) = self.session.change_vt(vt) {
            eprintln!("mcsapi-compositor: cannot switch to VT {vt}: {e}");
        }
    }
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
