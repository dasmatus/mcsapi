//! `wp_security_context_manager_v1`: a sandbox (Flatpak) opens a listening
//! socket for the app it starts and attaches its metadata, and clients that
//! connect through that socket are marked sandboxed. Globals that reach past
//! a client's own windows (placing surfaces over every window, reading the
//! clipboard without focus, typing or clicking into other clients) are
//! created with [`unsandboxed`] as their filter, so a sandboxed client never
//! sees them; nor may it open a further context.

use std::sync::Arc;

use smithay::{
    delegate_security_context,
    reexports::{
        calloop::LoopHandle,
        wayland_server::{Client, DisplayHandle},
    },
    wayland::security_context::{
        SecurityContext, SecurityContextHandler, SecurityContextListenerSource,
        SecurityContextState,
    },
};
use tracing::{info, warn};

use super::{ClientState, Host};
use crate::Shell;

pub(super) struct Security<S: Shell + 'static> {
    _state: SecurityContextState,
    handle: LoopHandle<'static, Host<S>>,
}

impl<S: Shell + 'static> Security<S> {
    pub(super) fn new(dh: &DisplayHandle, handle: LoopHandle<'static, Host<S>>) -> Self {
        Self {
            _state: SecurityContextState::new::<Host<S>, _>(dh, unsandboxed),
            handle,
        }
    }
}

/// Whether `client` connected outside any sandbox's security context, so it
/// may bind the privileged globals.
pub(super) fn unsandboxed(client: &Client) -> bool {
    client
        .get_data::<ClientState>()
        .is_some_and(|state| state.security_context.is_none())
}

impl<S: Shell + 'static> SecurityContextHandler for Host<S> {
    fn context_created(&mut self, source: SecurityContextListenerSource, context: SecurityContext) {
        info!(
            engine = context.sandbox_engine.as_deref().unwrap_or("?"),
            app = context.app_id.as_deref().unwrap_or("?"),
            "sandbox opened a socket"
        );
        // The source removes itself when the sandbox closes its end.
        let inserted = self
            .security
            .handle
            .insert_source(source, move |stream, _, host| {
                let state = ClientState {
                    security_context: Some(context.clone()),
                    ..ClientState::default()
                };
                if let Err(e) = host.display.insert_client(stream, Arc::new(state)) {
                    warn!(error = %e, "rejected sandboxed client");
                }
            });
        if let Err(e) = inserted {
            warn!(error = %e, "cannot listen for a sandbox");
        }
    }
}

delegate_security_context!(@<S: Shell + 'static> Host<S>);
