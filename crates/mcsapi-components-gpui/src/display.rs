//! Cards, alerts, avatars, typography, and loading states.

use std::{
    sync::OnceLock,
    time::{Duration, Instant},
};

use gpui::{
    Animation, AnimationExt as _, AnyElement, App, Bounds, Div, FontWeight, IntoElement,
    ParentElement, PathBuilder, Pixels, RenderOnce, SharedString, Styled, Window, canvas, div,
    point, px,
};

use ui::{LabelCommon as _, Severity};

use crate::Tokens;

pub use mcsapi_components::AlertVariant;

/// shadcn's Card: a bordered surface with an optional header over its children.
#[derive(Default, IntoElement)]
#[must_use = "add it as a child"]
pub struct Card {
    title: Option<SharedString>,
    description: Option<SharedString>,
    children: Vec<AnyElement>,
}

impl Card {
    /// A card with no header.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the header title.
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Sets the header description, shown under the title.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }
}

impl ParentElement for Card {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Card {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = Tokens::get(cx);
        let mut card = div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .p(px(24.0))
            .bg(t.card)
            .border_1()
            .border_color(t.border)
            .rounded(t.card_radius())
            .text_color(t.foreground);
        if let Some(title) = self.title {
            card = card.child(
                div()
                    .text_size(px(16.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            );
        }
        if let Some(description) = self.description {
            card = card.child(
                div()
                    .text_size(px(14.0))
                    .text_color(t.muted_foreground)
                    .child(description),
            );
        }
        if !self.children.is_empty() {
            card = card.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .mt(px(12.0))
                    .text_size(px(14.0))
                    .children(self.children),
            );
        }
        card
    }
}

/// shadcn's Alert: a callout with a title and an optional description.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Alert {
    title: SharedString,
    description: Option<SharedString>,
    variant: AlertVariant,
}

impl Alert {
    /// A default-variant alert titled `title`.
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            description: None,
            variant: AlertVariant::Default,
        }
    }

    /// Sets the description under the title.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the look.
    pub fn variant(mut self, variant: AlertVariant) -> Self {
        self.variant = variant;
        self
    }
}

impl RenderOnce for Alert {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::tokens::ensure_installed(cx);
        let severity = if self.variant == AlertVariant::Destructive {
            Severity::Error
        } else {
            Severity::Info
        };
        let mut callout = ui::Callout::new().severity(severity).title(self.title);
        if let Some(description) = self.description {
            callout = callout.description(description);
        }
        callout
    }
}

/// shadcn's Separator: a one-pixel rule.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Separator {
    vertical: bool,
}

impl Separator {
    /// A rule across the full width.
    pub fn horizontal() -> Self {
        Self { vertical: false }
    }

    /// A rule down the full height.
    pub fn vertical() -> Self {
        Self { vertical: true }
    }
}

impl RenderOnce for Separator {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::tokens::ensure_installed(cx);
        if self.vertical {
            ui::Divider::vertical()
        } else {
            ui::Divider::horizontal()
        }
    }
}

/// shadcn's Label: the caption of a form control.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Label(SharedString);

impl Label {
    /// A label showing `text`.
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self(text.into())
    }
}

impl RenderOnce for Label {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::tokens::ensure_installed(cx);
        ui::Label::new(self.0).weight(FontWeight::MEDIUM)
    }
}

/// shadcn's Avatar, showing initials as the fallback content.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Avatar {
    initials: SharedString,
    size: f32,
}

impl Avatar {
    /// An avatar for `name`, showing up to two initials.
    pub fn new(name: &str) -> Self {
        let initials: String = name
            .split_whitespace()
            .filter_map(|word| word.chars().next())
            .take(2)
            .flat_map(char::to_uppercase)
            .collect();
        Self {
            initials: initials.into(),
            size: 40.0,
        }
    }

    /// Sets the diameter in pixels (default 40).
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
}

impl RenderOnce for Avatar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = Tokens::get(cx);
        div()
            .flex_none()
            .size(px(self.size))
            .rounded_full()
            .bg(t.muted)
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(self.size * 0.4))
            .font_weight(FontWeight::MEDIUM)
            .text_color(t.foreground)
            .child(self.initials)
    }
}

/// shadcn's Skeleton: a pulsing placeholder for content that is loading.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Skeleton {
    width: f32,
    height: f32,
    circle: bool,
}

impl Skeleton {
    /// A rounded rectangle `width` by `height`.
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            circle: false,
        }
    }

    /// A circle `diameter` across.
    pub fn circle(diameter: f32) -> Self {
        Self {
            width: diameter,
            height: diameter,
            circle: true,
        }
    }
}

