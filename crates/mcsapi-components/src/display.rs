//! Static and feedback components: card, alert, separator, label, avatar,
//! skeleton, spinner, progress, empty state, aspect ratio, and typography.

use std::f32::consts::TAU;

use egui::{
    Align2, CornerRadius, FontId, Frame, InnerResponse, Margin, Pos2, Rect, Response, RichText,
    Sense, Shape, Stroke, Ui, Vec2, Widget, WidgetInfo, WidgetType, vec2,
};

use crate::Tokens;

/// shadcn's Card: a bordered surface with an optional header and footer.
///
/// ```
/// # egui::__run_test_ui(|ui| {
/// use mcsapi_components::{Button, Card};
/// Card::new()
///     .title("Create project")
///     .description("Deploy your new project in one click.")
///     .show(ui, |ui| ui.label("Body"));
/// # });
/// ```
#[derive(Default)]
#[must_use = "draw it with `card.show(ui, ...)`"]
pub struct Card {
    title: Option<String>,
    description: Option<String>,
}

impl Card {
    /// A card with no header.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the header title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Sets the header description, shown under the title.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Draws the card with `content` as its body.
    pub fn show<R>(self, ui: &mut Ui, content: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let tokens = Tokens::current(ui.ctx());
        Frame::new()
            .fill(tokens.card)
            .stroke(tokens.border_stroke())
            .corner_radius(tokens.card_radius())
            .inner_margin(Margin::same(24))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                if let Some(title) = &self.title {
                    ui.label(
                        RichText::new(title)
                            .font(FontId::proportional(16.0))
                            .strong()
                            .color(tokens.foreground),
                    );
                }
                if let Some(description) = &self.description {
                    ui.label(
                        RichText::new(description)
                            .font(tokens.body_font())
                            .color(tokens.muted_foreground),
                    );
                }
                if self.title.is_some() || self.description.is_some() {
                    ui.add_space(18.0);
                }
                content(ui)
            })
    }
}

/// Visual style of an [`Alert`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlertVariant {
    /// Neutral callout.
    #[default]
    Default,
    /// Error callout in the destructive color.
    Destructive,
}

/// shadcn's Alert: a callout with a title and description.
#[must_use = "add it with `ui.add(alert)`"]
pub struct Alert {
    title: String,
    description: Option<String>,
    variant: AlertVariant,
}

impl Alert {
    /// A default alert titled `title`.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: None,
            variant: AlertVariant::Default,
        }
    }

    /// Sets the description under the title.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the visual style.
    pub fn variant(mut self, variant: AlertVariant) -> Self {
        self.variant = variant;
        self
    }
}

impl Widget for Alert {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let (title_color, body_color) = match self.variant {
            AlertVariant::Default => (tokens.foreground, tokens.muted_foreground),
            AlertVariant::Destructive => {
                (tokens.destructive, tokens.destructive.gamma_multiply(0.9))
            }
        };
        let response = Frame::new()
            .fill(tokens.card)
            .stroke(tokens.border_stroke())
            .corner_radius(tokens.card_radius())
            .inner_margin(Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.label(
                    RichText::new(&self.title)
                        .font(tokens.body_font())
                        .strong()
                        .color(title_color),
                );
                if let Some(description) = &self.description {
                    ui.label(
                        RichText::new(description)
                            .font(tokens.body_font())
                            .color(body_color),
                    );
                }
            })
            .response;
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &self.title));
        response
    }
}

/// shadcn's Separator: a one-pixel rule.
#[derive(Clone, Copy, Debug, Default)]
#[must_use = "add it with `ui.add(separator)`"]
pub struct Separator {
    vertical: bool,
}

impl Separator {
    /// A horizontal rule spanning the available width.
    pub fn horizontal() -> Self {
        Self { vertical: false }
    }

    /// A vertical rule spanning the row height.
    pub fn vertical() -> Self {
        Self { vertical: true }
    }
}

impl Widget for Separator {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let size = if self.vertical {
            vec2(1.0, ui.available_height().min(ui.spacing().interact_size.y))
        } else {
            vec2(ui.available_width(), 1.0)
        };
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        ui.painter().rect_filled(rect, 0.0, tokens.border);
        response
    }
}

/// shadcn's Label: the medium-weight caption above a form control.
#[must_use = "add it with `ui.add(label)`"]
pub struct Label(String);

impl Label {
    /// A label showing `text`.
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }
}

impl Widget for Label {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        ui.label(
            RichText::new(self.0)
                .font(tokens.body_font())
                .strong()
                .color(tokens.foreground),
        )
    }
}

/// shadcn's Avatar, showing initials as the fallback content.
///
/// egui image loading is up to the host, so this draws the fallback circle;
/// put an `egui::Image` over it when a picture is available.
#[must_use = "add it with `ui.add(avatar)`"]
pub struct Avatar {
    initials: String,
    size: f32,
}

