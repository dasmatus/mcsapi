//! On-demand Xwayland: the X11 display is reserved at startup, but the
//! server only starts when an X11 client connects, and it is started again
//! the next time after it exits (Xwayland runs with `-terminate`, so it quits
//! once its last X11 client is gone).
//!
//! Smithay binds the X11 sockets itself when it spawns Xwayland, so the
//! client that triggers the start (and any that connect in the same instant)
//! is accepted here and relayed to the real server, file descriptors
//! included.

use std::{
    borrow::Cow,
    io::{self, IoSlice, IoSliceMut, Write as _},
    mem::MaybeUninit,
    net::Shutdown,
    os::{
        fd::{AsFd, OwnedFd},
        linux::net::SocketAddrExt,
        unix::net::{SocketAddr, UnixListener, UnixStream},
    },
    path::PathBuf,
};

use rustix::net::{
    RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags, SendAncillaryBuffer,
    SendAncillaryMessage, SendFlags,
};
use smithay::{
    input::{
        Seat, SeatHandler,
        keyboard::{KeyboardTarget, KeysymHandle, ModifiersState},
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{IsAlive, Serial},
    wayland::seat::WaylandFocus,
    xwayland::X11Surface,
};

/// Keyboard focus: a Wayland surface or an X11 window (which also needs X
/// input focus).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Focus {
    Wayland(WlSurface),
    X11(X11Surface),
}

impl IsAlive for Focus {
    fn alive(&self) -> bool {
        match self {
            Self::Wayland(s) => s.alive(),
            Self::X11(s) => s.alive(),
        }
    }
}

impl WaylandFocus for Focus {
    fn wl_surface(&self) -> Option<Cow<'_, WlSurface>> {
        match self {
            Self::Wayland(s) => Some(Cow::Borrowed(s)),
            Self::X11(s) => s.wl_surface().map(Cow::Owned),
        }
    }
}

impl<D: SeatHandler + 'static> KeyboardTarget<D> for Focus {
    fn enter(&self, seat: &Seat<D>, data: &mut D, keys: Vec<KeysymHandle<'_>>, serial: Serial) {
        match self {
            Self::Wayland(s) => KeyboardTarget::enter(s, seat, data, keys, serial),
            Self::X11(s) => KeyboardTarget::enter(s, seat, data, keys, serial),
        }
    }

    fn leave(&self, seat: &Seat<D>, data: &mut D, serial: Serial) {
        match self {
            Self::Wayland(s) => KeyboardTarget::leave(s, seat, data, serial),
            Self::X11(s) => KeyboardTarget::leave(s, seat, data, serial),
        }
    }

    fn key(
        &self,
        seat: &Seat<D>,
        data: &mut D,
        key: KeysymHandle<'_>,
        state: smithay::backend::input::KeyState,
        serial: Serial,
        time: u32,
    ) {
        match self {
            Self::Wayland(s) => KeyboardTarget::key(s, seat, data, key, state, serial, time),
            Self::X11(s) => KeyboardTarget::key(s, seat, data, key, state, serial, time),
        }
    }

    fn modifiers(&self, seat: &Seat<D>, data: &mut D, modifiers: ModifiersState, serial: Serial) {
        match self {
            Self::Wayland(s) => KeyboardTarget::modifiers(s, seat, data, modifiers, serial),
            Self::X11(s) => KeyboardTarget::modifiers(s, seat, data, modifiers, serial),
        }
    }
}

/// Whether an `Xwayland` executable is on `PATH`.
pub(crate) fn available() -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join("Xwayland").is_file()))
}

fn socket_path(display: u32) -> PathBuf {
    PathBuf::from(format!("/tmp/.X11-unix/X{display}"))
}

fn lock_path(display: u32) -> PathBuf {
    PathBuf::from(format!("/tmp/.X{display}-lock"))
}

/// An X11 display reserved while Xwayland is not running: its lock file and
/// listening sockets (filesystem and abstract).
pub(crate) struct Reservation {
    pub(crate) display: u32,
    pub(crate) listeners: Vec<UnixListener>,
}