impl RenderOnce for Skeleton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = Tokens::get(cx);
        let shape = div()
            .flex_none()
            .w(px(self.width))
            .h(px(self.height))
            .bg(t.muted);
        let shape = if self.circle {
            shape.rounded_full()
        } else {
            shape.rounded(t.radius)
        };
        let id = SharedString::from(format!(
            "skeleton-{}x{}-{}",
            self.width, self.height, self.circle
        ));
        shape.with_animation(
            id,
            Animation::new(Duration::from_secs(2)).repeat(),
            |shape, delta| {
                // shadcn's animate-pulse: full opacity, half at the midpoint.
                let wave = (delta * std::f32::consts::TAU).cos() * 0.5 + 0.5;
                shape.opacity(0.5 + 0.5 * wave)
            },
        )
    }
}

/// shadcn's Spinner: a turning arc.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Spinner {
    size: f32,
}

impl Spinner {
    /// A 16-pixel spinner.
    pub fn new() -> Self {
        Self { size: 16.0 }
    }

    /// Sets the diameter in pixels.
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

fn arc(bounds: Bounds<Pixels>, start: f32, sweep: f32, width: f32) -> Option<gpui::Path<Pixels>> {
    let center = bounds.center();
    let radius = f32::from(bounds.size.width.min(bounds.size.height)) / 2.0 - width / 2.0;
    let at = |angle: f32| {
        point(
            center.x + px(radius * angle.cos()),
            center.y + px(radius * angle.sin()),
        )
    };
    let mut path = PathBuilder::stroke(px(width));
    path.move_to(at(start));
    // Short line segments keep the arc smooth at any size.
    let steps = 24;
    for step in 1..=steps {
        path.line_to(at(start + sweep * step as f32 / steps as f32));
    }
    path.build().ok()
}

impl RenderOnce for Spinner {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        static EPOCH: OnceLock<Instant> = OnceLock::new();
        let color = Tokens::get(cx).muted_foreground;
        let width = (self.size / 8.0).max(1.5);
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let turns = EPOCH.get_or_init(Instant::now).elapsed().as_secs_f32() / 0.9;
                let start = turns.fract() * std::f32::consts::TAU;
                if let Some(path) = arc(bounds, start, 4.5, width) {
                    window.paint_path(path, color);
                }
                window.request_animation_frame();
            },
        )
        .flex_none()
        .size(px(self.size))
    }
}

/// shadcn's Progress: a filled track.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Progress {
    value: f32,
    width: Option<f32>,
}

impl Progress {
    /// A bar `value` full, from 0 to 1.
    pub fn new(value: f32) -> Self {
        Self {
            value: value.clamp(0.0, 1.0),
            width: None,
        }
    }

    /// Sets the width in pixels; it fills its parent otherwise.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }
}

impl RenderOnce for Progress {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::tokens::ensure_installed(cx);
        let t = Tokens::get(cx);
        // Zed's bar has no fill color of its own beyond status colors; the
        // track and fill keep shadcn's accent tint so progress reads as the
        // theme's highlight.
        let bar = ui::ProgressBar::new("progress", self.value, 1.0, cx)
            .bg_color(t.primary.opacity(0.2))
            .fg_color(t.primary);
        let frame = div().flex_none().child(bar);
        match self.width {
            Some(width) => frame.w(px(width)),
            None => frame.w_full(),
        }
    }
}

/// shadcn's Empty: what a view shows when it has nothing in it, with optional
/// actions as children.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct Empty {
    title: SharedString,
    description: Option<SharedString>,
    icon: Option<SharedString>,
    children: Vec<AnyElement>,
}

impl Empty {
    /// An empty state titled `title`.
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            description: None,
            icon: None,
            children: Vec::new(),
        }
    }

    /// Sets the line under the title.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets a glyph shown in a tile above the title.
    pub fn icon(mut self, icon: impl Into<SharedString>) -> Self {
        self.icon = Some(icon.into());
        self
    }
}

