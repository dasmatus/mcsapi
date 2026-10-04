//! UI toolkit for apps that run on an mcsapi desktop.
//!
//! Apps implement [`App`] and draw with egui, styled by the same [`Theme`] as
//! the desktop shell. The toolkit renders one frame at a time and owns no event
//! loop, window, or painter: the host (usually `mcsapi-runtime`) feeds input,
//! paints the returned output, and schedules repaints.
//!
//! [`gesture`] adds 1:1 touchpad gestures: pan, pinch and rotate content that
//! follows the fingers exactly and coasts when they lift.
//!
//! ```
//! use mcsapi_ui::{App, Theme, egui};
//!
//! struct Hello;
//!
//! impl App for Hello {
//!     fn title(&self) -> &str {
//!         "Hello"
//!     }
//!
//!     fn ui(&mut self, ui: &mut egui::Ui, theme: &Theme) {
//!         ui.label(egui::RichText::new("Hello").color(theme.foreground));
//!     }
//! }
//!
//! let context = egui::Context::default();
//! let mut output = mcsapi_ui::run_frame(&mut Hello, &context, Default::default(), &Theme::default());
//! assert!(!output.shapes.is_empty());
//! // No painter here; a graphical host uploads and frees these textures.
//! output.textures_delta.clear();
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod gesture;

pub use egui;
pub use gesture::{GestureEvent, GestureTracker, Transform};
pub use mcsapi::theme;
pub use mcsapi::toolkit::{FallbackReason, GraphicsCapabilities, Toolkit};
pub use mcsapi::widgets::Theme;

/// An application drawn with the toolkit.
pub trait App {
    /// Human-readable title, for example for a window decoration or task list.
    fn title(&self) -> &str;

    /// Draws one frame of the app's content.
    fn ui(&mut self, ui: &mut egui::Ui, theme: &Theme);
}

/// Runs one egui frame of `app` inside a surface filled with the theme background.
///
/// The caller paints the returned shapes, applies texture deltas, handles
/// platform output, and honors requested repaints. egui panics in debug builds
/// if texture deltas are dropped unapplied; call `textures_delta.clear()` when
/// discarding them on purpose.
pub fn run_frame(
    app: &mut (impl App + ?Sized),
    context: &egui::Context,
    input: egui::RawInput,
    theme: &Theme,
) -> egui::FullOutput {
    context.run_ui(input, |ui| {
        egui::Frame::new()
            .fill(theme.background)
            .inner_margin(8)
            .show(ui, |ui| app.ui(ui, theme));
    })
}
