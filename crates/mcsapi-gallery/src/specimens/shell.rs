use egui::Ui;
use mcsapi::{Desktop, WindowId, WorkspaceId, widgets::egui_workspace_bar};
use mcsapi_components::{Tokens, typography};

use super::row;
use crate::{Category, Specimen};

pub(super) const SPECIMENS: &[Specimen] = &[Specimen {
    name: "Workspace Bar",
    category: Category::Shell,
    source: "mcsapi",
    summary: "The shell's workspace switcher. GPUI hosts draw the same bar with gpui_workspace_bar.",
    api: &["egui_workspace_bar"],
    show: workspace_bar,
}];

#[derive(Default)]
pub(super) struct State {
    desktop: Option<Desktop>,
}

fn sample_desktop(workspaces: u64) -> Desktop {
    let mut desktop = Desktop::new((1..=workspaces).filter_map(WorkspaceId::new))
        .expect("workspace IDs are distinct and nonzero");
    for window in (1..=3).filter_map(WindowId::new) {
        desktop.insert(window).expect("window IDs are distinct");
    }
    desktop
}

fn workspace_bar(ui: &mut Ui, state: &mut super::State) {
    let tokens = Tokens::current(ui.ctx());
    let theme = state.theme;
    let desktop = state.shell.desktop.get_or_insert_with(|| sample_desktop(4));
    row(ui, "Interactive", |ui| {
        ui.vertical(|ui| {
            if let Some(id) = egui_workspace_bar(ui, desktop, theme) {
                desktop
                    .switch_to(id)
                    .expect("the bar only offers existing workspaces");
            }
            ui.label(typography::muted(
                &tokens,
                format!(
                    "Workspace {} is active with {} windows.",
                    desktop.active().id(),
                    desktop.active().windows().len()
                ),
            ));
        });
    });
    row(ui, "One workspace", |ui| {
        egui_workspace_bar(ui, &sample_desktop(1), theme);
    });
}
