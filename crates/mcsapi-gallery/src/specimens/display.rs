use egui::{RichText, Ui};
use mcsapi_components::{
    Alert, AlertVariant, AspectRatio, Avatar, Button, ButtonVariant, Card, Empty, Progress,
    Separator, Skeleton, Spinner, Tokens, blockquote, typography,
};

use super::{column, row};
use crate::{Category, Specimen};

pub(super) const SPECIMENS: &[Specimen] = &[
    Specimen {
        name: "Card",
        category: Category::Display,
        source: "shadcn/ui",
        summary: "A bordered surface with an optional title and description.",
        api: &["Card"],
        show: card,
    },
    Specimen {
        name: "Alert",
        category: Category::Display,
        source: "shadcn/ui",
        summary: "A callout for information or an error.",
        api: &["Alert", "AlertVariant"],
        show: alert,
    },
    Specimen {
        name: "Avatar",
        category: Category::Display,
        source: "shadcn/ui",
        summary: "A person's initials in a circle.",
        api: &["Avatar"],
        show: avatar,
    },
    Specimen {
        name: "Separator",
        category: Category::Display,
        source: "shadcn/ui",
        summary: "A thin rule between groups of content.",
        api: &["Separator"],
        show: separator,
    },
    Specimen {
        name: "Progress",
        category: Category::Display,
        source: "shadcn/ui",
        summary: "How far along a task is.",
        api: &["Progress"],
        show: progress,
    },
    Specimen {
        name: "Spinner",
        category: Category::Display,
        source: "shadcn/ui",
        summary: "Work of unknown length in progress.",
        api: &["Spinner"],
        show: spinner,
    },
    Specimen {
        name: "Skeleton",
        category: Category::Display,
        source: "shadcn/ui",
        summary: "A placeholder shaped like content that is still loading.",
        api: &["Skeleton"],
        show: skeleton,
    },
    Specimen {
        name: "Empty",
        category: Category::Display,
        source: "shadcn/ui",
        summary: "What a view shows when it has nothing in it.",
        api: &["Empty"],
        show: empty,
    },
    Specimen {
        name: "Aspect Ratio",
        category: Category::Display,
        source: "shadcn/ui",
        summary: "Content sized to a fixed width-to-height ratio.",
        api: &["AspectRatio"],
        show: aspect_ratio,
    },
    Specimen {
        name: "Typography",
        category: Category::Display,
        source: "shadcn/ui",
        summary: "Headings, body text, and quotations.",
        api: &["typography", "blockquote"],
        show: type_scale,
    },
];

fn card(ui: &mut Ui, _: &mut super::State) {
    row(ui, "Header and body", |ui| {
        column(ui, 360.0, |ui| {
            Card::new()
                .title("Create project")
                .description("Deploy your new project in one click.")
                .show(ui, |ui| {
                    ui.label("Name and framework go here.");
                    ui.horizontal(|ui| {
                        ui.add(Button::new("Deploy"));
                        ui.add(Button::new("Cancel").variant(ButtonVariant::Outline));
                    });
                });
        });
    });
    row(ui, "Body only", |ui| {
        column(ui, 360.0, |ui| {
            Card::new().show(ui, |ui| ui.label("A card with no header."));
        });
    });
}

fn alert(ui: &mut Ui, _: &mut super::State) {
    row(ui, "Default", |ui| {
        column(ui, 480.0, |ui| {
            ui.add(
                Alert::new("Heads up!")
                    .description("You can add components to your app using the gallery."),
            );
        });
    });
    row(ui, "Destructive", |ui| {
        column(ui, 480.0, |ui| {
            ui.add(
                Alert::new("Error")
                    .description("Your session has expired. Please log in again.")
                    .variant(AlertVariant::Destructive),
            );
        });
    });
    row(ui, "Title only", |ui| {
        column(ui, 480.0, |ui| {
            ui.add(Alert::new("Saved."));
        });
    });
}

fn avatar(ui: &mut Ui, _: &mut super::State) {
    row(ui, "Initials", |ui| {
        for name in ["Matus", "Ada Lovelace", "Grace Brewster Hopper", ""] {
            ui.add(Avatar::new(name));
        }
    });
    row(ui, "Sizes", |ui| {
        for size in [24.0, 32.0, 40.0, 56.0] {
            ui.add(Avatar::new("Linus Torvalds").size(size));
        }
    });
}

