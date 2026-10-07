//! Tabs, breadcrumbs, pagination, disclosure, and tables.

use std::rc::Rc;

use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, FontWeight, IntoElement, ParentElement,
    RenderOnce, SharedString, Stateful, Styled, Window, div, prelude::*, px,
};

use crate::{Button, ButtonSize, ButtonVariant, Handler, Tokens, actions::pressable};

pub use mcsapi_components::page_window;

/// shadcn's Tabs: the tab list; draw the selected panel under it yourself.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Tabs {
    id: ElementId,
    tabs: Vec<SharedString>,
    selected: usize,
    on_change: Option<Handler<usize>>,
}

impl Tabs {
    /// Tabs titled `tabs` with `selected` active. `id` must be unique among its siblings.
    pub fn new(
        id: impl Into<ElementId>,
        tabs: impl IntoIterator<Item = impl Into<SharedString>>,
        selected: usize,
    ) -> Self {
        Self {
            id: id.into(),
            tabs: tabs.into_iter().map(Into::into).collect(),
            selected,
            on_change: None,
        }
    }

    /// Calls `handler` with the clicked tab's index.
    pub fn on_change(mut self, handler: impl Fn(&usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Tabs {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = Tokens::get(cx);
        div()
            .id(self.id)
            .flex()
            .flex_none()
            // A sunken, bordered track with the active tab raised out of it,
            // as the web interface draws its segmented switchers.
            .p(px(2.0))
            .gap(px(2.0))
            .rounded(t.radius)
            .bg(t.field)
            .border_1()
            .border_color(t.border)
            .children(self.tabs.into_iter().enumerate().map(|(index, tab)| {
                let active = index == self.selected;
                let on_click = self.on_change.clone().map(|handler| {
                    Rc::new(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                        handler(&index, window, cx)
                    }) as Handler<ClickEvent>
                });
                pressable(index.into(), false, on_click, &t)
                    .h(px(30.0))
                    .px(px(12.0))
                    .rounded(t.radius)
                    .border_1()
                    .border_color(gpui::transparent_black())
                    .text_size(px(14.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if active {
                        t.foreground
                    } else {
                        t.muted_foreground
                    })
                    .when(active, |tab| tab.bg(t.muted))
                    .child(tab)
            }))
    }
}

/// shadcn's Breadcrumb. Every item but the last is a link that reports its
/// index to `on_select`.
pub fn breadcrumb(
    id: impl Into<ElementId>,
    items: impl IntoIterator<Item = impl Into<SharedString>>,
    tokens: &Tokens,
    on_select: impl Fn(&usize, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let items: Vec<SharedString> = items.into_iter().map(Into::into).collect();
    let last = items.len().saturating_sub(1);
    let on_select = Rc::new(on_select);
    let t = *tokens;
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(8.0))
        .text_size(px(14.0))
        .children(items.into_iter().enumerate().map(move |(index, item)| {
            if index == last {
                return div()
                    .text_color(t.foreground)
                    .child(item)
                    .into_any_element();
            }
            let on_select = on_select.clone();
            let color = t.foreground;
            div()
                .flex()
                .gap(px(8.0))
                .child(
                    div()
                        .id(index)
                        .cursor_pointer()
                        .text_color(t.muted_foreground)
                        .hover(move |style| style.text_color(color))
                        .child(item)
                        .on_click(move |_, window, cx| on_select(&index, window, cx)),
                )
                .child(div().text_color(t.muted_foreground).child("›"))
                .into_any_element()
        }))
}

/// shadcn's Pagination: previous, page numbers, and next. Pages are zero-based;
/// the labels are one-based.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Pagination {
    id: ElementId,
    page: usize,
    total: usize,
    on_change: Option<Handler<usize>>,
}

impl Pagination {
    /// Pagination over `total` pages on `page`. `id` must be unique among its siblings.
    pub fn new(id: impl Into<ElementId>, page: usize, total: usize) -> Self {
        Self {
            id: id.into(),
            page,
            total,
            on_change: None,
        }
    }

