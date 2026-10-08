//! Screen capture for screenshot tools, recorders, VNC servers and the
//! screen-cast portal, all hidden from sandboxed clients (a Flatpak app
//! asks the portal, which asks the user):
//!
//! - `ext_image_copy_capture_manager_v1`, with sources from
//!   `ext_output_image_capture_source_manager_v1` (the screen) and
//!   `ext_foreign_toplevel_image_capture_source_manager_v1` (one window, by
//!   its `ext_foreign_toplevel_list_v1` handle): xdg-desktop-portal-wlr,
//!   grim.
//! - `zwlr_screencopy_manager_v1`: grim, wf-recorder, wayvnc.
//!
//! A capture is copied into the client's `wl_shm` buffer (XRGB, ARGB, XBGR
//! or ABGR 8888) from the next frame drawn, with or without the pointer as
//! the client asked; there are no dmabuf captures yet. A window is cut out
//! of the frame where its content is placed, so whatever covers it (the
//! chrome's popups) is in the picture too, and a window that is not placed
//! (minimized, on another workspace) waits until it is. Damage is not
//! tracked: every frame counts as changed.

use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use smithay::{
    reexports::{
        wayland_protocols::ext::{
            foreign_toplevel_list::v1::server::ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
            image_capture_source::v1::server::{
                ext_foreign_toplevel_image_capture_source_manager_v1::{
                    self, ExtForeignToplevelImageCaptureSourceManagerV1,
                },
                ext_image_capture_source_v1::{self, ExtImageCaptureSourceV1},
                ext_output_image_capture_source_manager_v1::{
                    self, ExtOutputImageCaptureSourceManagerV1,
                },
            },
            image_copy_capture::v1::server::{
                ext_image_copy_capture_cursor_session_v1::{
                    self, ExtImageCopyCaptureCursorSessionV1,
                },
                ext_image_copy_capture_frame_v1::{
                    self, ExtImageCopyCaptureFrameV1, FailureReason,
                },
                ext_image_copy_capture_manager_v1::{self, ExtImageCopyCaptureManagerV1, Options},
                ext_image_copy_capture_session_v1::{self, ExtImageCopyCaptureSessionV1},
            },
        },
        wayland_protocols_wlr::screencopy::v1::server::{
            zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1},
            zwlr_screencopy_manager_v1::{self, ZwlrScreencopyManagerV1},
        },
        wayland_server::{
            Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
            backend::ClientId,
            protocol::{wl_buffer::WlBuffer, wl_output::Transform, wl_shm::Format},
        },
    },
    utils::{Clock, Logical, Monotonic, Rectangle},
    wayland::{foreign_toplevel_list::ForeignToplevelHandle, shm},
};
use tracing::warn;

use super::{Host, security::unsandboxed};
use crate::{Capture, Shell, WindowId};

/// The shm formats a capture can be written in.
const FORMATS: [Format; 4] = [
    Format::Xrgb8888,
    Format::Argb8888,
    Format::Xbgr8888,
    Format::Abgr8888,
];

/// What a capture shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Source {
    Output,
    Window(WindowId),
    /// A window that was gone before the source was made.
    Gone,
}

/// An ext capture session's settings and the size it last announced.
struct SessionData {
    source: Source,
    cursor: bool,
    size: Mutex<Option<(i32, i32)>>,
}

/// An ext frame: its buffer, once attached, and whether it was captured.
struct ExtFrameData {
    session: ExtImageCopyCaptureSessionV1,
    buffer: Mutex<Option<WlBuffer>>,
    captured: AtomicBool,
}

/// A wlr frame's settings, and whether it was copied into already.
struct WlrFrameData {
    cursor: bool,
    region: Rectangle<i32, Logical>,
    used: AtomicBool,
}

enum Target {
    Ext(ExtImageCopyCaptureFrameV1),
    Wlr {
        frame: ZwlrScreencopyFrameV1,
        damage: bool,
    },
}

/// A copy waiting for the next frame.
struct Request {
    target: Target,
    source: Source,
    /// For wlr captures of a region of the output.
    region: Option<Rectangle<i32, Logical>>,
    cursor: bool,
    buffer: WlBuffer,
}

#[derive(Default)]
pub(super) struct Captures {
    pending: Vec<Request>,
    sessions: Vec<ExtImageCopyCaptureSessionV1>,
}

