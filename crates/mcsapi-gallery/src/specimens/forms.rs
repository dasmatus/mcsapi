use egui::Ui;
use mcsapi_components::{Checkbox, Input, Label, RadioGroup, Select, Slider, Switch, Textarea};

use super::{disabled_row, row};
use crate::{Category, Specimen};

pub(super) const SPECIMENS: &[Specimen] = &[
    Specimen {
        name: "Input",
        category: Category::Forms,
        source: "shadcn/ui",
        summary: "A single-line text field, plain or masked.",
        api: &["Input"],
        show: input,
    },
    Specimen {
        name: "Textarea",
        category: Category::Forms,
        source: "shadcn/ui",
        summary: "A multi-line text field.",
        api: &["Textarea"],
        show: textarea,
    },
    Specimen {
        name: "Label",
        category: Category::Forms,
        source: "shadcn/ui",
        summary: "The caption above a form control.",
        api: &["Label"],
        show: label,
    },
    Specimen {
        name: "Checkbox",
        category: Category::Forms,
        source: "shadcn/ui",
        summary: "An on/off choice, usually one of several.",
        api: &["Checkbox"],
        show: checkbox,
    },
    Specimen {
        name: "Switch",
        category: Category::Forms,
        source: "shadcn/ui",
        summary: "An on/off setting that applies right away.",
        api: &["Switch"],
        show: switch,
    },
    Specimen {
        name: "Radio Group",
        category: Category::Forms,
        source: "shadcn/ui",
        summary: "Exactly one choice from a short list.",
        api: &["RadioGroup"],
        show: radio_group,
    },
    Specimen {
        name: "Slider",
        category: Category::Forms,
        source: "shadcn/ui",
        summary: "A value picked from a range, optionally stepped.",
        api: &["Slider"],
        show: slider,
    },
    Specimen {
        name: "Select",
        category: Category::Forms,
        source: "shadcn/ui",
        summary: "One choice from a list that opens on click.",
        api: &["Select"],
        show: select,
    },
];

pub(super) struct State {
    name: String,
    password: String,
    reveal: bool,
    message: String,
    terms: bool,
    updates: bool,
    airplane: bool,
    wifi: bool,
    density: usize,
    volume: f32,
    step: f32,
    fruit: Option<usize>,
    timezone: Option<usize>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            name: String::new(),
            password: String::new(),
            reveal: false,
            message: String::new(),
            terms: false,
            updates: true,
            airplane: false,
            wifi: true,
            density: 1,
            volume: 0.6,
            step: 40.0,
            fruit: None,
            timezone: Some(2),
        }
    }
}

fn input(ui: &mut Ui, state: &mut super::State) {
    let state = &mut state.forms;
    row(ui, "Placeholder", |ui| {
        ui.add(
            Input::new(&mut state.name)
                .placeholder("Your name")
                .width(240.0),
        );
    });
    row(ui, "Password", |ui| {
        ui.add(
            Input::new(&mut state.password)
                .password(!state.reveal)
                .placeholder("Type a password")
                .width(240.0),
        );
        ui.add(Checkbox::new(&mut state.reveal).label("Show"));
    });
    row(ui, "Touch", |ui| {
        ui.add(
            Input::new(&mut state.name)
                .placeholder("Your name")
                .width(240.0)
                .touch(true),
        );
    });
    disabled_row(ui, |ui| {
        ui.add(Input::new(&mut "Read only".to_owned()).width(240.0));
    });
}

fn textarea(ui: &mut Ui, state: &mut super::State) {
    let state = &mut state.forms;
    row(ui, "Placeholder", |ui| {
        ui.add(
            Textarea::new(&mut state.message)
                .placeholder("Type your message here.")
                .rows(3),
        );
    });
    row(ui, "Touch", |ui| {
        ui.add(
            Textarea::new(&mut state.message)
                .placeholder("Type your message here.")
                .rows(2)
                .touch(true),
        );
    });
    disabled_row(ui, |ui| {
        ui.add(Textarea::new(&mut "Read only".to_owned()).rows(2));
    });
}

fn label(ui: &mut Ui, state: &mut super::State) {
    let state = &mut state.forms;
    row(ui, "With a field", |ui| {
        ui.vertical(|ui| {
            ui.add(Label::new("Email"));
            ui.add(
                Input::new(&mut state.name)
                    .placeholder("m@example.com")
                    .width(240.0),
            );
        });
    });
}