    /// Calls `handler` with the page to go to.
    pub fn on_change(mut self, handler: impl Fn(&usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Pagination {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let page = self.page;
        let total = self.total;
        let go = |target: usize| {
            let handler = self.on_change.clone();
            move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                if let Some(handler) = &handler {
                    handler(&target, window, cx);
                }
            }
        };
        let numbers = page_window(page, total)
            .into_iter()
            .enumerate()
            .map(|(slot, entry)| match entry {
                Some(number) => Button::new((number + 1).to_string())
                    .variant(if number == page {
                        ButtonVariant::Outline
                    } else {
                        ButtonVariant::Ghost
                    })
                    .size(ButtonSize::Icon)
                    .on_click(go(number))
                    .into_any_element(),
                None => div()
                    .id(("gap", slot))
                    .w(px(36.0))
                    .flex()
                    .justify_center()
                    .child("…")
                    .into_any_element(),
            });
        div()
            .id(self.id)
            .flex()
            .flex_none()
            .items_center()
            .gap(px(4.0))
            .child(
                Button::new("‹ Previous")
                    .variant(ButtonVariant::Ghost)
                    .enabled(page > 0)
                    .on_click(go(page.saturating_sub(1))),
            )
            .children(numbers)
            .child(
                Button::new("Next ›")
                    .variant(ButtonVariant::Ghost)
                    .enabled(page + 1 < total)
                    .on_click(go((page + 1).min(total.saturating_sub(1)))),
            )
    }
}

/// shadcn's Collapsible: a header that shows or hides its children. It
/// remembers whether it is open under its ID.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Collapsible {
    id: ElementId,
    title: SharedString,
    default_open: bool,
    divider: bool,
    children: Vec<AnyElement>,
}

impl Collapsible {
    /// A closed collapsible titled `title`. `id` must be unique among its siblings.
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            default_open: false,
            divider: false,
            children: Vec::new(),
        }
    }

    /// Starts open the first time it is shown.
    pub fn default_open(mut self, open: bool) -> Self {
        self.default_open = open;
        self
    }
}

impl ParentElement for Collapsible {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Collapsible {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = Tokens::get(cx);
        let default_open = self.default_open;
        let state = window.use_keyed_state(self.id.clone(), cx, move |_, _| default_open);
        let open = *state.read(cx);
        let header = pressable(
            "header".into(),
            false,
            Some(Rc::new(
                move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                    state.update(cx, |open, cx| {
                        *open = !*open;
                        cx.notify();
                    });
                },
            )),
            &t,
        )
        .w_full()
        .h(px(44.0))
        .justify_between()
        .border_1()
        .border_color(gpui::transparent_black())
        .rounded(px(4.0))
        .text_size(px(14.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.foreground)
        .hover(|style| style.underline())
        .child(self.title)
        .child(
            div()
                .text_color(t.muted_foreground)
                .child(if open { "⌃" } else { "⌄" }),
        );
        div()
            .id(self.id)
            .flex()
            .flex_col()
            .w_full()
            .when(self.divider, |item| {
                item.border_b_1().border_color(t.border)
            })
            .child(header)
            .when(open, |item| {
                item.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .pb(px(12.0))
                        .text_size(px(14.0))
                        .text_color(t.foreground)
                        .children(self.children),
                )
            })
    }
}

/// shadcn's AccordionItem: a [`Collapsible`] with a rule under it. Stack
/// several to build an accordion.
pub fn accordion_item(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Collapsible {
    Collapsible {
        divider: true,
        ..Collapsible::new(id, title)
    }
}

/// shadcn's Table: a header row over rows highlighted on hover.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Table {
    header: Vec<SharedString>,
    rows: Vec<Vec<SharedString>>,
    caption: Option<SharedString>,
}

impl Table {
    /// A table with column titles `header` and text cells `rows`.
    pub fn new(
        header: impl IntoIterator<Item = impl Into<SharedString>>,
        rows: impl IntoIterator<Item = impl IntoIterator<Item = impl Into<SharedString>>>,
    ) -> Self {
        Self {
            header: header.into_iter().map(Into::into).collect(),
            rows: rows
                .into_iter()
                .map(|row| row.into_iter().map(Into::into).collect())
                .collect(),
            caption: None,
        }
    }

    /// Sets a muted caption under the table.
    pub fn caption(mut self, caption: impl Into<SharedString>) -> Self {
        self.caption = Some(caption.into());
        self
    }
}

impl RenderOnce for Table {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = Tokens::get(cx);
        let row = |cells: Vec<SharedString>, color| {
            div()
                .flex()
                .h(px(40.0))
                .items_center()
                .border_b_1()
                .border_color(t.border)
                .text_color(color)
                .children(
                    cells
                        .into_iter()
                        .map(|cell| div().flex_1().px(px(8.0)).truncate().child(cell)),
                )
        };
        let hover = t.muted.opacity(0.5);
        div()
            .flex()
            .flex_col()
            .w_full()
            .text_size(px(14.0))
            .child(row(self.header, t.muted_foreground).font_weight(FontWeight::MEDIUM))
            .children(self.rows.into_iter().enumerate().map(|(index, cells)| {
                row(cells, t.foreground)
                    .id(index)
                    .hover(move |style| style.bg(hover))
            }))
            .children(self.caption.map(|caption| {
                div()
                    .pt(px(12.0))
                    .flex()
                    .justify_center()
                    .text_color(t.muted_foreground)
                    .child(caption)
            }))
    }
}
