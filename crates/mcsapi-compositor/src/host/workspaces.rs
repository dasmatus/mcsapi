//! `ext_workspace_manager_v1`: the shell's workspaces for pagers outside it
//! (waybar's workspaces module). There is one group, on the one output,
//! holding what `Shell::workspaces` lists, each with its index as its
//! coordinate. A pager can activate a workspace, which reaches
//! `Shell::activate_workspace` when it commits; it cannot create, remove or
//! move workspaces. Sandboxed clients do not see the global.

use std::sync::Mutex;

use smithay::reexports::{
    wayland_protocols::ext::workspace::v1::server::{
        ext_workspace_group_handle_v1::{self, ExtWorkspaceGroupHandleV1, GroupCapabilities},
        ext_workspace_handle_v1::{self, ExtWorkspaceHandleV1, State, WorkspaceCapabilities},
        ext_workspace_manager_v1::{self, ExtWorkspaceManagerV1},
    },
    wayland_server::{
        Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, backend::ClientId,
    },
};

use super::{Host, security::unsandboxed};
use crate::{Shell, WorkspaceInfo};

/// One pager's view: its group and a handle per workspace it was told of.
struct Pager {
    manager: ExtWorkspaceManagerV1,
    group: ExtWorkspaceGroupHandleV1,
    workspaces: Vec<(WorkspaceInfo, ExtWorkspaceHandleV1)>,
}

/// A pager's requests, applied on its next commit.
#[derive(Default)]
struct Pending(Mutex<Vec<String>>);

#[derive(Default)]
pub(super) struct Workspaces {
    pagers: Vec<Pager>,
}

impl Workspaces {
    pub(super) fn new<S: Shell + 'static>(dh: &DisplayHandle) -> Self {
        dh.create_global::<Host<S>, ExtWorkspaceManagerV1, ()>(1, ());
        Self::default()
    }
}

fn state(info: &WorkspaceInfo) -> State {
    let mut state = State::empty();
    state.set(State::Active, info.active);
    state.set(State::Urgent, info.urgent);
    state.set(State::Hidden, info.hidden);
    state
}

impl<S: Shell + 'static> Host<S> {
    /// Tells every pager what changed in the shell's workspaces.
    pub(super) fn update_workspaces(&mut self) {
        if self.workspaces.pagers.is_empty() {
            return;
        }
        let now = self.shell.workspaces();
        let dh = self.display.clone();
        for pager in &mut self.workspaces.pagers {
            let mut changed = false;
            pager.workspaces.retain(|(info, handle)| {
                let keep = now.iter().any(|w| w.id == info.id);
                if !keep {
                    pager.group.workspace_leave(handle);
                    handle.removed();
                    changed = true;
                }
                keep
            });
            for (index, info) in now.iter().enumerate() {
                match pager.workspaces.iter_mut().find(|(w, _)| w.id == info.id) {
                    Some((old, _)) if old == info => {}
                    Some((old, handle)) => {
                        if old.name != info.name {
                            handle.name(info.name.clone());
                        }
                        if state(old) != state(info) {
                            handle.state(state(info));
                        }
                        *old = info.clone();
                        changed = true;
                    }
                    None => {
                        let Some(handle) = announce::<S>(&dh, pager, info, index) else {
                            continue;
                        };
                        pager.workspaces.push((info.clone(), handle));
                        changed = true;
                    }
                }
            }
            if changed {
                pager.manager.done();
            }
        }
    }
}

/// Creates `info`'s handle for `pager` and sends all of it.
fn announce<S: Shell + 'static>(
    dh: &DisplayHandle,
    pager: &Pager,
    info: &WorkspaceInfo,
    index: usize,
) -> Option<ExtWorkspaceHandleV1> {
    let client = pager.manager.client()?;
    let handle = client
        .create_resource::<ExtWorkspaceHandleV1, _, Host<S>>(
            dh,
            pager.manager.version(),
            info.id.clone(),
        )
        .ok()?;
    pager.manager.workspace(&handle);
    handle.id(info.id.clone());
    handle.name(info.name.clone());
    handle.coordinates((index as u32).to_ne_bytes().to_vec());
    handle.state(state(info));
    handle.capabilities(WorkspaceCapabilities::Activate);
    pager.group.workspace_enter(&handle);
    Some(handle)
}

impl<S: Shell + 'static> GlobalDispatch<ExtWorkspaceManagerV1, ()> for Host<S> {
    fn bind(
        host: &mut Self,
        dh: &DisplayHandle,
        client: &Client,
        resource: New<ExtWorkspaceManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        let manager = data_init.init(resource, Pending::default());
        let Ok(group) =
            client.create_resource::<ExtWorkspaceGroupHandleV1, _, Self>(dh, manager.version(), ())
        else {
            return;
        };
        manager.workspace_group(&group);
        group.capabilities(GroupCapabilities::empty());
        for output in host.output.client_outputs(client) {
            group.output_enter(&output);
        }
        let mut pager = Pager {
            manager,
            group,
            workspaces: Vec::new(),
        };
        for (index, info) in host.shell.workspaces().into_iter().enumerate() {
            if let Some(handle) = announce::<S>(dh, &pager, &info, index) {
                pager.workspaces.push((info, handle));
            }
        }
        pager.manager.done();
        host.workspaces.pagers.push(pager);
    }

    fn can_view(client: Client, _global_data: &()) -> bool {
        unsandboxed(&client)
    }
}

impl<S: Shell + 'static> Dispatch<ExtWorkspaceManagerV1, Pending> for Host<S> {
    fn request(
        host: &mut Self,
        _client: &Client,
        manager: &ExtWorkspaceManagerV1,
        request: ext_workspace_manager_v1::Request,
        pending: &Pending,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            ext_workspace_manager_v1::Request::Commit => {
                let ids = std::mem::take(&mut *pending.0.lock().unwrap_or_else(|e| e.into_inner()));
                for id in ids {
                    host.shell.activate_workspace(&id);
                }
                host.sync();
            }
            ext_workspace_manager_v1::Request::Stop => {
                host.workspaces.pagers.retain(|p| &p.manager != manager);
                manager.finished();
            }
            _ => {}
        }
    }

    fn destroyed(
        host: &mut Self,
        _client: ClientId,
        manager: &ExtWorkspaceManagerV1,
        _data: &Pending,
    ) {
        host.workspaces.pagers.retain(|p| &p.manager != manager);
    }
}

impl<S: Shell + 'static> Dispatch<ExtWorkspaceGroupHandleV1, ()> for Host<S> {
    fn request(
        _host: &mut Self,
        _client: &Client,
        _group: &ExtWorkspaceGroupHandleV1,
        _request: ext_workspace_group_handle_v1::Request,
        _data: &(),
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        // create_workspace is not among the group's capabilities.
    }
}

impl<S: Shell + 'static> Dispatch<ExtWorkspaceHandleV1, String> for Host<S> {
    fn request(
        host: &mut Self,
        _client: &Client,
        handle: &ExtWorkspaceHandleV1,
        request: ext_workspace_handle_v1::Request,
        id: &String,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        // Only activation is offered; it waits for the manager's commit.
        if let ext_workspace_handle_v1::Request::Activate = request
            && let Some(pager) = host
                .workspaces
                .pagers
                .iter()
                .find(|p| p.workspaces.iter().any(|(_, h)| h == handle))
            && let Some(pending) = pager.manager.data::<Pending>()
        {
            pending
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(id.clone());
        }
    }
}