impl ParentElement for Empty {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Empty {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = Tokens::get(cx);
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8.0))
            .p(px(24.0))
            .border_1()
            .border_dashed()
            .border_color(t.border)
            .rounded(t.card_radius())
            .text_color(t.foreground)
            .children(self.icon.map(|icon| {
                div()
                    .size(px(40.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(t.radius)
                    .bg(t.muted)
                    .text_size(px(20.0))
                    .child(icon)
            }))
            .child(
                div()
                    .text_size(px(18.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child(self.title),
            )
            .children(self.description.map(|description| {
                div()
                    .text_size(px(14.0))
                    .text_color(t.muted_foreground)
                    .text_center()
                    .child(description)
            }))
            .children((!self.children.is_empty()).then(|| {
                div()
                    .flex()
                    .gap(px(8.0))
                    .mt(px(8.0))
                    .children(self.children)
            }))
    }
}

/// shadcn's AspectRatio: children sized to a fixed width-to-height ratio.
#[derive(IntoElement)]
#[must_use = "add it as a child"]
pub struct AspectRatio {
    ratio: f32,
    width: f32,
    children: Vec<AnyElement>,
}

impl AspectRatio {
    /// A box `ratio` times as wide as it is tall, 240 pixels wide.
    pub fn new(ratio: f32) -> Self {
        Self {
            ratio,
            width: 240.0,
            children: Vec::new(),
        }
    }

    /// Sets the width in pixels; the height follows from the ratio.
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }
}

impl ParentElement for AspectRatio {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for AspectRatio {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .flex_none()
            .w(px(self.width))
            .h(px(self.width / self.ratio.max(f32::EPSILON)))
            .children(self.children)
    }
}

/// shadcn's typography styles, as styled text containers.
pub mod typography {
    use gpui::{Div, FontWeight, Hsla, ParentElement, SharedString, Styled, div, px};

    use crate::Tokens;

    fn styled(text: impl Into<SharedString>, size: f32, color: Hsla) -> Div {
        div()
            .text_size(px(size))
            .text_color(color)
            .child(text.into())
    }

    /// Page title (`h1`).
    pub fn h1(tokens: &Tokens, text: impl Into<SharedString>) -> Div {
        styled(text, 36.0, tokens.foreground).font_weight(FontWeight::EXTRA_BOLD)
    }

    /// Section title (`h2`).
    pub fn h2(tokens: &Tokens, text: impl Into<SharedString>) -> Div {
        styled(text, 30.0, tokens.foreground).font_weight(FontWeight::SEMIBOLD)
    }

    /// Subsection title (`h3`).
    pub fn h3(tokens: &Tokens, text: impl Into<SharedString>) -> Div {
        styled(text, 24.0, tokens.foreground).font_weight(FontWeight::SEMIBOLD)
    }

    /// Minor heading (`h4`).
    pub fn h4(tokens: &Tokens, text: impl Into<SharedString>) -> Div {
        styled(text, 20.0, tokens.foreground).font_weight(FontWeight::SEMIBOLD)
    }

    /// Body paragraph.
    pub fn p(tokens: &Tokens, text: impl Into<SharedString>) -> Div {
        styled(text, 16.0, tokens.foreground)
    }

    /// Larger, muted introductory paragraph.
    pub fn lead(tokens: &Tokens, text: impl Into<SharedString>) -> Div {
        styled(text, 20.0, tokens.muted_foreground)
    }

    /// Slightly larger emphasized text.
    pub fn large(tokens: &Tokens, text: impl Into<SharedString>) -> Div {
        styled(text, 18.0, tokens.foreground).font_weight(FontWeight::SEMIBOLD)
    }

    /// Fine print.
    pub fn small(tokens: &Tokens, text: impl Into<SharedString>) -> Div {
        styled(text, 14.0, tokens.foreground).font_weight(FontWeight::MEDIUM)
    }

    /// De-emphasized text.
    pub fn muted(tokens: &Tokens, text: impl Into<SharedString>) -> Div {
        styled(text, 14.0, tokens.muted_foreground)
    }

    /// Inline code on a muted background.
    pub fn inline_code(tokens: &Tokens, text: impl Into<SharedString>) -> Div {
        styled(text, 14.0, tokens.foreground)
            .flex_none()
            .font_family("monospace")
            .px(px(4.0))
            .rounded(px(4.0))
            .bg(tokens.muted)
    }

    /// Italic quotation; wrap it in [`blockquote`](super::blockquote) for the rule.
    pub fn blockquote(tokens: &Tokens, text: impl Into<SharedString>) -> Div {
        styled(text, 16.0, tokens.foreground).italic()
    }
}

/// shadcn's blockquote: a left rule beside `content`.
pub fn blockquote(tokens: &Tokens, content: impl IntoElement) -> Div {
    div()
        .border_l_2()
        .border_color(tokens.border)
        .pl(px(24.0))
        .child(content)
}