impl Avatar {
    /// An avatar for `name`, showing up to two initials.
    pub fn new(name: &str) -> Self {
        let initials = name
            .split_whitespace()
            .filter_map(|word| word.chars().next())
            .take(2)
            .flat_map(char::to_uppercase)
            .collect();
        Self {
            initials,
            size: 40.0,
        }
    }

    /// Sets the diameter in points (default 40).
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
}

impl Widget for Avatar {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(self.size), Sense::hover());
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &self.initials));
        let painter = ui.painter();
        painter.circle_filled(rect.center(), self.size / 2.0, tokens.muted);
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            &self.initials,
            FontId::proportional(self.size * 0.38),
            tokens.foreground,
        );
        response
    }
}

/// Opacity of a pulsing placeholder at `time` seconds, between 0.5 and 1.
pub(crate) fn pulse(time: f64) -> f32 {
    // shadcn's `animate-pulse`: a 2 s cycle dipping to half opacity.
    let phase = (time % 2.0) / 2.0;
    (0.75 + 0.25 * (phase * std::f64::consts::TAU).cos()) as f32
}

/// shadcn's Skeleton: a pulsing placeholder block.
#[must_use = "add it with `ui.add(skeleton)`"]
pub struct Skeleton {
    size: Vec2,
    circle: bool,
}

impl Skeleton {
    /// A rounded rectangle of `size`.
    pub fn new(size: impl Into<Vec2>) -> Self {
        Self {
            size: size.into(),
            circle: false,
        }
    }

    /// A circle of `diameter`, for avatar placeholders.
    pub fn circle(diameter: f32) -> Self {
        Self {
            size: Vec2::splat(diameter),
            circle: true,
        }
    }
}

impl Widget for Skeleton {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let (rect, response) = ui.allocate_exact_size(self.size, Sense::hover());
        let alpha = pulse(ui.input(|input| input.time));
        let radius = if self.circle {
            CornerRadius::same(u8::MAX)
        } else {
            tokens.control_radius()
        };
        ui.painter()
            .rect_filled(rect, radius, tokens.muted.gamma_multiply(alpha));
        ui.ctx().request_repaint();
        response
    }
}

/// shadcn's Spinner: a rotating arc.
#[must_use = "add it with `ui.add(spinner)`"]
pub struct Spinner {
    size: f32,
}

impl Spinner {
    /// A 16 pt spinner.
    pub fn new() -> Self {
        Self { size: 16.0 }
    }

    /// Sets the diameter in points.
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
}

impl Default for Spinner {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Spinner {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(self.size), Sense::hover());
        response
            .widget_info(|| WidgetInfo::labeled(WidgetType::ProgressIndicator, true, "Loading"));
        if ui.is_rect_visible(rect) {
            let start = (ui.input(|input| input.time) * 4.0) as f32;
            let radius = self.size / 2.0 - 1.5;
            let points: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let angle = start + i as f32 / 24.0 * TAU * 0.75;
                    rect.center() + radius * Vec2::angled(angle)
                })
                .collect();
            ui.painter().add(Shape::line(
                points,
                Stroke::new(2.0, tokens.muted_foreground),
            ));
            ui.ctx().request_repaint();
        }
        response
    }
}

/// shadcn's Progress: a horizontal bar filled to `value` in `0.0..=1.0`.
#[must_use = "add it with `ui.add(progress)`"]
pub struct Progress {
    value: f32,
    width: Option<f32>,
}

impl Progress {
    /// A bar filled to `value`, clamped to `0.0..=1.0`.
    pub fn new(value: f32) -> Self {
        Self {
            value: value.clamp(0.0, 1.0),
            width: None,
        }
    }

    /// Sets the width; the default is the available width.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }
}

impl Widget for Progress {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::current(ui.ctx());
        let width = self.width.unwrap_or_else(|| ui.available_width());
        let (rect, response) = ui.allocate_exact_size(vec2(width, 8.0), Sense::hover());
        response.widget_info(|| {
            let mut info = WidgetInfo::new(WidgetType::ProgressIndicator);
            info.value = Some(f64::from(self.value));
            info
        });
        let radius = CornerRadius::same(u8::MAX);
        let painter = ui.painter();
        painter.rect_filled(rect, radius, tokens.muted);
        let mut fill = rect;
        fill.set_width(rect.width() * self.value);
        if self.value > 0.0 {
            painter.rect_filled(fill, radius, tokens.primary);
        }
        response
    }
}

/// shadcn's Empty: a centered placeholder for views with no content yet.
#[must_use = "draw it with `empty.show(ui, ...)`"]
pub struct Empty {
    title: String,
    description: Option<String>,
    icon: Option<String>,
}

