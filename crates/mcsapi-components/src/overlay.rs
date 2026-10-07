//! Overlays: dialog, alert dialog, native dialog window, tooltip, and
//! toasts (shadcn's Sonner).

use egui::{Align2, Context, Frame, Id, Margin, Modal, Order, Response, RichText, Ui, vec2};

use crate::{Button, ButtonVariant, Tokens};

pub(crate) fn dialog_frame(tokens: &Tokens) -> Frame {
    Frame::new()
        .fill(tokens.background)
        .stroke(tokens.border_stroke())
        .corner_radius(tokens.card_radius())
        .inner_margin(Margin::same(24))
}

fn header(ui: &mut Ui, tokens: &Tokens, title: &str, description: Option<&str>) {
    ui.spacing_mut().item_spacing.y = 8.0;
    ui.label(
        RichText::new(title)
            .size(18.0)
            .strong()
            .color(tokens.foreground),
    );
    if let Some(description) = description {
        ui.label(
            RichText::new(description)
                .font(tokens.body_font())
                .color(tokens.muted_foreground),
        );
    }
}

/// shadcn's Dialog: a modal window over a dimmed backdrop.
///
/// Escape and clicks on the backdrop set `open` to `false`.
#[must_use = "draw it with `dialog.show(ctx, ...)`"]
pub struct Dialog<'a> {
    id: Id,
    open: &'a mut bool,
    title: String,
    description: Option<String>,
    width: f32,
}

impl<'a> Dialog<'a> {
    /// A dialog titled `title`, shown while `open` is true.
    pub fn new(id_salt: impl egui::AsId, open: &'a mut bool, title: impl Into<String>) -> Self {
        Self {
            id: Id::new(id_salt),
            open,
            title: title.into(),
            description: None,
            width: 420.0,
        }
    }

    /// Sets the description under the title.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the content width (default 420).
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Draws the dialog with `content` under its header. Returns `None` while closed.
    pub fn show<R>(self, ctx: &Context, content: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        if !*self.open {
            return None;
        }
        let tokens = Tokens::current(ctx);
        let response = Modal::new(self.id)
            .backdrop_color(tokens.overlay)
            .frame(dialog_frame(&tokens))
            .show(ctx, |ui| {
                ui.set_width(self.width);
                header(ui, &tokens, &self.title, self.description.as_deref());
                ui.add_space(8.0);
                content(ui)
            });
        if response.should_close() {
            *self.open = false;
        }
        Some(response.inner)
    }
}

/// The button a user chose in an [`AlertDialog`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertDialogAction {
    /// The confirming action.
    Confirm,
    /// Cancel, Escape, or the backdrop.
    Cancel,
}

/// shadcn's AlertDialog: a modal that demands a confirm-or-cancel answer.
#[must_use = "draw it with `alert.show(ctx)`"]
pub struct AlertDialog<'a> {
    id: Id,
    open: &'a mut bool,
    title: String,
    description: Option<String>,
    confirm: String,
    cancel: String,
    destructive: bool,
}

impl<'a> AlertDialog<'a> {
    /// An alert dialog titled `title`, shown while `open` is true.
    pub fn new(id_salt: impl egui::AsId, open: &'a mut bool, title: impl Into<String>) -> Self {
        Self {
            id: Id::new(id_salt),
            open,
            title: title.into(),
            description: None,
            confirm: "Continue".to_owned(),
            cancel: "Cancel".to_owned(),
            destructive: false,
        }
    }

    /// Sets the description under the title.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the confirm button's text (default "Continue").
    pub fn confirm_text(mut self, text: impl Into<String>) -> Self {
        self.confirm = text.into();
        self
    }

    /// Sets the cancel button's text (default "Cancel").
    pub fn cancel_text(mut self, text: impl Into<String>) -> Self {
        self.cancel = text.into();
        self
    }

    /// Draws the confirm button in the destructive color.
    pub fn destructive(mut self, destructive: bool) -> Self {
        self.destructive = destructive;
        self
    }

