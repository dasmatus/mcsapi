use std::{fmt, num::NonZeroU64};

use crate::{Error, Geometry, Layout, Placement};

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
/// across workspaces. The first window occupies the main pane in a tall layout.
#[derive(Debug)]
pub struct Workspace {
    id: WorkspaceId,
    windows: Vec<WindowId>,
    focused: Option<usize>,
    layout: Layout,
}

impl Workspace {
    /// Returns this workspace's stable identity.
    pub const fn id(&self) -> WorkspaceId {
        self.id
    }

    /// Returns windows in tiling order without allocating.
    pub fn windows(&self) -> &[WindowId] {
        &self.windows
    }

    /// Returns the focused window, or `None` for an empty workspace.
    pub fn focused(&self) -> Option<WindowId> {
        self.focused.map(|index| self.windows[index])
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
        let index = self
            .windows
            .iter()
            .position(|&id| id == window)
            .ok_or(Error::UnknownWindow(window))?;
        self.focused = Some(index);
        Ok(())
    }

    /// Cycles focus forward, wrapping at the end; empty workspaces stay empty.
    pub fn focus_next(&mut self) -> Option<WindowId> {
        self.focused = self.focused.map(|index| (index + 1) % self.windows.len());
        self.focused()
    }

    /// Cycles focus backward, wrapping at the beginning.
    pub fn focus_previous(&mut self) -> Option<WindowId> {
        self.focused = self.focused.map(|index| {
            if index == 0 {
                self.windows.len() - 1
            } else {
                index - 1
            }
        });
        self.focused()
    }

    /// Promotes the focused window to the main pane, preserving its focus.
    pub fn promote_focused(&mut self) {
        if let Some(index) = self.focused {
            self.windows.swap(0, index);
            self.focused = Some(0);
        }
    }

    /// Reuses `placements` to compute logical geometry in tiling order.
    ///
    /// In a monocle layout all windows share the bounds; the host should display
    /// only the focused window. On error, `placements` is left unchanged.
    pub fn arrange(&self, bounds: Geometry, placements: &mut Vec<Placement>) -> Result<(), Error> {
        self.layout.arrange(bounds, &self.windows, placements)
    }

    fn insert(&mut self, window: WindowId) {
        self.windows.push(window);
        self.focused = Some(self.windows.len() - 1);
    }

    fn remove(&mut self, index: usize) -> WindowId {
        let window = self.windows.remove(index);
        self.focused = match self.focused {
            _ if self.windows.is_empty() => None,
            Some(focus) if focus > index => Some(focus - 1),
            Some(focus) => Some(focus.min(self.windows.len() - 1)),
            None => None,
        };
        window
    }
}

/// Desktop policy with fixed workspace identities and globally unique windows.
///
/// This is intentionally a small ordered collection, not a surface registry.
/// The host maps [`WindowId`] to Smithay surfaces and removes destroyed windows.
#[derive(Debug)]
pub struct Desktop {
    workspaces: Vec<Workspace>,
    active: usize,
}

impl Desktop {
    /// Creates empty workspaces in iteration order and activates the first.
    pub fn new(ids: impl IntoIterator<Item = WorkspaceId>) -> Result<Self, Error> {
        let mut workspaces: Vec<Workspace> = Vec::new();
        for id in ids {
            if workspaces.iter().any(|workspace| workspace.id == id) {
                return Err(Error::DuplicateWorkspace(id));
            }
            workspaces.push(Workspace {
                id,
                windows: Vec::new(),
                focused: None,
                layout: Layout::default(),
            });
        }
        if workspaces.is_empty() {
            return Err(Error::NoWorkspaces);
        }
        Ok(Self {
            workspaces,
            active: 0,
        })
    }

    /// Returns all workspaces in configured order.
    pub fn workspaces(&self) -> &[Workspace] {
        &self.workspaces
    }

    /// Returns the active workspace.
    pub fn active(&self) -> &Workspace {
        &self.workspaces[self.active]
    }

    /// Returns the active workspace for focus and layout changes.
    pub fn active_mut(&mut self) -> &mut Workspace {
        &mut self.workspaces[self.active]
    }

    /// Activates an existing workspace, preserving each workspace's focus.
    pub fn switch_to(&mut self, id: WorkspaceId) -> Result<(), Error> {
        self.active = self.workspace_index(id)?;
        Ok(())
    }

    /// Adds a new window to the active workspace and focuses it.
    ///
    /// A duplicate anywhere on the desktop is rejected without changing state.
    pub fn insert(&mut self, window: WindowId) -> Result<(), Error> {
        if self.window_location(window).is_some() {
            return Err(Error::DuplicateWindow(window));
        }
        self.active_mut().insert(window);
        Ok(())
    }

    /// Removes a window from any workspace, repairing focus if necessary.
    pub fn remove(&mut self, window: WindowId) -> Result<(), Error> {
        let (workspace, index) = self
            .window_location(window)
            .ok_or(Error::UnknownWindow(window))?;
        self.workspaces[workspace].remove(index);
        Ok(())
    }

    /// Moves a window to another workspace and focuses it there.
    ///
    /// The active workspace is unchanged. Moving to its existing workspace is
    /// a no-op. All identities are checked before changing state.
    pub fn move_window(&mut self, window: WindowId, target: WorkspaceId) -> Result<(), Error> {
        let target = self.workspace_index(target)?;
        let (source, index) = self
            .window_location(window)
            .ok_or(Error::UnknownWindow(window))?;
        if source != target {
            self.workspaces[source].remove(index);
            self.workspaces[target].insert(window);
        }
        Ok(())
    }

    fn workspace_index(&self, id: WorkspaceId) -> Result<usize, Error> {
        self.workspaces
            .iter()
            .position(|workspace| workspace.id == id)
            .ok_or(Error::UnknownWorkspace(id))
    }

    fn window_location(&self, window: WindowId) -> Option<(usize, usize)> {
        self.workspaces.iter().enumerate().find_map(|(index, ws)| {
            ws.windows
                .iter()
                .position(|&id| id == window)
                .map(|position| (index, position))
        })
    }
}
