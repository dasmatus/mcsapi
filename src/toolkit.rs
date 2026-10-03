//! Capability-based UI selection without opening a display during construction.
//!
//! GPUI is optional. A host must probe its actual renderer and driver and provide
//! a fallible GPUI window/view initializer. egui provides UI state, not a software
//! renderer: a fallback host must also supply a suitable painter and input.

use std::fmt;

pub use egui;
#[cfg(feature = "gpui")]
pub use gpui;

/// Results of a host's renderer/driver probe, not guesses from device files.
///
/// The conservative default selects egui.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GraphicsCapabilities {
    /// The selected renderer has working hardware acceleration.
    pub hardware_acceleration: bool,
    /// Required GPUI graphics drivers and platform support are usable.
    pub gpui_drivers: bool,
}

/// Why GPUI was not selected.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FallbackReason {
    /// The crate was built without the `gpui` feature.
    GpuiNotCompiled,
    /// Hardware acceleration was unavailable or not verified.
    NoHardwareAcceleration,
    /// Required driver/platform support was unavailable or not verified.
    MissingDrivers,
    /// A host's fallible initialization failed.
    InitializationFailed(String),
}

/// The selected toolkit and its initialized state.
///
/// `G` is the host's GPUI window/view handle. Keeping it generic lets the host
/// initialize within GPUI's application context rather than nesting event loops.
pub enum Toolkit<G> {
    /// A successfully initialized GPUI window/view.
    #[cfg(feature = "gpui")]
    Gpui(G),
    /// An egui context plus an explicit explanation of the fallback.
    Egui {
        /// Reusable immediate-mode UI state.
        context: egui::Context,
        /// Why the accelerated toolkit was not selected.
        reason: FallbackReason,
        /// Tracks the host handle type when GPUI is not compiled.
        handle_type: std::marker::PhantomData<G>,
    },
}

impl<G> fmt::Debug for Toolkit<G> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "gpui")]
            Self::Gpui(_) => f.debug_tuple("Gpui").finish_non_exhaustive(),
            Self::Egui { reason, .. } => f
                .debug_struct("Egui")
                .field("reason", reason)
                .finish_non_exhaustive(),
        }
    }
}

impl<G> Toolkit<G> {
    /// Prefers GPUI only when compiled, accelerated, driver-ready, and initialized.
    ///
    /// The callback is called at most once, and never on an ineligible machine.
    /// It must report renderer/window creation failures as `Err`; panics are not
    /// caught. An error message is retained for diagnostics before using egui.
    pub fn initialize<E: fmt::Display>(
        capabilities: GraphicsCapabilities,
        initialize_gpui: impl FnOnce() -> Result<G, E>,
    ) -> Self {
        let reason = if !cfg!(feature = "gpui") {
            FallbackReason::GpuiNotCompiled
        } else if !capabilities.hardware_acceleration {
            FallbackReason::NoHardwareAcceleration
        } else if !capabilities.gpui_drivers {
            FallbackReason::MissingDrivers
        } else {
            #[cfg(feature = "gpui")]
            {
                match initialize_gpui() {
                    Ok(handle) => return Self::Gpui(handle),
                    Err(error) => FallbackReason::InitializationFailed(error.to_string()),
                }
            }
            #[cfg(not(feature = "gpui"))]
            {
                unreachable!("GPUI eligibility was checked above")
            }
        };
        #[cfg(not(feature = "gpui"))]
        let _ = initialize_gpui;
        Self::Egui {
            context: egui::Context::default(),
            reason,
            handle_type: std::marker::PhantomData,
        }
    }
}
