//! An [`mcsapi_ui::Error`] drawn with its causes, help and a "Learn more"
//! button into the documentation: inline as [`ErrorAlert`], or as the
//! modal [`ErrorDialog`].

use egui::{
    Color32, Context, FontId, Frame, Id, Margin, Modal, Response, RichText, Ui, WidgetInfo,
    WidgetType,
};
use mcsapi_ui::error::{Docs, Error};

use crate::{Button, ButtonSize, ButtonVariant, Tokens};

/// An [`Error`] as an alert: the message in the destructive color (the
/// accent for a warning), each cause under it, miette's help, the diagnostic
/// code, and buttons to open the documentation section the error names
/// (only when the error names one and there is documentation; see
/// [`ErrorAlert::address_only`] for a screen without a browser), copy the
/// whole report, and dismiss it.
///
/// Every color, radius and font comes from [`Tokens`], so it follows the
/// theme like the other components.
///
/// ```
/// # egui::__run_test_ui(|ui| {
/// use mcsapi_components::ErrorAlert;
/// use mcsapi_ui::error::{Context, Docs, DocLink};
///
/// let error = std::fs::read("/nonexistent")
///     .context("Could not open /nonexistent")
///     .doc(DocLink::new("layout").section("home"))
///     .unwrap_err();
/// let docs = Docs::new(None, Some("https://example.org/docs".into()));
/// let shown = ErrorAlert::new(&error).docs(&docs).dismissible(true).show(ui);
/// assert!(!shown.dismissed);
/// # });
/// ```
#[must_use = "draw it with `alert.show(ui)`"]
pub struct ErrorAlert<'a> {
    error: &'a Error,
    docs: Option<&'a Docs>,
    dismissible: bool,
    address_only: bool,
}

/// What happened to an [`ErrorAlert`] this frame.
pub struct ErrorAlertResponse {
    /// The alert's frame.
    pub response: Response,
    /// The person pressed "Dismiss"; the app should stop showing the error.
    pub dismissed: bool,
    /// The person pressed "Learn more" and this is what opening it did:
    /// `Ok(false)` when the offline copy lacks the page and there is no site
    /// to fall back to.
    pub learn_more: Option<std::io::Result<bool>>,
}

impl<'a> ErrorAlert<'a> {
    /// An alert for `error`, with "Learn more" reading [`Docs::from_env`].
    pub fn new(error: &'a Error) -> Self {
        Self {
            error,
            docs: None,
            dismissible: false,
            address_only: false,
        }
    }

    /// Where "Learn more" looks for the documentation, instead of the
    /// environment.
    pub fn docs(mut self, docs: &'a Docs) -> Self {
        self.docs = Some(docs);
        self
    }

    /// Whether to offer a "Dismiss" button.
    pub fn dismissible(mut self, dismissible: bool) -> Self {
        self.dismissible = dismissible;
        self
    }

    /// Shows the section's address on the published site as text instead
    /// of a "Learn more" button, for a screen with no browser to open it in,
    /// such as an installer: the person reads it on another device.
    pub fn address_only(mut self, address_only: bool) -> Self {
        self.address_only = address_only;
        self
    }

    /// Draws the alert.
    pub fn show(self, ui: &mut Ui) -> ErrorAlertResponse {
        let tokens = Tokens::current(ui.ctx());
        let error = self.error;
        let tone = if error.is_warning() {
            tokens.primary
        } else {
            tokens.destructive
        };
        let mut dismissed = false;
        let mut learn_more = None;
        let title = error.to_string();
        // Two environment reads, so a frame can afford them, and the button
        // only appears when there is somewhere for it to go.
        let env;
        let docs = match self.docs {
            Some(docs) => docs,
            None => {
                env = Docs::from_env();
                &env
            }
        };
        let address = self
            .address_only
            .then(|| error.doc().and_then(|link| docs.online_url(link)))
            .flatten();
        let response = Frame::new()
            .fill(tokens.card)
            .stroke(egui::Stroke::new(1.0, tone))
            .corner_radius(tokens.card_radius())
            .inner_margin(Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                body(
                    ui,
                    &tokens,
                    error,
                    (tokens.body_font(), tone),
                    address.as_deref(),
                );
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    let openable = !docs.is_empty() && !self.address_only;
                    if let Some(link) = error.doc().filter(|_| openable) {
                        let button = Button::new("Learn more")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Sm);
                        if ui.add(button).clicked() {
                            learn_more = Some(docs.open(link));
                        }
                    }
                    let copy = Button::new("Copy details")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm);
                    if ui.add(copy).clicked() {
                        ui.ctx().copy_text(error.details());
                    }
                    if self.dismissible {
                        let dismiss = Button::new("Dismiss")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Sm);
                        dismissed = ui.add(dismiss).clicked();
                    }
                });
            })
            .response;
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &title));
        ErrorAlertResponse {
            response,
            dismissed,
            learn_more,
        }
    }
}

