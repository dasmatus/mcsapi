//! Dialogs (drawn in the window, or [`NativeDialog`] windows of their
//! own), tooltips, and toasts.

use std::{rc::Rc, time::Duration};

use gpui::{
    Anchor, AnchoredPositionMode, AnyElement, App, Div, ElementId, FontWeight, Global, Hsla,
    IntoElement, KeyDownEvent, ParentElement, RenderOnce, SharedString, Stateful, Styled, Window,
    anchored, canvas, deferred, div, point, prelude::*, px,
};

use crate::{Button, ButtonVariant, Handler, Tokens};

/// A callback that needs no argument, such as closing a dialog.
type Callback = Rc<dyn Fn(&mut Window, &mut App)>;

/// A callback for after a window is gone, so with no window to pass.
type AppCallback = Rc<dyn Fn(&mut App)>;

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

/// A dialog in a window of its own: a native, modal child of the active
/// window (`gpui::WindowKind::Dialog`, so `xdg_dialog_v1` on Wayland and a
/// transient dialog on X11), titled `title` by the window system and
/// centered on the display. The window system keeps it over its parent and
/// the parent takes no input until it closes.
///
/// The view `build` makes is the window's root. Draw it with
/// [`NativeDialog::frame`] for the theme's background, padding, focus and a
/// window that fits its content, and close it with `window.remove_window()`.
#[must_use = "open it with `dialog.open(cx, ...)`"]
pub struct NativeDialog {
    title: SharedString,
    width: f32,
}

impl NativeDialog {
    /// Padding around the content, the same as [`Dialog`]'s.
    pub const PADDING: f32 = 24.0;