fn separator(ui: &mut Ui, _: &mut super::State) {
    let tokens = Tokens::current(ui.ctx());
    row(ui, "Horizontal", |ui| {
        column(ui, 320.0, |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Radix Primitives").strong());
                ui.label(typography::muted(
                    &tokens,
                    "An open-source UI component library.",
                ));
                ui.add(Separator::horizontal());
            });
        });
    });
    row(ui, "Vertical", |ui| {
        ui.set_height(20.0);
        ui.label("Blog");
        ui.add(Separator::vertical());
        ui.label("Docs");
        ui.add(Separator::vertical());
        ui.label("Source");
    });
}

fn progress(ui: &mut Ui, _: &mut super::State) {
    for value in [0.0, 0.33, 0.66, 1.0] {
        row(ui, &format!("{:.0}%", value * 100.0), |ui| {
            ui.add(Progress::new(value).width(320.0));
        });
    }
}

fn spinner(ui: &mut Ui, _: &mut super::State) {
    row(ui, "Sizes", |ui| {
        for size in [12.0, 16.0, 24.0, 32.0] {
            ui.add(Spinner::new().size(size));
        }
    });
    row(ui, "In a button", |ui| {
        ui.horizontal(|ui| {
            ui.add(Spinner::new());
            ui.add(Button::new("Please wait").enabled(false));
        });
    });
}

fn skeleton(ui: &mut Ui, _: &mut super::State) {
    row(ui, "Profile", |ui| {
        ui.add(Skeleton::circle(48.0));
        ui.vertical(|ui| {
            ui.add(Skeleton::new([240.0, 16.0]));
            ui.add(Skeleton::new([180.0, 16.0]));
        });
    });
    row(ui, "Card", |ui| {
        ui.vertical(|ui| {
            ui.add(Skeleton::new([280.0, 120.0]));
            ui.add(Skeleton::new([280.0, 16.0]));
            ui.add(Skeleton::new([200.0, 16.0]));
        });
    });
}

fn empty(ui: &mut Ui, _: &mut super::State) {
    row(ui, "With action", |ui| {
        column(ui, 420.0, |ui| {
            Empty::new("No projects yet")
                .description("You haven't created any projects. Get started by creating one.")
                .icon("📁")
                .show(ui, |ui| ui.add(Button::new("Create project")));
        });
    });
    row(ui, "Text only", |ui| {
        column(ui, 420.0, |ui| {
            Empty::new("Nothing found").show(ui, |_| ());
        });
    });
}

fn aspect_ratio(ui: &mut Ui, _: &mut super::State) {
    let tokens = Tokens::current(ui.ctx());
    for (caption, ratio) in [("16 : 9", 16.0 / 9.0), ("1 : 1", 1.0)] {
        row(ui, caption, |ui| {
            column(ui, 240.0, |ui| {
                AspectRatio::new(ratio).show(ui, |ui| {
                    let rect = ui.max_rect();
                    ui.painter()
                        .rect_filled(rect, tokens.control_radius(), tokens.muted);
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        caption,
                        tokens.body_font(),
                        tokens.muted_foreground,
                    );
                });
            });
        });
    }
}

fn type_scale(ui: &mut Ui, _: &mut super::State) {
    let tokens = Tokens::current(ui.ctx());
    let t = &tokens;
    row(ui, "h1", |ui| {
        ui.label(typography::h1(t, "Taxing Laughter"))
    });
    row(ui, "h2", |ui| {
        ui.label(typography::h2(t, "The People of the Kingdom"))
    });
    row(ui, "h3", |ui| ui.label(typography::h3(t, "The Joke Tax")));
    row(ui, "h4", |ui| {
        ui.label(typography::h4(t, "People stopped telling jokes"))
    });
    row(ui, "p", |ui| {
        ui.label(typography::p(
            t,
            "The king thought long and hard, and finally came up with a brilliant plan.",
        ))
    });
    row(ui, "lead", |ui| {
        ui.label(typography::lead(
            t,
            "A modal dialog that interrupts the user.",
        ))
    });
    row(ui, "large", |ui| {
        ui.label(typography::large(t, "Are you absolutely sure?"))
    });
    row(ui, "small", |ui| {
        ui.label(typography::small(t, "Email address"))
    });
    row(ui, "muted", |ui| {
        ui.label(typography::muted(t, "Enter your email address."))
    });
    row(ui, "inline_code", |ui| {
        ui.label(typography::inline_code(t, "cargo run -p mcsapi-gallery"))
    });
    row(ui, "blockquote", |ui| {
        blockquote(ui, |ui| {
            ui.label(typography::blockquote(
                t,
                "\"After all,\" he said, \"everyone enjoys a good joke.\"",
            ))
        })
    });
}
