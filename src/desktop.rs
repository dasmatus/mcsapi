use std::{
    collections::{BTreeMap, BTreeSet, btree_set},
    fmt,
    iter::FusedIterator,
    num::NonZeroU64,
};

use crate::{Error, Geometry, Layout, Placements};

/// A host-assigned window identity, independent of a Wayland surface's lifetime.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WindowId(NonZeroU64);

impl WindowId {
    /// Creates an identity; zero is reserved and returns `None`.
    pub const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Returns the numeric identity.
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl fmt::Display for WindowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A stable workspace identity, distinct from a window identity or list index.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorkspaceId(NonZeroU64);

impl WorkspaceId {
    /// Creates an identity; zero is reserved and returns `None`.
    pub const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Returns the numeric identity.
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl fmt::Display for WorkspaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// An ordered set of windows with a layout and optional keyboard focus.
///
/// Membership can only be changed through [`Desktop`], preserving uniqueness
/// across workspaces. The main window comes first; others are ordered by ID.
#[derive(Debug)]
pub struct Workspace {
    id: WorkspaceId,
    windows: BTreeSet<WindowId>,
    main: Option<WindowId>,
    focused: Option<WindowId>,
    layout: Layout,
}

impl Workspace {
    /// Returns this workspace's stable identity.
    pub const fn id(&self) -> WorkspaceId {
        self.id
    }

    /// Returns windows in tiling order without allocating.
    pub fn windows(&self) -> Windows<'_> {
        Windows {
            inner: self.windows.iter(),
            main: self.main,
            emit_main: true,
            remaining: self.windows.len(),
        }
    }

    /// Returns the focused window, or `None` for an empty workspace.
    pub fn focused(&self) -> Option<WindowId> {
        self.focused
    }

    /// Returns the current layout.
    pub const fn layout(&self) -> Layout {
        self.layout
    }

    /// Changes the layout without changing window order or focus.
    pub fn set_layout(&mut self, layout: Layout) {
        self.layout = layout;
    }

    /// Focuses a member window, leaving state unchanged on error.
    pub fn focus(&mut self, window: WindowId) -> Result<(), Error> {
        if !self.windows.contains(&window) {
            return Err(Error::UnknownWindow(window));
        }
        self.focused = Some(window);
        Ok(())
    }

    /// Cycles focus forward, wrapping at the end; empty workspaces stay empty.
    pub fn focus_next(&mut self) -> Option<WindowId> {
        self.focused = self
            .windows()
            .skip_while(|window| Some(*window) != self.focused)
            .nth(1)
            .or_else(|| self.windows().next());
        self.focused()
    }

    /// Cycles focus backward, wrapping at the beginning.
    pub fn focus_previous(&mut self) -> Option<WindowId> {
        self.focused = self
            .windows()
            .take_while(|window| Some(*window) != self.focused)
            .last()
            .or_else(|| self.windows().last());
        self.focused()
    }

    /// Promotes the focused window to the main pane, preserving its focus.
    pub fn promote_focused(&mut self) {
        self.main = self.focused;
    }

    /// Computes logical geometry lazily in tiling order, without allocation.
    ///
    /// In a monocle layout all windows share the bounds; the host should display
    /// only the focused window.
    pub fn arrange(&self, bounds: Geometry) -> Result<Placements<Windows<'_>>, Error> {
        self.layout.arrange(bounds, self.windows())
    }

    fn insert(&mut self, window: WindowId) {
        self.windows.insert(window);
        self.main = self.main.or(Some(window));
        self.focused = Some(window);
    }

    fn remove(&mut self, window: WindowId) {
        let next_focus = if self.focused == Some(window) {
            self.windows()
                .skip_while(|id| *id != window)
                .nth(1)
                .or_else(|| self.windows().find(|id| *id != window))
        } else {
            self.focused
        };
        self.windows.remove(&window);
        if self.main == Some(window) {
            self.main = self.windows.first().copied();
        }
        self.focused = next_focus;
    }
}

/// An allocation-free, exact-size iterator over windows in tiling order.
#[derive(Clone, Debug)]
pub struct Windows<'a> {
    inner: btree_set::Iter<'a, WindowId>,
    main: Option<WindowId>,
    emit_main: bool,
    remaining: usize,
}

