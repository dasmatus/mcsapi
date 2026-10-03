use egui::Ui;
use mcsapi_components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Kbd, Toggle, ToggleGroup, toast,
};

use super::{disabled_row, row};
use crate::{Category, Specimen};

pub(super) const SPECIMENS: &[Specimen] = &[
    Specimen {
        name: "Button",
        category: Category::Actions,
        source: "shadcn/ui",
        summary: "Triggers an action. Six variants and four sizes.",
        api: &["Button", "ButtonVariant", "ButtonSize"],
        show: button,
    },
    Specimen {
        name: "Badge",
        category: Category::Actions,
        source: "shadcn/ui",
        summary: "A small status or count label.",
        api: &["Badge", "BadgeVariant"],
        show: badge,
    },
    Specimen {
        name: "Toggle",
        category: Category::Actions,
        source: "shadcn/ui",
        summary: "A two-state button that stays pressed.",
        api: &["Toggle"],
        show: toggle,
    },
    Specimen {
        name: "Toggle Group",
        category: Category::Actions,
        source: "shadcn/ui",
        summary: "A row of toggles where at most one is pressed.",
        api: &["ToggleGroup"],
        show: toggle_group,
    },
    Specimen {
        name: "Kbd",
        category: Category::Actions,
        source: "shadcn/ui",
        summary: "A keyboard key or shortcut.",
        api: &["Kbd"],
        show: kbd,
    },
];

#[derive(Default)]
pub(super) struct State {
    bold: bool,
    italic: bool,
    underline: bool,
    align: Option<usize>,
    view: Option<usize>,
}

const VARIANTS: [(ButtonVariant, &str); 6] = [
    (ButtonVariant::Default, "Default"),
    (ButtonVariant::Secondary, "Secondary"),
    (ButtonVariant::Destructive, "Destructive"),
    (ButtonVariant::Outline, "Outline"),
    (ButtonVariant::Ghost, "Ghost"),
    (ButtonVariant::Link, "Link"),
];

fn button(ui: &mut Ui, _: &mut super::State) {
    row(ui, "Variants", |ui| {
        for (variant, name) in VARIANTS {
            if ui.add(Button::new(name).variant(variant)).clicked() {
                toast(ui.ctx(), format!("{name} button clicked"), None);
            }
        }
    });
    row(ui, "Sizes", |ui| {
        ui.add(Button::new("Small").size(ButtonSize::Sm));
        ui.add(Button::new("Default"));
        ui.add(Button::new("Large").size(ButtonSize::Lg));
        ui.add(
            Button::new("+")
                .size(ButtonSize::Icon)
                .variant(ButtonVariant::Outline),
        );
    });
    row(ui, "Disabled", |ui| {
        for (variant, name) in VARIANTS {
            ui.add(Button::new(name).variant(variant).enabled(false));
        }
    });
}

fn badge(ui: &mut Ui, _: &mut super::State) {
    row(ui, "Variants", |ui| {
        ui.add(Badge::new("Default"));
        ui.add(Badge::new("Secondary").variant(BadgeVariant::Secondary));
        ui.add(Badge::new("Destructive").variant(BadgeVariant::Destructive));
        ui.add(Badge::new("Outline").variant(BadgeVariant::Outline));
    });
    row(ui, "In context", |ui| {
        ui.label("Inbox");
        ui.add(Badge::new("12"));
        ui.label("Build");
        ui.add(Badge::new("failing").variant(BadgeVariant::Destructive));
    });
}

fn toggle(ui: &mut Ui, state: &mut super::State) {
    let state = &mut state.actions;
    row(ui, "Interactive", |ui| {
        ui.add(Toggle::new(&mut state.bold, "B"));
        ui.add(Toggle::new(&mut state.italic, "I"));
        ui.add(Toggle::new(&mut state.underline, "U"));
    });
    row(ui, "States", |ui| {
        ui.add(Toggle::new(&mut false, "Off"));
        ui.add(Toggle::new(&mut true, "On"));
    });
    disabled_row(ui, |ui| {
        ui.add(Toggle::new(&mut false, "Off"));
        ui.add(Toggle::new(&mut true, "On"));
    });
}

fn toggle_group(ui: &mut Ui, state: &mut super::State) {
    let state = &mut state.actions;
    row(ui, "Text alignment", |ui| {
        ui.add(ToggleGroup::new(
            &mut state.align,
            &["Left", "Center", "Right"],
        ));
    });
    if state.view.is_none() {
        state.view = Some(1);
    }
    row(ui, "Preselected", |ui| {
        ui.add(ToggleGroup::new(
            &mut state.view,
            &["List", "Grid", "Columns"],
        ));
    });
    disabled_row(ui, |ui| {
        ui.add(ToggleGroup::new(&mut Some(0), &["Day", "Week", "Month"]));
    });
}

fn kbd(ui: &mut Ui, _: &mut super::State) {
    row(ui, "Keys", |ui| {
        for key in ["Esc", "Tab", "Enter", "Space", "←", "→"] {
            ui.add(Kbd::new(key));
        }
    });
    row(ui, "Shortcut", |ui| {
        ui.add(Kbd::new("Super"));
        ui.label("+");
        ui.add(Kbd::new("Shift"));
        ui.label("+");
        ui.add(Kbd::new("Q"));
    });
}