impl Captures {
    pub(super) fn new<S: Shell + 'static>(dh: &DisplayHandle) -> Self {
        dh.create_global::<Host<S>, ExtOutputImageCaptureSourceManagerV1, ()>(1, ());
        dh.create_global::<Host<S>, ExtForeignToplevelImageCaptureSourceManagerV1, ()>(1, ());
        dh.create_global::<Host<S>, ExtImageCopyCaptureManagerV1, ()>(1, ());
        dh.create_global::<Host<S>, ZwlrScreencopyManagerV1, ()>(3, ());
        Self::default()
    }

    /// Whether a capture waits for a frame drawn with (or without) the
    /// pointer.
    pub(super) fn wants(&self, cursor: bool) -> bool {
        self.pending.iter().any(|r| r.cursor == cursor)
    }
}

/// The buffer's layout, if it is shm in a format we write and fits `size`.
fn fits(buffer: &WlBuffer, size: (i32, i32)) -> bool {
    shm::with_buffer_contents(buffer, |_, len, data| {
        data.width == size.0
            && data.height == size.1
            && data.stride >= data.width * 4
            && FORMATS.contains(&data.format)
            && (data.offset as usize + (data.stride * data.height) as usize) <= len
    })
    .unwrap_or(false)
}

/// Copies `rect` of `frame` into `buffer`.
fn write(buffer: &WlBuffer, frame: &Capture, rect: Rectangle<i32, Logical>) -> bool {
    let result = shm::with_buffer_contents_mut(buffer, |ptr, len, data| {
        // SAFETY: Smithay hands out the pool's whole mapping, `len` bytes
        // long, for the duration of this call.
        let pool = unsafe { std::slice::from_raw_parts_mut(ptr, len) };
        let swap = matches!(data.format, Format::Xrgb8888 | Format::Argb8888);
        let src_row = frame.width as usize * 4;
        for y in 0..rect.size.h.min(data.height) {
            let src_y = (rect.loc.y + y) as usize;
            if src_y >= frame.height as usize {
                break;
            }
            let start = src_y * src_row + rect.loc.x as usize * 4;
            let width = (rect.size.w.min(data.width) as usize)
                .min(frame.width as usize - rect.loc.x as usize);
            let src = &frame.rgba[start..start + width * 4];
            let dst_start = data.offset as usize + y as usize * data.stride as usize;
            let Some(dst) = pool.get_mut(dst_start..dst_start + width * 4) else {
                break;
            };
            for (d, s) in dst
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(src.as_chunks::<4>().0)
            {
                // ARGB and XRGB are B, G, R, A in memory; ABGR and XBGR
                // are R, G, B, A, as read back.
                *d = if swap { [s[2], s[1], s[0], s[3]] } else { *s };
            }
        }
    });
    result.is_ok()
}

/// Seconds (split in two) and nanoseconds of the monotonic clock now.
fn timestamp() -> (u32, u32, u32) {
    let now = Duration::from(Clock::<Monotonic>::new().now());
    let secs = now.as_secs();
    ((secs >> 32) as u32, secs as u32, now.subsec_nanos())
}

