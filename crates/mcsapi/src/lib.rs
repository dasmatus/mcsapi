//! Small, synchronous desktop policy for a Smithay Wayland compositor.
//!
//! The host owns surfaces, input, rendering, and the event loop. This crate owns
//! workspace membership, focus, and logical window placement.
//!
//! ```
//! use mcsapi::{Desktop, Geometry, WindowId, WorkspaceId};
//!
//! let workspace = WorkspaceId::new(1).unwrap();
//! let mut desktop = Desktop::new([workspace])?;
//! desktop.insert(WindowId::new(1).unwrap())?;
//! let placements = desktop.active().arrange(
//!     Geometry::new((0, 0).into(), (1920, 1080).into()),
//! )?;
//! assert_eq!(placements.len(), 1);
//! # Ok::<(), mcsapi::Error>(())
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod desktop;
mod layout;
pub mod toolkit;
pub mod widgets;

pub use desktop::{Desktop, WindowId, Windows, Workspace, WorkspaceId};
pub use layout::{Layout, Placement, Placements};
pub use mcsapi_theme as theme;
pub use smithay;
pub use smithay::utils::{Logical, Rectangle};

/// A rectangle measured in logical compositor coordinates, not physical pixels.
pub type Geometry = Rectangle<i32, Logical>;

/// An invalid desktop operation or layout input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// At least one workspace is required.
    NoWorkspaces,
    /// Workspace identifiers must be unique.
    DuplicateWorkspace(WorkspaceId),
    /// The requested workspace does not exist.
    UnknownWorkspace(WorkspaceId),
    /// The window already belongs to a workspace.
    DuplicateWindow(WindowId),
    /// The window does not belong to the requested workspace.
    UnknownWindow(WindowId),
    /// Bounds must be positive and their edges must fit in an `i32`.
    InvalidGeometry,
    /// Every tiled window needs at least one logical pixel in each dimension.
    InsufficientSpace,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoWorkspaces => f.write_str("at least one workspace is required"),
            Self::DuplicateWorkspace(id) => write!(f, "duplicate workspace: {id}"),
            Self::UnknownWorkspace(id) => write!(f, "unknown workspace: {id}"),
            Self::DuplicateWindow(id) => write!(f, "duplicate window: {id}"),
            Self::UnknownWindow(id) => write!(f, "unknown window: {id}"),
            Self::InvalidGeometry => f.write_str("invalid logical output bounds"),
            Self::InsufficientSpace => f.write_str("not enough space for tiled windows"),
        }
    }
}

impl std::error::Error for Error {}
