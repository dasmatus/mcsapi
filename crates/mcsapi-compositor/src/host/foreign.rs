//! Window lists for taskbars, docks and window switchers outside the shell,
//! covering Wayland clients and in-process apps alike:
//!
//! - `ext_foreign_toplevel_list_v1`: each window's title and app ID.
//! - `zwlr_foreign_toplevel_manager_v1`: the same plus parent, whether the
//!   window is focused or maximized, and requests to focus, close, maximize
//!   or minimize it (waybar's taskbar, rofi's window mode). Focusing goes
//!   through `Shell::activate`, maximizing and minimizing through
//!   `Shell::client_request`, as if the window had asked; restoring a
//!   minimized window focuses it. Minimized and fullscreen are not reported,
//!   since the shell does not say which of its unplaced windows are
//!   minimized.
//!
//! Both list every window, so sandboxed clients do not see them.

use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, Ordering},
};

use smithay::{
    delegate_foreign_toplevel_list,
    output::Output,
    reexports::{
        wayland_protocols_wlr::foreign_toplevel::v1::server::{
            zwlr_foreign_toplevel_handle_v1::{self, State, ZwlrForeignToplevelHandleV1},
            zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
        },
        wayland_server::{
            Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
            backend::ClientId,
        },
    },
    wayland::{
        compositor::with_states,
        foreign_toplevel_list::{
            ForeignToplevelHandle, ForeignToplevelListHandler, ForeignToplevelListState,
        },
        shell::xdg::XdgToplevelSurfaceData,
    },
};

use super::{Content, Host, security::unsandboxed};
use crate::{ClientRequest, Shell, WindowId};

/// What the lists were last told about a window.
#[derive(Clone, Default, PartialEq)]
struct Info {
    title: String,
    app_id: String,
    parent: Option<WindowId>,
    activated: bool,
    maximized: bool,
}

struct Entry {
    info: Info,
    ext: ForeignToplevelHandle,
    /// One per bound wlr manager.
    wlr: Vec<ZwlrForeignToplevelHandleV1>,
}

/// The user data of a wlr handle.
struct HandleData {
    window: WindowId,
    /// Whether its first batch of events went out.
    announced: AtomicBool,
}

pub(super) struct Foreign {
    list: ForeignToplevelListState,
    managers: Vec<ZwlrForeignToplevelManagerV1>,
    windows: HashMap<WindowId, Entry>,
}

impl Foreign {
    pub(super) fn new<S: Shell + 'static>(dh: &DisplayHandle) -> Self {
        dh.create_global::<Host<S>, ZwlrForeignToplevelManagerV1, ()>(3, ());
        Self {
            list: ForeignToplevelListState::new_with_filter::<Host<S>>(dh, unsandboxed),
            managers: Vec::new(),
            windows: HashMap::new(),
        }
    }

    /// The handle `client` has for `window`.
    fn handle_for(
        &self,
        window: WindowId,
        client: &ClientId,
    ) -> Option<&ZwlrForeignToplevelHandleV1> {
        self.windows
            .get(&window)?
            .wlr
            .iter()
            .find(|h| h.client().is_some_and(|c| &c.id() == client))
    }

    /// Sends what `handle` does not know yet about its window, then `done`.
    fn send(
        &self,
        output: &Output,
        handle: &ZwlrForeignToplevelHandleV1,
        info: &Info,
        old: Option<&Info>,
    ) {
        let Some(data) = handle.data::<HandleData>() else {
            return;
        };
        let first = !data.announced.swap(true, Ordering::Relaxed);
        let old = if first { None } else { old };
        if first && let Some(client) = handle.client() {
            for wl_output in output.client_outputs(&client) {
                handle.output_enter(&wl_output);
            }
        }
        if old.is_none_or(|o| o.title != info.title) {
            handle.title(info.title.clone());
        }
        if old.is_none_or(|o| o.app_id != info.app_id) {
            handle.app_id(info.app_id.clone());
        }
        if old.is_none_or(|o| (o.activated, o.maximized) != (info.activated, info.maximized)) {
            let mut state = Vec::new();
            for (on, s) in [
                (info.maximized, State::Maximized),
                (info.activated, State::Activated),
            ] {
                if on {
                    state.extend_from_slice(&(s as u32).to_ne_bytes());
                }
            }
            handle.state(state);
        }
        if handle.version() >= 3 && old.is_none_or(|o| o.parent != info.parent) {
            let client = handle.client().map(|c| c.id());
            let parent = info
                .parent
                .zip(client)
                .and_then(|(p, c)| self.handle_for(p, &c));
            handle.parent(parent);
        }
        handle.done();
    }
}