impl<S: Shell + 'static> Host<S> {
    /// Where `source` is in the frame, if it is on screen.
    fn capture_rect(&self, source: Source) -> Option<Rectangle<i32, Logical>> {
        match source {
            Source::Output => Some(Rectangle::from_size(self.backend.size().to_logical(1))),
            Source::Window(id) => self
                .shell
                .placements()
                .into_iter()
                .find(|p| p.window == id)
                .map(|p| Rectangle::new(p.client.loc, p.client.size))
                .filter(|_| !self.lock.locked),
            Source::Gone => None,
        }
    }

    /// The size a session's buffers must have, which for a window not on
    /// screen is the size it last had.
    fn session_size(&self, session: &ExtImageCopyCaptureSessionV1) -> Option<(i32, i32)> {
        let data = session.data::<SessionData>()?;
        let last = *data.size.lock().unwrap_or_else(|e| e.into_inner());
        match data.source {
            Source::Window(id) if !self.windows.contains_key(&id) => None,
            source => self
                .capture_rect(source)
                .map(|r| (r.size.w, r.size.h))
                .or(last),
        }
    }

    /// Tells sessions about new sizes, and stops those whose window is
    /// gone; called after every change.
    pub(super) fn update_capture_sessions(&mut self) {
        let sessions = std::mem::take(&mut self.screen_capture.sessions);
        let mut alive = Vec::with_capacity(sessions.len());
        for session in sessions {
            let Some(data) = session.data::<SessionData>() else {
                continue;
            };
            match self.session_size(&session) {
                None if !matches!(data.source, Source::Output) => {
                    session.stopped();
                    continue;
                }
                None => {}
                Some(size) => {
                    let mut last = data.size.lock().unwrap_or_else(|e| e.into_inner());
                    if *last != Some(size) {
                        *last = Some(size);
                        send_constraints(&session, size);
                    }
                }
            }
            alive.push(session);
        }
        self.screen_capture.sessions = alive;
    }

    /// Fills the captures that wait for this frame: `bare` was read before
    /// the pointer was drawn, `with_cursor` after.
    pub(super) fn deliver_captures(
        &mut self,
        bare: Option<&Capture>,
        with_cursor: Option<&Capture>,
    ) {
        let (sec_hi, sec_lo, nsec) = timestamp();
        let pending = std::mem::take(&mut self.screen_capture.pending);
        for request in pending {
            let frame = if request.cursor { with_cursor } else { bare };
            let rect = match (request.region, self.capture_rect(request.source)) {
                (_, None) => {
                    // A window that is not on screen waits; anything else
                    // has nothing to show any more.
                    if matches!(request.source, Source::Window(id) if self.windows.contains_key(&id))
                    {
                        self.screen_capture.pending.push(request);
                    } else {
                        fail(&request.target, FailureReason::Stopped);
                    }
                    continue;
                }
                (Some(region), Some(_)) => region,
                (None, Some(rect)) => rect,
            };
            let Some(frame) = frame else {
                self.screen_capture.pending.push(request);
                continue;
            };
            if !fits(&request.buffer, (rect.size.w, rect.size.h)) {
                fail(&request.target, FailureReason::BufferConstraints);
                continue;
            }
            if !write(&request.buffer, frame, rect) {
                fail(&request.target, FailureReason::Unknown);
                continue;
            }
            let (w, h) = (rect.size.w, rect.size.h);
            match request.target {
                Target::Ext(frame) => {
                    frame.transform(Transform::Normal);
                    frame.damage(0, 0, w, h);
                    frame.presentation_time(sec_hi, sec_lo, nsec);
                    frame.ready();
                }
                Target::Wlr { frame, damage } => {
                    frame.flags(zwlr_screencopy_frame_v1::Flags::empty());
                    if damage {
                        frame.damage(0, 0, w as u32, h as u32);
                    }
                    frame.ready(sec_hi, sec_lo, nsec);
                }
            }
        }
    }
}

fn fail(target: &Target, reason: FailureReason) {
    match target {
        Target::Ext(frame) => frame.failed(reason),
        Target::Wlr { frame, .. } => frame.failed(),
    }
}

/// Sends a session what its buffers must be.
fn send_constraints(session: &ExtImageCopyCaptureSessionV1, (w, h): (i32, i32)) {
    session.buffer_size(w.max(1) as u32, h.max(1) as u32);
    for format in FORMATS {
        session.shm_format(format);
    }
    session.done();
}

impl<S: Shell + 'static> GlobalDispatch<ExtOutputImageCaptureSourceManagerV1, ()> for Host<S> {
    fn bind(
        _host: &mut Self,
        _dh: &DisplayHandle,
        _client: &Client,
        resource: New<ExtOutputImageCaptureSourceManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }

    fn can_view(client: Client, _global_data: &()) -> bool {
        unsandboxed(&client)
    }
}

impl<S: Shell + 'static> Dispatch<ExtOutputImageCaptureSourceManagerV1, ()> for Host<S> {
    fn request(
        _host: &mut Self,
        _client: &Client,
        _manager: &ExtOutputImageCaptureSourceManagerV1,
        request: ext_output_image_capture_source_manager_v1::Request,
        _data: &(),
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        // There is one output, whichever the client names.
        if let ext_output_image_capture_source_manager_v1::Request::CreateSource {
            source, ..
        } = request
        {
            data_init.init(source, Source::Output);
        }
    }
}

impl<S: Shell + 'static> GlobalDispatch<ExtForeignToplevelImageCaptureSourceManagerV1, ()>
    for Host<S>
{
    fn bind(
        _host: &mut Self,
        _dh: &DisplayHandle,
        _client: &Client,
        resource: New<ExtForeignToplevelImageCaptureSourceManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }

    fn can_view(client: Client, _global_data: &()) -> bool {
        unsandboxed(&client)
    }
}

impl<S: Shell + 'static> Dispatch<ExtForeignToplevelImageCaptureSourceManagerV1, ()> for Host<S> {
    fn request(
        host: &mut Self,
        _client: &Client,
        _manager: &ExtForeignToplevelImageCaptureSourceManagerV1,
        request: ext_foreign_toplevel_image_capture_source_manager_v1::Request,
        _data: &(),
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let ext_foreign_toplevel_image_capture_source_manager_v1::Request::CreateSource {
            source,
            toplevel_handle,
        } = request
        {
            let window = host.window_of_foreign(&toplevel_handle);
            data_init.init(source, window.map_or(Source::Gone, Source::Window));
        }
    }
}

