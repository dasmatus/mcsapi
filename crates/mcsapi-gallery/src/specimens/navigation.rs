use egui::Ui;
use mcsapi_components::{
    Collapsible, Pagination, Table, Tabs, Tokens, accordion_item, breadcrumb, page_window,
    typography,
};

use super::{column, row};
use crate::{Category, Specimen};

pub(super) const SPECIMENS: &[Specimen] = &[
    Specimen {
        name: "Tabs",
        category: Category::Navigation,
        source: "shadcn/ui",
        summary: "Switches between views that share one place.",
        api: &["Tabs"],
        show: tabs,
    },
    Specimen {
        name: "Breadcrumb",
        category: Category::Navigation,
        source: "shadcn/ui",
        summary: "The path to the current page, with links back up.",
        api: &["breadcrumb"],
        show: breadcrumbs,
    },
    Specimen {
        name: "Pagination",
        category: Category::Navigation,
        source: "shadcn/ui",
        summary: "Moves through numbered pages, eliding long runs.",
        api: &["Pagination", "page_window"],
        show: pagination,
    },
    Specimen {
        name: "Collapsible",
        category: Category::Navigation,
        source: "shadcn/ui",
        summary: "A header that shows or hides the content under it.",
        api: &["Collapsible"],
        show: collapsible,
    },
    Specimen {
        name: "Accordion",
        category: Category::Navigation,
        source: "shadcn/ui",
        summary: "A stack of collapsibles separated by rules.",
        api: &["accordion_item"],
        show: accordion,
    },
    Specimen {
        name: "Table",
        category: Category::Navigation,
        source: "shadcn/ui",
        summary: "Rows of text under column headers, highlighted on hover.",
        api: &["Table"],
        show: table,
    },
];

#[derive(Default)]
pub(super) struct State {
    tab: usize,
    crumb: Option<usize>,
    short: usize,
    long: usize,
}

fn tabs(ui: &mut Ui, state: &mut super::State) {
    let tokens = Tokens::current(ui.ctx());
    let state = &mut state.navigation;
    const TABS: [&str; 3] = ["Account", "Password", "Notifications"];
    row(ui, "Interactive", |ui| {
        ui.vertical(|ui| {
            ui.add(Tabs::new(&mut state.tab, &TABS));
            ui.label(typography::muted(
                &tokens,
                format!("Showing the {} tab.", TABS[state.tab]),
            ));
        });
    });
    row(ui, "Two tabs", |ui| {
        ui.add(Tabs::new(&mut 1, &["Preview", "Code"]));
    });
}

fn breadcrumbs(ui: &mut Ui, state: &mut super::State) {
    let tokens = Tokens::current(ui.ctx());
    let state = &mut state.navigation;
    const PATH: [&str; 4] = ["Home", "Components", "Navigation", "Breadcrumb"];
    row(ui, "Interactive", |ui| {
        ui.vertical(|ui| {
            if let Some(index) = breadcrumb(ui, &PATH).inner {
                state.crumb = Some(index);
            }
            let text = match state.crumb {
                Some(index) => format!("Last clicked: {}", PATH[index]),
                None => "Click a link.".to_owned(),
            };
            ui.label(typography::muted(&tokens, text));
        });
    });
    row(ui, "Single item", |ui| breadcrumb(ui, &["Home"]));
}

fn pagination(ui: &mut Ui, state: &mut super::State) {
    let tokens = Tokens::current(ui.ctx());
    let state = &mut state.navigation;
    row(ui, "5 pages", |ui| {
        ui.add(Pagination::new(&mut state.short, 5))
    });
    row(ui, "20 pages", |ui| {
        ui.vertical(|ui| {
            ui.add(Pagination::new(&mut state.long, 20));
            let window: Vec<String> = page_window(state.long, 20)
                .into_iter()
                .map(|page| page.map_or("…".to_owned(), |page| (page + 1).to_string()))
                .collect();
            ui.label(typography::muted(
                &tokens,
                format!("page_window: {}", window.join(" ")),
            ));
        });
    });
}

fn collapsible(ui: &mut Ui, _: &mut super::State) {
    column(ui, 420.0, |ui| {
        Collapsible::new("starred", "@peduarte starred 3 repositories")
            .default_open(true)
            .show(ui, |ui| {
                for repo in [
                    "@radix-ui/primitives",
                    "@radix-ui/colors",
                    "@stitches/react",
                ] {
                    ui.label(repo);
                }
            });
        Collapsible::new("closed", "Starts closed").show(ui, |ui| {
            ui.label("Hidden until opened.");
        });
    });
}

fn accordion(ui: &mut Ui, _: &mut super::State) {
    column(ui, 420.0, |ui| {
        accordion_item(ui, "accessible", "Is it accessible?", |ui| {
            ui.label("Yes. Headers are focusable and report their open state.");
        });
        accordion_item(ui, "styled", "Is it styled?", |ui| {
            ui.label("Yes. It reads its colors from the shell theme.");
        });
        accordion_item(ui, "animated", "Is it animated?", |ui| {
            ui.label("The chevron turns as it opens.");
        });
    });
}

fn table(ui: &mut Ui, _: &mut super::State) {
    let rows: Vec<Vec<String>> = [
        ["INV001", "Paid", "Credit Card", "$250.00"],
        ["INV002", "Pending", "PayPal", "$150.00"],
        ["INV003", "Unpaid", "Bank Transfer", "$350.00"],
        ["INV004", "Paid", "Credit Card", "$450.00"],
    ]
    .iter()
    .map(|row| row.iter().map(|cell| (*cell).to_owned()).collect())
    .collect();
    column(ui, 560.0, |ui| {
        Table::new(&["Invoice", "Status", "Method", "Amount"], &rows)
            .caption("A list of your recent invoices.")
            .show(ui);
    });
}
