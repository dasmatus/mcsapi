//! The [`mcsapi-components`](mcsapi_components) library as native GPUI elements.
//!
//! Same components, names, variants, and colors as the egui versions, drawn by
//! GPUI instead. Everything here needs the `gpui` feature; without it the crate
//! is empty, so `cargo build --workspace` needs no GPUI system libraries.
//!
//! Colors come from `Tokens`, derived from the shell
//! [`Theme`](mcsapi_ui::Theme) exactly like the egui tokens. Install them once
//! as a GPUI global, then build components inside any `Render` impl:
//!
//! ```ignore
//! use mcsapi_components_gpui::{Button, ButtonVariant, Card, Switch, Tokens};
//!
//! Tokens::from_theme(&theme).install(cx);
//!
//! Card::new().title("Notifications").child(
//!     Switch::new("push", self.push)
//!         .label("Push notifications")
//!         .on_change(cx.listener(|this, on: &bool, _, cx| {
//!             this.push = *on;
//!             cx.notify();
//!         })),
//! )
//! ```
//!
//! Buttons, toggles, badges, checkboxes, switches, alerts, separators,
//! labels, progress bars and tooltips are drawn by Zed's `ui` components
//! (vendored as `mcsapi-zed-ui`), whose Zed theme is built from the same
//! tokens: `Tokens::install` installs both, and `install_theme` also
//! passes on a full theme's fonts. Zed's components draw icons from
//! `icons/*.svg`; create the application with `Assets` so they render.
//! (Not links: these exist only with the `gpui` feature, and the crate docs
//! are also built without it.)
//!
//! Interactive components hold no state of their own: they take the current
//! value and report changes through an `on_*` callback, so the owning view
//! decides what changes. Text fields are the exception; `TextInput` is an
//! entity because it owns its text, selection, and keyboard focus.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

#[cfg(feature = "gpui")]
mod actions;
#[cfg(feature = "gpui")]
mod assets;
#[cfg(feature = "gpui")]
mod display;
#[cfg(feature = "gpui")]
mod forms;
#[cfg(feature = "gpui")]
mod input;
#[cfg(feature = "gpui")]
mod navigation;
#[cfg(feature = "gpui")]
mod overlays;
#[cfg(feature = "gpui")]
mod tokens;

#[cfg(feature = "gpui")]
pub use actions::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Kbd, Toggle, ToggleGroup,
};
#[cfg(feature = "gpui")]
pub use assets::Assets;
#[cfg(feature = "gpui")]
pub use display::{
    Alert, AlertVariant, AspectRatio, Avatar, Card, Empty, ErrorAlert, Label, Progress, Separator,
    Skeleton, Spinner, blockquote, typography,
};
#[cfg(feature = "gpui")]
pub use forms::{Checkbox, Input, RadioGroup, Select, Slider, Switch, Textarea};
#[cfg(feature = "gpui")]
pub use input::{TextInput, bind_text_input_keys};
#[cfg(feature = "gpui")]
pub use navigation::{
    Collapsible, Pagination, Table, Tabs, accordion_item, breadcrumb, page_window,
};
#[cfg(feature = "gpui")]
pub use overlays::{
    AlertDialog, AlertDialogAction, Dialog, Toast, Toaster, toast, toasts, tooltip,
};
/// Zed's theme types, with [`theme::theme_from_mcsapi`] building them from an
/// mcsapi theme.
#[cfg(feature = "gpui")]
pub use theme;
#[cfg(feature = "gpui")]
pub use tokens::{Tokens, install_theme};
/// Zed's `ui` component library these components are built on, for apps
/// that want its other components (lists, menus, tabs, icons, ...).
#[cfg(feature = "gpui")]
pub use ui;

/// A callback from a component to the view that owns its value.
#[cfg(feature = "gpui")]
pub type Handler<T> = std::rc::Rc<dyn Fn(&T, &mut gpui::Window, &mut gpui::App)>;