impl Reservation {
    /// Reserves `display`, or the first free display from 0 when `None`.
    pub(crate) fn new(display: Option<u32>) -> io::Result<Self> {
        let candidates: Vec<u32> = match display {
            Some(d) => vec![d],
            None => (0..33).collect(),
        };
        let mut last = io::Error::new(io::ErrorKind::AddrInUse, "no free X11 display");
        for d in candidates {
            match Self::try_display(d) {
                Ok(r) => return Ok(r),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    fn try_display(display: u32) -> io::Result<Self> {
        let lock = lock_path(display);
        let mut file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock)
        {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists && stale_lock(display) => {
                std::fs::remove_file(&lock)?;
                return Self::try_display(display);
            }
            Err(e) => return Err(e),
        };
        let mut bind = || -> io::Result<Vec<UnixListener>> {
            writeln!(file, "{:>10}", std::process::id())?;
            std::fs::create_dir_all("/tmp/.X11-unix")?;
            let path = socket_path(display);
            let _ = std::fs::remove_file(&path);
            let fs = UnixListener::bind(&path)?;
            let name = path.as_os_str().as_encoded_bytes();
            let abs = UnixListener::bind_addr(&SocketAddr::from_abstract_name(name)?)?;
            for l in [&fs, &abs] {
                l.set_nonblocking(true)?;
            }
            Ok(vec![fs, abs])
        };
        match bind() {
            Ok(listeners) => Ok(Self { display, listeners }),
            Err(e) => {
                let _ = std::fs::remove_file(&lock);
                Err(e)
            }
        }
    }

    /// Accepts every pending connection, then gives the display up so
    /// Xwayland can claim it. Returns the accepted clients.
    pub(crate) fn release(self) -> Vec<UnixStream> {
        let mut accepted = Vec::new();
        for listener in &self.listeners {
            while let Ok((stream, _)) = listener.accept() {
                accepted.push(stream);
            }
        }
        drop(self.listeners);
        let _ = std::fs::remove_file(socket_path(self.display));
        let _ = std::fs::remove_file(lock_path(self.display));
        accepted
    }
}

/// Whether a lock file names a process that no longer exists.
fn stale_lock(display: u32) -> bool {
    let Ok(text) = std::fs::read_to_string(lock_path(display)) else {
        return false;
    };
    let Ok(pid) = text.trim().parse::<u32>() else {
        return false;
    };
    !std::path::Path::new(&format!("/proc/{pid}")).exists()
}

/// How many clients other than the window manager and this probe are
/// connected to the X server on `display`.
pub(crate) fn other_clients(display: u32) -> Option<usize> {
    use x11rb::{connection::RequestConnection as _, protocol::res::ConnectionExt as _};
    let (conn, _) =
        x11rb::rust_connection::RustConnection::connect(Some(&format!(":{display}"))).ok()?;
    conn.extension_information(x11rb::protocol::res::X11_EXTENSION_NAME)
        .ok()??;
    let clients = conn.res_query_clients().ok()?.reply().ok()?.clients;
    // The server's own client has resource base 0.
    let connected = clients.iter().filter(|c| c.resource_base != 0).count();
    Some(connected.saturating_sub(2))
}

/// The Xwayland child of this process serving `display`, found in /proc.
pub(crate) fn server_pid(display: u32) -> Option<rustix::process::Pid> {
    let me = std::process::id().to_string();
    let wanted = format!(":{display}");
    std::fs::read_dir("/proc")
        .ok()?
        .flatten()
        .find_map(|entry| {
            let pid: i32 = entry.file_name().to_str()?.parse().ok()?;
            let stat = std::fs::read_to_string(entry.path().join("stat")).ok()?;
            // pid (comm) state ppid ...
            let rest = &stat[stat.rfind(')')? + 2..];
            let ppid = rest.split(' ').nth(1)?;
            let cmdline = std::fs::read(entry.path().join("cmdline")).ok()?;
            let mut args = cmdline.split(|b| *b == 0);
            let program = args.next()?;
            let is_xwayland = program.ends_with(b"Xwayland");
            (ppid == me && is_xwayland && args.next() == Some(wanted.as_bytes()))
                .then(|| rustix::process::Pid::from_raw(pid))
                .flatten()
        })
}

/// Relays an early client to the running server on `display`, both ways,
/// passing file descriptors along (MIT-SHM and DRI3 use them).
pub(crate) fn relay(client: UnixStream, display: u32) -> io::Result<()> {
    let server = UnixStream::connect(socket_path(display))?;
    for s in [&client, &server] {
        s.set_nonblocking(false)?;
    }
    let (client2, server2) = (client.try_clone()?, server.try_clone()?);
    std::thread::Builder::new()
        .name("xwayland-relay".into())
        .spawn(move || pump(&client, &server))?;
    std::thread::Builder::new()
        .name("xwayland-relay".into())
        .spawn(move || pump(&server2, &client2))?;
    Ok(())
}

/// Copies bytes and descriptors from `from` to `to` until `from` closes.
fn pump(from: &UnixStream, to: &UnixStream) {
    let mut buf = vec![0u8; 64 * 1024];
    let mut space = [MaybeUninit::<u8>::uninit(); rustix::cmsg_space!(ScmRights(32))];
    loop {
        let mut control = RecvAncillaryBuffer::new(&mut space);
        let received = rustix::net::recvmsg(
            from,
            &mut [IoSliceMut::new(&mut buf)],
            &mut control,
            RecvFlags::CMSG_CLOEXEC,
        );
        let n = match received {
            Ok(msg) if msg.bytes > 0 => msg.bytes,
            Err(rustix::io::Errno::INTR) => continue,
            _ => break,
        };
        let fds: Vec<OwnedFd> = control
            .drain()
            .flat_map(|m| match m {
                RecvAncillaryMessage::ScmRights(fds) => fds.collect(),
                _ => Vec::new(),
            })
            .collect();
        if send_all(to, &buf[..n], &fds).is_err() {
            break;
        }
    }
    let _ = to.shutdown(Shutdown::Write);
    let _ = from.shutdown(Shutdown::Read);
}

fn send_all(to: &UnixStream, mut data: &[u8], fds: &[OwnedFd]) -> io::Result<()> {
    let borrowed: Vec<_> = fds.iter().map(AsFd::as_fd).collect();
    let mut space = [MaybeUninit::<u8>::uninit(); rustix::cmsg_space!(ScmRights(32))];
    let mut first = true;
    while !data.is_empty() {
        let mut control = SendAncillaryBuffer::new(&mut space);
        if first && !borrowed.is_empty() {
            control.push(SendAncillaryMessage::ScmRights(&borrowed));
        }
        match rustix::net::sendmsg(to, &[IoSlice::new(data)], &mut control, SendFlags::NOSIGNAL) {
            Ok(sent) => {
                data = &data[sent..];
                first = false;
            }
            Err(rustix::io::Errno::INTR) => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Read as _;

    use super::*;

    #[test]
    fn pump_passes_bytes_and_descriptors() {
        let (client, from) = UnixStream::pair().unwrap();
        let (to, server) = UnixStream::pair().unwrap();
        let thread = std::thread::spawn(move || pump(&from, &to));

        let (shared, _keep) = UnixStream::pair().unwrap();
        let fds = [shared.as_fd()];
        let mut space = [MaybeUninit::<u8>::uninit(); rustix::cmsg_space!(ScmRights(1))];
        let mut control = SendAncillaryBuffer::new(&mut space);
        control.push(SendAncillaryMessage::ScmRights(&fds));
        rustix::net::sendmsg(
            &client,
            &[IoSlice::new(b"xproto")],
            &mut control,
            SendFlags::empty(),
        )
        .unwrap();
        drop(client);

        let mut buf = [0u8; 16];
        let mut space = [MaybeUninit::<u8>::uninit(); rustix::cmsg_space!(ScmRights(1))];
        let mut control = RecvAncillaryBuffer::new(&mut space);
        let msg = rustix::net::recvmsg(
            &server,
            &mut [IoSliceMut::new(&mut buf)],
            &mut control,
            RecvFlags::empty(),
        )
        .unwrap();
        assert_eq!(&buf[..msg.bytes], b"xproto");
        let passed = control
            .drain()
            .filter(|m| matches!(m, RecvAncillaryMessage::ScmRights(_)))
            .count();
        assert_eq!(passed, 1);

        thread.join().unwrap();
        let mut rest = Vec::new();
        (&server).read_to_end(&mut rest).unwrap();
        assert!(rest.is_empty(), "the relay closes after the client does");
    }
}