/// Announces `window` to `manager`'s client.
fn new_wlr<S: Shell + 'static>(
    dh: &DisplayHandle,
    manager: &ZwlrForeignToplevelManagerV1,
    window: WindowId,
) -> Option<ZwlrForeignToplevelHandleV1> {
    let client = manager.client()?;
    let data = HandleData {
        window,
        announced: AtomicBool::new(false),
    };
    let handle = client
        .create_resource::<ZwlrForeignToplevelHandleV1, _, Host<S>>(dh, manager.version(), data)
        .ok()?;
    manager.toplevel(&handle);
    Some(handle)
}

impl<S: Shell + 'static> Host<S> {
    /// A window's title and app ID, whichever kind it is.
    fn names(content: &Content) -> (String, String) {
        match content {
            Content::Wayland(window) => window
                .toplevel()
                .map(|t| {
                    with_states(t.wl_surface(), |states| {
                        states
                            .data_map
                            .get::<XdgToplevelSurfaceData>()
                            .and_then(|d| d.lock().ok())
                            .map(|d| {
                                (
                                    d.title.clone().unwrap_or_default(),
                                    d.app_id.clone().unwrap_or_default(),
                                )
                            })
                            .unwrap_or_default()
                    })
                })
                .unwrap_or_default(),
            Content::Internal { title, app_id, .. } => (title.clone(), app_id.clone()),
        }
    }

    /// Brings both lists up to date with the windows; called after every
    /// change the shell or a client may have made.
    pub(super) fn update_foreign(&mut self) {
        let placements = self.shell.placements();
        let focused = self.shell.focused();
        let infos: HashMap<WindowId, Info> = self
            .windows
            .iter()
            .map(|(&id, content)| {
                let (title, app_id) = Self::names(content);
                let parent = match content {
                    Content::Wayland(w) => w.toplevel().and_then(|t| self.parent_of(t)),
                    Content::Internal { .. } => None,
                };
                let info = Info {
                    title,
                    app_id,
                    parent,
                    activated: focused == Some(id),
                    maximized: placements.iter().any(|p| p.window == id && p.maximized),
                };
                (id, info)
            })
            .collect();

        let dh = self.display.clone();
        let foreign = &mut self.foreign;
        foreign.windows.retain(|id, entry| {
            if infos.contains_key(id) {
                return true;
            }
            foreign.list.remove_toplevel(&entry.ext);
            for handle in &entry.wlr {
                handle.closed();
            }
            false
        });
        // Windows that are new, then what changed, so that a new window's
        // parent already has its handle.
        let mut changed = Vec::new();
        for (&id, info) in &infos {
            match foreign.windows.get_mut(&id) {
                Some(entry) if &entry.info == info => {}
                Some(entry) => {
                    if (&entry.info.title, &entry.info.app_id) != (&info.title, &info.app_id) {
                        entry.ext.send_title(&info.title);
                        entry.ext.send_app_id(&info.app_id);
                        entry.ext.send_done();
                    }
                    changed.push((id, std::mem::replace(&mut entry.info, info.clone())));
                }
                None => {
                    let ext = foreign
                        .list
                        .new_toplevel::<Self>(info.title.clone(), info.app_id.clone());
                    let wlr = foreign
                        .managers
                        .iter()
                        .filter_map(|m| new_wlr::<S>(&dh, m, id))
                        .collect();
                    foreign.windows.insert(
                        id,
                        Entry {
                            info: info.clone(),
                            ext,
                            wlr,
                        },
                    );
                }
            }
        }
        let foreign = &self.foreign;
        for (id, old) in &changed {
            if let Some(entry) = foreign.windows.get(id) {
                for handle in &entry.wlr {
                    foreign.send(&self.output, handle, &entry.info, Some(old));
                }
            }
        }
        // Handles not announced yet: new windows, and every window for a
        // manager bound since the last update.
        for entry in foreign.windows.values() {
            for handle in &entry.wlr {
                if handle
                    .data::<HandleData>()
                    .is_some_and(|d| !d.announced.load(Ordering::Relaxed))
                {
                    foreign.send(&self.output, handle, &entry.info, None);
                }
            }
        }
    }
}