    /// A dialog window titled `title`.
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            width: 420.0,
        }
    }

    /// Sets the content width (default 420).
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Opens the window with the view `build` makes. It starts with a guess
    /// at its height, which [`NativeDialog::frame`] corrects on the first
    /// frame.
    pub fn open<V: Render + 'static>(
        self,
        cx: &mut App,
        build: impl FnOnce(&mut Window, &mut gpui::Context<V>) -> V,
    ) -> gpui::Result<gpui::WindowHandle<V>> {
        let size = gpui::size(px(self.width + 2.0 * Self::PADDING), px(200.0));
        let options = gpui::WindowOptions {
            window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds::centered(
                None, size, cx,
            ))),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some(self.title),
                ..Default::default()
            }),
            kind: gpui::WindowKind::Dialog,
            is_resizable: false,
            is_minimizable: false,
            ..Default::default()
        };
        cx.open_window(options, |window, cx| cx.new(|cx| build(window, cx)))
    }

    /// The root element for a dialog window's view: `content` on the
    /// theme's background with [`NativeDialog::PADDING`] around it, resizing
    /// the window to the content's height, and calling `on_escape` when
    /// Escape is pressed. `focus` is the view's; the frame takes it on the
    /// first frame, because a window with nothing focused gets no keys.
    pub fn frame(
        window: &mut Window,
        cx: &mut App,
        focus: &gpui::FocusHandle,
        content: impl IntoElement,
        on_escape: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Div {
        if window.focused(cx).is_none() {
            window.focus(focus, cx);
        }
        let t = Tokens::get(cx);
        let width = window.viewport_size().width;
        let measured = div().relative().flex().flex_col().child(content).child(
            canvas(
                move |bounds, window, _| {
                    let wanted = bounds.size.height + px(2.0 * Self::PADDING);
                    if (window.viewport_size().height - wanted).abs() > px(1.0) {
                        window.resize(gpui::size(width, wanted));
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        );
        div()
            .track_focus(focus)
            .size_full()
            .p(px(Self::PADDING))
            .bg(t.background)
            .text_color(t.foreground)
            .child(measured)
            .child(escape_listener(Rc::new(on_escape)))
    }
}

/// An [`mcsapi_ui::Error`] as an alert dialog in a [`NativeDialog`] window:
/// the message as the title in the destructive color (the accent for a
/// warning), each cause, miette's help and the code under it, and "Copy
/// details", "Learn more" (when the error names a section and there is
/// documentation) and "OK". The GPUI twin of `mcsapi_components::ErrorDialog`.
/// "OK" and Escape close the window and call [`ErrorDialog::on_close`]. It
/// copies what it draws, so `error` need not outlive the call.
#[must_use = "open it with `dialog.open(cx)`"]
pub struct ErrorDialog {
    title: SharedString,
    window_title: Option<SharedString>,
    lines: Vec<SharedString>,
    code: Option<SharedString>,
    warning: bool,
    details: SharedString,
    doc: Option<mcsapi_ui::DocLink>,
    docs: mcsapi_ui::Docs,
    address_only: bool,
    on_close: Option<AppCallback>,
    focus: Option<gpui::FocusHandle>,
}

impl ErrorDialog {
    /// A dialog for `error`, with "Learn more" reading
    /// [`mcsapi_ui::Docs::from_env`].
    pub fn new(error: &mcsapi_ui::Error) -> Self {
        Self {
            title: error.to_string().into(),
            window_title: None,
            lines: error
                .causes()
                .map(|cause| format!("• {cause}").into())
                .chain(error.help().map(Into::into))
                .collect(),
            code: error.code().map(Into::into),
            warning: error.is_warning(),
            details: error.details().into(),
            doc: error.doc().cloned(),
            docs: mcsapi_ui::Docs::from_env(),
            address_only: false,
            on_close: None,
            focus: None,
        }
    }

    /// Where "Learn more" looks for the documentation, instead of the
    /// environment.
    pub fn docs(mut self, docs: mcsapi_ui::Docs) -> Self {
        self.docs = docs;
        self
    }

    /// Sets the window's title, such as the app's name; "Error", or
    /// "Warning" for a warning, without one.
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.window_title = Some(title.into());
        self
    }

    /// Shows the section's address on the published site instead of "Learn
    /// more", for a screen with no browser to open it in.
    pub fn address_only(mut self, address_only: bool) -> Self {
        self.address_only = address_only;
        self
    }

    /// Calls `handler` when the person closes the dialog.
    pub fn on_close(mut self, handler: impl Fn(&mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(handler));
        self
    }

    /// Opens the dialog's window.
    pub fn open(mut self, cx: &mut App) -> gpui::Result<gpui::WindowHandle<Self>> {
        let title = self
            .window_title
            .take()
            .unwrap_or_else(|| if self.warning { "Warning" } else { "Error" }.into());
        NativeDialog::new(title).open(cx, |_, cx| Self {
            focus: Some(cx.focus_handle()),
            ..self
        })
    }

    fn close(&self, window: &mut Window, cx: &mut App) {
        if let Some(on_close) = &self.on_close {
            on_close(cx);
        }
        window.remove_window();
    }
}

impl Render for ErrorDialog {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let t = Tokens::get(cx);
        let tone = if self.warning {
            t.primary
        } else {
            t.destructive
        };
        let address = self
            .doc
            .as_ref()
            .filter(|_| self.address_only)
            .and_then(|link| self.docs.online_url(link));
        let muted = |text: SharedString| {
            div()
                .text_size(px(14.0))
                .text_color(t.muted_foreground)
                .child(text)
        };
        let mut buttons = div().flex().justify_end().gap(px(8.0)).mt(px(16.0));
        let details = self.details.clone();
        buttons = buttons.child(
            Button::new("Copy details")
                .id("error-dialog-copy")
                .variant(ButtonVariant::Ghost)
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(details.to_string()));
                }),
        );
        let openable = !self.docs.is_empty() && !self.address_only;
        if let Some(link) = self.doc.clone().filter(|_| openable) {
            let docs = self.docs.clone();
            buttons = buttons.child(
                Button::new("Learn more")
                    .id("error-dialog-learn-more")
                    .variant(ButtonVariant::Outline)
                    .on_click(move |_, _, _| {
                        if let Err(error) = docs.open(&link) {
                            tracing::warn!(%link, %error, "could not open the documentation");
                        }
                    }),
            );
        }
        buttons = buttons.child(
            Button::new("OK")
                .id("error-dialog-ok")
                .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
        );
        let content = div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(18.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(tone)
                    .child(self.title.clone()),
            )
            .children(self.lines.iter().cloned().map(muted))
            .children(
                self.code
                    .clone()
                    .map(|code| muted(code).font_family("monospace")),
            )
            .children(address.map(|address| muted(format!("More at {address}").into())))
            .child(buttons);
        let this = cx.entity().downgrade();
        let focus = self.focus.get_or_insert_with(|| cx.focus_handle()).clone();
        NativeDialog::frame(window, cx, &focus, content, move |window, cx| {
            if let Some(this) = this.upgrade() {
                this.read(cx)
                    .on_close
                    .clone()
                    .inspect(|on_close| on_close(cx));
            }
            window.remove_window();
        })
    }
}

/// Wraps `child` so Zed's [`ui::Tooltip`] with `text` shows while it is
/// hovered. `id` must be unique among its siblings.
pub fn tooltip(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    child: impl IntoElement,
) -> Stateful<Div> {
    let text = text.into();
    div().id(id).flex_none().child(child).tooltip(move |_, cx| {
        crate::tokens::ensure_installed(cx);
        ui::Tooltip::simple(text.clone(), cx)
    })
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
