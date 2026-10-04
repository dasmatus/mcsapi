//! The specimens, one module per group of related widgets.
//!
//! Each module lists its specimens in a `SPECIMENS` slice and keeps the state
//! its demos mutate (text in inputs, toggled switches, open dialogs) in its own
//! `State`, so adding a component touches only one module.

mod actions;
mod display;
mod forms;
mod navigation;
mod overlays;
mod shell;

use egui::{RichText, Ui};
use mcsapi_components::Tokens;

use crate::Specimen;

/// Every module's specimens.
pub(crate) const ALL: [&[Specimen]; 6] = [
    actions::SPECIMENS,
    forms::SPECIMENS,
    display::SPECIMENS,
    navigation::SPECIMENS,
    overlays::SPECIMENS,
    shell::SPECIMENS,
];

/// What the gallery's live demos remember between frames.
#[derive(Default)]
pub struct State {
    /// The theme the gallery is drawn with this frame.
    pub(crate) theme: mcsapi_ui::Theme,
    actions: actions::State,
    forms: forms::State,
    navigation: navigation::State,
    overlays: overlays::State,
    shell: shell::State,
}

impl State {
    /// Where [`mcsapi_components::Toaster`] stacks the gallery's toasts.
    pub(crate) fn toaster_position(&self) -> mcsapi_components::ToasterPosition {
        self.overlays.toaster_position()
    }
}

/// Draws one labeled row of a specimen: a muted caption, then `content`
/// laid out left to right.
fn row<R>(ui: &mut Ui, caption: &str, content: impl FnOnce(&mut Ui) -> R) -> R {
    let tokens = Tokens::current(ui.ctx());
    ui.horizontal_top(|ui| {
        // As tall as a default button, so the caption lines up with the
        // first line of controls.
        let size = egui::vec2(120.0, 36.0);
        ui.allocate_ui_with_layout(
            size,
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_size(size);
                ui.label(
                    RichText::new(caption)
                        .font(tokens.small_font())
                        .color(tokens.muted_foreground),
                );
            },
        );
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), size.y),
            egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true),
            |ui| {
                ui.set_min_height(size.y);
                content(ui)
            },
        )
        .inner
    })
    .inner
}

/// Lays `content` out top to bottom in a column `width` wide, for widgets
/// such as cards and alerts that stack their own parts vertically.
fn column<R>(ui: &mut Ui, width: f32, content: impl FnOnce(&mut Ui) -> R) -> R {
    ui.allocate_ui_with_layout(
        egui::vec2(width, 0.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_width(width);
            content(ui)
        },
    )
    .inner
}

/// Like [`row`], with `content` disabled to show the widget's disabled state.
fn disabled_row(ui: &mut Ui, content: impl FnOnce(&mut Ui)) {
    row(ui, "Disabled", |ui| {
        ui.disable();
        content(ui);
    });
}
