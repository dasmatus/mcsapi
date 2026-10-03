use mcsapi::{
    Desktop, WorkspaceId,
    toolkit::{FallbackReason, GraphicsCapabilities, Toolkit},
    widgets::{Theme, egui_workspace_bar},
};

#[test]
fn conservative_capabilities_do_not_initialize_gpui() {
    let toolkit =
        Toolkit::<()>::initialize(GraphicsCapabilities::default(), || -> Result<(), &str> {
            panic!("ineligible GPUI must not initialize")
        });
    let Toolkit::Egui { reason, .. } = toolkit else {
        panic!("expected egui")
    };
    let expected = if cfg!(feature = "gpui") {
        FallbackReason::NoHardwareAcceleration
    } else {
        FallbackReason::GpuiNotCompiled
    };
    assert_eq!(reason, expected);
}

#[test]
fn accelerated_initialization_is_gated_by_compilation() {
    let toolkit = Toolkit::initialize(
        GraphicsCapabilities {
            hardware_acceleration: true,
            gpui_drivers: true,
        },
        || Ok::<_, &str>(42),
    );
    if cfg!(feature = "gpui") {
        assert!(matches!(toolkit, Toolkit::Gpui(42)));
    } else {
        assert!(matches!(
            toolkit,
            Toolkit::Egui {
                reason: FallbackReason::GpuiNotCompiled,
                ..
            }
        ));
    }
}

#[cfg(feature = "gpui")]
#[test]
fn missing_drivers_skip_initialization_and_failure_preserves_diagnostics() {
    let toolkit = Toolkit::<()>::initialize(
        GraphicsCapabilities {
            hardware_acceleration: true,
            gpui_drivers: false,
        },
        || -> Result<(), &str> { panic!("missing drivers") },
    );
    assert!(matches!(
        toolkit,
        Toolkit::Egui {
            reason: FallbackReason::MissingDrivers,
            ..
        }
    ));
    let toolkit = Toolkit::<()>::initialize(
        GraphicsCapabilities {
            hardware_acceleration: true,
            gpui_drivers: true,
        },
        || Err::<(), _>("renderer creation failed"),
    );
    assert!(
        matches!(toolkit, Toolkit::Egui { reason: FallbackReason::InitializationFailed(message), .. } if message == "renderer creation failed")
    );
}

#[test]
fn egui_workspace_bar_produces_a_headless_frame_without_mutating_state() {
    let desktop = Desktop::new((1..=3).map(|id| WorkspaceId::new(id).unwrap())).unwrap();
    let context = egui::Context::default();
    let mut output = context.run_ui(egui::RawInput::default(), |ui| {
        assert_eq!(egui_workspace_bar(ui, &desktop, Theme::default()), None);
    });
    assert!(!output.shapes.is_empty());
    output.textures_delta.clear();
    assert_eq!(desktop.active().id(), WorkspaceId::new(1).unwrap());
}