/// What an error says, shared by [`ErrorAlert`] and [`ErrorDialog`]: the
/// message in `tone` and `title`'s font, each cause, miette's help, the code, and the
/// section's `address` when the screen cannot open it.
fn body(
    ui: &mut Ui,
    tokens: &Tokens,
    error: &Error,
    (title, tone): (FontId, Color32),
    address: Option<&str>,
) {
    ui.spacing_mut().item_spacing.y = 4.0;
    ui.label(
        RichText::new(error.to_string())
            .font(title)
            .strong()
            .color(tone),
    );
    for cause in error.causes() {
        ui.label(
            RichText::new(format!("• {cause}"))
                .font(tokens.body_font())
                .color(tokens.muted_foreground),
        );
    }
    if let Some(help) = error.help() {
        ui.label(
            RichText::new(help)
                .font(tokens.body_font())
                .color(tokens.foreground),
        );
    }
    if let Some(code) = error.code() {
        ui.label(
            RichText::new(code)
                .font(FontId::monospace(tokens.small_font().size))
                .color(tokens.muted_foreground),
        );
    }
    if let Some(address) = address {
        ui.add(
            egui::Label::new(
                RichText::new(format!("More at {address}"))
                    .font(tokens.small_font())
                    .color(tokens.muted_foreground),
            )
            .selectable(true),
        );
    }
}

/// An [`Error`] as an alert dialog: a modal over a dimmed backdrop that
/// says what [`ErrorAlert`] says, with "Learn more", "Copy details" and
/// "OK". It shows while `error` holds one, and "OK" or Escape takes it, so
/// the app's next failure opens it again; like shadcn's AlertDialog, a click
/// on the backdrop does not dismiss it.
///
/// ```
/// # egui::__run_test_ui(|ui| {
/// use mcsapi_components::ErrorDialog;
/// use mcsapi_ui::error::{Docs, Error};
///
/// let mut error = Some(Error::msg("Could not save"));
/// let docs = Docs::new(None, None);
/// let shown = ErrorDialog::new("save-error", &mut error).docs(&docs).show(ui.ctx());
/// assert!(shown.learn_more.is_none());
/// assert!(error.is_some());
/// # });
/// ```
#[must_use = "draw it with `dialog.show(ctx)`"]
pub struct ErrorDialog<'a> {
    id: Id,
    error: &'a mut Option<Error>,
    docs: Option<&'a Docs>,
    address_only: bool,
}

/// What happened to an [`ErrorDialog`] this frame.
#[derive(Debug, Default)]
pub struct ErrorDialogResponse {
    /// The person closed the dialog with "OK" or Escape, which cleared the
    /// error.
    pub closed: bool,
    /// The person pressed "Learn more" and this is what opening it did, as
    /// in [`ErrorAlertResponse::learn_more`].
    pub learn_more: Option<std::io::Result<bool>>,
}

impl<'a> ErrorDialog<'a> {
    /// A dialog for the error in `error`, with "Learn more" reading
    /// [`Docs::from_env`]. Nothing shows while `error` is `None`.
    pub fn new(id_salt: impl egui::AsId, error: &'a mut Option<Error>) -> Self {
        Self {
            id: Id::new(id_salt),
            error,
            docs: None,
            address_only: false,
        }
    }

    /// Where "Learn more" looks for the documentation, instead of the
    /// environment.
    pub fn docs(mut self, docs: &'a Docs) -> Self {
        self.docs = Some(docs);
        self
    }

    /// Shows the section's address instead of "Learn more", as
    /// [`ErrorAlert::address_only`] does.
    pub fn address_only(mut self, address_only: bool) -> Self {
        self.address_only = address_only;
        self
    }

    /// Draws the dialog while there is an error.
    pub fn show(self, ctx: &Context) -> ErrorDialogResponse {
        let Some(error) = self.error.as_ref() else {
            return ErrorDialogResponse::default();
        };
        let tokens = Tokens::current(ctx);
        let tone = if error.is_warning() {
            tokens.primary
        } else {
            tokens.destructive
        };
        let env;
        let docs = match self.docs {
            Some(docs) => docs,
            None => {
                env = Docs::from_env();
                &env
            }
        };
        let address = self
            .address_only
            .then(|| error.doc().and_then(|link| docs.online_url(link)))
            .flatten();
        let mut closed = false;
        let mut learn_more = None;
        let response = Modal::new(self.id)
            .backdrop_color(tokens.overlay)
            .frame(crate::overlay::dialog_frame(&tokens))
            .show(ctx, |ui| {
                ui.set_width(420.0);
                // The message is the dialog's title, as large as AlertDialog's.
                let title = FontId::proportional(18.0);
                body(ui, &tokens, error, (title, tone), address.as_deref());
                ui.add_space(16.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    closed = ui.add(Button::new("OK")).clicked();
                    let openable = !docs.is_empty() && !self.address_only;
                    if let Some(link) = error.doc().filter(|_| openable) {
                        let button = Button::new("Learn more").variant(ButtonVariant::Outline);
                        if ui.add(button).clicked() {
                            learn_more = Some(docs.open(link));
                        }
                    }
                    let copy = Button::new("Copy details").variant(ButtonVariant::Ghost);
                    if ui.add(copy).clicked() {
                        ui.ctx().copy_text(error.details());
                    }
                });
            });
        closed |= response.is_top_modal
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if closed {
            *self.error = None;
        }
        ErrorDialogResponse { closed, learn_more }
    }
}