    /// Draws the dialog. Returns the user's answer on the frame they give one,
    /// and closes the dialog.
    pub fn show(self, ctx: &Context) -> Option<AlertDialogAction> {
        if !*self.open {
            return None;
        }
        let tokens = Tokens::current(ctx);
        let response = Modal::new(self.id)
            .backdrop_color(tokens.overlay)
            .frame(dialog_frame(&tokens))
            .show(ctx, |ui| {
                ui.set_width(420.0);
                header(ui, &tokens, &self.title, self.description.as_deref());
                ui.add_space(16.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let variant = if self.destructive {
                        ButtonVariant::Destructive
                    } else {
                        ButtonVariant::Default
                    };
                    if ui
                        .add(Button::new(&self.confirm).variant(variant))
                        .clicked()
                    {
                        return Some(AlertDialogAction::Confirm);
                    }
                    if ui
                        .add(Button::new(&self.cancel).variant(ButtonVariant::Outline))
                        .clicked()
                    {
                        return Some(AlertDialogAction::Cancel);
                    }
                    None
                })
                .inner
            });
        // Unlike Dialog, a backdrop click does not dismiss an alert dialog.
        let escaped = response.is_top_modal
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let action = response
            .inner
            .or(escaped.then_some(AlertDialogAction::Cancel));
        if action.is_some() {
            *self.open = false;
        }
        action
    }
}

/// A dialog in a window of its own: a native child window over the app's,
/// titled `title` by the window system, with `content` drawn on the theme's
/// background. While it is open the app's window is dimmed and takes no
/// input, as under [`Dialog`], so the person answers the dialog first.
/// Escape and the window's close button set `open` to `false`.
///
/// It is native where the egui backend can open more windows (eframe on
/// Wayland, X11, macOS and Windows), and there the window is a dialog to the
/// window system: X11 gets `_NET_WM_WINDOW_TYPE_DIALOG`. Where it cannot,
/// such as the mcsapi compositor drawing egui itself, the same content is a
/// modal in the app's window, framed like [`Dialog`].
///
/// The window fits its content's height; `width` sets its width.
#[must_use = "draw it with `dialog.show(ctx, ...)`"]
pub struct NativeDialog<'a> {
    id: Id,
    open: &'a mut bool,
    title: String,
    width: f32,
}

impl<'a> NativeDialog<'a> {
    /// A dialog window titled `title`, shown while `open` is true.
    pub fn new(id_salt: impl egui::AsId, open: &'a mut bool, title: impl Into<String>) -> Self {
        Self {
            id: Id::new(id_salt),
            open,
            title: title.into(),
            width: 420.0,
        }
    }

    /// Sets the content width (default 420).
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Whether this backend opens the dialog as a window of its own rather
    /// than a modal in the app's window.
    pub fn is_native(ctx: &Context) -> bool {
        !ctx.embed_viewports()
    }

    /// Draws the dialog with `content`. Returns `None` while closed.
    pub fn show<R>(self, ctx: &Context, mut content: impl FnMut(&mut Ui) -> R) -> Option<R> {
        if !*self.open {
            return None;
        }
        let tokens = Tokens::current(ctx);
        if !Self::is_native(ctx) {
            let response = Modal::new(self.id)
                .backdrop_color(tokens.overlay)
                .frame(dialog_frame(&tokens))
                .show(ctx, |ui| {
                    ui.set_width(self.width);
                    content(ui)
                });
            let escaped = response.is_top_modal
                && ctx
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            if escaped {
                *self.open = false;
            }
            return Some(response.inner);
        }

        let viewport = egui::ViewportId::from_hash_of(self.id);
        // The app's window, dimmed and deaf until the dialog closes; a press
        // on it raises the dialog instead, as a modal child window does.
        let screen = ctx.content_rect();
        let backdrop = egui::Area::new(self.id.with("backdrop"))
            .order(Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (rect, response) = ui.allocate_exact_size(screen.size(), egui::Sense::click());
                ui.painter().rect_filled(rect, 0.0, tokens.overlay);
                response
            });
        if backdrop.inner.clicked() {
            ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::Focus);
        }
        // The height the content took last frame, so the window fits it.
        let height_id = self.id.with("height");
        let height = ctx.data(|data| data.get_temp::<f32>(height_id));
        let margin = 24.0;
        let builder = egui::ViewportBuilder::default()
            .with_title(&self.title)
            .with_inner_size([self.width + 2.0 * margin, height.unwrap_or(160.0)])
            .with_resizable(false)
            .with_minimize_button(false)
            .with_maximize_button(false)
            .with_window_type(egui::X11WindowType::Dialog)
            .with_active(true);
        let (inner, closed) = ctx.show_viewport_immediate(viewport, builder, |ui, _| {
            let frame = Frame::new()
                .fill(tokens.background)
                .inner_margin(Margin::same(margin as i8));
            let inner = egui::CentralPanel::default()
                .frame(frame)
                .show(ui, |ui| {
                    ui.set_width(self.width);
                    let inner = content(ui);
                    let wanted = ui.min_rect().height() + 2.0 * margin;
                    if height.is_none_or(|h| (h - wanted).abs() > 0.5) {
                        ui.ctx()
                            .data_mut(|data| data.insert_temp(height_id, wanted));
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(
                                self.width + 2.0 * margin,
                                wanted,
                            )));
                    }
                    inner
                })
                .inner;
            // egui cannot make the window a child of the app's, so a window
            // manager may stack it under a maximized app window: raise it
            // on its first frame, which is the only one without a height.
            if height.is_none() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            let closed = ui.input_mut(|input| {
                input.viewport().close_requested()
                    || input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
            });
            (inner, closed)
        });
        if closed {
            *self.open = false;
            // The next opening measures and raises its window afresh.
            ctx.data_mut(|data| data.remove::<f32>(height_id));
        }
        Some(inner)
    }
}