impl Empty {
    /// An empty state titled `title`.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: None,
            icon: None,
        }
    }

    /// Sets the description under the title.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets a glyph shown in a muted tile above the title.
    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Draws the empty state, with `actions` (usually buttons) at the bottom.
    pub fn show<R>(self, ui: &mut Ui, actions: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let tokens = Tokens::current(ui.ctx());
        Frame::new()
            .stroke(Stroke::new(1.0, tokens.border))
            .corner_radius(tokens.card_radius())
            .inner_margin(Margin::same(32))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.vertical_centered(|ui| {
                    if let Some(icon) = &self.icon {
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::hover());
                        ui.painter()
                            .rect_filled(rect, tokens.card_radius(), tokens.muted);
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            icon,
                            FontId::proportional(20.0),
                            tokens.foreground,
                        );
                        ui.add_space(8.0);
                    }
                    ui.label(
                        RichText::new(&self.title)
                            .font(FontId::proportional(18.0))
                            .strong()
                            .color(tokens.foreground),
                    );
                    if let Some(description) = &self.description {
                        ui.label(
                            RichText::new(description)
                                .font(tokens.body_font())
                                .color(tokens.muted_foreground),
                        );
                    }
                    ui.add_space(12.0);
                    actions(ui)
                })
                .inner
            })
    }
}

/// shadcn's AspectRatio: lays out content in a box of a fixed width-to-height ratio.
#[must_use = "draw it with `aspect.show(ui, ...)`"]
pub struct AspectRatio(f32);

impl AspectRatio {
    /// A box whose width divided by height is `ratio`, for example `16.0 / 9.0`.
    pub fn new(ratio: f32) -> Self {
        Self(ratio.max(f32::EPSILON))
    }

    /// Draws `content` in a box as wide as the available width.
    pub fn show<R>(self, ui: &mut Ui, content: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(vec2(width, width / self.0), Sense::hover());
        let inner = content(&mut ui.new_child(egui::UiBuilder::new().max_rect(rect)));
        InnerResponse::new(inner, response)
    }
}

/// shadcn's typography styles, as [`RichText`] constructors.
pub mod typography {
    use egui::{FontId, RichText};

    use crate::Tokens;

    fn styled(text: impl Into<String>, size: f32, color: egui::Color32) -> RichText {
        RichText::new(text)
            .font(FontId::proportional(size))
            .color(color)
    }

    /// Page title (`h1`).
    pub fn h1(tokens: &Tokens, text: impl Into<String>) -> RichText {
        styled(text, 36.0, tokens.foreground).strong()
    }

    /// Section title (`h2`).
    pub fn h2(tokens: &Tokens, text: impl Into<String>) -> RichText {
        styled(text, 30.0, tokens.foreground).strong()
    }

    /// Subsection title (`h3`).
    pub fn h3(tokens: &Tokens, text: impl Into<String>) -> RichText {
        styled(text, 24.0, tokens.foreground).strong()
    }

    /// Minor heading (`h4`).
    pub fn h4(tokens: &Tokens, text: impl Into<String>) -> RichText {
        styled(text, 20.0, tokens.foreground).strong()
    }

    /// Body paragraph.
    pub fn p(tokens: &Tokens, text: impl Into<String>) -> RichText {
        styled(text, 16.0, tokens.foreground)
    }

    /// Larger, muted introductory paragraph.
    pub fn lead(tokens: &Tokens, text: impl Into<String>) -> RichText {
        styled(text, 20.0, tokens.muted_foreground)
    }

    /// Slightly larger emphasized text.
    pub fn large(tokens: &Tokens, text: impl Into<String>) -> RichText {
        styled(text, 18.0, tokens.foreground).strong()
    }

    /// Fine print.
    pub fn small(tokens: &Tokens, text: impl Into<String>) -> RichText {
        styled(text, 14.0, tokens.foreground)
    }

    /// De-emphasized text.
    pub fn muted(tokens: &Tokens, text: impl Into<String>) -> RichText {
        styled(text, 14.0, tokens.muted_foreground)
    }

    /// Inline code on a muted background.
    pub fn inline_code(tokens: &Tokens, text: impl Into<String>) -> RichText {
        RichText::new(text)
            .font(FontId::monospace(14.0))
            .color(tokens.foreground)
            .background_color(tokens.muted)
    }

    /// Italic quotation; draw a left rule beside it for the full blockquote look.
    pub fn blockquote(tokens: &Tokens, text: impl Into<String>) -> RichText {
        styled(text, 16.0, tokens.foreground).italics()
    }
}

/// Draws the colored rule shadcn puts left of a blockquote, then `content`.
pub fn blockquote<R>(ui: &mut Ui, content: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    let tokens = Tokens::current(ui.ctx());
    let response = ui.horizontal(|ui| {
        let (rule, _) = ui.allocate_exact_size(vec2(2.0, 0.0), Sense::hover());
        ui.add_space(22.0);
        let inner = ui.vertical(content).inner;
        (rule, inner)
    });
    let (rule, inner) = response.inner;
    let rect = Rect::from_x_y_ranges(rule.x_range(), response.response.rect.y_range());
    ui.painter().rect_filled(rect, 0.0, tokens.border);
    InnerResponse::new(inner, response.response)
}
