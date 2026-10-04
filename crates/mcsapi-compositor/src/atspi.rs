//! Publishes the merged accessibility tree on the AT-SPI bus, so Orca and
//! other assistive technology see the chrome and the in-process apps.
//! Wayland clients publish their own trees there.

use accesskit::{ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, TreeUpdate};
use accesskit_unix::Adapter;
use smithay::reexports::calloop::channel::Sender;

/// The AT-SPI adapter. It talks D-Bus on its own thread and stays idle
/// until a client (a screen reader, accerciser) asks for the tree.
pub(crate) struct Bridge {
    adapter: Adapter,
}

impl Bridge {
    /// Action requests from AT-SPI clients arrive on `requests`.
    pub fn new(requests: Sender<ActionRequest>) -> Self {
        let mut adapter = Adapter::new(Activation, Actions(requests), Deactivation);
        adapter.update_window_focus_state(true);
        Self { adapter }
    }

    /// Sends a fresh tree if a client is listening; `tree` is not called
    /// otherwise, so the desktop pays nothing without one. Returns `false`
    /// if the adapter panicked; it is unusable after that.
    #[must_use]
    pub fn update(&mut self, tree: impl FnOnce() -> TreeUpdate) -> bool {
        let adapter = &mut self.adapter;
        survived(move || adapter.update_if_active(tree))
    }

    /// The session window gained or lost focus. Returns `false` if the
    /// adapter panicked.
    #[must_use]
    pub fn focused(&mut self, focused: bool) -> bool {
        let adapter = &mut self.adapter;
        survived(move || adapter.update_window_focus_state(focused))
    }
}

/// Runs `f`, reporting a panic instead of unwinding into the compositor: a
/// bug in the accessibility stack must not end the session.
fn survived(f: impl FnOnce()) -> bool {
    let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_ok();
    if !ok {
        eprintln!("mcsapi-compositor: AT-SPI adapter failed; screen readers are off until restart");
    }
    ok
}

/// The tree is built on the compositor thread, so the first one goes out
/// with the next frame rather than from here.
struct Activation;

impl ActivationHandler for Activation {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        None
    }
}

struct Actions(Sender<ActionRequest>);

impl ActionHandler for Actions {
    fn do_action(&mut self, request: ActionRequest) {
        // Fails only once the compositor has stopped.
        let _ = self.0.send(request);
    }
}

struct Deactivation;

impl DeactivationHandler for Deactivation {
    fn deactivate_accessibility(&mut self) {}
}