/// Shows a shadcn-styled tooltip with `text` while `response` is hovered.
pub fn tooltip(response: Response, text: impl Into<String>) -> Response {
    let text = text.into();
    let tokens = Tokens::current(&response.ctx);
    response.on_hover_ui(|ui| {
        let style = ui.style_mut();
        style.visuals.window_fill = tokens.primary;
        style.visuals.popup_shadow = egui::Shadow::NONE;
        ui.label(
            RichText::new(text)
                .font(tokens.small_font())
                .color(tokens.primary_foreground),
        );
    })
}

/// One queued toast.
#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    /// Main line.
    pub title: String,
    /// Optional second line.
    pub description: Option<String>,
    /// Seconds the toast stays up.
    pub duration: f64,
    created: f64,
}

#[derive(Clone, Default)]
struct ToastQueue(Vec<Toast>);

fn queue_id() -> Id {
    Id::new("mcsapi_components::toasts")
}

/// Queues a toast (shadcn's `toast()` from Sonner). [`Toaster::show`] draws it.
pub fn toast(ctx: &Context, title: impl Into<String>, description: Option<String>) {
    let created = ctx.input(|input| input.time);
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<ToastQueue>(queue_id())
            .0
            .push(Toast {
                title: title.into(),
                description,
                duration: 4.0,
                created,
            });
    });
    ctx.request_repaint();
}

/// The toasts currently queued in `ctx`, oldest first.
pub fn toasts(ctx: &Context) -> Vec<Toast> {
    ctx.data(|data| data.get_temp::<ToastQueue>(queue_id()))
        .unwrap_or_default()
        .0
}

/// shadcn's Toaster (Sonner): draws queued toasts in the bottom-right corner.
pub struct Toaster;

impl Toaster {
    /// Draws the queued toasts and drops the expired ones. Call once per frame.
    pub fn show(ctx: &Context) {
        let now = ctx.input(|input| input.time);
        let mut queue = ctx.data_mut(|data| {
            let queue = data.get_temp_mut_or_default::<ToastQueue>(queue_id());
            queue.0.retain(|toast| now - toast.created < toast.duration);
            queue.0.clone()
        });
        if queue.is_empty() {
            return;
        }
        let tokens = Tokens::current(ctx);
        let mut dismissed = None;
        egui::Area::new(Id::new("mcsapi_components::toaster"))
            .order(Order::Tooltip)
            .anchor(Align2::RIGHT_BOTTOM, vec2(-16.0, -16.0))
            .show(ctx, |ui| {
                for (index, toast) in queue.iter().enumerate().rev() {
                    let response = Frame::new()
                        .fill(tokens.background)
                        .stroke(tokens.border_stroke())
                        .corner_radius(tokens.card_radius())
                        .inner_margin(Margin::same(16))
                        .show(ui, |ui| {
                            ui.set_width(320.0);
                            ui.label(
                                RichText::new(&toast.title)
                                    .font(tokens.body_font())
                                    .strong()
                                    .color(tokens.foreground),
                            );
                            if let Some(description) = &toast.description {
                                ui.label(
                                    RichText::new(description)
                                        .font(tokens.body_font())
                                        .color(tokens.muted_foreground),
                                );
                            }
                        })
                        .response
                        .interact(egui::Sense::click());
                    if response.clicked() {
                        dismissed = Some(index);
                    }
                    ui.add_space(8.0);
                }
            });
        if let Some(index) = dismissed {
            queue.remove(index);
            ctx.data_mut(|data| data.insert_temp(queue_id(), ToastQueue(queue)));
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
}
