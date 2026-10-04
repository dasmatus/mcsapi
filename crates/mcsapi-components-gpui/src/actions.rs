//! Buttons, badges, toggles, and keys.

use std::rc::Rc;

use gpui::{
    App, ClickEvent, Div, ElementId, FontWeight, Hsla, IntoElement, ParentElement, RenderOnce,
    SharedString, Stateful, StatefulInteractiveElement, Styled, Window, div, prelude::*, px,
};

use crate::{Handler, Tokens};

pub use mcsapi_components::{BadgeVariant, ButtonSize, ButtonVariant};

/// A focusable, clickable box that shows the focus ring and reports clicks,
/// or a dimmed inert box when `disabled`.
pub(crate) fn pressable(
    id: ElementId,
    disabled: bool,
    on_click: Option<Handler<ClickEvent>>,
    tokens: &Tokens,
) -> Stateful<Div> {
    let base = div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center();
    if disabled {
        return base.opacity(0.5);
    }
    let ring = tokens.ring;
    let base = base
        .focusable()
        .cursor_pointer()
        .focus(move |style| style.border_color(ring));
    match on_click {
        Some(handler) => base.on_click(move |event, window, cx| handler(event, window, cx)),
        None => base,
    }
}

fn variant_colors(variant: ButtonVariant, t: &Tokens) -> (Option<Hsla>, Hsla, Option<Hsla>, Hsla) {
    // (fill, text, border, hover fill)
    match variant {
        ButtonVariant::Secondary => (Some(t.secondary), t.foreground, None, t.hover),
        ButtonVariant::Destructive => (
            Some(t.destructive),
            t.destructive_foreground,
            None,
            t.destructive.opacity(0.9),
        ),
        ButtonVariant::Outline => (None, t.foreground, Some(t.border), t.hover),
        ButtonVariant::Ghost => (None, t.foreground, None, t.hover),
        ButtonVariant::Link => (None, t.primary, None, gpui::transparent_black()),
        _ => (
            Some(t.primary),
            t.primary_foreground,
            None,
            t.primary.opacity(0.9),
        ),
    }
}

/// shadcn's Button.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Button {
    id: ElementId,
    label: SharedString,
    variant: ButtonVariant,
    size: ButtonSize,
    disabled: bool,
    on_click: Option<Handler<ClickEvent>>,
}

impl Button {
    /// A default-variant button labelled `label`, which is also its element ID.
    pub fn new(label: impl Into<SharedString>) -> Self {
        let label = label.into();
        Self {
            id: ElementId::Name(label.clone()),
            label,
            variant: ButtonVariant::Default,
            size: ButtonSize::Default,
            disabled: false,
            on_click: None,
        }
    }

    /// Sets the element ID, for buttons that share a label.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = id.into();
        self
    }

    /// Sets the look.
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Sets the size.
    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    /// Dims the button and ignores clicks when `false`.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.disabled = !enabled;
        self
    }

    /// Calls `handler` when the button is clicked or activated from the keyboard.
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Button {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = Tokens::get(cx);
        let (fill, text, border, hover) = variant_colors(self.variant, &t);
        let (height, pad_x, text_size) = match self.size {
            ButtonSize::Sm => (32.0, 12.0, 13.0),
            ButtonSize::Lg => (40.0, 32.0, 14.0),
            ButtonSize::Icon => (36.0, 0.0, 14.0),
            _ => (36.0, 16.0, 14.0),
        };
        let link = self.variant == ButtonVariant::Link;
        let mut button = pressable(self.id, self.disabled, self.on_click, &t)
            .h(px(height))
            .px(px(pad_x))
            .rounded(t.radius)
            .border_1()
            .border_color(border.unwrap_or(gpui::transparent_black()))
            .text_size(px(text_size))
            .font_weight(FontWeight::MEDIUM)
            .text_color(text)
            .child(self.label);
        if self.size == ButtonSize::Icon {
            button = button.w(px(height));
        }
        if let Some(fill) = fill {
            button = button.bg(fill);
        }
        if !self.disabled {
            button = if link {
                button.hover(|style| style.underline())
            } else {
                button.hover(move |style| style.bg(hover))
            };
        }
        button
    }
}

/// shadcn's Badge.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Badge {
    text: SharedString,
    variant: BadgeVariant,
}

impl Badge {
    /// A default-variant badge.
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            variant: BadgeVariant::Default,
        }
    }

    /// Sets the look.
    pub fn variant(mut self, variant: BadgeVariant) -> Self {
        self.variant = variant;
        self
    }
}

