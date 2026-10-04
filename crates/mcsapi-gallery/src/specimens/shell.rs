use egui::Ui;
use mcsapi::{
    Desktop, WindowId, WorkspaceId,
    widgets::{PreviewWindow, egui_workspace_bar, egui_workspace_previews},
};
use mcsapi_components::{Tokens, typography};

use super::row;
use crate::{Category, Specimen};

pub(super) const SPECIMENS: &[Specimen] = &[Specimen {
    name: "Workspace Bar",
    category: Category::Shell,
    source: "mcsapi",
    summary: "The shell's workspace switcher, from mcsapi::widgets: gpui_workspace_bar for GPUI hosts, egui_workspace_bar for egui ones, and egui_workspace_previews with live window thumbnails.",
    api: &[
        "gpui_workspace_bar",
        "egui_workspace_bar",
        "egui_workspace_previews",
    ],
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
    row(ui, "Previews", |ui| {
        if let Some(id) = egui_workspace_previews(ui, desktop, theme, sample_windows) {
            desktop
                .switch_to(id)
                .expect("the bar only offers existing workspaces");
        }
    });
    row(ui, "One workspace", |ui| {
        egui_workspace_bar(ui, &sample_desktop(1), theme);
    });
}

/// A tall layout on workspace 1, a pair on 2, one window on 3, none on 4.
fn sample_windows(workspace: WorkspaceId) -> Vec<PreviewWindow> {
    let window = |x, y, w, h, focused| PreviewWindow {
        x,
        y,
        w,
        h,
        focused,
    };
    match workspace.get() {
        1 => vec![
            window(0.03, 0.08, 0.56, 0.88, true),
            window(0.61, 0.08, 0.36, 0.43, false),
            window(0.61, 0.53, 0.36, 0.43, false),
        ],
        2 => vec![
            window(0.03, 0.08, 0.46, 0.88, false),
            window(0.51, 0.08, 0.46, 0.88, true),
        ],
        3 => vec![window(0.15, 0.15, 0.7, 0.75, true)],
        _ => Vec::new(),
    }
}