impl<S: Shell + 'static> Host<S> {
    /// The window an `ext_foreign_toplevel_list_v1` handle stands for.
    pub(super) fn foreign_window(&self, identifier: &str) -> Option<WindowId> {
        self.foreign
            .windows
            .iter()
            .find(|(_, entry)| entry.ext.identifier() == identifier)
            .map(|(&id, _)| id)
    }
}

impl<S: Shell + 'static> ForeignToplevelListHandler for Host<S> {
    fn foreign_toplevel_list_state(&mut self) -> &mut ForeignToplevelListState {
        &mut self.foreign.list
    }
}

impl<S: Shell + 'static> GlobalDispatch<ZwlrForeignToplevelManagerV1, ()> for Host<S> {
    fn bind(
        host: &mut Self,
        handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrForeignToplevelManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        let manager = data_init.init(resource, ());
        for (&id, entry) in &mut host.foreign.windows {
            entry.wlr.extend(new_wlr::<S>(handle, &manager, id));
        }
        host.foreign.managers.push(manager);
        host.update_foreign();
    }

    fn can_view(client: Client, _global_data: &()) -> bool {
        unsandboxed(&client)
    }
}

impl<S: Shell + 'static> Dispatch<ZwlrForeignToplevelManagerV1, ()> for Host<S> {
    fn request(
        host: &mut Self,
        _client: &Client,
        manager: &ZwlrForeignToplevelManagerV1,
        request: zwlr_foreign_toplevel_manager_v1::Request,
        _data: &(),
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        if let zwlr_foreign_toplevel_manager_v1::Request::Stop = request {
            host.foreign.managers.retain(|m| m != manager);
            manager.finished();
        }
    }

    fn destroyed(
        host: &mut Self,
        _client: ClientId,
        manager: &ZwlrForeignToplevelManagerV1,
        _data: &(),
    ) {
        host.foreign.managers.retain(|m| m != manager);
    }
}

impl<S: Shell + 'static> Dispatch<ZwlrForeignToplevelHandleV1, HandleData> for Host<S> {
    fn request(
        host: &mut Self,
        _client: &Client,
        _handle: &ZwlrForeignToplevelHandleV1,
        request: zwlr_foreign_toplevel_handle_v1::Request,
        data: &HandleData,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        use zwlr_foreign_toplevel_handle_v1::Request;
        let window = data.window;
        if !host.windows.contains_key(&window) {
            return;
        }
        let maximized = host
            .foreign
            .windows
            .get(&window)
            .is_some_and(|e| e.info.maximized);
        match request {
            Request::Activate { .. } | Request::UnsetMinimized => host.shell.activate(window),
            Request::Close => host.close(window),
            Request::SetMaximized if !maximized => {
                host.shell.client_request(window, ClientRequest::Maximize);
            }
            Request::UnsetMaximized if maximized => {
                host.shell.client_request(window, ClientRequest::Maximize);
            }
            Request::SetMinimized => host.shell.client_request(window, ClientRequest::Minimize),
            // Fullscreen is not something the shell offers, and the
            // rectangle only aims a minimize animation.
            _ => return,
        }
        host.sync();
    }

    fn destroyed(
        host: &mut Self,
        _client: ClientId,
        handle: &ZwlrForeignToplevelHandleV1,
        data: &HandleData,
    ) {
        if let Some(entry) = host.foreign.windows.get_mut(&data.window) {
            entry.wlr.retain(|h| h != handle);
        }
    }
}

delegate_foreign_toplevel_list!(@<S: Shell + 'static> Host<S>);