impl RenderOnce for Badge {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = Tokens::get(cx);
        let (fill, text, border) = match self.variant {
            BadgeVariant::Secondary => (Some(t.secondary), t.foreground, None),
            BadgeVariant::Destructive => (Some(t.destructive), t.destructive_foreground, None),
            BadgeVariant::Outline => (None, t.foreground, Some(t.border)),
            _ => (Some(t.primary), t.primary_foreground, None),
        };
        let mut badge = div()
            .flex_none()
            .px(px(8.0))
            .py(px(2.0))
            .rounded_full()
            .border_1()
            .border_color(border.unwrap_or(gpui::transparent_black()))
            .text_size(px(12.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(text)
            .child(self.text);
        if let Some(fill) = fill {
            badge = badge.bg(fill);
        }
        badge
    }
}

/// shadcn's Toggle: a button that stays pressed.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Toggle {
    id: ElementId,
    label: SharedString,
    pressed: bool,
    disabled: bool,
    on_toggle: Option<Handler<bool>>,
}

impl Toggle {
    /// A toggle labelled `label` (also its element ID), currently `pressed`.
    pub fn new(label: impl Into<SharedString>, pressed: bool) -> Self {
        let label = label.into();
        Self {
            id: ElementId::Name(label.clone()),
            label,
            pressed,
            disabled: false,
            on_toggle: None,
        }
    }

    /// Sets the element ID, for toggles that share a label.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = id.into();
        self
    }

    /// Dims the toggle and ignores clicks when `false`.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.disabled = !enabled;
        self
    }

    /// Calls `handler` with the new pressed state when clicked.
    pub fn on_toggle(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Toggle {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = Tokens::get(cx);
        let pressed = self.pressed;
        let on_click: Option<Handler<ClickEvent>> = self.on_toggle.map(|handler| {
            Rc::new(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                handler(&!pressed, window, cx)
            }) as Handler<ClickEvent>
        });
        let hover = t.hover;
        let mut toggle = pressable(self.id, self.disabled, on_click, &t)
            .h(px(36.0))
            .min_w(px(36.0))
            .px(px(10.0))
            .rounded(t.radius)
            .border_1()
            .border_color(gpui::transparent_black())
            .text_size(px(14.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(t.foreground)
            .child(self.label);
        if pressed {
            toggle = toggle.bg(t.hover);
        } else if !self.disabled {
            toggle = toggle.hover(move |style| style.bg(hover));
        }
        toggle
    }
}

/// shadcn's ToggleGroup with single selection.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct ToggleGroup {
    id: ElementId,
    items: Vec<SharedString>,
    selected: Option<usize>,
    disabled: bool,
    on_select: Option<Handler<Option<usize>>>,
}

impl ToggleGroup {
    /// A group over `items` with `selected` pressed. `id` must be unique among
    /// its siblings.
    pub fn new(
        id: impl Into<ElementId>,
        items: impl IntoIterator<Item = impl Into<SharedString>>,
        selected: Option<usize>,
    ) -> Self {
        Self {
            id: id.into(),
            items: items.into_iter().map(Into::into).collect(),
            selected,
            disabled: false,
            on_select: None,
        }
    }

    /// Dims the group and ignores clicks when `false`.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.disabled = !enabled;
        self
    }

    /// Calls `handler` with the new selection; clicking the pressed item clears it.
    pub fn on_select(
        mut self,
        handler: impl Fn(&Option<usize>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for ToggleGroup {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let selected = self.selected;
        div()
            .id(self.id)
            .flex()
            .gap(px(4.0))
            .children(self.items.into_iter().enumerate().map(|(index, item)| {
                let mut toggle = Toggle::new(item, selected == Some(index))
                    .id(index)
                    .enabled(!self.disabled);
                if let Some(handler) = self.on_select.clone() {
                    toggle = toggle.on_toggle(move |pressed, window, cx| {
                        handler(&pressed.then_some(index), window, cx)
                    });
                }
                toggle
            }))
    }
}

/// shadcn's Kbd: a key cap.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Kbd(SharedString);

impl Kbd {
    /// A key cap showing `key`.
    pub fn new(key: impl Into<SharedString>) -> Self {
        Self(key.into())
    }
}

impl RenderOnce for Kbd {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = Tokens::get(cx);
        div()
            .flex_none()
            .h(px(20.0))
            .min_w(px(20.0))
            .px(px(4.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .bg(t.muted)
            .text_size(px(12.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(t.muted_foreground)
            .child(self.0)
    }
}
