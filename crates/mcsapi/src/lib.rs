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
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum Error {
    /// At least one workspace is required.
    #[error("at least one workspace is required")]
    #[diagnostic(code(mcsapi::no_workspaces))]
    NoWorkspaces,
    /// Workspace identifiers must be unique.
    #[error("duplicate workspace: {0}")]
    #[diagnostic(code(mcsapi::duplicate_workspace))]
    DuplicateWorkspace(WorkspaceId),
    /// The requested workspace does not exist.
    #[error("unknown workspace: {0}")]
    #[diagnostic(code(mcsapi::unknown_workspace))]
    UnknownWorkspace(WorkspaceId),
    /// The window already belongs to a workspace.
    #[error("duplicate window: {0}")]
    #[diagnostic(code(mcsapi::duplicate_window))]
    DuplicateWindow(WindowId),
    /// The window does not belong to the requested workspace.
    #[error("unknown window: {0}")]
    #[diagnostic(code(mcsapi::unknown_window))]
    UnknownWindow(WindowId),
    /// Bounds must be positive and their edges must fit in an `i32`.
    #[error("invalid logical output bounds")]
    #[diagnostic(code(mcsapi::invalid_geometry))]
    InvalidGeometry,
    /// Every tiled window needs at least one logical pixel in each dimension.
    #[error("not enough space for tiled windows")]
    #[diagnostic(code(mcsapi::insufficient_space))]
    InsufficientSpace,
}
