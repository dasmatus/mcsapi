//! Dialogs, tooltips, and toasts.

use std::{rc::Rc, time::Duration};

use gpui::{
    Anchor, AnchoredPositionMode, AnyElement, App, Context, Div, ElementId, FontWeight, Global,
    Hsla, IntoElement, KeyDownEvent, ParentElement, Render, RenderOnce, SharedString, Stateful,
    Styled, Window, anchored, canvas, deferred, div, point, prelude::*, px,
};

use crate::{Button, ButtonVariant, Handler, Tokens};

/// A callback that needs no argument, such as closing a dialog.
type Callback = Rc<dyn Fn(&mut Window, &mut App)>;

/// A window-sized layer that swallows the pointer and calls `on_press` when
/// it is pressed, for closing menus and dialogs.
pub(crate) fn backdrop(
    window: &Window,
    color: Option<Hsla>,
    on_press: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let viewport = window.viewport_size();
    anchored()
        .position_mode(AnchoredPositionMode::Window)
        .position(point(px(0.0), px(0.0)))
        .child(
            div()
                .id("backdrop")
                .occlude()
                .w(viewport.width)
                .h(viewport.height)
                .when_some(color, |layer, color| layer.bg(color))
                .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                    on_press(window, cx)
                }),
        )
}

/// Calls `on_escape` when Escape is pressed anywhere in the window.
fn escape_listener(on_escape: Callback) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |_, _, window, _| {
            let on_escape = on_escape.clone();
            window.on_key_event(move |event: &KeyDownEvent, phase, window, cx| {
                if phase.bubble() && event.keystroke.key == "escape" {
                    on_escape(window, cx);
                }
            });
        },
    )
    .size_0()
}

fn dialog_layer(
    window: &Window,
    tokens: &Tokens,
    width: f32,
    on_dismiss: Callback,
    content: Div,
) -> AnyElement {
    let viewport = window.viewport_size();
    let dismiss = on_dismiss.clone();
    let panel = div()
        .id("dialog")
        .occlude()
        .w(px(width))
        .p(px(24.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .bg(tokens.background)
        .border_1()
        .border_color(tokens.border)
        .rounded(tokens.card_radius())
        .shadow_lg()
        .text_color(tokens.foreground)
        .child(content)
        .child(escape_listener(on_dismiss));
    deferred(
        div()
            .child(backdrop(window, Some(tokens.overlay), move |window, cx| {
                dismiss(window, cx)
            }))
            .child(
                anchored()
                    .position_mode(AnchoredPositionMode::Window)
                    .position(point(px(0.0), px(0.0)))
                    .child(
                        div()
                            .w(viewport.width)
                            .h(viewport.height)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(panel),
                    ),
            ),
    )
    .with_priority(10)
    .into_any_element()
}

fn header(tokens: &Tokens, title: SharedString, description: Option<SharedString>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .text_size(px(18.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .children(description.map(|description| {
            div()
                .text_size(px(14.0))
                .text_color(tokens.muted_foreground)
                .child(description)
        }))
}

/// shadcn's Dialog: a modal panel over a dimmed backdrop. Escape and clicks on
/// the backdrop call [`Dialog::on_close`].
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Dialog {
    open: bool,
    title: SharedString,
    description: Option<SharedString>,
    width: f32,
    on_close: Option<Callback>,
    children: Vec<AnyElement>,
}

impl Dialog {
    /// A dialog titled `title`, drawn while `open`.
    pub fn new(open: bool, title: impl Into<SharedString>) -> Self {
        Self {
            open,
            title: title.into(),
            description: None,
            width: 420.0,
            on_close: None,
            children: Vec::new(),
        }
    }

    /// Sets the description under the title.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the content width (default 420).
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Calls `handler` when the user dismisses the dialog.
    pub fn on_close(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(handler));
        self
    }
}

impl ParentElement for Dialog {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Dialog {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        let t = Tokens::get(cx);
        let on_close = self.on_close.unwrap_or_else(|| Rc::new(|_, _| {}));
        let content = header(&t, self.title, self.description).child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .mt(px(8.0))
                .text_size(px(14.0))
                .children(self.children),
        );
        dialog_layer(window, &t, self.width, on_close, content)
    }
}

/// The button a user chose in an [`AlertDialog`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum AlertDialogAction {
    /// The confirm button.
    Confirm,
    /// The cancel button, Escape, or the backdrop.
    Cancel,
}

/// shadcn's AlertDialog: a modal question with confirm and cancel buttons.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct AlertDialog {
    open: bool,
    title: SharedString,
    description: Option<SharedString>,
    confirm: SharedString,
    cancel: SharedString,
    destructive: bool,
    on_action: Option<Handler<AlertDialogAction>>,
}

impl AlertDialog {
    /// An alert dialog titled `title`, drawn while `open`.
    pub fn new(open: bool, title: impl Into<SharedString>) -> Self {
        Self {
            open,
            title: title.into(),
            description: None,
            confirm: "Continue".into(),
            cancel: "Cancel".into(),
            destructive: false,
            on_action: None,
        }
    }

    /// Sets the description under the title.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the confirm button's text (default "Continue").
    pub fn confirm_text(mut self, text: impl Into<SharedString>) -> Self {
        self.confirm = text.into();
        self
    }

