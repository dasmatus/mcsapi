use egui::Ui;
use mcsapi_components::{
    AlertDialog, AlertDialogAction, Button, ButtonVariant, Dialog, ErrorDialog, Input, Label,
    Tokens, toast, toasts, tooltip, typography,
};

use super::row;
use crate::{Category, Specimen};

pub(super) const SPECIMENS: &[Specimen] = &[
    Specimen {
        name: "Dialog",
        category: Category::Overlays,
        source: "shadcn/ui",
        summary: "A modal window over a dimmed backdrop; Escape closes it.",
        api: &["Dialog"],
        show: dialog,
    },
    Specimen {
        name: "Alert Dialog",
        category: Category::Overlays,
        source: "shadcn/ui",
        summary: "A modal question that needs an answer before going on.",
        api: &["AlertDialog", "AlertDialogAction"],
        show: alert_dialog,
    },
    Specimen {
        name: "Error Dialog",
        category: Category::Overlays,
        source: "mcsapi",
        summary: "An error the app reports, as an alert dialog that links into the documentation.",
        api: &["ErrorDialog", "ErrorDialogResponse"],
        show: error_dialog,
    },
    Specimen {
        name: "Tooltip",
        category: Category::Overlays,
        source: "shadcn/ui",
        summary: "A short hint shown while the pointer rests on a widget.",
        api: &["tooltip"],
        show: tooltips,
    },
    Specimen {
        name: "Toast",
        category: Category::Overlays,
        source: "shadcn/ui (Sonner)",
        summary: "A brief notice in the corner that dismisses itself.",
        api: &["toast", "toasts", "Toast", "Toaster"],
        show: toast_demo,
    },
];

pub(super) struct State {
    profile_open: bool,
    name: String,
    delete_open: bool,
    publish_open: bool,
    answer: Option<String>,
    error: Option<mcsapi_ui::Error>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            profile_open: false,
            name: "Pedro Duarte".to_owned(),
            delete_open: false,
            publish_open: false,
            answer: None,
            error: None,
        }
    }
}

fn dialog(ui: &mut Ui, state: &mut super::State) {
    let state = &mut state.overlays;
    row(ui, "Edit profile", |ui| {
        if ui
            .add(Button::new("Open dialog").variant(ButtonVariant::Outline))
            .clicked()
        {
            state.profile_open = true;
        }
    });
    let ctx = ui.ctx().clone();
    let mut save = false;
    Dialog::new("gallery-profile", &mut state.profile_open, "Edit profile")
        .description("Make changes to your profile here. Click save when you're done.")
        .show(&ctx, |ui| {
            ui.add(Label::new("Name"));
            ui.add(Input::new(&mut state.name).width(ui.available_width()));
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                save = ui.add(Button::new("Save changes")).clicked();
            });
        });
    if save {
        state.profile_open = false;
        toast(&ctx, "Profile saved", Some(state.name.clone()));
    }
}

fn error_dialog(ui: &mut Ui, state: &mut super::State) {
    let state = &mut state.overlays;
    row(ui, "Full report", |ui| {
        if ui
            .add(Button::new("Apply theme").variant(ButtonVariant::Outline))
            .clicked()
        {
            state.error = Some(crate::sample_error());
        }
    });
    let docs = crate::sample_docs();
    let ctx = ui.ctx().clone();
    ErrorDialog::new("gallery-error", &mut state.error)
        .docs(&docs)
        .show(&ctx);
}

fn alert_dialog(ui: &mut Ui, state: &mut super::State) {
    let tokens = Tokens::current(ui.ctx());
    let state = &mut state.overlays;
    row(ui, "Variants", |ui| {
        if ui
            .add(Button::new("Delete account").variant(ButtonVariant::Destructive))
            .clicked()
        {
            state.delete_open = true;
        }
        if ui
            .add(Button::new("Publish").variant(ButtonVariant::Outline))
            .clicked()
        {
            state.publish_open = true;
        }
    });
    if let Some(answer) = &state.answer {
        row(ui, "Last answer", |ui| {
            ui.label(typography::muted(&tokens, answer.as_str()))
        });
    }
    let ctx = ui.ctx().clone();
    let deleted = AlertDialog::new(
        "gallery-delete",
        &mut state.delete_open,
        "Are you absolutely sure?",
    )
    .description("This permanently deletes your account and removes your data from our servers.")
    .confirm_text("Delete")
    .destructive(true)
    .show(&ctx);
    let published = AlertDialog::new(
        "gallery-publish",
        &mut state.publish_open,
        "Publish this post?",
    )
    .description("Everyone with the link will be able to read it.")
    .confirm_text("Publish")
    .cancel_text("Not yet")
    .show(&ctx);
    for (dialog, action) in [("Delete", deleted), ("Publish", published)] {
        match action {
            Some(AlertDialogAction::Confirm) => state.answer = Some(format!("{dialog}: confirmed")),
            Some(AlertDialogAction::Cancel) => state.answer = Some(format!("{dialog}: cancelled")),
            _ => {}
        }
    }
}

fn tooltips(ui: &mut Ui, _: &mut super::State) {
    row(ui, "Hover these", |ui| {
        tooltip(
            ui.add(Button::new("Hover").variant(ButtonVariant::Outline)),
            "Add to library",
        );
        tooltip(
            ui.add(Button::new("?").variant(ButtonVariant::Ghost)),
            "Tooltips work on any widget's response.",
        );
    });
}

fn toast_demo(ui: &mut Ui, _: &mut super::State) {
    let tokens = Tokens::current(ui.ctx());
    row(ui, "Show", |ui| {
        if ui
            .add(Button::new("Title only").variant(ButtonVariant::Outline))
            .clicked()
        {
            toast(ui.ctx(), "Event has been created", None);
        }
        if ui
            .add(Button::new("With description").variant(ButtonVariant::Outline))
            .clicked()
        {
            toast(
                ui.ctx(),
                "Event has been created",
                Some("Sunday, December 03, 2023 at 9:00 AM".to_owned()),
            );
        }
    });
    let queued = toasts(ui.ctx()).len();
    row(ui, "Queued", |ui| {
        ui.label(typography::muted(
            &tokens,
            format!("{queued} showing; click one to dismiss it."),
        ))
    });
}
