//! Buttons, badges, toggles, and keys.

use std::rc::Rc;

use gpui::{
    App, ClickEvent, Div, ElementId, FontWeight, IntoElement, ParentElement, RenderOnce,
    SharedString, Stateful, StatefulInteractiveElement, Styled, Window, div, prelude::*, px,
};
use ui::{
    ButtonCommon as _, ButtonSize as ZedButtonSize, ButtonStyle, Clickable as _, Disableable as _,
    FixedWidth as _, LabelSize,
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

/// The Zed button style and label color for a shadcn variant that Zed draws
/// the way shadcn does. The solid ones are [`solid_button`]s.
fn zed_style(variant: ButtonVariant) -> (ButtonStyle, Option<ui::Color>) {
    match variant {
        ButtonVariant::Outline => (ButtonStyle::Outlined, None),
        ButtonVariant::Ghost => (ButtonStyle::Subtle, None),
        ButtonVariant::Link => (ButtonStyle::Transparent, Some(ui::Color::Accent)),
        _ => (ButtonStyle::Filled, None),
    }
}

/// Zed's size one step up from shadcn's name for it: Zed's sizes are for a
/// dense editor, these controls are for a desktop shell.
fn zed_size(size: ButtonSize) -> ZedButtonSize {
    match size {
        ButtonSize::Sm => ZedButtonSize::Default,
        ButtonSize::Lg => ZedButtonSize::Large,
        _ => ZedButtonSize::Medium,
    }
}

/// shadcn's Button, drawn by Zed's [`ui::Button`].
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

/// shadcn's solid fill for the primary and destructive variants, which Zed's
/// buttons only tint: the web interface's buttons and selected items are
/// solid accent with dark text, so mcsapi's are too.
fn solid_button(button: Button, t: &Tokens) -> gpui::AnyElement {
    let (fill, text) = match button.variant {
        ButtonVariant::Destructive => (t.destructive, t.destructive_foreground),
        _ => (t.primary, t.primary_foreground),
    };
    let (height, padding) = match button.size {
        ButtonSize::Sm => (32.0, 12.0),
        ButtonSize::Lg => (40.0, 32.0),
        ButtonSize::Icon => (36.0, 0.0),
        _ => (36.0, 16.0),
    };
    let mut element = pressable(button.id, button.disabled, button.on_click, t)
        .h(px(height))
        .px(px(padding))
        .rounded(t.radius)
        .border_1()
        .border_color(gpui::transparent_black())
        .bg(fill)
        .text_color(text)
        .text_size(px(14.0))
        .font_weight(FontWeight::MEDIUM)
        .child(button.label);
    if button.size == ButtonSize::Icon {
        element = element.w(px(height));
    }
    if !button.disabled {
        element = element.hover(|style| style.opacity(0.9));
    }
    element.into_any_element()
}

impl RenderOnce for Button {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::tokens::ensure_installed(cx);
        if matches!(
            self.variant,
            ButtonVariant::Default | ButtonVariant::Destructive
        ) {
            return solid_button(self, &Tokens::get(cx));
        }
        let (style, color) = zed_style(self.variant);
        let size = zed_size(self.size);
        let mut button = ui::Button::new(self.id, self.label)
            .style(style)
            .size(size)
            .label_size(LabelSize::Default)
            .disabled(self.disabled);
        if let Some(color) = color {
            button = button.color(color);
        }
        if self.size == ButtonSize::Icon {
            // A square as tall as the button, like shadcn's icon size.
            button = button.width(size.rems());
        }
        if let Some(handler) = self.on_click {
            button = button.on_click(move |event, window, cx| handler(event, window, cx));
        }
        button.into_any_element()
    }
}

/// shadcn's Badge: a small pill of text, the same shape as the egui one.
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
        let clear = gpui::transparent_black();
        let (fill, border, text) = match self.variant {
            BadgeVariant::Secondary => (t.secondary, clear, t.muted_foreground),
            BadgeVariant::Destructive => (t.destructive, clear, t.destructive_foreground),
            BadgeVariant::Outline => (clear, t.border, t.foreground),
            _ => (t.primary, clear, t.primary_foreground),
        };
        div()
            .flex_none()
            .flex()
            .items_center()
            .px(px(8.0))
            .py(px(2.0))
            .rounded_full()
            .border_1()
            .border_color(border)
            .bg(fill)
            .text_size(px(12.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(text)
            .child(self.text)
    }
}

/// shadcn's Toggle: a button that stays pressed, raised onto the muted fill
/// like the egui one.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Toggle {
    id: ElementId,
    label: SharedString,
    pressed: bool,
    disabled: bool,
    height: f32,
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
            height: 36.0,
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
        let on_click = self.on_toggle.map(|handler| {
            Rc::new(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                handler(&!pressed, window, cx)
            }) as Handler<ClickEvent>
        });
        let (muted, foreground) = (t.muted, t.foreground);
        let mut toggle = pressable(self.id, self.disabled, on_click, &t)
            .h(px(self.height))
            .min_w(px(self.height))
            .px(px(10.0))
            .rounded(t.radius)
            .border_1()
            .border_color(gpui::transparent_black())
            .text_size(px(14.0))
            .text_color(if pressed {
                t.foreground
            } else {
                t.muted_foreground
            })
            .when(pressed, |toggle| toggle.bg(t.muted))
            .child(self.label);
        if !self.disabled {
            toggle = toggle.hover(move |style| style.bg(muted).text_color(foreground));
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
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = Tokens::get(cx);
        let selected = self.selected;
        // One sunken, bordered segment holding the items, like the web
        // interface's system/light/dark switcher.
        div()
            .id(self.id)
            .flex()
            .flex_none()
            .p(px(2.0))
            .gap(px(2.0))
            .bg(t.field)
            .border_1()
            .border_color(t.border)
            .rounded(t.radius)
            .children(self.items.into_iter().enumerate().map(|(index, item)| {
                let mut toggle = Toggle::new(item, selected == Some(index))
                    .id(index)
                    .enabled(!self.disabled);
                toggle.height = 30.0;
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
