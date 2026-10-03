use std::convert::Infallible;

use mcsapi::{
    Desktop, Geometry, WindowId, WorkspaceId,
    toolkit::{GraphicsCapabilities, Toolkit},
    widgets::{Theme, egui_workspace_bar},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut desktop = Desktop::new((1..=3).map(|id| WorkspaceId::new(id).unwrap()))?;
    for window in (1..=3).map(|id| WindowId::new(id).unwrap()) {
        desktop.insert(window)?;
    }
    for placement in desktop
        .active()
        .arrange(Geometry::new((0, 0).into(), (1280, 720).into()))?
    {
        println!("window {}: {:?}", placement.window, placement.geometry);
    }

    // An unprobed/headless host conservatively uses egui.
    let toolkit = Toolkit::initialize(GraphicsCapabilities::default(), || Ok::<_, Infallible>(()));
    if let Toolkit::Egui { context, reason } = toolkit {
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            if let Some(id) = egui_workspace_bar(ui, &desktop, Theme::default()) {
                desktop
                    .switch_to(id)
                    .expect("widget returned an existing ID");
            }
        });
        println!("egui fallback: {reason:?}; {} shapes", output.shapes.len());
        // This example has no painter; a graphical host must apply these updates.
        output.textures_delta.clear();
    }
    Ok(())
}
