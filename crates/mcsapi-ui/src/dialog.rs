//! What a dialog is, apart from how a toolkit draws it: the window it is
//! modal to and the actions in its button row. `NativeDialog` in
//! mcsapi-components (egui) and mcsapi-components-gpui take these, so a
//! dialog described once, such as an xdg-desktop-portal prompt, reads the
//! same in either.
//!
//! ```
//! use mcsapi_ui::dialog::{ActionRole, DialogAction, ParentWindow};
//!
//! let parent: ParentWindow = "wayland:3f2a9c".parse().unwrap();
//! assert_eq!(parent, ParentWindow::Wayland("3f2a9c".into()));
//! assert_eq!(parent.to_string(), "wayland:3f2a9c");
//!
//! let actions = [
//!     DialogAction::new("Take Screenshot", ActionRole::Default),
//!     DialogAction::new("Cancel", ActionRole::Cancel),
//! ];
//! assert_eq!(DialogAction::default_index(&actions), Some(0));
//! assert_eq!(DialogAction::cancel_index(&actions), Some(1));
//! ```

use std::fmt;
use std::str::FromStr;

/// The window a dialog is modal to, as the xdg-desktop-portal spells it in
/// a request's `parent_window`: `wayland:<handle>` for a toplevel another
/// client exported with xdg-foreign, `x11:<window id in hex>`, or empty for
/// none.
///
/// The dialog's own app is not named here: a dialog an app opens over its
/// own window is that window's child already. This is for a window in
/// another process, which is what a portal is asked about.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub enum ParentWindow {
    /// No parent: the dialog stands alone, centered on the display.
    #[default]
    None,
    /// An xdg-foreign v2 handle the parent's client exported.
    Wayland(String),
    /// An X11 window id.
    X11(u32),
}

impl ParentWindow {
    /// Whether there is a parent at all.
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

/// A `parent_window` that is neither empty nor `wayland:` or `x11:`
/// followed by a handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseParentWindowError(String);

impl fmt::Display for ParseParentWindowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not a portal parent window: {:?}", self.0)
    }
}

impl std::error::Error for ParseParentWindowError {}

impl FromStr for ParentWindow {
    type Err = ParseParentWindowError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || ParseParentWindowError(s.to_owned());
        if s.is_empty() {
            return Ok(Self::None);
        }
        match s.split_once(':') {
            Some(("wayland", handle)) if !handle.is_empty() => Ok(Self::Wayland(handle.to_owned())),
            // The portal documents the id as hexadecimal; some clients
            // still prefix it.
            Some(("x11", id)) => u32::from_str_radix(id.trim_start_matches("0x"), 16)
                .map(Self::X11)
                .map_err(|_| invalid()),
            _ => Err(invalid()),
        }
    }
}

impl fmt::Display for ParentWindow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => Ok(()),
            Self::Wayland(handle) => write!(f, "wayland:{handle}"),
            Self::X11(id) => write!(f, "x11:{id:x}"),
        }
    }
}

/// What a button in a dialog's row does besides its own action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionRole {
    /// The answer Enter gives, drawn as the primary button. At most one.
    Default,
    /// The answer Escape and the window's close button give, drawn as an
    /// outline button. At most one.
    Cancel,
    /// Any other answer, drawn as a ghost button.
    Other,
}

/// A button in a dialog's row. A row lists its actions in reading order;
/// the toolkit lays them out from the right, with the default last.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DialogAction {
    /// The button's label.
    pub label: String,
    /// What else answers with it.
    pub role: ActionRole,
}

impl DialogAction {
    /// A button labelled `label`.
    pub fn new(label: impl Into<String>, role: ActionRole) -> Self {
        Self {
            label: label.into(),
            role,
        }
    }

    /// The index Enter answers with: the first [`ActionRole::Default`].
    pub fn default_index(actions: &[Self]) -> Option<usize> {
        actions.iter().position(|a| a.role == ActionRole::Default)
    }

    /// The index Escape and closing answer with: the first
    /// [`ActionRole::Cancel`].
    pub fn cancel_index(actions: &[Self]) -> Option<usize> {
        actions.iter().position(|a| a.role == ActionRole::Cancel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_portal_parent_forms() {
        assert_eq!("".parse(), Ok(ParentWindow::None));
        assert_eq!(
            "wayland:abc".parse(),
            Ok(ParentWindow::Wayland("abc".into()))
        );
        assert_eq!("x11:1a2b".parse(), Ok(ParentWindow::X11(0x1a2b)));
        assert_eq!("x11:0x1a2b".parse(), Ok(ParentWindow::X11(0x1a2b)));
        assert!("wayland:".parse::<ParentWindow>().is_err());
        assert!("x11:zz".parse::<ParentWindow>().is_err());
        assert!("mir:1".parse::<ParentWindow>().is_err());
    }

    #[test]
    fn prints_what_it_parses() {
        for s in ["", "wayland:abc", "x11:1a2b"] {
            assert_eq!(s.parse::<ParentWindow>().unwrap().to_string(), s);
        }
    }
}