impl Iterator for Windows<'_> {
    type Item = WindowId;

    fn next(&mut self) -> Option<Self::Item> {
        let window = if self.emit_main {
            self.emit_main = false;
            self.main
        } else {
            self.inner.find(|&&id| Some(id) != self.main).copied()
        };
        if window.is_some() {
            self.remaining -= 1;
        }
        window
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl ExactSizeIterator for Windows<'_> {}
impl FusedIterator for Windows<'_> {}

/// Desktop policy with fixed workspace identities and globally unique windows.
///
/// This is intentionally a small ordered collection, not a surface registry.
/// The host maps [`WindowId`] to Smithay surfaces and removes destroyed windows.
#[derive(Debug)]
pub struct Desktop {
    workspaces: BTreeMap<WorkspaceId, Workspace>,
    active: WorkspaceId,
}

impl Desktop {
    /// Creates workspaces ordered by ID and activates the first supplied ID.
    pub fn new(ids: impl IntoIterator<Item = WorkspaceId>) -> Result<Self, Error> {
        let mut workspaces = BTreeMap::new();
        let mut active = None;
        for id in ids {
            if workspaces.contains_key(&id) {
                return Err(Error::DuplicateWorkspace(id));
            }
            active = active.or(Some(id));
            workspaces.insert(
                id,
                Workspace {
                    id,
                    windows: BTreeSet::new(),
                    main: None,
                    focused: None,
                    layout: Layout::default(),
                },
            );
        }
        let active = active.ok_or(Error::NoWorkspaces)?;
        Ok(Self { workspaces, active })
    }

    /// Iterates over workspaces in ID order without allocating.
    pub fn workspaces(&self) -> impl ExactSizeIterator<Item = &Workspace> + DoubleEndedIterator {
        self.workspaces.values()
    }

    /// Returns the active workspace.
    pub fn active(&self) -> &Workspace {
        &self.workspaces[&self.active]
    }

    /// Returns the active workspace for focus and layout changes.
    pub fn active_mut(&mut self) -> &mut Workspace {
        self.workspaces
            .get_mut(&self.active)
            .expect("active workspace exists")
    }

    /// Activates an existing workspace, preserving each workspace's focus.
    pub fn switch_to(&mut self, id: WorkspaceId) -> Result<(), Error> {
        self.check_workspace(id)?;
        self.active = id;
        Ok(())
    }

    /// Adds a new window to the active workspace and focuses it.
    ///
    /// A duplicate anywhere on the desktop is rejected without changing state.
    pub fn insert(&mut self, window: WindowId) -> Result<(), Error> {
        if self.window_workspace(window).is_some() {
            return Err(Error::DuplicateWindow(window));
        }
        self.active_mut().insert(window);
        Ok(())
    }

    /// Removes a window from any workspace, repairing focus if necessary.
    pub fn remove(&mut self, window: WindowId) -> Result<(), Error> {
        let workspace = self
            .window_workspace(window)
            .ok_or(Error::UnknownWindow(window))?;
        self.workspaces
            .get_mut(&workspace)
            .expect("workspace exists")
            .remove(window);
        Ok(())
    }

    /// Moves a window to another workspace and focuses it there.
    ///
    /// The active workspace is unchanged. Moving to its existing workspace is
    /// a no-op. All identities are checked before changing state.
    pub fn move_window(&mut self, window: WindowId, target: WorkspaceId) -> Result<(), Error> {
        self.check_workspace(target)?;
        let source = self
            .window_workspace(window)
            .ok_or(Error::UnknownWindow(window))?;
        if source != target {
            self.workspaces
                .get_mut(&source)
                .expect("source exists")
                .remove(window);
            self.workspaces
                .get_mut(&target)
                .expect("target exists")
                .insert(window);
        }
        Ok(())
    }

    fn check_workspace(&self, id: WorkspaceId) -> Result<(), Error> {
        if self.workspaces.contains_key(&id) {
            Ok(())
        } else {
            Err(Error::UnknownWorkspace(id))
        }
    }

    fn window_workspace(&self, window: WindowId) -> Option<WorkspaceId> {
        self.workspaces
            .values()
            .find(|ws| ws.windows.contains(&window))
            .map(Workspace::id)
    }
}