fn checkbox(ui: &mut Ui, state: &mut super::State) {
    let state = &mut state.forms;
    row(ui, "Interactive", |ui| {
        ui.add(Checkbox::new(&mut state.terms).label("Accept terms and conditions"));
        ui.add(Checkbox::new(&mut state.updates).label("Email me about updates"));
    });
    row(ui, "States", |ui| {
        ui.add(Checkbox::new(&mut false).label("Unchecked"));
        ui.add(Checkbox::new(&mut true).label("Checked"));
        ui.add(Checkbox::new(&mut true));
    });
    row(ui, "Touch", |ui| {
        ui.add(
            Checkbox::new(&mut state.terms)
                .label("Accept terms and conditions")
                .touch(true),
        );
    });
    disabled_row(ui, |ui| {
        ui.add(Checkbox::new(&mut false).label("Unchecked"));
        ui.add(Checkbox::new(&mut true).label("Checked"));
    });
}

fn switch(ui: &mut Ui, state: &mut super::State) {
    let state = &mut state.forms;
    row(ui, "Interactive", |ui| {
        ui.add(Switch::new(&mut state.airplane).label("Airplane mode"));
        ui.add(Switch::new(&mut state.wifi).label("Wi-Fi"));
    });
    row(ui, "States", |ui| {
        ui.add(Switch::new(&mut false).label("Off"));
        ui.add(Switch::new(&mut true).label("On"));
        ui.add(Switch::new(&mut true));
    });
    row(ui, "Touch", |ui| {
        ui.add(Switch::new(&mut state.wifi).label("Wi-Fi").touch(true));
    });
    disabled_row(ui, |ui| {
        ui.add(Switch::new(&mut false).label("Off"));
        ui.add(Switch::new(&mut true).label("On"));
    });
}

fn radio_group(ui: &mut Ui, state: &mut super::State) {
    let state = &mut state.forms;
    row(ui, "Interactive", |ui| {
        ui.add(RadioGroup::new(
            &mut state.density,
            &["Default", "Comfortable", "Compact"],
        ));
    });
    row(ui, "Touch", |ui| {
        ui.add(
            RadioGroup::new(&mut state.density, &["Default", "Comfortable", "Compact"]).touch(true),
        );
    });
    disabled_row(ui, |ui| {
        ui.add(RadioGroup::new(&mut 0, &["Yes", "No"]));
    });
}

fn slider(ui: &mut Ui, state: &mut super::State) {
    let state = &mut state.forms;
    row(ui, "Continuous", |ui| {
        ui.add(Slider::new(&mut state.volume, 0.0..=1.0).width(240.0));
        ui.label(format!("{:.0}%", state.volume * 100.0));
    });
    row(ui, "Stepped by 10", |ui| {
        ui.add(
            Slider::new(&mut state.step, 0.0..=100.0)
                .step(10.0)
                .width(240.0),
        );
        ui.label(format!("{:.0}", state.step));
    });
    row(ui, "Touch", |ui| {
        ui.add(
            Slider::new(&mut state.volume, 0.0..=1.0)
                .width(240.0)
                .touch(true),
        );
    });
    disabled_row(ui, |ui| {
        ui.add(Slider::new(&mut 0.3, 0.0..=1.0).width(240.0));
    });
}

const FRUITS: [&str; 5] = ["Apple", "Banana", "Blueberry", "Grapes", "Pineapple"];
const TIMEZONES: [&str; 4] = ["UTC", "Europe/London", "Europe/Prague", "America/New_York"];

fn select(ui: &mut Ui, state: &mut super::State) {
    let state = &mut state.forms;
    row(ui, "Placeholder", |ui| {
        ui.add(Select::new("fruit", &mut state.fruit, &FRUITS).placeholder("Select a fruit"));
    });
    row(ui, "Selected", |ui| {
        ui.add(Select::new("timezone", &mut state.timezone, &TIMEZONES).width(220.0));
    });
    row(ui, "Touch", |ui| {
        ui.add(
            Select::new("touch", &mut state.timezone, &TIMEZONES)
                .width(220.0)
                .touch(true),
        );
    });
    disabled_row(ui, |ui| {
        ui.add(Select::new("disabled", &mut Some(0), &FRUITS));
    });
}