impl<S: Shell + 'static> Host<S> {
    fn window_of_foreign(&self, handle: &ExtForeignToplevelHandleV1) -> Option<WindowId> {
        let handle = ForeignToplevelHandle::from_resource(handle)?;
        self.foreign_window(&handle.identifier())
    }
}

impl<S: Shell + 'static> Dispatch<ExtImageCaptureSourceV1, Source> for Host<S> {
    fn request(
        _host: &mut Self,
        _client: &Client,
        _source: &ExtImageCaptureSourceV1,
        _request: ext_image_capture_source_v1::Request,
        _data: &Source,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}

impl<S: Shell + 'static> GlobalDispatch<ExtImageCopyCaptureManagerV1, ()> for Host<S> {
    fn bind(
        _host: &mut Self,
        _dh: &DisplayHandle,
        _client: &Client,
        resource: New<ExtImageCopyCaptureManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }

    fn can_view(client: Client, _global_data: &()) -> bool {
        unsandboxed(&client)
    }
}

impl<S: Shell + 'static> Dispatch<ExtImageCopyCaptureManagerV1, ()> for Host<S> {
    fn request(
        host: &mut Self,
        _client: &Client,
        manager: &ExtImageCopyCaptureManagerV1,
        request: ext_image_copy_capture_manager_v1::Request,
        _data: &(),
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            ext_image_copy_capture_manager_v1::Request::CreateSession {
                session,
                source,
                options,
            } => {
                let cursor = match options.into_result() {
                    Ok(options) => options.contains(Options::PaintCursors),
                    Err(_) => {
                        manager.post_error(
                            ext_image_copy_capture_manager_v1::Error::InvalidOption,
                            "unknown option",
                        );
                        return;
                    }
                };
                let source = source.data::<Source>().copied().unwrap_or(Source::Gone);
                let session = data_init.init(
                    session,
                    SessionData {
                        source,
                        cursor,
                        size: Mutex::new(None),
                    },
                );
                host.screen_capture.sessions.push(session);
                host.update_capture_sessions();
            }
            // Cursor images are drawn into the frame when asked for, not
            // offered on their own.
            ext_image_copy_capture_manager_v1::Request::CreatePointerCursorSession {
                session,
                ..
            } => {
                data_init.init(session, ());
            }
            _ => {}
        }
    }
}

impl<S: Shell + 'static> Dispatch<ExtImageCopyCaptureCursorSessionV1, ()> for Host<S> {
    fn request(
        _host: &mut Self,
        _client: &Client,
        _cursor: &ExtImageCopyCaptureCursorSessionV1,
        request: ext_image_copy_capture_cursor_session_v1::Request,
        _data: &(),
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let ext_image_copy_capture_cursor_session_v1::Request::GetCaptureSession { session } =
            request
        {
            let session = data_init.init(
                session,
                SessionData {
                    source: Source::Gone,
                    cursor: false,
                    size: Mutex::new(None),
                },
            );
            session.stopped();
        }
    }
}

impl<S: Shell + 'static> Dispatch<ExtImageCopyCaptureSessionV1, SessionData> for Host<S> {
    fn request(
        _host: &mut Self,
        _client: &Client,
        session: &ExtImageCopyCaptureSessionV1,
        request: ext_image_copy_capture_session_v1::Request,
        _data: &SessionData,
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let ext_image_copy_capture_session_v1::Request::CreateFrame { frame } = request {
            data_init.init(
                frame,
                ExtFrameData {
                    session: session.clone(),
                    buffer: Mutex::new(None),
                    captured: AtomicBool::new(false),
                },
            );
        }
    }

    fn destroyed(
        host: &mut Self,
        _client: ClientId,
        session: &ExtImageCopyCaptureSessionV1,
        _data: &SessionData,
    ) {
        host.screen_capture.sessions.retain(|s| s != session);
    }
}