    /// Sets the cancel button's text (default "Cancel").
    pub fn cancel_text(mut self, text: impl Into<SharedString>) -> Self {
        self.cancel = text.into();
        self
    }

    /// Draws the confirm button as destructive.
    pub fn destructive(mut self, destructive: bool) -> Self {
        self.destructive = destructive;
        self
    }

    /// Calls `handler` with the user's answer.
    pub fn on_action(
        mut self,
        handler: impl Fn(&AlertDialogAction, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_action = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for AlertDialog {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        let t = Tokens::get(cx);
        let handler = self.on_action.unwrap_or_else(|| Rc::new(|_, _, _| {}));
        let answer = |action| {
            let handler = handler.clone();
            move |_: &gpui::ClickEvent, window: &mut Window, cx: &mut App| {
                handler(&action, window, cx)
            }
        };
        let content = header(&t, self.title, self.description).child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .mt(px(16.0))
                .child(
                    Button::new(self.cancel)
                        .variant(ButtonVariant::Outline)
                        .on_click(answer(AlertDialogAction::Cancel)),
                )
                .child(
                    Button::new(self.confirm)
                        .variant(if self.destructive {
                            ButtonVariant::Destructive
                        } else {
                            ButtonVariant::Default
                        })
                        .on_click(answer(AlertDialogAction::Confirm)),
                ),
        );
        let cancel = handler.clone();
        dialog_layer(
            window,
            &t,
            420.0,
            Rc::new(move |window, cx| cancel(&AlertDialogAction::Cancel, window, cx)),
            content,
        )
    }
}

struct TooltipView(SharedString);

impl Render for TooltipView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Tokens::get(cx);
        div()
            .px(px(12.0))
            .py(px(6.0))
            .rounded(t.radius)
            .bg(t.primary)
            .text_size(px(12.0))
            .text_color(t.primary_foreground)
            .child(self.0.clone())
    }
}

/// Wraps `child` so a shadcn-styled tooltip with `text` shows while it is
/// hovered. `id` must be unique among its siblings.
pub fn tooltip(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    child: impl IntoElement,
) -> Stateful<Div> {
    let text = text.into();
    div()
        .id(id)
        .flex_none()
        .child(child)
        .tooltip(move |_, cx| cx.new(|_| TooltipView(text.clone())).into())
}

/// One queued toast.
#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    /// Main line.
    pub title: SharedString,
    /// Optional second line.
    pub description: Option<SharedString>,
    /// How long the toast stays up.
    pub duration: Duration,
    id: u64,
}

#[derive(Default)]
struct ToastQueue {
    toasts: Vec<Toast>,
    next_id: u64,
}

impl Global for ToastQueue {}

fn dismiss(cx: &mut App, id: u64) {
    if let Some(queue) = cx.try_global::<ToastQueue>()
        && queue.toasts.iter().any(|toast| toast.id == id)
    {
        cx.global_mut::<ToastQueue>()
            .toasts
            .retain(|toast| toast.id != id);
        cx.refresh_windows();
    }
}

/// Queues a toast (shadcn's `toast()` from Sonner); [`Toaster`] draws it.
pub fn toast(cx: &mut App, title: impl Into<SharedString>, description: Option<SharedString>) {
    let queue = cx.default_global::<ToastQueue>();
    let id = queue.next_id;
    queue.next_id += 1;
    let duration = Duration::from_secs(4);
    queue.toasts.push(Toast {
        title: title.into(),
        description,
        duration,
        id,
    });
    cx.refresh_windows();
    let timer = cx.background_executor().timer(duration);
    cx.spawn(async move |cx| {
        timer.await;
        cx.update(|cx| dismiss(cx, id));
    })
    .detach();
}

/// The toasts currently queued, oldest first.
pub fn toasts(cx: &App) -> Vec<Toast> {
    cx.try_global::<ToastQueue>()
        .map(|queue| queue.toasts.clone())
        .unwrap_or_default()
}

/// shadcn's Toaster (Sonner): draws queued toasts in the bottom-right corner.
/// Add one to the root of the window.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Toaster;

impl RenderOnce for Toaster {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let queue = toasts(cx);
        if queue.is_empty() {
            return div().into_any_element();
        }
        let t = Tokens::get(cx);
        let viewport = window.viewport_size();
        deferred(
            anchored()
                .position_mode(AnchoredPositionMode::Window)
                .anchor(Anchor::BottomRight)
                .position(point(viewport.width - px(16.0), viewport.height - px(16.0)))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .children(queue.into_iter().map(|toast| {
                            let id = toast.id;
                            div()
                                .id(ElementId::Integer(id))
                                .occlude()
                                .w(px(320.0))
                                .p(px(16.0))
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .bg(t.background)
                                .border_1()
                                .border_color(t.border)
                                .rounded(t.card_radius())
                                .shadow_lg()
                                .cursor_pointer()
                                .text_size(px(14.0))
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(t.foreground)
                                        .child(toast.title),
                                )
                                .children(toast.description.map(|description| {
                                    div().text_color(t.muted_foreground).child(description)
                                }))
                                .on_click(move |_, _, cx| dismiss(cx, id))
                        })),
                ),
        )
        .with_priority(20)
        .into_any_element()
    }
}