impl<S: Shell + 'static> Dispatch<ExtImageCopyCaptureFrameV1, ExtFrameData> for Host<S> {
    fn request(
        host: &mut Self,
        _client: &Client,
        frame: &ExtImageCopyCaptureFrameV1,
        request: ext_image_copy_capture_frame_v1::Request,
        data: &ExtFrameData,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        use ext_image_copy_capture_frame_v1::{Error, Request as R};
        match request {
            R::AttachBuffer { buffer } => {
                *data.buffer.lock().unwrap_or_else(|e| e.into_inner()) = Some(buffer);
            }
            // The whole buffer is written every time.
            R::DamageBuffer {
                x,
                y,
                width,
                height,
            } => {
                if x < 0 || y < 0 || width <= 0 || height <= 0 {
                    frame.post_error(Error::InvalidBufferDamage, "damage out of range");
                }
            }
            R::Capture => {
                if data.captured.swap(true, Ordering::Relaxed) {
                    frame.post_error(Error::AlreadyCaptured, "frame was already captured");
                    return;
                }
                let Some(buffer) = data
                    .buffer
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone()
                else {
                    frame.post_error(Error::NoBuffer, "no buffer attached");
                    return;
                };
                let Some(session) = data.session.data::<SessionData>() else {
                    return;
                };
                if !data.session.is_alive() || session.source == Source::Gone {
                    frame.failed(FailureReason::Stopped);
                    return;
                }
                host.screen_capture.pending.push(Request {
                    target: Target::Ext(frame.clone()),
                    source: session.source,
                    region: None,
                    cursor: session.cursor,
                    buffer,
                });
            }
            _ => {}
        }
    }

    fn destroyed(
        host: &mut Self,
        _client: ClientId,
        frame: &ExtImageCopyCaptureFrameV1,
        _data: &ExtFrameData,
    ) {
        host.screen_capture
            .pending
            .retain(|r| !matches!(&r.target, Target::Ext(f) if f == frame));
    }
}

impl<S: Shell + 'static> GlobalDispatch<ZwlrScreencopyManagerV1, ()> for Host<S> {
    fn bind(
        _host: &mut Self,
        _dh: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrScreencopyManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }

    fn can_view(client: Client, _global_data: &()) -> bool {
        unsandboxed(&client)
    }
}

impl<S: Shell + 'static> Dispatch<ZwlrScreencopyManagerV1, ()> for Host<S> {
    fn request(
        host: &mut Self,
        _client: &Client,
        _manager: &ZwlrScreencopyManagerV1,
        request: zwlr_screencopy_manager_v1::Request,
        _data: &(),
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        let output = Rectangle::from_size(host.backend.size().to_logical(1));
        let (frame, cursor, region) = match request {
            zwlr_screencopy_manager_v1::Request::CaptureOutput {
                frame,
                overlay_cursor,
                ..
            } => (frame, overlay_cursor != 0, output),
            zwlr_screencopy_manager_v1::Request::CaptureOutputRegion {
                frame,
                overlay_cursor,
                x,
                y,
                width,
                height,
                ..
            } => {
                let asked = Rectangle::new((x, y).into(), (width, height).into());
                let region = asked.intersection(output).unwrap_or_default();
                (frame, overlay_cursor != 0, region)
            }
            _ => return,
        };
        let frame = data_init.init(
            frame,
            WlrFrameData {
                cursor,
                region,
                used: AtomicBool::new(false),
            },
        );
        if region.is_empty() {
            frame.failed();
            return;
        }
        let (w, h) = (region.size.w as u32, region.size.h as u32);
        frame.buffer(Format::Xrgb8888, w, h, w * 4);
        if frame.version() >= 3 {
            frame.buffer_done();
        }
    }
}

impl<S: Shell + 'static> Dispatch<ZwlrScreencopyFrameV1, WlrFrameData> for Host<S> {
    fn request(
        host: &mut Self,
        _client: &Client,
        frame: &ZwlrScreencopyFrameV1,
        request: zwlr_screencopy_frame_v1::Request,
        data: &WlrFrameData,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        let (buffer, damage) = match request {
            zwlr_screencopy_frame_v1::Request::Copy { buffer } => (buffer, false),
            zwlr_screencopy_frame_v1::Request::CopyWithDamage { buffer } => (buffer, true),
            _ => return,
        };
        if data.used.swap(true, Ordering::Relaxed) {
            frame.post_error(
                zwlr_screencopy_frame_v1::Error::AlreadyUsed,
                "frame was already used",
            );
            return;
        }
        if !fits(&buffer, (data.region.size.w, data.region.size.h)) {
            warn!("screencopy buffer does not match");
            frame.post_error(
                zwlr_screencopy_frame_v1::Error::InvalidBuffer,
                "buffer does not match the announced one",
            );
            return;
        }
        host.screen_capture.pending.push(Request {
            target: Target::Wlr {
                frame: frame.clone(),
                damage,
            },
            source: Source::Output,
            region: Some(data.region),
            cursor: data.cursor,
            buffer,
        });
    }

    fn destroyed(
        host: &mut Self,
        _client: ClientId,
        frame: &ZwlrScreencopyFrameV1,
        _data: &WlrFrameData,
    ) {
        host.screen_capture
            .pending
            .retain(|r| !matches!(&r.target, Target::Wlr { frame: f, .. } if f == frame));
    }
}
